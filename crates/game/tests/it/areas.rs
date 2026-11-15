//! The areas' guards (docs/design/performance-and-areas.md, Part 2,
//! `crates/base/src/areas.rs`): everything spawned into the world belongs to
//! one area or is a global, and these tests keep it that way while the
//! places move over one by one.

use avian3d::prelude::Collider;
use bevy::prelude::*;
use friendslop::AppState;
use friendslop::areas::{self, AREAS, AreaRoot, Force, GLOBALS, InArea, Layer, Roots};

/// What "spawns into the world on entering the game" looks like in source.
const AT_START: &str = "OnEnter(AppState::InGame)";

/// The ratchet: every file that still spawns on entering the game and
/// hasn't moved into an area yet. It may only shrink. Moving a place into
/// its area (spawning in `areas::loaded(..)` systems under its root) means
/// taking its files off this list; a new file that spawns at game start
/// fails here: spawn it when its area loads instead.
const NOT_YET_IN_AN_AREA: &[&str] = &[
    // Commune.
    "crates/base/src/features/bear.rs",
    "crates/base/src/features/crows.rs",
    "crates/base/src/features/dave_commune.rs",
    "crates/base/src/features/great_storm.rs",
    "crates/game/src/scenery.rs",
    "crates/game/src/world.rs",
    "crates/game/src/zones.rs",
    "crates/world/src/holes.rs",
    "crates/world/src/landmarks.rs",
    "crates/world/src/terrain.rs",
    "crates/places/src/features/fart.rs",
    "crates/places/src/features/fart_sound.rs",
    "crates/places/src/features/helicopter.rs",
    "crates/places/src/features/skills.rs",
    "crates/places/src/features/smoking.rs",
    "crates/places/src/features/weed.rs",
    "crates/play/src/features/climb.rs",
    "crates/play/src/features/duel.rs",
    "crates/side/src/features/affair.rs",
    "crates/side/src/features/ambience.rs",
    "crates/side/src/features/broadcast.rs",
    "crates/side/src/features/clues.rs",
    "crates/side/src/features/goat.rs",
    "crates/side/src/features/kori.rs",
    "crates/side/src/features/lily_garden.rs",
    "crates/side/src/features/sesbian_lex.rs",
    "crates/side/src/features/shipwreck.rs",
    "crates/side/src/features/surf.rs",
    "crates/story/src/features/abuela.rs",
    "crates/story/src/features/gamble.rs",
    "crates/story/src/features/gods_tears.rs",
    "crates/story/src/features/halt.rs",
    "crates/story/src/features/landlord.rs",
    "crates/story/src/features/mushroom.rs",
    "crates/story/src/features/pets.rs",
    "crates/story/src/features/totems.rs",
    "crates/village/src/features/hide_and_seek.rs",
    "crates/village/src/features/organ_currency.rs",
    "crates/village/src/features/punishment.rs",
    "crates/village/src/features/wendys.rs",
    // New York (Penn, Fulton and Miami stacked on it).
    "crates/side-nyc/src/features/boba.rs",
    "crates/side-nyc/src/features/fulton.rs",
    "crates/side-nyc/src/features/fulton_hunt.rs",
    "crates/side-nyc/src/features/hats.rs",
    "crates/side-nyc/src/features/high_places.rs",
    "crates/side-nyc/src/features/ice_cream.rs",
    "crates/side-nyc/src/features/jokah.rs",
    "crates/side-nyc/src/features/miami.rs",
    "crates/side-nyc/src/features/nyc.rs",
    "crates/side-nyc/src/features/penn.rs",
    "crates/side-nyc/src/features/trains.rs",
    "crates/side-nyc/src/features/vessel.rs",
    "crates/side-nyc/src/features/vessel_jump.rs",
    "crates/side-nyc/src/features/wallet.rs",
    // Interiors.
    "crates/places/src/features/underground/visuals.rs",
    "crates/places/src/features/warehouse/visuals.rs",
    // Off-map places.
    "crates/afterlife/src/features/afterlife.rs",
    "crates/afterlife/src/features/mute_spell.rs",
    "crates/side/src/features/infinite.rs",
];

fn spawns_at_start() -> Vec<String> {
    let mut files: Vec<String> = super::place_looks::sources()
        .into_iter()
        .filter(|(_, text)| text.contains(AT_START))
        .map(|(path, _)| path)
        .collect();
    files.sort();
    files
}

#[test]
fn only_globals_and_the_ratchet_list_spawn_at_game_start() {
    let now = spawns_at_start();
    let global = |p: &str| GLOBALS.iter().any(|(g, _)| *g == p);
    for path in &now {
        assert!(
            global(path) || NOT_YET_IN_AN_AREA.contains(&path.as_str()),
            "{path} spawns on entering the game: spawn it when its area loads \
             (`areas::loaded(Layer::.., \"<area>\")`, under `areas::Roots`), see areas.rs"
        );
    }
    for path in NOT_YET_IN_AN_AREA {
        assert!(
            now.contains(&path.to_string()),
            "{path} no longer spawns at game start: take it off NOT_YET_IN_AN_AREA"
        );
        assert!(!global(path), "{path} is both a global and on the ratchet");
    }
    for (path, _) in GLOBALS {
        assert!(
            now.contains(&path.to_string()),
            "the global {path} doesn't spawn at game start: take it off areas::GLOBALS"
        );
    }
}

fn load_data() {
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let files: [&dyn friendslop_data::DataFile; 3] = [
        &AREAS,
        &friendslop::sites::SITES,
        &friendslop_world::places::COMMUNE,
    ];
    for file in files {
        let path = file.path();
        let Ok(bytes) = std::fs::read(assets.join(path)) else {
            panic!("missing data file {path}");
        };
        assert!(file.set_bytes(&bytes), "{path} doesn't parse");
    }
}

#[test]
fn every_site_names_an_area_that_exists() {
    load_data();
    let areas = areas::areas();
    for site in &friendslop::sites::sites().sites {
        assert!(
            areas.areas.iter().any(|a| a.id == site.area),
            "site {:?} is in the area {:?}, which data/base/areas.ron doesn't have",
            site.id,
            site.area
        );
    }
}

/// What an area may not keep once unloaded.
type Heavy = Or<(
    With<Mesh3d>,
    With<Collider>,
    With<PointLight>,
    With<SpotLight>,
    With<DirectionalLight>,
)>;

#[test]
fn unloading_every_area_leaves_nothing_of_theirs() {
    load_data();
    let mut app = super::harness::peer();
    app.init_state::<AppState>();
    app.insert_resource(Force(Some(true)));
    app.world_mut()
        .resource_mut::<NextState<AppState>>()
        .set(AppState::InGame);
    app.update();
    app.update();
    // Stand-ins for a migrated place: one under a root, one tagged.
    let Some(root) = app.world().resource::<Roots>().get("nyc", Layer::Show) else {
        panic!("forcing every area loaded didn't give New York a root");
    };
    app.world_mut()
        .spawn((Mesh3d::default(), PointLight::default(), ChildOf(root)));
    app.world_mut()
        .spawn((Collider::sphere(1.0), InArea::sim("commune")));
    app.update();

    app.insert_resource(Force(Some(false)));
    app.update();
    app.update();
    assert_eq!(app.world().resource::<Roots>().iter().count(), 0);
    let world = app.world_mut();
    let mut left = world.query_filtered::<Entity, Heavy>();
    let mut parents = world.query::<&ChildOf>();
    let mut tags = world.query::<(Option<&InArea>, Option<&AreaRoot>)>();
    let left: Vec<Entity> = left.iter(world).collect();
    for e in left {
        // Walk up: nothing heavy may still belong to an area.
        let mut at = e;
        loop {
            if let Ok((tag, root)) = tags.get(world, at) {
                assert!(
                    tag.is_none() && root.is_none(),
                    "{e:?} still belongs to an area after every area unloaded"
                );
            }
            let Ok(parent) = parents.get(world, at) else {
                break;
            };
            at = parent.parent();
        }
    }
    let mut owned = world.query_filtered::<(), Or<(With<InArea>, With<AreaRoot>)>>();
    assert_eq!(owned.iter(world).count(), 0, "areas left entities behind");
}
