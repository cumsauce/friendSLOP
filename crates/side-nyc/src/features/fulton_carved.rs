//! The carved Fulton St: a hand-made station (the modeler document
//! `fulton-carved`, exported to `maps/fulton.bsn`) that Fulton's street
//! entrance leads into instead of the generated one, while
//! `data/side/fulton_carved.ron` has `enabled: true`.
//!
//! It is its own area, hung deep under empty ground. The map brings its walls
//! and floors (a glTF), its colliders (`Block` markers for the stairs and
//! walls, plus the smooth `fulton.collision` mesh for ramps), its lights
//! (`Lamp` markers, shown only near the camera), the `terminal` markers that
//! start the Andrew date, and the `exit` gate that puts you back on the street.
//!
//! Not yet: the monster's AI (`fulton_hunt`) still plans around the old
//! generated station; it does not hunt here, and the lockers are visual only.

use avian3d::prelude::{Collider, RigidBody};
use bevy::camera::primitives::{Frustum, Sphere};
use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use friendslop_world::collision::CollisionMesh;
use serde::Deserialize;

use crate::AppState;
use crate::features::fps::{WalkInput, Walker, teleport};
use crate::features::interact::{Clickable, Clicked};
use crate::player::PlayerOwner;

pub const AREA: &str = "fulton_carved";

/// `assets/data/side/fulton_carved.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Does the street entrance lead here (else to the generated station)?
    pub enabled: bool,
    /// The map's origin in the world (x, y, z).
    pub origin: (f32, f32, f32),
    /// How close (m) to the exit gate sends you back up.
    pub exit_reach: f32,
    /// How far (m) from the camera a lamp still shines.
    pub lamp_reach: f32,
    /// The map's lamps' reach is multiplied by this (their brightness is the
    /// map's: the area's look sets the exposure that reads them, `place_looks.ron`).
    pub lamp_range_gain: f32,
    /// A soft light that goes with you so no corner is pitch black: lumens, reach (m).
    pub follow_lumens: f32,
    pub follow_range: f32,
    /// At most this many lamps shine at once: the nearest ones the camera can see.
    pub max_lamps: usize,
    /// Seconds in the map when Andrew's calls ring (the third sends the monster).
    pub call_secs: Vec<f32>,
    /// The monster comes out of the nearest door at least this far (m) from you.
    pub min_spawn_dist: f32,
    /// It searches lockers this close (m) to where it lost you.
    pub search_reach: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/fulton_carved.ron");

/// The map's markers.
pub static MAP: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/fulton.map.json");

/// The smooth collider (ramps), in the map's frame.
pub static COLLISION: friendslop_data::Data<CollisionMesh> =
    friendslop_data::Data::new("maps/fulton.collision", CollisionMesh::parse);

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Is anyone in New York or the carved station?
pub fn awake_either() -> impl FnMut(Res<friendslop_base::sites::Awake>) -> bool + Clone {
    |a: Res<friendslop_base::sites::Awake>| a.anyone("nyc") || a.anyone(AREA)
}

/// Are you in New York or the carved station?
pub fn here_either() -> impl FnMut(Res<friendslop_base::sites::Awake>) -> bool + Clone {
    |a: Res<friendslop_base::sites::Awake>| a.me("nyc") || a.me(AREA)
}

/// Is `feet` in the carved station's area?
pub fn inside(feet: Vec3) -> bool {
    friendslop_base::areas::area_at(feet).as_deref() == Some(AREA)
}

pub fn origin() -> Vec3 {
    let o = tuning().origin;
    Vec3::new(o.0, o.1, o.2)
}

/// Does the street entrance lead to the carved map?
pub fn enabled() -> bool {
    tuning().enabled
}

fn marker(kind: &str) -> Option<(Vec3, f32)> {
    let map = MAP.get();
    let e = map.entities.iter().find(|e| e.kind == kind)?;
    Some((
        origin() + Vec3::from_array(e.translation),
        // Marker yaw 0 faces +z; a walker's yaw 0 faces -z.
        e.yaw.to_radians() + std::f32::consts::PI,
    ))
}

/// Where the street entrance lands you, and the way you face.
pub fn arrival() -> Option<(Vec3, f32)> {
    marker("spawn")
}

/// The exit gate's world position.
pub fn exit_at() -> Option<Vec3> {
    marker("exit").map(|m| m.0)
}

/// Is `feet` within `reach` of `gate` (the exit)? Pure, for the test.
pub fn near(feet: Vec3, gate: Vec3, reach: f32) -> bool {
    feet.distance(gate) < reach
}

/// A locker of the map (a `hide` marker, a metre above its floor): where you
/// stand inside it, and the way its door faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LockerSpot {
    pub feet: Vec3,
    pub out: Vec3,
    pub yaw: f32,
}

/// The map's lockers in marker-number order (`hide_1`, `hide_2`, ...): a
/// locker's index is the number a `Hide` request carries.
pub fn lockers() -> Vec<LockerSpot> {
    let origin = origin();
    let map = MAP.get();
    let mut marks: Vec<_> = map
        .entities
        .iter()
        .filter(|e| e.kind == "hide")
        .map(|e| {
            let n: u32 = e
                .name
                .rsplit('_')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            (n, e)
        })
        .collect();
    marks.sort_by_key(|m| m.0);
    marks
        .into_iter()
        .map(|(_, e)| {
            let yaw = e.yaw.to_radians();
            LockerSpot {
                feet: origin + Vec3::from_array(e.translation) - Vec3::Y,
                out: Vec3::new(yaw.sin(), 0.0, yaw.cos()),
                yaw,
            }
        })
        .collect()
}

/// The monster's doors (`hunter_spawn` markers): where it comes out, and the way it faces.
pub fn monster_doors() -> Vec<(Vec3, f32)> {
    let origin = origin();
    MAP.get()
        .entities
        .iter()
        .filter(|e| e.kind == "hunter_spawn")
        .map(|e| (origin + Vec3::from_array(e.translation), e.yaw.to_radians()))
        .collect()
}

fn rotation_of(e: &friendslop_world::places::MapEntity) -> Quat {
    match e.rotation {
        Some(r) => Quat::from_xyzw(r[0], r[1], r[2], r[3]).normalize(),
        None => Quat::from_rotation_y(e.yaw.to_radians()),
    }
}

/// Colliders: the stairs' and walls' `Block` markers and the smooth mesh.
fn build_colliders(mut commands: Commands) {
    let origin = origin();
    for e in MAP.get().entities.iter().filter(|e| e.has("Block")) {
        let size = e.size.map_or(Vec3::ONE, Vec3::from_array);
        commands.spawn((
            Transform::from_translation(origin + Vec3::from_array(e.translation))
                .with_rotation(rotation_of(e)),
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            friendslop_base::areas::sim(AREA),
        ));
    }
    if let Some(c) = COLLISION.get().collider() {
        commands.spawn((
            Transform::from_translation(origin),
            RigidBody::Static,
            c,
            friendslop_base::areas::sim(AREA),
        ));
    }
}

/// A lamp that only shines when the camera can see it (and is near): its
/// brightness and reach as the map has them.
#[derive(Component)]
struct NearLamp {
    lumens: f32,
    range: f32,
}

/// A terminal that starts the Andrew date.
#[derive(Component)]
struct Terminal;

/// The map itself, its lamps and its terminals.
fn build_show(mut commands: Commands, assets: Res<AssetServer>) {
    // SALT: not needing `Cargo.lock` for this `.bsn` either; x didn't set us
    // up for success, we need more SME's, we haven't considered the problem
    // space enough, BSN syntax needs more changes, and `Children` sugaring?
    // Ask Sam "Salt" Saltman.
    let origin = origin();
    commands.spawn((
        friendslop_world::map::Map(assets.load("maps/fulton.bsn")),
        friendslop_world::map::FlatGround,
        Transform::from_translation(origin),
        Visibility::default(),
        friendslop_base::areas::show(AREA),
    ));
    for e in MAP.get().entities.iter() {
        let at = origin + Vec3::from_array(e.translation);
        if e.has("Lamp") {
            let (r, g, b) = (e.num("Lamp", "r"), e.num("Lamp", "g"), e.num("Lamp", "b"));
            let color = if r + g + b <= 0.0 {
                Color::WHITE
            } else {
                Color::srgb(r, g, b)
            };
            let (lumens, range) = (e.num("Lamp", "lumens"), e.num("Lamp", "range").max(1.0));
            commands.spawn((
                NearLamp { lumens, range },
                PointLight {
                    intensity: lumens,
                    range,
                    color,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_translation(at),
                Visibility::Hidden,
                friendslop_base::areas::show(AREA),
            ));
        } else if e.kind == "hunter_spawn" {
            // (the monster's doors are data only: it comes out of them)
        } else if e.kind == "terminal" {
            commands.spawn((
                Terminal,
                Transform::from_translation(at),
                Visibility::default(),
                Clickable::at_height(1.0)
                    .radius(90.0)
                    .labelled("date Andrew"),
                friendslop_base::areas::show(AREA),
            ));
        }
    }
}

/// The lockers: a rigged door at each `hide` marker you can click to climb in
/// (the map's own static locker models under them are hidden).
fn build_lockers(mut commands: Commands) {
    let w = super::fulton_hunt::WRITING.get();
    let label: &'static str = w.hide_label.clone().leak();
    for (index, l) in lockers().into_iter().enumerate() {
        let door = commands
            .spawn((
                super::fulton_hunt::LockerDoor { index },
                crate::rigged::Rigged {
                    path: "models/features/fs_locker.gltf",
                },
                crate::rigged::RigClip::looping("closed"),
                Transform::from_translation(l.feet).with_rotation(Quat::from_rotation_y(l.yaw)),
                Visibility::default(),
                friendslop_base::areas::show(AREA),
            ))
            .id();
        commands.spawn((
            super::fulton_hunt::Locker,
            Transform::from_xyz(0.0, 0.0, super::fulton_hunt::LOCKER_FACE),
            Visibility::default(),
            Clickable::at_height(1.05)
                .tall(2.15)
                .radius(90.0)
                .labelled(label),
            ChildOf(door),
        ));
    }
}

/// Local: the map's static `locker_*` models give way to the rigged ones.
fn hide_static_lockers(mut statics: Query<(&Name, &mut Visibility), Added<Name>>) {
    for (name, mut vis) in &mut statics {
        if name.as_str().starts_with("locker_") {
            *vis = Visibility::Hidden;
        }
    }
}

/// The glow that goes with you down here.
#[derive(Component)]
struct FollowLight;

fn build_follow(mut commands: Commands) {
    let t = tuning();
    commands.spawn((
        FollowLight,
        PointLight {
            intensity: t.follow_lumens,
            range: t.follow_range,
            color: Color::srgb(1.0, 0.92, 0.82),
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::default(),
        Visibility::default(),
        friendslop_base::areas::show(AREA),
    ));
}

/// Local: the glow keeps up with the camera (and follows the data file).
fn follow_light(
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    mut light: Query<(&mut Transform, &mut PointLight), With<FollowLight>>,
) {
    let Some(at) = camera.iter().next().map(GlobalTransform::translation) else {
        return;
    };
    let t = tuning();
    for (mut tf, mut l) in &mut light {
        tf.translation = at + Vec3::Y * 0.6;
        if (l.intensity - t.follow_lumens).abs() > 0.01 {
            l.intensity = t.follow_lumens;
            l.range = t.follow_range;
        }
    }
}

fn click_terminal(ev: On<Clicked>, mut commands: Commands, terminals: Query<(), With<Terminal>>) {
    if terminals.contains(ev.entity) {
        commands.trigger(crate::features::dating_sim::StartDate);
    }
}

/// Local: only the nearest lamps the camera can see shine.
fn lamp_reach(
    camera: Query<(&GlobalTransform, &Frustum), With<crate::world::MainCamera>>,
    mut lamps: Query<(
        Entity,
        &GlobalTransform,
        &NearLamp,
        &mut PointLight,
        &mut Visibility,
    )>,
) {
    let Some((cam, frustum)) = camera.iter().next() else {
        return;
    };
    let cam = cam.translation();
    let t = tuning();
    let mut seen: Vec<(f32, Entity)> = lamps
        .iter()
        .filter_map(|(e, g, lamp, _, _)| {
            let at = g.translation();
            let d = at.distance(cam);
            let sphere = Sphere {
                center: at.into(),
                radius: lamp.range * t.lamp_range_gain,
            };
            (d < t.lamp_reach && frustum.intersects_sphere(&sphere, true)).then_some((d, e))
        })
        .collect();
    seen.sort_by(|a, b| a.0.total_cmp(&b.0));
    seen.truncate(t.max_lamps);
    for (e, _, lamp, mut light, mut vis) in &mut lamps {
        let on = seen.iter().any(|(_, s)| *s == e);
        let want = if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != want {
            *vis = want;
        }
        if on {
            let (lumens, range) = (lamp.lumens, lamp.range * t.lamp_range_gain);
            if (light.intensity - lumens).abs() > 0.01 {
                light.intensity = lumens;
            }
            if (light.range - range).abs() > 0.01 {
                light.range = range;
            }
        }
    }
}

/// Server: reach the exit gate and you are on the street again.
fn exit_gate(
    mut cover: MessageWriter<friendslop_base::areas::Cover>,
    mut bodies: Query<(
        &PlayerOwner,
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    let (Some(gate), t) = (exit_at(), super::fulton::tuning()) else {
        return;
    };
    let reach = tuning().exit_reach;
    for (_, mut walker, input, kcc) in &mut bodies {
        if near(walker.pos, gate, reach) {
            let yaw = walker.yaw;
            cover.write(friendslop_base::areas::Cover);
            teleport(
                &mut walker,
                input.map(|i| i.into_inner()),
                kcc.map(|k| k.into_inner()),
                super::fulton::street_at(&t) + Vec3::Y * 0.05,
                yaw,
            );
        }
    }
}

/// `FRIENDSLOP_START=fulton_carved` (the way in) and `fulton_carved_lockers`
/// (in front of the first locker).
fn starts() -> Vec<(String, Vec3)> {
    let mut out = Vec::new();
    if let Some((at, _)) = arrival() {
        out.push(("fulton_carved".to_string(), at));
    }
    if let Some(l) = lockers().first() {
        out.push(("fulton_carved_lockers".to_string(), l.feet + l.out * 2.5));
    }
    out
}

pub fn plugin(app: &mut App) {
    crate::features::start::provide(starts);
    friendslop_base::areas::on_sim(app, AREA, build_colliders);
    friendslop_base::areas::on_show(app, AREA, (build_show, build_lockers, build_follow));
    app.add_observer(click_terminal).add_systems(
        Update,
        (
            (lamp_reach, hide_static_lockers, follow_light)
                .run_if(in_state(AppState::InGame))
                .run_if(crate::sites::here(AREA)),
            exit_gate
                .run_if(in_state(bevy_replicon::prelude::ServerState::Running))
                .run_if(crate::sites::awake(AREA)),
        ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_and_map_have_what_the_code_names() {
        let t = tuning();
        assert!(t.exit_reach > 0.0 && t.lamp_reach > 0.0);
        let (spawn, _) = arrival().expect("a spawn marker");
        assert!(near(spawn, spawn, 0.1));
        assert!(exit_at().is_some(), "an exit marker");
        let map = MAP.get();
        assert!(map.entities.iter().any(|e| e.kind == "terminal"));
        assert!(map.entities.iter().any(|e| e.has("Block")));
        assert!(map.entities.iter().filter(|e| e.has("Lamp")).count() > 10);
        assert!(COLLISION.get().collider().is_some(), "the smooth collider");
    }

    #[test]
    fn the_map_sits_inside_its_area() {
        let o = origin();
        for e in MAP.get().entities.iter() {
            let p = o + Vec3::from_array(e.translation);
            assert!(
                p.x > 590.0 && p.x < 835.0 && p.z > 80.0 && p.z < 355.0,
                "{}",
                e.name
            );
            assert!(p.y > -395.0 && p.y < -290.0, "{}", e.name);
        }
    }
}
