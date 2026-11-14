//! Places never write the world's light themselves (`place.rs`): only the
//! day/night cycle does, laying the place you're in over the outdoors. Each
//! place used to save the sun, ambient light and fog and put them back, and
//! two overlapping places put back each other's darkness (NYC stuck dark
//! after the PATH). These keep it that way.

use std::path::{Path, PathBuf};

/// The only files allowed to write the sun, the ambient light or the fog:
/// the day/night cycle, and the menu's own little scene.
const WRITERS: [&str; 2] = ["crates/game/src/atmosphere.rs", "crates/game/src/menu.rs"];

/// What writing them looks like.
const WRITES: [&str; 4] = [
    ".illuminance =",
    ".brightness =",
    "DistanceFog {",
    "remove::<DistanceFog>",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            rust_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

fn sources() -> Vec<(String, String)> {
    let mut files = Vec::new();
    let Ok(crates) = std::fs::read_dir(root().join("crates")) else {
        panic!("no crates folder");
    };
    for c in crates.flatten() {
        rust_files(&c.path().join("src"), &mut files);
    }
    files
        .into_iter()
        .filter_map(|p| {
            let text = std::fs::read_to_string(&p).ok()?;
            let rel = p
                .strip_prefix(root())
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            Some((rel, text))
        })
        .collect()
}

#[test]
fn only_the_day_night_cycle_writes_the_light() {
    let mut bad = Vec::new();
    for (path, text) in sources() {
        if WRITERS.contains(&path.as_str()) {
            continue;
        }
        for (n, line) in text.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("//") {
                continue;
            }
            if WRITES.iter().any(|w| code.contains(w)) {
                bad.push(format!("{path}:{}: {}", n + 1, code));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "places push a look (place.rs, data/base/place_looks.ron) instead of \
         writing the sun, ambient light or fog:\n{}",
        bad.join("\n")
    );
}

#[test]
fn every_place_look_is_in_the_data_file() {
    let looks =
        std::fs::read_to_string(root().join("crates/game/assets/data/base/place_looks.ron"))
            .unwrap_or_default();
    let mut ids = Vec::new();
    for (path, text) in sources() {
        for line in text.lines() {
            let code = line.trim_start();
            let Some(rest) = code
                .strip_prefix("pub const ")
                .or_else(|| code.strip_prefix("const "))
            else {
                continue;
            };
            let Some((name, value)) = rest.split_once(": &str = \"") else {
                continue;
            };
            if name.ends_with("LOOK") {
                let id = value.trim_end_matches("\";").to_owned();
                ids.push((path.clone(), id));
            }
        }
    }
    assert!(ids.len() >= 7, "found only {ids:?}");
    for (path, id) in &ids {
        assert!(
            looks.contains(&format!("\"{id}\":")),
            "{path} pushes the look {id:?}, which data/base/place_looks.ron doesn't have"
        );
    }
}
