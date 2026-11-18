//! The office tower of the surf course (#306, doomy: "inside an office
//! building: a huge lobby with pretty trees and glass you can look out and
//! some executive offices", then: "just a snake back and forth ... boring").
//!
//! Stage 5 starts on the base course's last pad and runs through a glass
//! atrium as three lines, not one snake (`data/side/surf_office.ron`):
//!
//! - the MAIN LINE (ramp 0) sweeps south round an elevator core, cruises
//!   gently over the indoor garden, crosses OVER the floor cut, then
//!   plunges round the north bend to the exit;
//! - the FLOOR CUT (ramps 1 and 2): a 22 m leap off the main line onto a
//!   straight, faster ramp that runs under the main line through the
//!   boardroom gates and clears a speed gap;
//! - the SKYBRIDGE chord (ramp 3): a short leap off the north bend onto the
//!   floor cut's last stretch.
//!
//! All three end on the same ridge point, 30 m before the mall's first pad.
//!
//! [`scene`] is a pure function of the course (`rules::surf`) and the data
//! file: the shell's solid walls, floor and roof, the office tiers, the
//! elevator cores (round colliders), the boardroom gates, the skybridges,
//! the garden and the lights. The shell's floor lies below the stage's fall
//! line, so you can never land on it (a fall is a wipe-out). The glass and
//! slab are modeler prefabs `surf-office-{glass,slab}`, a tier is
//! `surf-office-column` (stacked `surf-office-pod`s) and the garden is the
//! meadow's trees; the colliders come from here, never the files.
//! [`in_tunnel`] proves nothing solid enters a ramp's way.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;
use serde::Deserialize;

use crate::rules::surf::{Pad, Stage, Tuning, pad, ridge};

/// An elevator core: a glass shaft from the floor to the roof.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Core {
    pub x: f32,
    pub z: f32,
    pub radius: f32,
    /// The neon material of its rings.
    pub neon: String,
}

/// Tuning: `assets/data/side/surf_office.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Office {
    pub stage: usize,
    pub pad: Pad,
    pub next_pad: Pad,
    /// The ramps' ridge points, hall coordinates.
    pub ramps: Vec<Vec<(f32, f32, f32)>>,
    /// Ramps whose end is a speed gap (to the next ramp).
    pub speed_gaps: Vec<usize>,
    pub min: (f32, f32, f32),
    pub max: (f32, f32, f32),
    pub wall: f32,
    pub glass_max: (f32, f32),
    /// West and east wall openings: half width, below and above the ridge (m).
    pub opening: (f32, f32, f32),
    pub garden_x: (f32, f32),
    /// Trees stay this far (plan) from every ridge.
    pub garden_keep: f32,
    pub tree_scale: (f32, f32),
    pub tree_below: f32,
    pub trees: usize,
    pub bushes: usize,
    pub flowers: usize,
    pub seed: u32,
    pub tree_models: Vec<String>,
    pub bush_models: Vec<String>,
    pub flower_models: Vec<String>,
    /// The office tiers lining the long walls: from x, to x, and the storey
    /// height (m; the pod model's height).
    pub offices: (f32, f32),
    pub storey: f32,
    pub cores: Vec<Core>,
    pub ring_every: f32,
    /// The boardroom gates: the ramp they span, their x, how far from the
    /// ridge their posts stand, the beam's underside over the ridge, and
    /// the post and beam sizes.
    pub gates_ramp: usize,
    pub gates_x: Vec<f32>,
    pub gate_side: f32,
    pub gate_lift: f32,
    pub gate_post: f32,
    pub gate_beam: f32,
    /// Skybridges: x of each, their underside under the roof, size (w, h).
    pub bridges_x: Vec<f32>,
    pub bridge_drop: f32,
    pub bridge_size: (f32, f32),
    /// The pool: from x, to x, half width (m), centred on the floor cut.
    pub pool: (f32, f32, f32),
    pub light_every: f32,
    pub light_up: f32,
    pub light_colour: (f32, f32, f32),
    pub light_intensity: f32,
    pub light_range: f32,
    /// The stage's ramp look: r, g, b, glow, roughness, metal.
    pub ramp_look: (f32, f32, f32, f32, f32, f32),
    pub clear_side: f32,
    pub clear_up: f32,
}

pub static OFFICE: friendslop_data::Data<Office> =
    friendslop_data::Data::ron("data/side/surf_office.ron");

const GLASS: &str = "features/surf_office_glass.gltf";
const SLAB: &str = "features/surf_office_slab.gltf";
/// A whole tier column: 20 office pods stacked, one merged model.
const COLUMN: &str = "features/surf_office_column.gltf";
/// How many storeys the column model has.
const COLUMN_STOREYS: usize = 20;
/// The pod model's size (m): its collider is this, scaled.
const POD_SIZE: Vec3 = Vec3::new(20.0, 6.0, 14.0);
/// The meadow tree models' height (m) and how far their roots reach below 0.
const TREE_HEIGHT: f32 = 6.1;
const TREE_ROOT: f32 = 0.43;
/// The pad pillar (as the base course's): full size.
const PILLAR_HEIGHT: f32 = 141.0;

/// A placed model (hall coordinates; the path is under `models/`).
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub model: String,
    pub at: Vec3,
    pub rot: Quat,
    pub scale: Vec3,
}

/// A solid box (hall coordinates), turned `yaw` about y.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solid {
    pub centre: Vec3,
    pub half: Vec3,
    pub yaw: f32,
}

impl Solid {
    pub fn y_range(&self) -> (f32, f32) {
        (self.centre.y - self.half.y, self.centre.y + self.half.y)
    }
    /// How far (m) a point is from this box across the ground.
    pub fn distance_xz(&self, p: Vec3) -> f32 {
        let d = Vec2::new(p.x - self.centre.x, p.z - self.centre.z);
        let (s, c) = self.yaw.sin_cos();
        let l = Vec2::new(d.x * c - d.y * s, d.x * s + d.y * c);
        let q = Vec2::new(l.x.abs() - self.half.x, l.y.abs() - self.half.z);
        q.max(Vec2::ZERO).length()
    }
}

/// Everything in the office tower.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Layout {
    pub pieces: Vec<Piece>,
    pub solids: Vec<Solid>,
    pub lights: Vec<Vec3>,
    /// Catwalks (the surf mall's): solids that may cross the ramp's tunnel.
    pub catwalks: Vec<Solid>,
    /// The whole building (centre, half size), claimed from the city and props.
    pub claim: (Vec3, Vec3),
}

/// A box of the map's own geometry with a modeler material.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub centre: Vec3,
    pub yaw: f32,
    pub size: Vec3,
    pub pbr: String,
}

/// The office as the generator writes it: the shared layout plus the box
/// geometry and the cores' round colliders.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Scene {
    pub layout: Layout,
    pub parts: Vec<Part>,
    /// Upright cylinders (centre, half = radius, half height, radius).
    pub rounds: Vec<Solid>,
}

pub(crate) fn v3((x, y, z): (f32, f32, f32)) -> Vec3 {
    Vec3::new(x, y, z)
}

/// A deterministic random number in 0..1.
pub(crate) fn hash(seed: u32, i: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ i.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// The ridge point nearest an x (hall coordinates), and the ridge's across
/// direction there (left of its heading, flat).
pub fn at_x(line: &[Vec3], x: f32) -> Option<(Vec3, Vec3)> {
    let i =
        (0..line.len()).min_by(|&a, &b| (line[a].x - x).abs().total_cmp(&(line[b].x - x).abs()))?;
    let a = line[i.saturating_sub(1)];
    let b = line[(i + 1).min(line.len() - 1)];
    let f = (b - a).with_y(0.0).normalize_or(Vec3::X);
    Some((line[i], Vec3::new(f.z, 0.0, -f.x)))
}

/// Split `lo..hi` into equal parts no longer than `max`: (middle, length).
pub(crate) fn tiles(lo: f32, hi: f32, max: f32) -> Vec<(f32, f32)> {
    let n = ((hi - lo) / max.max(1.0)).ceil().max(1.0);
    let len = (hi - lo) / n;
    (0..n as usize)
        .map(|i| (lo + len * (i as f32 + 0.5), len))
        .collect()
}

/// A glass wall over a rectangle: tiled panes and one solid. `along_x`
/// walls stand in a z plane, the others in an x plane.
pub(crate) fn wall(
    out: &mut Layout,
    thick: f32,
    glass_max: (f32, f32),
    along_x: bool,
    plane: f32,
    span: (f32, f32),
    y: (f32, f32),
) {
    if span.1 - span.0 < 0.5 || y.1 - y.0 < 0.5 {
        return;
    }
    let rot = if along_x {
        Quat::IDENTITY
    } else {
        Quat::from_rotation_y(FRAC_PI_2)
    };
    for (a, len) in tiles(span.0, span.1, glass_max.0) {
        for (b, h) in tiles(y.0, y.1, glass_max.1) {
            let at = if along_x {
                Vec3::new(a, b, plane)
            } else {
                Vec3::new(plane, b, a)
            };
            out.pieces.push(Piece {
                model: GLASS.into(),
                at,
                rot,
                scale: Vec3::new(len / 10.0, h / 10.0, 1.0),
            });
        }
    }
    let mid = (span.0 + span.1) / 2.0;
    let (len, h) = (span.1 - span.0, y.1 - y.0);
    let (centre, half) = if along_x {
        (
            Vec3::new(mid, (y.0 + y.1) / 2.0, plane),
            Vec3::new(len / 2.0, h / 2.0, thick / 2.0),
        )
    } else {
        (
            Vec3::new(plane, (y.0 + y.1) / 2.0, mid),
            Vec3::new(thick / 2.0, h / 2.0, len / 2.0),
        )
    };
    out.solids.push(Solid {
        centre,
        half,
        yaw: 0.0,
    });
}

/// The stage this office is: its pad and ramps.
pub fn stage(d: &Office) -> Stage {
    Stage {
        pad: d.pad,
        ramps: d.ramps.clone(),
    }
}

/// The tuning with this office's stage and speed gaps put in (the maps'
/// markers say the same once the pack is baked).
pub fn with_course(t: &Tuning, d: &Office) -> Tuning {
    let mut t = t.clone();
    if t.stages.len() <= d.stage {
        t.stages.resize_with(d.stage + 1, Stage::default);
    }
    t.stages[d.stage] = stage(d);
    t.speed_gaps.retain(|g| g.0 != d.stage);
    t.speed_gaps
        .extend(d.speed_gaps.iter().map(|&r| (d.stage, r)));
    t
}

/// The pad's solid boxes: the pad and its pillar down (as the base course's).
pub fn course_blocks(d: &Office) -> Vec<Pad> {
    let (c, s) = d.pad;
    vec![
        (c, s),
        (
            (c.0, c.1 - s.1 / 2.0 - PILLAR_HEIGHT / 2.0, c.2),
            (s.0 - 2.0, PILLAR_HEIGHT, s.2 - 2.0),
        ),
    ]
}

/// Every ramp's ridge samples, hall coordinates.
pub fn lines(t: &Tuning, d: &Office) -> Vec<Vec<Vec3>> {
    d.ramps
        .iter()
        .map(|r| ridge(r, t.ramp_samples * 4))
        .collect()
}

fn part(out: &mut Vec<Part>, centre: Vec3, yaw: f32, size: Vec3, pbr: &str) {
    out.push(Part {
        centre,
        yaw,
        size,
        pbr: pbr.into(),
    });
}

/// A regular octagon of boxes round a centre: its apothem, each side's
/// height and thickness.
fn octagon(out: &mut Vec<Part>, centre: Vec3, apothem: f32, height: f32, thick: f32, pbr: &str) {
    let side = 2.0 * apothem * (PI / 8.0).tan() + thick;
    for k in 0..8 {
        let a = k as f32 * PI / 4.0;
        let at = centre + Vec3::new(a.cos(), 0.0, a.sin()) * apothem;
        part(out, at, FRAC_PI_2 - a, Vec3::new(side, height, thick), pbr);
    }
}

/// The z the floor cut (and the pool beside it) runs along.
fn cut_z(d: &Office) -> f32 {
    d.ramps
        .get(1)
        .and_then(|r| r.first())
        .map_or(-1939.0, |p| p.2)
}

/// Is a point inside the elevator cores' (plus `margin`) or the pool's way?
fn in_keep_out(d: &Office, x: f32, z: f32, margin: f32) -> bool {
    d.cores
        .iter()
        .any(|c| Vec2::new(x - c.x, z - c.z).length() < c.radius + margin)
        || ((d.pool.0 - margin..=d.pool.1 + margin).contains(&x)
            && (z - cut_z(d)).abs() < d.pool.2 + margin)
}

/// Where everything goes.
pub fn scene(t: &Tuning, d: &Office) -> Scene {
    let mut s = Scene::default();
    let lines = lines(t, d);
    let Some(main) = lines.first().filter(|l| l.len() >= 2) else {
        return s;
    };
    let (lo, hi) = (v3(d.min), v3(d.max));
    let mid = (lo + hi) / 2.0;
    let size = hi - lo;
    let out = &mut s.layout;
    out.claim = (mid, size / 2.0 + Vec3::splat(8.0));
    // The floor (below the fall line: you can't land on it) and the roof.
    out.pieces.push(Piece {
        model: SLAB.into(),
        at: Vec3::new(mid.x, lo.y - d.wall / 2.0, mid.z),
        rot: Quat::IDENTITY,
        scale: Vec3::new(size.x / 10.0, d.wall, size.z / 10.0),
    });
    out.solids.push(Solid {
        centre: Vec3::new(mid.x, lo.y - d.wall / 2.0, mid.z),
        half: Vec3::new(size.x / 2.0, d.wall / 2.0, size.z / 2.0),
        yaw: 0.0,
    });
    let roof_y = hi.y - d.wall / 2.0;
    for (a, la) in tiles(lo.x, hi.x, d.glass_max.0) {
        for (b, lb) in tiles(lo.z, hi.z, d.glass_max.0) {
            out.pieces.push(Piece {
                model: GLASS.into(),
                at: Vec3::new(a, roof_y, b),
                rot: Quat::from_rotation_x(FRAC_PI_2),
                scale: Vec3::new(la / 10.0, lb / 10.0, 1.0),
            });
        }
    }
    out.solids.push(Solid {
        centre: Vec3::new(mid.x, roof_y, mid.z),
        half: Vec3::new(size.x / 2.0, d.wall / 2.0, size.z / 2.0),
        yaw: 0.0,
    });
    // North and south walls; the west and east walls round their openings.
    for plane in [hi.z - d.wall / 2.0, lo.z + d.wall / 2.0] {
        wall(
            out,
            d.wall,
            d.glass_max,
            true,
            plane,
            (lo.x, hi.x),
            (lo.y, hi.y),
        );
    }
    let mut portals = Vec::new();
    for (plane, x) in [(lo.x + d.wall / 2.0, lo.x), (hi.x - d.wall / 2.0, hi.x)] {
        let Some((p, _)) = at_x(main, x) else {
            continue;
        };
        let (z0, z1) = (p.z - d.opening.0, p.z + d.opening.0);
        let (y0, y1) = (p.y - d.opening.1, p.y + d.opening.2);
        let g = d.glass_max;
        wall(out, d.wall, g, false, plane, (lo.z, z0), (lo.y, hi.y));
        wall(out, d.wall, g, false, plane, (z1, hi.z), (lo.y, hi.y));
        wall(out, d.wall, g, false, plane, (z0, z1), (lo.y, y0));
        wall(out, d.wall, g, false, plane, (z0, z1), (y1, hi.y));
        portals.push((plane, p, x == lo.x));
    }
    // Neon frames round the two openings: the reception door and the exit.
    for (plane, p, west) in portals {
        let inward = if west { 1.0 } else { -1.0 };
        let x = plane + inward * 0.9;
        let neon = if west { "sf-neon-cyan" } else { "sf-neon-pink" };
        let (w, below, above) = d.opening;
        let (yc, h) = (p.y + (above - below) / 2.0, above + below);
        for side in [-1.0, 1.0] {
            part(
                &mut s.parts,
                Vec3::new(x, yc, p.z + side * w),
                0.0,
                Vec3::new(1.2, h, 1.2),
                neon,
            );
        }
        for y in [p.y - below, p.y + above] {
            part(
                &mut s.parts,
                Vec3::new(x, y, p.z),
                0.0,
                Vec3::new(1.2, 1.2, 2.0 * w + 1.2),
                neon,
            );
        }
    }
    // The pad the stage starts on (the pad box and its pillar are the
    // course's blocks).
    let (pc, ps) = pad(d.pad);
    part(&mut s.parts, pc, 0.0, ps, "sf-fa-deck");
    part(
        &mut s.parts,
        pc - Vec3::Y * (ps.y / 2.0 + PILLAR_HEIGHT / 2.0),
        0.0,
        Vec3::new(ps.x - 2.0, PILLAR_HEIGHT, ps.z - 2.0),
        "sf-hull",
    );
    // The floor: terrazzo, a stone path under the main line, the pool.
    part(
        &mut s.parts,
        Vec3::new(mid.x, lo.y + 0.1, mid.z),
        0.0,
        Vec3::new(size.x - 2.0, 0.2, size.z - 2.0),
        "pn-terrazzo",
    );
    for w in main.iter().step_by(12).collect::<Vec<_>>().windows(2) {
        let (a, b) = (*w[0], *w[1]);
        let f = (b - a).with_y(0.0);
        if f.length() < 1.0 || !(lo.x..=hi.x).contains(&a.x) {
            continue;
        }
        let at = (a + b) / 2.0;
        part(
            &mut s.parts,
            Vec3::new(at.x, lo.y + 0.25, at.z),
            (-f.z).atan2(f.x),
            Vec3::new(f.length() + 1.0, 0.1, 7.0),
            "sf-stone",
        );
    }
    let pool_z = cut_z(d);
    let (px, pw) = ((d.pool.0 + d.pool.1) / 2.0, d.pool.1 - d.pool.0);
    part(
        &mut s.parts,
        Vec3::new(px, lo.y + 0.3, pool_z),
        0.0,
        Vec3::new(pw, 0.2, 2.0 * d.pool.2),
        "sf-rt-pool",
    );
    for side in [-1.0, 1.0] {
        part(
            &mut s.parts,
            Vec3::new(px, lo.y + 0.7, pool_z + side * (d.pool.2 + 0.8)),
            0.0,
            Vec3::new(pw + 3.0, 1.0, 1.6),
            "sf-stone",
        );
    }
    for x in [d.pool.0 - 0.8, d.pool.1 + 0.8] {
        part(
            &mut s.parts,
            Vec3::new(x, lo.y + 0.7, pool_z),
            0.0,
            Vec3::new(1.6, 1.0, 2.0 * d.pool.2 + 3.2),
            "sf-stone",
        );
    }
    // The elevator cores: glass shafts with steel mullions and neon rings,
    // round colliders from the floor to the roof.
    let height = hi.y - lo.y;
    for c in &d.cores {
        let base = Vec3::new(c.x, lo.y, c.z);
        s.rounds.push(Solid {
            centre: base + Vec3::Y * height / 2.0,
            half: Vec3::new(c.radius, height / 2.0, c.radius),
            yaw: 0.0,
        });
        octagon(
            &mut s.parts,
            base + Vec3::Y * height / 2.0,
            c.radius,
            height,
            0.8,
            "sf-fa-glass",
        );
        let corner = c.radius / (PI / 8.0).cos();
        for k in 0..8 {
            let a = (k as f32 + 0.5) * PI / 4.0;
            part(
                &mut s.parts,
                base + Vec3::new(a.cos(), 0.0, a.sin()) * corner + Vec3::Y * height / 2.0,
                0.0,
                Vec3::new(0.7, height, 0.7),
                "sf-hull",
            );
        }
        let rings = (height / d.ring_every.max(1.0)) as usize;
        for i in 1..=rings {
            octagon(
                &mut s.parts,
                base + Vec3::Y * (i as f32 * d.ring_every),
                c.radius + 0.6,
                1.4,
                1.4,
                &c.neon,
            );
        }
        // A halo of light at the top.
        octagon(
            &mut s.parts,
            base + Vec3::Y * (height - 1.0),
            c.radius + 1.8,
            2.0,
            2.0,
            "sf-lamp-white",
        );
    }
    // A neon ring hung from the roof over the crossing.
    if let Some(c) = d.ramps.get(1).and_then(|r| r.last()) {
        let at = Vec3::new(c.0 + 13.5, hi.y - d.wall - 14.0, c.2);
        octagon(&mut s.parts, at, 28.0, 1.6, 1.6, "sf-neon-gold");
        octagon(&mut s.parts, at, 34.0, 1.0, 1.0, "sf-neon-cyan");
    }
    // The boardroom gates over the floor cut.
    if let Some(line) = lines.get(d.gates_ramp) {
        for &x in &d.gates_x {
            let Some((p, across)) = at_x(line, x) else {
                continue;
            };
            let beam_y = p.y + d.gate_lift + d.gate_beam / 2.0;
            let top = beam_y + d.gate_beam / 2.0;
            let yaw = (-across.z).atan2(across.x);
            let len = 2.0 * d.gate_side + d.gate_post;
            for side in [-1.0, 1.0] {
                let at = p + across * side * d.gate_side;
                let (centre, h) = (Vec3::new(at.x, (lo.y + top) / 2.0, at.z), top - lo.y);
                part(
                    &mut s.parts,
                    centre,
                    0.0,
                    Vec3::new(d.gate_post, h, d.gate_post),
                    "sf-fa-office",
                );
                out.solids.push(Solid {
                    centre,
                    half: Vec3::new(d.gate_post, h, d.gate_post) / 2.0,
                    yaw: 0.0,
                });
            }
            let centre = Vec3::new(p.x, beam_y, p.z);
            part(
                &mut s.parts,
                centre,
                yaw,
                Vec3::new(len, d.gate_beam, d.gate_post),
                "sf-fa-office",
            );
            part(
                &mut s.parts,
                centre - Vec3::Y * (d.gate_beam / 2.0 + 0.2),
                yaw,
                Vec3::new(len - 1.0, 0.4, 1.0),
                "sf-neon-cyan",
            );
            out.solids.push(Solid {
                centre,
                half: Vec3::new(len, d.gate_beam, d.gate_post) / 2.0,
                yaw,
            });
        }
    }
    // Skybridges: glass walkways across the atrium, high under the roof.
    let (bw, bh) = d.bridge_size;
    let (z0, z1) = (lo.z + d.wall, hi.z - d.wall);
    for &x in &d.bridges_x {
        let y = hi.y - d.bridge_drop + bh / 2.0;
        let len = z1 - z0;
        let centre = Vec3::new(x, y, (z0 + z1) / 2.0);
        part(
            &mut s.parts,
            centre,
            0.0,
            Vec3::new(bw, bh, len),
            "sf-fa-glass",
        );
        for side in [-1.0, 1.0] {
            part(
                &mut s.parts,
                centre + Vec3::new(side * bw / 2.0, bh / 2.0 + 0.15, 0.0),
                0.0,
                Vec3::new(0.4, 0.3, len),
                "sf-neon-gold",
            );
        }
        part(
            &mut s.parts,
            centre - Vec3::Y * (bh / 2.0 + 0.2),
            0.0,
            Vec3::new(bw - 1.0, 0.4, len),
            "sf-lamp-white",
        );
        s.layout.solids.push(Solid {
            centre,
            half: Vec3::new(bw, bh, len) / 2.0,
            yaw: 0.0,
        });
    }
    let out = &mut s.layout;
    // The garden: trees (their tops stay under the ramps), bushes, flowers.
    let mut n = 0u32;
    let mut rnd = || {
        n += 1;
        hash(d.seed, n)
    };
    let edge = d.wall + POD_SIZE.z + 4.0;
    let mut plant = |count: usize, models: &[String], scale: (f32, f32), tree: bool| {
        if models.is_empty() {
            return;
        }
        for _ in 0..count {
            let x = d.garden_x.0 + (d.garden_x.1 - d.garden_x.0) * rnd();
            let z = lo.z + 6.0 + (size.z - 12.0) * rnd();
            let model = &models[(rnd() * models.len() as f32) as usize % models.len()];
            let mut sc = scale.0 + (scale.1 - scale.0) * rnd();
            let yaw = rnd() * std::f32::consts::TAU;
            let tiers = (d.offices.0..=d.offices.1).contains(&x);
            if tiers && (z < lo.z + edge || z > hi.z - edge) {
                continue;
            }
            if in_keep_out(d, x, z, 6.0) {
                continue;
            }
            let mut y = lo.y;
            if tree {
                match tree_cap(d, &lines, Vec3::new(x, lo.y, z)) {
                    Some(cap) => sc = sc.min(cap / TREE_HEIGHT),
                    None => continue,
                }
                if sc < 2.5 {
                    continue;
                }
                y += TREE_ROOT * sc;
            }
            out.pieces.push(Piece {
                model: model.clone(),
                at: Vec3::new(x, y, z),
                rot: Quat::from_rotation_y(yaw),
                scale: Vec3::splat(sc),
            });
        }
    };
    plant(d.trees, &d.tree_models, d.tree_scale, true);
    plant(d.bushes, &d.bush_models, (3.0, 5.0), false);
    plant(d.flowers, &d.flower_models, (3.0, 4.5), false);
    // The executive offices: tiers of glass offices built into both long
    // walls of the atrium, facing in, as high as the roof allows.
    let (x0, x1) = d.offices;
    let rows = ((x1 - x0) / POD_SIZE.x).floor().max(0.0) as usize;
    let tower = d.storey * COLUMN_STOREYS as f32;
    let depth = POD_SIZE.z;
    for (z, yaw) in [
        (hi.z - d.wall - depth / 2.0, PI),
        (lo.z + d.wall + depth / 2.0, 0.0),
    ] {
        for i in 0..rows {
            out.pieces.push(Piece {
                model: COLUMN.into(),
                at: Vec3::new(x0 + POD_SIZE.x * (i as f32 + 0.5), lo.y, z),
                rot: Quat::from_rotation_y(yaw),
                scale: Vec3::ONE,
            });
        }
        if rows > 0 {
            let w = POD_SIZE.x * rows as f32;
            out.solids.push(Solid {
                centre: Vec3::new(x0 + w / 2.0, lo.y + tower / 2.0, z),
                half: Vec3::new(w / 2.0, tower / 2.0, depth / 2.0),
                yaw: 0.0,
            });
        }
    }
    // Lights over the ridges.
    for line in &lines {
        let (mut walked, mut next) = (0.0, 0.0);
        for w in line.windows(2) {
            walked += w[0].distance(w[1]);
            if walked >= next {
                next += d.light_every.max(1.0);
                if (lo.x..=hi.x).contains(&w[0].x) {
                    out.lights.push(w[0] + Vec3::Y * d.light_up);
                }
            }
        }
    }
    s
}

/// The tallest a tree may stand at `at` (m): its top stays `tree_below`
/// under every ridge within reach, and none passes within `garden_keep`
/// of it across the ground. `None` when a ridge is too close.
pub fn tree_cap(d: &Office, lines: &[Vec<Vec3>], at: Vec3) -> Option<f32> {
    let mut cap = f32::MAX;
    for p in lines.iter().flatten() {
        let near = Vec2::new(p.x - at.x, p.z - at.z).length();
        if near < d.garden_keep {
            return None;
        }
        if near < 40.0 {
            cap = cap.min(p.y - d.tree_below - at.y);
        }
    }
    Some(cap)
}

/// The shared layout (kept for the mall's tools).
pub fn layout(t: &Tuning, d: &Office) -> Layout {
    scene(t, d).layout
}

/// Is any solid in a ramp's tunnel: within `clear_side` m of a ridge
/// point across the ground and from the ramp's body (`body` m under the
/// ridge) up to `clear_up` over it?
pub fn in_tunnel(line: &[Vec3], solids: &[Solid], d: &Office, body: f32) -> Option<Vec3> {
    line.iter().copied().find(|p| {
        solids.iter().any(|s| {
            let (lo, hi) = s.y_range();
            hi > p.y - body && lo < p.y + d.clear_up && s.distance_xz(*p) < d.clear_side
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::surf::{gap, hall_at, ideal_speed, in_hall, speed_needed, stage_low, tuning};

    fn load() -> (std::sync::Arc<Tuning>, std::sync::Arc<Office>) {
        crate::features::surf_map::load_course_for_tests();
        for path in [crate::rules::surf::TUNING.path(), OFFICE.path()] {
            let full = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), path);
            let Ok(bytes) = std::fs::read(&full) else {
                panic!("no {full}")
            };
            let ok = if path == OFFICE.path() {
                OFFICE.set_bytes(&bytes)
            } else {
                crate::rules::surf::TUNING.set_bytes(&bytes)
            };
            assert!(ok, "{full} doesn't parse");
        }
        let d = OFFICE.get();
        (std::sync::Arc::new(with_course(&tuning(), &d)), d)
    }

    /// The least radius of curvature (m) across the ground.
    fn min_radius(line: &[Vec3]) -> f32 {
        let l: Vec<Vec3> = line.iter().copied().step_by(3).collect();
        l.windows(3)
            .filter_map(|w| {
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
        assert!(d.wall > 0.0 && d.glass_max.0 > 0.0 && d.light_every > 0.0);
        assert!(
            !d.tree_models.is_empty() && !d.bush_models.is_empty() && !d.flower_models.is_empty()
        );
        assert!(d.offices.1 > d.offices.0 && d.trees > 0 && d.clear_side > 0.0);
        assert!(d.ramps.len() == 4 && d.cores.len() >= 3 && d.gates_x.len() >= 3);
        assert!(!d.bridges_x.is_empty() && d.pool.2 > 0.0 && d.ring_every > 0.0);
        assert!(d.ramp_look.0 > 0.0 && d.gate_lift > 0.0 && d.speed_gaps == [1]);
    }

    #[test]
    fn the_handoffs_are_exact() {
        let (t, d) = load();
        let (pc, _) = pad(d.pad);
        let (nc, _) = pad(d.next_pad);
        // P5 and P6, as the brief gives them.
        assert!(pc.distance(Vec3::new(483.65, -563.02, -1938.87)) < 0.01);
        assert!(nc.distance(Vec3::new(1030.0, -645.88, -1938.0)) < 0.01);
        let first = v3(d.ramps[0][0]);
        // A 4 m hop off the pad: 5.5 m under its top, past its edge.
        assert!((first.y - (pc.y - 5.5)).abs() < 0.1, "{first}");
        assert!(first.x > pc.x + 12.0 && first.x < pc.x + 18.0, "{first}");
        // Every line that leaves the lobby ends on the same last point,
        // 30 m before P6's centre and 8.5 m above it.
        let end = nc + Vec3::new(-30.0, 8.5, 0.0);
        for i in [0, 2, 3] {
            let last = v3(d.ramps[i].last().copied().unwrap_or_default());
            assert!(
                last.distance(end) < 0.01,
                "ramp {i} ends at {last}, not {end}"
            );
        }
        assert!(t.stages[d.stage].ramps.len() == 4);
    }

    #[test]
    fn every_ramp_is_smooth_and_runs_downhill() {
        let (t, d) = load();
        for (i, line) in lines(&t, &d).iter().enumerate() {
            assert!(
                min_radius(line) >= 60.0,
                "ramp {i}: radius {}",
                min_radius(line)
            );
            let run = line.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
            let drop = line[0].y - line[line.len() - 1].y;
            assert!(
                drop / run > 0.04 && drop / run < 0.22,
                "ramp {i}: {} down per metre",
                drop / run
            );
            for w in line.windows(8) {
                let (l, y) = (w[0].xz().distance(w[7].xz()), w[0].y - w[7].y);
                assert!(
                    y >= -0.1 && y / l < 0.3,
                    "ramp {i} at {}: {y} over {l}",
                    w[0]
                );
            }
        }
        // The main line is the long way round; the floor cut is shorter.
        let len = |l: &[Vec3]| l.windows(2).map(|w| w[0].distance(w[1])).sum::<f32>();
        let ls = lines(&t, &d);
        let main =
            len(&ls[0]) - len(&ls[0][..ls[0].iter().position(|p| p.x >= 560.0).unwrap_or(0)]);
        let cut = len(&ls[1]) + len(&ls[2]) + 27.0;
        assert!(cut < main * 0.95, "floor cut {cut} vs main {main}");
    }

    #[test]
    fn the_floor_cut_leaps_off_the_main_line_and_crosses_under_it() {
        let (t, d) = load();
        let ls = lines(&t, &d);
        let a = v3(d.ramps[1][0]);
        // It starts beside the main line, well under it, not touching it.
        let near = ls[0]
            .iter()
            .min_by(|p, q| p.xz().distance(a.xz()).total_cmp(&q.xz().distance(a.xz())))
            .copied()
            .unwrap_or_default();
        let lateral = near.xz().distance(a.xz());
        assert!(
            (12.0..22.0).contains(&lateral),
            "{lateral} m beside the main line"
        );
        assert!(near.y - a.y >= 18.0, "only {} m under it", near.y - a.y);
        // The main line passes over it, with room to surf under.
        let over = ls[0]
            .iter()
            .flat_map(|m| ls[1].iter().chain(&ls[2]).map(move |c| (m, c)))
            .filter(|(m, c)| m.xz().distance(c.xz()) < 4.0 && m.x < 900.0 && c.x < 900.0)
            .map(|(m, c)| m.y - c.y)
            .fold(f32::MAX, f32::min);
        assert!(
            (24.0..f32::MAX).contains(&over),
            "crossing clearance {over}"
        );
        // The skybridge chord drops onto the floor cut from the north bend.
        let b = v3(d.ramps[3][0]);
        assert!(
            ls[0]
                .iter()
                .any(|p| p.xz().distance(b.xz()) < 20.0 && p.y - b.y >= 8.0)
        );
        let from_cut = ls[2]
            .iter()
            .map(|p| p.xz().distance(ls[3][ls[3].len() - 1].xz()))
            .fold(f32::MAX, f32::min);
        assert!(from_cut < 1.0);
    }

    #[test]
    fn the_speed_gap_needs_a_clean_line() {
        let (t, d) = load();
        let Some(g) = gap(&t, d.stage, 1) else {
            panic!("no gap after the floor cut's first ramp")
        };
        let Some(need) = speed_needed(&t, &g) else {
            panic!("no speed clears the gap")
        };
        let ideal = ideal_speed(&t, d.stage, g.from.y).unwrap_or(0.0);
        assert!(
            need > ideal * 0.5 && need < ideal * 0.75,
            "needs {need} of {ideal}"
        );
        assert!(
            g.length > 15.0 && g.lateral < t.ramp_half_width * 0.75,
            "{g:?}"
        );
    }

    #[test]
    fn the_building_holds_the_course_and_the_floor_is_under_the_fall_line() {
        let (t, d) = load();
        let (lo, hi) = (v3(d.min), v3(d.max));
        let inside = |p: Vec3| p.cmpge(lo).all() && p.cmple(hi).all();
        for (i, line) in lines(&t, &d).iter().enumerate() {
            // Out of the west wall's opening in, and out of the east one.
            let in_box = line.iter().filter(|p| inside(**p)).count();
            assert!(in_box * 10 > line.len() * 7, "ramp {i} mostly outside");
            assert!(line.iter().all(|p| p.z > lo.z + 30.0 && p.z < hi.z - 30.0));
        }
        let o = hall_at(&t);
        assert!(in_hall(&t, o + lo).is_some() && in_hall(&t, o + hi).is_some());
        let low = stage_low(&t, d.stage).unwrap_or(0.0);
        assert!(
            lo.y < low - t.stage_fall,
            "floor {} vs fall line {}",
            lo.y,
            low - t.stage_fall
        );
    }

    #[test]
    fn nothing_solid_enters_a_ramps_tunnel() {
        let (t, d) = load();
        let s = scene(&t, &d);
        let mut solids = s.layout.solids.clone();
        solids.extend(&s.rounds);
        for (i, line) in lines(&t, &d).iter().enumerate() {
            // The gates' beams and the bridges pass well over a ridge; the
            // rest stay out of the way.
            let hit = in_tunnel(line, &solids, &d, t.ramp_height);
            assert!(hit.is_none(), "a solid blocks ramp {i} at {hit:?}");
        }
        // Every face stays free under a gate: the posts stand outside it.
        assert!(d.gate_side > t.ramp_half_width + d.gate_post);
        assert!(d.gate_lift >= d.clear_up + 3.0);
    }

    #[test]
    fn a_solid_in_the_way_is_found() {
        let (t, d) = load();
        let line = &lines(&t, &d)[0];
        let p = line[line.len() / 2];
        let wall = Solid {
            centre: p + Vec3::Y,
            half: Vec3::splat(1.0),
            yaw: 0.0,
        };
        assert!(in_tunnel(line, &[wall], &d, t.ramp_height).is_some());
    }

    #[test]
    fn the_shell_the_cores_the_gates_and_the_garden_are_all_there() {
        let (t, d) = load();
        let s = scene(&t, &d);
        let l = &s.layout;
        let (lo, hi) = (v3(d.min), v3(d.max));
        let columns = l.pieces.iter().filter(|p| p.model == COLUMN).count();
        let rows = ((d.offices.1 - d.offices.0) / POD_SIZE.x) as usize;
        assert_eq!(columns, 2 * rows);
        assert!(rows >= 10, "only {rows} columns a side");
        let mid_z = (lo.z + hi.z) / 2.0;
        let on = |north: bool| {
            l.pieces
                .iter()
                .filter(|p| p.model == COLUMN && (p.at.z > mid_z) == north)
                .count()
        };
        assert_eq!(on(true), on(false));
        assert!(lo.y + d.storey * COLUMN_STOREYS as f32 <= hi.y - d.wall);
        let trees = l
            .pieces
            .iter()
            .filter(|p| d.tree_models.contains(&p.model))
            .count();
        assert!(trees >= 20, "only {trees} trees");
        assert!(l.pieces.iter().filter(|p| p.model == GLASS).count() > 20);
        assert!(l.lights.len() >= 12);
        // A round collider per core, floor to roof.
        assert_eq!(s.rounds.len(), d.cores.len());
        for r in &s.rounds {
            assert!((r.y_range().1 - hi.y).abs() < 0.01 && (r.y_range().0 - lo.y).abs() < 0.01);
        }
        // A pad and its pillar are the course's blocks.
        assert_eq!(course_blocks(&d).len(), 2);
        // Same data, same building.
        assert_eq!(s, scene(&t, &d));
    }

    #[test]
    fn it_stays_inside_the_budget() {
        let (t, d) = load();
        let s = scene(&t, &d);
        let entities = s.layout.pieces.len() + s.parts.len();
        assert!(entities <= 2400, "{entities} entities");
        assert!(s.layout.solids.len() + s.rounds.len() <= 1000);
        assert!(
            s.layout.lights.len() <= 30,
            "{} lights",
            s.layout.lights.len()
        );
    }

    #[test]
    fn every_tree_stays_under_the_ramps_and_off_them() {
        let (t, d) = load();
        let l = scene(&t, &d).layout;
        let ls = lines(&t, &d);
        for p in l.pieces.iter().filter(|p| d.tree_models.contains(&p.model)) {
            let top = p.at.y - TREE_ROOT * p.scale.y + TREE_HEIGHT * p.scale.y;
            for r in ls.iter().flatten() {
                let near = Vec2::new(r.x - p.at.x, r.z - p.at.z).length();
                assert!(near >= d.garden_keep - 0.01, "tree {} beside a ridge", p.at);
                if near < 40.0 {
                    assert!(top <= r.y - d.tree_below + 0.5, "tree {} too tall", p.at);
                }
            }
        }
    }

    #[test]
    fn the_models_and_materials_exist() {
        let (_, d) = load();
        let assets = format!("{}/../game/assets/", env!("CARGO_MANIFEST_DIR"));
        for m in [COLUMN]
            .into_iter()
            .chain(d.tree_models.iter().map(String::as_str))
            .chain(d.bush_models.iter().map(String::as_str))
            .chain(d.flower_models.iter().map(String::as_str))
        {
            let path = format!("{assets}models/{m}");
            assert!(std::path::Path::new(&path).exists(), "{path} is missing");
        }
        let prefabs = format!("{}/../game/.modeler/prefabs/", env!("CARGO_MANIFEST_DIR"));
        for p in [
            "surf-office-glass",
            "surf-office-slab",
            "surf-office-column",
        ] {
            assert!(
                std::path::Path::new(&format!("{prefabs}{p}.json")).exists(),
                "{p}"
            );
        }
        let mats = format!("{}/../game/.modeler/materials/", env!("CARGO_MANIFEST_DIR"));
        let l = scene(&load().0, &d);
        for m in l.parts.iter().map(|p| p.pbr.as_str()) {
            assert!(
                std::path::Path::new(&format!("{mats}{m}.json")).exists(),
                "material {m}"
            );
        }
    }

    #[test]
    fn the_city_and_props_keep_out() {
        let (t, d) = load();
        let l = scene(&t, &d).layout;
        crate::features::surf_props::claim(l.claim.0, l.claim.1);
        assert!(crate::features::surf_props::claimed(l.claim.0, 0.0));
    }
}
