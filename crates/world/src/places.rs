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

/// The commune's map as plain data (the modeler writes `commune.map.json`
/// beside the scene), embedded so every peer reads the same layout before
/// any asset loads, the browser included.
const COMMUNE_MAP: &str = include_str!("../../game/assets/maps/commune.map.json");

/// A map as the modeler writes it beside the scene (`modeler::scene::MapFile`).
#[derive(serde::Deserialize, Default)]
struct MapFile {
    entities: Vec<MapEntity>,
}

/// One entity of a modeler map: a marker or a placed model, with the
/// components from `.modeler/components.json` it carries.
#[derive(serde::Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct MapEntity {
    pub name: String,
    /// The marker's kind (`spot`, `waypoint`, `plot`, `trip`, ...).
    pub kind: String,
    pub translation: [f32; 3],
    /// Facing in degrees about +y (0 faces +z).
    pub yaw: f32,
    /// A marker's box (x, y, z), if it has one.
    pub size: Option<[f32; 3]>,
    /// The model it places, if any.
    pub gltf: Option<String>,
    pub components: serde_json::Map<String, serde_json::Value>,
}

impl MapEntity {
    /// Where it stands on the ground (x, z).
    pub fn at(&self) -> Vec2 {
        Vec2::new(self.translation[0], self.translation[2])
    }

    /// Its box on the ground (x, z size); zero without one.
    pub fn footprint(&self) -> Vec2 {
        self.size.map_or(Vec2::ZERO, |s| Vec2::new(s[0], s[2]))
    }

    /// Has it the component `c`?
    pub fn has(&self, c: &str) -> bool {
        self.components.contains_key(c)
    }

    fn field(&self, c: &str, field: &str) -> Option<&serde_json::Value> {
        self.components.get(c)?.get(field)
    }

    /// A string field; empty when missing.
    pub fn text(&self, c: &str, field: &str) -> String {
        self.field(c, field)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    }

    /// A number field; 0 when missing (the component's default).
    pub fn num(&self, c: &str, field: &str) -> f32 {
        self.field(c, field)
            .and_then(|v| v.as_f64())
            .unwrap_or_default() as f32
    }

    /// A bool field; false when missing.
    pub fn flag(&self, c: &str, field: &str) -> bool {
        self.field(c, field)
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    /// A list of names (a `refs` / `strings` field); empty when missing.
    pub fn names(&self, c: &str, field: &str) -> Vec<&str> {
        self.field(c, field)
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default()
    }

    /// Its Spot's name, if it is one.
    pub fn spot(&self) -> Option<String> {
        self.has("Spot").then(|| self.text("Spot", "name"))
    }
}

/// Reads a modeler `.map.json`; an unreadable map reads as empty (the
/// map test catches it).
pub fn read_map(json: &str) -> Vec<MapEntity> {
    serde_json::from_str::<MapFile>(json)
        .map(|m| m.entities)
        .unwrap_or_default()
}

/// The commune map's entities.
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
        .map(|e| e.at())
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

/// A spot's ground feature sizes (its `Feature` component; `size` is the
/// marker's own box, x by z).
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

/// A static collider from the map: a box (or upright cylinder) of `size`
/// at `centre`, whose y is above the ground at `ground_at`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapBlock {
    pub centre: Vec3,
    pub size: Vec3,
    /// Turn about +y, radians.
    pub yaw: f32,
    pub round: bool,
    pub ground_at: Vec2,
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
    /// Static colliders the map places (see [`MapBlock`]).
    pub blocks: Vec<MapBlock>,
    /// Every spot's sizes, by name (see [`sizes`]).
    pub spots: std::collections::HashMap<String, Sizes>,
}

/// Reads the layout out of a map's entities.
pub fn layout_of(map: &[MapEntity]) -> Layout {
    let pos = |name: &str| map.iter().find(|e| e.name == name).map(|e| e.at());
    let sizes = |name: &str| {
        map.iter()
            .find(|e| e.spot().as_deref() == Some(name))
            .map(|e| Sizes {
                radius: e.num("Feature", "radius"),
                height: e.num("Feature", "height"),
                depth: e.num("Feature", "depth"),
                size: e.footprint(),
            })
            .unwrap_or_default()
    };
    let mut districts: Vec<(u32, String, Vec2, Vec2)> = map
        .iter()
        .filter(|e| e.has("District"))
        .map(|e| {
            let half = e.footprint() / 2.0;
            (
                e.num("District", "order") as u32,
                e.text("District", "kind"),
                e.at() - half,
                e.at() + half,
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
                let from = e.at();
                e.names("Path", "to")
                    .into_iter()
                    .filter_map(pos)
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
                at: e.at(),
                size: e.footprint(),
                turns: ((e.yaw / 90.0).round() as i32).rem_euclid(4),
                upper: e.flag("Plot", "upper"),
            })
            .collect(),
        hangouts: map
            .iter()
            .filter(|e| e.has("Hangout") && e.kind != "trip")
            .map(|e| e.at())
            .collect(),
        trips: map
            .iter()
            .filter(|e| e.has("Hangout") && e.kind == "trip")
            .map(|e| e.at())
            .collect(),
        clears: map
            .iter()
            .filter(|e| e.has("Clear"))
            .map(|e| (e.at(), e.num("Clear", "radius")))
            .collect(),
        pond: sizes("pond"),
        hill: sizes("hill"),
        lake: sizes("lake"),
        quarry: sizes("quarry"),
        arena: sizes("arena"),
        village: sizes("village"),
        blocks: map
            .iter()
            .filter(|e| e.has("Block"))
            .map(|e| {
                let on = e.text("Block", "on");
                MapBlock {
                    centre: Vec3::from_array(e.translation),
                    size: e.size.map_or(Vec3::ONE, Vec3::from_array),
                    yaw: e.yaw.to_radians(),
                    round: e.flag("Block", "round"),
                    ground_at: pos(&on).unwrap_or(e.at()),
                }
            })
            .collect(),
        spots: map
            .iter()
            .filter_map(|e| Some((e.spot()?, sizes(&e.spot()?))))
            .collect(),
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

/// A spot's sizes (its `Feature` and box); zero for an unknown spot.
pub fn sizes(name: &str) -> Sizes {
    layout().spots.get(name).copied().unwrap_or_default()
}

/// Half the duel arena's width (its `Feature` radius).
pub fn arena_half() -> f32 {
    layout().arena.radius
}

/// Keeps the dressing off the models the commune map places.
pub fn claim_map_props() {
    for e in commune().iter().filter(|e| e.gltf.is_some()) {
        crate::footprints::claim(e.at(), 1.2);
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
            crates.and_then(|e| e.gltf.as_deref()),
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
