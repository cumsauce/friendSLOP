//! The overgrown ruins (stages 10, 11 and 12 of the surf course, hall x 3150
//! to 4450; see `docs/design/surf-areas-brief.md`): an abandoned city the
//! jungle took back. A collapsed highway interchange (stage 10), a
//! half-fallen skyscraper you spiral down round in a helix (stage 11) and a
//! drowned mall under a glass-roof skeleton with a banyan bursting out of it
//! (stage 12), on three terraces stepping down the valley, with waterfalls
//! at each step, giant trees, vines, fireflies and moss-green ramps.
//!
//! The course is data (`data/side/surf_overgrown.ron`: every ramp's ridge
//! points, the pads, the speed gaps) and so is the scenery's vocabulary.
//! [`layout`] is a pure function of the course and that file: the geometry
//! (boxes with modeler materials), the colliders, the lamps and the placed
//! models the example `surf_pack_overgrown` writes as modeler ops. Nothing
//! here is registered at run time: the baked map (`maps/surf_overgrown.bsn`)
//! is the truth once exported.
//!
//! The ramps:
//!   stage 10 "Interchange": a fast highway run, then a **fork** at the end
//!   of it: the main line hops a broken deck and an overpass (two 12-14 m
//!   gaps, the second after a bulge round a pit) while the median leap (a
//!   22 m speed gap) flies straight over to the same finish;
//!   stage 11 "Skyscraper": a lead-in over the fallen lobby, then one full
//!   helix (radius 85 m, 38 m down) round the leaning tower; you leave it
//!   directly under where you went in (a cross-over), then a long plaza run;
//!   stage 12 "Drowned mall": an atrium sweep, a calm flat cruise across the
//!   flooded concourse with the giant banyan in view, then a **fork**: the
//!   escalator-rubble chain or the skylight leap (a 22 m speed gap).

use bevy::prelude::*;
use serde::Deserialize;

use crate::features::surf_map::Block;
use crate::features::surf_office::hash;
use crate::rules::surf::{Pad, Stage, Tuning, ridge};

/// One pad of the area: the stage it starts, its centre and size.
#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq)]
pub struct PadData {
    pub stage: usize,
    pub at: (f32, f32, f32),
    pub size: (f32, f32, f32),
}

/// One ramp: its stage, a name for the generator, whether it is the calm
/// stretch, and its ridge points (hall coordinates).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct RampData {
    pub stage: usize,
    pub name: String,
    pub calm: bool,
    pub points: Vec<(f32, f32, f32)>,
}

/// A stage's ramp look (`RampLook`).
#[derive(Deserialize, Default, Debug, Clone, Copy)]
pub struct Look {
    pub stage: u32,
    pub colour: (f32, f32, f32),
    pub glow: f32,
    pub rough: f32,
    pub metal: f32,
}

/// A terrace of the valley floor: x and z extent, its top and thickness, and
/// what grows on it: giant trees (count, smallest and biggest scale), bushes,
/// ferns, flowers and rubble.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Terrace {
    pub x: (f32, f32),
    pub z: (f32, f32),
    pub top: f32,
    pub thick: f32,
    pub trees: (usize, f32, f32),
    pub bushes: usize,
    pub ferns: usize,
    pub flowers: usize,
    pub rubble: usize,
}

/// A waterfall off a terrace's edge: the edge's x, the z span, top and
/// bottom of the fall, and which way the water runs (+1 towards +x).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Waterfall {
    pub x: f32,
    pub z: (f32, f32),
    pub top: f32,
    pub bottom: f32,
    pub thick: f32,
    pub streaks: usize,
}

/// A highway deck: centre, size (length along its heading, thickness, width),
/// heading and tilts (degrees), the columns under it (fractions of the
/// length from the middle, -0.5..0.5), whether it is a collider, how many
/// vines hang off it and how many plants grow on it.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Deck {
    pub at: (f32, f32, f32),
    pub size: (f32, f32, f32),
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub columns: Vec<f32>,
    pub solid: bool,
    pub vines: usize,
    pub plants: usize,
}

/// A skyscraper: where it stands (x, z; its foot is the terrace's top), its
/// size, how many stacked segments, the lean per segment (x, z), its
/// heading, and whether the top is broken into stubs.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tower {
    pub at: (f32, f32),
    pub size: (f32, f32),
    pub height: f32,
    pub segments: usize,
    pub lean: (f32, f32),
    pub yaw: f32,
    pub broken: bool,
    pub vines: usize,
}

/// A curved off-ramp loop of deck pieces round a centre.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Loop {
    pub at: (f32, f32),
    pub radius: f32,
    /// From and to angles (degrees, 0 = +x, 90 = +z).
    pub arc: (f32, f32),
    /// Deck height at each end.
    pub y: (f32, f32),
    pub width: f32,
    pub thick: f32,
    pub pieces: usize,
    /// A column every this many pieces.
    pub column_every: usize,
}

/// A sign gantry over the course: two columns either side of a line and a
/// beam, plus a mossy sign panel.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Gantry {
    pub at: (f32, f32),
    pub half_span: f32,
    pub beam_y: f32,
    pub column: f32,
    pub panel: (f32, f32),
}

/// The drowned mall: a hall of columns, roof ribs and flooded floor.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Hall {
    pub x: (f32, f32),
    pub z: (f32, f32),
    pub water_y: f32,
    pub roof_y: f32,
    pub column: f32,
    pub column_every: f32,
    pub rib_every: f32,
    /// Chance (0..1) a roof piece has fallen away.
    pub broken: f32,
    pub balcony_y: f32,
    pub escalators: usize,
}

/// A hand-placed giant tree (model number 1..3, x, z, scale, heading) with
/// a trunk collider.
#[derive(Deserialize, Default, Debug, Clone, Copy)]
pub struct Giant {
    pub model: usize,
    pub at: (f32, f32),
    pub scale: f32,
    pub yaw: f32,
}

/// Scatter and light settings.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Scatter {
    pub seed: u32,
    /// Fireflies per 100 m of ridge, and the calm stretch's multiple.
    pub fireflies: f32,
    pub calm_fireflies: f32,
    pub firefly_size: f32,
    /// Fireflies float this far off the ridge sideways (min, max) and up.
    pub firefly_side: (f32, f32),
    pub firefly_up: (f32, f32),
    /// Plants keep this far (m) from the ridge, and a tree this far plus its
    /// crown (about `crown` times its scale).
    pub plant_clear: f32,
    pub tree_clear: f32,
    pub crown: f32,
    /// A lamp every this far (m) along the course, its lumens, reach and colour.
    pub lamp_every: f32,
    pub lamp_side: f32,
    pub lamp_up: f32,
    pub lumens: f32,
    pub range: f32,
    pub colour: (f32, f32, f32),
    /// Vine length range (m) and thickness.
    pub vine_len: (f32, f32),
    pub vine_width: f32,
    /// Rubble block size range (m).
    pub rubble_size: (f32, f32),
    /// Tower segments get a moss collar this often.
    pub collar_every: usize,
}

/// Where the sideways space round a ridge is kept free of solids, as in the
/// other areas' tunnel rule: sideways (m) and up (m) over the ridge.
#[derive(Deserialize, Default, Debug, Clone, Copy)]
pub struct Clearance {
    pub side: f32,
    pub up: f32,
}

/// Tuning and layout: `assets/data/side/surf_overgrown.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Overgrown {
    pub pads: Vec<PadData>,
    /// The stand-in for the next area's first pad (stage 13), replaced when
    /// the foundry's map is there.
    pub standin: PadData,
    pub ramps: Vec<RampData>,
    pub speed_gaps: Vec<(usize, usize)>,
    pub looks: Vec<Look>,
    pub terraces: Vec<Terrace>,
    pub waterfalls: Vec<Waterfall>,
    pub decks: Vec<Deck>,
    pub towers: Vec<Tower>,
    pub loops: Vec<Loop>,
    pub gantries: Vec<Gantry>,
    pub halls: Vec<Hall>,
    pub giants: Vec<Giant>,
    pub scatter: Scatter,
    pub clearance: Clearance,
    /// Column and pillar widths (m).
    pub column: f32,
    /// How far a speed-gap arrow floats over the middle of the gap (m).
    pub arrow_up: f32,
}

pub static OVERGROWN: friendslop_data::Data<Overgrown> =
    friendslop_data::Data::ron("data/side/surf_overgrown.ron");

/// The stages this area owns (0-based, start pad to the next area's pad).
pub const STAGES: std::ops::Range<usize> = 10..13;
/// The modeler's tree models (the meadow's), scaled up.
pub const TREES: [&str; 3] = [
    "nature/tree_1.gltf",
    "nature/tree_2.gltf",
    "nature/tree_3.gltf",
];
pub const BUSHES: [&str; 2] = ["nature/bush_1.gltf", "nature/bush_2.gltf"];
pub const FERN: &str = "nature/fern.gltf";
pub const FLOWERS: [&str; 3] = [
    "nature/flower_1.gltf",
    "nature/flower_2.gltf",
    "nature/flower_3.gltf",
];
/// The speed-gap arrow model.
pub const ARROW: &str = "features/surf_giant_arrow.gltf";
/// The meadow trees' height (m) and root reach below their origin.
const TREE_HEIGHT: f32 = 6.1;
const TREE_ROOT: f32 = 0.43;

// The modeler materials the pack uses.
pub const CONCRETE: &str = "sf-og-concrete";
pub const MOSS: &str = "sf-og-moss";
pub const FACADE: &str = "sf-fa-office";
pub const GLASS: &str = "sf-fa-glass";
pub const WATER: &str = "de-water";
pub const FIREFLY: [&str; 2] = ["sf-neon-lime", "sf-neon-gold"];
pub const RIM: &str = "sf-neon-lime";
/// Every pbr material the pack's parts use (the map document loads them).
pub const MATERIALS: [&str; 9] = [
    CONCRETE,
    MOSS,
    FACADE,
    GLASS,
    WATER,
    "sf-neon-lime",
    "sf-neon-gold",
    "sf-leaves",
    "sf-metal",
];

/// How a part is coloured: a modeler material, or a flat colour.
#[derive(Debug, Clone, PartialEq)]
pub enum Mat {
    Pbr(&'static str),
    Flat([f32; 3], f32),
}

/// A box of the map's own geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub centre: Vec3,
    pub size: Vec3,
    pub rot: Quat,
    pub mat: Mat,
}

/// A placed model (a path under `models/`).
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub model: String,
    pub at: Vec3,
    pub rot: Quat,
    pub scale: Vec3,
}

/// A point light: where, lumens, reach, colour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    pub at: Vec3,
    pub lumens: f32,
    pub range: f32,
    pub colour: (f32, f32, f32),
}

/// Everything the area's scenery is made of.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Layout {
    pub parts: Vec<Part>,
    pub blocks: Vec<Block>,
    pub models: Vec<Placed>,
    pub lights: Vec<Light>,
    /// A speed-gap arrow: where and its heading (radians about y).
    pub arrows: Vec<(Vec3, f32)>,
}

/// The area's stages as the course wants them (index = stage number; the
/// earlier ones are empty).
pub fn stages(d: &Overgrown) -> Vec<Stage> {
    let n = d.pads.iter().map(|p| p.stage + 1).max().unwrap_or(0);
    let mut out = vec![Stage::default(); n];
    for p in &d.pads {
        out[p.stage].pad = (p.at, p.size);
    }
    for r in &d.ramps {
        if let Some(s) = out.get_mut(r.stage) {
            s.ramps.push(r.points.clone());
        }
    }
    out
}

/// The tuning with this area's stages and speed gaps in (the map's markers
/// say the same once the pack is baked).
pub fn with_course(t: &Tuning, d: &Overgrown) -> Tuning {
    let mut t = t.clone();
    let mine = stages(d);
    if t.stages.len() < mine.len() {
        t.stages.resize_with(mine.len(), Stage::default);
    }
    for i in STAGES {
        if let Some(s) = mine.get(i) {
            t.stages[i] = s.clone();
        }
    }
    t.speed_gaps.retain(|g| !STAGES.contains(&g.0));
    t.speed_gaps.extend(d.speed_gaps.iter().copied());
    t
}

/// Where the valley floor is under `x`.
pub fn floor_y(d: &Overgrown, x: f32) -> f32 {
    d.terraces
        .iter()
        .find(|t| (t.x.0..=t.x.1).contains(&x))
        .or_else(|| d.terraces.last())
        .map_or(0.0, |t| t.top)
}

/// How far down a pad's pillar goes: to the floor under it.
fn pillar(d: &Overgrown, p: &PadData) -> Pad {
    let top = p.at.1 - p.size.1 / 2.0;
    let floor = floor_y(d, p.at.0);
    let h = (top - floor).max(1.0);
    (
        (p.at.0, top - h / 2.0, p.at.2),
        (p.size.0 - 2.0, h, p.size.2 - 2.0),
    )
}

/// The pads' solid boxes: each pad and its pillar down to the floor, and the
/// stand-in's.
pub fn course_blocks(d: &Overgrown) -> Vec<Pad> {
    let mut out = Vec::new();
    for p in d.pads.iter().chain(std::iter::once(&d.standin)) {
        out.push((p.at, p.size));
        out.push(pillar(d, p));
    }
    out
}

/// Every ramp's ridge samples (hall coordinates), with its data.
pub fn lines<'a>(t: &Tuning, d: &'a Overgrown) -> Vec<(&'a RampData, Vec<Vec3>)> {
    d.ramps
        .iter()
        .map(|r| (r, ridge(&r.points, t.ramp_samples * 4)))
        .collect()
}

fn rot(yaw: f32, pitch: f32, roll: f32) -> Quat {
    Quat::from_rotation_y(yaw.to_radians())
        * Quat::from_rotation_z(pitch.to_radians())
        * Quat::from_rotation_x(roll.to_radians())
}

fn v3((x, y, z): (f32, f32, f32)) -> Vec3 {
    Vec3::new(x, y, z)
}

/// How far (m) a point is from a polyline across the ground.
fn distance_to(lines: &[Vec<Vec3>], x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let mut best = f32::MAX;
    for l in lines {
        for w in l.windows(2) {
            let (a, b) = (w[0].xz(), w[1].xz());
            let ab = b - a;
            let t = if ab.length_squared() > 1e-6 {
                ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            best = best.min((a + ab * t - p).length());
        }
    }
    best
}

/// The builder the layout functions share: the data, the seed counter and
/// what is made so far.
struct Build<'a> {
    d: &'a Overgrown,
    out: Layout,
    n: u32,
    /// Circles (x, z, radius) the scatter keeps out of.
    keep: Vec<(f32, f32, f32)>,
    /// The ramps' ridges (what solids keep clear of).
    ridges: Vec<Vec<Vec3>>,
}

impl Build<'_> {
    fn rnd(&mut self) -> f32 {
        self.n += 1;
        hash(self.d.scatter.seed, self.n)
    }

    fn part(&mut self, centre: Vec3, size: Vec3, rot: Quat, mat: Mat) {
        self.out.parts.push(Part {
            centre,
            size,
            rot,
            mat,
        });
    }

    fn pbr(&mut self, centre: Vec3, size: Vec3, rot: Quat, pbr: &'static str) {
        self.part(centre, size, rot, Mat::Pbr(pbr));
    }

    fn block(&mut self, centre: Vec3, size: Vec3, yaw_deg: f32, round: bool) {
        self.out.blocks.push(Block {
            centre,
            size,
            yaw: yaw_deg.to_radians(),
            round,
        });
    }

    /// A vine: a thin dark green strand hanging from `top` down `len`.
    fn vine(&mut self, top: Vec3) {
        let s = &self.d.scatter;
        let len = s.vine_len.0 + (s.vine_len.1 - s.vine_len.0) * self.rnd();
        let w = s.vine_width * (0.7 + 0.6 * self.rnd());
        // Three greens, so the map has three vine materials, not hundreds.
        let g = [0.15, 0.21, 0.27][(self.rnd() * 3.0) as usize % 3];
        self.part(
            top - Vec3::Y * len / 2.0,
            Vec3::new(w, len, w),
            Quat::IDENTITY,
            Mat::Flat([0.09, g, 0.07], 0.9),
        );
    }

    /// A plant (bush, fern or flower) standing at a point.
    fn plant(&mut self, at: Vec3, scale: f32, kind: usize) {
        let model = match kind {
            0 => BUSHES[(self.rnd() * BUSHES.len() as f32) as usize % BUSHES.len()],
            1 => FERN,
            _ => FLOWERS[(self.rnd() * FLOWERS.len() as f32) as usize % FLOWERS.len()],
        };
        let yaw = self.rnd() * std::f32::consts::TAU;
        self.out.models.push(Placed {
            model: model.into(),
            at,
            rot: Quat::from_rotation_y(yaw),
            scale: Vec3::splat(scale),
        });
    }

    /// A giant tree with its roots at `at`.
    fn tree(&mut self, model: usize, at: Vec3, scale: f32, yaw: f32) {
        self.out.models.push(Placed {
            model: TREES[model % TREES.len()].into(),
            at: at - Vec3::Y * TREE_ROOT * scale * 0.5,
            rot: Quat::from_rotation_y(yaw.to_radians()),
            scale: Vec3::splat(scale),
        });
    }
}

/// The pads: slab, moss rim, pillar, lantern and light.
fn pads(b: &mut Build) {
    let d = b.d;
    let all: Vec<PadData> = d
        .pads
        .iter()
        .chain(std::iter::once(&d.standin))
        .copied()
        .collect();
    for p in all {
        let (c, s) = (v3(p.at), v3(p.size));
        b.pbr(c, s, Quat::IDENTITY, CONCRETE);
        let top = c.y + s.y / 2.0;
        for (dx, dz, sx, sz) in [
            (0.0, 1.0, s.x, 0.4),
            (0.0, -1.0, s.x, 0.4),
            (1.0, 0.0, 0.4, s.z),
            (-1.0, 0.0, 0.4, s.z),
        ] {
            b.pbr(
                Vec3::new(
                    c.x + dx * (s.x / 2.0 - 0.2),
                    top + 0.05,
                    c.z + dz * (s.z / 2.0 - 0.2),
                ),
                Vec3::new(sx, 0.2, sz),
                Quat::IDENTITY,
                RIM,
            );
        }
        let (pc, ps) = pillar(d, &p);
        let pc = v3(pc);
        b.pbr(pc, v3(ps), Quat::IDENTITY, CONCRETE);
        // Moss climbing the pillar's foot, and a lantern post with one lamp
        // per stage (the stage's number into the area).
        b.pbr(
            Vec3::new(pc.x, pc.y - ps.1 / 2.0 + 4.0, pc.z),
            Vec3::new(ps.0 + 1.0, 8.0, ps.2 + 1.0),
            Quat::IDENTITY,
            MOSS,
        );
        let corner = Vec3::new(c.x - s.x / 2.0 + 0.6, top, c.z + s.z / 2.0 - 0.6);
        b.part(
            corner + Vec3::Y * 2.0,
            Vec3::new(0.25, 4.0, 0.25),
            Quat::IDENTITY,
            Mat::Flat([0.1, 0.12, 0.1], 0.8),
        );
        let n = (p.stage + 1).saturating_sub(STAGES.start).min(5);
        for k in 0..n {
            b.pbr(
                corner + Vec3::Y * (4.3 + k as f32 * 0.55),
                Vec3::splat(0.4),
                Quat::IDENTITY,
                RIM,
            );
        }
        b.out.lights.push(Light {
            at: c + Vec3::Y * 5.0,
            lumens: d.scatter.lumens,
            range: d.scatter.range,
            colour: d.scatter.colour,
        });
    }
}

/// The valley floor's terraces: a concrete body under a moss cap, with the
/// plants, rubble and giant trees on them.
fn terraces(b: &mut Build, ridges: &[Vec<Vec3>]) {
    let d = b.d;
    for t in &d.terraces {
        let (mx, mz) = ((t.x.0 + t.x.1) / 2.0, (t.z.0 + t.z.1) / 2.0);
        let (sx, sz) = (t.x.1 - t.x.0, t.z.1 - t.z.0);
        let cap = 3.0;
        b.pbr(
            Vec3::new(mx, t.top - cap - t.thick / 2.0, mz),
            Vec3::new(sx, t.thick, sz),
            Quat::IDENTITY,
            CONCRETE,
        );
        b.pbr(
            Vec3::new(mx, t.top - cap / 2.0, mz),
            Vec3::new(sx + 0.8, cap, sz + 0.8),
            Quat::IDENTITY,
            MOSS,
        );
    }
    // Scatter on each terrace, away from the course and the set pieces.
    let sc = &d.scatter;
    for (ti, t) in d.terraces.iter().enumerate() {
        let inset = 12.0;
        let pick = |b: &mut Build, clear: f32| -> Option<Vec3> {
            for _ in 0..40 {
                let x = t.x.0 + inset + (t.x.1 - t.x.0 - 2.0 * inset) * b.rnd();
                let z = t.z.0 + inset + (t.z.1 - t.z.0 - 2.0 * inset) * b.rnd();
                if distance_to(ridges, x, z) < clear {
                    continue;
                }
                if b.keep
                    .iter()
                    .any(|&(kx, kz, kr)| Vec2::new(x - kx, z - kz).length() < kr + clear * 0.3)
                {
                    continue;
                }
                return Some(Vec3::new(x, t.top, z));
            }
            None
        };
        let _ = ti;
        for _ in 0..t.trees.0 {
            let s = t.trees.1 + (t.trees.2 - t.trees.1) * b.rnd();
            let clear = sc.tree_clear + sc.crown * s;
            if let Some(at) = pick(b, clear) {
                let model = (b.rnd() * 3.0) as usize;
                let yaw = b.rnd() * 360.0;
                b.tree(model, at, s, yaw);
            }
        }
        for (count, lo, hi, kind) in [
            (t.bushes, 3.0, 6.0, 0),
            (t.ferns, 2.5, 5.0, 1),
            (t.flowers, 2.0, 4.0, 2),
        ] {
            for _ in 0..count {
                let s = lo + (hi - lo) * b.rnd();
                if let Some(at) = pick(b, sc.plant_clear) {
                    b.plant(at, s, kind);
                }
            }
        }
        for _ in 0..t.rubble {
            let w = sc.rubble_size.0 + (sc.rubble_size.1 - sc.rubble_size.0) * b.rnd();
            if let Some(at) = pick(b, sc.plant_clear + w) {
                let size = Vec3::new(w, w * (0.4 + 0.5 * b.rnd()), w * (0.6 + 0.6 * b.rnd()));
                let q = rot(
                    b.rnd() * 360.0,
                    b.rnd() * 30.0 - 15.0,
                    b.rnd() * 30.0 - 15.0,
                );
                b.pbr(at + Vec3::Y * size.y * 0.3, size, q, CONCRETE);
            }
        }
    }
}

/// A waterfall off a terrace edge: the cliff, the sheet, streaks, foam and
/// a cool lamp.
fn waterfalls(b: &mut Build) {
    let d = b.d;
    for w in &d.waterfalls {
        let (z0, z1) = w.z;
        let (mid, span) = ((z0 + z1) / 2.0, z1 - z0);
        let h = w.top - w.bottom;
        // The sheet leans off the cliff, thin and glassy.
        b.pbr(
            Vec3::new(w.x + w.thick / 2.0 + 0.2, w.bottom + h / 2.0, mid),
            Vec3::new(w.thick, h, span),
            Quat::IDENTITY,
            WATER,
        );
        // White streaks and the foam at its foot.
        for _ in 0..w.streaks {
            let z = z0 + span * b.rnd();
            let ww = 1.0 + 2.5 * b.rnd();
            let yy = w.bottom + (h - 6.0) * (0.2 + 0.8 * b.rnd());
            let sh = 4.0 + 8.0 * b.rnd();
            b.part(
                Vec3::new(w.x + w.thick + 0.5, yy, z),
                Vec3::new(0.5, sh, ww),
                Quat::IDENTITY,
                Mat::Flat([0.85, 0.95, 0.95], 0.35),
            );
        }
        b.part(
            Vec3::new(w.x + w.thick + 5.0, w.bottom + 0.6, mid),
            Vec3::new(14.0, 1.2, span),
            Quat::IDENTITY,
            Mat::Flat([0.8, 0.92, 0.92], 0.3),
        );
        // The cliff face is solid (it is the terrace's edge) but never near a ridge.
        b.out.lights.push(Light {
            at: Vec3::new(w.x + w.thick + 6.0, w.bottom + 10.0, mid),
            lumens: d.scatter.lumens,
            range: d.scatter.range,
            colour: (0.55, 0.9, 1.0),
        });
        b.keep.push((w.x + 10.0, mid, span / 2.0 + 6.0));
    }
}

/// A highway deck with its columns, vines and plants.
fn deck(b: &mut Build, k: &Deck) {
    let d = b.d;
    let c = v3(k.at);
    let q = rot(k.yaw, k.pitch, k.roll);
    let s = v3(k.size);
    b.pbr(c, s, q, CONCRETE);
    // Moss along the edges, and a faded lane stripe on the top.
    for side in [-1.0, 1.0] {
        b.pbr(
            c + q * Vec3::new(0.0, s.y / 2.0 + 0.4, side * (s.z / 2.0 - 0.5)),
            Vec3::new(s.x, 0.8, 1.0),
            q,
            MOSS,
        );
    }
    b.part(
        c + q * Vec3::new(0.0, s.y / 2.0 + 0.06, 0.0),
        Vec3::new(s.x * 0.96, 0.1, 0.3),
        q,
        Mat::Flat([0.62, 0.55, 0.2], 0.85),
    );
    if k.solid {
        b.block(c, s, k.yaw, false);
    }
    for &f in &k.columns {
        let p = c + q * Vec3::new(f * s.x, -s.y / 2.0, 0.0);
        let floor = floor_y(d, p.x);
        let h = p.y - floor;
        if h > 1.0 {
            let col = d.column;
            let at = Vec3::new(p.x, floor + h / 2.0, p.z);
            b.pbr(
                at,
                Vec3::new(col, h, col),
                Quat::from_rotation_y(k.yaw.to_radians()),
                CONCRETE,
            );
            b.pbr(
                Vec3::new(p.x, floor + 8.0, p.z),
                Vec3::new(col + 0.8, 16.0, col + 0.8),
                Quat::from_rotation_y(k.yaw.to_radians()),
                MOSS,
            );
            b.block(at, Vec3::new(col, h, col), k.yaw, false);
        }
    }
    for _ in 0..k.vines {
        let f = b.rnd() - 0.5;
        let side = if b.rnd() < 0.5 { -1.0 } else { 1.0 };
        let top = c + q * Vec3::new(f * s.x, -s.y / 2.0, side * (s.z / 2.0 - 0.3));
        b.vine(top);
    }
    for _ in 0..k.plants {
        let f = b.rnd() - 0.5;
        let z = (b.rnd() - 0.5) * (s.z - 3.0);
        let top = c + q * Vec3::new(f * s.x, s.y / 2.0 + 0.5, z);
        let kind = if b.rnd() < 0.5 { 0 } else { 1 };
        let sc = 1.5 + 2.0 * b.rnd();
        b.plant(top, sc, kind);
    }
    b.keep.push((c.x, c.z, s.x.max(s.z) / 2.0));
}

/// A skyscraper of stacked, sheared segments (every one a collider).
fn tower(b: &mut Build, t: &Tower) {
    let d = b.d;
    let floor = floor_y(d, t.at.0);
    let n = t.segments.max(1);
    let hs = t.height / n as f32;
    let yaw = t.yaw.to_radians();
    let collar = d.scatter.collar_every.max(1);
    for i in 0..n {
        let shear = Vec3::new(t.lean.0, 0.0, t.lean.1) * i as f32;
        let c = Vec3::new(t.at.0, floor + hs * (i as f32 + 0.5), t.at.1) + shear;
        let size = Vec3::new(t.size.0, hs, t.size.1);
        let top = i + 1 == n && t.broken;
        if top {
            // Jagged stubs where the top broke off.
            for _ in 0..5 {
                let fx = b.rnd() - 0.5;
                let fz = b.rnd() - 0.5;
                let h = hs * (0.25 + 0.65 * b.rnd());
                let sz = Vec3::new(t.size.0 * 0.3, h, t.size.1 * 0.3);
                let at =
                    c + Vec3::new(fx * t.size.0 * 0.7, h / 2.0 - hs / 2.0, fz * t.size.1 * 0.7);
                b.pbr(at, sz, Quat::from_rotation_y(yaw), FACADE);
            }
            continue;
        }
        b.pbr(
            c,
            size,
            Quat::from_rotation_y(yaw),
            if i % 2 == 0 { FACADE } else { GLASS },
        );
        b.block(c, size, t.yaw, false);
        if i % collar == 0 {
            b.pbr(
                c - Vec3::Y * hs / 2.0 + Vec3::Y * 1.5,
                Vec3::new(t.size.0 + 1.2, 3.0, t.size.1 + 1.2),
                Quat::from_rotation_y(yaw),
                MOSS,
            );
        }
        if i > 0 {
            for _ in 0..t.vines / n.max(1) {
                let f = b.rnd() - 0.5;
                let side = if b.rnd() < 0.5 { -1.0 } else { 1.0 };
                let top = c + Quat::from_rotation_y(yaw)
                    * Vec3::new(f * t.size.0, -hs / 2.0, side * (t.size.1 / 2.0 + 0.3));
                b.vine(top);
            }
        }
    }
    b.keep.push((
        t.at.0,
        t.at.1,
        t.size.0.max(t.size.1) / 2.0 + t.lean.0.abs() * n as f32 * 0.3,
    ));
}

/// A curved off-ramp loop of deck pieces.
fn curve(b: &mut Build, l: &Loop) {
    let d = b.d;
    let n = l.pieces.max(1);
    let (a0, a1) = (l.arc.0.to_radians(), l.arc.1.to_radians());
    let chord = 2.0 * l.radius * ((a1 - a0) / n as f32 / 2.0).sin().abs() * 1.04;
    for i in 0..n {
        let f = (i as f32 + 0.5) / n as f32;
        let a = a0 + (a1 - a0) * f;
        let y = l.y.0 + (l.y.1 - l.y.0) * f;
        let (sin, cos) = a.sin_cos();
        let at = Vec3::new(l.at.0 + l.radius * cos, y, l.at.1 + l.radius * sin);
        // The box's x runs along the tangent (-sin, cos), turned by `dir`.
        let dir = if a1 > a0 { 1.0 } else { -1.0 };
        let yaw = (-(cos * dir)).atan2(-sin * dir);
        let q = Quat::from_rotation_y(yaw);
        let size = Vec3::new(chord, l.thick, l.width);
        b.pbr(at, size, q, CONCRETE);
        b.pbr(
            at + Vec3::Y * (l.thick / 2.0 + 0.4),
            Vec3::new(chord, 0.8, 1.0),
            q,
            MOSS,
        );
        if l.column_every > 0 && i % l.column_every == 0 {
            let floor = floor_y(d, at.x);
            let h = at.y - l.thick / 2.0 - floor;
            if h > 1.0 {
                let c = Vec3::new(at.x, floor + h / 2.0, at.z);
                b.pbr(c, Vec3::new(d.column, h, d.column), q, CONCRETE);
                b.block(c, Vec3::new(d.column, h, d.column), yaw.to_degrees(), false);
            }
        }
        b.block(at, size, yaw.to_degrees(), false);
    }
    b.keep.push((l.at.0, l.at.1, l.radius + l.width));
}

/// A sign gantry across the course.
fn gantry(b: &mut Build, g: &Gantry) {
    let d = b.d;
    let floor = floor_y(d, g.at.0);
    let h = g.beam_y - floor;
    for side in [-1.0, 1.0] {
        let c = Vec3::new(g.at.0, floor + h / 2.0, g.at.1 + side * g.half_span);
        b.pbr(
            c,
            Vec3::new(g.column, h, g.column),
            Quat::IDENTITY,
            CONCRETE,
        );
        b.block(c, Vec3::new(g.column, h, g.column), 0.0, false);
        b.pbr(
            Vec3::new(c.x, floor + 10.0, c.z),
            Vec3::new(g.column + 0.8, 20.0, g.column + 0.8),
            Quat::IDENTITY,
            MOSS,
        );
    }
    let beam = Vec3::new(g.at.0, g.beam_y, g.at.1);
    b.pbr(
        beam,
        Vec3::new(4.0, 4.0, g.half_span * 2.0 + g.column),
        Quat::IDENTITY,
        CONCRETE,
    );
    b.part(
        beam + Vec3::new(-2.4, -1.0, 0.0),
        Vec3::new(0.6, g.panel.1, g.panel.0),
        Quat::IDENTITY,
        Mat::Flat([0.07, 0.3, 0.16], 0.55),
    );
    b.pbr(
        beam + Vec3::new(-2.8, 2.6, 0.0),
        Vec3::new(1.2, 1.4, g.panel.0 * 0.9),
        Quat::IDENTITY,
        MOSS,
    );
    for _ in 0..8 {
        let z = g.at.1 + (b.rnd() - 0.5) * g.half_span * 2.0;
        b.vine(Vec3::new(g.at.0 - 2.2, g.beam_y - 2.0, z));
    }
    b.keep.push((g.at.0, g.at.1, g.half_span + 6.0));
}

/// The drowned mall: water, colonnades, ribs, fallen escalators.
fn hall(b: &mut Build, h: &Hall) {
    let d = b.d;
    let (mx, mz) = ((h.x.0 + h.x.1) / 2.0, (h.z.0 + h.z.1) / 2.0);
    let (sx, sz) = (h.x.1 - h.x.0, h.z.1 - h.z.0);
    b.pbr(
        Vec3::new(mx, h.water_y - 0.5, mz),
        Vec3::new(sx, 1.0, sz),
        Quat::IDENTITY,
        WATER,
    );
    let floor = floor_y(d, mx);
    let height = h.roof_y - floor;
    let nx = (sx / h.column_every.max(4.0)).floor().max(1.0) as usize;
    for i in 0..=nx {
        let x = h.x.0 + sx * i as f32 / nx as f32;
        for z in [h.z.0, h.z.1] {
            // A column the course swings through is left out (fallen).
            if distance_to(&b.ridges, x, z) < d.clearance.side + h.column {
                continue;
            }
            let c = Vec3::new(x, floor + height / 2.0, z);
            let size = Vec3::new(h.column, height, h.column);
            b.pbr(c, size, Quat::IDENTITY, CONCRETE);
            b.block(c, size, 0.0, false);
            b.pbr(
                Vec3::new(x, floor + 9.0, z),
                Vec3::new(h.column + 0.8, 18.0, h.column + 0.8),
                Quat::IDENTITY,
                MOSS,
            );
            for _ in 0..2 {
                let (jx, jz) = ((b.rnd() - 0.5) * h.column, (b.rnd() - 0.5) * h.column);
                b.vine(Vec3::new(x + jx, h.roof_y - 2.0, z + jz));
            }
        }
    }
    // Roof ribs across, purlins along; some fallen away, glass where it held.
    let ribs = (sx / h.rib_every.max(4.0)).floor().max(1.0) as usize;
    for i in 0..=ribs {
        let x = h.x.0 + sx * i as f32 / ribs as f32;
        if b.rnd() < h.broken * 0.4 {
            continue;
        }
        b.pbr(
            Vec3::new(x, h.roof_y, mz),
            Vec3::new(1.6, 2.0, sz + h.column),
            Quat::IDENTITY,
            CONCRETE,
        );
        if i < ribs {
            let x2 = x + sx / ribs as f32 / 2.0;
            for k in 0..4 {
                let z = h.z.0 + sz * (k as f32 + 0.5) / 4.0;
                if b.rnd() < h.broken {
                    continue;
                }
                b.pbr(
                    Vec3::new(x2, h.roof_y + 0.4, z),
                    Vec3::new(sx / ribs as f32 - 2.0, 0.5, sz / 4.0 - 1.0),
                    Quat::IDENTITY,
                    GLASS,
                );
            }
        }
    }
    for k in 0..3 {
        let z = h.z.0 + sz * (k as f32 + 0.5) / 3.0;
        b.pbr(
            Vec3::new(mx, h.roof_y + 1.2, z),
            Vec3::new(sx + h.column, 1.4, 1.4),
            Quat::IDENTITY,
            CONCRETE,
        );
    }
    // Balconies along both walls and fallen escalators on the floor.
    for z in [h.z.0 + 6.0, h.z.1 - 6.0] {
        let c = Vec3::new(mx, h.balcony_y, z);
        let size = Vec3::new(sx, 1.6, 10.0);
        b.pbr(c, size, Quat::IDENTITY, CONCRETE);
        b.block(c, size, 0.0, false);
    }
    for k in 0..h.escalators {
        let x = h.x.0 + sx * (k as f32 + 0.5) / h.escalators.max(1) as f32;
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let z = mz + side * (sz / 2.0 - 16.0);
        let len = 26.0;
        let q = rot(if k % 2 == 0 { 90.0 } else { -90.0 }, 28.0, 0.0);
        b.pbr(
            Vec3::new(x, h.water_y + len * 0.22, z),
            Vec3::new(len, 1.4, 5.0),
            q,
            CONCRETE,
        );
        b.pbr(
            Vec3::new(x, h.water_y + len * 0.22 + 1.0, z),
            Vec3::new(len, 0.4, 4.0),
            q,
            MOSS,
        );
    }
    b.keep.push((mx, mz, sx.max(sz) / 2.0));
}

/// Fireflies along every ridge, and the lamps beside the course.
fn glow(b: &mut Build, lines: &[(&RampData, Vec<Vec3>)]) {
    let sc = b.d.scatter.clone();
    let mut since_lamp = 0.0;
    let mut side = 1.0f32;
    for (r, line) in lines {
        let per_m = sc.fireflies / 100.0 * if r.calm { sc.calm_fireflies } else { 1.0 };
        let mut acc = 0.0;
        for w in line.windows(2) {
            let seg = w[0].distance(w[1]);
            acc += seg * per_m;
            since_lamp += seg;
            let dir = (w[1] - w[0]).with_y(0.0).normalize_or(Vec3::X);
            let left = Vec3::new(dir.z, 0.0, -dir.x);
            while acc >= 1.0 {
                acc -= 1.0;
                let f = b.rnd();
                let p = w[0].lerp(w[1], f);
                let lat = sc.firefly_side.0 + (sc.firefly_side.1 - sc.firefly_side.0) * b.rnd();
                let sgn = if b.rnd() < 0.5 { -1.0 } else { 1.0 };
                let up = sc.firefly_up.0 + (sc.firefly_up.1 - sc.firefly_up.0) * b.rnd();
                let m = FIREFLY[(b.rnd() * 2.0) as usize % 2];
                b.pbr(
                    p + left * lat * sgn + Vec3::Y * up,
                    Vec3::splat(sc.firefly_size),
                    Quat::IDENTITY,
                    m,
                );
            }
            if since_lamp >= sc.lamp_every {
                since_lamp = 0.0;
                side = -side;
                b.out.lights.push(Light {
                    at: w[0] + left * sc.lamp_side * side + Vec3::Y * sc.lamp_up,
                    lumens: sc.lumens,
                    range: sc.range,
                    colour: sc.colour,
                });
            }
        }
    }
}

/// Where everything goes. Pure: the same data gives the same layout.
pub fn layout(t: &Tuning, d: &Overgrown) -> Layout {
    let ls = lines(t, d);
    let ridges: Vec<Vec<Vec3>> = ls.iter().map(|(_, l)| l.clone()).collect();
    let mut b = Build {
        d,
        out: Layout::default(),
        n: 0,
        keep: Vec::new(),
        ridges: ridges.clone(),
    };
    pads(&mut b);
    waterfalls(&mut b);
    for k in &d.decks {
        deck(&mut b, k);
    }
    for k in &d.towers {
        tower(&mut b, k);
    }
    for k in &d.loops {
        curve(&mut b, k);
    }
    for k in &d.gantries {
        gantry(&mut b, k);
    }
    for k in &d.halls {
        hall(&mut b, k);
    }
    for g in &d.giants {
        let floor = floor_y(d, g.at.0);
        let at = Vec3::new(g.at.0, floor, g.at.1);
        b.tree(g.model.saturating_sub(1), at, g.scale, g.yaw);
        let r = 0.5 * g.scale;
        b.block(
            at + Vec3::Y * TREE_HEIGHT * g.scale * 0.2,
            Vec3::new(r, TREE_HEIGHT * g.scale * 0.4, r),
            0.0,
            true,
        );
        b.keep.push((at.x, at.z, d.scatter.crown * g.scale));
    }
    terraces(&mut b, &ridges);
    glow(&mut b, &ls);
    // An arrow over each speed gap (its sign).
    let t = with_course(t, d);
    for &(stage, ramp) in &d.speed_gaps {
        if let Some(g) = crate::rules::surf::gap(&t, stage, ramp) {
            b.out.arrows.push((
                (g.from + g.to) / 2.0 + Vec3::Y * d.arrow_up,
                g.dir.x.atan2(g.dir.y),
            ));
        }
    }
    b.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::surf::{
        Tuning, clears, course_parts, gap, ideal_speed, joints, speed_needed,
    };

    fn load() -> (Tuning, std::sync::Arc<Overgrown>) {
        let full = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            crate::rules::surf::TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&full) else {
            panic!("no {full}")
        };
        assert!(crate::rules::surf::TUNING.set_bytes(&bytes), "surf.ron");
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            OVERGROWN.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(OVERGROWN.set_bytes(&bytes), "{path} doesn't parse");
        let d = OVERGROWN.get();
        // The ron's own tuning (the ramp numbers), not the merged course.
        let t = (*crate::rules::surf::TUNING.get()).clone();
        (with_course(&t, &d), d)
    }

    fn pad_of(d: &Overgrown, stage: usize) -> PadData {
        if stage == d.standin.stage {
            return d.standin;
        }
        *d.pads
            .iter()
            .find(|p| p.stage == stage)
            .unwrap_or(&d.standin)
    }

    /// The least radius of curvature (m) across the ground, along the ridge
    /// samples (the end metres, where the spline starts, aside).
    fn min_radius(line: &[Vec3]) -> f32 {
        let l: Vec<Vec3> = line.iter().copied().step_by(3).collect();
        let n = l.len();
        l.windows(3)
            .enumerate()
            .filter(|(i, _)| *i >= 3 && *i + 6 < n)
            .filter_map(|(_, w)| {
                let (a, b, c) = (w[0].xz(), w[1].xz(), w[2].xz());
                let cross = (b - a).perp_dot(c - a).abs();
                (cross > 1e-3)
                    .then(|| (b - a).length() * (c - b).length() * (c - a).length() / (2.0 * cross))
            })
            .fold(f32::MAX, f32::min)
    }

    #[test]
    fn the_data_file_has_every_field() {
        let (_, d) = load();
        assert_eq!(d.pads.len(), 3);
        assert_eq!(d.standin.stage, 13);
        assert!(d.ramps.len() >= 9 && d.speed_gaps.len() >= 2);
        assert!(d.terraces.len() >= 3 && !d.waterfalls.is_empty());
        assert!(!d.decks.is_empty() && !d.towers.is_empty() && !d.halls.is_empty());
        assert!(!d.loops.is_empty() && !d.gantries.is_empty() && !d.giants.is_empty());
        assert!(d.looks.len() == 3 && d.column > 0.0 && d.arrow_up > 0.0);
        let s = &d.scatter;
        assert!(s.fireflies > 0.0 && s.lamp_every > 0.0 && s.crown > 0.0 && s.lumens > 0.0);
        assert!(d.clearance.side > 0.0 && d.clearance.up > 0.0);
    }

    #[test]
    fn the_handoff_pads_are_where_the_brief_says() {
        let (_, d) = load();
        let p10 = pad_of(&d, 10);
        assert_eq!(p10.at, (3200.0, -905.0, -1935.0));
        assert_eq!(p10.size, (16.0, 1.0, 16.0));
        let p13 = d.standin;
        assert_eq!(p13.at, (4400.0, -1128.84, -1935.0));
        assert_eq!(p13.size, (16.0, 1.0, 16.0));
    }

    #[test]
    fn every_stage_starts_a_hop_off_its_pad() {
        let (t, d) = load();
        for stage in STAGES {
            let p = pad_of(&d, stage);
            let first = t.stages[stage].ramps[0][0];
            // 12.35 m past the pad centre, 5.5 m below it, in line.
            assert!(
                (first.0 - p.at.0 - 12.35).abs() < 0.2
                    && (p.at.1 - first.1 - 5.5).abs() < 0.2
                    && (first.2 - p.at.2).abs() < 0.5,
                "stage {stage} starts at {first:?} for pad {p:?}"
            );
        }
    }

    #[test]
    fn every_stage_lands_on_the_next_pad() {
        let (t, d) = load();
        for stage in STAGES {
            let next = pad_of(&d, stage + 1);
            // Every ramp that ends near the pad (the forks) ends 30 m before
            // it, 8.5 m above it, inside its width and heading in.
            let mut ending = 0;
            for r in &t.stages[stage].ramps {
                let last = *r.last().unwrap_or(&(0.0, 0.0, 0.0));
                if (next.at.0 - last.0).abs() > 60.0 {
                    continue;
                }
                ending += 1;
                assert!(
                    (next.at.0 - last.0 - 30.0).abs() < 0.2
                        && (last.1 - next.at.1 - 8.5).abs() < 0.2
                        && (last.2 - next.at.2).abs() <= next.size.2 / 2.0 - 1.5,
                    "stage {stage} ramp ends at {last:?}, next pad {next:?}"
                );
            }
            assert!(ending >= 1, "no ramp lands on the pad after stage {stage}");
        }
    }

    #[test]
    fn the_course_descends_gently_and_stays_in_its_region() {
        let (t, d) = load();
        for (r, line) in lines(&t, &d) {
            let (mut lo, mut hi) = (f32::MAX, f32::MIN);
            for p in &line {
                lo = lo.min(p.x);
                hi = hi.max(p.x);
                assert!(p.z > -2500.0 && p.z < -1400.0, "{} leaves z at {p}", r.name);
            }
            assert!(lo > 3150.0 && hi < 4450.0, "{} x {lo}..{hi}", r.name);
            // Always down along the way, never steeper than 0.28 or flatter than 0.
            for w in line.windows(2) {
                let run = (w[1] - w[0]).xz().length().max(1e-3);
                let slope = (w[0].y - w[1].y) / run;
                assert!(slope >= -0.001, "{} climbs at {}", r.name, w[0]);
                assert!(slope < 0.3, "{} is {slope} steep at {}", r.name, w[0]);
            }
            let drop = line[0].y - line[line.len() - 1].y;
            assert!(drop > 2.0, "{} drops only {drop}", r.name);
        }
        // Stages step down by pad.
        let ys: Vec<f32> = [10, 11, 12, 13]
            .iter()
            .map(|&s| pad_of(&d, s).at.1)
            .collect();
        assert!(ys.windows(2).all(|w| w[0] > w[1] + 30.0), "{ys:?}");
    }

    #[test]
    fn the_ridges_never_turn_too_sharp() {
        let (t, d) = load();
        for (r, line) in lines(&t, &d) {
            let radius = min_radius(&line);
            // Brief: keep the ridge's radius of curvature at 60 m or more.
            assert!(radius >= 58.0, "{} turns at {radius:.0} m", r.name);
        }
    }

    #[test]
    fn the_forks_both_reach_the_next_ramp() {
        let (t, d) = load();
        let tt = with_course(&t, &d);
        // Every consecutive jump (gap after ramp r to ramp r + 1) that is a
        // listed speed gap, and the chain hops of the main lines, is clearable
        // with the speed a clean run carries there.
        for &(stage, ramp) in &d.speed_gaps {
            let g = gap(&tt, stage, ramp).expect("speed gap has a next ramp");
            let need = speed_needed(&tt, &g).expect("gap can be cleared");
            let have = ideal_speed(&tt, stage, g.from.y).unwrap_or(0.0);
            assert!(
                need < have * 0.9,
                "stage {stage} gap {ramp}: needs {need:.1} m/s of {have:.1}"
            );
            assert!(need > 18.0, "a speed gap that is too easy: {need:.1}");
            assert!(g.lateral < 12.0, "lateral {}", g.lateral);
            assert!(clears(&tt, &g, have * 0.9));
        }
        // Each stage forks after its dive: ramps are [dive, main, main,
        // shortcut, shortcut]. The main line's one hop is easy; the
        // shortcut's leap is the speed check. Both land on the next pad, and
        // the shortcut is the shorter way.
        for stage in STAGES {
            let ramps = &t.stages[stage].ramps;
            assert_eq!(ramps.len(), 5, "stage {stage}");
            let hop = |a: &Vec<(f32, f32, f32)>, b: &Vec<(f32, f32, f32)>| {
                let mut t2 = tt.clone();
                t2.stages = vec![Stage {
                    ramps: vec![a.clone(), b.clone()],
                    ..Default::default()
                }];
                let g = gap(&t2, 0, 0).expect("gap");
                (speed_needed(&t2, &g).unwrap_or(f32::MAX), g.lateral)
            };
            let (v_main, lat_main) = hop(&ramps[1], &ramps[2]);
            let (v_leap, lat_leap) = hop(&ramps[3], &ramps[4]);
            for into in [1, 3] {
                let (v, lat) = hop(&ramps[0], &ramps[into]);
                assert!(v < v_leap && lat < 12.0, "stage {stage}: the split's hop");
            }
            assert!(
                v_leap > v_main + 3.0,
                "stage {stage}: the leap isn't harder"
            );
            assert!(lat_main < 12.0 && lat_leap < 12.0);
            let next = pad_of(&d, stage + 1);
            for r in [2, 4] {
                let end = *ramps[r].last().unwrap_or(&(0.0, 0.0, 0.0));
                assert!(
                    (next.at.0 - end.0 - 30.0).abs() < 0.2,
                    "stage {stage} ramp {r} ends at {end:?}"
                );
            }
            let len = |r: &Vec<(f32, f32, f32)>| {
                r.windows(2)
                    .map(|w| Vec3::from(w[0]).distance(Vec3::from(w[1])))
                    .sum::<f32>()
            };
            assert!(
                len(&ramps[3]) + len(&ramps[4]) < len(&ramps[1]) + len(&ramps[2]),
                "stage {stage}: the shortcut is longer than the main line"
            );
        }
    }

    #[test]
    fn nothing_solid_blocks_a_ramp() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let course: Vec<Pad> = course_blocks(&d);
        for (r, line) in lines(&t, &d) {
            for p in &line {
                for b in &l.blocks {
                    // The pads' own boxes aren't in `blocks`; everything here is scenery.
                    let (s, c) = (-b.yaw).sin_cos();
                    let dd = Vec2::new(p.x - b.centre.x, p.z - b.centre.z);
                    let loc = Vec2::new(dd.x * c - dd.y * s, dd.x * s + dd.y * c);
                    let q = Vec2::new(loc.x.abs() - b.size.x / 2.0, loc.y.abs() - b.size.z / 2.0);
                    let flat = q.max(Vec2::ZERO).length();
                    let (lo, hi) = (b.centre.y - b.size.y / 2.0, b.centre.y + b.size.y / 2.0);
                    let in_way = hi > p.y - t.ramp_height && lo < p.y + d.clearance.up;
                    assert!(
                        !(flat < d.clearance.side && in_way),
                        "{} at {p}: a solid at {} ({} x {} x {}) is in the way",
                        r.name,
                        b.centre,
                        b.size.x,
                        b.size.y,
                        b.size.z
                    );
                }
            }
        }
        // The pads' boxes are clear of the other stages' ramps.
        for (c, s) in course {
            let (c, s) = (Vec3::from(c), Vec3::from(s));
            for (r, line) in lines(&t, &d) {
                for p in &line {
                    let q = Vec2::new((p.x - c.x).abs() - s.x / 2.0, (p.z - c.z).abs() - s.z / 2.0);
                    let flat = q.max(Vec2::ZERO).length();
                    let in_way =
                        c.y + s.y / 2.0 > p.y - t.ramp_height && c.y - s.y / 2.0 < p.y + 3.0;
                    // A ramp starts 12 m off its own pad; only that one may be this close.
                    assert!(
                        !(flat < 3.0 && in_way && p.distance(c) > 40.0),
                        "{} passes a pad box at {p}",
                        r.name
                    );
                }
            }
        }
    }

    #[test]
    fn ramps_do_not_overlap_each_other_but_at_the_ends() {
        let (t, d) = load();
        let all = lines(&t, &d);
        for (i, (a, la)) in all.iter().enumerate() {
            for (b, lb) in all.iter().skip(i + 1) {
                for p in la.iter().step_by(2) {
                    for q in lb.iter().step_by(2) {
                        let flat = (p.xz() - q.xz()).length();
                        let vert = (p.y - q.y).abs();
                        // Two ridges closer than 11 m across need 18 m between them up,
                        // except where forks meet at the pad (12 m apart there).
                        if flat < 11.0 && vert < 18.0 {
                            let near_pad = [10usize, 11, 12, 13]
                                .iter()
                                .any(|&s| (p.x - pad_of(&d, s).at.0).abs() < 36.0);
                            assert!(
                                near_pad || a.stage != b.stage && (p.x - q.x).abs() > 1e9,
                                "{} and {} overlap at {p} / {q}",
                                a.name,
                                b.name
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_layout_stays_in_its_budget() {
        let (t, d) = load();
        let l = layout(&t, &d);
        assert!(l.blocks.len() <= 1500, "{} colliders", l.blocks.len());
        assert!(l.lights.len() <= 40, "{} lights", l.lights.len());
        assert!(l.parts.len() <= 3000, "{} parts", l.parts.len());
        assert!(
            l.models.len() + l.parts.len() / 4 + l.blocks.len() + l.lights.len() < 3000,
            "{} models",
            l.models.len()
        );
        assert!(l.models.len() > 150 && l.arrows.len() == d.speed_gaps.len());
        // Everything stands in the area's region.
        for b in &l.blocks {
            assert!(b.centre.x > 3150.0 && b.centre.x < 4450.0, "{}", b.centre);
        }
    }

    #[test]
    fn the_floor_is_below_every_fall_line() {
        let (t, d) = load();
        for stage in STAGES {
            let low = crate::rules::surf::stage_low(&t, stage).expect("low");
            for (r, line) in lines(&t, &d).iter().filter(|(r, _)| r.stage == stage) {
                for p in line {
                    let floor = floor_y(&d, p.x);
                    assert!(
                        floor < low - t.stage_fall - 2.0 || floor < p.y - 25.0,
                        "{} at {p}: floor {floor} vs fall line {}",
                        r.name,
                        low - t.stage_fall
                    );
                }
            }
        }
    }

    #[test]
    fn the_models_and_materials_exist() {
        let root = format!("{}/../game/assets/models", env!("CARGO_MANIFEST_DIR"));
        let prefabs = format!("{}/../game/.modeler/prefabs", env!("CARGO_MANIFEST_DIR"));
        assert!(std::path::Path::new(&format!("{root}/{ARROW}")).exists());
        for m in TREES
            .iter()
            .chain(BUSHES.iter())
            .chain(FLOWERS.iter())
            .chain([FERN].iter())
        {
            assert!(std::path::Path::new(&format!("{root}/{m}")).exists(), "{m}");
            // Scaled models are prefab instances: their prefab is there too.
            let stem = m.trim_start_matches("nature/").trim_end_matches(".gltf");
            let prefab = match stem {
                "fern" => "fern".to_string(),
                s => s.replace('_', "-"),
            };
            let named = match prefab.as_str() {
                "tree-1" | "tree-2" | "tree-3" => format!("tr-{prefab}"),
                _ => prefab,
            };
            assert!(
                std::path::Path::new(&format!("{prefabs}/{named}.json")).exists(),
                "prefab {named}"
            );
        }
        let mats = format!("{}/../game/.modeler/materials", env!("CARGO_MANIFEST_DIR"));
        for m in MATERIALS {
            assert!(
                std::path::Path::new(&format!("{mats}/{m}.json")).exists(),
                "material {m}"
            );
        }
    }

    #[test]
    fn the_same_data_gives_the_same_layout() {
        let (t, d) = load();
        assert_eq!(layout(&t, &d), layout(&t, &d));
    }

    /// If the pack is baked, its map says what the data says.
    #[test]
    fn the_baked_map_matches_the_data() {
        let (_, d) = load();
        let path = format!(
            "{}/../game/assets/maps/surf_overgrown.map.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let map: friendslop_world::places::MapFile =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        let parts = course_parts(&map);
        let want = stages(&d);
        for stage in STAGES {
            let got = parts.stages.get(&(stage as u32)).expect("stage in the map");
            assert_eq!(got.ramps.len(), want[stage].ramps.len(), "stage {stage}");
            assert!((got.pad.0.0 - want[stage].pad.0.0).abs() < 0.01);
            for (g, w) in got.ramps.iter().zip(&want[stage].ramps) {
                assert_eq!(g.len(), w.len());
                for (a, b) in g.iter().zip(w) {
                    assert!(
                        (a.0 - b.0).abs() < 0.01
                            && (a.1 - b.1).abs() < 0.01
                            && (a.2 - b.2).abs() < 0.01,
                        "{a:?} vs {b:?}"
                    );
                }
            }
        }
        assert!(parts.gaps.len() == d.speed_gaps.len());
        let _ = joints;
    }
}
