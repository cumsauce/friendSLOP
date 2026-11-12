//! Where the map's fixed places stand. The map shapes the ground around them
//! (flattened arenas, paths between them), so they live down here; the
//! features that use those places import them from here.

use bevy::prelude::*;

/// The commune's parliament (burned down; Dave's revolt, #3).
pub const PARLIAMENT_AT: Vec2 = Vec2::new(-12.0, -10.0);
/// The commune bonfire, in front of the parliament.
pub const BONFIRE_AT: Vec2 = Vec2::new(PARLIAMENT_AT.x + 3.5, PARLIAMENT_AT.y + 8.5);
/// The duel arena: centre and half its width (a flat square, #20 and on).
pub const ARENA_CENTER: Vec3 = Vec3::new(0.0, 0.0, -12.0);
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

/// The bonfire: its spot on the commune map, else [`BONFIRE_AT`].
pub fn bonfire() -> Vec2 {
    spot("bonfire").unwrap_or(BONFIRE_AT)
}

/// The parliament: its spot on the commune map, else [`PARLIAMENT_AT`].
pub fn parliament() -> Vec2 {
    spot("parliament").unwrap_or(PARLIAMENT_AT)
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
        assert!(bonfire().distance(BONFIRE_AT) < 0.01);
        assert!(parliament().distance(PARLIAMENT_AT) < 0.01);
        let crates = commune().iter().find(|e| e.name == "picnic_crates");
        assert_eq!(
            crates.and_then(|e| e.model.as_deref()),
            Some("models/prefabs/fu-crates.gltf")
        );
        assert!(spot("nowhere").is_none());
    }
}
