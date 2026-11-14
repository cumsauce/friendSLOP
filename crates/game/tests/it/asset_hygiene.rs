//! Asset hygiene lint: no orphan models or textures, no junk files, CREDITS
//! enumerates every top-level asset group, and every binary-ish asset is in
//! Git LFS (so clones without LFS just miss files and degrade quietly).
//!
//! Allowlist maintenance (keep this comment current):
//! - `KNOWN_ORPHANS` is the exact set of models nothing references and no
//!   dynamic template covers. Today that is only `nyc-kit-cruiser`, which is
//!   deliberately unplaced (`hardening/modeler-todo.md` asks for place-or-drop).
//!   Placing it anywhere (a `.rs`, data `.ron` or `maps/*.json|bsn` literal) or
//!   deleting its file flips this test: shrink the list when that happens.
//! - A model is also covered when it lives under `models/features/` (the
//!   `format!("models/features/{file}")` loaders, afterlife.rs / helicopter.rs),
//!   when its stem is in `PROP_MODELS` (`crates/world/src/buildings.rs`), or
//!   when its stem starts with a dynamic family prefix (`hs-*`, `wh-tile-*` /
//!   `wh_tile-*`, `fs-tile-*` / `fs_tile-*`, `nyc-kit-*` — grammar trim, WFC
//!   tile kits and the map-placed street kit). New models that are placed by
//!   literal name need no entry anywhere: the corpus scan sees the literal.
//! - Stems are matched as raw substrings of the corpus (`crates/**/*.rs`,
//!   `assets/data/**.ron`, `assets/maps/*.json|*.bsn`), so a stem that is also
//!   a common word (e.g. `log`) passes trivially. That direction is safe: the
//!   lint only fails on references it cannot see at all.
//! - Binary-ish: everything under `assets/` whose extension is not in
//!   `TEXT_EXTS` must match a `filter=lfs` pattern in the root `.gitattributes`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Deliberately unreferenced models. See the allowlist comment above.
const KNOWN_ORPHANS: &[&str] = &["nyc-kit-cruiser"];

/// Stems loaded by family/dynamic template instead of a literal per file.
const FAMILY_PREFIXES: &[&str] = &[
    "hs-", "hs_", "wh-tile-", "wh_tile-", "fs-tile-", "fs_tile-", "nyc-kit-", "nyc_kit-",
];

/// Files under `assets/` that are fine as plain git blobs; anything else must
/// be covered by a root `.gitattributes` LFS pattern.
const TEXT_EXTS: &[&str] = &["ron", "json", "bsn", "md", "txt"];

/// Junk file names/`*.tmp` never belong under `assets/`.
const JUNK_NAMES: &[&str] = &[".DS_Store", "Thumbs.db"];

fn root() -> PathBuf {
    // No `..` segments: paths must compare equal to repo-relative ones.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .unwrap_or(manifest)
        .to_path_buf()
}

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("assets")
}

/// Recollect every `out/ext` file under `dir`, skipping `target` and hidden
/// folders, yielding repo-relative `/`-separated paths.
fn walk(dir: &Path, ext: &str, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue;
        }
        if p.is_dir() {
            if name != "target" {
                walk(&p, ext, out);
            }
        } else if p.extension().is_some_and(|x| x == ext) {
            let rel = p
                .strip_prefix(root())
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            out.push(rel);
        }
    }
}

/// Every file under `assets/`, repo-relative, plus whether it looks like junk.
fn walk_all(dir: &Path, out: &mut Vec<(String, bool)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        let junk = JUNK_NAMES.contains(&name.as_str()) || name.ends_with(".tmp");
        if p.is_dir() {
            walk_all(&p, out);
        } else {
            let rel = p
                .strip_prefix(root())
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, junk));
        }
    }
}

/// The corpus a model stem may be referenced from: `crates/**/*.rs` (except
/// this file, which names `KNOWN_ORPHANS`), `assets/data/**.ron` and
/// `assets/maps/*.json|*.bsn`.
fn corpus() -> String {
    let me = Path::new(file!()).file_name();
    let mut rs = Vec::new();
    walk(&root().join("crates"), "rs", &mut rs);
    let mut ron = Vec::new();
    walk(&assets().join("data"), "ron", &mut ron);
    let mut maps = Vec::new();
    for f in std::fs::read_dir(assets().join("maps"))
        .into_iter()
        .flatten()
        .flatten()
    {
        let p = f.path();
        let is_map = p
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x == "json" || x == "bsn");
        if is_map {
            let rel = p
                .strip_prefix(root())
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            maps.push(rel);
        }
    }
    let mut text = String::new();
    for rel in rs.into_iter().chain(ron).chain(maps) {
        let p = root().join(&rel);
        if p.file_name() == me {
            continue;
        }
        text.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
        text.push('\n');
    }
    text
}

/// The `PROP_MODELS` name list from `crates/world/src/buildings.rs`, parsed so
/// the lint cannot drift from the real list.
fn prop_models() -> Vec<String> {
    let Ok(src) = std::fs::read_to_string(root().join("crates/world/src/buildings.rs")) else {
        return Vec::new();
    };
    let Some(start) = src.find("pub const PROP_MODELS") else {
        return Vec::new();
    };
    let Some(end) = src[start..].find("];") else {
        return Vec::new();
    };
    src[start..start + end]
        .split('"')
        .skip(1)
        .step_by(2)
        .map(|s| s.to_string())
        .collect()
}

/// Models under `models/` with their stem.
fn models() -> Vec<(String, String)> {
    let mut out = Vec::new();
    walk(&assets().join("models"), "gltf", &mut out);
    out.into_iter()
        .map(|rel| {
            let stem = rel
                .rsplit('/')
                .next()
                .unwrap_or(&rel)
                .trim_end_matches(".gltf")
                .to_string();
            (rel, stem)
        })
        .collect()
}

/// A stem loaded by one of the dynamic templates (see the module comment).
fn dynamic_cover(rel: &str, stem: &str, props: &[String]) -> bool {
    rel.starts_with("crates/game/assets/models/features/")
        || props.iter().any(|p| p == stem)
        || FAMILY_PREFIXES.iter().any(|f| stem.starts_with(f))
}

/// Match a `.gitattributes` pattern against a repo-relative path: `**` crosses
/// directories (including none), `*` and `?` stay inside one segment.
fn pattern_matches(pattern: &str, path: &str) -> bool {
    fn m(pat: &[u8], p: &[u8]) -> bool {
        let Some(&c) = pat.first() else {
            return p.is_empty();
        };
        match c {
            b'*' => {
                let (cross, rest) = if pat.len() > 1 && pat[1] == b'*' {
                    if pat.len() > 2 && pat[2] == b'/' {
                        (true, &pat[3..])
                    } else {
                        (true, &pat[2..])
                    }
                } else {
                    (false, &pat[1..])
                };
                let mut i = 0;
                loop {
                    if m(rest, &p[i..]) {
                        return true;
                    }
                    if i >= p.len() || (!cross && p[i] == b'/') {
                        return false;
                    }
                    i += 1;
                }
            }
            b'?' => !p.is_empty() && p[0] != b'/' && m(&pat[1..], &p[1..]),
            _ => !p.is_empty() && p[0] == c && m(&pat[1..], &p[1..]),
        }
    }
    m(pattern.as_bytes(), path.as_bytes())
}

/// The `filter=lfs` patterns from the root `.gitattributes`.
fn lfs_patterns() -> Vec<String> {
    let Ok(src) = std::fs::read_to_string(root().join(".gitattributes")) else {
        return Vec::new();
    };
    src.lines()
        .filter(|l| l.contains("filter=lfs"))
        .filter_map(|l| l.split_whitespace().next().map(|p| p.to_string()))
        .collect()
}

/// The stems nothing references: not a corpus literal, not a dynamic template
/// cover, except the documented `KNOWN_ORPHANS` which must stay unreferenced.
fn orphans() -> BTreeSet<String> {
    let text = corpus();
    let props = prop_models();
    let mut found = BTreeSet::new();
    for (rel, stem) in models() {
        let known = KNOWN_ORPHANS.contains(&stem.as_str());
        let orphan = if known {
            // Documented exception: must stay a real orphan, so placing it
            // (a corpus literal) or removing the file flips this test.
            !text.contains(&stem)
        } else {
            !text.contains(&stem) && !dynamic_cover(&rel, &stem, &props)
        };
        if orphan {
            found.insert(stem);
        }
    }
    found
}

#[test]
fn the_only_orphan_model_is_the_known_one() {
    let found = orphans();
    let known: BTreeSet<String> = KNOWN_ORPHANS.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        found, known,
        "orphan models changed - place/remove the model or update KNOWN_ORPHANS \
         (see the allowlist comment in this file)"
    );
}

#[test]
fn every_texture_is_named_by_a_gltf() {
    let mut tex = Vec::new();
    walk(&assets().join("textures"), "ktx2", &mut tex);
    let mut gltfs = Vec::new();
    walk(&assets(), "gltf", &mut gltfs);
    let mut text = String::new();
    for rel in &gltfs {
        text.push_str(&std::fs::read_to_string(root().join(rel)).unwrap_or_default());
    }
    let mut orphans = Vec::new();
    for rel in &tex {
        let name = rel.rsplit('/').next().unwrap_or(rel);
        if !text.contains(name) {
            orphans.push(rel.clone());
        }
    }
    assert!(
        orphans.is_empty(),
        "textures no .gltf names (stale re-export leftovers?): {orphans:?}"
    );
}

#[test]
fn no_junk_under_assets() {
    let mut all = Vec::new();
    walk_all(&assets(), &mut all);
    let junk: Vec<&String> = all.iter().filter(|(_, j)| *j).map(|(r, _)| r).collect();
    assert!(junk.is_empty(), "junk files under assets/: {junk:?}");
}

#[test]
fn credits_covers_every_asset_group() {
    let credits = std::fs::read_to_string(assets().join("CREDITS.md"))
        .unwrap_or_default()
        .to_lowercase();
    let mut missing = Vec::new();
    for e in std::fs::read_dir(assets()).into_iter().flatten().flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string().to_lowercase();
        if !credits.contains(&format!("{name}/")) {
            missing.push(name);
        }
    }
    assert!(
        missing.is_empty(),
        "asset groups missing from CREDITS.md: {missing:?}"
    );
}

#[test]
fn every_binary_asset_is_in_lfs() {
    let patterns = lfs_patterns();
    assert!(
        !patterns.is_empty(),
        "no filter=lfs patterns in .gitattributes"
    );
    let mut all = Vec::new();
    walk_all(&assets(), &mut all);
    let mut uncovered = Vec::new();
    for (rel, _) in &all {
        let ext = rel.rsplit('.').next().unwrap_or("");
        if ext == rel || TEXT_EXTS.contains(&ext) {
            continue; // no extension or known text
        }
        if !patterns.iter().any(|p| pattern_matches(p, rel)) {
            uncovered.push(rel.clone());
        }
    }
    assert!(
        uncovered.is_empty(),
        "binary assets with no .gitattributes LFS pattern: {uncovered:?}"
    );
}

#[test]
fn glob_matcher_checks_out() {
    assert!(pattern_matches(
        "crates/game/assets/**/*.ktx2",
        "crates/game/assets/textures/a.ktx2"
    ));
    assert!(pattern_matches(
        "crates/game/assets/**/*.ktx2",
        "crates/game/assets/a.ktx2"
    ));
    assert!(!pattern_matches(
        "crates/game/assets/**/*.ktx2",
        "crates/game/other/a.ktx2"
    ));
    assert!(pattern_matches(
        "crates/game/assets/**/*.heights",
        "crates/game/assets/maps/x.heights"
    ));
    assert!(!pattern_matches(
        "crates/game/assets/**/*.ktx2",
        "crates/game/assets/a.ktx2.txt"
    ));
    assert!(pattern_matches("a/b?.c", "a/b1.c"));
    assert!(!pattern_matches("a/b?.c", "a/b/.c"));
}
