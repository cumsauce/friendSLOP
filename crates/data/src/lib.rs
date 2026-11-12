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

use std::sync::{Arc, RwLock};

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;

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
                warn!("data file {}: {e}", self.path);
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

/// A data file the game registers with [`DataPlugin`], whatever its type.
pub trait DataFile: Sync {
    fn path(&self) -> &'static str;
    fn loaded(&self) -> bool;
    fn set_bytes(&self, bytes: &[u8]) -> bool;
    fn refresh(&self) -> bool;
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

    pub fn refresh<T>(d: &Data<T>) -> bool {
        let file = root().join(d.path);
        let Ok(stamp) = std::fs::metadata(&file).and_then(|m| m.modified()) else {
            return false;
        };
        if d.stamp.read().is_ok_and(|s| *s == Some(stamp)) && d.loaded() {
            return false;
        }
        if let Ok(mut s) = d.stamp.write() {
            *s = Some(stamp);
        }
        std::fs::read(&file).is_ok_and(|bytes| d.set_bytes(&bytes))
    }
}

#[cfg(target_arch = "wasm32")]
mod disk {
    use super::Data;

    pub fn set_root(_: std::path::PathBuf) {}

    pub fn refresh<T>(_: &Data<T>) -> bool {
        false
    }
}

/// Where the assets folder is (desktop: the game sets it at startup).
pub fn set_root(dir: std::path::PathBuf) {
    disk::set_root(dir);
}

/// The game's data files, loaded and kept fresh.
pub struct DataPlugin {
    pub files: Vec<&'static dyn DataFile>,
}

#[derive(Resource)]
struct Files {
    files: Vec<&'static dyn DataFile>,
    /// Browser: the fetches in flight.
    handles: Vec<(usize, Handle<RawData>)>,
    since_check: f32,
}

/// Fired when a data file changes on disk and was reloaded.
#[derive(Message, Clone, Debug)]
pub struct DataReloaded(pub &'static str);

impl Plugin for DataPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<RawData>()
            .register_asset_loader(RawLoader)
            .add_message::<DataReloaded>()
            .insert_resource(Files {
                files: self.files.clone(),
                handles: Vec::new(),
                since_check: 0.0,
            })
            .add_systems(Startup, fetch)
            .add_systems(Update, (take_fetched, watch));
    }
}

/// Every registered file is in.
pub fn all_loaded(files: &[&'static dyn DataFile]) -> bool {
    files.iter().all(|f| f.loaded())
}

/// Is every data file the game registered loaded?
pub fn ready(files: Option<Res<FilesReady>>) -> bool {
    files.is_some_and(|f| f.0)
}

/// Set once every registered data file is loaded (the loading curtain waits).
#[derive(Resource, Default)]
pub struct FilesReady(pub bool);

fn fetch(mut files: ResMut<Files>, assets: Res<AssetServer>, mut commands: Commands) {
    commands.init_resource::<FilesReady>();
    if cfg!(target_arch = "wasm32") {
        let handles = files
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| (i, assets.load(f.path())))
            .collect();
        files.handles = handles;
    } else {
        // Desktop reads from disk now, so systems never see a default.
        for f in &files.files {
            f.refresh();
        }
    }
}

fn take_fetched(
    mut files: ResMut<Files>,
    raw: Res<Assets<RawData>>,
    mut ready: Option<ResMut<FilesReady>>,
) {
    let Files {
        files: list,
        handles,
        ..
    } = &mut *files;
    handles.retain(|(i, h)| {
        let Some(data) = raw.get(h) else {
            return true;
        };
        if let Some(f) = list.get(*i) {
            f.set_bytes(&data.0);
        }
        false
    });
    if let Some(ready) = ready.as_mut()
        && !ready.0
    {
        ready.0 = all_loaded(list);
    }
}

/// Desktop: re-reads changed files about once a second.
fn watch(time: Res<Time>, mut files: ResMut<Files>, mut out: MessageWriter<DataReloaded>) {
    files.since_check += time.delta_secs();
    if files.since_check < 1.0 || cfg!(target_arch = "wasm32") {
        return;
    }
    files.since_check = 0.0;
    for f in &files.files {
        if f.loaded() && f.refresh() {
            info!("reloaded {}", f.path());
            out.write(DataReloaded(f.path()));
        }
    }
}

/// A data file's raw bytes, as fetched by the asset server.
#[derive(Asset, TypePath, Debug)]
pub struct RawData(pub Vec<u8>);

#[derive(Default, TypePath)]
struct RawLoader;

impl AssetLoader for RawLoader {
    type Asset = RawData;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> Result<RawData, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        Ok(RawData(bytes))
    }

    fn extensions(&self) -> &[&str] {
        &["ron", "json", "heights"]
    }
}

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
}
