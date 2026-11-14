//! The surf map (#306): where its hall is and how the air feels in it. The
//! walker (`play::fps`) reads [`air_at`]; the feature (`side::surf`) builds
//! the hall and moves bodies in and out.

use bevy::prelude::*;
use serde::Deserialize;

/// Tuning and writing: `assets/data/side/surf.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The brownstone's front door (x, z).
    pub door: (f32, f32),
    /// How close (m) to the door takes you in.
    pub door_reach: f32,
    /// The neon sign over the door (x, y, z).
    pub sign_at: (f32, f32, f32),
    /// Where you come back out (x, z).
    pub street: (f32, f32),
    /// The start pad's top (x, y, z): the hall's origin.
    pub hall_at: (f32, f32, f32),
    /// Fall this far below the start pad and you're back on it.
    pub fall_below: f32,
    /// Air acceleration (per second) inside the hall.
    pub air_accel_hz: f32,
    /// Speed you may add towards where you steer in the air (m/s).
    pub air_max_wish: f32,
    /// Top speed inside the hall (m/s).
    pub max_speed: f32,
    pub enter_line: String,
    /// `{secs}`.
    pub wipeout_line: String,
    /// `{secs}`, `{best}`.
    pub finish_line: String,
    pub exit_label: String,
    /// The hall's inside relative to its origin (min, max).
    pub hall: ((f32, f32, f32), (f32, f32, f32)),
    /// Every ramp's half width and height (m).
    pub ramp_half_width: f32,
    pub ramp_height: f32,
    /// Pieces each span between two ridge points is cut into.
    pub ramp_samples: u32,
    /// Fall this far below your stage's lowest ridge and you're back on its pad.
    pub stage_fall: f32,
    /// The course: each stage's pad and ramps. The first pad is the start.
    pub stages: Vec<Stage>,
    /// The finish pad (centre, size).
    pub finish: Pad,
    /// Solid boxes (centre, size): pads, towers, the skyline.
    pub blocks: Vec<Pad>,
}

/// A box: (centre, size), hall coordinates.
pub type Pad = ((f32, f32, f32), (f32, f32, f32));

/// One stage of the course: its pad (a checkpoint) and its ramps, each the
/// ridge's points (a smooth curve through them).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Stage {
    pub pad: Pad,
    pub ramps: Vec<Vec<(f32, f32, f32)>>,
}

/// A pad as (centre, size) vectors.
pub fn pad(p: Pad) -> (Vec3, Vec3) {
    (Vec3::from(p.0), Vec3::from(p.1))
}

/// The ridge's points, smoothed (uniform Catmull-Rom, `samples` per span).
pub fn ridge(points: &[(f32, f32, f32)], samples: u32) -> Vec<Vec3> {
    let p: Vec<Vec3> = points.iter().map(|&q| Vec3::from(q)).collect();
    let n = p.len();
    let samples = samples.max(1);
    let mut out = Vec::new();
    for i in 0..n.saturating_sub(1) {
        let (p0, p1, p2, p3) = (
            p[i.saturating_sub(1)],
            p[i],
            p[i + 1],
            p[(i + 2).min(n - 1)],
        );
        for s in 0..samples {
            let t = s as f32 / samples as f32;
            let (t2, t3) = (t * t, t * t * t);
            out.push(
                0.5 * (2.0 * p1
                    + (p2 - p0) * t
                    + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
                    + (3.0 * p1 - p0 - 3.0 * p2 + p3) * t3),
            );
        }
    }
    out.extend(p.last());
    out
}

/// Cross-sections along a ramp: the ridge point and its two face bottoms,
/// square to the ridge across the ground.
pub fn joints(
    points: &[(f32, f32, f32)],
    samples: u32,
    half_width: f32,
    height: f32,
) -> Vec<[Vec3; 3]> {
    let r = ridge(points, samples);
    let n = r.len();
    (0..n)
        .map(|i| {
            let d = r[(i + 1).min(n - 1)] - r[i.saturating_sub(1)];
            let side = Vec3::new(d.z, 0.0, -d.x).normalize_or(Vec3::X) * half_width;
            let down = Vec3::Y * height;
            [r[i], r[i] + side - down, r[i] - side - down]
        })
        .collect()
}

/// A ramp as convex pieces (six corners each), for colliders.
pub fn pieces(
    points: &[(f32, f32, f32)],
    samples: u32,
    half_width: f32,
    height: f32,
) -> Vec<[Vec3; 6]> {
    joints(points, samples, half_width, height)
        .windows(2)
        .map(|w| [w[0][0], w[0][1], w[0][2], w[1][0], w[1][1], w[1][2]])
        .collect()
}

/// The lowest ridge point of a stage (hall coordinates).
pub fn stage_low(t: &Tuning, stage: usize) -> Option<f32> {
    let s = t.stages.get(stage)?;
    s.ramps
        .iter()
        .flat_map(|r| r.iter().map(|p| p.1))
        .reduce(f32::min)
}

/// Where you stand on a stage's pad (world).
pub fn stage_spawn(t: &Tuning, stage: usize) -> Option<Vec3> {
    let (c, s) = pad(t.stages.get(stage)?.pad);
    Some(hall_at(t) + c + Vec3::Y * (s.y / 2.0 + 0.05))
}

/// Is a hall-coordinate point standing on this pad?
pub fn on_pad(p: (Vec3, Vec3), local: Vec3) -> bool {
    let (c, s) = p;
    let top = c.y + s.y / 2.0;
    (local.x - c.x).abs() <= s.x / 2.0
        && (local.z - c.z).abs() <= s.z / 2.0
        && (top - 0.3..top + 2.0).contains(&local.y)
}

/// One step of a body's stage for feet at `local` (hall coordinates): the
/// stage it's on now, and whether it fell off (back to that stage's pad).
pub fn stage_step(t: &Tuning, stage: usize, local: Vec3) -> (usize, bool) {
    let reached = (0..t.stages.len())
        .rev()
        .find(|&i| i > stage && on_pad(pad(t.stages[i].pad), local));
    let stage = reached.unwrap_or(stage);
    let fell = stage_low(t, stage).is_some_and(|low| local.y < low - t.stage_fall);
    (stage, fell)
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/surf.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The hall's origin (the start pad's top, middle of its front edge).
pub fn hall_at(t: &Tuning) -> Vec3 {
    let (x, y, z) = t.hall_at;
    Vec3::new(x, y, z)
}

/// A point relative to the hall, if it's inside it (the box in the data
/// file, around `surf-hall.gltf`).
pub fn in_hall(t: &Tuning, p: Vec3) -> Option<Vec3> {
    let local = p - hall_at(t);
    let (min, max) = (Vec3::from(t.hall.0), Vec3::from(t.hall.1));
    (local.cmpge(min).all() && local.cmple(max).all()).then_some(local)
}

/// Is this body in the surf course? It's far bigger than the world, so the
/// world's edge doesn't hold it there.
pub fn unbounded(feet: Vec3) -> bool {
    in_hall(&tuning(), feet).is_some()
}

/// The walker's air control for a body with feet at `feet`.
pub fn air_at(feet: Vec3) -> friendslop_kcc::Air {
    let t = tuning();
    if in_hall(&t, feet).is_some() && t.max_speed > 0.0 {
        friendslop_kcc::Air {
            accel_hz: t.air_accel_hz,
            max_wish_speed: t.air_max_wish,
            max_speed: t.max_speed,
        }
    } else {
        friendslop_kcc::Air::NORMAL
    }
}
