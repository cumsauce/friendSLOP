//! New York traffic: yellow cabs drive the avenue and the cross streets,
//! stop at the lights, keep their distance from the cab ahead, honk at anyone
//! standing in their lane and don't brake for them: get hit and you're sent
//! flying.
//!
//! The server drives every cab (`Car { route, s }`, replicated): each route is
//! a straight road driven down its right-hand lane, U-turned at the far end
//! (out in the haze past the city) and driven back. Clients only draw the cab
//! where `s` puts it and play the horns the server sends.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::deny;
use crate::player::PlayerOwner;

/// One road, driven both ways: from `a` to `b` in the right-hand lane, a U-turn,
/// and back.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Road {
    /// The road's ends (x, z), on its centre line.
    pub a: (f32, f32),
    pub b: (f32, f32),
    /// How far right of the centre line the cabs drive (m).
    pub lane: f32,
    /// Which lights it obeys: roads in the same group get green together.
    pub group: u8,
    /// How far before a crossing's centre a cab stops at red (m).
    pub stop_gap: f32,
    /// How many cabs drive it.
    pub cabs: usize,
}

/// Tuning: `assets/data/side/nyc_traffic.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The roads cabs drive.
    pub roads: Vec<Road>,
    /// The crossings with lights (x, z).
    pub crossings: Vec<(f32, f32)>,
    /// Seconds a group's light stays green.
    pub green: f32,
    /// Seconds all lights stay red between greens.
    pub all_red: f32,
    /// Cruising speed (m/s).
    pub speed: f32,
    /// Speed through a U-turn (m/s).
    pub turn_speed: f32,
    /// Speeding up (m/s²) and braking (m/s²).
    pub accel: f32,
    pub brake: f32,
    /// Bumper-to-bumper gap kept to the cab ahead (m, centre to centre).
    pub follow_gap: f32,
    /// How far ahead a cab honks at someone in its lane (m).
    pub honk_range: f32,
    /// Seconds between one cab's honks.
    pub honk_every: f32,
    /// How fast a cab must go to knock you over (m/s).
    pub hit_speed: f32,
    /// How hard a hit throws you (m/s sideways out of the lane, and m/s up);
    /// half the cab's speed is added along the lane, then you ragdoll (#320).
    pub throw: f32,
    pub throw_up: f32,
    /// What you're told when a cab hits you.
    pub hit_lines: Vec<String>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/nyc_traffic.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The cab model (from the modeler); traffic still runs without it.
pub const CAB_MODEL: &str = "models/prefabs/nyc-kit-cab.gltf";

/// Half a cab's footprint (m): width, length.
const CAB_HALF: Vec2 = Vec2::new(1.0, 2.75);

/// A cab on a road: `route` indexes [`Tuning::roads`], `s` is how far round its loop it is.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Car {
    pub route: u8,
    pub s: f32,
}

/// Server only: a cab's speed and when it may honk again.
#[derive(Component, Default)]
struct Driving {
    speed: f32,
    honk_ready: f32,
}

/// Server → clients: a horn at `at`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Honk {
    pub at: Vec3,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Car>()
        .add_server_event::<Honk>(Channel::Unordered)
        .add_systems(OnEnter(ServerState::Running), spawn_cars)
        .add_systems(Update, drive.run_if(in_state(ServerState::Running)));
}

pub fn plugin(app: &mut App) {
    app.add_observer(play_honk)
        .add_systems(Update, draw_cars.run_if(in_state(AppState::InGame)));
}

// ------------------------------------------------------------------ geometry

fn v2((x, z): (f32, f32)) -> Vec2 {
    Vec2::new(x, z)
}

/// The road's direction from `a` to `b`, its length and its right-hand side.
fn frame(r: &Road) -> (Vec2, f32, Vec2) {
    let d = v2(r.b) - v2(r.a);
    let len = d.length().max(1.0);
    let d = d / len;
    (d, len, Vec2::new(-d.y, d.x))
}

/// How long the loop round a road is: both ways plus two U-turns.
pub fn loop_len(r: &Road) -> f32 {
    let (_, len, _) = frame(r);
    2.0 * len + 2.0 * std::f32::consts::PI * r.lane
}

/// Where a cab `s` metres round road `r`'s loop is (x, z), which way it faces,
/// and whether it's on a straight (not U-turning).
pub fn pose(r: &Road, s: f32) -> (Vec2, Vec2, bool) {
    let (d, len, right) = frame(r);
    let turn = std::f32::consts::PI * r.lane;
    let (a, b) = (v2(r.a), v2(r.b));
    let s = s.rem_euclid(loop_len(r));
    if s < len {
        (a + d * s + right * r.lane, d, true)
    } else if s < len + turn {
        let t = (s - len) / r.lane.max(0.01);
        let at = b + right * r.lane * t.cos() + d * r.lane * t.sin();
        (at, -right * t.sin() + d * t.cos(), false)
    } else if s < 2.0 * len + turn {
        let u = s - len - turn;
        (b - d * u - right * r.lane, -d, true)
    } else {
        let t = (s - 2.0 * len - turn) / r.lane.max(0.01);
        let at = a - right * r.lane * t.cos() - d * r.lane * t.sin();
        (at, right * t.sin() - d * t.cos(), false)
    }
}

/// Is group `group`'s light green `t` seconds into the session?
pub fn green(group: u8, t: f32, tu: &Tuning) -> bool {
    let groups = tu
        .roads
        .iter()
        .map(|r| r.group as usize + 1)
        .max()
        .unwrap_or(1);
    let slot = tu.green + tu.all_red;
    let phase = t.rem_euclid(slot * groups as f32);
    (phase / slot) as usize == group as usize && phase % slot < tu.green
}

/// How far ahead (m) the stop line is that a cab at `at` facing `dir` must stop at,
/// if its light is red and a crossing lies ahead on its road.
fn stop_ahead(at: Vec2, dir: Vec2, r: &Road, crossings: &[(f32, f32)]) -> Option<f32> {
    crossings
        .iter()
        .filter_map(|&c| {
            let rel = v2(c) - at;
            let along = rel.dot(dir) - r.stop_gap;
            let side = rel.perp_dot(dir).abs();
            (side < r.lane + 4.0 && along > -1.0).then_some(along.max(0.0))
        })
        .min_by(f32::total_cmp)
}

/// The speed a cab wants: cruising, slowed for U-turns, the cab ahead and red lights.
pub fn wanted_speed(gap: f32, stop: Option<f32>, straight: bool, tu: &Tuning) -> f32 {
    let room = |d: f32| (2.0 * tu.brake * d.max(0.0)).sqrt();
    let mut v = if straight { tu.speed } else { tu.turn_speed };
    v = v.min(room(gap - tu.follow_gap));
    if let Some(stop) = stop {
        v = v.min(room(stop));
    }
    v
}

/// Where someone at `p` is relative to a cab at `at` facing `dir`: (ahead, to the side).
fn relative(p: Vec2, at: Vec2, dir: Vec2) -> (f32, f32) {
    let rel = p - at;
    (rel.dot(dir), rel.perp_dot(dir))
}

/// Is someone at `p` under the cab?
pub fn hits(p: Vec2, at: Vec2, dir: Vec2) -> bool {
    let (ahead, side) = relative(p, at, dir);
    ahead.abs() < CAB_HALF.y + 0.3 && side.abs() < CAB_HALF.x + 0.3
}

/// Is someone at `p` standing in the cab's lane, close ahead?
pub fn in_the_way(p: Vec2, at: Vec2, dir: Vec2, range: f32) -> bool {
    let (ahead, side) = relative(p, at, dir);
    ahead > CAB_HALF.y && ahead < range && side.abs() < CAB_HALF.x + 0.8
}

// ------------------------------------------------------------------ server

fn spawn_cars(mut commands: Commands) {
    let tu = tuning();
    for (i, r) in tu.roads.iter().enumerate() {
        let l = loop_len(r);
        for k in 0..r.cabs {
            commands.spawn((
                Car {
                    route: i as u8,
                    s: l * k as f32 / r.cabs.max(1) as f32,
                },
                Driving::default(),
                Replicated,
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
}

#[allow(clippy::type_complexity)]
fn drive(
    time: Res<Time>,
    mut commands: Commands,
    mut cars: Query<(&mut Car, &mut Driving)>,
    bodies: Query<(
        Entity,
        &PlayerOwner,
        &Walker,
        Option<&crate::features::ragdoll::Ragdoll>,
    )>,
    mut hit_line: Local<usize>,
) {
    let tu = tuning();
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    // Everyone's position along each loop, to keep a gap to the cab ahead.
    let mut lanes: Vec<Vec<f32>> = vec![Vec::new(); tu.roads.len()];
    for (car, _) in &cars {
        if let Some(l) = lanes.get_mut(car.route as usize) {
            l.push(car.s);
        }
    }
    let bodies_at: Vec<Vec3> = bodies.iter().map(|b| b.2.pos).collect();
    for (mut car, mut driving) in &mut cars {
        let Some(road) = tu.roads.get(car.route as usize) else {
            continue;
        };
        let l = loop_len(road);
        let gap = lanes[car.route as usize]
            .iter()
            .map(|&o| (o - car.s).rem_euclid(l))
            .filter(|&g| g > 0.01)
            .min_by(f32::total_cmp)
            .unwrap_or(l);
        let (at, dir, straight) = pose(road, car.s);
        let stop = (straight && !green(road.group, now, &tu))
            .then(|| stop_ahead(at, dir, road, &tu.crossings))
            .flatten();
        let want = wanted_speed(gap, stop, straight, &tu);
        driving.speed = (driving.speed + tu.accel * dt).min(want);
        car.s = (car.s + driving.speed * dt).rem_euclid(l);

        // Honk at anyone in the lane, and don't stop for them.
        let (at, dir, _) = pose(road, car.s);
        let near = |p: &Vec3| p.y < crate::rules::nyc::CITY_Y + 2.0;
        if now >= driving.honk_ready
            && bodies_at
                .iter()
                .filter(|p| near(p))
                .any(|p| in_the_way(p.xz(), at, dir, tu.honk_range))
        {
            driving.honk_ready = now + tu.honk_every;
            honk(&mut commands, at);
        }
        if driving.speed < tu.hit_speed {
            continue;
        }
        for (entity, owner, walker, ragdoll) in &bodies {
            if ragdoll.is_some() || !near(&walker.pos) || !hits(walker.pos.xz(), at, dir) {
                continue; // already flying (#320)
            }
            let (_, side) = relative(walker.pos.xz(), at, dir);
            let away = Vec2::new(-dir.y, dir.x) * if side < 0.0 { 1.0 } else { -1.0 };
            // Punted out of the lane and along it, then you ragdoll (#320).
            let fling = away * tu.throw + dir * driving.speed * 0.5;
            crate::features::ragdoll::knock(
                &mut commands,
                entity,
                Vec3::new(fling.x, tu.throw_up, fling.y),
            );
            honk(&mut commands, at);
            if !tu.hit_lines.is_empty() {
                *hit_line = (*hit_line + 1) % tu.hit_lines.len();
                let client = owner.0.map_or(ClientId::Server, ClientId::Client);
                deny(&mut commands, client, tu.hit_lines[*hit_line].clone());
            }
        }
    }
}

fn honk(commands: &mut Commands, at: Vec2) {
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: Honk {
            at: Vec3::new(at.x, crate::rules::nyc::CITY_Y + 1.0, at.y),
        },
    });
}

// ------------------------------------------------------------------ clients

/// Draw each cab where the server says it is (smoothed between updates),
/// only on screens near the city.
fn draw_cars(
    mut commands: Commands,
    time: Res<Time>,
    assets: Res<AssetServer>,
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    mut cars: Query<(
        Entity,
        &Car,
        Option<&mut Transform>,
        Option<&mut Visibility>,
    )>,
) {
    let tu = tuning();
    let near = camera
        .iter()
        .next()
        .is_some_and(|g| super::nyc::city_drawn(g.translation().x, g.translation().z));
    let blend = 1.0 - (-15.0 * time.delta_secs()).exp();
    for (entity, car, transform, vis) in &mut cars {
        let Some(road) = tu.roads.get(car.route as usize) else {
            continue;
        };
        let (at, dir, _) = pose(road, car.s);
        let want = Transform::from_xyz(at.x, crate::rules::nyc::CITY_Y, at.y)
            .looking_to(Vec3::new(-dir.x, 0.0, -dir.y), Vec3::Y);
        let shown = if near {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        match (transform, vis) {
            (Some(mut t), Some(mut v)) => {
                t.translation = t.translation.lerp(want.translation, blend);
                t.rotation = t.rotation.slerp(want.rotation, blend);
                v.set_if_neq(shown);
            }
            _ => {
                commands.entity(entity).insert((
                    want,
                    shown,
                    WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(CAB_MODEL))),
                    DespawnOnExit(AppState::InGame),
                ));
            }
        }
    }
}

const HORN_RATE: u32 = crate::features::synth_audio::RATE;

/// A cab horn: two slightly sour square-ish tones, a short "HONK".
pub fn horn_samples(rate: u32) -> Vec<f32> {
    let n = (0.45 * rate as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let (mut p1, mut p2) = (0.0f32, 0.0f32);
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let (i1, i2) = (415.0 / rate as f32, 494.0 / rate as f32);
        p1 = (p1 + i1).fract();
        p2 = (p2 + i2).fract();
        let s1 = crate::features::synth_audio::saw(p1, i1);
        let s2 = crate::features::synth_audio::saw(p2, i2);
        let env = (t * 60.0).min(1.0) * ((0.45 - t) * 25.0).clamp(0.0, 1.0);
        out.push(((s1 + s2) * 1.4).tanh() * env * 0.6);
    }
    crate::features::synth_audio::master(&mut out, rate, 3500.0, 0.7);
    out
}

fn play_honk(ev: On<Honk>, mut commands: Commands) {
    crate::spatial::play_wav_at(
        &mut commands,
        ev.at,
        1.0,
        crate::spatial::Carry::LOUD,
        || crate::features::bear::wav_bytes(&horn_samples(HORN_RATE), HORN_RATE),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/side/nyc_traffic.ron"
        );
        let Ok(bytes) = std::fs::read(path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_has_every_field() {
        let t = t();
        assert!(!t.roads.is_empty() && !t.crossings.is_empty() && !t.hit_lines.is_empty());
        assert!(t.speed > 0.0 && t.green > 0.0 && t.brake > 0.0);
    }

    #[test]
    fn the_loop_is_continuous_and_drives_on_the_right() {
        let t = t();
        for r in &t.roads {
            let l = loop_len(r);
            let mut prev = pose(r, 0.0).0;
            for i in 1..=2000 {
                let (p, _, _) = pose(r, l * i as f32 / 2000.0);
                assert!(
                    p.distance(prev) < l / 2000.0 * 1.5 + 0.01,
                    "{r:?} jumps at {i}"
                );
                prev = p;
            }
            // Heading a→b, the lane is on the right.
            let (p, d, _) = pose(r, 10.0);
            let (_, len, right) = frame(r);
            assert!(len > 10.0 && (p - v2(r.a)).dot(right) > 0.0 && d.dot(right).abs() < 1e-4);
        }
    }

    #[test]
    fn one_group_at_a_time_and_red_means_stop() {
        let t = t();
        for s in 0..200 {
            let at = s as f32 * 0.37;
            assert!(!(green(0, at, &t) && green(1, at, &t)));
        }
        assert!(green(0, 0.5, &t) && !green(1, 0.5, &t));
        assert_eq!(wanted_speed(100.0, Some(0.0), true, &t), 0.0);
        assert_eq!(wanted_speed(t.follow_gap, None, true, &t), 0.0);
        assert_eq!(wanted_speed(1000.0, None, true, &t), t.speed);
    }

    #[test]
    fn a_cab_honks_ahead_and_hits_what_it_drives_into() {
        let (at, dir) = (Vec2::ZERO, Vec2::Y);
        assert!(in_the_way(Vec2::new(0.3, 10.0), at, dir, 20.0));
        assert!(!in_the_way(Vec2::new(4.0, 10.0), at, dir, 20.0));
        assert!(!in_the_way(Vec2::new(0.0, -10.0), at, dir, 20.0));
        assert!(hits(Vec2::new(0.5, 2.0), at, dir));
        assert!(!hits(Vec2::new(3.0, 0.0), at, dir));
    }
}
