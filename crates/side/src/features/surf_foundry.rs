//! The foundry at the end of the surf map (see `docs/design/surf-areas-brief.md`):
//! stages 14, 15 and 16 (0-based 13-15), a molten industrial complex that ends
//! on the course's finish pad. Furnace stacks, cooling towers, crane gantries,
//! conveyors, gears and pistons stand round the ramps over a lava sea, and a
//! gold arch is the finish.
//!
//! * Stage 14 "Slag Run" teaches the signature: the line splits after a short
//!   run-up into a wide main bend and a straight shortcut with a jump over the
//!   slag (a quick hop for the main line).
//! * Stage 15 "The Spiral" winds a full turn round the Great Furnace (a
//!   helix that passes under itself) and cruises out; the shortcut plunges
//!   steeply past it and jumps a lava channel, between two pillars.
//! * Stage 16 "Cooling Alley" opens fast, cruises calmly down an alley of
//!   cooling towers and crane gantries, then splits again for the finish: a
//!   big sweeping bend, or a straight run with a jump and pillars to dodge.
//!
//! Both sides of each split end side by side on the next pad. Everything here
//! is a pure function of `data/side/surf_foundry.ron` and the course it
//! holds; the game never calls it (the generator example bakes it into the
//! modeler document `surf-foundry-map`, and `maps/surf_foundry.bsn` is what
//! runs).

use std::f32::consts::PI;

use bevy::math::{Quat, Vec3};
use serde::Deserialize;

use crate::rules::surf::{self, Pad, Stage, Tuning, ridge};

pub static FOUNDRY: friendslop_data::Data<Foundry> =
    friendslop_data::Data::ron("data/side/surf_foundry.ron");

/// What a ramp is for.
#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The run-up off a pad, before a split.
    #[default]
    Approach,
    /// A calm cruise between set pieces.
    Alley,
    /// The approachable line of a split.
    Main,
    /// The harder, shorter line of a split.
    Shortcut,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct RampDef {
    pub stage: usize,
    pub role: Role,
    /// The ridge's points, hall coordinates.
    pub points: Vec<(f32, f32, f32)>,
}

/// How a stage's ramps look (a `RampLook` marker).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Look {
    pub stage: u32,
    pub colour: (f32, f32, f32),
    pub glow: f32,
    pub rough: f32,
    pub metal: f32,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Supports {
    pub every: f32,
    pub radius: f32,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Terrace {
    pub step: f32,
    pub below: f32,
    pub width: f32,
    pub every: f32,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Great {
    pub at: (f32, f32),
    pub radius: f32,
    pub over: f32,
    pub lake: f32,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Walls {
    pub north: f32,
    pub south: f32,
    pub thick: f32,
    pub top: f32,
    pub falls: usize,
    pub windows: usize,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Pillar {
    pub lateral: f32,
    pub radius: f32,
    pub below: f32,
    pub above: f32,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Light {
    pub lumens: f32,
    pub range: f32,
    pub colour: (f32, f32, f32),
    /// A lamp over the main line every this many metres.
    pub every: f32,
    pub up: f32,
    /// Only this many stacks get a lamp.
    pub stacks: usize,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Arch {
    pub radius: f32,
    pub tube: f32,
    pub x: f32,
    pub pylon: f32,
    pub segments: usize,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Foundry {
    /// Bottoms of the pads' pillars (m).
    pub floor: f32,
    /// Top of the lava sea (m, never solid).
    pub sea: f32,
    pub pad_size: (f32, f32, f32),
    /// Width of a pad's pillar (m).
    pub pillar: f32,
    /// The pads of stages 13, 14 and 15.
    pub pads: Vec<(f32, f32, f32)>,
    pub finish: (f32, f32, f32),
    pub ramps: Vec<RampDef>,
    pub speed_gaps: Vec<(usize, usize)>,
    pub looks: Vec<Look>,
    pub seed: u32,
    pub supports: Supports,
    pub terrace: Terrace,
    /// Furnace stacks: x, z, radius, how high over the ridge nearby.
    pub stacks: Vec<(f32, f32, f32, f32)>,
    /// Cooling towers: x, z, scale.
    pub towers: Vec<(f32, f32, f32)>,
    pub great: Great,
    /// Crane gantries over the course: x, half span, height over the ridge.
    pub gantries: Vec<(f32, f32, f32)>,
    /// Conveyor belts along +x: x, z, length.
    pub belts: Vec<(f32, f32, f32)>,
    /// Turntable gears: x, z, scale, rpm, how far under the ridge.
    pub gears: Vec<(f32, f32, f32, f32, f32)>,
    /// Pistons: x, z.
    pub pistons: Vec<(f32, f32)>,
    /// Sparks: how many.
    pub sparks: usize,
    pub walls: Walls,
    /// Pillars to dodge on the shortcuts: stage, ramp (index in the
    /// stage's list), metres along it, side (-1 or 1).
    pub obstacles: Vec<(usize, usize, f32, f32)>,
    pub pillar_shape: Pillar,
    pub light: Light,
    pub arch: Arch,
}

// ------------------------------------------------------------- the course

/// A pad as (centre, size).
fn pad_of(d: &Foundry, centre: (f32, f32, f32)) -> Pad {
    (centre, d.pad_size)
}

/// The pads of the foundry in order: stage 13, 14, 15 and the finish.
pub fn pads(d: &Foundry) -> Vec<Pad> {
    d.pads
        .iter()
        .copied()
        .chain([d.finish])
        .map(|c| pad_of(d, c))
        .collect()
}

/// A pad box and the pillar under it down to the floor, for each pad.
pub fn pad_blocks(d: &Foundry) -> Vec<Pad> {
    let mut out = Vec::new();
    for (c, s) in pads(d) {
        out.push((c, s));
        let top = c.1 - s.1 / 2.0;
        let h = top - d.floor;
        out.push(((c.0, d.floor + h / 2.0, c.2), (d.pillar, h, d.pillar)));
    }
    out
}

/// The stage number the foundry starts at.
pub fn first_stage(d: &Foundry) -> usize {
    d.ramps.iter().map(|r| r.stage).min().unwrap_or(13)
}

/// The tuning with the foundry's stages (and the finish) in it, over `base`:
/// the stages before it are left empty.
pub fn course(base: &Tuning, d: &Foundry) -> Tuning {
    let mut t = base.clone();
    let first = first_stage(d);
    let last = first + d.pads.len() - 1;
    t.stages = (0..=last)
        .map(|i| {
            if i < first {
                return Stage::default();
            }
            Stage {
                pad: pad_of(d, d.pads[i - first]),
                ramps: d
                    .ramps
                    .iter()
                    .filter(|r| r.stage == i)
                    .map(|r| r.points.clone())
                    .collect(),
            }
        })
        .collect();
    t.finish = pad_of(d, d.finish);
    t.speed_gaps = d.speed_gaps.clone();
    t.blocks = pad_blocks(d);
    t
}

/// Every ridge sample of the foundry (finer than the ramp's own).
pub fn samples(t: &Tuning, d: &Foundry) -> Vec<Vec3> {
    d.ramps
        .iter()
        .flat_map(|r| ridge(&r.points, t.ramp_samples * 2))
        .collect()
}

/// The ridge's height nearest to (x, z): the reference for scenery.
pub fn ref_y(samples: &[Vec3], x: f32, z: f32) -> f32 {
    samples
        .iter()
        .min_by(|a, b| {
            let da = (a.x - x).powi(2) + (a.z - z).powi(2);
            let db = (b.x - x).powi(2) + (b.z - z).powi(2);
            da.total_cmp(&db)
        })
        .map_or(0.0, |p| p.y)
}

/// How far (m) across the ground (x, z) is from the nearest ridge point.
pub fn ridge_distance(samples: &[Vec3], x: f32, z: f32) -> f32 {
    samples
        .iter()
        .map(|p| ((p.x - x).powi(2) + (p.z - z).powi(2)).sqrt())
        .fold(f32::MAX, f32::min)
}

fn v3((x, y, z): (f32, f32, f32)) -> Vec3 {
    Vec3::new(x, y, z)
}

/// The yaw that turns a box's x axis along `f` (flat).
pub fn yaw_of(f: Vec3) -> f32 {
    (-f.z).atan2(f.x)
}

/// Points along a line every `every` metres (first at half): position and
/// flat forward.
pub fn stations(line: &[Vec3], every: f32) -> Vec<(Vec3, Vec3)> {
    let mut out = Vec::new();
    let mut next = every / 2.0;
    let mut walked = 0.0;
    for w in line.windows(2) {
        let seg = (w[1] - w[0]).with_y(0.0);
        let len = seg.length();
        if len < 1e-4 {
            continue;
        }
        while walked + len >= next {
            let t = (next - walked) / len;
            out.push((w[0].lerp(w[1], t), seg / len));
            next += every;
        }
        walked += len;
    }
    out
}

/// The point `s` metres along a line, and its flat forward.
pub fn at_distance(line: &[Vec3], s: f32) -> Option<(Vec3, Vec3)> {
    let mut walked = 0.0;
    for w in line.windows(2) {
        let seg = (w[1] - w[0]).with_y(0.0);
        let len = seg.length();
        if len < 1e-4 {
            continue;
        }
        if walked + len >= s {
            return Some((w[0].lerp(w[1], (s - walked) / len), seg / len));
        }
        walked += len;
    }
    None
}

// -------------------------------------------------------------- the scene

#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    Box(Vec3),
    /// Upright cylinder: radius, height.
    Cyl(f32, f32),
    Cone(f32, f32),
    /// A hollow solid of revolution: half cross-section (radius, y).
    Revolve(Vec<(f32, f32)>),
    /// Lettering facing +z: the text and its capital height.
    Text(String, f32),
}

/// A piece of the map's own geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub shape: Shape,
    pub at: Vec3,
    pub rot: Quat,
    pub mat: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Blk {
    pub centre: Vec3,
    pub size: Vec3,
    pub round: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lamp {
    pub at: Vec3,
    pub lumens: f32,
    pub range: f32,
    pub rgb: (f32, f32, f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion {
    Still,
    Bob { height: f32, hz: f32, phase: f32 },
    Spin { rpm: f32 },
}

/// A placed model file.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub model: &'static str,
    pub at: Vec3,
    pub yaw: f32,
    pub scale: Vec3,
    pub motion: Motion,
}

/// A pillar standing in a ramp's face on purpose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Obstacle {
    pub stage: usize,
    pub ramp: usize,
    /// The ridge point beside it.
    pub ridge: Vec3,
    pub side: f32,
    pub centre: Vec3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub items: Vec<Item>,
    pub blocks: Vec<Blk>,
    pub lamps: Vec<Lamp>,
    pub placed: Vec<Placed>,
    pub obstacles: Vec<Obstacle>,
}

pub const GEAR: &str = "features/surf_foundry_gear.gltf";
pub const PISTON: &str = "features/surf_foundry_piston.gltf";
pub const SPARK: &str = "features/surf_foundry_spark.gltf";

const LAVA: &str = "sf-fd-lava";
const BRICK: &str = "sf-fd-brick";
const HULL: &str = "sf-hull";
const GLOW: &str = "sf-neon-orange";
const GOLD: &str = "sf-neon-gold";
const STONE: &str = "sf-stone";
const YELLOW: &str = "sf-crane-yellow";
const CRANE_RED: &str = "sf-crane-red";
const BLACK: &str = "sf-paint-black";
const LIME: &str = "sf-neon-lime";

/// The materials the scene uses.
pub const MATERIALS: [&str; 11] = [
    LAVA,
    BRICK,
    HULL,
    GLOW,
    GOLD,
    STONE,
    YELLOW,
    CRANE_RED,
    BLACK,
    LIME,
    "sf-neon-red",
];

/// A tiny deterministic generator (xorshift).
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / 16_777_216.0
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.next()
    }
}

impl Scene {
    fn put(&mut self, shape: Shape, at: Vec3, rot: Quat, mat: &'static str) {
        self.items.push(Item {
            shape,
            at,
            rot,
            mat,
        });
    }
    fn bx(&mut self, at: Vec3, size: Vec3, yaw: f32, mat: &'static str) {
        self.put(Shape::Box(size), at, Quat::from_rotation_y(yaw), mat);
    }
    fn cyl(&mut self, at: Vec3, radius: f32, height: f32, mat: &'static str) {
        self.put(Shape::Cyl(radius, height), at, Quat::IDENTITY, mat);
    }
    fn cone(&mut self, at: Vec3, radius: f32, height: f32, mat: &'static str) {
        self.put(Shape::Cone(radius, height), at, Quat::IDENTITY, mat);
    }
    /// A column from the sea up to `top`.
    fn column(&mut self, x: f32, z: f32, top: f32, sea: f32, radius: f32, mat: &'static str) {
        let h = top - sea;
        self.cyl(Vec3::new(x, sea + h / 2.0, z), radius, h, mat);
    }
    fn text(&mut self, s: &str, size: f32, at: Vec3, mat: &'static str) {
        // Facing -x, the way the surfers come.
        self.put(
            Shape::Text(s.to_owned(), size),
            at,
            Quat::from_rotation_y(-PI / 2.0),
            mat,
        );
    }
    fn lamp(&mut self, at: Vec3, d: &Foundry) {
        self.lamps.push(Lamp {
            at,
            lumens: d.light.lumens,
            range: d.light.range,
            rgb: d.light.colour,
        });
    }
}

/// The whole foundry: everything but the ramps themselves.
pub fn scene(t: &Tuning, d: &Foundry) -> Scene {
    let mut s = Scene::default();
    let smp = samples(t, d);
    let lines: Vec<Vec<Vec3>> = d
        .ramps
        .iter()
        .map(|r| ridge(&r.points, t.ramp_samples))
        .collect();
    let mut rng = Rng(d.seed.max(1));
    let sea = d.sea;

    // The lava sea and the walls round the hall.
    let (x0, x1) = (4350.0, 5650.0);
    let (zn, zs) = (d.walls.north, d.walls.south);
    s.bx(
        Vec3::new((x0 + x1) / 2.0, sea - 1.0, (zn + zs) / 2.0),
        Vec3::new(x1 - x0 + 200.0, 2.0, (zs - zn).abs() + 200.0),
        0.0,
        LAVA,
    );
    walls(&mut s, d, &mut rng);

    // Supports and the lava terraces under every ramp.
    for line in &lines {
        for (p, _) in stations(line, d.supports.every) {
            s.column(p.x, p.z, p.y - t.ramp_height, sea, d.supports.radius, HULL);
            s.cyl(
                Vec3::new(p.x, p.y - t.ramp_height - 1.0, p.z),
                d.supports.radius + 0.6,
                0.9,
                GLOW,
            );
        }
        for (p, f) in stations(line, d.terrace.every) {
            if (p.x - d.great.at.0).hypot(p.z - d.great.at.1) < d.great.lake {
                continue;
            }
            let y = ((p.y - d.terrace.below) / d.terrace.step).floor() * d.terrace.step;
            s.bx(
                Vec3::new(p.x, y - 1.0, p.z),
                Vec3::new(d.terrace.every + 6.0, 2.0, d.terrace.width),
                yaw_of(f),
                LAVA,
            );
        }
    }

    great_furnace(&mut s, t, d);
    stacks(&mut s, d, &smp);
    towers(&mut s, d, &smp);
    gantries(&mut s, d, &smp);
    belts(&mut s, d, &smp);
    machines(&mut s, d, &smp);
    sparks(&mut s, d, &smp, &mut rng);
    obstacles(&mut s, t, d, &lines);
    dressing(&mut s, t, d, &lines);
    arch(&mut s, d);
    // Lamps over the main line.
    for (r, line) in d.ramps.iter().zip(&lines) {
        if r.role == Role::Shortcut {
            continue;
        }
        for (p, _) in stations(line, d.light.every) {
            s.lamp(p + Vec3::Y * d.light.up, d);
        }
    }
    s
}

fn walls(s: &mut Scene, d: &Foundry, rng: &mut Rng) {
    let w = &d.walls;
    let (x0, x1) = (4350.0, 5650.0);
    let bottom = d.sea - 6.0;
    let h = w.top - bottom;
    for (z, toward) in [(w.north, 1.0_f32), (w.south, -1.0)] {
        s.bx(
            Vec3::new((x0 + x1) / 2.0, bottom + h / 2.0, z),
            Vec3::new(x1 - x0 + 160.0, h, w.thick),
            0.0,
            BRICK,
        );
        let face = z + toward * (w.thick / 2.0);
        // Lava falls pouring from spouts near the top.
        for k in 0..w.falls {
            let x = x0 + (k as f32 + 0.5) * (x1 - x0) / w.falls as f32 + rng.range(-20.0, 20.0);
            let top = w.top - rng.range(18.0, 60.0);
            s.bx(
                Vec3::new(x, top - 1.5, face + toward * 2.4),
                Vec3::new(9.0, 3.0, 5.0),
                0.0,
                HULL,
            );
            let fall = top - 1.5 - d.sea;
            s.bx(
                Vec3::new(x, d.sea + fall / 2.0, face + toward * 0.5),
                Vec3::new(rng.range(5.0, 8.0), fall, 0.8),
                0.0,
                LAVA,
            );
        }
        // Rows of glowing slits.
        for row in 0..2 {
            for k in 0..w.windows {
                let x = x0 + (k as f32 + 0.5) * (x1 - x0) / w.windows as f32;
                s.bx(
                    Vec3::new(x, w.top - 70.0 - row as f32 * 60.0, face + toward * 0.2),
                    Vec3::new(5.0, 18.0, 0.6),
                    0.0,
                    GLOW,
                );
            }
        }
    }
}

fn great_furnace(s: &mut Scene, t: &Tuning, d: &Foundry) {
    let g = &d.great;
    let (x, z) = g.at;
    // The helix: stage 13+1's main ramp is the one that starts at its top.
    let top_ridge = d
        .ramps
        .iter()
        .filter(|r| r.role == Role::Main)
        .flat_map(|r| ridge(&r.points, t.ramp_samples))
        .filter(|p| Vec3::new(p.x - x, 0.0, p.z - z).length() < g.lake)
        .map(|p| p.y)
        .fold(f32::MIN, f32::max);
    let top = top_ridge + g.over;
    s.column(x, z, top, d.sea, g.radius, BRICK);
    // Glowing rings and mouths at the helix's levels.
    for k in 0..6 {
        let y = top_ridge - 2.0 - k as f32 * 5.5;
        s.cyl(Vec3::new(x, y, z), g.radius + 0.5, 1.4, GLOW);
    }
    for k in 0..4 {
        let a = k as f32 * PI / 2.0 + 0.4;
        let dir = Vec3::new(a.cos(), 0.0, a.sin());
        for n in 0..2 {
            s.bx(
                Vec3::new(x, top_ridge - 6.0 - n as f32 * 11.0, z) + dir * (g.radius + 0.9),
                Vec3::new(2.4, 8.0, 12.0),
                yaw_of(Vec3::new(-dir.z, 0.0, dir.x)),
                LAVA,
            );
        }
    }
    s.cone(Vec3::new(x, top + 4.0, z), g.radius + 2.0, 8.0, HULL);
    s.column(x, z, top + 40.0, top + 7.0, g.radius * 0.45, BRICK);
    s.cone(Vec3::new(x, top + 52.0, z), g.radius * 0.5, 24.0, GLOW);
    s.cyl(Vec3::new(x, top + 40.5, z), g.radius * 0.5, 1.2, GLOW);
    s.lamp(Vec3::new(x, top + 50.0, z), d);
    s.lamp(Vec3::new(x, top_ridge - 6.0, z), d);
    // The lake round its foot.
    let y = ((top_ridge - d.terrace.below - 8.0) / d.terrace.step).floor() * d.terrace.step;
    s.cyl(Vec3::new(x, y - 1.0, z), g.lake, 2.0, LAVA);
}

fn stacks(s: &mut Scene, d: &Foundry, smp: &[Vec3]) {
    for (i, &(x, z, r, over)) in d.stacks.iter().enumerate() {
        let top = ref_y(smp, x, z) + over;
        s.column(x, z, top, d.sea, r, BRICK);
        let mut y = top - 5.0;
        while y > d.sea + 20.0 {
            s.cyl(Vec3::new(x, y, z), r + 0.4, 1.2, GLOW);
            y -= 48.0;
        }
        s.cone(Vec3::new(x, top + 3.0, z), r + 1.4, 6.0, HULL);
        s.cyl(Vec3::new(x, top + 6.2, z), r * 0.8, 0.6, LAVA);
        s.cone(Vec3::new(x, top + 15.0, z), r * 0.7, 18.0, GLOW);
        if i < d.light.stacks {
            s.lamp(Vec3::new(x, top + 14.0, z), d);
        }
    }
}

/// A cooling tower's half cross-section as a hollow shell, `scale` times the
/// base size (24 m radius, 64 m tall).
pub fn tower_profile(scale: f32) -> Vec<(f32, f32)> {
    let (h, throat, rb, rt, rn) = (64.0_f32, 0.68_f32, 24.0_f32, 18.5_f32, 15.5_f32);
    let r = |y: f32| {
        let yt = h * throat;
        if y < yt {
            rn + (rb - rn) * ((yt - y) / yt).powi(2)
        } else {
            rn + (rt - rn) * ((y - yt) / (h - yt)).powi(2)
        }
    };
    let ys: Vec<f32> = (0..=10).map(|i| h * i as f32 / 10.0).collect();
    let outer = ys.iter().map(|&y| (r(y) * scale, y * scale));
    let inner = ys.iter().rev().map(|&y| ((r(y) - 1.4) * scale, y * scale));
    outer.chain(inner).collect()
}

fn towers(s: &mut Scene, d: &Foundry, smp: &[Vec3]) {
    for &(x, z, sc) in &d.towers {
        let base = ref_y(smp, x, z) - 52.0 * sc;
        s.put(
            Shape::Revolve(tower_profile(sc)),
            Vec3::new(x, base, z),
            Quat::IDENTITY,
            STONE,
        );
        s.column(x, z, base + 2.0, d.sea, 7.0 * sc, HULL);
        // The rim glows like it's full of steam and embers.
        s.cyl(Vec3::new(x, base + 64.0 * sc, z), 18.5 * sc, 0.5, GLOW);
        s.cyl(
            Vec3::new(x, base + 10.0 * sc, z),
            24.0 * sc + 0.3,
            1.0,
            GLOW,
        );
    }
}

fn gantries(s: &mut Scene, d: &Foundry, smp: &[Vec3]) {
    let zc = -1935.0;
    for (i, &(x, half, over)) in d.gantries.iter().enumerate() {
        let beam = ref_y(smp, x, zc) + over;
        for side in [-1.0, 1.0] {
            let h = beam - d.sea;
            s.bx(
                Vec3::new(x, d.sea + h / 2.0, zc + side * half),
                Vec3::new(2.6, h, 2.6),
                0.0,
                YELLOW,
            );
        }
        s.bx(
            Vec3::new(x, beam, zc),
            Vec3::new(3.6, 3.6, half * 2.0 + 6.0),
            0.0,
            YELLOW,
        );
        let shift = ((i as f32 * 2.3).sin()) * half * 0.5;
        s.bx(
            Vec3::new(x, beam - 3.4, zc + shift),
            Vec3::new(6.0, 3.0, 5.0),
            0.0,
            CRANE_RED,
        );
        s.cyl(Vec3::new(x, beam - 8.5, zc + shift), 0.2, 8.0, HULL);
        s.cyl(Vec3::new(x, beam - 14.5, zc + shift), 3.2, 5.0, HULL);
        s.cyl(Vec3::new(x, beam - 12.1, zc + shift), 2.9, 0.4, LAVA);
        if i == 0 {
            // The first gantry is the foundry's sign.
            s.bx(
                Vec3::new(x - 2.4, beam - 6.5, zc),
                Vec3::new(0.8, 8.0, half * 2.0 - 4.0),
                0.0,
                BLACK,
            );
            s.text("FOUNDRY", 5.0, Vec3::new(x - 3.1, beam - 9.1, zc), GOLD);
        }
    }
}

fn belts(s: &mut Scene, d: &Foundry, smp: &[Vec3]) {
    for &(x, z, len) in &d.belts {
        let y = ref_y(smp, x, z) - 15.0;
        s.bx(Vec3::new(x, y, z), Vec3::new(len, 1.2, 4.6), 0.0, BLACK);
        for side in [-1.0, 1.0] {
            s.bx(
                Vec3::new(x, y + 0.9, z + side * 2.5),
                Vec3::new(len, 0.9, 0.5),
                0.0,
                HULL,
            );
        }
        let n = (len / 6.0) as usize;
        for k in 0..n {
            let px = x - len / 2.0 + 3.0 + k as f32 * 6.0;
            s.bx(
                Vec3::new(px, y + 0.65, z),
                Vec3::new(0.6, 0.15, 3.8),
                0.0,
                GLOW,
            );
            if k % 2 == 1 {
                s.bx(
                    Vec3::new(px + 1.4, y + 1.3, z),
                    Vec3::new(2.6, 1.4, 1.8),
                    0.0,
                    LAVA,
                );
            }
        }
        let legs = (len / 24.0).ceil() as usize + 1;
        for k in 0..legs {
            let px = x - len / 2.0 + k as f32 * len / (legs - 1).max(1) as f32;
            s.column(px, z, y - 0.6, d.sea, 0.5, HULL);
        }
    }
}

fn machines(s: &mut Scene, d: &Foundry, smp: &[Vec3]) {
    for (i, &(x, z, sc, rpm, drop)) in d.gears.iter().enumerate() {
        let y = ref_y(smp, x, z) - drop;
        s.column(x, z, y, d.sea, 1.3 * sc.sqrt(), HULL);
        s.placed.push(Placed {
            model: GEAR,
            at: Vec3::new(x, y, z),
            yaw: i as f32,
            scale: Vec3::splat(sc),
            motion: Motion::Spin { rpm },
        });
    }
    for (i, &(x, z)) in d.pistons.iter().enumerate() {
        let base = ref_y(smp, x, z) - 22.0;
        s.column(x, z, base, d.sea, 1.0, HULL);
        s.cyl(Vec3::new(x, base + 6.0, z), 2.6, 12.0, HULL);
        s.cyl(Vec3::new(x, base + 12.1, z), 2.9, 0.5, GLOW);
        s.placed.push(Placed {
            model: PISTON,
            at: Vec3::new(x, base + 12.4, z),
            yaw: 0.0,
            scale: Vec3::ONE,
            motion: Motion::Bob {
                height: 4.5,
                hz: 0.35,
                phase: i as f32 * 0.9,
            },
        });
    }
}

fn sparks(s: &mut Scene, d: &Foundry, smp: &[Vec3], rng: &mut Rng) {
    if smp.is_empty() {
        return;
    }
    for i in 0..d.sparks {
        let p = smp[(rng.next() * smp.len() as f32) as usize % smp.len()];
        let a = rng.range(0.0, 2.0 * PI);
        let r = rng.range(12.0, 34.0);
        s.placed.push(Placed {
            model: SPARK,
            at: p + Vec3::new(a.cos() * r, rng.range(-12.0, 24.0), a.sin() * r),
            yaw: i as f32,
            scale: Vec3::ONE,
            motion: Motion::Bob {
                height: rng.range(1.5, 4.0),
                hz: rng.range(0.2, 0.5),
                phase: rng.range(0.0, 6.0),
            },
        });
    }
}

/// Pillars in a face on the shortcuts: a solid, and what it looks like.
fn obstacles(s: &mut Scene, t: &Tuning, d: &Foundry, lines: &[Vec<Vec3>]) {
    let o = &d.pillar_shape;
    for &(stage, slot, along, side) in &d.obstacles {
        let Some(i) = nth_ramp(d, stage, slot) else {
            continue;
        };
        let Some((p, f)) = at_distance(&lines[i], along) else {
            continue;
        };
        let n = Vec3::new(f.z, 0.0, -f.x);
        let c = p + n * side * o.lateral;
        let h = o.below + o.above;
        let centre = Vec3::new(c.x, p.y - o.below + h / 2.0, c.z);
        s.blocks.push(Blk {
            centre,
            size: Vec3::new(o.radius * 2.0, h, o.radius * 2.0),
            round: true,
        });
        s.cyl(centre, o.radius, h, HULL);
        s.cyl(
            centre + Vec3::Y * (h / 2.0 - 1.4),
            o.radius + 0.25,
            0.8,
            GLOW,
        );
        s.cone(
            centre + Vec3::Y * (h / 2.0 + 0.9),
            o.radius + 0.1,
            1.8,
            LAVA,
        );
        let _ = t;
        s.obstacles.push(Obstacle {
            stage,
            ramp: slot,
            ridge: p,
            side,
            centre,
        });
    }
}

/// The index in `d.ramps` of stage `stage`'s `slot`-th ramp.
pub fn nth_ramp(d: &Foundry, stage: usize, slot: usize) -> Option<usize> {
    d.ramps
        .iter()
        .enumerate()
        .filter(|(_, r)| r.stage == stage)
        .nth(slot)
        .map(|(i, _)| i)
}

/// Pad neon, stage numbers, lane lights and the shortcut arrows.
fn dressing(s: &mut Scene, t: &Tuning, d: &Foundry, lines: &[Vec<Vec3>]) {
    let first = first_stage(d);
    for (n, (c, size)) in pads(d).into_iter().enumerate() {
        let finish = n == d.pads.len();
        let mat = if finish { GOLD } else { GLOW };
        let (cv, sv) = (v3(c), v3(size));
        for (at, sz) in crate::features::surf::edge_strips((cv, sv), 0.24) {
            s.bx(at, sz, 0.0, mat);
        }
        let top = cv.y + sv.y / 2.0;
        s.lamp(cv + Vec3::Y * 5.0, d);
        if !finish {
            // The stage number, on a post at the pad's back corner.
            let stage = first + n + 1;
            s.bx(
                Vec3::new(cv.x - sv.x / 2.0 + 0.6, top + 2.0, cv.z + sv.z / 2.0 - 0.6),
                Vec3::new(0.3, 4.0, 0.3),
                0.0,
                HULL,
            );
            s.text(
                &stage.to_string(),
                2.4,
                Vec3::new(cv.x - sv.x / 2.0 + 0.2, top + 4.4, cv.z + sv.z / 2.0 - 0.6),
                mat,
            );
        }
    }
    // Lane lights beside every ramp.
    for r in &d.ramps {
        for at in
            crate::features::surf::lane_lights(t, &crate::features::surf::DRESSING.get(), &r.points)
        {
            s.bx(at, Vec3::splat(0.5), 0.0, GLOW);
        }
    }
    // Arrows over the gaps of the shortcuts (each stage's first Shortcut
    // ramp is followed by its second across a jump).
    for stage in first..first + d.pads.len() {
        let shorts: Vec<usize> = d
            .ramps
            .iter()
            .enumerate()
            .filter(|(_, r)| r.stage == stage && r.role == Role::Shortcut)
            .map(|(i, _)| i)
            .collect();
        let [a, b] = shorts[..] else { continue };
        let (end, fwd) = (*lines[a].last().unwrap_or(&Vec3::ZERO), {
            let l = &lines[a];
            (l[l.len() - 1] - l[l.len() - 2])
                .with_y(0.0)
                .normalize_or(Vec3::X)
        });
        let start = lines[b].first().copied().unwrap_or(end);
        let mid = (end + start) / 2.0 + Vec3::Y * 8.0;
        let n = Vec3::new(fwd.z, 0.0, -fwd.x);
        for k in 0..3 {
            let tip = mid + fwd * (k as f32 * 3.6 - 3.6);
            for sg in [-1.0_f32, 1.0] {
                let arm = -fwd * 0.766 + n * sg * 0.643;
                s.bx(tip + arm * 2.6, Vec3::new(5.2, 0.5, 1.1), yaw_of(arm), LIME);
            }
        }
    }
}

/// The finish arch: a gold ring over the course on two pylons.
fn arch(s: &mut Scene, d: &Foundry) {
    let a = &d.arch;
    let f = v3(d.finish);
    let top = f.y + d.pad_size.1 / 2.0;
    let (x, cy) = (f.x + a.x, top + 4.0);
    for side in [-1.0_f32, 1.0] {
        let h = cy + 2.0 - d.sea;
        s.bx(
            Vec3::new(x, d.sea + h / 2.0, f.z + side * a.radius),
            Vec3::new(a.pylon, h, a.pylon),
            0.0,
            HULL,
        );
        s.bx(
            Vec3::new(x, cy + 3.0, f.z + side * a.radius),
            Vec3::new(a.pylon + 0.8, 3.0, a.pylon + 0.8),
            0.0,
            GOLD,
        );
        s.placed.push(Placed {
            model: GEAR,
            at: Vec3::new(x, cy + 5.0, f.z + side * a.radius),
            yaw: 0.0,
            scale: Vec3::splat(0.9),
            motion: Motion::Spin { rpm: -side * 8.0 },
        });
        s.lamp(Vec3::new(x - 4.0, cy + 8.0, f.z + side * a.radius), d);
    }
    for (r, tube, mat) in [
        (a.radius, a.tube, GOLD),
        (a.radius - 4.0, a.tube * 0.5, GLOW),
    ] {
        let n = a.segments.max(6);
        for k in 0..n {
            let th = (k as f32 + 0.5) * PI / n as f32;
            let len = r * PI / n as f32 + 0.5;
            let alpha = (-th.cos()).atan2(-th.sin());
            s.put(
                Shape::Box(Vec3::new(tube * 2.4, tube * 2.0, len)),
                Vec3::new(x, cy + r * th.sin(), f.z + r * th.cos()),
                Quat::from_rotation_x(alpha),
                mat,
            );
        }
    }
    // A sign hanging from the apex.
    let apex = cy + a.radius;
    s.bx(
        Vec3::new(x - 0.6, apex - 7.0, f.z),
        Vec3::new(1.0, 8.0, 34.0),
        0.0,
        BLACK,
    );
    s.text("FINISH", 5.4, Vec3::new(x - 1.4, apex - 9.6, f.z), GOLD);
    s.lamp(Vec3::new(x - 6.0, apex - 6.0, f.z), d);
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::surf::{gap, ideal_speed, speed_needed};
    use std::sync::Arc;

    fn load() -> (Arc<Tuning>, Arc<Foundry>) {
        let root = format!("{}/../game/assets", env!("CARGO_MANIFEST_DIR"));
        let read = |p: &str| match std::fs::read(format!("{root}/{p}")) {
            Ok(b) => b,
            Err(e) => panic!("{p}: {e}"),
        };
        assert!(surf::TUNING.set_bytes(&read(surf::TUNING.path())));
        assert!(
            FOUNDRY.set_bytes(&read(FOUNDRY.path())),
            "foundry.ron doesn't parse"
        );
        (surf::TUNING.get(), FOUNDRY.get())
    }

    fn course_of() -> (Tuning, Arc<Foundry>) {
        let (base, d) = load();
        (course(&base, &d), d)
    }

    /// Radius of curvature at the interior of a ridge (m), skipping the
    /// first and last spans where the spline has no neighbour.
    fn min_radius(t: &Tuning, pts: &[(f32, f32, f32)]) -> f32 {
        let r = ridge(pts, t.ramp_samples);
        let n = t.ramp_samples as usize;
        let mut best = f32::MAX;
        for i in n + 1..r.len().saturating_sub(n + 1) {
            let a = (r[i] - r[i - 1]).with_y(0.0);
            let b = (r[i + 1] - r[i]).with_y(0.0);
            let (la, lb) = (a.length(), b.length());
            if la < 1e-4 || lb < 1e-4 {
                continue;
            }
            let ang = (a.x * b.z - a.z * b.x).abs() / (la * lb);
            if ang > 1e-4 {
                best = best.min((la + lb) / 2.0 / ang.asin());
            }
        }
        best
    }

    #[test]
    fn the_data_file_has_every_field() {
        let (_, d) = load();
        assert!(d.floor < d.sea && d.pillar > 0.0 && d.seed > 0);
        assert_eq!(d.pads.len(), 3);
        assert!(d.supports.every > 0.0 && d.terrace.every > 0.0 && d.great.radius > 0.0);
        assert!(d.stacks.len() >= 10 && d.towers.len() >= 5 && d.gantries.len() >= 4);
        assert!(d.belts.len() >= 3 && d.gears.len() >= 8 && d.pistons.len() >= 8);
        assert!(d.light.lumens > 0.0 && d.arch.radius > 10.0 && d.walls.falls > 0);
        assert!(d.looks.len() == 3 && d.sparks > 0 && d.pillar_shape.radius > 0.0);
        for r in &d.ramps {
            assert!(r.points.len() >= 2, "{r:?}");
        }
        for (stage, slot, along, side) in &d.obstacles {
            assert!(nth_ramp(&d, *stage, *slot).is_some() && *along > 0.0 && side.abs() == 1.0);
        }
    }

    #[test]
    fn the_handoff_pads_are_where_the_brief_says() {
        let (t, d) = course_of();
        // (The course's handoffs since the late packs were steepened.)
        assert_eq!(d.pads[0], (4400.0, -1128.84, -1935.0));
        assert_eq!(d.finish, (5600.0, -1351.26, -1935.0));
        // Each stage starts 12.35 m past its pad, 5.5 m below it, heading east.
        for (n, st) in t.stages.iter().enumerate().skip(first_stage(&d)) {
            let (c, _) = surf::pad(st.pad);
            let first = Vec3::from(st.ramps[0][0]);
            assert!(
                (first - (c + Vec3::new(12.35, -5.5, 0.0))).length() < 0.05,
                "stage {n} starts at {first}"
            );
        }
        // Every end of a stage is 30 m before the next pad, 8.5 above it,
        // within the pad's width (the two sides of a split are 12 m apart).
        for n in first_stage(&d)..first_stage(&d) + 3 {
            let next = if n == first_stage(&d) + 2 {
                surf::pad(t.finish).0
            } else {
                surf::pad(t.stages[n + 1].pad).0
            };
            let mut zs = Vec::new();
            for r in d.ramps.iter().filter(|r| r.stage == n) {
                let e = Vec3::from(r.points.last().copied().unwrap_or_default());
                // Not the ramps that end across a jump: those end in a gap.
                if (e.x - (next.x - 30.0)).abs() < 0.05 {
                    assert!((e.y - (next.y + 8.5)).abs() < 0.05, "stage {n} ends at {e}");
                    zs.push(e.z - next.z);
                }
            }
            zs.sort_by(f32::total_cmp);
            assert_eq!(zs.len(), 2, "stage {n}: two lines land on the next pad");
            assert!(zs.iter().all(|z| z.abs() <= 8.0), "{zs:?}");
            assert!(zs[1] - zs[0] >= 11.5, "the two lines are too close: {zs:?}");
        }
    }

    #[test]
    fn every_ramp_is_a_smooth_gentle_descent() {
        let (t, d) = course_of();
        for r in &d.ramps {
            let line = ridge(&r.points, t.ramp_samples);
            assert!(
                line.windows(2).all(|w| w[1].y <= w[0].y + 1e-3),
                "{:?} goes up",
                r.role
            );
            let len: f32 = line
                .windows(2)
                .map(|w| (w[1] - w[0]).with_y(0.0).length())
                .sum();
            let drop = line[0].y - line[line.len() - 1].y;
            assert!(
                (0.018..0.26).contains(&(drop / len)),
                "stage {} {:?}: slope {}",
                r.stage,
                r.role,
                drop / len
            );
            let radius = min_radius(&t, &r.points);
            assert!(
                radius >= 58.0,
                "stage {} {:?}: radius {radius}",
                r.stage,
                r.role
            );
        }
    }

    #[test]
    fn the_two_lines_of_a_split_never_touch() {
        let (t, d) = course_of();
        for stage in first_stage(&d)..first_stage(&d) + 3 {
            let mains: Vec<Vec<Vec3>> = d
                .ramps
                .iter()
                .filter(|r| r.stage == stage && r.role == Role::Main)
                .map(|r| ridge(&r.points, t.ramp_samples * 2))
                .collect();
            let shorts: Vec<Vec<Vec3>> = d
                .ramps
                .iter()
                .filter(|r| r.stage == stage && r.role == Role::Shortcut)
                .map(|r| ridge(&r.points, t.ramp_samples * 2))
                .collect();
            for m in &mains {
                for sh in &shorts {
                    for a in m {
                        for b in sh {
                            // Overlapping in plan is only a crossover when one
                            // is far above the other.
                            let flat = (*a - *b).with_y(0.0).length();
                            assert!(
                                flat > 12.0 || (a.y - b.y).abs() > 15.0,
                                "stage {stage}: lines meet at {a} / {b}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_main_lines_really_turn() {
        let (t, d) = course_of();
        // Every stage's main line swings well off the middle and back: the
        // heading turns through 120 degrees or more along it.
        for stage in first_stage(&d)..first_stage(&d) + 3 {
            let line: Vec<Vec3> = d
                .ramps
                .iter()
                .filter(|r| r.stage == stage && r.role == Role::Main)
                .flat_map(|r| ridge(&r.points, t.ramp_samples))
                .collect();
            let mut turn = 0.0f32;
            for w in line.windows(3) {
                let (a, b) = (
                    bevy::math::Vec2::new(w[1].x - w[0].x, w[1].z - w[0].z),
                    bevy::math::Vec2::new(w[2].x - w[1].x, w[2].z - w[1].z),
                );
                if a.length() > 0.1 && b.length() > 0.1 {
                    turn += a.angle_to(b).abs();
                }
            }
            assert!(
                turn.to_degrees() > 120.0,
                "stage {stage} turns {}",
                turn.to_degrees()
            );
        }
    }

    #[test]
    fn the_jumps_take_speed_but_not_too_much() {
        let (t, d) = course_of();
        // Every hop between ramps of a stage is short and easy.
        let first = first_stage(&d);
        // Each stage: the dive (0) hops onto either line of the split (1, 3),
        // and the main line hops once (1 -> 2).
        let hops: Vec<(usize, usize, usize)> = (first..first + 3)
            .flat_map(|s| [(s, 0, 1), (s, 0, 3), (s, 1, 2)])
            .collect();
        for (stage, from, to) in hops {
            let pts = |slot: usize| {
                nth_ramp(&d, stage, slot)
                    .map(|i| d.ramps[i].points.clone())
                    .unwrap_or_default()
            };
            let (a, b) = (pts(from), pts(to));
            let mut tmp = t.clone();
            tmp.stages = vec![Stage {
                pad: d.pads.first().map(|c| pad_of(&d, *c)).unwrap_or_default(),
                ramps: vec![a.clone(), b.clone()],
            }];
            let Some(g) = gap(&tmp, 0, 0) else {
                panic!("stage {stage}: no hop {from} -> {to}");
            };
            let Some(need) = speed_needed(&tmp, &g) else {
                panic!("stage {stage}: no speed clears hop {from} -> {to}");
            };
            let fall = {
                let (c, s) = surf::pad(t.stages[stage].pad);
                c.y + s.y / 2.0 - g.from.y
            };
            let ideal = (t.wish_speed.powi(2) + 2.0 * t.gravity * fall).sqrt();
            assert!(
                g.length < 12.0 && need < ideal * 0.6,
                "stage {stage} hop {from}->{to}: {g:?} needs {need} of {ideal}"
            );
        }
        for (stage, slot) in [(first, 3), (first + 1, 3), (first + 2, 3)] {
            let Some(g) = gap(&t, stage, slot) else {
                panic!("no gap after stage {stage} ramp {slot}");
            };
            let Some(need) = speed_needed(&t, &g) else {
                panic!("stage {stage}: no speed clears it");
            };
            let ideal = ideal_speed(&t, stage, g.from.y).unwrap_or(1.0);
            let at = format!("stage {stage} ramp {slot}: {g:?} needs {need} (ideal {ideal})");
            assert!(
                g.length > 15.0 && g.lateral < t.ramp_half_width * 0.75,
                "{at}"
            );
            assert!(need > ideal * 0.5 && need < ideal * 0.75, "{at}");
        }
        // The marked ones are the speed checks.
        for &(stage, ramp) in &d.speed_gaps {
            assert!(gap(&t, stage, ramp).is_some());
        }
    }

    #[test]
    fn nothing_solid_blocks_a_ramp() {
        let (t, d) = course_of();
        let sc = scene(&t, &d);
        let smp = samples(&t, &d);
        // The only solids are the pillars in a face, and the pads' own.
        assert_eq!(sc.blocks.len(), d.obstacles.len());
        for b in &sc.blocks {
            let near = smp
                .iter()
                .map(|p| (p.x - b.centre.x).hypot(p.z - b.centre.z))
                .fold(f32::MAX, f32::min);
            // Beside the ridge, in a face, not on it.
            assert!(near > b.size.x / 2.0 + 1.5, "{b:?} is on the ridge");
        }
        // Each pillar leaves the other face free (the next one on a ramp is
        // on the other side or far along) and the rest of its own face.
        let half = t.ramp_half_width;
        for o in &sc.obstacles {
            let lateral = (o.centre - o.ridge).with_y(0.0).length();
            assert!(
                lateral + d.pillar_shape.radius < half,
                "{o:?} is off the face"
            );
            assert!(
                half - (lateral + d.pillar_shape.radius) >= 1.2,
                "{o:?} leaves no room beside it"
            );
            for p in &sc.obstacles {
                if p != o && p.stage == o.stage && p.ramp == o.ramp {
                    let along = (p.ridge - o.ridge).with_y(0.0).length();
                    assert!(
                        along > 18.0 || p.side != o.side,
                        "pillars crowd a face: {o:?} {p:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_set_dressing_stays_clear_of_the_ramps() {
        let (t, d) = course_of();
        let smp = samples(&t, &d);
        for &(x, z, r, _) in &d.stacks {
            assert!(ridge_distance(&smp, x, z) > r + 30.0, "stack at {x},{z}");
        }
        for &(x, z, sc) in &d.towers {
            assert!(
                ridge_distance(&smp, x, z) > 24.0 * sc + 20.0,
                "tower at {x},{z}"
            );
        }
        for &(x, half, _) in &d.gantries {
            for side in [-1.0, 1.0] {
                let z = -1935.0 + side * half;
                assert!(ridge_distance(&smp, x, z) > 16.0, "gantry leg at {x},{z}");
            }
        }
        for &(x, z, len) in &d.belts {
            for k in 0..5 {
                let px = x - len / 2.0 + len * k as f32 / 4.0;
                assert!(ridge_distance(&smp, px, z) > 20.0, "belt at {px},{z}");
            }
        }
        for &(x, z, sc, _, _) in &d.gears {
            assert!(ridge_distance(&smp, x, z) > 12.0 + sc, "gear at {x},{z}");
        }
        for &(x, z) in &d.pistons {
            assert!(ridge_distance(&smp, x, z) > 14.0, "piston at {x},{z}");
        }
        // The Great Furnace stands between stage 14's lines, clear of both.
        let (gx, gz) = d.great.at;
        // (It's scenery, not solid: clear of the faces with room to spare.)
        assert!(ridge_distance(&smp, gx, gz) > d.great.radius + 16.0);
    }

    #[test]
    fn the_scene_is_inside_the_budget() {
        let (t, d) = course_of();
        let sc = scene(&t, &d);
        assert!(sc.lamps.len() <= 40, "{} lamps", sc.lamps.len());
        assert!(sc.blocks.len() + pad_blocks(&d).len() <= 1500);
        assert!(sc.placed.len() <= 400, "{} models", sc.placed.len());
        assert!(sc.items.len() <= 3000, "{} parts", sc.items.len());
        assert!(sc.items.len() > 300);
    }

    #[test]
    fn the_same_data_gives_the_same_scene() {
        let (t, d) = course_of();
        assert_eq!(scene(&t, &d), scene(&t, &d));
    }

    #[test]
    fn the_models_it_places_exist() {
        let root = format!("{}/../game/assets/models", env!("CARGO_MANIFEST_DIR"));
        for m in [GEAR, PISTON, SPARK] {
            let name = m.trim_start_matches("features/");
            assert!(
                std::path::Path::new(&format!("{root}/features/{name}")).exists(),
                "{m} isn't there"
            );
        }
    }

    #[test]
    fn the_floor_is_under_the_fall_line() {
        let (t, d) = course_of();
        let low = samples(&t, &d).iter().map(|p| p.y).fold(f32::MAX, f32::min);
        assert!(d.floor < low - t.stage_fall && d.sea < low - t.stage_fall);
    }
}
