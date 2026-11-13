//! Data files: the game's maps, tuning numbers and writing live in
//! `crates/game/assets/`, not in code, so changing one never recompiles.
//!
//! A [`Data<T>`] is a static handle on one file, parsed into `T`:
//!
//! ```ignore
//! pub static TUNING: Data<Tuning> = Data::ron("data/landlord.ron");
//! let rent = TUNING.get().old_rent;
//! ```
//!
//! - **Desktop and tests** read the file from disk the first time it's asked
//!   for (so pure functions and headless tests just work), and
//!   [`DataPlugin`] re-reads it whenever it changes on disk: edit a number,
//!   save, and the running game uses it.
//! - **The browser** fetches every file the game lists in [`DataPlugin`]
//!   through the asset server; [`all_loaded`] tells the loading curtain when
//!   they're in. Until then `get` returns `T::default()`.
//!
//! Parse errors keep the last good value (and warn), so a typo in a file
//! never takes the game down.
//!
//! A [`Many<T>`] is a folder of files of one kind (the soundtrack's songs):
//! an index file lists them, and each is parsed into a `T`:
//!
//! ```ignore
//! pub static SONGS: Many<Song> = Many::ron("data/music/songs.ron");
//! // songs.ron: ["village.ron", "forest.ron"], read from data/music/songs/
//! for song in SONGS.get().iter() { ... }
//! ```

use std::sync::{Arc, RwLock};

/// How a file's bytes become its value.
type Parse<T> = fn(&[u8]) -> Result<T, String>;

/// One data file, parsed into `T` (see the crate docs).
pub struct Data<T> {
    path: &'static str,
    parse: Parse<T>,
    value: RwLock<Option<Arc<T>>>,
    /// The file's modification time when it was last read (desktop).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    stamp: RwLock<Option<std::time::SystemTime>>,
    /// The modification time of a version that didn't parse: it's not read
    /// again until the file changes (desktop).
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    failed: RwLock<Option<std::time::SystemTime>>,
}

impl<T: serde::de::DeserializeOwned> Data<T> {
    /// A RON file (tuning, writing: hand-edited, comments allowed).
    pub const fn ron(path: &'static str) -> Self {
        Self::new(path, |b| {
            let text = std::str::from_utf8(b).map_err(|e| e.to_string())?;
            ron::from_str(text).map_err(|e| e.to_string())
        })
    }

    /// A JSON file (what the modeler writes: maps).
    pub const fn json(path: &'static str) -> Self {
        Self::new(path, |b| {
            serde_json::from_slice(b).map_err(|e| e.to_string())
        })
    }
}

impl<T> Data<T> {
    /// A file with its own parser (binary data such as baked heights).
    pub const fn new(path: &'static str, parse: Parse<T>) -> Self {
        Self {
            path,
            parse,
            value: RwLock::new(None),
            stamp: RwLock::new(None),
            failed: RwLock::new(None),
        }
    }

    /// The file's path under the assets folder.
    pub fn path(&self) -> &'static str {
        self.path
    }

    /// Has the file been read (or fetched) yet?
    pub fn loaded(&self) -> bool {
        self.value.read().is_ok_and(|v| v.is_some())
    }

    /// Parses `bytes` and keeps them as the file's value; on a parse error
    /// the last good value stays. Returns whether it took.
    pub fn set_bytes(&self, bytes: &[u8]) -> bool {
        match (self.parse)(bytes) {
            Ok(v) => {
                if let Ok(mut slot) = self.value.write() {
                    *slot = Some(Arc::new(v));
                }
                true
            }
            Err(e) => {
                warn(&format!("data file {}: {e}", self.path));
                false
            }
        }
    }

    /// Sets the value directly (tests).
    pub fn set(&self, value: T) {
        if let Ok(mut slot) = self.value.write() {
            *slot = Some(Arc::new(value));
        }
    }

    /// Re-reads the file if it changed on disk since the last read (desktop;
    /// does nothing in the browser). Returns whether it was reloaded.
    pub fn refresh(&self) -> bool {
        disk::refresh(self)
    }
}

impl<T: Default> Data<T> {
    /// The file's value. Read from disk on first use on desktop; the default
    /// until it's fetched in the browser (or if the file is missing).
    pub fn get(&self) -> Arc<T> {
        if let Ok(slot) = self.value.read()
            && let Some(v) = slot.as_ref()
        {
            return v.clone();
        }
        // Another thread may have loaded it meanwhile, so look either way.
        disk::refresh(self);
        if let Ok(slot) = self.value.read()
            && let Some(v) = slot.as_ref()
        {
            return v.clone();
        }
        Arc::new(T::default())
    }
}

/// Logs a bad data file (through Bevy's log when the plugin is on).
fn warn(msg: &str) {
    #[cfg(feature = "plugin")]
    bevy::log::warn!("{msg}");
    #[cfg(not(feature = "plugin"))]
    eprintln!("warning: {msg}");
}

/// A data file the game registers with [`DataPlugin`], whatever its type.
pub trait DataFile: Sync {
    fn path(&self) -> &'static str;
    fn loaded(&self) -> bool;
    fn set_bytes(&self, bytes: &[u8]) -> bool;
    fn refresh(&self) -> bool;
    /// The files still to fetch (browser): just this one until it's in.
    fn wanted(&self) -> Vec<String> {
        if self.loaded() {
            Vec::new()
        } else {
            vec![self.path().to_owned()]
        }
    }
    /// The bytes of `path`, one of [`DataFile::wanted`], arrived.
    fn set_file(&self, path: &str, bytes: &[u8]) -> bool {
        let _ = path;
        self.set_bytes(bytes)
    }
}

/// A folder of data files of one kind, listed by an index file (see the
/// crate docs). The index (`data/music/songs.ron`) is a RON list of file
/// names in the folder named after it (`data/music/songs/`); the files keep
/// the index's order. A file that's missing or doesn't parse is left out.
pub struct Many<T> {
    index: Data<Vec<String>>,
    parse: Parse<T>,
    files: RwLock<Vec<Entry<T>>>,
    value: RwLock<Option<Arc<Vec<Arc<T>>>>>,
    /// Held while reading from disk, so a reader never sees half a folder.
    loading: std::sync::Mutex<()>,
}

struct Entry<T> {
    name: String,
    value: Option<Arc<T>>,
    /// Read (or fetched) at least once, whether or not it parsed.
    tried: bool,
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    stamp: Option<std::time::SystemTime>,
}

impl<T: serde::de::DeserializeOwned> Many<T> {
    /// RON files listed by the RON index at `index`.
    pub const fn ron(index: &'static str) -> Self {
        Self {
            index: Data::ron(index),
            parse: |b| {
                let text = std::str::from_utf8(b).map_err(|e| e.to_string())?;
                ron::from_str(text).map_err(|e| e.to_string())
            },
            files: RwLock::new(Vec::new()),
            value: RwLock::new(None),
            loading: std::sync::Mutex::new(()),
        }
    }
}

impl<T> Many<T> {
    /// The index file's path under the assets folder.
    pub fn path(&self) -> &'static str {
        self.index.path()
    }

    /// The folder the listed files are in (the index's path without its
    /// extension).
    pub fn dir(&self) -> &'static str {
        let p = self.index.path();
        p.rsplit_once('.').map_or(p, |(stem, _)| stem)
    }

    /// The path of the listed file `name`.
    pub fn file_path(&self, name: &str) -> String {
        format!("{}/{name}", self.dir())
    }

    /// The file names the index lists, in order.
    pub fn names(&self) -> Vec<String> {
        self.files
            .read()
            .map(|f| f.iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Have the index and every file it lists been read (or fetched)?
    pub fn loaded(&self) -> bool {
        self.index.loaded() && self.files.read().is_ok_and(|f| f.iter().all(|e| e.tried))
    }

    /// Matches the entries to the index (keeping files already read);
    /// returns whether the list changed.
    fn sync_names(&self) -> bool {
        let names = if self.index.loaded() {
            self.index.get()
        } else {
            return false;
        };
        let Ok(mut files) = self.files.write() else {
            return false;
        };
        if files.len() == names.len() && files.iter().zip(names.iter()).all(|(e, n)| e.name == *n) {
            return false;
        }
        let mut old: Vec<Entry<T>> = std::mem::take(&mut *files);
        *files = names
            .iter()
            .map(|n| match old.iter().position(|e| e.name == *n) {
                Some(i) => old.swap_remove(i),
                None => Entry {
                    name: n.clone(),
                    value: None,
                    tried: false,
                    stamp: None,
                },
            })
            .collect();
        true
    }

    fn rebuild(&self) {
        let all = self
            .files
            .read()
            .map(|f| f.iter().filter_map(|e| e.value.clone()).collect())
            .unwrap_or_default();
        if let Ok(mut v) = self.value.write() {
            *v = Some(Arc::new(all));
        }
    }

    /// Parses `bytes` as the listed file `name`; a parse error keeps its
    /// last good value. Returns whether it took.
    pub fn set_named(&self, name: &str, bytes: &[u8]) -> bool {
        let took = self.parse_named(name, bytes);
        self.rebuild();
        took
    }

    fn parse_named(&self, name: &str, bytes: &[u8]) -> bool {
        let parsed = (self.parse)(bytes);
        let took = parsed.is_ok();
        if let Err(e) = &parsed {
            warn(&format!("data file {}: {e}", self.file_path(name)));
        }
        if let Ok(mut files) = self.files.write()
            && let Some(e) = files.iter_mut().find(|e| e.name == name)
        {
            e.tried = true;
            if let Ok(v) = parsed {
                e.value = Some(Arc::new(v));
            }
        }
        took
    }

    /// Sets the index's bytes (the browser fetch).
    pub fn set_index(&self, bytes: &[u8]) -> bool {
        let took = self.index.set_bytes(bytes);
        if self.sync_names() {
            self.rebuild();
        }
        took
    }

    /// Re-reads the index and every file that changed on disk (desktop;
    /// does nothing in the browser). Returns whether anything was reloaded.
    pub fn refresh(&self) -> bool {
        let _loading = self.loading.lock();
        let mut changed = self.index.refresh();
        changed |= self.sync_names();
        let todo: Vec<(String, Option<std::time::SystemTime>)> = self
            .files
            .read()
            .map(|f| f.iter().map(|e| (e.name.clone(), e.stamp)).collect())
            .unwrap_or_default();
        for (name, stamp) in todo {
            let path = self.file_path(&name);
            let Some(now) = disk::stamp(&path) else {
                // Missing: left out, and never waited for.
                if let Ok(mut files) = self.files.write()
                    && let Some(e) = files.iter_mut().find(|e| e.name == name)
                    && !e.tried
                {
                    e.tried = true;
                    changed = true;
                }
                continue;
            };
            if stamp == Some(now) {
                continue;
            }
            if let Ok(mut files) = self.files.write()
                && let Some(e) = files.iter_mut().find(|e| e.name == name)
            {
                e.stamp = Some(now);
            }
            if let Some(bytes) = disk::read(&path) {
                self.parse_named(&name, &bytes);
                changed = true;
            }
        }
        if changed || (self.value.read().is_ok_and(|v| v.is_none()) && self.index.loaded()) {
            self.rebuild();
        }
        changed
    }

    /// Every listed file's value, in the index's order: read from disk on
    /// first use on desktop, empty until fetched in the browser.
    pub fn get(&self) -> Arc<Vec<Arc<T>>> {
        if let Ok(v) = self.value.read()
            && let Some(v) = v.as_ref()
        {
            return v.clone();
        }
        Many::refresh(self);
        if let Ok(v) = self.value.read()
            && let Some(v) = v.as_ref()
        {
            return v.clone();
        }
        Arc::new(Vec::new())
    }
}

impl<T: Send + Sync> DataFile for Many<T> {
    fn path(&self) -> &'static str {
        Many::path(self)
    }
    fn loaded(&self) -> bool {
        Many::loaded(self)
    }
    fn set_bytes(&self, bytes: &[u8]) -> bool {
        self.set_index(bytes)
    }
    fn refresh(&self) -> bool {
        Many::refresh(self)
    }
    fn wanted(&self) -> Vec<String> {
        if !self.index.loaded() {
            return vec![self.path().to_owned()];
        }
        self.files
            .read()
            .map(|f| {
                f.iter()
                    .filter(|e| !e.tried)
                    .map(|e| self.file_path(&e.name))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn set_file(&self, path: &str, bytes: &[u8]) -> bool {
        if path == self.path() {
            return self.set_index(bytes);
        }
        let dir = self.dir();
        match path
            .strip_prefix(dir)
            .and_then(|rest| rest.strip_prefix('/'))
        {
            Some(name) => self.set_named(name, bytes),
            None => false,
        }
    }
}

impl<T: Send + Sync> DataFile for Data<T> {
    fn path(&self) -> &'static str {
        Data::path(self)
    }
    fn loaded(&self) -> bool {
        Data::loaded(self)
    }
    fn set_bytes(&self, bytes: &[u8]) -> bool {
        Data::set_bytes(self, bytes)
    }
    fn refresh(&self) -> bool {
        Data::refresh(self)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod disk {
    use std::path::PathBuf;
    use std::sync::RwLock;

    use super::Data;

    static ROOT: RwLock<Option<PathBuf>> = RwLock::new(None);

    /// The assets folder: what the game set, else the checkout's.
    pub fn root() -> PathBuf {
        if let Ok(r) = ROOT.read()
            && let Some(r) = r.as_ref()
        {
            return r.clone();
        }
        if let Ok(r) = std::env::var("FRIENDSLOP_ASSETS") {
            return PathBuf::from(r);
        }
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../game/assets")
    }

    pub fn set_root(dir: PathBuf) {
        if let Ok(mut r) = ROOT.write() {
            *r = Some(dir);
        }
    }

    /// When the file at `path` (under the assets folder) last changed.
    pub fn stamp(path: &str) -> Option<std::time::SystemTime> {
        std::fs::metadata(root().join(path))
            .and_then(|m| m.modified())
            .ok()
    }

    pub fn read(path: &str) -> Option<Vec<u8>> {
        std::fs::read(root().join(path)).ok()
    }

    pub fn refresh<T>(d: &Data<T>) -> bool {
        let file = root().join(d.path);
        let Ok(stamp) = std::fs::metadata(&file).and_then(|m| m.modified()) else {
            return false;
        };
        if d.stamp.read().is_ok_and(|s| *s == Some(stamp)) && d.loaded() {
            return false;
        }
        if d.failed.read().is_ok_and(|s| *s == Some(stamp)) {
            return false;
        }
        if let Ok(mut s) = d.stamp.write() {
            *s = Some(stamp);
        }
        let took = std::fs::read(&file).is_ok_and(|bytes| d.set_bytes(&bytes));
        if !took && let Ok(mut s) = d.failed.write() {
            *s = Some(stamp);
        }
        took
    }
}

#[cfg(target_arch = "wasm32")]
mod disk {
    use super::Data;

    pub fn set_root(_: std::path::PathBuf) {}

    pub fn stamp(_: &str) -> Option<std::time::SystemTime> {
        None
    }

    pub fn read(_: &str) -> Option<Vec<u8>> {
        None
    }

    pub fn refresh<T>(_: &Data<T>) -> bool {
        false
    }
}

/// Where the assets folder is (desktop: the game sets it at startup).
pub fn set_root(dir: std::path::PathBuf) {
    disk::set_root(dir);
}

#[cfg(feature = "plugin")]
mod plugin;
#[cfg(feature = "plugin")]
pub use plugin::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Deserialize, Default, Debug, PartialEq)]
    struct Two {
        a: u32,
        b: String,
    }

    static FILE: Data<Two> = Data::ron("data/nowhere/never.ron");

    #[test]
    fn bad_bytes_keep_the_last_good_value() {
        assert_eq!(*FILE.get(), Two::default(), "missing file: default");
        assert!(FILE.set_bytes(b"(a: 3, b: \"hi\") // a comment"));
        assert!(!FILE.set_bytes(b"(a: oops"));
        assert_eq!(FILE.get().a, 3);
        assert_eq!(FILE.get().b, "hi");
    }

    static SOME: Many<Two> = Many::ron("data/nowhere/many.ron");

    /// The browser path: the index, then each file it lists, in its order.
    #[test]
    fn many_fetches_the_index_then_its_files() {
        assert!(SOME.get().is_empty(), "missing index: empty");
        assert!(!DataFile::loaded(&SOME));
        assert_eq!(SOME.wanted(), vec!["data/nowhere/many.ron".to_owned()]);
        assert!(SOME.set_file("data/nowhere/many.ron", b"[\"b.ron\", \"a.ron\"]"));
        assert_eq!(
            SOME.wanted(),
            vec![
                "data/nowhere/many/b.ron".to_owned(),
                "data/nowhere/many/a.ron".to_owned()
            ]
        );
        assert!(SOME.set_file("data/nowhere/many/a.ron", b"(a: 1, b: \"a\")"));
        assert!(!DataFile::loaded(&SOME));
        assert!(!SOME.set_file("data/nowhere/many/b.ron", b"(a: oops"));
        assert!(DataFile::loaded(&SOME), "a bad file doesn't hold things up");
        assert_eq!(SOME.get().len(), 1);
        assert!(SOME.set_file("data/nowhere/many/b.ron", b"(a: 2, b: \"b\")"));
        let got: Vec<u32> = SOME.get().iter().map(|t| t.a).collect();
        assert_eq!(got, vec![2, 1], "the index's order");
    }
}
