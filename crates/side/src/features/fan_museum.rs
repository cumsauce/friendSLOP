//! The Ceiling Fan Museum: a big-box store on the commune's north-east
//! outskirts full of ceiling fans, every one of them spinning: ordinary ones
//! from 2 to 12 blades, weird ones hung low over roped-off exhibits, a Wall of
//! Fans where (invented) ceiling-fan celebrities signed beside their favourite
//! fan, and a west annex of whole-room installations (one giant fan behind a
//! grate, and a Möbius strip covered in desk fans).
//!
//! The whole place is a modeler map, `cfm-museum` (`maps/fan_museum.bsn` for
//! the models, `maps/fan_museum.map.json` for this code), laid out around the
//! museum's floor centre: every model is a prefab instance, every rotor carries
//! a `Spin { rpm }`, and the map's markers say the rest: `Block` boxes are the
//! colliders (so nothing waits on a model file), `Lamp`s are its lights,
//! `room_*` boxes are indoors and the `door` spot is the way in. Change the
//! museum in the modeler and re-export; this file is only the wiring. Local
//! scenery, the same on every peer; the rotors turn while you're there.
//!
//! It's a load-in place, like the warehouse: in the open world only its
//! outside stands (`cfm-museum-exterior` on the commune map, with its own
//! `Block`s), and the inside is its own area stacked deep under the ground
//! (`data/base/areas.ron`), at `interior_floor` (`data/side/fan_museum.ron`).
//! Walk into the front doors and the server moves you inside (the loading
//! curtain covers it); walk back out of the lobby doors and you're on the
//! steps outside.

use bevy::prelude::*;
use friendslop_world::map::Spin;
use friendslop_world::places::{MapEntity, MapFile};

use crate::AppState;
use crate::areas;
use crate::hud::HudSlot;

pub use friendslop_world::places::fan_museum_at;

/// The museum's site id (`data/base/sites.ron`).
pub const SITE: &str = "fan_museum";
/// Its own area (`data/base/areas.ron`): loaded as you come near, gone when you leave.
pub const AREA: &str = "fan_museum";

/// The museum map's markers (read at run time, re-read when it's re-exported).
pub static MAP: friendslop_data::Data<MapFile> =
    friendslop_data::Data::json("maps/fan_museum.map.json");

/// Tuning: `assets/data/side/fan_museum.ron` (read at run time).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The interior's floor height (m): inside the museum area's height band, far under the ground.
    pub interior_floor: f32,
    /// Stand this close (m, across the ground) to a door and you go through it.
    pub door_reach: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/fan_museum.ron");

/// The front doors, from the museum's floor centre: where walking in (from
/// outside) and walking out (from the lobby) take you through, and where
/// you come out on the other side.
const OUTSIDE_DOOR: Vec3 = Vec3::new(0.0, 0.0, -14.3);
const OUTSIDE_STEP: Vec3 = Vec3::new(0.0, 0.1, -16.6);
const INSIDE_DOOR: Vec3 = Vec3::new(0.0, 0.0, -13.4);
const INSIDE_STEP: Vec3 = Vec3::new(0.0, 0.1, -11.4);

/// Going through the front doors: from `pos` near the outside doors (the
/// museum's outside floor centre `outside`) into the lobby (interior floor
/// centre `inside`), or from the lobby back out. Where you end up and which
/// way you face, if anywhere.
pub fn through_door(pos: Vec3, outside: Vec3, inside: Vec3, reach: f32) -> Option<(Vec3, f32)> {
    let near = |at: Vec3| {
        Vec2::new(pos.x - at.x, pos.z - at.z).length() < reach && (pos.y - at.y).abs() < 3.0
    };
    use crate::features::doorway::yaw_towards;
    if near(outside + OUTSIDE_DOOR) {
        Some((inside + INSIDE_STEP, yaw_towards(Vec2::Y)))
    } else if near(inside + INSIDE_DOOR) {
        Some((outside + OUTSIDE_STEP, yaw_towards(-Vec2::Y)))
    } else {
        None
    }
}

/// How far a rotor turns in `dt` seconds at `rpm` (radians).
pub fn turn(rpm: f32, dt: f32) -> f32 {
    rpm / 60.0 * std::f32::consts::TAU * dt
}

/// A box marker: centre and full size in map coordinates, turned by yaw (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Box3 {
    pub centre: Vec3,
    pub size: Vec3,
    pub yaw: f32,
    pub round: bool,
}

impl Box3 {
    fn of(e: &MapEntity) -> Self {
        Self {
            centre: Vec3::from_array(e.translation),
            size: e.size.map_or(Vec3::ONE, Vec3::from_array),
            yaw: e.yaw.to_radians(),
            round: e.flag("Block", "round"),
        }
    }

    /// Is `p` (map coordinates) inside it?
    pub fn contains(&self, p: Vec3) -> bool {
        let d = Quat::from_rotation_y(-self.yaw) * (p - self.centre);
        (d.abs() * 2.0).cmple(self.size).all()
    }
}

/// The museum as its map has it: colliders, indoor rooms, lights and the door.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub blocks: Vec<Box3>,
    pub rooms: Vec<Box3>,
    pub lamps: Vec<(Vec3, f32, f32)>,
    /// The front door's spot and the way you face walking in (yaw, radians).
    pub door: Option<(Vec3, f32)>,
}

/// Read the museum out of its map file.
pub fn layout(map: &MapFile) -> Layout {
    let mut l = Layout::default();
    for e in &map.entities {
        if e.has("Block") {
            l.blocks.push(Box3::of(e));
        }
        if e.name.starts_with("room_") && e.size.is_some() {
            l.rooms.push(Box3::of(e));
        }
        if e.has("Lamp") {
            l.lamps.push((
                Vec3::from_array(e.translation),
                e.num("Lamp", "lumens"),
                e.num("Lamp", "range"),
            ));
        }
        if e.name == "door" {
            l.door = Some((Vec3::from_array(e.translation), e.yaw.to_radians()));
        }
    }
    l
}

/// The outside floor's height: the highest ground under every room, so the
/// building never sinks into a slope.
pub fn outside_floor() -> f32 {
    let at = fan_museum_at();
    layout(&MAP.get())
        .rooms
        .iter()
        .flat_map(|r| {
            let h = r.size / 2.0;
            [
                (-1.0, -1.0),
                (1.0, -1.0),
                (-1.0, 1.0),
                (1.0, 1.0),
                (0.0, 0.0),
            ]
            .map(|(x, z)| r.centre + Vec3::new(x * h.x, 0.0, z * h.z))
        })
        .map(|p| crate::terrain::height_at(at.x + p.x, at.y + p.z))
        .fold(0.0, f32::max)
}

/// The interior's floor centre in the world: the map's origin, deep under
/// the museum's spot.
fn centre() -> Vec3 {
    let at = fan_museum_at();
    Vec3::new(at.x, TUNING.get().interior_floor, at.y)
}

/// The outside's floor centre in the world, by the front doors.
fn outside() -> Vec3 {
    let at = fan_museum_at();
    Vec3::new(at.x, outside_floor(), at.y)
}

/// Is `p` (world) inside the museum?
pub fn inside(p: Vec3) -> bool {
    let local = p - centre();
    layout(&MAP.get()).rooms.iter().any(|r| r.contains(local))
}

pub fn protocol(_app: &mut App) {
    // The building, its annex and its parking lot keep trees and rocks off.
    let at = fan_museum_at();
    for dx in (-5..=2).map(|i| i as f32 * 10.0) {
        for dz in [-20.0, -4.0, 8.0] {
            friendslop_world::footprints::claim(at + Vec2::new(dx, dz), 12.0);
        }
    }
}

pub fn plugin(app: &mut App) {
    use friendslop_base::areas::AreaApp as _;
    areas::on_sim(app, AREA, build_colliders);
    areas::on_sim(app, "commune", build_entrance);
    crate::features::start::provide(starts);
    areas::on_show(app, AREA, build_visuals);
    app.add_systems(
        Update,
        doors
            .run_if(in_state(AppState::InGame))
            .run_if(in_state(bevy_replicon::prelude::ServerState::Running))
            .run_if(crate::sites::awake(SITE)),
    );
    app.add_hud(spawn_help).add_systems(
        Update,
        (
            spin.run_if(crate::sites::here(SITE)),
            museum_look.in_set(crate::place::PlaceSet),
        )
            .run_if(in_state(AppState::InGame)),
    );
}

/// Named starts (`FRIENDSLOP_START=`): inside the lobby, and outside the doors.
fn starts() -> Vec<(String, Vec3)> {
    vec![
        ("fan_museum_inside".into(), centre() + INSIDE_STEP),
        ("fan_museum_door".into(), outside() + OUTSIDE_STEP),
    ]
}

/// Server: the front doors, both ways.
fn doors(
    mut bodies: Query<(
        &mut crate::features::fps::Walker,
        Option<&mut crate::features::fps::WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    let (outside, inside, reach) = (outside(), centre(), TUNING.get().door_reach);
    for (mut walker, mut input, mut kcc) in &mut bodies {
        let Some((to, yaw)) = through_door(walker.pos, outside, inside, reach) else {
            continue;
        };
        crate::features::fps::teleport(
            &mut walker,
            input.as_deref_mut(),
            kcc.as_deref_mut(),
            to,
            yaw,
        );
    }
}

/// Local: turn every rotor.
fn spin(time: Res<Time>, mut rotors: Query<(&Spin, &mut Transform)>) {
    let dt = time.delta_secs();
    for (s, mut t) in &mut rotors {
        t.rotate_local_y(turn(s.rpm, dt));
    }
}

/// Local: inside, the sky's flat ambient comes down (`place::INDOORS`).
fn museum_look(here: Res<crate::place::Here>, mut looks: ResMut<crate::place::PlaceLooks>) {
    if inside(here.0) {
        looks.push(crate::place::INDOORS);
    }
}

fn spawn_help(mut commands: Commands) {
    commands.spawn((
        Text::new(
            "The Ceiling Fan Museum: every fan you can think of, all spinning. The installations are through the west door.",
        ),
        crate::hud::slot(HudSlot::Help),
        crate::sites::AtSite(SITE),
        DespawnOnExit(AppState::InGame),
    ));
}

/// The map's `Block` boxes as invisible colliders, and the way in.
fn build_colliders(mut commands: Commands) {
    use avian3d::prelude::{Collider, RigidBody};
    let c = centre();
    let lay = layout(&MAP.get());
    for b in &lay.blocks {
        let collider = if b.round {
            Collider::cylinder(b.size.x / 2.0, b.size.y)
        } else {
            Collider::cuboid(b.size.x, b.size.y, b.size.z)
        };
        commands.spawn((
            Transform::from_translation(c + b.centre).with_rotation(Quat::from_rotation_y(b.yaw)),
            RigidBody::Static,
            collider,
            areas::sim(AREA),
        ));
    }
    if let Some((at, yaw)) = lay.door {
        use crate::features::doorway::{Doorway, Way, spawn};
        let door = spawn(
            &mut commands,
            c + at,
            yaw + std::f32::consts::PI,
            Doorway::new(Way::Out, "outside").size(4.0, 3.5),
        );
        commands.entity(door).insert(areas::sim(AREA));
    }
}

/// The way in, on the commune: the front doors of the museum's outside.
fn build_entrance(mut commands: Commands) {
    use crate::features::doorway::{Doorway, spawn, yaw_towards};
    let door = spawn(
        &mut commands,
        outside() + Vec3::new(0.0, 0.1, -14.15),
        yaw_towards(Vec2::Y),
        Doorway::way_in("The Ceiling Fan Museum").size(4.0, 3.5),
    );
    commands.entity(door).insert(areas::sim("commune"));
}

/// The map's models (on their own flat floor at the museum) and its lamps.
fn build_visuals(mut commands: Commands, assets: Res<AssetServer>) {
    let c = centre();
    // SALT: not needing `Cargo.lock` for this `.bsn` either; x didn't set us
    // up for success, we need more SME's, we haven't considered the problem
    // space enough, BSN syntax needs more changes, and `Children` sugaring?
    // Ask Sam "Salt" Saltman.
    commands.spawn((
        friendslop_world::map::Map(assets.load("maps/fan_museum.bsn")),
        friendslop_world::map::FlatGround,
        Transform::from_translation(c),
        Visibility::default(),
        areas::show(AREA),
    ));
    for (at, lumens, range) in layout(&MAP.get()).lamps {
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.95, 0.85),
                intensity: lumens,
                range,
                ..default()
            },
            Transform::from_translation(c + at),
            areas::show(AREA),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> MapFile {
        let path = format!(
            "{}/../game/assets/maps/fan_museum.map.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str(&text).unwrap_or_default()
    }

    #[test]
    fn its_area_holds_the_whole_map() {
        let text = std::fs::read_to_string(format!(
            "{}/../game/assets/data/base/areas.ron",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_default();
        assert!(friendslop_base::areas::AREAS.set_bytes(text.as_bytes()));
        let areas = friendslop_base::areas::areas();
        let Some(area) = areas.areas.iter().find(|a| a.id == AREA) else {
            panic!("no {AREA} area");
        };
        let commune: MapFile = serde_json::from_str(
            &std::fs::read_to_string(format!(
                "{}/../game/assets/maps/commune.map.json",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap_or_default(),
        )
        .unwrap_or_default();
        let Some(spot) = commune
            .entities
            .iter()
            .find(|e| e.spot().as_deref() == Some(SITE))
        else {
            panic!("no {SITE} spot");
        };
        let tuning = std::fs::read(format!(
            "{}/../game/assets/data/side/fan_museum.ron",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_default();
        assert!(TUNING.set_bytes(&tuning), "fan_museum.ron doesn't parse");
        let t = TUNING.get();
        assert!(t.door_reach > 0.5 && t.door_reach < 2.0);
        let origin = Vec3::from_array(spot.translation).with_y(t.interior_floor);
        for r in layout(&map()).rooms.iter().chain(&layout(&map()).blocks) {
            for corner in [r.centre - r.size / 2.0, r.centre + r.size / 2.0] {
                assert!(area.holds(origin + corner), "{corner} is outside the area");
            }
        }
    }

    #[test]
    fn the_front_doors_go_both_ways_and_dont_bounce() {
        let (out, ins) = (Vec3::new(62.0, 0.0, -56.0), Vec3::new(62.0, -95.0, -56.0));
        let Some((inside, _)) = through_door(out + OUTSIDE_DOOR, out, ins, 1.3) else {
            panic!("the outside door doesn't take you in");
        };
        assert!((inside - (ins + INSIDE_STEP)).length() < 1e-3);
        // Where you land isn't a door again.
        assert!(through_door(inside, out, ins, 1.3).is_none());
        let Some((back, _)) = through_door(ins + INSIDE_DOOR, out, ins, 1.3) else {
            panic!("the lobby door doesn't take you out");
        };
        assert!(through_door(back, out, ins, 1.3).is_none());
        // Walking past the building isn't going in.
        assert!(through_door(out + Vec3::new(8.0, 0.0, -15.0), out, ins, 1.3).is_none());
    }

    #[test]
    fn a_minute_at_sixty_rpm_is_sixty_turns() {
        let r = turn(60.0, 60.0);
        assert!((r - 60.0 * std::f32::consts::TAU).abs() < 1e-2);
        assert!(turn(-30.0, 1.0) < 0.0);
    }

    #[test]
    fn a_turned_box_contains_what_it_should() {
        let b = Box3 {
            centre: Vec3::new(1.0, 0.0, 0.0),
            size: Vec3::new(4.0, 2.0, 1.0),
            yaw: std::f32::consts::FRAC_PI_2,
            round: false,
        };
        assert!(b.contains(Vec3::new(1.0, 0.0, 1.8)));
        assert!(!b.contains(Vec3::new(2.8, 0.0, 0.0)));
    }

    #[test]
    fn the_map_has_its_rooms_walls_lights_and_door() {
        let lay = layout(&map());
        assert!(lay.rooms.len() >= 3, "museum, two halls");
        assert!(lay.blocks.len() > 20, "walls and exhibits");
        assert!(!lay.lamps.is_empty());
        let (door, _) = lay.door.unwrap_or_default();
        // The door is in front of the museum, and no wall stands in it.
        assert!(lay.rooms.iter().all(|r| !r.contains(door - Vec3::Z)) || door.z < -13.0);
        let walk_in = door + Vec3::new(0.0, 1.0, 1.0);
        assert!(lay.blocks.iter().all(|b| !b.contains(walk_in)), "{walk_in}");
    }

    #[test]
    fn the_doorways_between_rooms_are_open() {
        let lay = layout(&map());
        // The museum's west door and the annex's partition door, at z = 5.
        for x in [-20.0, -35.0] {
            let p = Vec3::new(x, 1.5, 5.0);
            assert!(lay.blocks.iter().all(|b| !b.contains(p)), "blocked at {p}");
        }
    }

    /// Where a visitor (0.3 m round, 1.8 m tall) can walk from `from`, on a
    /// 0.25 m grid over the museum's floor plan.
    fn walkable_from(lay: &Layout, from: Vec2) -> impl Fn(Vec2) -> bool {
        const CELL: f32 = 0.25;
        let (x0, z0, nx, nz) = (-50.0, -14.0, 280usize, 112usize);
        let free = |x: usize, z: usize| {
            let p = Vec2::new(x0 + (x as f32 + 0.5) * CELL, z0 + (z as f32 + 0.5) * CELL);
            [0.3, 1.0, 1.7].iter().all(|&y| {
                [(0.0, 0.0), (0.3, 0.0), (-0.3, 0.0), (0.0, 0.3), (0.0, -0.3)]
                    .iter()
                    .all(|(dx, dz)| {
                        let q = Vec3::new(p.x + dx, y, p.y + dz);
                        !lay.blocks.iter().any(|b| b.contains(q))
                    })
            })
        };
        let cell = move |p: Vec2| {
            let (x, z) = (((p.x - x0) / CELL) as i32, ((p.y - z0) / CELL) as i32);
            (x >= 0 && z >= 0 && (x as usize) < nx && (z as usize) < nz)
                .then_some((x as usize, z as usize))
        };
        let mut seen = vec![false; nx * nz];
        let mut todo: Vec<(usize, usize)> = cell(from).into_iter().collect();
        while let Some((x, z)) = todo.pop() {
            if seen[z * nx + x] || !free(x, z) {
                continue;
            }
            seen[z * nx + x] = true;
            if x > 0 {
                todo.push((x - 1, z));
            }
            if z > 0 {
                todo.push((x, z - 1));
            }
            if x + 1 < nx {
                todo.push((x + 1, z));
            }
            if z + 1 < nz {
                todo.push((x, z + 1));
            }
        }
        move |p: Vec2| cell(p).is_some_and(|(x, z)| seen[z * nx + x])
    }

    #[test]
    fn the_route_reaches_every_gallery() {
        let lay = layout(&map());
        let reach = walkable_from(&lay, Vec2::new(0.0, -12.5));
        for (name, at) in [
            ("a brief history", Vec2::new(12.5, 0.0)),
            ("the hall of fame", Vec2::new(0.0, 11.0)),
            ("the atrium", Vec2::new(-3.0, 0.0)),
            ("the cabinet of curiosities", Vec2::new(-12.8, -2.0)),
            ("the grate one", Vec2::new(-27.5, 5.0)),
            ("the möbius", Vec2::new(-42.5, -9.0)),
            ("the gift shop", Vec2::new(-17.5, -10.0)),
        ] {
            assert!(reach(at), "can't walk to {name}");
        }
    }

    #[test]
    fn the_atrium_opens_only_to_the_back() {
        // Walls between the galleries and the atrium: from the history
        // gallery the straight line into the atrium is blocked.
        let lay = layout(&map());
        let blocked = |a: Vec3, b: Vec3| {
            (0..=40).any(|i| {
                let p = a.lerp(b, i as f32 / 40.0);
                lay.blocks.iter().any(|w| w.contains(p))
            })
        };
        assert!(blocked(
            Vec3::new(12.0, 1.5, 0.0),
            Vec3::new(-3.0, 1.5, 0.0)
        ));
        assert!(blocked(
            Vec3::new(0.0, 1.5, -11.0),
            Vec3::new(0.0, 1.5, -4.0)
        ));
        assert!(!blocked(
            Vec3::new(0.0, 1.5, 11.0),
            Vec3::new(-3.0, 1.5, 2.5)
        ));
    }

    #[test]
    fn every_rotor_spins() {
        let rotors = map()
            .entities
            .iter()
            .filter(|e| e.gltf.as_deref().is_some_and(|g| g.contains("rotor")))
            .filter(|e| e.num("Spin", "rpm") == 0.0)
            .map(|e| e.name.clone())
            .collect::<Vec<_>>();
        assert!(rotors.is_empty(), "not spinning: {rotors:?}");
    }
}
