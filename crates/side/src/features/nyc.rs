//! New York City. From issue #283 by doomy: "extending the forest a bit if
//! you go far south youll make out WTC one and the path station oculus.
//! right now the train station isnt functional but in the future it might go
//! places. in new york city make sure it really looks like new york and have
//! at least 4 enterable buildings in addition to the train station".
//!
//! Beyond the valley's south rim hills lies a flat city floor
//! ([`crate::rules::nyc`]). The forest carries on over the rim for a stretch,
//! then the city starts: a wide avenue with cross streets, sidewalks, yellow
//! cabs, street lamps, hydrants, brick walk-ups and glass towers, One World
//! Trade Center at the south end (541 m to the top of its spire, so you can
//! make it out over the rim hills from the valley's south edge) and the
//! white ribbed wings of the Oculus beside it, a train station you can walk
//! into but whose trains aren't running yet.
//!
//! The four enterable shops ([`SHOPS`]): a bodega (with a cat), a 99¢ pizza
//! counter, a laundromat and a diner. Step inside one and its line shows.
//!
//! The way there: a trail post at the south edge of the valley ("New York
//! City, this way"). Click it and the server walks you over the rim to the
//! city; click the post at the city's north end to come back. Everything you
//! see is local scenery, the same on every peer; the server only moves
//! bodies.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{WalkInput, Walker};
use crate::features::interact::{Clickable, Clicked, SERVER_REACH, deny, within};
use crate::player::{LocalPlayer, Player, PlayerOwner};
pub use crate::rules::nyc::*;

/// Tuning and writing: `assets/data/side/nyc.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The trail post at the valley's south edge (x, z).
    pub trail_post: (f32, f32),
    /// Where you arrive in the city (x, z); the way back post stands beside it.
    pub arrival: (f32, f32),
    /// Where you come back to in the valley (x, z), beside the trail post.
    pub return_to: (f32, f32),
    /// The shops' names and the line shown when you step in, in [`SHOPS`] order.
    pub shops: Vec<(String, String)>,
    /// Shown inside the Oculus.
    pub oculus_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/nyc.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

fn v2((x, z): (f32, f32)) -> Vec2 {
    Vec2::new(x, z)
}

/// Client → server: walk over the rim (to the city) or back.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Travel {
    pub to_city: bool,
}

pub fn protocol(app: &mut App) {
    app.add_client_event::<Travel>(Channel::Ordered)
        .add_observer(travel);
    // Keep the valley's trees off the trail post.
    friendslop_world::footprints::claim(v2(tuning().trail_post), 4.0);
}

/// Where a traveller from `pos` ends up, or `None` if they aren't at a post.
pub fn destination(pos: Vec3, to_city: bool, t: &Tuning) -> Option<Vec3> {
    let (post, to) = if to_city {
        (v2(t.trail_post), v2(t.arrival))
    } else {
        (v2(t.arrival), v2(t.return_to))
    };
    if !within(pos, post.extend(0.0).xzy(), SERVER_REACH + 1.5) {
        return None;
    }
    let y = if to_city {
        CITY_Y
    } else {
        crate::terrain::height_at(to.x, to.y)
    };
    Some(Vec3::new(to.x, y + 0.05, to.y))
}

fn travel(
    ev: On<FromClient<Travel>>,
    mut commands: Commands,
    mut bodies: Query<(
        &PlayerOwner,
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((_, mut walker, input, kcc)) = bodies.iter_mut().find(|b| b.0.0 == owner) else {
        return;
    };
    let Some(to) = destination(walker.pos, ev.to_city, &tuning()) else {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    };
    walker.pos = to;
    if let Some(mut input) = input {
        input.stick = Vec2::ZERO;
        input.jump = false;
    }
    if let Some(mut kcc) = kcc {
        kcc.velocity = Vec3::ZERO;
    }
}

// ------------------------------------------------------------------ layout

/// The city as the modeler lays it out: the `nyc-map` document, exported to
/// `assets/maps/nyc.bsn` (the models) and `nyc.map.json` (this data: placed
/// models, `Block` collider boxes and `room_*` spots). Read at run time on every
/// peer, so the colliders and rooms agree everywhere. Change the city in the
/// modeler and re-export; nothing here holds positions.
pub static MAP: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/nyc.map.json");

/// A shop you can walk into: the floor rectangle (x, z) inside its walls.
#[derive(Clone, Copy, Debug)]
pub struct Room {
    pub min: Vec2,
    pub max: Vec2,
}

impl Room {
    pub fn contains(&self, p: Vec2) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }
}

/// One static collider box from the map: centre, size and turn about +y.
#[derive(Clone, Copy, Debug)]
pub struct Wall {
    pub centre: Vec3,
    pub size: Vec3,
    pub yaw: f32,
}

/// What the game needs from the city map.
#[derive(Default)]
pub struct City {
    pub walls: Vec<Wall>,
    /// The four shops, in [`SHOPS`] order (missing ones are empty rooms).
    pub rooms: Vec<Room>,
    /// The Oculus hall's floor.
    pub oculus: Option<Room>,
}

/// The shops, by kind (names and lines are in the data file).
pub const SHOPS: [&str; 4] = ["bodega", "pizza", "laundromat", "diner"];

/// The room a `room_<name>` spot marks, if the map has it.
fn room(map: &friendslop_world::places::MapFile, name: &str) -> Option<Room> {
    let spot = format!("room_{name}");
    let e = map.entities.iter().find(|e| e.name == spot)?;
    let half = e.footprint() / 2.0;
    Some(Room {
        min: e.at() - half,
        max: e.at() + half,
    })
}

/// Read the city out of its map file.
pub fn city(map: &friendslop_world::places::MapFile) -> City {
    let walls = map
        .entities
        .iter()
        .filter(|e| e.has("Block"))
        .map(|e| Wall {
            centre: Vec3::from_array(e.translation),
            size: e.size.map_or(Vec3::ONE, Vec3::from_array),
            yaw: e.yaw.to_radians(),
        })
        .collect();
    let empty = Room {
        min: Vec2::splat(f32::MAX),
        max: Vec2::splat(f32::MIN),
    };
    City {
        walls,
        rooms: SHOPS
            .iter()
            .map(|s| room(map, s).unwrap_or(empty))
            .collect(),
        oculus: room(map, "oculus"),
    }
}

/// The city map's root. Its models (streets, towers, cabs, trees) are drawn only on screens
/// near the city; the colliders are separate and stay for everyone. One World Trade Center
/// is always drawn: it stays on the skyline from the valley.
#[derive(Component)]
struct NycMap;

/// How far outside the city floor (metres) you still see its streets.
const CITY_DRAW_MARGIN: f32 = 25.0;

/// Should the city's streets be drawn for a camera at (x, z)?
pub fn city_drawn(x: f32, z: f32) -> bool {
    let (min, max) = (
        crate::rules::nyc::CITY_MIN - Vec2::splat(CITY_DRAW_MARGIN),
        crate::rules::nyc::CITY_MAX + Vec2::splat(CITY_DRAW_MARGIN),
    );
    (min.x..=max.x).contains(&x) && (min.y..=max.y).contains(&z)
}

/// Local: show the city's streets only while this screen's camera is near the city.
fn cull_city(
    mut commands: Commands,
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    maps: Query<&Children, With<NycMap>>,
    mut parts: Query<(Option<&mut Visibility>, &friendslop_world::map::GltfSource)>,
) {
    let Some(at) = camera.iter().next().map(|g| g.translation()) else {
        return;
    };
    let want = city_drawn(at.x, at.z);
    let vis = if want {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for child in maps.iter().flatten() {
        let Ok((v, gltf)) = parts.get_mut(*child) else {
            continue; // a collider or room marker: nothing to draw
        };
        let want = if gltf.path.contains("nyc-wtc") {
            Visibility::Inherited
        } else {
            vis
        };
        match v {
            Some(mut v) => {
                v.set_if_neq(want);
            }
            None => {
                commands.entity(*child).insert(want);
            }
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_observer(click)
        .add_systems(OnEnter(AppState::InGame), build_city)
        .add_systems(
            Update,
            (where_am_i, cull_city).run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct Post {
    to_city: bool,
}

fn click(ev: On<Clicked>, mut commands: Commands, posts: Query<&Post>) {
    if let Ok(p) = posts.get(ev.entity) {
        commands.client_trigger(Travel { to_city: p.to_city });
    }
}

fn build_city(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    let t = tuning();
    let c = city(&MAP.get());
    // The models, all from the modeler map. SALT: not needing `Cargo.lock` for this
    // `.bsn` either; x didn't set us up for success, we need more SME's, we haven't
    // considered the problem space enough, BSN syntax needs more changes, and
    // `Children` sugaring? Ask Sam "Salt" Saltman.
    commands.spawn((
        friendslop_world::map::Map(assets.load("maps/nyc.bsn")),
        NycMap,
        Transform::default(),
        Visibility::default(),
        DespawnOnExit(AppState::InGame),
    ));
    // Colliders never wait on a model: mesh-less boxes from the map, and the city floor.
    let mid_z = (CITY_MIN.y + CITY_MAX.y) / 2.0;
    commands.spawn((
        Transform::from_xyz(0.0, CITY_Y - 0.5, mid_z),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::Collider::cuboid(CITY_MAX.x - CITY_MIN.x, 1.0, CITY_MAX.y - CITY_MIN.y),
        DespawnOnExit(AppState::InGame),
    ));
    for w in &c.walls {
        commands.spawn((
            Transform::from_translation(w.centre).with_rotation(Quat::from_rotation_y(w.yaw)),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(w.size.x, w.size.y, w.size.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let wood = materials.add(Color::srgb(0.45, 0.3, 0.18));
    let sign = materials.add(Color::srgb(0.1, 0.35, 0.2));
    // The trail posts: one at the valley's south edge, one in the city.
    for (at, to_city, label) in [
        (v2(t.trail_post), true, "walk to New York City"),
        (
            v2(t.arrival) + Vec2::new(2.5, -2.0),
            false,
            "walk back to the valley",
        ),
    ] {
        let y = if to_city {
            crate::terrain::height_at(at.x, at.y)
        } else {
            CITY_Y
        };
        commands
            .spawn((
                Post { to_city },
                Transform::from_xyz(at.x, y, at.y),
                Visibility::default(),
                Clickable::at_height(1.6).radius(60.0).labelled(label),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(0.0, 1.1, 0.0).with_scale(Vec3::new(0.15, 2.2, 0.15)),
                ));
                p.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(sign.clone()),
                    Transform::from_xyz(0.0, 1.9, 0.0).with_scale(Vec3::new(1.4, 0.4, 0.06)),
                ));
            });
    }
    commands.insert_resource(Rooms(c));
}

#[derive(Resource)]
struct Rooms(City);

/// Local: step into a shop (or the Oculus) and its line shows.
fn where_am_i(
    me: Option<Res<LocalPlayer>>,
    rooms: Option<Res<Rooms>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut inside: Local<Option<usize>>,
) {
    let (Some(me), Some(rooms), Some(mut toast)) = (me, rooms, toast) else {
        return;
    };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let here = room_at(&rooms.0, w.pos.xz());
    if here != *inside {
        *inside = here;
        let t = tuning();
        if let Some(i) = here {
            let line = if i < SHOPS.len() {
                t.shops.get(i).map(|(n, l)| format!("{n}: {l}"))
            } else {
                Some(t.oculus_line.clone())
            };
            if let Some(line) = line {
                toast.show(line);
            }
        }
    }
}

/// Which shop (index into [`SHOPS`]) or the Oculus (`SHOPS.len()`) `p` is in.
pub fn room_at(c: &City, p: Vec2) -> Option<usize> {
    c.rooms.iter().position(|r| r.contains(p)).or_else(|| {
        c.oculus
            .is_some_and(|o| o.contains(p))
            .then_some(SHOPS.len())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_city_is_drawn_only_near_it() {
        let mid = (crate::rules::nyc::CITY_MIN + crate::rules::nyc::CITY_MAX) / 2.0;
        assert!(city_drawn(mid.x, mid.y));
        // Just over the rim, looking in: still drawn.
        assert!(city_drawn(0.0, crate::rules::nyc::CITY_MIN.y - 10.0));
        // The commune, far up the valley: not drawn.
        assert!(!city_drawn(0.0, 0.0));
    }

    fn t() -> Tuning {
        Tuning {
            trail_post: (10.0, 86.0),
            arrival: (0.0, 125.0),
            return_to: (10.0, 83.0),
            ..default()
        }
    }

    #[test]
    fn the_posts_take_you_there_and_back() {
        let t = t();
        let there = destination(Vec3::new(10.0, 0.0, 86.5), true, &t).unwrap_or_default();
        assert!(in_city(there.x, there.z));
        let back = destination(there, false, &t).unwrap_or_default();
        assert!(!in_city(back.x, back.z));
        assert!(destination(Vec3::ZERO, true, &t).is_none());
    }

    fn map() -> friendslop_world::places::MapFile {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/maps/nyc.map.json"
        );
        let Ok(text) = std::fs::read_to_string(path) else {
            panic!("no {path}");
        };
        let Ok(map) = serde_json::from_str(&text) else {
            panic!("{path} doesn't parse");
        };
        map
    }

    #[test]
    fn four_shops_and_a_station_all_in_the_city() {
        let c = city(&map());
        assert_eq!(c.rooms.len(), 4);
        let Some(oculus) = c.oculus else {
            panic!("the map has no room_oculus");
        };
        for r in c.rooms.iter().chain([&oculus]) {
            assert!(
                in_city(r.min.x, r.min.y) && in_city(r.max.x, r.max.y),
                "{r:?}"
            );
            assert!(r.max.x - r.min.x > 4.0 && r.max.y - r.min.y > 4.0, "{r:?}");
        }
        for (i, r) in c.rooms.iter().enumerate() {
            assert_eq!(room_at(&c, (r.min + r.max) / 2.0), Some(i));
        }
    }

    #[test]
    fn the_city_is_walled_and_every_model_is_from_the_modeler() {
        let m = map();
        assert!(city(&m).walls.len() > 100);
        let models: Vec<_> = m
            .entities
            .iter()
            .filter_map(|e| e.gltf.as_deref())
            .collect();
        assert!(models.iter().any(|g| g.contains("nyc-wtc")));
        assert!(
            models.iter().all(|g| g.starts_with("models/prefabs/nyc-")),
            "{models:?}"
        );
    }

    #[test]
    fn the_arrival_and_the_avenue_are_clear() {
        let c = city(&map());
        let t = t();
        for w in &c.walls {
            let half = w.size.xz().length() / 2.0;
            let d = w.centre.xz().distance(v2(t.arrival));
            assert!(d > half + 1.0, "a wall on the arrival: {w:?}");
            // The avenue's roadway (|x| < 8) has nothing taller than a cab on it.
            if w.centre.x.abs() < 8.0 && w.centre.z > 150.0 {
                assert!(w.size.y < 2.0, "{w:?}");
            }
        }
    }
}
