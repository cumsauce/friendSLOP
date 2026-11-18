//! The mall of the surf course (#306). doomy: the mall was "just a snake back
//! and forth", so it is a stage of set pieces now (`data/side/surf_mall.ron`):
//!
//! - the concourse: one wide, calm bump past the tiers of shops (it teaches),
//! - the skybridge gauntlet: bridges across the hall to go over, under and
//!   duck,
//! - the loop round the clock tower and food court, a figure-8 whose exit leg
//!   runs 30 m under its own start,
//! - the glass-roof cruise, a long easy stretch with sky gardens, and the
//!   straight onto pad P7.
//!
//! Two shortcuts: the ESCALATOR (hop off the bump, a straight steeper lane that
//! fuses back before the gauntlet ends) and the SERVICE LINE (hop off the
//! gauntlet, over the loop's exit leg, a 32 m speed-check gap, then a plunge
//! that fuses into the cruise). Each shortcut ends by fusing with the main
//! line, so both reach the next ramp.
//!
//! [`layout`] is a pure function of the course and the data file; the
//! example `surf_pack_mall` bakes it (and the course markers) into the
//! modeler ops. Never registered at run time: the map is the `.bsn`.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::prelude::*;
use serde::Deserialize;

use crate::rules::surf::{Pad, Stage, Tuning, pad as pad_of, ridge};

/// One ramp of the stage: a name for people, and its ridge points.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct RampData {
    pub name: String,
    pub points: Vec<(f32, f32, f32)>,
}

/// The two pads at the ends (centre, size).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Pads {
    pub start: Pad,
    pub end: Pad,
}

/// The clock tower and fountain.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tower {
    pub at: (f32, f32),
    pub pool: f32,
    pub rim: f32,
    pub plinth: f32,
    pub plinth_height: f32,
    pub shaft: f32,
    pub clock: f32,
    pub clock_y: f32,
    pub spire: f32,
}

/// The kiosks round the pool.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Court {
    pub count: usize,
    pub radius: f32,
    pub size: (f32, f32, f32),
}

/// A kind of bridge: a 10 m segment along +x, and its solid.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Kind {
    pub segment: String,
    pub half_width: f32,
    pub below: f32,
    pub above: f32,
}

/// One bridge across the hall.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct CatWalk {
    pub x: f32,
    pub kind: usize,
    pub lift: f32,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Roof {
    pub every: f32,
    pub rib: (f32, f32),
    pub material: String,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Banners {
    pub every: f32,
    pub size: (f32, f32),
    pub from_wall: f32,
    pub under_roof: f32,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Garden {
    pub at: (f32, f32),
    pub top: f32,
    pub radius: f32,
}

/// Tuning: `assets/data/side/surf_mall.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Mall {
    pub stage: usize,
    pub pads: Pads,
    pub look: (f32, f32, f32, f32, f32, f32),
    pub ramps: Vec<RampData>,
    pub speed_gaps: Vec<usize>,
    pub min: (f32, f32, f32),
    pub max: (f32, f32, f32),
    pub wall: f32,
    pub wall_material: String,
    pub floor_materials: (String, String),
    pub floor_strip: f32,
    pub west_gate: (f32, f32, f32),
    pub east_gate: (f32, f32, f32),
    pub shops: (f32, f32),
    pub tower: Tower,
    pub court: Court,
    pub awnings: Vec<String>,
    pub kinds: Vec<Kind>,
    pub catwalks: Vec<CatWalk>,
    pub gap_needed: f32,
    pub band: (f32, f32),
    pub roof: Roof,
    pub banners: Banners,
    pub gardens: Vec<Garden>,
    pub light_every: f32,
    pub light_up: f32,
    pub light_colour: (f32, f32, f32),
    pub light_intensity: f32,
    pub light_range: f32,
    pub clear_side: f32,
    pub clear_up: f32,
    pub seed: u32,
}

pub static MALL: friendslop_data::Data<Mall> =
    friendslop_data::Data::ron("data/side/surf_mall.ron");

/// The shop column model: 20 m wide, 12 m deep, 96 m tall.
const SHOP: &str = "features/surf_mall_column.gltf";
const SHOP_SIZE: Vec3 = Vec3::new(20.0, 96.0, 12.0);

pub fn v3(v: (f32, f32, f32)) -> Vec3 {
    Vec3::new(v.0, v.1, v.2)
}

/// A box collider, turned about +y.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solid {
    pub centre: Vec3,
    pub half: Vec3,
    pub yaw: f32,
}

impl Solid {
    /// How far (m) a point is from the box's footprint on the ground (0
    /// inside it).
    pub fn distance_xz(&self, p: Vec3) -> f32 {
        let d = Quat::from_rotation_y(-self.yaw)
            * Vec3::new(p.x - self.centre.x, 0.0, p.z - self.centre.z);
        (Vec2::new(d.x, d.z).abs() - Vec2::new(self.half.x, self.half.z))
            .max(Vec2::ZERO)
            .length()
    }
    pub fn bottom(&self) -> f32 {
        self.centre.y - self.half.y
    }
    pub fn top(&self) -> f32 {
        self.centre.y + self.half.y
    }
}

/// A placed model file.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub model: String,
    pub at: Vec3,
    pub rot: Quat,
    pub scale: Vec3,
}

/// A box of geometry in a modeler material.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    pub centre: Vec3,
    pub yaw: f32,
    pub size: Vec3,
    pub material: String,
}

/// An upright cylinder of geometry.
#[derive(Debug, Clone, PartialEq)]
pub struct Cyl {
    pub centre: Vec3,
    pub radius: f32,
    pub height: f32,
    pub material: String,
}

/// A point light.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lamp {
    pub at: Vec3,
    pub lumens: f32,
    pub range: f32,
    pub colour: (f32, f32, f32),
}

/// Everything the mall puts in the map besides the course.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Layout {
    pub models: Vec<Placed>,
    pub parts: Vec<Part>,
    pub cyls: Vec<Cyl>,
    /// Static colliders.
    pub solids: Vec<Solid>,
    /// The bridges' colliders (they may enter the ramps' way, see [`free_gap`]).
    pub catwalks: Vec<Solid>,
    pub lamps: Vec<Lamp>,
}

impl Layout {
    fn part(&mut self, centre: Vec3, size: Vec3, material: &str) {
        self.parts.push(Part {
            centre,
            yaw: 0.0,
            size,
            material: material.into(),
        });
    }
    fn part_yaw(&mut self, centre: Vec3, yaw: f32, size: Vec3, material: &str) {
        self.parts.push(Part {
            centre,
            yaw,
            size,
            material: material.into(),
        });
    }
    fn cyl(&mut self, centre: Vec3, radius: f32, height: f32, material: &str) {
        self.cyls.push(Cyl {
            centre,
            radius,
            height,
            material: material.into(),
        });
    }
    fn solid(&mut self, centre: Vec3, size: Vec3) {
        self.solids.push(Solid {
            centre,
            half: size / 2.0,
            yaw: 0.0,
        });
    }
    /// A wall piece: looks and collider, between x/z/y ranges.
    fn wall(&mut self, x: (f32, f32), z: (f32, f32), y: (f32, f32), material: &str) {
        if x.1 - x.0 < 0.01 || z.1 - z.0 < 0.01 || y.1 - y.0 < 0.01 {
            return;
        }
        let centre = Vec3::new((x.0 + x.1) / 2.0, (y.0 + y.1) / 2.0, (z.0 + z.1) / 2.0);
        let size = Vec3::new(x.1 - x.0, y.1 - y.0, z.1 - z.0);
        self.part(centre, size, material);
        self.solid(centre, size);
    }
}

/// The ramps' ridge lines, smoothed the way the game does.
pub fn lines(t: &Tuning, d: &Mall) -> Vec<Vec<Vec3>> {
    d.ramps
        .iter()
        .map(|r| ridge(&r.points, t.ramp_samples))
        .collect()
}

/// The stage as the course reads it: the pad and the ramps.
pub fn stage(d: &Mall) -> Stage {
    Stage {
        pad: d.pads.start,
        ramps: d.ramps.iter().map(|r| r.points.clone()).collect(),
    }
}

/// The speed-check gaps, (stage, ramp), as the course's `SpeedGap` markers.
pub fn speed_gaps(d: &Mall) -> Vec<(usize, usize)> {
    d.speed_gaps.iter().map(|&r| (d.stage, r)).collect()
}

/// A pad's pillar, down to the floor (a box, like every `course_block`).
fn pillar(d: &Mall, p: Pad) -> Pad {
    let (c, s) = pad_of(p);
    let top = c.y - s.y / 2.0;
    let h = (top - d.min.1).max(1.0);
    ((c.x, top - h / 2.0, c.z), (s.x - 2.0, h, s.z - 2.0))
}

/// The solid boxes of the course: both pads and their pillars (the
/// `course_block` markers).
pub fn blocks(d: &Mall) -> Vec<Pad> {
    vec![
        d.pads.start,
        pillar(d, d.pads.start),
        d.pads.end,
        pillar(d, d.pads.end),
    ]
}

/// Where a ridge line crosses the plane `x` (the first time).
pub fn at_x(line: &[Vec3], x: f32) -> Option<Vec3> {
    line.windows(2).find_map(|w| {
        ((w[0].x - x) * (w[1].x - x) <= 0.0 && w[0].x != w[1].x)
            .then(|| w[0].lerp(w[1], (x - w[0].x) / (w[1].x - w[0].x)))
    })
}

/// Where everything goes.
pub fn layout(t: &Tuning, d: &Mall) -> Layout {
    let mut o = Layout::default();
    let ridges = lines(t, d);
    let Some(main) = ridges.first() else {
        return o;
    };
    let (lo, hi) = (v3(d.min), v3(d.max));
    let (mid, size) = ((lo + hi) / 2.0, hi - lo);
    // The floor: strips of two materials along the hall, one collider.
    let strips = (size.x / d.floor_strip.max(1.0)).ceil() as usize;
    for i in 0..strips {
        let x0 = lo.x + i as f32 * d.floor_strip;
        let x1 = (x0 + d.floor_strip).min(hi.x);
        let m = if i % 2 == 0 {
            &d.floor_materials.0
        } else {
            &d.floor_materials.1
        };
        o.part(
            Vec3::new((x0 + x1) / 2.0, lo.y - d.wall / 2.0, mid.z),
            Vec3::new(x1 - x0, d.wall, size.z),
            m,
        );
    }
    o.solid(
        Vec3::new(mid.x, lo.y - d.wall / 2.0, mid.z),
        Vec3::new(size.x, d.wall, size.z),
    );
    // The long walls, whole.
    let m = d.wall_material.as_str();
    o.wall((lo.x, hi.x), (hi.z - d.wall, hi.z), (lo.y, hi.y), m);
    o.wall((lo.x, hi.x), (lo.z, lo.z + d.wall), (lo.y, hi.y), m);
    // The west wall, round the opening for the main ridge.
    let west = (lo.x, lo.x + d.wall);
    if let Some(p) = at_x(main, lo.x) {
        let (z0, z1) = (p.z - d.west_gate.0, p.z + d.west_gate.0);
        let (y0, y1) = (p.y - d.west_gate.1, p.y + d.west_gate.2);
        o.wall(west, (lo.z, z0), (lo.y, hi.y), m);
        o.wall(west, (z1, hi.z), (lo.y, hi.y), m);
        o.wall(west, (z0, z1), (lo.y, y0), m);
        o.wall(west, (z0, z1), (y1, hi.y), m);
    }
    // The east wall, round the gate for the neon ramp.
    let east = (hi.x - d.wall, hi.x);
    let (ez, eh, etop) = d.east_gate;
    o.wall(east, (lo.z, ez - eh), (lo.y, hi.y), m);
    o.wall(east, (ez + eh, hi.z), (lo.y, hi.y), m);
    o.wall(east, (ez - eh, ez + eh), (etop, hi.y), m);
    // The pads and their pillars (their colliders are the course blocks).
    for p in [d.pads.start, d.pads.end] {
        let (c, s) = pad_of(p);
        o.part(c, s, "sf-fa-deck");
        let (pc, ps) = pad_of(pillar(d, p));
        o.part(pc, ps, "sf-fa-office");
    }
    // The skylight roof: ribs along and across the hall, light bars under.
    let (nz, nx) = (
        (size.z / d.roof.every.max(1.0)).ceil() as usize,
        (size.x / d.roof.every.max(1.0)).ceil() as usize,
    );
    let ry = hi.y - d.roof.rib.1 / 2.0;
    for i in 0..=nz {
        let z = lo.z + size.z * i as f32 / nz as f32;
        o.part(
            Vec3::new(mid.x, ry, z),
            Vec3::new(size.x, d.roof.rib.1, d.roof.rib.0),
            &d.roof.material,
        );
    }
    for i in 0..=nx {
        let x = lo.x + size.x * i as f32 / nx as f32;
        o.part(
            Vec3::new(x, ry, mid.z),
            Vec3::new(d.roof.rib.0, d.roof.rib.1, size.z),
            &d.roof.material,
        );
    }
    for i in 0..nz {
        let z = lo.z + size.z * (i as f32 + 0.5) / nz as f32;
        o.part(
            Vec3::new(mid.x, hi.y - d.roof.rib.1 - 0.3, z),
            Vec3::new(size.x - 4.0, 0.4, 1.2),
            "sf-lamp-white",
        );
    }
    // The storefront columns up both long walls, facing in.
    let rows = ((d.shops.1 - d.shops.0) / SHOP_SIZE.x).floor().max(0.0) as usize;
    let depth = SHOP_SIZE.z;
    let (north_z, south_z) = (hi.z - d.wall - depth / 2.0, lo.z + d.wall + depth / 2.0);
    for (z, yaw) in [(north_z, PI), (south_z, 0.0)] {
        for i in 0..rows {
            o.models.push(Placed {
                model: SHOP.into(),
                at: Vec3::new(d.shops.0 + SHOP_SIZE.x * (i as f32 + 0.5), lo.y, z),
                rot: Quat::from_rotation_y(yaw),
                scale: Vec3::ONE,
            });
        }
        if rows > 0 {
            let wid = SHOP_SIZE.x * rows as f32;
            o.solid(
                Vec3::new(d.shops.0 + wid / 2.0, lo.y + SHOP_SIZE.y / 2.0, z),
                Vec3::new(wid, SHOP_SIZE.y, depth),
            );
        }
    }
    // The clock tower and the fountain round it, the landmark of the mall.
    let k = &d.tower;
    let c = Vec3::new(k.at.0, lo.y, k.at.1);
    o.cyl(c + Vec3::Y * 1.2, k.rim, 2.4, "sf-stone");
    o.cyl(c + Vec3::Y * 1.6, k.pool, 2.8, "sf-rt-pool");
    o.cyl(
        c + Vec3::Y * k.plinth_height / 2.0,
        k.plinth,
        k.plinth_height,
        "sf-stone",
    );
    let shaft_bottom = lo.y + k.plinth_height;
    let shaft_top = k.clock_y - k.clock;
    o.cyl(
        Vec3::new(c.x, (shaft_bottom + shaft_top) / 2.0, c.z),
        k.shaft,
        shaft_top - shaft_bottom,
        "sf-fa-office",
    );
    o.part(
        Vec3::new(c.x, k.clock_y, c.z),
        Vec3::splat(k.clock * 2.0),
        "sf-fa-glass",
    );
    for (dx, dz) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
        let face = Vec3::new(
            c.x + dx * (k.clock + 0.4),
            k.clock_y,
            c.z + dz * (k.clock + 0.4),
        );
        let sz = |w: f32| {
            if dx != 0.0 {
                Vec3::new(0.8, w, w)
            } else {
                Vec3::new(w, w, 0.8)
            }
        };
        o.part(face, sz(k.clock * 1.7), "sf-neon-gold");
        o.part(
            face + Vec3::new(dx, 0.0, dz) * 0.5,
            sz(k.clock * 1.45),
            "sf-lamp-white",
        );
    }
    o.part(
        Vec3::new(c.x, k.clock_y + k.clock + 1.0, c.z),
        Vec3::new(k.clock * 2.0 + 2.0, 2.0, k.clock * 2.0 + 2.0),
        "sf-neon-gold",
    );
    let spire_bottom = k.clock_y + k.clock + 2.0;
    let spire_top = hi.y - 3.0;
    o.cyl(
        Vec3::new(c.x, (spire_bottom + spire_top) / 2.0, c.z),
        k.spire,
        spire_top - spire_bottom,
        "sf-chrome",
    );
    for (dx, dz) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        o.lamps.push(Lamp {
            at: Vec3::new(c.x + dx * 22.0, k.clock_y, c.z + dz * 22.0),
            lumens: d.light_intensity * 1.6,
            range: d.light_range * 1.4,
            colour: (1.0, 0.9, 0.7),
        });
    }
    // The food court: kiosks round the pool, awnings in neon.
    let (ks, n) = (v3(d.court.size), d.court.count.max(1));
    for i in 0..d.court.count {
        let a = TAU * i as f32 / n as f32;
        let at = Vec3::new(
            k.at.0 + a.cos() * d.court.radius,
            lo.y + ks.y / 2.0,
            k.at.1 + a.sin() * d.court.radius,
        );
        let yaw = -(a + FRAC_PI_2);
        o.part_yaw(at, yaw, ks, "sf-fa-brick");
        let awning = d
            .awnings
            .get(i % d.awnings.len().max(1))
            .map_or("sf-neon-pink", String::as_str);
        o.part_yaw(
            at + Vec3::Y * (ks.y / 2.0 + 0.3),
            yaw,
            Vec3::new(ks.x + 2.0, 0.6, ks.z + 2.0),
            awning,
        );
    }
    // The skybridge gauntlet across the hall.
    let (z_south, z_north) = (south_z + depth / 2.0, north_z - depth / 2.0);
    for c in &d.catwalks {
        let (Some(kind), Some(p)) = (d.kinds.get(c.kind), at_x(main, c.x)) else {
            continue;
        };
        let y = p.y + c.lift;
        let len = z_north - z_south;
        let segs = (len / 10.0).ceil().max(1.0) as usize;
        for i in 0..segs {
            o.models.push(Placed {
                model: format!("features/{}", kind.segment),
                at: Vec3::new(c.x, y, z_south + len * (i as f32 + 0.5) / segs as f32),
                rot: Quat::from_rotation_y(FRAC_PI_2),
                scale: Vec3::new(len / segs as f32 / 10.0, 1.0, 1.0),
            });
        }
        o.catwalks.push(Solid {
            centre: Vec3::new(
                c.x,
                y + (kind.above - kind.below) / 2.0,
                (z_south + z_north) / 2.0,
            ),
            half: Vec3::new(kind.half_width, (kind.above + kind.below) / 2.0, len / 2.0),
            yaw: 0.0,
        });
    }
    // Hanging banners by the long walls.
    const BANNERS: [&str; 6] = [
        "sf-neon-pink",
        "sf-neon-cyan",
        "sf-neon-violet",
        "sf-neon-gold",
        "sf-neon-coral",
        "sf-neon-lime",
    ];
    let b = &d.banners;
    let nb = ((size.x - 2.0 * b.every) / b.every.max(1.0))
        .floor()
        .max(0.0) as usize;
    let mut seed = d.seed;
    for i in 0..=nb {
        let x = lo.x + b.every + i as f32 * b.every;
        for z in [hi.z - b.from_wall, lo.z + b.from_wall] {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            o.part(
                Vec3::new(x, hi.y - b.under_roof - b.size.1 / 2.0, z),
                Vec3::new(0.5, b.size.1, b.size.0),
                BANNERS[(seed >> 16) as usize % BANNERS.len()],
            );
        }
    }
    // Sky gardens beside the cruise.
    for g in &d.gardens {
        let (x, z) = g.at;
        o.cyl(Vec3::new(x, g.top - 0.7, z), g.radius, 1.4, "sf-deck");
        o.cyl(
            Vec3::new(x, g.top + 0.15, z),
            g.radius - 1.5,
            0.3,
            "sf-leaves",
        );
        let stem_h = g.top - 1.4 - lo.y;
        o.cyl(
            Vec3::new(x, lo.y + stem_h / 2.0, z),
            3.0,
            stem_h,
            "sf-metal",
        );
        o.cyl(Vec3::new(x, g.top + 4.5, z), 1.1, 9.0, "sf-wood");
        o.cyl(Vec3::new(x, g.top + 11.0, z), 7.5, 6.0, "sf-leaves");
        o.cyl(Vec3::new(x, g.top + 16.0, z), 4.5, 5.0, "sf-leaves");
    }
    // Lights over the main ridge.
    let (mut walked, mut next) = (0.0, 0.0);
    for seg in main.windows(2) {
        walked += seg[0].distance(seg[1]);
        if walked >= next {
            next += d.light_every.max(1.0);
            if (lo.x..=hi.x).contains(&seg[0].x) {
                o.lamps.push(Lamp {
                    at: seg[0] + Vec3::Y * d.light_up,
                    lumens: d.light_intensity,
                    range: d.light_range,
                    colour: d.light_colour,
                });
            }
        }
    }
    o
}

/// The tallest free stretch (m) of a face's band at a ridge point, minus
/// every catwalk over it: how much room a surfer has to get by.
pub fn free_gap(p: Vec3, half: f32, band: (f32, f32), boxes: &[Solid]) -> f32 {
    let mut cuts: Vec<(f32, f32)> = boxes
        .iter()
        .filter(|b| b.distance_xz(p) < half)
        .map(|b| (b.bottom(), b.top()))
        .collect();
    cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let (mut at, end) = (p.y + band.0, p.y + band.1);
    let mut best = 0.0f32;
    for (lo, hi) in cuts {
        if lo > at {
            best = best.max(lo.min(end) - at);
        }
        at = at.max(hi);
    }
    best.max(end - at)
}

/// The first ridge point whose tunnel a solid enters: within `side` metres
/// sideways of the ridge, from the ramp's underside to `up` metres over it.
pub fn in_tunnel(
    ridges: &[Vec<Vec3>],
    solids: &[Solid],
    side: f32,
    up: f32,
    body: f32,
) -> Option<Vec3> {
    ridges.iter().flatten().copied().find(|p| {
        solids
            .iter()
            .any(|s| s.distance_xz(*p) < side && s.bottom() < p.y + up && s.top() > p.y - body)
    })
}

/// Plan distance along a line up to each sample.
fn walked(line: &[Vec3]) -> Vec<f32> {
    let mut out = vec![0.0];
    for w in line.windows(2) {
        out.push(out[out.len() - 1] + Vec2::new(w[1].x - w[0].x, w[1].z - w[0].z).length());
    }
    out
}

/// Where two ramps (or two parts of one) are on top of each other without
/// the clearance to pass: plan distance under `near` and less than `need`
/// apart in height. A shortcut's last `fuse` metres may join the main line.
pub fn crossings(
    ridges: &[Vec<Vec3>],
    near: f32,
    need: f32,
    fuse: f32,
) -> Vec<(usize, usize, Vec3, Vec3)> {
    let mut out = Vec::new();
    let sums: Vec<Vec<f32>> = ridges.iter().map(|l| walked(l)).collect();
    for a in 0..ridges.len() {
        for b in a..ridges.len() {
            for (i, p) in ridges[a].iter().enumerate().step_by(2) {
                for (j, q) in ridges[b].iter().enumerate().step_by(2) {
                    if a == b && (sums[a][i] - sums[b][j]).abs() < 70.0 {
                        continue;
                    }
                    let h = Vec2::new(p.x - q.x, p.z - q.z).length();
                    if h >= near || (p.y - q.y).abs() >= need {
                        continue;
                    }
                    // A shortcut fusing into the main line (ramp 0).
                    let fused = |b: usize, line: &[Vec3], sum: &[f32], k: usize| {
                        b != 0 && sum[line.len() - 1] - sum[k] < fuse
                    };
                    if a == 0 && b != 0 && fused(b, &ridges[b], &sums[b], j) {
                        continue;
                    }
                    out.push((a, b, *p, *q));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::surf::{self, TUNING, gap, ideal_speed, speed_needed};

    fn read(path: &str) -> Vec<u8> {
        let full = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), path);
        std::fs::read(&full).unwrap_or_else(|_| panic!("no {full}"))
    }

    fn load() -> (std::sync::Arc<Tuning>, std::sync::Arc<Mall>) {
        assert!(
            TUNING.set_bytes(&read(TUNING.path())),
            "surf.ron doesn't parse"
        );
        assert!(
            MALL.set_bytes(&read(MALL.path())),
            "surf_mall.ron doesn't parse"
        );
        (TUNING.get(), MALL.get())
    }

    /// The tuning with this stage in place (its pad and ramps).
    fn with_stage(t: &Tuning, d: &Mall) -> Tuning {
        let mut t = t.clone();
        t.stages = vec![Stage::default(); d.stage];
        t.stages.push(stage(d));
        t.finish = d.pads.end;
        t.speed_gaps = speed_gaps(d);
        t
    }

    #[test]
    fn the_data_file_has_every_field() {
        let (t, d) = load();
        assert_eq!(d.stage, 6);
        assert_eq!(d.ramps.len(), 4);
        assert!(d.ramps.iter().all(|r| r.points.len() >= 4));
        assert!(d.wall > 0.0 && d.floor_strip > 0.0 && d.light_every > 0.0);
        assert!(d.gap_needed > 0.0 && d.shops.1 > d.shops.0 && !d.kinds.is_empty());
        assert!(d.catwalks.len() >= 5 && d.court.count >= 8 && !d.awnings.is_empty());
        assert!(d.gardens.len() >= 3 && d.banners.every > 0.0 && d.seed != 0);
        for c in &d.catwalks {
            assert!(c.kind < d.kinds.len(), "{c:?}");
        }
        assert!(d.speed_gaps.iter().all(|&r| r + 1 < d.ramps.len()));
        assert!(t.ramp_half_width > 0.0);
    }

    #[test]
    fn the_handoffs_are_exactly_the_briefs() {
        let (_, d) = load();
        let main = &d.ramps[0].points;
        let (sc, ss) = pad_of(d.pads.start);
        let (ec, es) = pad_of(d.pads.end);
        assert_eq!(sc, Vec3::new(1030.0, -645.88, -1938.0));
        assert_eq!(ec, Vec3::new(1814.85, -749.16, -1915.74));
        assert_eq!(
            (ss, es),
            (Vec3::new(24.0, 1.0, 24.0), Vec3::new(24.0, 1.0, 24.0))
        );
        // Off P6 heading east: a 4 m hop off the edge, 5.5 m under the pad's middle.
        let first = Vec3::from(main[0]);
        assert!(
            (first - (sc + Vec3::new(16.0, -5.5, 0.0))).length() < 0.02,
            "{first}"
        );
        assert!((first.x - sc.x) - ss.x / 2.0 > 2.0 && (first.x - sc.x) - ss.x / 2.0 < 8.0);
        // Onto P7: the last point 30 m before the pad, 8.5 m over it, still
        // heading east.
        let n = main.len();
        let (last, before) = (Vec3::from(main[n - 1]), Vec3::from(main[n - 2]));
        assert!(
            (last - (ec + Vec3::new(-30.0, 8.5, 0.0))).length() < 0.02,
            "{last}"
        );
        assert!((last.z - before.z).abs() < 0.5 && last.x > before.x);
    }

    #[test]
    fn the_main_line_is_a_real_course() {
        let (t, d) = load();
        let ridges = lines(&t, &d);
        let main = &d.ramps[0].points;
        // It only goes down, never too steeply, and averages a surfable slope.
        for (r, line) in d.ramps.iter().zip(&ridges) {
            for w in line.windows(2) {
                let run = Vec2::new(w[1].x - w[0].x, w[1].z - w[0].z)
                    .length()
                    .max(1e-3);
                let slope = (w[0].y - w[1].y) / run;
                assert!(
                    (-0.001..0.36).contains(&slope),
                    "{} has a slope of {slope} at {}",
                    r.name,
                    w[0]
                );
            }
        }
        let w = walked(&ridges[0]);
        let length = w[w.len() - 1];
        let drop = main[0].1 - main[main.len() - 1].1;
        assert!((1200.0..1500.0).contains(&length), "{length} m of ridge");
        assert!(
            (0.055..0.1).contains(&(drop / length)),
            "{} average",
            drop / length
        );
        // Never a hairpin: the radius stays at 60 m or more (85+ is the fast part).
        for (r, line) in d.ramps.iter().zip(&ridges) {
            for tri in line.windows(3) {
                let (a, b, c) = (tri[0], tri[1], tri[2]);
                let (ab, bc, ca) = (
                    Vec2::new(b.x - a.x, b.z - a.z).length(),
                    Vec2::new(c.x - b.x, c.z - b.z).length(),
                    Vec2::new(c.x - a.x, c.z - a.z).length(),
                );
                let cross = ((b.x - a.x) * (c.z - a.z) - (b.z - a.z) * (c.x - a.x)).abs();
                let radius = if cross < 1e-4 {
                    f32::MAX
                } else {
                    ab * bc * ca / (2.0 * cross)
                };
                assert!(radius >= 60.0, "{} turns at {radius} m around {b}", r.name);
            }
        }
        // A figure-8: the loop's exit passes well under its own start.
        let under = crossings(&ridges[..1], 13.0, 40.0, 0.0);
        assert!(!under.is_empty(), "the loop never passes over itself");
        // Set pieces all the way: the ridge sweeps across the hall, and the
        // loop is a full turn round the tower.
        let zs = ridges[0].iter().map(|p| p.z);
        let (zmin, zmax) = (
            zs.clone().fold(f32::MAX, f32::min),
            zs.fold(f32::MIN, f32::max),
        );
        assert!(zmax - zmin > 200.0, "swings only {} m", zmax - zmin);
        let tower = Vec2::new(d.tower.at.0, d.tower.at.1);
        let ring = ridges[0]
            .iter()
            .filter(|p| (Vec2::new(p.x, p.z).distance(tower) - 72.0).abs() < 6.0)
            .count();
        assert!(ring > 60, "only {ring} samples round the tower");
    }

    #[test]
    fn forks_share_a_start_and_a_finish() {
        let (t, d) = load();
        let ridges = lines(&t, &d);
        let main = &ridges[0];
        let level = |p: Vec3, q: &Vec3| (q.y - p.y).abs() < 15.0;
        let near = |p: Vec3| {
            main.iter()
                .filter(|q| level(p, q))
                .map(|q| Vec2::new(q.x - p.x, q.z - p.z).length())
                .fold(f32::MAX, f32::min)
        };
        let main_pts = &d.ramps[0].points;
        // Both shortcuts end exactly on the main line's own points.
        for name in ["escalator", "service_end"] {
            let r = d.ramps.iter().find(|r| r.name == name).expect(name);
            for p in &r.points[r.points.len() - 2..] {
                assert!(
                    main_pts
                        .iter()
                        .any(|m| (Vec3::from(*m) - Vec3::from(*p)).length() < 0.02),
                    "{name} doesn't fuse into the main line at {p:?}"
                );
            }
        }
        // They begin as hops: 14-40 m beside the main line, a few metres
        // under its ridge there (or, for the second half of the service
        // line, across a gap from the first).
        for name in ["escalator", "service"] {
            let r = d.ramps.iter().find(|r| r.name == name).expect(name);
            let s = Vec3::from(r.points[0]);
            let side = near(s);
            assert!(
                (14.0..40.0).contains(&side),
                "{name} starts {side} m off the ridge"
            );
            let q = main
                .iter()
                .filter(|q| level(s, q))
                .min_by(|a, b| {
                    Vec2::new(a.x - s.x, a.z - s.z)
                        .length()
                        .total_cmp(&Vec2::new(b.x - s.x, b.z - s.z).length())
                })
                .copied()
                .unwrap_or_default();
            assert!(
                (2.0..9.0).contains(&(q.y - s.y)),
                "{name} starts {} m under",
                q.y - s.y
            );
        }
        // The service line's gap is a real speed check and an achievable one.
        let full = with_stage(&t, &d);
        let g = gap(&full, d.stage, 1).expect("a gap after the service line");
        let need = speed_needed(&full, &g).expect("some speed clears it");
        let ideal = ideal_speed(&full, d.stage, g.from.y).expect("an ideal speed");
        assert!(g.length > 20.0 && g.length < 40.0, "{} m gap", g.length);
        assert!(
            need > 28.0 && need < 0.8 * ideal,
            "needs {need}, can carry {ideal}"
        );
        // The shortcuts really are shorter than what they skip.
        let len = |name: &str| {
            let i = d.ramps.iter().position(|r| r.name == name).unwrap_or(0);
            *walked(&ridges[i]).last().unwrap_or(&0.0)
        };
        assert!(len("escalator") > 100.0 && len("service") + len("service_end") > 250.0);
    }

    #[test]
    fn ramps_pass_over_each_other_with_room_to_spare() {
        let (t, d) = load();
        let ridges = lines(&t, &d);
        // A shortcut's last 170 m may run beside (and above) the line it fuses into.
        let bad = crossings(&ridges, 2.0 * t.ramp_half_width + 1.0, 24.0, 170.0);
        assert!(bad.is_empty(), "{:?}", &bad[..bad.len().min(3)]);
    }

    #[test]
    fn a_face_is_open_under_every_bridge_on_every_lane() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let ridges = lines(&t, &d);
        assert!(l.catwalks.len() >= 5);
        for line in &ridges {
            for p in line {
                let gap = free_gap(*p, t.ramp_half_width, d.band, &l.catwalks);
                assert!(gap >= d.gap_needed, "only {gap} m free at {p}");
                for b in l.catwalks.iter().filter(|b| b.distance_xz(*p) < 0.5) {
                    assert!(
                        b.bottom() > p.y || b.top() < p.y - t.ramp_height,
                        "a bridge is inside the ramp at {p}"
                    );
                }
            }
        }
        // Some to duck, some to go over, some to pass under (on the main line).
        let rel = |b: &Solid| at_x(&ridges[0], b.centre.x).map_or(0.0, |p| b.bottom() - p.y);
        assert!(l.catwalks.iter().any(|b| (0.0..3.0).contains(&rel(b))));
        assert!(l.catwalks.iter().any(|b| rel(b) >= 6.0));
        assert!(l.catwalks.iter().any(|b| rel(b) < -10.0));
    }

    #[test]
    fn nothing_else_solid_is_in_a_ramps_tunnel() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let ridges = lines(&t, &d);
        let hit = in_tunnel(&ridges, &l.solids, d.clear_side, d.clear_up, t.ramp_height);
        assert!(hit.is_none(), "a solid is in the way at {hit:?}");
        // The pads' blocks are not on any ridge either (the ridge flies over P7).
        let blocks: Vec<Solid> = blocks(&d)
            .iter()
            .map(|b| {
                let (c, s) = pad_of(*b);
                Solid {
                    centre: c,
                    half: s / 2.0,
                    yaw: 0.0,
                }
            })
            .collect();
        let hit = in_tunnel(&ridges, &blocks, 2.0, 0.0, 0.0);
        assert!(hit.is_none(), "a pad block is under the ridge at {hit:?}");
    }

    #[test]
    fn the_hall_fits_the_course_and_the_floor_is_out_of_reach() {
        let (t, d) = load();
        let ridges = lines(&t, &d);
        let (lo, hi) = (v3(d.min), v3(d.max));
        let full = with_stage(&t, &d);
        let low = surf::stage_low(&full, d.stage).expect("a low point");
        assert!(
            lo.y < low - full.stage_fall - 20.0,
            "the floor is within reach"
        );
        let inside = |p: Vec3| p.cmpge(lo).all() && p.cmple(hi).all();
        // Only the first stretch (before the west wall) and nothing else is outside.
        let outside = ridges.iter().flatten().filter(|p| !inside(**p)).count();
        assert!(outside < 24, "{outside} samples outside the hall");
        for p in &ridges[0] {
            if p.x > lo.x {
                assert!(
                    p.z > lo.z + 70.0 && p.z < hi.z - 20.0 - SHOP_SIZE.z,
                    "{p} too near a wall"
                );
                assert!(p.y < hi.y - 40.0, "{p} too near the roof");
            }
        }
        let o = surf::hall_at(&t);
        assert!(surf::in_hall(&t, o + lo).is_some() && surf::in_hall(&t, o + hi).is_some());
        // The tower stands well inside the loop (the ring is 72 m out).
        assert!(d.tower.pool < 72.0 - d.clear_side - 6.0);
        assert!(d.court.radius + d.court.size.2 / 2.0 < 72.0 - d.clear_side);
    }

    #[test]
    fn the_layout_is_whole_deterministic_and_within_budget() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let again = layout(&t, &d);
        assert_eq!(l, again);
        let rows = ((d.shops.1 - d.shops.0) / SHOP_SIZE.x) as usize;
        assert_eq!(
            l.models.iter().filter(|m| m.model == SHOP).count(),
            2 * rows
        );
        let entities = l.models.len() + l.parts.len() + l.cyls.len();
        assert!(entities <= 1500, "{entities} entities before the dressing");
        assert!(l.solids.len() + l.catwalks.len() <= 100);
        assert!(l.lamps.len() + 4 <= 40, "{} lamps", l.lamps.len());
        assert!(l.lamps.len() >= 12);
        // The pillars reach the floor.
        let bs = blocks(&d);
        assert_eq!(bs.len(), 4);
        assert!((bs[1].0.1 - bs[1].1.1 / 2.0 - d.min.1).abs() < 0.01);
    }

    #[test]
    fn its_models_and_materials_exist() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let assets = format!("{}/../game/assets", env!("CARGO_MANIFEST_DIR"));
        let mut models: Vec<&str> = l.models.iter().map(|m| m.model.as_str()).collect();
        models.sort_unstable();
        models.dedup();
        for m in models {
            assert!(
                std::path::Path::new(&format!("{assets}/models/{m}")).exists(),
                "no model {m}"
            );
        }
        let mut mats: Vec<&str> = l
            .parts
            .iter()
            .map(|p| p.material.as_str())
            .chain(l.cyls.iter().map(|c| c.material.as_str()))
            .collect();
        mats.sort_unstable();
        mats.dedup();
        for m in mats {
            assert!(
                std::path::Path::new(&format!("{assets}/../.modeler/materials/{m}.json")).exists(),
                "no material {m}"
            );
        }
    }

    #[test]
    fn the_baked_map_has_this_course() {
        let (_, d) = load();
        let path = format!(
            "{}/../game/assets/maps/surf_mall.map.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let map: friendslop_world::places::MapFile = serde_json::from_str(&text).expect("map json");
        let parts = surf::course_parts(&map);
        let Some(stage) = parts.stages.get(&(d.stage as u32)) else {
            panic!("the map has no stage {}", d.stage);
        };
        assert_eq!(stage.ramps.len(), d.ramps.len());
        for (a, b) in stage.ramps.iter().zip(&d.ramps) {
            assert_eq!(a.len(), b.points.len(), "{}", b.name);
            for (p, q) in a.iter().zip(&b.points) {
                assert!(
                    (Vec3::from(*p) - Vec3::from(*q)).length() < 0.01,
                    "{} drifted",
                    b.name
                );
            }
        }
        assert_eq!(parts.gaps, speed_gaps(&d));
    }
}
