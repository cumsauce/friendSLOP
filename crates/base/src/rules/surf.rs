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
    /// Air acceleration (per second) inside the hall (`sv_airaccelerate`).
    pub air_accel_hz: f32,
    /// Cap on the wish speed in the air (m/s; Source's 30 u/s).
    pub air_max_wish: f32,
    /// Top speed inside the hall (m/s, `sv_maxvelocity`).
    pub max_speed: f32,
    /// Gravity inside the hall (m/s^2, `sv_gravity`).
    pub gravity: f32,
    /// Ground acceleration inside the hall (`sv_accelerate`).
    pub ground_accel_hz: f32,
    /// Ground friction inside the hall (`sv_friction` / the surface's 0.5).
    pub friction_hz: f32,
    /// `sv_stopspeed` (m/s).
    pub stop_speed: f32,
    /// Wish speed at full stick inside the hall (m/s; 250 u/s with a knife).
    pub wish_speed: f32,
    pub enter_line: String,
    /// `{secs}`.
    pub wipeout_line: String,
    /// `{secs}`, `{best}`.
    pub finish_line: String,
    /// Seconds the restart button is held to go back to the start.
    pub restart_hold: f32,
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
    /// A checkpoint (a stage's pad, and the finish) counts you this high
    /// above its top (m) and this much past its edges, so flying over it
    /// counts too.
    pub checkpoint_reach: (f32, f32),
    /// The course: each stage's pad and ramps. The first pad is the start.
    /// Not in the data file: it comes from the modeler map (`COURSE`).
    #[serde(default)]
    pub stages: Vec<Stage>,
    /// Speed checks: (stage, ramp), the gap after that ramp of that stage,
    /// cleared only carrying speed ([`speed_needed`]).
    #[serde(default)]
    pub speed_gaps: Vec<(usize, usize)>,
    /// The finish pad (centre, size). From the map.
    #[serde(default)]
    pub finish: Pad,
    /// Solid boxes (centre, size): the pads and their pillars. From the map's
    /// `course_block` markers.
    #[serde(default)]
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
/// Space set pieces have taken for themselves (hall coordinates: centre,
/// half size): the city and the generated props keep out of it.
static CLAIMS: std::sync::RwLock<Vec<(Vec3, Vec3)>> = std::sync::RwLock::new(Vec::new());

/// Keep the city and the props out of a box (hall coordinates: centre,
/// half size). Set pieces call it from their plugin.
pub fn claim(centre: Vec3, half: Vec3) {
    if let Ok(mut c) = CLAIMS.write()
        && !c.contains(&(centre, half))
    {
        c.push((centre, half));
    }
}

/// Is a point (with `reach` metres around it) in a claimed box?
pub fn claimed(p: Vec3, reach: f32) -> bool {
    CLAIMS.read().is_ok_and(|c| {
        c.iter()
            .any(|(m, h)| ((p - *m).abs() - *h).max_element() < reach)
    })
}

/// Does a box (centre, half size; turned any way about y) touch a claimed box?
pub fn claimed_box(centre: Vec3, half: Vec3) -> bool {
    let r = Vec2::new(half.x, half.z).length();
    CLAIMS.read().is_ok_and(|c| {
        c.iter().any(|(m, h)| {
            let d = (centre - *m).abs();
            d.x < h.x + r && d.z < h.z + r && d.y < h.y + half.y
        })
    })
}

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
    // Nothing in a set piece's own space (the slalom, the skyscraper
    // complex, the sky train's line): see [`claim`].
    out.retain(|b| !claimed_box(b.centre, b.size / 2.0));
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

/// A gap between the end of one ramp and the start of the next (hall
/// coordinates): you leave `from` heading along `dir` (across the ground,
/// unit) and dropping `pitch` (tan of the angle down), and land on the next
/// ramp's face, whose ridge starts at `to`.
#[derive(Debug, Clone, Copy)]
pub struct Gap {
    pub from: Vec3,
    pub to: Vec3,
    pub dir: Vec2,
    pub pitch: f32,
    /// Distance across the ground along `dir` to the next ramp's start.
    pub length: f32,
    /// How far the next ramp's start is off that line (m).
    pub lateral: f32,
    /// How far the next ridge starts below this one ends (m).
    pub drop: f32,
}

/// The gap after ramp `ramp` of stage `stage`.
pub fn gap(t: &Tuning, stage: usize, ramp: usize) -> Option<Gap> {
    let ramps = &t.stages.get(stage)?.ramps;
    let (a, b) = (ramps.get(ramp)?, ramps.get(ramp + 1)?);
    let r = ridge(a, t.ramp_samples);
    let (p, q) = (*r.get(r.len().checked_sub(2)?)?, *r.last()?);
    let to = Vec3::from(*b.first()?);
    let run = (q - p).xz();
    let dir = run.normalize_or_zero();
    if dir == Vec2::ZERO {
        return None;
    }
    let off = (to - q).xz();
    Some(Gap {
        from: q,
        to,
        dir,
        pitch: (p.y - q.y) / run.length(),
        length: off.dot(dir),
        lateral: off.perp_dot(dir).abs(),
        drop: q.y - to.y,
    })
}

/// How far below the next ridge (m) you may be when you reach it and still
/// land: the upper half of its face.
pub fn landing_depth(t: &Tuning) -> f32 {
    t.ramp_height / 2.0
}

/// Do you clear the gap leaving at `speed` (m/s, along the ramp's end)?
/// A ballistic flight from the ridge's end: you clear it if you reach the
/// next ramp no more than [`landing_depth`] below its ridge.
pub fn clears(t: &Tuning, g: &Gap, speed: f32) -> bool {
    let cos = 1.0 / (1.0 + g.pitch * g.pitch).sqrt();
    let across = speed * cos;
    if across <= 0.0 || g.length <= 0.0 {
        return g.length <= 0.0;
    }
    let time = g.length / across;
    let y = g.from.y - speed * cos * g.pitch * time - t.gravity * time * time / 2.0;
    y >= g.to.y - landing_depth(t)
}

/// The least speed (m/s) that clears the gap, or `None` if no speed does.
pub fn speed_needed(t: &Tuning, g: &Gap) -> Option<f32> {
    // y(L) = from - pitch L - g L^2 / (2 v^2 cos^2) >= to - depth.
    let room = g.drop + landing_depth(t) - g.pitch * g.length;
    (room > 0.0).then(|| {
        let cos2 = 1.0 / (1.0 + g.pitch * g.pitch);
        (t.gravity * g.length * g.length / (2.0 * cos2 * room)).sqrt()
    })
}

/// The most you could be carrying at hall height `y` on a stage: off its
/// pad at the run speed and everything the drop since gives you (no
/// losses). A clean run gets most of it.
pub fn ideal_speed(t: &Tuning, stage: usize, y: f32) -> Option<f32> {
    let (c, s) = pad(t.stages.get(stage)?.pad);
    let fall = (c.y + s.y / 2.0 - y).max(0.0);
    Some((t.wish_speed * t.wish_speed + 2.0 * t.gravity * fall).sqrt())
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

/// Are feet at this hall-coordinate point standing on something flat: the
/// top of a stage pad, the finish or a solid block? (Ramps are too steep to
/// stand on: sliding down one is surfing, which counts as in the air.)
pub fn standing(t: &Tuning, local: Vec3) -> bool {
    t.stages
        .iter()
        .map(|s| s.pad)
        .chain(std::iter::once(t.finish))
        .chain(t.blocks.iter().copied())
        .map(pad)
        .any(|(c, s)| {
            let top = c.y + s.y / 2.0;
            (local.x - c.x).abs() <= s.x / 2.0
                && (local.z - c.z).abs() <= s.z / 2.0
                && (top - 0.3..top + 0.25).contains(&local.y)
        })
}

/// Is a hall-coordinate point passing through this pad's checkpoint: over
/// the pad (a little past its edges) and anywhere up to `reach.0` metres
/// above its top?
pub fn through_checkpoint(p: (Vec3, Vec3), local: Vec3, reach: (f32, f32)) -> bool {
    let (c, s) = p;
    let top = c.y + s.y / 2.0;
    (local.x - c.x).abs() <= s.x / 2.0 + reach.1
        && (local.z - c.z).abs() <= s.z / 2.0 + reach.1
        && (top - 0.3..top + reach.0).contains(&local.y)
}

#[cfg(test)]
mod checkpoint_tests {
    use super::*;

    #[test]
    fn flying_over_a_checkpoint_counts() {
        let p = (Vec3::ZERO, Vec3::new(16.0, 1.0, 16.0));
        let reach = (30.0, 3.0);
        // Standing on it, high over it, just past its edge.
        assert!(through_checkpoint(p, Vec3::new(0.0, 0.5, 0.0), reach));
        assert!(through_checkpoint(p, Vec3::new(4.0, 25.0, 0.0), reach));
        assert!(through_checkpoint(p, Vec3::new(10.0, 3.0, 0.0), reach));
        // Too high, too far out, or under it.
        assert!(!through_checkpoint(p, Vec3::new(0.0, 40.0, 0.0), reach));
        assert!(!through_checkpoint(p, Vec3::new(14.0, 3.0, 0.0), reach));
        assert!(!through_checkpoint(p, Vec3::new(0.0, -2.0, 0.0), reach));
    }
}

/// One step of a body's stage for feet at `local` (hall coordinates): the
/// stage it's on now, and whether it fell off (back to that stage's pad).
pub fn stage_step(t: &Tuning, stage: usize, local: Vec3) -> (usize, bool) {
    let reached = (0..t.stages.len()).rev().find(|&i| {
        i > stage && through_checkpoint(pad(t.stages[i].pad), local, t.checkpoint_reach)
    });
    let stage = reached.unwrap_or(stage);
    let fell = stage_low(t, stage).is_some_and(|low| local.y < low - t.stage_fall);
    (stage, fell)
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/surf.ron");

/// The course as the modeler made it: one map per area, in course order
/// (`maps/surf*.map.json`). Each has `Pad`, `Ridge`, `SpeedGap` and
/// `course_block` markers; [`tuning`] puts them together. A later map's stage
/// replaces an earlier map's with the same number, and the last map with a
/// finish pad ends the course.
pub static COURSE: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/surf.map.json");
pub static COURSE_OFFICE: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/surf_office.map.json");
pub static COURSE_MALL: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/surf_mall.map.json");
pub static COURSE_NEON: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/surf_neon.map.json");
pub static COURSE_OVERGROWN: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/surf_overgrown.map.json");
pub static COURSE_FOUNDRY: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/surf_foundry.map.json");

/// Every map of the course, in order.
pub fn courses() -> [&'static friendslop_data::Data<friendslop_world::places::MapFile>; 6] {
    [
        &COURSE,
        &COURSE_OFFICE,
        &COURSE_MALL,
        &COURSE_NEON,
        &COURSE_OVERGROWN,
        &COURSE_FOUNDRY,
    ]
}

/// What one map says about the course.
#[derive(Debug, Default, Clone)]
pub struct CourseParts {
    /// Stages by their number (a stage is its pad and its ramps).
    pub stages: std::collections::BTreeMap<u32, Stage>,
    /// The finish pad, if this map has it.
    pub finish: Option<Pad>,
    /// Solid boxes: the pads and their pillars (`course_block` markers).
    pub blocks: Vec<Pad>,
    /// Speed-check gaps: (stage, ramp).
    pub gaps: Vec<(usize, usize)>,
}

/// One map's course out of its markers.
pub fn course_parts(map: &friendslop_world::places::MapFile) -> CourseParts {
    let boxed = |e: &friendslop_world::places::MapEntity| -> Pad {
        (
            (e.translation[0], e.translation[1], e.translation[2]),
            e.size.map_or((1.0, 1.0, 1.0), |s| (s[0], s[1], s[2])),
        )
    };
    let mut out = CourseParts::default();
    let mut pads: Vec<(u32, Pad)> = Vec::new();
    let mut ridge: Vec<(u32, u32, u32, (f32, f32, f32))> = Vec::new();
    for e in &map.entities {
        if e.has("Pad") {
            if e.flag("Pad", "finish") {
                out.finish = Some(boxed(e));
            } else {
                pads.push((e.num("Pad", "stage") as u32, boxed(e)));
            }
        }
        if e.has("Ridge") {
            let n = |f: &str| e.num("Ridge", f) as u32;
            let p = (e.translation[0], e.translation[1], e.translation[2]);
            ridge.push((n("stage"), n("ramp"), n("order"), p));
        }
        if e.has("SpeedGap") {
            out.gaps.push((
                e.num("SpeedGap", "stage") as usize,
                e.num("SpeedGap", "ramp") as usize,
            ));
        }
        if e.kind == "course_block" {
            out.blocks.push(boxed(e));
        }
    }
    ridge.sort_by_key(|r| (r.0, r.1, r.2));
    for (i, pad) in pads {
        let mut ramps: Vec<Vec<(f32, f32, f32)>> = Vec::new();
        let mut last = None;
        for r in ridge.iter().filter(|r| r.0 == i) {
            if last != Some(r.1) {
                ramps.push(Vec::new());
                last = Some(r.1);
            }
            if let Some(v) = ramps.last_mut() {
                v.push(r.3);
            }
        }
        out.stages.insert(i, Stage { pad, ramps });
    }
    out
}

/// The tuning with the course from the maps (rebuilt only when a file is
/// re-read).
pub fn tuning() -> std::sync::Arc<Tuning> {
    use std::sync::{Arc, Mutex};
    type Merged = (
        Arc<Tuning>,
        Vec<Arc<friendslop_world::places::MapFile>>,
        Arc<Tuning>,
    );
    static CACHE: Mutex<Option<Merged>> = Mutex::new(None);
    let ron = TUNING.get();
    let maps: Vec<_> = courses().iter().map(|c| c.get()).collect();
    let Ok(mut cache) = CACHE.lock() else {
        return ron;
    };
    if let Some((r, m, merged)) = cache.as_ref()
        && Arc::ptr_eq(r, &ron)
        && m.len() == maps.len()
        && m.iter().zip(&maps).all(|(a, b)| Arc::ptr_eq(a, b))
    {
        return merged.clone();
    }
    let mut t = (*ron).clone();
    if t.stages.is_empty() {
        let mut stages = std::collections::BTreeMap::new();
        for map in &maps {
            let c = course_parts(map);
            stages.extend(c.stages);
            if let Some(f) = c.finish {
                t.finish = f;
            }
            t.blocks.extend(c.blocks);
            t.speed_gaps.extend(c.gaps);
        }
        t.stages = stages.into_values().collect();
    }
    let merged = Arc::new(t);
    *cache = Some((ron, maps, merged.clone()));
    merged
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

/// One Hammer unit in metres (a CS player is 72 u tall).
pub const HAMMER_UNIT: f32 = 0.0254;

/// Quake/Source `PM_AirAccelerate`, as ahoy's air move does it: only the
/// part of `velocity` along `wish_dir` is tested against the capped wish
/// speed, so steering across your velocity (strafing while turning) always
/// adds speed, while pushing along it adds almost nothing. `wish_dir` is a
/// unit vector (or zero).
pub fn air_accelerate(
    velocity: Vec3,
    wish_dir: Vec3,
    wish_speed: f32,
    max_wish: f32,
    accel_hz: f32,
    dt: f32,
) -> Vec3 {
    let capped = wish_speed.min(max_wish);
    let add = capped - velocity.dot(wish_dir);
    if add <= 0.0 {
        return velocity;
    }
    velocity + wish_dir * (accel_hz * wish_speed * dt).min(add)
}

/// Source's `ClipVelocity`: what's left of `velocity` after hitting a plane
/// with this (unit) `normal`. `overbounce` 1 slides along it, keeping every
/// bit of speed along the plane: a surf ramp turns falling into gliding.
pub fn clip_velocity(velocity: Vec3, normal: Vec3, overbounce: f32) -> Vec3 {
    let into = velocity.dot(normal);
    if into >= 0.0 {
        return velocity;
    }
    velocity - normal * into * overbounce
}

/// The walker's air control for a body with feet at `feet`.
pub fn air_at(feet: Vec3) -> crate::kcc::Air {
    let t = tuning();
    if in_hall(&t, feet).is_some() && t.max_speed > 0.0 {
        crate::kcc::Air {
            accel_hz: t.air_accel_hz,
            max_wish_speed: t.air_max_wish,
            max_speed: t.max_speed,
            gravity: t.gravity,
            ground_accel_hz: t.ground_accel_hz,
            friction_hz: t.friction_hz,
            stop_speed: t.stop_speed,
            wish_speed: t.wish_speed,
        }
    } else {
        crate::kcc::Air::NORMAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn air_accel_is_capped_along_the_wish_direction() {
        // Pushing straight ahead in the air adds at most the 30 u/s cap.
        let mut v = Vec3::ZERO;
        for _ in 0..600 {
            v = air_accelerate(v, Vec3::NEG_Z, 6.35, 0.762, 150.0, DT);
        }
        assert!((v.length() - 0.762).abs() < 1e-4, "{v}");
        // Already faster than the cap that way: nothing.
        let fast = Vec3::NEG_Z * 10.0;
        assert_eq!(
            air_accelerate(fast, Vec3::NEG_Z, 6.35, 0.762, 150.0, DT),
            fast
        );
    }

    #[test]
    fn strafing_sideways_adds_speed_every_tick() {
        // The wish direction square to your velocity: each tick adds up to
        // the cap, so speed grows without bound (sv_airaccelerate 150).
        let v0 = Vec3::NEG_Z * 10.0;
        let v1 = air_accelerate(v0, Vec3::X, 6.35, 0.762, 150.0, DT);
        assert!(v1.length() > v0.length());
        assert!((v1.x - 0.762).abs() < 1e-4, "one tick adds the cap: {v1}");
    }

    #[test]
    fn turning_with_the_strafe_gains_speed_like_cs() {
        // Hold strafe right and turn right at a steady rate, keeping the
        // wish direction just over 90 degrees off velocity: the classic
        // air strafe. With sv_airaccelerate this high a tick can add the
        // whole 30 u/s cap, so square to velocity is the best angle and
        // speed^2 grows by cap^2 a tick: 6.35 -> ~14.6 m/s in five seconds.
        let mut v = Vec3::NEG_Z * 6.35;
        for _ in 0..300 {
            let along = v.normalize();
            let right = Vec3::new(-along.z, 0.0, along.x);
            v = air_accelerate(v, right, 6.35, 0.762, 150.0, DT);
        }
        assert!(v.length() > 14.0, "{}", v.length());
    }

    #[test]
    fn clipping_onto_a_ramp_keeps_the_speed_along_it() {
        // A 59 degree face leaning towards +x; falling straight down onto it.
        let a = 59f32.to_radians();
        let n = Vec3::new(a.sin(), a.cos(), 0.0);
        let v = Vec3::new(0.0, -10.0, -20.0);
        let c = clip_velocity(v, n, 1.0);
        assert!(c.dot(n).abs() < 1e-4, "on the plane: {c}");
        assert_eq!(c.z, -20.0, "speed along the ramp is kept");
        assert!(c.x > 0.0 && c.y < 0.0, "sliding down and across: {c}");
        // Moving away from it: untouched.
        let away = Vec3::new(5.0, 1.0, 0.0);
        assert_eq!(clip_velocity(away, n, 1.0), away);
    }

    #[test]
    fn surf_numbers_come_from_hammer_units() {
        assert!((30.0 * HAMMER_UNIT - 0.762).abs() < 1e-6);
        assert!((800.0 * HAMMER_UNIT - 20.32).abs() < 1e-4);
        assert!((3500.0 * HAMMER_UNIT - 88.9).abs() < 1e-3);
    }

    #[test]
    fn the_data_file_has_the_surf_movement() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        let t = TUNING.get();
        assert!(t.air_accel_hz >= 100.0 && t.air_max_wish > 0.5 && t.air_max_wish < 1.0);
        assert!(t.gravity > 15.0 && t.ground_accel_hz > 0.0 && t.friction_hz > 0.0);
        assert!(t.stop_speed > 0.0 && t.wish_speed > 5.0 && t.max_speed > 45.0);
    }
    #[test]
    fn speed_gaps_need_speed_but_not_too_much() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        for c in courses() {
            let course = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), c.path());
            if let Ok(bytes) = std::fs::read(&course) {
                assert!(c.set_bytes(&bytes), "{course} doesn't parse");
            }
        }
        let t = tuning();
        assert!(t.speed_gaps.len() >= 3);
        // Fly it tick by tick: from the ramp's end along its ridge.
        let fly = |g: &Gap, speed: f32| {
            let cos = 1.0 / (1.0 + g.pitch * g.pitch).sqrt();
            let mut v = Vec3::new(g.dir.x * cos, -g.pitch * cos, g.dir.y * cos) * speed;
            let mut p = g.from;
            let dt = 1.0 / 240.0;
            while (p - g.from).xz().dot(g.dir) < g.length {
                v.y -= t.gravity * dt;
                p += v * dt;
            }
            p.y >= g.to.y - landing_depth(&t)
        };
        for &(stage, ramp) in &t.speed_gaps {
            let Some(g) = gap(&t, stage, ramp) else {
                panic!("no gap after stage {stage} ramp {ramp}");
            };
            let Some(need) = speed_needed(&t, &g) else {
                panic!("stage {stage} ramp {ramp}: no speed clears it");
            };
            let Some(ideal) = ideal_speed(&t, stage, g.from.y) else {
                panic!("stage {stage}");
            };
            let at = format!("stage {stage} ramp {ramp}: {g:?} needs {need} (ideal {ideal})");
            assert!(fly(&g, need * 1.02) && clears(&t, &g, need * 1.02), "{at}");
            assert!(!fly(&g, need * 0.6) && !clears(&t, &g, need * 0.6), "{at}");
            // A real gap, mostly straight ahead, and it takes a clean line:
            // more than half what the drop could give you, but not all of it.
            assert!(
                g.length > 15.0 && g.lateral < t.ramp_half_width * 0.75,
                "{at}"
            );
            assert!(need > ideal * 0.5 && need < ideal * 0.75, "{at}");
            assert!(need < t.max_speed, "{at}");
        }
    }
}
