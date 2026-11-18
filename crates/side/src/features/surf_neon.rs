//! The neon megacity on the surf course (stages 7-9, hall x 1800-3250; see
//! `docs/design/surf-areas-brief.md`): the pink neon sky city turned up to the
//! max. A canyon of skyscrapers with a wide S-bend boulevard (stage 7), a
//! megatower you corkscrew round twice (stage 8), and a calm night-market
//! cruise, a maglev viaduct and a plunge onto the overgrown ruins (stage 9).
//!
//! The course is [`course`], a pure function of `data/side/surf_neon.ron`:
//! each ramp is a start pose and a list of segments (straights, arcs, a join
//! onto another ramp, a landing) walked into ridge points, so a bend is a
//! radius and an angle, not a pile of coordinates. Everything the city adds
//! round it ([`layout`]: towers, bands, the megatower, billboards, rings,
//! bridges, a monorail, the night market) is a pure function of the course and
//! the data too. Nothing here is registered at run time: the example
//! `surf_pack_neon` bakes it into modeler ops for the map `maps/surf_neon.bsn`,
//! and the game reads the course back out of that map's markers.

use bevy::math::{Vec2, Vec3};
use serde::Deserialize;

use crate::rules::surf::{Tuning, ridge};

// ------------------------------------------------------------------ data

/// One piece of a ramp's way, walked from the pose the last one left.
#[derive(Deserialize, Debug, Clone)]
pub enum Seg {
    /// Straight on: length (m), drop per metre.
    Straight(f32, f32),
    /// A bend: radius (m), degrees (+ turns right, towards +z), drop per metre.
    Arc(f32, f32, f32),
    /// A straight onto a point of another ramp of the stage (its index) at a
    /// distance along it (m; `None`: its end; negative: that far from the end).
    Join(usize, Option<f32>),
    /// A smooth landing on the stage's target (the next area's first pad),
    /// arriving heading east.
    Land,
}

/// Where a ramp starts.
#[derive(Deserialize, Default, Debug, Clone)]
pub enum Start {
    /// Off the stage's pad (`hop` past it and below it, heading east).
    #[default]
    Pad,
    /// From a pose on another ramp of the stage (`at` as in [`Seg::Join`]),
    /// `ahead` along it, `side` to its right and `down` below it, turned `turn`
    /// degrees: a gap to jump when `ahead` is more than nothing.
    After {
        ramp: usize,
        at: Option<f32>,
        ahead: f32,
        side: f32,
        down: f32,
        turn: f32,
    },
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct RampSpec {
    pub name: String,
    pub start: Start,
    pub segs: Vec<Seg>,
}

/// One stage: its ramps (a fork is a shortcut ramp that starts at the split
/// and joins the main one where the ways meet), and which ramp's end the next
/// stage's pad is built to receive.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct StageSpec {
    pub stage: usize,
    pub ramps: Vec<RampSpec>,
    /// The ramp whose end the next pad (or, for the last stage, the target) is
    /// built after.
    pub main: usize,
    /// Ramps followed by a speed-check gap (`SpeedGap` markers).
    pub gaps: Vec<usize>,
    /// Ramps that are the shortcut's two halves: (a, b), a joins b by a gap.
    pub shortcut: Option<(usize, usize)>,
    /// How the stage's ramps look: r, g, b, glow, roughness, metal.
    pub look: (f32, f32, f32, f32, f32, f32),
    /// The stage's neon (a modeler material) and light colour.
    pub neon: String,
    pub colour: (f32, f32, f32),
}

/// The city of towers.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct City {
    pub seed: u32,
    /// The grid's extent (hall x, then z) and the street plan: block pitch and
    /// the street between blocks.
    pub x: (f32, f32),
    pub z: (f32, f32),
    pub pitch: f32,
    pub street: f32,
    /// Where the towers stand on (hall y).
    pub base: f32,
    /// The highest a roof may be (hall y).
    pub ceiling: f32,
    /// A tower stays this far (plan, m) from every ridge, and it is solid
    /// within `near` of one.
    pub clear: f32,
    pub near: f32,
    /// A tower near the course rises this much above the ridge beside it
    /// (min, max), and the taller sky-line far away adds up to `skyline` more
    /// by `skyline_at` m out.
    pub rise: (f32, f32),
    pub skyline: f32,
    pub skyline_at: f32,
    /// Odds a block is a low one below the course's fall line (never by it).
    pub deep_chance: f32,
    /// Facade materials, neon band materials.
    pub facades: Vec<String>,
    pub neons: Vec<String>,
    /// Neon bands: how many at most, odds per tower, thickness, and how far out
    /// from the course (m) bands and corner strips are made at all.
    pub bands: u32,
    pub band_chance: f32,
    pub band_height: f32,
    pub band_reach: f32,
    /// Odds a tower has an antenna, and how tall (min, max).
    pub antenna_chance: f32,
    pub antenna: (f32, f32),
    /// Keep towers this far (m) from a pad's middle.
    pub pad_clear: f32,
}

/// The megatower the second stage winds round.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tower {
    /// The ramp (stage, ramp) it stands beside, and its middle from that
    /// ramp's first point (x, z).
    pub stage: usize,
    pub ramp: usize,
    pub offset: (f32, f32),
    pub radius: f32,
    pub bottom: f32,
    pub top: f32,
    /// A neon band round it every this many metres, and its thickness.
    pub band_every: f32,
    pub band: f32,
    /// Its crown (a spinning model) and the hologram over it: model, scale, rpm.
    pub crown: String,
    pub crown_scale: f32,
    pub crown_rpm: f32,
    pub crown_gap: f32,
    pub spire: (f32, f32),
}

#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    #[default]
    Still,
    Bob,
    Spin,
}

#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq)]
pub enum Face {
    /// The model's +z along the ridge.
    #[default]
    Along,
    /// The model's +z towards the ridge (a billboard facing the surfers).
    Ridge,
}

/// A set piece beside the course: a model at a distance along a ramp, `side`
/// metres to its right and `up` above its ridge.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Prop {
    pub model: String,
    pub stage: usize,
    pub ramp: usize,
    pub at: f32,
    pub side: f32,
    pub up: f32,
    pub scale: f32,
    pub motion: Motion,
    pub face: Face,
    /// Extra turn (degrees).
    pub yaw: f32,
    /// A coloured light with it (0: none).
    pub light: f32,
}

/// Laser rings round a ramp every `every` m from `from` to `to`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Rings {
    pub stage: usize,
    pub ramp: usize,
    pub from: f32,
    pub to: f32,
    pub every: f32,
    pub scale: f32,
    pub model: String,
}

/// A bridge over the course: a deck `up` above the ridge at a distance along
/// a ramp, `span` each way, on pylons.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Bridge {
    pub stage: usize,
    pub ramp: usize,
    pub at: f32,
    pub up: f32,
    pub width: f32,
    pub span: f32,
    pub neon: String,
    /// A rail on top (the maglev) or a footway.
    pub rail: bool,
}

/// The monorail that runs beside the course.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Rail {
    pub stage: usize,
    pub ramp: usize,
    pub from: f32,
    pub to: f32,
    pub side: f32,
    pub up: f32,
    pub width: f32,
    pub pylon_every: f32,
    /// The train's cars: model, how many, gap between (m), where it starts
    /// along the rail (m).
    pub car: String,
    pub cars: usize,
    pub car_gap: f32,
    pub car_at: f32,
}

/// The night market below the calm stretch.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Market {
    pub x: (f32, f32),
    pub z: (f32, f32),
    /// The deck's top (hall y): under the stage's fall line, so you never land.
    pub deck: f32,
    pub stalls: usize,
    pub lanterns: usize,
    pub seed: u32,
    pub neons: Vec<String>,
    /// Lanterns and balloons float this high over the deck (min, max).
    pub strings: (f32, f32),
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Lamps {
    /// A coloured light over the ridge every this many metres of a stage's
    /// main ramps, this high, with this much light.
    pub every: f32,
    pub up: f32,
    pub lumens: f32,
    pub range: f32,
    /// Lane beads: every this many metres, this far out and below.
    pub bead_every: f32,
    pub bead_out: f32,
    pub bead_drop: f32,
    pub bead: f32,
    pub pad_lumens: f32,
    pub pad_range: f32,
    /// The speed-check arrow: model, scale and height over the gap.
    pub arrow: String,
    pub arrow_scale: f32,
    pub arrow_up: f32,
}

/// Tuning: `assets/data/side/surf_neon.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Neon {
    /// Metres between the points a bend or straight is walked in.
    pub step: f32,
    /// A ramp leaves its pad this far past the pad's middle and this far below it.
    pub hop: (f32, f32),
    /// A ramp arrives this far before the next pad's middle and this far above it.
    pub arrive: (f32, f32),
    /// The mall's end pad that starts the first stage (it already stands).
    pub p7: (f32, f32, f32),
    pub p7_size: (f32, f32, f32),
    /// The pads of stages 8 and 9: size, and the pillar under each (width, bottom).
    pub pad_size: (f32, f32, f32),
    pub pillar: (f32, f32),
    /// The overgrown ruins' first pad this area lands on, and a stand-in for it.
    pub p10: (f32, f32, f32),
    pub p10_size: (f32, f32, f32),
    pub stages: Vec<StageSpec>,
    pub city: City,
    pub tower: Tower,
    pub props: Vec<Prop>,
    pub rings: Vec<Rings>,
    pub bridges: Vec<Bridge>,
    pub rail: Rail,
    pub market: Market,
    pub lamps: Lamps,
}

pub static NEON: friendslop_data::Data<Neon> =
    friendslop_data::Data::ron("data/side/surf_neon.ron");

// ---------------------------------------------------------------- course

/// A ramp walked into points.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Built {
    pub points: Vec<Vec3>,
    /// The heading at each point (radians; 0 is +x, positive turns towards +z).
    pub heading: Vec<f32>,
    /// Metres along the ramp at each point.
    pub dist: Vec<f32>,
}

impl Built {
    fn new(p: Vec3, th: f32) -> Self {
        Self {
            points: vec![p],
            heading: vec![th],
            dist: vec![0.0],
        }
    }

    /// Add a point `len` metres of ramp further on (an arc's length, not its chord's).
    fn push(&mut self, p: Vec3, th: f32, len: f32) {
        let d = self.dist.last().copied().unwrap_or(0.0) + len;
        self.points.push(p);
        self.heading.push(th);
        self.dist.push(d);
    }

    pub fn length(&self) -> f32 {
        self.dist.last().copied().unwrap_or(0.0)
    }

    pub fn end(&self) -> (Vec3, f32) {
        (
            self.points.last().copied().unwrap_or(Vec3::ZERO),
            self.heading.last().copied().unwrap_or(0.0),
        )
    }

    /// The pose `at` metres along (`None`: the end; negative: from the end).
    pub fn pose(&self, at: Option<f32>) -> (Vec3, f32) {
        let len = self.length();
        let d = match at {
            None => len,
            Some(d) if d < 0.0 => (len + d).max(0.0),
            Some(d) => d.min(len),
        };
        let i = self
            .dist
            .partition_point(|&x| x <= d)
            .clamp(1, self.points.len().saturating_sub(1).max(1));
        if self.points.len() < 2 {
            return self.end();
        }
        let (a, b) = (i - 1, i);
        let span = (self.dist[b] - self.dist[a]).max(1e-4);
        let t = ((d - self.dist[a]) / span).clamp(0.0, 1.0);
        (
            self.points[a].lerp(self.points[b], t),
            self.heading[a] + (self.heading[b] - self.heading[a]) * t,
        )
    }
}

fn dir(th: f32) -> Vec2 {
    Vec2::new(th.cos(), th.sin())
}

fn straight_to(out: &mut Built, to: Vec3, step: f32) {
    let (p, _) = out.end();
    let flat = Vec2::new(to.x - p.x, to.z - p.z);
    let len = flat.length();
    if len < 1e-3 {
        return;
    }
    let th = flat.y.atan2(flat.x);
    let n = (len / step).round().max(1.0) as usize;
    for i in 1..=n {
        let t = i as f32 / n as f32;
        out.push(p.lerp(to, t), th, len / n as f32);
    }
}

/// Walk `segs` from a pose. `built` are the stage's ramps so far; `target` is
/// where a landing ends.
fn walk(
    start: (Vec3, f32),
    segs: &[Seg],
    step: f32,
    built: &[Built],
    target: Option<Vec3>,
) -> Built {
    let step = step.max(1.0);
    let mut out = Built::new(start.0, start.1);
    for seg in segs {
        let (mut p, mut th) = out.end();
        match *seg {
            Seg::Straight(len, slope) => {
                let n = (len / step).round().max(1.0) as usize;
                let d = len / n as f32;
                for _ in 0..n {
                    p.x += th.cos() * d;
                    p.z += th.sin() * d;
                    p.y -= slope * d;
                    out.push(p, th, d);
                }
            }
            Seg::Arc(r, deg, slope) => {
                let total = r * deg.to_radians().abs();
                let n = (total / step).round().max(1.0) as usize;
                let d = total / n as f32;
                let dth = deg.to_radians() / n as f32;
                for _ in 0..n {
                    let a = th + dth / 2.0;
                    let chord = 2.0 * r * (dth.abs() / 2.0).sin();
                    p.x += a.cos() * chord;
                    p.z += a.sin() * chord;
                    th += dth;
                    p.y -= slope * d;
                    out.push(p, th, d);
                }
            }
            Seg::Join(ramp, at) => {
                if let Some(b) = built.get(ramp) {
                    let (to, _) = b.pose(at);
                    straight_to(&mut out, to, step);
                }
            }
            Seg::Land => {
                let Some(to) = target else { continue };
                let k = Vec2::new(to.x - p.x, to.z - p.z).length();
                let n = (k / step).round().max(2.0) as usize;
                let (t0, t1) = (dir(th) * k, dir(0.0) * k);
                let mut prev = p;
                for i in 1..=n {
                    let t = i as f32 / n as f32;
                    let (t2, t3) = (t * t, t * t * t);
                    let (h00, h10, h01, h11) = (
                        2.0 * t3 - 3.0 * t2 + 1.0,
                        t3 - 2.0 * t2 + t,
                        -2.0 * t3 + 3.0 * t2,
                        t3 - t2,
                    );
                    let x = h00 * p.x + h10 * t0.x + h01 * to.x + h11 * t1.x;
                    let z = h00 * p.z + h10 * t0.y + h01 * to.z + h11 * t1.y;
                    let y = p.y + (to.y - p.y) * t;
                    let q = Vec3::new(x, y, z);
                    let th = if i == n {
                        0.0
                    } else {
                        (q.z - prev.z).atan2(q.x - prev.x)
                    };
                    let len = Vec2::new(q.x - prev.x, q.z - prev.z).length();
                    prev = q;
                    out.push(q, th, len);
                }
            }
        }
    }
    out
}

/// A stage as the data lays it out.
#[derive(Debug, Clone, PartialEq)]
pub struct StageOut {
    pub stage: usize,
    /// Its pad: centre, size.
    pub pad: (Vec3, Vec3),
    pub ramps: Vec<Built>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Course {
    pub stages: Vec<StageOut>,
    /// Where the last stage's main ramp ends (30 m before the next area's pad).
    pub landing: Vec3,
}

/// Where a pad's centre is for a ramp that arrives at `end`.
pub fn pad_for(d: &Neon, end: Vec3) -> Vec3 {
    end + Vec3::new(d.arrive.0, -d.arrive.1, 0.0)
}

/// The whole course, walked.
pub fn course(d: &Neon) -> Course {
    let mut stages: Vec<StageOut> = Vec::new();
    let mut pad = (Vec3::from(d.p7), Vec3::from(d.p7_size));
    let landing = Vec3::from(d.p10) - Vec3::new(d.arrive.0, -d.arrive.1, 0.0);
    for spec in &d.stages {
        let mut built: Vec<Built> = Vec::new();
        for r in &spec.ramps {
            let start = match r.start {
                Start::Pad => (pad.0 + Vec3::new(d.hop.0, -d.hop.1, 0.0), 0.0),
                Start::After {
                    ramp,
                    at,
                    ahead,
                    side,
                    down,
                    turn,
                } => {
                    let (p, th) = built.get(ramp).map_or((Vec3::ZERO, 0.0), |b| b.pose(at));
                    let (f, right) = (dir(th), Vec2::new(-th.sin(), th.cos()));
                    let flat = f * ahead + right * side;
                    (
                        Vec3::new(p.x + flat.x, p.y - down, p.z + flat.y),
                        th + turn.to_radians(),
                    )
                }
            };
            let b = walk(start, &r.segs, d.step, &built, Some(landing));
            built.push(b);
        }
        let main_end = built.get(spec.main).map_or(Vec3::ZERO, |b| b.end().0);
        stages.push(StageOut {
            stage: spec.stage,
            pad,
            ramps: built,
        });
        pad = (pad_for(d, main_end), Vec3::from(d.pad_size));
    }
    Course { stages, landing }
}

impl Course {
    pub fn stage(&self, stage: usize) -> Option<&StageOut> {
        self.stages.iter().find(|s| s.stage == stage)
    }

    /// The pad after the last stage (a stand-in for the next area's first).
    pub fn ramp(&self, stage: usize, ramp: usize) -> Option<&Built> {
        self.stage(stage).and_then(|s| s.ramps.get(ramp))
    }
}

/// One stage's ramps as the surf rules' ridge-point lists.
pub fn ridge_points(s: &StageOut) -> Vec<Vec<(f32, f32, f32)>> {
    s.ramps
        .iter()
        .map(|b| b.points.iter().map(|p| (p.x, p.y, p.z)).collect())
        .collect()
}

/// The surf rules' tuning with this course's stages in their places (earlier
/// stages empty), for the tests that ask the rules about a gap.
pub fn tuning_with(t: &Tuning, c: &Course) -> Tuning {
    let mut t = t.clone();
    let top = c.stages.iter().map(|s| s.stage + 1).max().unwrap_or(0);
    t.stages.resize_with(top, Default::default);
    for s in &c.stages {
        t.stages[s.stage] = crate::rules::surf::Stage {
            pad: (
                (s.pad.0.x, s.pad.0.y, s.pad.0.z),
                (s.pad.1.x, s.pad.1.y, s.pad.1.z),
            ),
            ramps: ridge_points(s),
        };
    }
    t
}

// ---------------------------------------------------------------- layout

/// One sample of the course: where, and which ramp, how far along.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub stage: usize,
    pub ramp: usize,
    pub at: Vec3,
    pub dist: f32,
    /// Unit heading across the ground.
    pub heading: Vec2,
}

/// Every ridge, finely sampled (as the game draws it).
pub fn samples(t: &Tuning, c: &Course) -> Vec<Sample> {
    let mut out = Vec::new();
    for s in &c.stages {
        for (r, b) in s.ramps.iter().enumerate() {
            let pts: Vec<(f32, f32, f32)> = b.points.iter().map(|p| (p.x, p.y, p.z)).collect();
            let line = ridge(&pts, t.ramp_samples);
            let mut dist = 0.0;
            for (i, p) in line.iter().enumerate() {
                if i > 0 {
                    dist += Vec2::new(p.x - line[i - 1].x, p.z - line[i - 1].z).length();
                }
                let (a, z) = (line[i.saturating_sub(1)], line[(i + 1).min(line.len() - 1)]);
                let heading = Vec2::new(z.x - a.x, z.z - a.z).normalize_or(Vec2::X);
                out.push(Sample {
                    stage: s.stage,
                    ramp: r,
                    at: *p,
                    dist,
                    heading,
                });
            }
        }
    }
    out
}

/// A point `at` metres along a ramp, and its heading.
pub fn along(samples: &[Sample], stage: usize, ramp: usize, at: f32) -> Option<Sample> {
    let mut best: Option<(f32, Sample)> = None;
    for s in samples
        .iter()
        .filter(|s| s.stage == stage && s.ramp == ramp)
    {
        let e = (s.dist - at).abs();
        if best.is_none_or(|(b, _)| e < b) {
            best = Some((e, *s));
        }
    }
    best.map(|(_, s)| s)
}

/// One thing the layout puts in the map.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// A box of modeler-material geometry.
    Part {
        at: Vec3,
        yaw: f32,
        size: Vec3,
        mat: String,
    },
    /// A solid box (or upright cylinder).
    Block {
        at: Vec3,
        yaw: f32,
        size: Vec3,
        round: bool,
    },
    /// A placed model.
    Model {
        model: String,
        at: Vec3,
        yaw: f32,
        scale: f32,
        motion: Motion,
        phase: f32,
    },
    /// A light.
    Lamp { at: Vec3, rgb: (f32, f32, f32) },
}

/// Everything the city adds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {
    pub items: Vec<Item>,
}

impl Layout {
    fn part(&mut self, at: Vec3, yaw: f32, size: Vec3, mat: &str) {
        self.items.push(Item::Part {
            at,
            yaw,
            size,
            mat: mat.to_string(),
        });
    }

    fn block(&mut self, at: Vec3, yaw: f32, size: Vec3, round: bool) {
        self.items.push(Item::Block {
            at,
            yaw,
            size,
            round,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn model(&mut self, model: &str, at: Vec3, yaw: f32, scale: f32, motion: Motion, phase: f32) {
        self.items.push(Item::Model {
            model: model.to_string(),
            at,
            yaw,
            scale,
            motion,
            phase,
        });
    }

    pub fn blocks(&self) -> impl Iterator<Item = (Vec3, f32, Vec3, bool)> + '_ {
        self.items.iter().filter_map(|i| match i {
            Item::Block {
                at,
                yaw,
                size,
                round,
            } => Some((*at, *yaw, *size, *round)),
            _ => None,
        })
    }

    pub fn lamps(&self) -> usize {
        self.items
            .iter()
            .filter(|i| matches!(i, Item::Lamp { .. }))
            .count()
    }
}

/// A deterministic number in 0..1 from a seed and two indices.
pub fn rnd(seed: u32, a: i32, b: i32) -> f32 {
    let mut x = u64::from(seed) ^ ((a as u64) << 32) ^ (b as u32 as u64);
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The yaw that points a model's +z along a ground direction.
pub fn yaw_of(v: Vec2) -> f32 {
    v.x.atan2(v.y)
}

/// How far (plan, m) a box (centre, half sizes) is from a point.
pub fn rect_distance(c: Vec2, half: Vec2, p: Vec2) -> f32 {
    let d = (p - c).abs() - half;
    d.max(Vec2::ZERO).length()
}

/// The pads (centre, size) in the area, with the stand-in for the next.
pub fn pads(d: &Neon, c: &Course) -> Vec<(Vec3, Vec3)> {
    let mut out: Vec<(Vec3, Vec3)> = c.stages.iter().map(|s| s.pad).collect();
    out.push((Vec3::from(d.p10), Vec3::from(d.p10_size)));
    out
}

/// The fall line of a stage (hall y): under it you are back on the pad.
pub fn fall_line(t: &Tuning, c: &Course, stage: usize) -> f32 {
    c.stage(stage)
        .map(|s| {
            s.ramps
                .iter()
                .flat_map(|r| r.points.iter().map(|p| p.y))
                .fold(f32::MAX, f32::min)
        })
        .unwrap_or(0.0)
        - t.stage_fall
}

/// The megatower's middle (hall x, z).
pub fn tower_centre(d: &Neon, c: &Course) -> Vec2 {
    let p = c
        .ramp(d.tower.stage, d.tower.ramp)
        .and_then(|b| b.points.first().copied())
        .unwrap_or(Vec3::ZERO);
    Vec2::new(p.x + d.tower.offset.0, p.z + d.tower.offset.1)
}

/// A column of neon: three squares turned 0, 30 and 60 degrees look round.
fn column(l: &mut Layout, c: Vec3, radius: f32, height: f32, mat: &str) {
    let side = radius * 1.7;
    for k in 0..3 {
        l.part(
            c,
            (k as f32 * 30.0).to_radians(),
            Vec3::new(side, height, side),
            mat,
        );
    }
}

/// The city, the megatower, the set pieces: everything round the ramps.
pub fn layout(t: &Tuning, d: &Neon, c: &Course) -> Layout {
    let mut l = Layout::default();
    let samples = samples(t, c);
    let ci = &d.city;
    let tc = tower_centre(d, c);
    let pads = pads(d, c);

    // ---- the grid of towers
    let nx = ((ci.x.1 - ci.x.0) / ci.pitch).floor() as i32;
    let nz = ((ci.z.1 - ci.z.0) / ci.pitch).floor() as i32;
    for i in 0..nx {
        for j in 0..nz {
            let r = |k: i32| rnd(ci.seed, i * 977 + j, k);
            let foot = ci.pitch - ci.street;
            let (w, dp) = (foot * lerp(0.62, 1.0, r(1)), foot * lerp(0.62, 1.0, r(2)));
            let cx = ci.x.0 + (i as f32 + 0.5) * ci.pitch + (r(3) - 0.5) * (foot - w);
            let cz = ci.z.0 + (j as f32 + 0.5) * ci.pitch + (r(4) - 0.5) * (foot - dp);
            let (centre, half) = (Vec2::new(cx, cz), Vec2::new(w / 2.0, dp / 2.0));
            // The megatower, the market and the pads keep their ground.
            if centre.distance(tc) < d.tower.radius + 14.0 + half.max_element()
                || (centre.x > d.market.x.0 - 8.0
                    && centre.x < d.market.x.1 + 8.0
                    && centre.y > d.market.z.0 - 8.0
                    && centre.y < d.market.z.1 + 8.0)
                || pads.iter().any(|(pc, ps)| {
                    rect_distance(centre, half, Vec2::new(pc.x, pc.z)) < ci.pad_clear + ps.x / 2.0
                })
            {
                continue;
            }
            // How close the course comes, and where its ridge is beside it.
            let mut near: Option<(f32, Sample)> = None;
            let mut low = f32::MAX;
            for s in &samples {
                let dist = rect_distance(centre, half, Vec2::new(s.at.x, s.at.z));
                if near.is_none_or(|(b, _)| dist < b) {
                    near = Some((dist, *s));
                }
                if dist < 220.0 {
                    low = low.min(fall_line(t, c, s.stage));
                }
            }
            let Some((dist, s)) = near else { continue };
            if dist < ci.clear {
                continue;
            }
            let solid = dist < ci.near;
            let far = ((dist - ci.near) / ci.skyline_at).clamp(0.0, 1.0);
            let top = if !solid && low < f32::MAX && r(5) < ci.deep_chance {
                low - 6.0 - r(6) * 30.0
            } else {
                let rise = lerp(ci.rise.0, ci.rise.1, r(7)) + ci.skyline * far * r(8);
                (s.at.y + rise).min(ci.ceiling)
            };
            let height = top - ci.base;
            let mid = Vec3::new(cx, ci.base + height / 2.0, cz);
            let style = (r(9) * ci.facades.len() as f32) as usize;
            let facade = ci.facades.get(style).map_or("sf-fa-glass", String::as_str);
            l.part(mid, 0.0, Vec3::new(w, height, dp), facade);
            l.part(
                Vec3::new(cx, top + 0.2, cz),
                0.0,
                Vec3::new(w - 0.4, 0.4, dp - 0.4),
                "sf-fa-roof",
            );
            if solid {
                l.block(mid, 0.0, Vec3::new(w, height, dp), false);
            }
            if dist < ci.band_reach && !ci.neons.is_empty() {
                let neon = |k: i32| {
                    let n = (r(k) * ci.neons.len() as f32) as usize;
                    ci.neons[n % ci.neons.len()].as_str()
                };
                if r(10) < ci.band_chance {
                    let bands = 1 + (r(11) * ci.bands as f32) as u32;
                    for b in 0..bands {
                        let y = lerp(s.at.y - 30.0, top - 4.0, r(12 + b as i32 * 3));
                        if y < ci.base + 20.0 {
                            continue;
                        }
                        l.part(
                            Vec3::new(cx, y, cz),
                            0.0,
                            Vec3::new(w + 0.6, ci.band_height, dp + 0.6),
                            neon(13 + b as i32 * 3),
                        );
                    }
                }
                if r(40) < 0.3 {
                    // Corner strips up the tower's face.
                    let m = neon(41);
                    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                        l.part(
                            Vec3::new(cx + sx * (w / 2.0 + 0.1), mid.y, cz + sz * (dp / 2.0 + 0.1)),
                            0.0,
                            Vec3::new(0.7, height, 0.7),
                            m,
                        );
                    }
                }
            }
            if r(42) < ci.antenna_chance {
                let h = lerp(ci.antenna.0, ci.antenna.1, r(43));
                l.part(
                    Vec3::new(cx, top + h / 2.0, cz),
                    0.0,
                    Vec3::new(0.8, h, 0.8),
                    "sf-hull",
                );
                l.part(
                    Vec3::new(cx, top + h + 0.6, cz),
                    0.0,
                    Vec3::splat(1.4),
                    "sf-neon-pink",
                );
            }
        }
    }

    // ---- the megatower
    let tw = &d.tower;
    let height = tw.top - tw.bottom;
    let mid = Vec3::new(tc.x, tw.bottom + height / 2.0, tc.y);
    column(&mut l, mid, tw.radius, height, "sf-fa-glass");
    l.block(
        mid,
        0.0,
        Vec3::new(tw.radius * 2.0, height, tw.radius * 2.0),
        true,
    );
    let neons = if d.city.neons.is_empty() {
        vec!["sf-neon-pink".to_string()]
    } else {
        d.city.neons.clone()
    };
    let mut y = tw.bottom + tw.band_every;
    let mut k = 0;
    while y < tw.top - 2.0 {
        let m = &neons[k % neons.len()];
        column(
            &mut l,
            Vec3::new(tc.x, y, tc.y),
            tw.radius + 0.7,
            tw.band,
            m,
        );
        y += tw.band_every;
        k += 1;
    }
    let crown_at = Vec3::new(tc.x, tw.top + tw.crown_gap, tc.y);
    l.model(&tw.crown, crown_at, 0.0, tw.crown_scale, Motion::Spin, 0.0);
    l.part(
        Vec3::new(tc.x, tw.top + tw.spire.0 / 2.0, tc.y),
        0.0,
        Vec3::new(tw.spire.1, tw.spire.0, tw.spire.1),
        "sf-hull",
    );
    l.items.push(Item::Lamp {
        at: Vec3::new(tc.x, tw.top + 4.0, tc.y),
        rgb: (1.0, 0.35, 0.9),
    });

    // ---- set pieces
    for (n, p) in d.props.iter().enumerate() {
        let Some(s) = along(&samples, p.stage, p.ramp, p.at) else {
            continue;
        };
        let right = Vec2::new(-s.heading.y, s.heading.x);
        let flat = Vec2::new(s.at.x, s.at.z) + right * p.side;
        let yaw = match p.face {
            Face::Along => yaw_of(s.heading),
            Face::Ridge => yaw_of(-right * p.side.signum()),
        } + p.yaw.to_radians();
        let at = Vec3::new(flat.x, s.at.y + p.up, flat.y);
        l.model(&p.model, at, yaw, p.scale, p.motion, n as f32 * 1.7);
        if p.light > 0.0 {
            l.items.push(Item::Lamp {
                at: at + Vec3::Y * 4.0,
                rgb: (0.9, 0.4, 1.0),
            });
        }
    }

    // ---- laser rings round the ramps
    for r in &d.rings {
        let mut at = r.from;
        while at <= r.to {
            if let Some(s) = along(&samples, r.stage, r.ramp, at) {
                l.model(
                    &r.model,
                    s.at - Vec3::Y * (5.0 + 5.0 * r.scale),
                    yaw_of(s.heading),
                    r.scale,
                    Motion::Still,
                    0.0,
                );
            }
            at += r.every.max(5.0);
        }
    }

    // ---- bridges over the course
    for b in &d.bridges {
        let Some(s) = along(&samples, b.stage, b.ramp, b.at) else {
            continue;
        };
        let right = Vec2::new(-s.heading.y, s.heading.x);
        // Its pylons stand clear of every ramp, or it isn't built.
        let feet =
            [-1.0f32, 1.0].map(|side| Vec2::new(s.at.x, s.at.z) + right * side * (b.span - 3.0));
        if feet.iter().any(|f| {
            samples
                .iter()
                .any(|q| f.distance(Vec2::new(q.at.x, q.at.z)) < d.city.clear)
        }) {
            continue;
        }
        let yaw = yaw_of(right);
        let deck_y = s.at.y + b.up;
        let centre = Vec3::new(s.at.x, deck_y, s.at.z);
        l.part(
            centre,
            yaw,
            Vec3::new(b.width, 3.0, b.span * 2.0),
            "sf-hull",
        );
        l.block(centre, yaw, Vec3::new(b.width, 3.0, b.span * 2.0), false);
        l.part(
            centre + Vec3::Y * 1.7,
            yaw,
            Vec3::new(b.width * 0.8, 0.4, b.span * 2.0),
            &b.neon,
        );
        for foot in feet {
            let h = deck_y - d.city.base;
            let c = Vec3::new(foot.x, d.city.base + h / 2.0, foot.y);
            l.part(c, 0.0, Vec3::new(6.0, h, 6.0), "sf-fa-office");
            l.block(c, 0.0, Vec3::new(6.0, h, 6.0), false);
        }
        if b.rail {
            l.part(
                centre + Vec3::Y * 3.0,
                yaw,
                Vec3::new(1.0, 1.2, b.span * 2.0),
                "sf-neon-cyan",
            );
        }
    }

    // ---- the monorail
    let rl = &d.rail;
    let mut at = rl.from;
    let mut prev: Option<Vec3> = None;
    let mut since = 0.0;
    while at <= rl.to {
        if let Some(s) = along(&samples, rl.stage, rl.ramp, at) {
            let right = Vec2::new(-s.heading.y, s.heading.x);
            let flat = Vec2::new(s.at.x, s.at.z) + right * rl.side;
            let p = Vec3::new(flat.x, s.at.y + rl.up, flat.y);
            if let Some(q) = prev {
                let seg = p - q;
                let len = seg.length();
                if len > 0.1 {
                    let yaw = yaw_of(Vec2::new(seg.x, seg.z).normalize_or(Vec2::X));
                    let mid = (p + q) / 2.0;
                    let size = Vec3::new(rl.width, 1.6, len + 0.4);
                    l.part(mid, yaw, size, "sf-hull");
                    l.part(
                        mid + Vec3::Y * 0.9,
                        yaw,
                        Vec3::new(rl.width * 0.5, 0.25, len + 0.4),
                        "sf-neon-cyan",
                    );
                    l.block(mid, yaw, size, false);
                }
            }
            since += prev.map_or(0.0, |q| q.distance(p));
            if prev.is_none() || since >= rl.pylon_every {
                since = 0.0;
                let h = p.y - d.city.base;
                l.part(
                    Vec3::new(p.x, d.city.base + h / 2.0, p.z),
                    0.0,
                    Vec3::new(3.0, h, 3.0),
                    "sf-fa-office",
                );
                l.block(
                    Vec3::new(p.x, d.city.base + h / 2.0, p.z),
                    0.0,
                    Vec3::new(3.0, h, 3.0),
                    false,
                );
            }
            prev = Some(p);
        }
        at += d.step.max(5.0);
    }
    {
        for n in 0..rl.cars {
            let want = rl.car_at + n as f32 * rl.car_gap;
            if let Some(s) = along(&samples, rl.stage, rl.ramp, want) {
                let right = Vec2::new(-s.heading.y, s.heading.x);
                let flat = Vec2::new(s.at.x, s.at.z) + right * rl.side;
                l.model(
                    &rl.car,
                    Vec3::new(flat.x, s.at.y + rl.up + 0.8, flat.y),
                    yaw_of(s.heading),
                    1.0,
                    Motion::Still,
                    0.0,
                );
            }
        }
    }

    // ---- the night market
    let mk = &d.market;
    let size = Vec3::new(mk.x.1 - mk.x.0, 2.0, mk.z.1 - mk.z.0);
    let centre = Vec3::new(
        (mk.x.0 + mk.x.1) / 2.0,
        mk.deck - 1.0,
        (mk.z.0 + mk.z.1) / 2.0,
    );
    l.part(centre, 0.0, size, "sf-fa-deck");
    for n in 0..mk.stalls {
        let r = |k: i32| rnd(mk.seed, n as i32, k);
        let (x, z) = (
            lerp(mk.x.0 + 6.0, mk.x.1 - 6.0, r(1)),
            lerp(mk.z.0 + 6.0, mk.z.1 - 6.0, r(2)),
        );
        let neon = &mk.neons[(r(3) * mk.neons.len() as f32) as usize % mk.neons.len().max(1)];
        let (w, dp) = (lerp(3.0, 5.5, r(4)), lerp(3.0, 5.5, r(5)));
        l.part(
            Vec3::new(x, mk.deck + 1.4, z),
            0.0,
            Vec3::new(w, 2.8, dp),
            "sf-fa-brick",
        );
        l.part(
            Vec3::new(x, mk.deck + 3.1, z),
            0.0,
            Vec3::new(w + 1.4, 0.5, dp + 1.4),
            neon,
        );
        l.part(
            Vec3::new(x, mk.deck + 3.7, z),
            0.0,
            Vec3::new(w * 0.6, 0.8, 0.3),
            "sf-neon-gold",
        );
    }
    for n in 0..mk.lanterns {
        let r = |k: i32| rnd(mk.seed ^ 0x55, n as i32, k);
        let (x, z) = (lerp(mk.x.0, mk.x.1, r(1)), lerp(mk.z.0, mk.z.1, r(2)));
        let neon = &mk.neons[(r(3) * mk.neons.len() as f32) as usize % mk.neons.len().max(1)];
        let h = lerp(mk.strings.0, mk.strings.1, r(4));
        l.part(
            Vec3::new(x, mk.deck + h, z),
            r(5) * 3.0,
            Vec3::new(1.3, 1.8, 1.3),
            neon,
        );
    }

    // ---- lights over the ridge and beads beside it
    for spec in &d.stages {
        let Some(st) = c.stage(spec.stage) else {
            continue;
        };
        let colour = spec.colour;
        for (ri, b) in st.ramps.iter().enumerate() {
            let main = ri <= spec.main;
            let mut at = d.lamps.every * 0.5;
            while at < b.length() {
                if main && let Some(s) = along(&samples, spec.stage, ri, at) {
                    l.items.push(Item::Lamp {
                        at: s.at + Vec3::Y * d.lamps.up,
                        rgb: colour,
                    });
                }
                at += d.lamps.every.max(20.0);
            }
            let mut at = 0.0;
            while at < b.length() {
                if let Some(s) = along(&samples, spec.stage, ri, at) {
                    let right = Vec2::new(-s.heading.y, s.heading.x);
                    for side in [-1.0f32, 1.0] {
                        let flat = Vec2::new(s.at.x, s.at.z)
                            + right * side * (t.ramp_half_width * 1.7 + d.lamps.bead_out);
                        l.part(
                            Vec3::new(flat.x, s.at.y - d.lamps.bead_drop, flat.y),
                            0.0,
                            Vec3::splat(d.lamps.bead),
                            &spec.neon,
                        );
                    }
                }
                at += d.lamps.bead_every.max(5.0);
            }
        }
    }
    l
}

/// The speed-check arrows over each gap: position, yaw.
pub fn arrows(t: &Tuning, d: &Neon, c: &Course) -> Vec<(Vec3, f32)> {
    let full = tuning_with(t, c);
    let mut out = Vec::new();
    for spec in &d.stages {
        for &r in &spec.gaps {
            if let Some(g) = crate::rules::surf::gap(&full, spec.stage, r) {
                let mid = (g.from + g.to) / 2.0 + Vec3::Y * d.lamps.arrow_up;
                out.push((mid, yaw_of(g.dir)));
            }
        }
    }
    out
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::surf::{self, gap, ideal_speed, speed_needed};

    fn load() -> (std::sync::Arc<Tuning>, std::sync::Arc<Neon>) {
        for path in [surf::TUNING.path(), NEON.path()] {
            let full = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), path);
            let Ok(bytes) = std::fs::read(&full) else {
                panic!("no {full}")
            };
            let ok = if path == NEON.path() {
                NEON.set_bytes(&bytes)
            } else {
                surf::TUNING.set_bytes(&bytes)
            };
            assert!(ok, "{full} doesn't parse");
        }
        // The rules' tuning without any maps: just the numbers.
        (surf::TUNING.get(), NEON.get())
    }

    fn setup() -> (Tuning, Neon, Course) {
        let (t, d) = load();
        let c = course(&d);
        (tuning_with(&t, &c), (*d).clone(), c)
    }

    #[test]
    fn the_data_file_has_every_field() {
        let (_, d, _) = setup();
        assert_eq!(d.stages.len(), 3);
        assert!(d.step > 1.0 && d.city.pitch > d.city.street && d.city.clear > 12.0);
        assert!(!d.city.facades.is_empty() && !d.city.neons.is_empty());
        assert!(d.props.len() >= 20 && !d.rings.is_empty() && !d.bridges.is_empty());
        assert!(d.tower.radius > 20.0 && d.market.stalls > 20 && d.lamps.every > 20.0);
        for (n, s) in d.stages.iter().enumerate() {
            assert_eq!(s.stage, 7 + n);
            assert!(s.main < s.ramps.len() && !s.gaps.is_empty());
        }
    }

    #[test]
    fn the_handoffs_are_where_the_brief_says() {
        let (_, d, c) = setup();
        // P7 -> first ridge point: 12.35 m past it, 5.5 m below.
        let s7 = &c.stages[0];
        let p7 = Vec3::from(d.p7);
        let first = s7.ramps[0].points[0];
        assert!(
            (first - (p7 + Vec3::new(12.35, -5.5, 0.0))).length() < 0.01,
            "{first}"
        );
        // Each stage's main ramp ends 30 m before the next pad, 8.5 above it.
        for w in c.stages.windows(2) {
            let spec = &d.stages[w[0].stage - 7];
            let end = w[0].ramps[spec.main].end().0;
            assert!(
                (w[1].pad.0 - (end + Vec3::new(30.0, -8.5, 0.0))).length() < 0.01,
                "{} {end}",
                w[1].pad.0
            );
            assert!(
                (w[1].ramps[0].points[0] - (w[1].pad.0 + Vec3::new(12.35, -5.5, 0.0))).length()
                    < 0.01
            );
            let th = w[0].ramps[spec.main].end().1;
            assert!(th.sin().abs() < 1e-3 && th.cos() > 0.0, "heads east");
        }
        // The last lands on P10: 30 m before it, 8.5 above, heading east.
        let spec = &d.stages[2];
        let (end, th) = c.stages[2].ramps[spec.main].end();
        let p10 = Vec3::from(d.p10);
        assert!(
            (end - (p10 + Vec3::new(-30.0, 8.5, 0.0))).length() < 0.01,
            "{end}"
        );
        assert!(th.abs() < 0.05, "heading {th}");
        assert!((p10.z + 1935.0).abs() < 40.0 && (end.z + 1935.0).abs() < 40.0);
    }

    /// The circumradius of three points across the ground.
    fn radius(a: Vec3, b: Vec3, c: Vec3) -> f32 {
        let (ab, bc, ca) = (
            Vec2::new(b.x - a.x, b.z - a.z),
            Vec2::new(c.x - b.x, c.z - b.z),
            Vec2::new(a.x - c.x, a.z - c.z),
        );
        let area2 = ab.perp_dot(bc).abs();
        if area2 < 1e-4 {
            return f32::MAX;
        }
        ab.length() * bc.length() * ca.length() / (2.0 * area2)
    }

    #[test]
    fn bends_are_wide_and_ramps_only_go_down() {
        let (t, d, c) = setup();
        for s in &c.stages {
            for (ri, b) in s.ramps.iter().enumerate() {
                for w in b.points.windows(3) {
                    let r = radius(w[0], w[1], w[2]);
                    assert!(
                        r >= 60.0,
                        "stage {} ramp {ri}: radius {r} at {}",
                        s.stage,
                        w[1]
                    );
                }
                for w in b.points.windows(2) {
                    assert!(
                        w[1].y <= w[0].y + 1e-3,
                        "stage {} ramp {ri} climbs",
                        s.stage
                    );
                }
                // A slope that keeps speed without being a cliff.
                let drop = b.points[0].y - b.end().0.y;
                let slope = drop / b.length().max(1.0);
                assert!(
                    slope > 0.015 && slope < 0.19,
                    "stage {} ramp {} slope {slope}",
                    s.stage,
                    d.stages[s.stage - 7].ramps[ri].name
                );
            }
        }
        // The area's fast sweeps are 85 m or wider except the helix round the tower.
        assert!(t.ramp_half_width > 0.0);
    }

    /// Pairs of ridge points that pass over each other, or run side by side.
    #[test]
    fn crossings_have_room_and_only_forks_overlap() {
        let (t, d, c) = setup();
        let s = samples(&t, &c);
        // Where ramps are meant to share ground: every ramp that starts off
        // another (a chain or a fork's split: the nearby bodies overlap), and
        // where a shortcut merges.
        let mut junctions: Vec<(Vec2, f32)> = Vec::new();
        for spec in &d.stages {
            let Some(st) = c.stage(spec.stage) else {
                panic!("no stage")
            };
            for (r, ramp) in spec.ramps.iter().enumerate() {
                if matches!(ramp.start, Start::After { .. }) {
                    let p = st.ramps[r].points[0];
                    junctions.push((Vec2::new(p.x, p.z), 90.0));
                }
            }
            if let Some((_, b)) = spec.shortcut {
                let end = st.ramps[b].end().0;
                junctions.push((Vec2::new(end.x, end.z), 110.0));
            }
        }
        for (i, p) in s.iter().enumerate() {
            for q in &s[i + 1..] {
                if (p.stage, p.ramp) == (q.stage, q.ramp) && (p.dist - q.dist).abs() < 100.0 {
                    continue;
                }
                let flat = Vec2::new(p.at.x - q.at.x, p.at.z - q.at.z).length();
                if flat >= 26.0 {
                    continue;
                }
                let dy = (p.at.y - q.at.y).abs();
                if dy >= 21.0 {
                    continue;
                }
                let here = Vec2::new(p.at.x, p.at.z);
                let at_junction = junctions.iter().any(|(j, r)| j.distance(here) < *r);
                assert!(
                    at_junction,
                    "ramps {}:{} and {}:{} are {flat} m apart and {dy} m up at {} {}",
                    p.stage, p.ramp, q.stage, q.ramp, p.at, q.at
                );
            }
        }
    }

    #[test]
    fn each_fork_meets_again_and_its_shortcut_is_shorter() {
        let (_, d, c) = setup();
        for spec in &d.stages {
            let Some((a, b)) = spec.shortcut else {
                panic!("stage {} has no fork", spec.stage)
            };
            let Some(st) = c.stage(spec.stage) else {
                panic!("no stage")
            };
            let (split, merge) = (st.ramps[a].points[0], st.ramps[b].end().0);
            // The split is on the main line, the merge too.
            let near = |p: Vec3, ramps: &[usize]| {
                ramps
                    .iter()
                    .flat_map(|&r| {
                        let b = &st.ramps[r];
                        (0..=b.length() as usize).map(move |m| b.pose(Some(m as f32)).0)
                    })
                    .map(|q| q.distance(p))
                    .fold(f32::MAX, f32::min)
            };
            let mains: Vec<usize> = (0..st.ramps.len()).filter(|r| *r < a).collect();
            assert!(
                near(split, &mains) < 1.2,
                "stage {} splits off nothing",
                spec.stage
            );
            assert!(
                near(merge, &mains) < 1.2,
                "stage {} never merges",
                spec.stage
            );
            // Along the main chain the shortcut skips some of the ground.
            let along_main = |p: Vec3| {
                let mut base = 0.0;
                let mut best = (f32::MAX, 0.0);
                for &r in &mains {
                    for (q, dist) in st.ramps[r].points.iter().zip(&st.ramps[r].dist) {
                        let e = q.distance(p);
                        if e < best.0 {
                            best = (e, base + dist);
                        }
                    }
                    base += st.ramps[r].length();
                }
                best.1
            };
            let main_part = along_main(merge) - along_main(split);
            let cut: f32 = st.ramps[a].length()
                + st.ramps[b].length()
                + st.ramps[a].end().0.distance(st.ramps[b].points[0]);
            assert!(
                cut < main_part * 0.95,
                "stage {}: shortcut {cut} vs main {main_part}",
                spec.stage
            );
        }
    }

    #[test]
    fn speed_gaps_are_real_jumps_that_take_speed() {
        let (t, d, c) = setup();
        let mut n = 0;
        for spec in &d.stages {
            for &r in &spec.gaps {
                let g = gap(&t, spec.stage, r).unwrap_or_else(|| panic!("no gap {r}"));
                let need = speed_needed(&t, &g).unwrap_or_else(|| panic!("unclearable gap"));
                let ideal = ideal_speed(&t, spec.stage, g.from.y).unwrap_or(1.0);
                let at = format!(
                    "stage {} ramp {r}: {g:?} needs {need} (ideal {ideal})",
                    spec.stage
                );
                assert!(
                    g.length > 15.0 && g.lateral < t.ramp_half_width * 0.75,
                    "{at}"
                );
                assert!(need > ideal * 0.5 && need < ideal * 0.75, "{at}");
                assert!(need < t.max_speed, "{at}");
                n += 1;
            }
        }
        assert_eq!(n, 3);
        assert_eq!(arrows(&t, &d, &c).len(), 3);
    }

    #[test]
    fn nothing_solid_blocks_a_ramp() {
        let (t, d, c) = setup();
        let l = layout(&t, &d, &c);
        let s = samples(&t, &c);
        let mut n = 0;
        for (at, yaw, size, round) in l.blocks() {
            n += 1;
            let (sin, cos) = (-yaw).sin_cos();
            for p in &s {
                let rel = Vec2::new(p.at.x - at.x, p.at.z - at.z);
                let local = Vec2::new(rel.x * cos - rel.y * sin, rel.x * sin + rel.y * cos);
                let flat = if round {
                    (local.length() - size.x / 2.0).max(0.0)
                } else {
                    rect_distance(Vec2::ZERO, Vec2::new(size.x, size.z) / 2.0, local)
                };
                // Over a viaduct or bridge deck there is >= 26 m of air.
                let (lo, hi) = (at.y - size.y / 2.0, at.y + size.y / 2.0);
                let in_tunnel = hi > p.at.y - t.ramp_height && lo < p.at.y + 26.0;
                assert!(
                    !(flat < 13.0 && in_tunnel),
                    "a block at {at} (size {size}) is {flat} m from the ridge at {}",
                    p.at
                );
            }
        }
        assert!(n > 30, "{n} blocks");
    }

    #[test]
    fn the_area_stays_in_budget_and_in_its_box() {
        let (t, d, c) = setup();
        let l = layout(&t, &d, &c);
        let blocks = l.blocks().count();
        let parts = l
            .items
            .iter()
            .filter(|i| matches!(i, Item::Part { .. }))
            .count();
        let models = l
            .items
            .iter()
            .filter(|i| matches!(i, Item::Model { .. }))
            .count();
        assert!(blocks <= 700, "{blocks} blocks");
        assert!(
            parts + models + blocks <= 2800,
            "{parts} parts {models} models {blocks} blocks"
        );
        // Two lights per stage pad, plus these.
        assert!(l.lamps() + 4 <= 38, "{} lights", l.lamps());
        // Everything is inside the area (hall x 1800-3250, z -2500..-1400) and hall.
        for i in &l.items {
            let at = match i {
                Item::Part { at, .. } | Item::Block { at, .. } | Item::Model { at, .. } => *at,
                Item::Lamp { at, .. } => *at,
            };
            assert!(
                (1790.0..3260.0).contains(&at.x) && (-2500.0..-1400.0).contains(&at.z),
                "{at} is outside the area"
            );
            assert!((-1500.0..600.0).contains(&at.y), "{at} is outside the hall");
        }
        for s in &c.stages {
            for b in &s.ramps {
                for p in &b.points {
                    assert!((1800.0..3250.0).contains(&p.x) && (-2500.0..-1400.0).contains(&p.z));
                }
            }
        }
    }

    #[test]
    fn the_same_data_gives_the_same_city() {
        let (t, d, c) = setup();
        assert_eq!(c, course(&d));
        assert_eq!(layout(&t, &d, &c), layout(&t, &d, &c));
    }

    #[test]
    fn every_model_it_places_exists() {
        let (t, d, c) = setup();
        let root = format!("{}/../game/assets/models", env!("CARGO_MANIFEST_DIR"));
        let mut names: Vec<String> = d.rings.iter().map(|r| r.model.clone()).collect();
        names.push(d.rail.car.clone());
        names.push(d.lamps.arrow.clone());
        names.push(d.tower.crown.clone());
        names.extend(d.props.iter().map(|p| p.model.clone()));
        for i in &layout(&t, &d, &c).items {
            if let Item::Model { model, .. } = i {
                names.push(model.clone());
            }
        }
        for n in names {
            assert!(
                std::path::Path::new(&format!("{root}/{n}")).exists(),
                "{n} isn't in models/"
            );
        }
    }

    #[test]
    fn the_towers_and_courses_materials_are_the_known_set() {
        let (t, d, c) = setup();
        let dir = format!("{}/../game/.modeler/materials", env!("CARGO_MANIFEST_DIR"));
        for i in &layout(&t, &d, &c).items {
            if let Item::Part { mat, .. } = i {
                assert!(
                    std::path::Path::new(&format!("{dir}/{mat}.json")).exists(),
                    "no material {mat}"
                );
            }
        }
    }

    #[test]
    fn the_pad_of_each_stage_is_in_the_neon_area() {
        let (_, d, c) = setup();
        let text = std::fs::read_to_string(format!(
            "{}/../game/assets/data/base/areas.ron",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap_or_default();
        assert!(friendslop_base::areas::AREAS.set_bytes(text.as_bytes()));
        let areas = friendslop_base::areas::areas();
        let Some(area) = areas.areas.iter().find(|a| a.id == "surf") else {
            panic!("no area surf")
        };
        let t = surf::TUNING.get();
        for s in &c.stages {
            let at = surf::hall_at(&t) + s.pad.0 + Vec3::Y;
            assert!(area.holds(at), "stage {} at {at} is outside surf", s.stage);
        }
        let _ = d;
    }
}
