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
    /// The city the course weaves through: towers, building sites, cranes.
    pub city: City,
}

/// How the city around the course is laid out (generated from the ridges,
/// seeded, so every peer builds the same one).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct City {
    pub seed: u32,
    /// A building every this many metres along each ramp (min, max).
    pub every: (f32, f32),
    /// Gap (m) between a ramp's face and a tall tower's wall (min, max).
    pub gap: (f32, f32),
    /// A tower's width and depth (min, max).
    pub size: (f32, f32),
    /// How far a tall tower's roof rises above the ridge beside it (min, max).
    pub rise: (f32, f32),
    /// Where the towers stand on (hall y): their feet.
    pub floor: f32,
    /// Chance a spot is a building site whose roof is just under the ramp's
    /// side instead: fall off onto it and kick back up off its ramp.
    pub site_chance: f32,
    /// How far (m) a site's roof sits below the ramp's face bottoms (min, max).
    pub site_drop: (f32, f32),
    /// The kicker on a site's roof: length, width, height.
    pub kicker: (f32, f32, f32),
    /// Chance a tall tower has a crane on its roof.
    pub crane_chance: f32,
    /// A crane's mast height and jib length.
    pub crane: (f32, f32),
    /// Chance a spot gets towers on both sides with a girder across, over the ramp.
    pub girder_chance: f32,
    /// How far (m) above the ridge a girder's underside stays.
    pub girder_clear: f32,
    /// Keep every tower this far (m) from a pad's middle.
    pub pad_clear: f32,
}

/// What a city piece is (for its look).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Piece {
    /// A glass tower, `style` picks its colours.
    Tower { style: u32 },
    /// A building site's concrete deck.
    Site,
    /// A wedge on a site's roof rising along +z of its box.
    Kicker,
    /// Steel: crane masts, jibs, girders.
    Steel,
    /// A pallet or a stack of rebar on a deck.
    Clutter,
}

/// One box of the city (hall coordinates): centre, size, yaw about y.
#[derive(Debug, Clone, Copy)]
pub struct Block {
    pub piece: Piece,
    pub centre: Vec3,
    pub size: Vec3,
    pub yaw: f32,
}

impl Block {
    /// Distance across the ground from `p` to this box's footprint.
    pub fn ground_distance(&self, p: Vec2) -> f32 {
        let d = p - self.centre.xz();
        let (s, c) = self.yaw.sin_cos();
        // Into the box's frame (inverse of rotating its +x/+z by yaw).
        let local = Vec2::new(c * d.x - s * d.y, s * d.x + c * d.y);
        let q = local.abs() - Vec2::new(self.size.x, self.size.z) / 2.0;
        q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.0)
    }
    pub fn top(&self) -> f32 {
        self.centre.y + self.size.y / 2.0
    }
    pub fn bottom(&self) -> f32 {
        self.centre.y - self.size.y / 2.0
    }
    pub fn rotation(&self) -> Quat {
        Quat::from_rotation_y(self.yaw)
    }
    /// A kicker's six corners (a wedge low at -z, high at +z).
    pub fn wedge(&self) -> [Vec3; 6] {
        let (hx, hy, hz) = (self.size.x / 2.0, self.size.y / 2.0, self.size.z / 2.0);
        [
            Vec3::new(-hx, -hy, -hz),
            Vec3::new(hx, -hy, -hz),
            Vec3::new(-hx, -hy, hz),
            Vec3::new(hx, -hy, hz),
            Vec3::new(-hx, hy, hz),
            Vec3::new(hx, hy, hz),
        ]
    }
}

/// A small seeded hash: the same city on every peer.
fn hash(seed: u32, i: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ i.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// The city around the course: tall towers you weave between, building
/// sites just under the ramps' sides with kickers, cranes and girders over
/// the ramps. Nothing comes within reach of a ramp it would block.
pub fn city(t: &Tuning) -> Vec<Block> {
    let c = &t.city;
    let mut n = 0u32;
    let mut rnd = |lo: f32, hi: f32| {
        n += 1;
        lo + (hi - lo) * hash(c.seed, n)
    };
    let ridges: Vec<Vec<Vec3>> = t
        .stages
        .iter()
        .flat_map(|s| s.ramps.iter().map(|r| ridge(r, t.ramp_samples)))
        .collect();
    let all: Vec<Vec3> = ridges.iter().flatten().copied().collect();
    let pads: Vec<Vec3> = t
        .stages
        .iter()
        .map(|s| Vec3::from(s.pad.0))
        .chain([Vec3::from(t.finish.0)])
        .collect();
    let (hw, rh) = (t.ramp_half_width, t.ramp_height);
    let mut out: Vec<Block> = Vec::new();
    // A box is fine if no ramp it reaches up to comes within `gap` of it,
    // no pad is near and it doesn't overlap what's there.
    let fits = |b: &Block, out: &[Block], gap: f32| {
        all.iter()
            .all(|q| q.y - rh > b.top() + 1.0 || b.ground_distance(q.xz()) > hw + gap)
            && pads.iter().all(|p| b.ground_distance(p.xz()) > c.pad_clear)
            && out
                .iter()
                .filter(|o| !matches!(o.piece, Piece::Steel | Piece::Clutter | Piece::Kicker))
                .all(|o| {
                    o.centre.xz().distance(b.centre.xz())
                        > (o.size.xz().max_element() + b.size.xz().max_element()) * 0.55
                })
    };
    let floor = c.floor;
    let column = |at: Vec2, top: f32, size: Vec2, yaw: f32, piece| Block {
        piece,
        centre: Vec3::new(at.x, (top + floor) / 2.0, at.y),
        size: Vec3::new(size.x, top - floor, size.y),
        yaw,
    };
    for r in &ridges {
        let mut walked = 0.0;
        let mut next = rnd(0.0, c.every.1);
        let mut side = if rnd(0.0, 1.0) < 0.5 { 1.0 } else { -1.0 };
        for w in r.windows(2) {
            walked += w[0].distance(w[1]);
            if walked < next {
                continue;
            }
            next = walked + rnd(c.every.0, c.every.1);
            let p = w[0];
            let d = (w[1] - w[0]).xz().normalize_or(Vec2::NEG_Y);
            let across = Vec2::new(-d.y, d.x);
            // Boxes' +z runs along the ramp.
            let yaw = d.x.atan2(d.y);
            side = -side;
            let size = Vec2::new(rnd(c.size.0, c.size.1), rnd(c.size.0, c.size.1));
            let roll = rnd(0.0, 1.0);
            if roll < c.site_chance {
                // A building site just under the ramp's side.
                let top = p.y - rh - rnd(c.site_drop.0, c.site_drop.1);
                let at = p.xz() + across * side * (hw + size.x / 2.0 - 1.0);
                let b = column(at, top, size, yaw, Piece::Site);
                if !fits(&b, &out, 0.0) {
                    continue;
                }
                out.push(b);
                let (kl, kw, kh) = c.kicker;
                let k = Block {
                    piece: Piece::Kicker,
                    centre: Vec3::new(
                        at.x + d.x * size.y * 0.12,
                        top + kh / 2.0,
                        at.y + d.y * size.y * 0.12,
                    ),
                    size: Vec3::new(kw, kh, kl),
                    yaw,
                };
                out.push(k);
                for i in 0..3 {
                    let off = across * rnd(-0.35, 0.35) * size.x + d * rnd(-0.45, -0.25) * size.y;
                    let s = Vec3::new(rnd(1.0, 2.2), rnd(0.4, 1.4), rnd(1.0, 3.0));
                    out.push(Block {
                        piece: Piece::Clutter,
                        centre: Vec3::new(at.x + off.x, top + s.y / 2.0, at.y + off.y),
                        size: s,
                        yaw: yaw + i as f32 * 0.4,
                    });
                }
                continue;
            }
            let girder = roll > 1.0 - c.girder_chance;
            let sides: &[f32] = if girder { &[1.0, -1.0] } else { &[side] };
            let mut tops = Vec::new();
            for &s in sides {
                let gap = rnd(c.gap.0, c.gap.1);
                let rise =
                    rnd(c.rise.0, c.rise.1).max(if girder { c.girder_clear + 6.0 } else { 0.0 });
                let at = p.xz() + across * s * (hw + gap + size.x / 2.0);
                let b = column(
                    at,
                    p.y + rise,
                    size,
                    yaw,
                    Piece::Tower {
                        style: (rnd(0.0, 4.0)) as u32,
                    },
                );
                if !fits(&b, &out, c.gap.0) {
                    continue;
                }
                out.push(b);
                tops.push((at, b.top()));
                if !girder && rnd(0.0, 1.0) < c.crane_chance {
                    let (mast, jib) = c.crane;
                    let foot = Vec3::new(at.x, b.top(), at.y);
                    out.push(Block {
                        piece: Piece::Steel,
                        centre: foot + Vec3::Y * mast / 2.0,
                        size: Vec3::new(1.6, mast, 1.6),
                        yaw,
                    });
                    let swing = rnd(0.0, std::f32::consts::TAU);
                    let dir = Vec2::from_angle(swing);
                    let mid = foot.xz() + dir * (jib / 2.0 - jib * 0.2);
                    out.push(Block {
                        piece: Piece::Steel,
                        centre: Vec3::new(mid.x, foot.y + mast + 0.6, mid.y),
                        size: Vec3::new(1.2, 1.2, jib),
                        yaw: dir.x.atan2(dir.y),
                    });
                }
            }
            if let [(a, ta), (b, tb)] = tops[..] {
                let y = ta.min(tb) - 1.5;
                let mid = (a + b) / 2.0;
                let g = Block {
                    piece: Piece::Steel,
                    centre: Vec3::new(mid.x, y, mid.y),
                    size: Vec3::new(a.distance(b), 1.4, 1.0),
                    yaw,
                };
                let clear = all
                    .iter()
                    .all(|q| g.ground_distance(q.xz()) > 1.0 || q.y + c.girder_clear < g.bottom());
                if clear {
                    out.push(g);
                }
            }
        }
    }
    out
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
pub fn air_at(feet: Vec3) -> crate::kcc::Air {
    let t = tuning();
    if in_hall(&t, feet).is_some() && t.max_speed > 0.0 {
        crate::kcc::Air {
            accel_hz: t.air_accel_hz,
            max_wish_speed: t.air_max_wish,
            max_speed: t.max_speed,
        }
    } else {
        crate::kcc::Air::NORMAL
    }
}
