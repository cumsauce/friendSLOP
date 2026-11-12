//! Where the map's fixed places stand: each is a `Spot` marker in the
//! modeler's `commune-map` document, exported to `assets/maps/commune.bsn`
//! and embedded here, so every peer reads the same spots before any asset
//! loads. Move a place in the modeler, re-export, and the ground, scenery
//! and features follow. A new place: add a marker with a `Spot` component,
//! its name to the end of [`PLACES`] and a `<name>_at()` function below.
//!
//! The rest of the layout lives there too, read once into [`layout()`]:
//! feature sizes, paths, districts, house plots, villager hangouts and
//! ground kept clear (the components in `crates/game/.modeler/components.json`).

use bevy::prelude::*;

/// The commune's map as the modeler exported it (embedded, so every peer
/// reads the same spots before any asset loads).
const COMMUNE_MAP: &str = include_str!("../../game/assets/maps/commune.bsn");

/// One entity of a modeler scene, read from its text: its name, position and
/// the map components it carries, as `component -> field -> raw value`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MapEntity {
    pub name: String,
    pub at: Vec3,
    pub components: Vec<(String, Vec<(String, String)>)>,
}

impl MapEntity {
    /// Has it the component `c` (the type's last path segment)?
    pub fn has(&self, c: &str) -> bool {
        self.components.iter().any(|(n, _)| n == c)
    }

    /// The raw value of `c.field`, if written.
    pub fn raw(&self, c: &str, field: &str) -> Option<&str> {
        let (_, fields) = self.components.iter().find(|(n, _)| n == c)?;
        fields
            .iter()
            .find(|(k, _)| k == field)
            .map(|(_, v)| v.as_str())
    }

    /// A string field (quotes stripped); empty when missing.
    pub fn text(&self, c: &str, field: &str) -> String {
        self.raw(c, field)
            .map(|v| v.trim_matches('"').to_string())
            .unwrap_or_default()
    }

    /// A number field; 0 when missing (the component's default).
    pub fn num(&self, c: &str, field: &str) -> f32 {
        self.raw(c, field)
            .and_then(|v| v.parse().ok())
            .unwrap_or_default()
    }

    /// A bool field; false when missing.
    pub fn flag(&self, c: &str, field: &str) -> bool {
        self.raw(c, field) == Some("true")
    }

    /// A `Vec2` field; zero when missing.
    pub fn vec2(&self, c: &str, field: &str) -> Vec2 {
        let v = self.raw(c, field).unwrap_or_default();
        Vec2::new(coord(v, "x"), coord(v, "y"))
    }

    /// Its Spot's name, if it is one.
    pub fn spot(&self) -> Option<String> {
        self.has("Spot").then(|| self.text("Spot", "name"))
    }

    /// The model it places, if any.
    pub fn model(&self) -> Option<String> {
        self.has("GltfSource")
            .then(|| self.text("GltfSource", "path"))
    }
}

/// `axis` of a written vector (`glam::Vec3 { x: 1.0, y: 0.0, z: 2.0 }`).
fn coord(v: &str, axis: &str) -> f32 {
    let key = format!("{axis}: ");
    v.find(&key)
        .map(|i| &v[i + key.len()..])
        .and_then(|r| r[..r.find([',', ' ', '}']).unwrap_or(r.len())].parse().ok())
        .unwrap_or_default()
}

/// Reads a modeler `.bsn` scene's top-level entities. It knows the modeler's
/// own layout: `#name`, then one component per 4-space line, its fields on
/// 8-space `key: value,` lines.
pub fn read_map(text: &str) -> Vec<MapEntity> {
    text.split("\n    --")
        .filter_map(|block| {
            let mut e = MapEntity {
                name: block
                    .split('#')
                    .nth(1)?
                    .split_whitespace()
                    .next()?
                    .to_string(),
                ..default()
            };
            for line in block.lines() {
                let depth = line.len() - line.trim_start().len();
                let line = line.trim();
                if depth == 4 && !line.starts_with('#') {
                    let path = line.split([' ', '{', '(']).next().unwrap_or_default();
                    let name = path.rsplit("::").next().unwrap_or_default();
                    e.components.push((name.to_string(), Vec::new()));
                } else if depth == 8
                    && let Some((k, v)) = line.split_once(": ")
                    && let Some((_, fields)) = e.components.last_mut()
                {
                    fields.push((k.to_string(), v.trim_end_matches(',').to_string()));
                }
            }
            e.at = e
                .raw("Transform", "translation")
                .map(|v| Vec3::new(coord(v, "x"), coord(v, "y"), coord(v, "z")))
                .unwrap_or_default();
            Some(e)
        })
        .collect()
}

/// The commune map's entities, sorted by name (the modeler writes them so).
pub fn commune() -> &'static [MapEntity] {
    static MAP: std::sync::OnceLock<Vec<MapEntity>> = std::sync::OnceLock::new();
    MAP.get_or_init(|| read_map(COMMUNE_MAP))
}

/// The entity called `name` on the commune map.
pub fn entity(name: &str) -> Option<&'static MapEntity> {
    commune().iter().find(|e| e.name == name)
}

/// Where the commune map puts the spot called `name` (x, z).
pub fn spot(name: &str) -> Option<Vec2> {
    commune()
        .iter()
        .find(|e| e.spot().as_deref() == Some(name))
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
    "centre",
    "forest_clearing",
    "ruins_tower",
    "village",
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

/// A spot's ground feature sizes (its `Feature` component).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sizes {
    pub radius: f32,
    pub height: f32,
    pub depth: f32,
    pub size: Vec2,
}

/// A house plot from the map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HousePlot {
    pub at: Vec2,
    /// Footprint (w, d).
    pub size: Vec2,
    pub turns: i32,
    pub upper: bool,
}

/// Everything the map lays out, read once from the commune map.
#[derive(Debug, Default)]
pub struct Layout {
    /// Path segments between markers.
    pub paths: Vec<(Vec2, Vec2)>,
    /// Districts by `order`: kind and box (min, max).
    pub districts: Vec<(String, Vec2, Vec2)>,
    /// The map's edge (the `bounds` district): half its size.
    pub half: Vec2,
    pub plots: Vec<HousePlot>,
    /// Villager hangouts, and their far (navmesh) trips, in name order.
    pub hangouts: Vec<Vec2>,
    pub trips: Vec<Vec2>,
    /// Ground kept clear of dressing: centre and radius.
    pub clears: Vec<(Vec2, f32)>,
    pub pond: Sizes,
    pub hill: Sizes,
    pub lake: Sizes,
    pub quarry: Sizes,
    pub arena: Sizes,
    pub village: Sizes,
}

/// Reads the layout out of a map's entities.
pub fn layout_of(map: &[MapEntity]) -> Layout {
    let pos = |name: &str| map.iter().find(|e| e.name == name).map(|e| e.at.xz());
    let sizes = |name: &str| {
        map.iter()
            .find(|e| e.spot().as_deref() == Some(name))
            .map(|e| Sizes {
                radius: e.num("Feature", "radius"),
                height: e.num("Feature", "height"),
                depth: e.num("Feature", "depth"),
                size: e.vec2("Feature", "size"),
            })
            .unwrap_or_default()
    };
    let mut districts: Vec<(u32, String, Vec2, Vec2)> = map
        .iter()
        .filter(|e| e.has("District"))
        .map(|e| {
            let half = e.vec2("District", "size") / 2.0;
            (
                e.num("District", "order") as u32,
                e.text("District", "kind"),
                e.at.xz() - half,
                e.at.xz() + half,
            )
        })
        .collect();
    districts.sort_by_key(|d| d.0);
    let half = districts
        .iter()
        .find(|d| d.1 == "bounds")
        .map(|d| (d.3 - d.2) / 2.0)
        .unwrap_or(Vec2::splat(88.0));
    Layout {
        paths: map
            .iter()
            .filter(|e| e.has("Path"))
            .flat_map(|e| {
                let from = e.at.xz();
                e.text("Path", "to")
                    .split(',')
                    .filter_map(|n| pos(n.trim()))
                    .map(move |to| (from, to))
                    .collect::<Vec<_>>()
            })
            .collect(),
        districts: districts
            .into_iter()
            .filter(|d| d.1 != "bounds")
            .map(|(_, k, lo, hi)| (k, lo, hi))
            .collect(),
        half,
        plots: map
            .iter()
            .filter(|e| e.has("Plot"))
            .map(|e| HousePlot {
                at: e.at.xz(),
                size: e.vec2("Plot", "size"),
                turns: e.num("Plot", "turns") as i32,
                upper: e.flag("Plot", "upper"),
            })
            .collect(),
        hangouts: map
            .iter()
            .filter(|e| e.has("Hangout") && !e.flag("Hangout", "trip"))
            .map(|e| e.at.xz())
            .collect(),
        trips: map
            .iter()
            .filter(|e| e.has("Hangout") && e.flag("Hangout", "trip"))
            .map(|e| e.at.xz())
            .collect(),
        clears: map
            .iter()
            .filter(|e| e.has("Clear"))
            .map(|e| (e.at.xz(), e.num("Clear", "radius")))
            .collect(),
        pond: sizes("pond"),
        hill: sizes("hill"),
        lake: sizes("lake"),
        quarry: sizes("quarry"),
        arena: sizes("arena"),
        village: sizes("village"),
    }
}

/// The commune map's layout (read once; the terrain asks per sample).
pub fn layout() -> &'static Layout {
    static LAYOUT: std::sync::OnceLock<Layout> = std::sync::OnceLock::new();
    LAYOUT.get_or_init(|| layout_of(commune()))
}

/// The commune's middle, where paths meet.
pub fn centre_at() -> Vec2 {
    placed(23)
}

/// The forest clearing (the woodcutter's camp).
pub fn forest_clearing_at() -> Vec2 {
    placed(24)
}

/// The ruins' collapsed tower.
pub fn ruins_tower_at() -> Vec2 {
    placed(25)
}

/// Half the duel arena's width (its `Feature` radius).
pub fn arena_half() -> f32 {
    layout().arena.radius
}

/// Keeps the dressing off the models the commune map places.
pub fn claim_map_props() {
    for e in commune().iter().filter(|e| e.model().is_some()) {
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
            crates.and_then(|e| e.model()).as_deref(),
            Some("models/prefabs/fu-crates.gltf")
        );
        assert!(spot("nowhere").is_none());
        let l = layout();
        assert_eq!(l.paths.len(), 13);
        assert_eq!(l.plots.len(), 6);
        assert_eq!(l.plots.iter().filter(|p| p.upper).count(), 2);
        assert_eq!((l.hangouts.len(), l.trips.len()), (15, 7));
        assert_eq!(l.half, Vec2::splat(88.0));
        assert_eq!(l.districts[0].0, "commune");
        assert_eq!(l.lake.height, -0.35);
        assert_eq!(arena_half(), 7.0);
        assert!(l.clears.len() >= 16);
    }
}
