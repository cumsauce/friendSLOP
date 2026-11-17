//! The sky train (surf map, #306: doomy wanted "an area where you ride
//! alongside a sky train").
//!
//! An elevated monorail beam on tall pylons runs beside one of the course's
//! ramps (stage 5's run into the finish), and a train of sleek neon cars
//! glides back and forth on it at surf speed, so you race it while you surf.
//! You can land on its roof and ride it a bit.
//!
//! Where it goes is worked out from the ramp it follows ([`rail`]), so it
//! follows the course when the course changes. Its motion comes from the
//! wall clock ([`crate::settings::since_epoch`]), so every peer sees the
//! train in the same place without replicating anything: it's scenery.
//! Numbers: `data/side/surf_train.ron`; models: modeler prefabs
//! `surf-train-*` (`models/features/surf_train_*.gltf`); colliders come from
//! here, never the files. The beam and pylons are in the surf map; this is
//! the moving cars.

use avian3d::prelude::{Collider, LinearVelocity, RigidBody};
use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use crate::AppState;
use crate::rules::surf::{Tuning, hall_at, ridge, tuning};

/// Tuning numbers: `assets/data/side/surf_train.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Train {
    /// The ramp the line follows (stage, ramp; from 0).
    pub stage: usize,
    pub ramp: usize,
    /// Which side (1 left looking downhill, -1 right), how far from the
    /// ridge (m) and how far above it the beam's top runs.
    pub side: f32,
    pub offset: f32,
    pub rise: f32,
    /// How far the line runs on past the ramp's start and end (m).
    pub extend: (f32, f32),
    /// The beam's width and depth, and one model segment's length (m).
    pub beam: (f32, f32),
    pub segment: f32,
    /// A pylon every this many metres, its width, and where its feet stand.
    pub pylon_every: f32,
    pub pylon_width: f32,
    pub floor: f32,
    /// How many cars, each one's length, width and height, the gap between.
    pub cars: usize,
    pub car: (f32, f32, f32),
    pub car_gap: f32,
    /// Top speed (m/s) and how long it waits at each end (s).
    pub speed: f32,
    pub dwell: f32,
    /// How far generated props keep from the line (m).
    pub claim_margin: f32,
}

pub static TRAIN: friendslop_data::Data<Train> =
    friendslop_data::Data::ron("data/side/surf_train.ron");

/// The beam's top centre line (where the cars ride), hall coordinates,
/// resampled every `segment` metres.
pub fn rail(t: &Tuning, d: &Train) -> Vec<Vec3> {
    let Some(r) = t.stages.get(d.stage).and_then(|s| s.ramps.get(d.ramp)) else {
        return Vec::new();
    };
    let ridge = ridge(r, t.ramp_samples * 4);
    if ridge.len() < 2 {
        return Vec::new();
    }
    let n = ridge.len();
    let mut raw: Vec<Vec3> = (0..n)
        .map(|i| {
            let a = ridge[i.saturating_sub(1)];
            let b = ridge[(i + 1).min(n - 1)];
            let f = (b - a).with_y(0.0).normalize_or(Vec3::NEG_Z);
            let left = Vec3::new(f.z, 0.0, -f.x);
            ridge[i] + left * d.side * d.offset + Vec3::Y * d.rise
        })
        .collect();
    let first = raw[0] - (raw[1] - raw[0]).normalize_or(Vec3::Z) * d.extend.0;
    let last = raw[n - 1] + (raw[n - 1] - raw[n - 2]).normalize_or(Vec3::NEG_Z) * d.extend.1;
    raw.insert(0, first);
    raw.push(last);
    resample(&raw, d.segment.max(0.5))
}

/// Points every `step` metres along a polyline (and its very end).
fn resample(p: &[Vec3], step: f32) -> Vec<Vec3> {
    let total = length(p);
    let count = (total / step).ceil().max(1.0) as usize;
    (0..=count)
        .map(|i| along(p, total * i as f32 / count as f32).0)
        .collect()
}

pub fn length(p: &[Vec3]) -> f32 {
    p.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// The point `u` metres along a polyline, and its direction there.
pub fn along(p: &[Vec3], u: f32) -> (Vec3, Vec3) {
    let mut left = u.max(0.0);
    for w in p.windows(2) {
        let l = w[0].distance(w[1]);
        let dir = (w[1] - w[0]).normalize_or(Vec3::NEG_Z);
        if left <= l {
            return (w[0] + dir * left, dir);
        }
        left -= l;
    }
    match p {
        [.., a, b] => (*b, (*b - *a).normalize_or(Vec3::NEG_Z)),
        [a] => (*a, Vec3::NEG_Z),
        [] => (Vec3::ZERO, Vec3::NEG_Z),
    }
}

/// The whole train's length (m).
pub fn train_length(d: &Train) -> f32 {
    d.cars as f32 * d.car.0 + d.cars.saturating_sub(1) as f32 * d.car_gap
}

/// How far along the line the train's tail is at `secs`, and its speed
/// along it (m/s). It eases from one end to the other (top speed
/// mid-line), waits, and comes back.
pub fn travel(range: f32, d: &Train, secs: f64) -> (f32, f32) {
    if range <= 0.0 || d.speed <= 0.0 {
        return (0.0, 0.0);
    }
    let pi = std::f32::consts::PI;
    // One way, eased with a half cosine whose peak is `speed`.
    let go = pi * range / (2.0 * d.speed);
    let cycle = 2.0 * (go + d.dwell.max(0.0));
    let s = secs.rem_euclid(cycle as f64) as f32;
    let ease = |s: f32| {
        (
            range * (1.0 - (pi * s / go).cos()) / 2.0,
            range * pi / (2.0 * go) * (pi * s / go).sin(),
        )
    };
    let back = go + d.dwell;
    if s < go {
        ease(s)
    } else if s < back {
        (range, 0.0)
    } else if s < back + go {
        let (u, v) = ease(s - back);
        (range - u, -v)
    } else {
        (0.0, 0.0)
    }
}

/// Each car at `secs`: where its floor's middle is (on the beam's top),
/// which way it faces (+z along the line) and its velocity.
pub fn cars(rail: &[Vec3], d: &Train, secs: f64) -> Vec<(Vec3, Quat, Vec3)> {
    let range = length(rail) - train_length(d);
    let (u, v) = travel(range, d, secs);
    (0..d.cars)
        .map(|i| {
            let mid = u + d.car.0 / 2.0 + i as f32 * (d.car.0 + d.car_gap);
            let a = along(rail, mid - d.car.0 / 2.0).0;
            let b = along(rail, mid + d.car.0 / 2.0).0;
            let f = (b - a).normalize_or(Vec3::NEG_Z);
            let rot = Transform::IDENTITY.looking_to(-f, Vec3::Y).rotation;
            ((a + b) / 2.0, rot, f * v)
        })
        .collect()
}

/// The pylons: each one's top (under the beam) and its height.
pub fn pylons(rail: &[Vec3], d: &Train) -> Vec<(Vec3, f32)> {
    let total = length(rail);
    let count = (total / d.pylon_every.max(1.0)).floor() as usize;
    let gap = total / count.max(1) as f32;
    (0..=count)
        .map(|i| {
            let top = along(rail, gap * i as f32).0 - Vec3::Y * d.beam.1;
            (top, (top.y - d.floor).max(0.0))
        })
        .collect()
}

/// The beam's pieces: the middle of each one's top, its rotation (+z along
/// the line) and its length.
pub fn beams(rail: &[Vec3]) -> Vec<(Vec3, Quat, f32)> {
    rail.windows(2)
        .map(|w| {
            let f = (w[1] - w[0]).normalize_or(Vec3::NEG_Z);
            let rot = Transform::IDENTITY.looking_to(-f, Vec3::Y).rotation;
            ((w[0] + w[1]) / 2.0, rot, w[0].distance(w[1]))
        })
        .collect()
}

/// Boxes (centre, half size) the line takes up, for `surf_props::claim`.
pub fn claims(rail: &[Vec3], d: &Train) -> Vec<(Vec3, Vec3)> {
    let m = d.claim_margin + d.car.1.max(d.pylon_width);
    rail.windows(2)
        .map(|w| {
            let lo = w[0].min(w[1]) - Vec3::new(m, 0.0, m);
            let hi = w[0].max(w[1]) + Vec3::new(m, d.car.2 + m, m);
            let lo = lo.with_y(d.floor);
            ((lo + hi) / 2.0, (hi - lo) / 2.0)
        })
        .collect()
}

pub fn plugin(app: &mut App) {
    app.spawn_into("surf", build).add_systems(
        Update,
        glide
            .run_if(crate::areas::shown("surf"))
            .run_if(in_state(AppState::InGame)),
    );
}

/// A car of the train (which one, from the tail).
#[derive(Component)]
struct Car(usize);

fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

fn build(mut commands: Commands, assets: Res<AssetServer>) {
    let t = tuning();
    let d = TRAIN.get();
    let o = hall_at(&t);
    let line = rail(&t, &d);
    let now = crate::settings::since_epoch().as_secs_f64();
    for (i, (at, rot, _)) in cars(&line, &d, now).into_iter().enumerate() {
        // The two ends are noses (the tail's turned round); the rest coaches.
        let (file, turn) = if i + 1 == d.cars {
            ("surf_train_nose.gltf", Quat::IDENTITY)
        } else if i == 0 {
            (
                "surf_train_nose.gltf",
                Quat::from_rotation_y(std::f32::consts::PI),
            )
        } else {
            ("surf_train_car.gltf", Quat::IDENTITY)
        };
        commands
            .spawn((
                Car(i),
                Transform::from_translation(o + at + rot * Vec3::Y * (d.car.2 / 2.0))
                    .with_rotation(rot),
                RigidBody::Kinematic,
                Collider::cuboid(d.car.1, d.car.2, d.car.0),
                DespawnOnExit(AppState::InGame),
            ))
            .with_child((
                model(&assets, file),
                Transform::from_translation(Vec3::Y * (-d.car.2 / 2.0)).with_rotation(turn),
            ));
    }
}

/// Move the cars to where the clock says they are, with their velocity so
/// riders on the roof go along.
fn glide(mut q: Query<(&Car, &mut Transform, &mut LinearVelocity)>) {
    let t = tuning();
    let d = TRAIN.get();
    let o = hall_at(&t);
    let line = rail(&t, &d);
    let now = crate::settings::since_epoch().as_secs_f64();
    let all = cars(&line, &d, now);
    for (car, mut tf, mut v) in &mut q {
        let Some((at, rot, vel)) = all.get(car.0) else {
            continue;
        };
        tf.translation = o + *at + *rot * Vec3::Y * (d.car.2 / 2.0);
        tf.rotation = *rot;
        v.0 = *vel;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::surf::city;

    fn load() -> (std::sync::Arc<Tuning>, std::sync::Arc<Train>) {
        crate::features::surf_map::load_course_for_tests();
        for path in [crate::rules::surf::TUNING.path(), TRAIN.path()] {
            let full = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), path);
            let Ok(bytes) = std::fs::read(&full) else {
                panic!("no {full}")
            };
            let ok = if path == TRAIN.path() {
                TRAIN.set_bytes(&bytes)
            } else {
                crate::rules::surf::TUNING.set_bytes(&bytes)
            };
            assert!(ok, "{full} doesn't parse");
        }
        (tuning(), TRAIN.get())
    }

    #[test]
    fn data_file_parses_with_every_field() {
        let (_, d) = load();
        assert!(d.cars >= 3 && d.cars <= 5, "{} cars", d.cars);
        assert!(d.speed >= 30.0 && d.speed <= 45.0);
        assert!(d.offset >= 15.0 && d.offset <= 25.0);
        assert!(d.segment > 0.0 && d.pylon_every > 0.0 && d.car.0 > 0.0);
        let text = std::fs::read_to_string(format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TRAIN.path()
        ))
        .unwrap_or_default();
        for f in [
            "stage:",
            "ramp:",
            "side:",
            "offset:",
            "rise:",
            "extend:",
            "beam:",
            "segment:",
            "pylon_every:",
            "pylon_width:",
            "floor:",
            "cars:",
            "car:",
            "car_gap:",
            "speed:",
            "dwell:",
            "claim_margin:",
        ] {
            assert!(text.contains(f), "no {f}");
        }
    }

    #[test]
    fn the_line_runs_beside_its_ramp() {
        let (t, d) = load();
        let line = rail(&t, &d);
        assert!(length(&line) > 200.0, "{} m", length(&line));
        let ridge = ridge(&t.stages[d.stage].ramps[d.ramp], t.ramp_samples * 4);
        for q in &ridge {
            let near = line
                .iter()
                .map(|p| (p - q).with_y(0.0).length())
                .fold(f32::MAX, f32::min);
            assert!((near - d.offset).abs() < 3.0, "{near} m from the ridge");
        }
    }

    #[test]
    fn the_train_moves_at_surf_speed_and_stays_on_the_line() {
        let (t, d) = load();
        let line = rail(&t, &d);
        let range = length(&line) - train_length(&d);
        let mut top = 0.0f32;
        for i in 0..2000 {
            let s = i as f64 * 0.05;
            let (u, v) = travel(range, &d, s);
            assert!((-0.01..=range + 0.01).contains(&u));
            top = top.max(v.abs());
            // Speed matches how far it goes.
            let (u2, _) = travel(range, &d, s + 0.001);
            assert!(((u2 - u) / 0.001 - v).abs() < 1.0 || s % 1.0 > 0.9);
        }
        assert!((top - d.speed).abs() < 1.0, "top speed {top}");
        // The same time, the same place: every peer agrees.
        assert_eq!(cars(&line, &d, 1234.5), cars(&line, &d, 1234.5));
        assert_eq!(cars(&line, &d, 0.0).len(), d.cars);
    }

    #[test]
    fn nothing_blocks_a_ramps_faces() {
        let (t, d) = load();
        let line = rail(&t, &d);
        let ridges: Vec<Vec3> = t
            .stages
            .iter()
            .flat_map(|s| s.ramps.iter().flat_map(|r| ridge(r, t.ramp_samples * 4)))
            .collect();
        let reach = t.ramp_half_width + 3.0;
        // Beam and swept train: every metre along the line.
        let half = d.beam.0.max(d.car.1) / 2.0;
        let total = length(&line);
        let mut u = 0.0;
        while u <= total {
            let p = along(&line, u).0;
            let bottom = p.y - d.beam.1;
            for q in &ridges {
                let flat = (p - *q).with_y(0.0).length();
                assert!(
                    flat > reach + half || bottom > q.y + 3.0,
                    "line at {p} is {flat} m from a ridge at {q}"
                );
            }
            u += 1.0;
        }
        for (top, _) in pylons(&line, &d) {
            for q in &ridges {
                let flat = (top - *q).with_y(0.0).length();
                assert!(
                    flat > reach + d.pylon_width / 2.0,
                    "pylon at {top} is {flat} m from a ridge at {q}"
                );
            }
        }
    }

    #[test]
    fn pylons_miss_the_citys_towers() {
        let (t, d) = load();
        let line = rail(&t, &d);
        let towers = city(&t);
        for (top, _) in pylons(&line, &d) {
            for b in &towers {
                let hit = b.ground_distance(top.xz()) < d.pylon_width && b.top() > d.floor;
                assert!(
                    !hit,
                    "pylon at {top} stands in a {:?} at {}",
                    b.piece, b.centre
                );
            }
        }
    }
}
