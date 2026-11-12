//! Where the map's fixed places stand: each is a `Spot` marker in the
//! modeler's `commune-map` document, exported to `assets/maps/commune.bsn`
//! and embedded here, so every peer reads the same spots before any asset
//! loads. Move a place in the modeler, re-export, and the ground, scenery
//! and features follow. A new place: add a marker with a `Spot` component,
//! its name to [`PLACES`] and a `<name>_at()` function below.

use bevy::prelude::*;

pub const ARENA_HALF: f32 = 7.0;

/// The commune's map as the modeler exported it (embedded, so every peer
/// reads the same spots before any asset loads).
const COMMUNE_MAP: &str = include_str!("../../game/assets/maps/commune.bsn");

/// One entity of a modeler scene, read from its text: name, position, the
/// `Spot` name it carries and the model it places.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapEntity {
    pub name: String,
    pub at: Vec3,
    pub spot: Option<String>,
    pub model: Option<String>,
}

/// Reads a modeler `.bsn` scene's top-level entities (the modeler's own
/// layout: `#name`, a `Transform` translation, `Spot`/`GltfSource` fields).
pub fn read_map(text: &str) -> Vec<MapEntity> {
    let quoted = |s: &str, key: &str| -> Option<String> {
        let rest = &s[s.find(key)? + key.len()..];
        let rest = &rest[rest.find('"')? + 1..];
        Some(rest[..rest.find('"')?].to_string())
    };
    let num = |s: &str, key: &str| -> f32 {
        s.find(key)
            .map(|i| &s[i + key.len()..])
            .and_then(|r| {
                let end = r.find([',', ' ', '}']).unwrap_or(r.len());
                r[..end].trim().parse().ok()
            })
            .unwrap_or(0.0)
    };
    text.split("\n    --")
        .filter_map(|block| {
            let name = block
                .split('#')
                .nth(1)?
                .split_whitespace()
                .next()?
                .to_string();
            let at = block
                .find("translation:")
                .map(|i| {
                    let t = &block[i..block[i..].find('}').map_or(block.len(), |e| i + e)];
                    Vec3::new(num(t, "x: "), num(t, "y: "), num(t, "z: "))
                })
                .unwrap_or_default();
            let spot = block
                .find("map::Spot")
                .and_then(|i| quoted(&block[i..], "name:"));
            let model = block
                .find("GltfSource")
                .and_then(|i| quoted(&block[i..], "path:"));
            Some(MapEntity {
                name,
                at,
                spot,
                model,
            })
        })
        .collect()
}

fn commune() -> &'static [MapEntity] {
    static MAP: std::sync::OnceLock<Vec<MapEntity>> = std::sync::OnceLock::new();
    MAP.get_or_init(|| read_map(COMMUNE_MAP))
}

/// Where the commune map puts the spot called `name` (x, z).
pub fn spot(name: &str) -> Option<Vec2> {
    commune()
        .iter()
        .find(|e| e.spot.as_deref() == Some(name))
        .map(|e| e.at.xz())
}

/// Every place the game needs a spot for (the map test checks them all).
pub const PLACES: &[&str] = &[
    "parliament",
    "bonfire",
    "lectern",
    "hell",
    "warehouse",
    "wendys",
    "market",
    "tower",
    "barn",
    "big_cottage",
    "pond",
    "hill",
    "lake",
    "quarry",
    "wreck",
    "officer",
    "maze",
    "windmill",
    "dave",
    "vault",
    "gamble",
    "arena",
    "landlord",
];

/// Where a place is: its spot on the commune map. A missing spot (the map
/// test catches it) puts the place at the origin.
pub fn at(name: &str) -> Vec2 {
    spot(name).unwrap_or_default()
}

/// [`PLACES`]`[i]`, looked up once (the terrain asks per height sample).
fn placed(i: usize) -> Vec2 {
    static TABLE: std::sync::OnceLock<Vec<Vec2>> = std::sync::OnceLock::new();
    TABLE
        .get_or_init(|| PLACES.iter().map(|p| at(p)).collect())
        .get(i)
        .copied()
        .unwrap_or_default()
}

/// The `parliament` spot on the commune map.
pub fn parliament_at() -> Vec2 {
    placed(0)
}

/// The `bonfire` spot on the commune map.
pub fn bonfire_at() -> Vec2 {
    placed(1)
}

/// The `lectern` spot on the commune map.
pub fn lectern_at() -> Vec2 {
    placed(2)
}

/// The `hell` spot on the commune map.
pub fn hell_at() -> Vec2 {
    placed(3)
}

/// The `warehouse` spot on the commune map.
pub fn outside_at() -> Vec2 {
    placed(4)
}

/// The `wendys` spot on the commune map.
pub fn wendys_at() -> Vec2 {
    placed(5)
}

/// The `market` spot on the commune map.
pub fn market_at() -> Vec2 {
    placed(6)
}

/// The `tower` spot on the commune map.
pub fn tower_at() -> Vec2 {
    placed(7)
}

/// The `barn` spot on the commune map.
pub fn barn_at() -> Vec2 {
    placed(8)
}

/// The `big_cottage` spot on the commune map.
pub fn big_cottage_at() -> Vec2 {
    placed(9)
}

/// The `pond` spot on the commune map.
pub fn pond_at() -> Vec2 {
    placed(10)
}

/// The `hill` spot on the commune map.
pub fn hill_at() -> Vec2 {
    placed(11)
}

/// The `lake` spot on the commune map.
pub fn lake_at() -> Vec2 {
    placed(12)
}

/// The `quarry` spot on the commune map.
pub fn quarry_at() -> Vec2 {
    placed(13)
}

/// The `wreck` spot on the commune map.
pub fn wreck_at() -> Vec2 {
    placed(14)
}

/// The `officer` spot on the commune map.
pub fn officer_at() -> Vec2 {
    placed(15)
}

/// The `maze` spot on the commune map.
pub fn maze_at() -> Vec2 {
    placed(16)
}

/// The `windmill` spot on the commune map.
pub fn windmill_at() -> Vec2 {
    placed(17)
}

/// The `dave` spot on the commune map.
pub fn dave_at() -> Vec2 {
    placed(18)
}

/// The `vault` spot on the commune map.
pub fn vault_at() -> Vec2 {
    placed(19)
}

/// The `gamble` spot on the commune map.
pub fn gamble_at() -> Vec2 {
    placed(20)
}
/// The `arena` spot on the commune map, on the ground plane.
pub fn arena_center() -> Vec3 {
    let p = placed(21);
    Vec3::new(p.x, 0.0, p.y)
}

/// The `landlord` spot on the commune map, on the ground plane.
pub fn landlord_at() -> Vec3 {
    let p = placed(22);
    Vec3::new(p.x, 0.0, p.y)
}

/// Keeps the dressing off the models the commune map places.
pub fn claim_map_props() {
    for e in commune().iter().filter(|e| e.model.is_some()) {
        crate::footprints::claim(e.at.xz(), 1.2);
    }
}

#[cfg(test)]
mod map_tests {
    use super::*;

    #[test]
    fn the_commune_map_has_its_spots() {
        assert_eq!(spot("picnic"), Some(Vec2::new(6.0, 20.0)));
        for p in PLACES {
            assert!(spot(p).is_some(), "no `{p}` spot on the commune map");
        }
        assert_eq!(bonfire_at(), Vec2::new(-8.5, -1.5));
        assert_eq!(wendys_at(), at("wendys"));
        assert_eq!(landlord_at(), Vec3::new(13.0, 0.0, -4.0));
        let crates = commune().iter().find(|e| e.name == "picnic_crates");
        assert_eq!(
            crates.and_then(|e| e.model.as_deref()),
            Some("models/prefabs/fu-crates.gltf")
        );
        assert!(spot("nowhere").is_none());
    }
}
