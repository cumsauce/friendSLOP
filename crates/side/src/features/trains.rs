//! Trains. From issue #321 by doomy ("take the path to nyc"): "instead of a
//! sign to click, players board the train headed to the oculus station. it
//! would be cool to have a little transition in the subway when loading
//! between zones with the SAM announcing the stop. can generalize this, we
//! might have more trains".
//!
//! A line runs between two stops and a train (the `path-car` model, beside a
//! `path-platform`) waits at both. Walk in and stand there a moment and the
//! server moves you into that direction's ride car, hung deep under the
//! city, where tunnel lights rush past the windows and the announcer (the
//! `samiam` speech synth, like Andrew's phone calls) says the next stop. When
//! the ride's over you're standing in the train at the other end. Lines,
//! stops, times and lines of speech are data (`data/side/trains.ron`): add a
//! line and you get another train. Colliders are mesh-less boxes, so the
//! trains work without the model files.

use std::collections::HashMap;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::Deserialize;

use crate::AppState;
use crate::features::fps::{WalkInput, Walker};
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// One stop of a line.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Stop {
    /// The stop's name.
    pub name: String,
    /// The car's middle (x, z).
    pub at: (f32, f32),
    /// Which way the car points, degrees about up.
    pub yaw: f32,
    /// On the city floor (true) or the valley's ground (false).
    pub city: bool,
    /// Said on the way to this stop.
    pub announce: String,
}

/// A line: a train between two stops.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Line {
    pub name: String,
    pub stops: [Stop; 2],
}

/// Tuning and writing: `assets/data/side/trains.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Seconds standing in a waiting train before it leaves with you.
    pub board_secs: f32,
    /// Seconds the ride takes.
    pub ride_secs: f32,
    /// Where the ride cars hang (x, y, z); one per line and direction, 30 m apart along x.
    pub ride_at: (f32, f32, f32),
    /// The announcer's voice: pitch and speed.
    pub voice: (f32, f32),
    /// Shown while a train waits to leave with you.
    pub boarding_line: String,
    pub lines: Vec<Line>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/trains.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

// ------------------------------------------------------------------ the car's shape

/// Half the car's length (along its own x) and width (z), from the model.
const HALF_LEN: f32 = 8.0;
const HALF_WIDE: f32 = 1.42;
/// The car's floor top above its origin, and its roof.
const FLOOR: f32 = 0.26;
const ROOF: f32 = 3.1;
/// The doors (on the car's +z side): their middles along x and their width.
const DOORS: [f32; 2] = [-3.5, 3.5];
const DOOR_W: f32 = 1.4;

/// Where a stop's car stands (its origin, on the ground).
pub fn stop_origin(s: &Stop) -> Vec3 {
    let (x, z) = s.at;
    let y = if s.city {
        crate::rules::nyc::CITY_Y
    } else {
        crate::terrain::height_at(x, z)
    };
    Vec3::new(x, y, z)
}

/// Where the ride car for line `line` heading to stop `to` hangs.
pub fn ride_origin(t: &Tuning, line: usize, to: usize) -> Vec3 {
    Vec3::from(t.ride_at) + Vec3::X * 30.0 * (line * 2 + to) as f32
}

/// Is `p` (feet) inside a car with this origin and yaw?
pub fn in_car(origin: Vec3, yaw_deg: f32, p: Vec3) -> bool {
    let l = Quat::from_rotation_y(-yaw_deg.to_radians()) * (p - origin);
    l.x.abs() < HALF_LEN - 0.2 && l.z.abs() < HALF_WIDE - 0.1 && (-0.5..2.5).contains(&l.y)
}

/// Which car someone at `p` is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Car {
    /// The train waiting at a stop of a line.
    Waiting { line: usize, stop: usize },
    /// A ride car, on the way to stop `to`.
    Riding { line: usize, to: usize },
}

pub fn car_at(t: &Tuning, p: Vec3) -> Option<Car> {
    t.lines.iter().enumerate().find_map(|(line, l)| {
        (0..2).find_map(|i| {
            if in_car(stop_origin(&l.stops[i]), l.stops[i].yaw, p) {
                Some(Car::Waiting { line, stop: i })
            } else if in_car(ride_origin(t, line, i), 0.0, p) {
                Some(Car::Riding { line, to: i })
            } else {
                None
            }
        })
    })
}

/// Where you stand on arrival: the middle of the car, on its floor.
fn aboard(origin: Vec3) -> Vec3 {
    origin + Vec3::Y * (FLOOR + 0.05)
}

/// One rider's progress (server).
#[derive(Default, Debug, Clone, Copy)]
pub struct Rider {
    /// Has been off the trains since they last arrived (so arriving doesn't board you again).
    pub ready: bool,
    /// Seconds stood in a waiting train.
    pub aboard: f32,
    /// On a ride: the line, the stop it goes to, seconds left.
    pub riding: Option<(usize, usize, f32)>,
}

/// One server tick of a rider in `car`: where to put them, if anywhere.
pub fn step(r: &mut Rider, car: Option<Car>, dt: f32, t: &Tuning) -> Option<Vec3> {
    if let Some((line, to, left)) = r.riding {
        if car != Some(Car::Riding { line, to }) {
            // Moved off the ride some other way.
            r.riding = None;
            return None;
        }
        let left = left - dt;
        if left > 0.0 {
            r.riding = Some((line, to, left));
            return None;
        }
        r.riding = None;
        r.ready = false;
        let stop = &t.lines.get(line)?.stops[to];
        return Some(aboard(stop_origin(stop)));
    }
    match car {
        Some(Car::Waiting { line, stop }) if r.ready => {
            r.aboard += dt;
            if r.aboard < t.board_secs {
                return None;
            }
            r.aboard = 0.0;
            let to = 1 - stop;
            r.riding = Some((line, to, t.ride_secs));
            Some(aboard(ride_origin(t, line, to)))
        }
        Some(_) => None,
        None => {
            r.ready = true;
            r.aboard = 0.0;
            None
        }
    }
}

pub fn protocol(app: &mut App) {
    app.add_systems(Update, ride.run_if(in_state(ServerState::Running)));
    // Keep the valley's trees off the platforms.
    for l in &tuning().lines {
        for s in &l.stops {
            if !s.city {
                friendslop_world::footprints::claim(Vec2::new(s.at.0, s.at.1), 12.0);
            }
        }
    }
}

fn ride(
    time: Res<Time>,
    mut riders: Local<HashMap<Entity, Rider>>,
    mut bodies: Query<
        (
            Entity,
            &mut Walker,
            Option<&mut WalkInput>,
            Option<&mut crate::kcc::Kcc>,
        ),
        With<PlayerOwner>,
    >,
) {
    let t = tuning();
    let dt = time.delta_secs();
    riders.retain(|e, _| bodies.contains(*e));
    for (e, mut walker, input, kcc) in &mut bodies {
        let r = riders.entry(e).or_default();
        let car = car_at(&t, walker.pos);
        let Some(to) = step(r, car, dt, &t) else {
            continue;
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
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), build)
        .add_systems(
            Update,
            (announce, tunnel_lights).run_if(in_state(AppState::InGame)),
        );
}

fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/{file}"))))
}

/// The static boxes of a car (centre, size in its own frame); `closed` shuts the doors.
pub fn car_boxes(closed: bool) -> Vec<(Vec3, Vec3)> {
    let h = ROOF / 2.0;
    let mut b = vec![
        (Vec3::Y * FLOOR / 2.0, Vec3::new(16.0, FLOOR, 2.8)),
        (Vec3::new(0.0, h, -HALF_WIDE), Vec3::new(16.0, ROOF, 0.08)),
        (Vec3::new(-HALF_LEN, h, 0.0), Vec3::new(0.08, ROOF, 2.8)),
        (Vec3::new(HALF_LEN, h, 0.0), Vec3::new(0.08, ROOF, 2.8)),
        (Vec3::Y * ROOF, Vec3::new(16.0, 0.08, 2.8)),
    ];
    // The door side, in pieces around the doors.
    let mut x0 = -HALF_LEN;
    for d in DOORS {
        let x1 = d - DOOR_W / 2.0;
        b.push((
            Vec3::new((x0 + x1) / 2.0, h, HALF_WIDE),
            Vec3::new(x1 - x0, ROOF, 0.08),
        ));
        if closed {
            b.push((Vec3::new(d, h, HALF_WIDE), Vec3::new(DOOR_W, ROOF, 0.08)));
        }
        x0 = d + DOOR_W / 2.0;
    }
    b.push((
        Vec3::new((x0 + HALF_LEN) / 2.0, h, HALF_WIDE),
        Vec3::new(HALF_LEN - x0, ROOF, 0.08),
    ));
    b
}

/// The platform's slab beside a waiting car (from the `path-platform` model).
const PLATFORM: (Vec3, Vec3) = (Vec3::new(0.0, 0.13, 3.25), Vec3::new(20.0, 0.26, 3.6));

fn spawn_boxes(commands: &mut Commands, at: Transform, boxes: &[(Vec3, Vec3)]) {
    for (c, s) in boxes {
        commands.spawn((
            Transform::from_translation(at.transform_point(*c)).with_rotation(at.rotation),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(s.x, s.y, s.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// A light rushing past a ride car's windows.
#[derive(Component)]
struct TunnelLight {
    origin: Vec3,
}

fn build(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    let t = tuning();
    let waiting = car_boxes(false);
    let shut = car_boxes(true);
    for (li, l) in t.lines.iter().enumerate() {
        for s in &l.stops {
            let at = Transform::from_translation(stop_origin(s))
                .with_rotation(Quat::from_rotation_y(s.yaw.to_radians()));
            for file in ["path-car.gltf", "path-platform.gltf"] {
                commands.spawn((model(&assets, file), at, DespawnOnExit(AppState::InGame)));
            }
            spawn_boxes(&mut commands, at, &waiting);
            spawn_boxes(&mut commands, at, &[PLATFORM]);
        }
        for to in 0..2 {
            let o = ride_origin(&t, li, to);
            let at = Transform::from_translation(o);
            commands.spawn((
                model(&assets, "path-car.gltf"),
                at,
                DespawnOnExit(AppState::InGame),
            ));
            spawn_boxes(&mut commands, at, &shut);
            commands.spawn((
                PointLight {
                    intensity: 60_000.0,
                    range: 12.0,
                    ..default()
                },
                Transform::from_translation(o + Vec3::Y * 2.6),
                DespawnOnExit(AppState::InGame),
            ));
            // The tunnel: lights streaming past both sides.
            let mesh = meshes.add(Cuboid::new(1.2, 0.15, 0.15));
            let glow = materials.add(StandardMaterial {
                base_color: Color::BLACK,
                emissive: LinearRgba::rgb(6.0, 5.0, 3.0),
                ..default()
            });
            for i in 0..8 {
                for side in [-3.0, 3.0] {
                    commands.spawn((
                        TunnelLight { origin: o },
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(glow.clone()),
                        Transform::from_translation(
                            o + Vec3::new(-40.0 + i as f32 * 10.0, 2.0, side),
                        ),
                        DespawnOnExit(AppState::InGame),
                    ));
                }
            }
        }
    }
}

/// Speed of the tunnel lights going by (m/s).
const TUNNEL_SPEED: f32 = 25.0;

fn tunnel_lights(time: Res<Time>, mut lights: Query<(&TunnelLight, &mut Transform)>) {
    let dx = TUNNEL_SPEED * time.delta_secs();
    for (l, mut tf) in &mut lights {
        tf.translation.x -= dx;
        if tf.translation.x < l.origin.x - 40.0 {
            tf.translation.x += 80.0;
        }
    }
}

/// The announcer's voice saying `text`.
pub fn announcer_says(text: &str, (pitch, speed): (f32, f32)) -> (Vec<f32>, u32) {
    let mut voice = samiam::Voice::new();
    voice.pitch = pitch;
    voice.speed = speed;
    (voice.speak(text), voice.sample_rate)
}

/// Local: the doors line when you step into a waiting train, and the
/// announcement when the ride starts.
fn announce(
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut was: Local<Option<Car>>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let t = tuning();
    let car = car_at(&t, w.pos);
    if car == *was {
        return;
    }
    let before = *was;
    *was = car;
    match car {
        Some(Car::Waiting { .. }) if !matches!(before, Some(Car::Riding { .. })) => {
            toast.show(t.boarding_line.clone());
        }
        Some(Car::Riding { line, to }) => {
            let Some(stop) = t.lines.get(line).map(|l| &l.stops[to]) else {
                return;
            };
            toast.show(stop.announce.clone());
            let (samples, rate) = announcer_says(&stop.announce, t.voice);
            crate::spatial::play_wav_at(
                &mut commands,
                w.pos + Vec3::Y * 2.5,
                1.0,
                crate::spatial::Carry::NEARBY,
                move || crate::features::bear::wav_bytes(&samples, rate),
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.board_secs > 0.0 && t.ride_secs > t.board_secs);
        assert!(t.voice.0 > 0.0 && t.voice.1 > 0.0 && !t.boarding_line.is_empty());
        assert!(!t.lines.is_empty());
        for l in &t.lines {
            assert!(!l.name.is_empty());
            for s in &l.stops {
                assert!(!s.name.is_empty() && !s.announce.is_empty());
            }
        }
        // The PATH goes to the Oculus.
        let path = &t.lines[0];
        let city = path.stops.iter().find(|s| s.city).expect("a city stop");
        let oculus = Vec2::new(-55.0, 230.0);
        assert!(Vec2::new(city.at.0, city.at.1).distance(oculus) < 15.0);
    }

    #[test]
    fn walking_in_rides_you_to_the_other_end() {
        let mut t = t();
        t.lines.truncate(1);
        let mut r = Rider::default();
        // Outside: ready to board.
        assert_eq!(step(&mut r, None, 0.1, &t), None);
        let waiting = Some(Car::Waiting { line: 0, stop: 0 });
        assert_eq!(step(&mut r, waiting, t.board_secs / 2.0, &t), None);
        let into = step(&mut r, waiting, t.board_secs, &t).expect("boards");
        assert_eq!(car_at(&t, into), Some(Car::Riding { line: 0, to: 1 }));
        let riding = car_at(&t, into);
        assert_eq!(step(&mut r, riding, t.ride_secs / 2.0, &t), None);
        let out = step(&mut r, riding, t.ride_secs, &t).expect("arrives");
        assert_eq!(car_at(&t, out), Some(Car::Waiting { line: 0, stop: 1 }));
        assert!(crate::rules::nyc::in_city(out.x, out.z));
        // Standing in the train you arrived on doesn't send you straight back.
        let there = Some(Car::Waiting { line: 0, stop: 1 });
        for _ in 0..100 {
            assert_eq!(step(&mut r, there, 0.1, &t), None);
        }
        // Step off and back on: you ride home.
        step(&mut r, None, 0.1, &t);
        assert!(step(&mut r, there, t.board_secs + 0.1, &t).is_some());
    }

    #[test]
    fn the_cars_are_boxed_in() {
        assert_eq!(car_boxes(true).len(), car_boxes(false).len() + DOORS.len());
        let o = Vec3::new(0.0, 0.0, 0.0);
        assert!(in_car(o, 0.0, Vec3::new(0.0, FLOOR, 0.0)));
        assert!(!in_car(o, 0.0, Vec3::new(0.0, FLOOR, 3.0)));
        // Turned 90 degrees, the car runs along z.
        assert!(in_car(o, 90.0, Vec3::new(0.0, FLOOR, 6.0)));
    }

    #[test]
    fn the_announcer_speaks() {
        let (s, rate) = announcer_says("Next stop: World Trade Center.", (80.0, 0.9));
        assert!(rate > 0 && s.len() > rate as usize / 2);
    }
}
