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
const NOT_YET_IN_AN_AREA: &[&str] = &[];

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
    let files: [&dyn friendslop_data::DataFile; 5] = [
        &AREAS,
        &friendslop_world::wastes::TUNING,
        &friendslop::sites::SITES,
        &friendslop_world::places::COMMUNE,
        &friendslop::features::nyc::MAP,
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

/// How many colliders `app` holds that belong to `area`.
fn colliders_in(app: &mut App, area: &str) -> usize {
    let world = app.world_mut();
    let mut q = world.query_filtered::<Entity, With<Collider>>();
    let mut parents = world.query::<&ChildOf>();
    let mut tags = world.query::<(Option<&InArea>, Option<&AreaRoot>)>();
    let all: Vec<Entity> = q.iter(world).collect();
    all.into_iter()
        .filter(|&e| {
            let mut at = e;
            loop {
                if let Ok((tag, root)) = tags.get(world, at) {
                    if let Some(t) = tag {
                        return t.area == area;
                    }
                    if let Some(r) = root {
                        return r.area == area;
                    }
                }
                let Ok(p) = parents.get(world, at) else {
                    return false;
                };
                at = p.parent();
            }
        })
        .count()
}

fn has(app: &App, area: &str, layer: Layer) -> bool {
    app.world().resource::<Roots>().get(area, layer).is_some()
}

/// Put player `id`'s body at `at` (on the server: it replicates).
fn stand(sim: &mut super::harness::Sim, id: u64, at: Vec3) {
    let world = sim.server.world_mut();
    let mut q = world.query::<(
        &friendslop::player::Player,
        &mut friendslop::features::fps::Walker,
    )>();
    for (p, mut w) in q.iter_mut(world) {
        if p.id == id {
            w.pos = at;
        }
    }
}

/// The per-area checklist's two-player test, for New York: a host in the
/// commune keeps the city's colliders while a friend is in it, without its
/// visuals; the friend has both and not the commune's visuals; once the
/// friend leaves, the host lets the city go.
#[test]
fn a_host_elsewhere_keeps_new_york_s_colliders_while_a_friend_is_inside() {
    load_data();
    let mut sim = super::harness::Sim::new(1);
    let commune = Vec3::new(0.0, 0.0, 0.0);
    let city = Vec3::new(0.0, friendslop::rules::nyc::CITY_Y, 223.0);
    for app in std::iter::once(&mut sim.server).chain(sim.clients.iter_mut()) {
        app.init_state::<AppState>();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InGame);
    }
    // Each peer's "you" (`place::Here`, written by the game's own plugins).
    sim.server.insert_resource(friendslop::place::Here(commune));
    sim.clients[0].insert_resource(friendslop::place::Here(city));
    stand(&mut sim, 0, commune);
    stand(&mut sim, 1, city);
    sim.run(0.2);

    // The host: New York's colliders for the friend, none of its visuals.
    assert!(
        has(&sim.server, "nyc", Layer::Sim),
        "the host dropped New York's colliders"
    );
    assert!(
        !has(&sim.server, "nyc", Layer::Show),
        "the host draws New York from the commune"
    );
    assert!(has(&sim.server, "commune", Layer::Show));
    assert!(
        colliders_in(&mut sim.server, "nyc") > 100,
        "no city floor and walls on the host"
    );
    // The friend: New York, both layers, and nothing of the commune's visuals.
    let client = &mut sim.clients[0];
    assert!(has(client, "nyc", Layer::Sim) && has(client, "nyc", Layer::Show));
    assert!(
        !has(client, "commune", Layer::Show),
        "the friend draws the commune from New York"
    );
    assert!(
        colliders_in(client, "nyc") > 100,
        "the friend walks New York without its floor"
    );

    // The friend takes the train home: the host lets New York go.
    sim.clients[0].insert_resource(friendslop::place::Here(commune));
    stand(&mut sim, 1, commune);
    sim.run(0.2);
    assert!(!has(&sim.server, "nyc", Layer::Sim));
    assert_eq!(colliders_in(&mut sim.server, "nyc"), 0);
    assert_eq!(colliders_in(&mut sim.clients[0], "nyc"), 0);
}

/// Start a host and one friend in the game, the host at `home` and the
/// friend at `away` (each peer's "you" and their bodies).
fn two_players(home: Vec3, away: Vec3) -> super::harness::Sim {
    let mut sim = super::harness::Sim::new(1);
    for app in std::iter::once(&mut sim.server).chain(sim.clients.iter_mut()) {
        app.init_state::<AppState>();
        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::InGame);
    }
    sim.server.insert_resource(friendslop::place::Here(home));
    sim.clients[0].insert_resource(friendslop::place::Here(away));
    stand(&mut sim, 0, home);
    stand(&mut sim, 1, away);
    sim
}

/// The per-area checklist's two-player test for a place stacked over or
/// under the commune: a host up top keeps its colliders (and none of its
/// visuals) while the friend is in it; the friend has both; once the friend
/// is back up, neither keeps any of it.
fn a_host_up_top_keeps_the_colliders_of(
    area: &str,
    inside: Vec3,
    prepare: fn(&mut super::harness::Sim),
) {
    load_data();
    let home = Vec3::new(0.0, friendslop::terrain::height_at(0.0, 0.0), 0.0);
    let mut sim = two_players(home, inside);
    prepare(&mut sim);
    sim.run(0.2);
    assert!(
        has(&sim.server, area, Layer::Sim),
        "the host dropped {area}'s colliders"
    );
    assert!(
        !has(&sim.server, area, Layer::Show),
        "the host draws {area} from up top"
    );
    assert!(
        colliders_in(&mut sim.server, area) > 0,
        "no {area} colliders on the host"
    );
    let client = &mut sim.clients[0];
    assert!(has(client, area, Layer::Sim) && has(client, area, Layer::Show));
    assert!(
        colliders_in(client, area) > 0,
        "the friend walks {area} without its walls"
    );

    sim.clients[0].insert_resource(friendslop::place::Here(home));
    stand(&mut sim, 1, home);
    sim.run(0.2);
    for app in std::iter::once(&mut sim.server).chain(sim.clients.iter_mut()) {
        assert!(!has(app, area, Layer::Sim) && !has(app, area, Layer::Show));
        assert_eq!(colliders_in(app, area), 0, "{area} left colliders behind");
    }
}

#[test]
fn a_host_up_top_keeps_the_warehouse_s_colliders_while_a_friend_is_inside() {
    a_host_up_top_keeps_the_colliders_of(
        "warehouse",
        friendslop::features::warehouse::in_spot(0),
        |sim| {
            // An expedition is on: everyone builds the same inside.
            let world = sim.server.world_mut();
            let mut q = world.query::<&mut friendslop::features::warehouse::Warehouse>();
            for mut w in q.iter_mut(world) {
                w.seed = Some(7);
            }
        },
    );
}

#[test]
fn a_host_up_top_keeps_the_tunnels_colliders_while_a_friend_is_down() {
    a_host_up_top_keeps_the_colliders_of(
        "underground",
        friendslop::features::underground::down_spot(0),
        |sim| {
            for app in std::iter::once(&mut sim.server).chain(sim.clients.iter_mut()) {
                app.insert_resource(friendslop::features::world_seed::WorldSeed(1));
            }
        },
    );
}

#[test]
fn a_host_up_top_keeps_hell_s_colliders_while_a_friend_is_in_it() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(friendslop_data::DataFile::path(
            &friendslop::features::afterlife::TUNING,
        ));
    let Ok(bytes) = std::fs::read(&path) else {
        panic!("missing {path:?}");
    };
    assert!(friendslop_data::DataFile::set_bytes(
        &friendslop::features::afterlife::TUNING,
        &bytes
    ));
    let Some(stone) = friendslop::features::afterlife::hell_stones()
        .first()
        .copied()
    else {
        panic!("hell has no stones");
    };
    a_host_up_top_keeps_the_colliders_of("afterlife", stone.top, |_| {});
}

#[test]
fn a_host_down_here_keeps_the_sky_courses_colliders_while_a_friend_is_up_there() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(friendslop_data::DataFile::path(
            &friendslop::features::infinite::TUNING,
        ));
    let Ok(bytes) = std::fs::read(&path) else {
        panic!("missing {path:?}");
    };
    assert!(friendslop_data::DataFile::set_bytes(
        &friendslop::features::infinite::TUNING,
        &bytes
    ));
    let plan = friendslop::features::infinite::Plan::seeded(1);
    let Some(space) = plan.spaces.first() else {
        panic!("no sky courses");
    };
    a_host_up_top_keeps_the_colliders_of("infinite", space.origin, |sim| {
        for app in std::iter::once(&mut sim.server).chain(sim.clients.iter_mut()) {
            app.insert_resource(friendslop::features::infinite::Plan::seeded(1));
        }
    });
}

/// The commune's two-player test: a host in New York keeps the commune's
/// colliders (ground included) for a friend at home, without its visuals;
/// the friend has both. Once the friend is in New York too, nobody keeps any
/// of the commune.
#[test]
fn a_host_in_new_york_keeps_the_commune_s_colliders_while_a_friend_is_home() {
    load_data();
    let home = Vec3::new(0.0, friendslop::terrain::height_at(0.0, 0.0), 0.0);
    let city = Vec3::new(0.0, friendslop::rules::nyc::CITY_Y, 223.0);
    let mut sim = two_players(city, home);
    sim.run(0.2);
    assert!(
        has(&sim.server, "commune", Layer::Sim),
        "the host dropped the commune's colliders"
    );
    assert!(
        !has(&sim.server, "commune", Layer::Show),
        "the host draws the commune from New York"
    );
    assert!(
        colliders_in(&mut sim.server, "commune") > 0,
        "no commune ground on the host"
    );
    let client = &mut sim.clients[0];
    assert!(has(client, "commune", Layer::Sim) && has(client, "commune", Layer::Show));
    assert!(
        colliders_in(client, "commune") > 0,
        "the friend has no ground"
    );

    sim.clients[0].insert_resource(friendslop::place::Here(city));
    stand(&mut sim, 1, city);
    sim.run(0.2);
    for app in std::iter::once(&mut sim.server).chain(sim.clients.iter_mut()) {
        assert!(!has(app, "commune", Layer::Sim) && !has(app, "commune", Layer::Show));
        assert_eq!(
            colliders_in(app, "commune"),
            0,
            "the commune left colliders"
        );
    }
}

/// Everyone in a place stacked under the commune (the tunnels): the commune
/// unloads, though the tunnels are inside its box.
#[test]
fn the_commune_unloads_while_everyone_is_in_the_tunnels() {
    load_data();
    let down = Vec3::new(0.0, -30.0, 0.0);
    let mut sim = two_players(down, down);
    sim.run(0.2);
    for app in std::iter::once(&mut sim.server).chain(sim.clients.iter_mut()) {
        assert!(!has(app, "commune", Layer::Sim) && !has(app, "commune", Layer::Show));
        assert_eq!(colliders_in(app, "commune"), 0);
    }
}

/// The wastes' chunks around the friend: their colliders' (x, z).
fn waste_colliders(app: &mut App) -> Vec<Vec2> {
    let world = app.world_mut();
    let mut q = world
        .query_filtered::<&Transform, (With<friendslop_world::wastes::WasteChunk>, With<Collider>)>(
        );
    q.iter(world).map(|t| t.translation.xz()).collect()
}

/// The wastes stream like an area's layers: a host at home keeps the
/// colliders of the chunks around a friend out in the wastes, and lets
/// them go once the friend is back.
#[test]
fn a_host_at_home_keeps_the_wastes_under_a_friend_out_there() {
    load_data();
    let home = Vec3::new(0.0, friendslop::terrain::height_at(0.0, 0.0), 0.0);
    let out = Vec3::new(0.0, friendslop::terrain::height_at(0.0, -900.0), -900.0);
    let mut sim = two_players(home, out);
    // (The harness has only the protocol: add the wastes' streaming.)
    for app in std::iter::once(&mut sim.server).chain(sim.clients.iter_mut()) {
        bevy::app::Plugin::build(&friendslop_world::wastes::WastesPlugin, app);
        app.add_systems(
            Update,
            friendslop::features::wastes::watch.before(friendslop_world::wastes::StreamSet),
        );
    }
    sim.run(1.0);
    let near = |v: &Vec2| v.distance(out.xz()) < 200.0;
    assert!(
        waste_colliders(&mut sim.server).iter().any(near),
        "the host dropped the ground under the friend"
    );
    assert!(
        waste_colliders(&mut sim.clients[0]).iter().any(near),
        "the friend walks the wastes without ground"
    );
    sim.clients[0].insert_resource(friendslop::place::Here(home));
    stand(&mut sim, 1, home);
    sim.run(0.5);
    for app in std::iter::once(&mut sim.server).chain(sim.clients.iter_mut()) {
        assert!(
            !waste_colliders(app).iter().any(near),
            "chunks stayed with nobody near"
        );
    }
}

/// The plan's done condition, headless: after a game has run, everything
/// in the world is in an area, a wastes chunk, replicated, or a global (the
/// census's "no area" line is empty).
#[test]
fn nothing_in_the_world_is_in_no_area() {
    load_data();
    let home = Vec3::new(0.0, friendslop::terrain::height_at(0.0, 0.0), 0.0);
    let mut sim = two_players(home, home);
    sim.run(1.0);
    for app in std::iter::once(&mut sim.server).chain(sim.clients.iter_mut()) {
        let world = app.world_mut();
        let mut state = bevy::ecs::system::SystemState::<(
            Query<(Entity, Option<&Name>)>,
            Query<&ChildOf>,
            Query<areas::Tags>,
        )>::new(world);
        let Ok((all, parents, tags)) = state.get(world) else {
            panic!("census query")
        };
        let stray: Vec<String> = all
            .iter()
            .filter(|(e, _)| areas::owner(*e, &parents, &tags) == Some(None))
            .map(|(e, n)| n.map_or(format!("{e}"), |n| n.to_string()))
            .collect();
        let stray: Vec<String> = stray
            .into_iter()
            .zip(
                all.iter()
                    .filter(|(e, _)| areas::owner(*e, &parents, &tags) == Some(None)),
            )
            .map(|(s, (e, _))| {
                let cs: Vec<String> = world
                    .inspect_entity(e)
                    .map(|c| c.map(|c| c.1.name().to_string()).collect())
                    .unwrap_or_default();
                format!("{s} {cs:?}")
            })
            .collect();
        assert!(stray.is_empty(), "in no area: {stray:?}");
    }
}
