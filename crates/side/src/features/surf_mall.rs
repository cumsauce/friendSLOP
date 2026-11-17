//! The mall at the end of the surf map (#306, doomy: "a big mall area
//! complete with storefronts, balconies and catwalks to dodge, a food court,
//! a nice glass roof to see out of, and plenty of winding paths").
//!
//! Stage 7 (`data/side/surf.ron`) starts on the office's far-end pad and
//! winds in two big S-bends through a huge glass-roofed hall: tiers of
//! storefronts with balconies and neon signs up both long walls, a food
//! court on the floor far below, FOOD COURT signs, and catwalks across the
//! hall to dodge (over, under, a low one to duck, stubs from the balconies).
//!
//! [`layout`] is a pure function of the course (`rules::surf`) and
//! `data/side/surf_mall.ron`; every wall, shop block and catwalk is a solid
//! made here (never from a model file). The floor is below the stage's fall
//! line, so you can't land on it. [`in_tunnel`] proves only the catwalks enter
//! the ramp's way and [`free_gap`] proves a face stays open under each.

use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use crate::AppState;
use crate::features::surf_office::{Layout, Piece, Solid, at_x, in_tunnel, v3, wall};
use crate::features::surf_pillars::ridge_line_of;
use crate::features::surf_slalom::{Kind, Span};
use crate::rules::surf::{Tuning, hall_at, pad, tuning};

/// One catwalk. `span`: across the whole mall, or a stub from one side.
#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq)]
pub enum Side {
    #[default]
    Across,
    North,
    South,
}

#[derive(Deserialize, Default, Debug, Clone)]
pub struct CatWalk {
    pub x: f32,
    pub kind: usize,
    pub span: Side,
    pub lift: f32,
}

/// Tuning: `assets/data/side/surf_mall.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Mall {
    pub stage: usize,
    pub ramp: usize,
    pub min: (f32, f32, f32),
    pub max: (f32, f32, f32),
    pub wall: f32,
    pub glass_max: (f32, f32),
    pub opening: (f32, f32, f32),
    pub shops: (f32, f32),
    pub food_court: (f32, f32),
    pub sign: (f32, f32, f32),
    pub kinds: Vec<Kind>,
    pub catwalks: Vec<CatWalk>,
    pub stub_reach: f32,
    pub gap_needed: f32,
    pub band: (f32, f32),
    pub light_every: f32,
    pub light_up: f32,
    pub light_colour: (f32, f32, f32),
    pub light_intensity: f32,
    pub light_range: f32,
    pub clear_side: f32,
    pub clear_up: f32,
}

pub static MALL: friendslop_data::Data<Mall> =
    friendslop_data::Data::ron("data/side/surf_mall.ron");

const GLASS: &str = "features/surf_office_glass.gltf";
const SLAB: &str = "features/surf_office_slab.gltf";
const SHOP: &str = "features/surf_mall_column.gltf";
const FOOD: &str = "features/surf_mall_foodcourt.gltf";
const SIGN: &str = "features/surf_mall_sign.gltf";
/// The shop column model: 20 m wide, 12 m deep, 96 m tall.
const SHOP_SIZE: Vec3 = Vec3::new(20.0, 96.0, 12.0);

// `Span` (the slalom's) is imported only so the data's kind type is shared.
const _: Option<Span> = None;

/// Where everything goes.
pub fn layout(t: &Tuning, d: &Mall) -> Layout {
    let mut out = Layout::default();
    let line = ridge_line_of(t, d.stage, d.ramp);
    if line.len() < 2 {
        return out;
    }
    let (lo, hi) = (v3(d.min), v3(d.max));
    let (mid, size) = ((lo + hi) / 2.0, hi - lo);
    out.claim = (mid, size / 2.0 + Vec3::splat(8.0));
    // Floor (below the fall line) and glass roof.
    let floor = Vec3::new(mid.x, lo.y - d.wall / 2.0, mid.z);
    out.pieces.push(Piece {
        model: SLAB.into(),
        at: floor,
        rot: Quat::IDENTITY,
        scale: Vec3::new(size.x / 10.0, d.wall, size.z / 10.0),
    });
    out.solids.push(Solid {
        centre: floor,
        half: Vec3::new(size.x / 2.0, d.wall / 2.0, size.z / 2.0),
        yaw: 0.0,
    });
    let roof_y = hi.y - d.wall / 2.0;
    for (a, la) in crate::features::surf_office::tiles(lo.x, hi.x, d.glass_max.0) {
        for (b, lb) in crate::features::surf_office::tiles(lo.z, hi.z, d.glass_max.0) {
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
    // Walls: north, south and east whole; the west one around the opening.
    let w = (d.wall, d.glass_max);
    wall(
        &mut out,
        w.0,
        w.1,
        true,
        hi.z - d.wall / 2.0,
        (lo.x, hi.x),
        (lo.y, hi.y),
    );
    wall(
        &mut out,
        w.0,
        w.1,
        true,
        lo.z + d.wall / 2.0,
        (lo.x, hi.x),
        (lo.y, hi.y),
    );
    wall(
        &mut out,
        w.0,
        w.1,
        false,
        hi.x - d.wall / 2.0,
        (lo.z, hi.z),
        (lo.y, hi.y),
    );
    let west = lo.x + d.wall / 2.0;
    if let Some((p, _)) = at_x(&line, lo.x) {
        let (z0, z1) = (p.z - d.opening.0, p.z + d.opening.0);
        let (y0, y1) = (p.y - d.opening.1, p.y + d.opening.2);
        wall(&mut out, w.0, w.1, false, west, (lo.z, z0), (lo.y, hi.y));
        wall(&mut out, w.0, w.1, false, west, (z1, hi.z), (lo.y, hi.y));
        wall(&mut out, w.0, w.1, false, west, (z0, z1), (lo.y, y0));
        wall(&mut out, w.0, w.1, false, west, (z0, z1), (y1, hi.y));
    }
    // The finish pad and its pillar down to the floor (colliders: the blocks).
    let (c, s) = pad(t.finish);
    out.pieces.push(Piece {
        model: SLAB.into(),
        at: c,
        rot: Quat::IDENTITY,
        scale: Vec3::new(s.x / 10.0, s.y, s.z / 10.0),
    });
    let h = (c.y - s.y / 2.0) - lo.y;
    if h > 0.5 {
        out.pieces.push(Piece {
            model: SLAB.into(),
            at: Vec3::new(c.x, lo.y + h / 2.0, c.z),
            rot: Quat::IDENTITY,
            scale: Vec3::new((s.x - 2.0) / 10.0, h, (s.z - 2.0) / 10.0),
        });
    }
    // Storefront columns up both long walls, facing in.
    let rows = ((d.shops.1 - d.shops.0) / SHOP_SIZE.x).floor().max(0.0) as usize;
    let depth = SHOP_SIZE.z;
    let (north_z, south_z) = (hi.z - d.wall - depth / 2.0, lo.z + d.wall + depth / 2.0);
    for (z, yaw) in [(north_z, PI), (south_z, 0.0)] {
        for i in 0..rows {
            out.pieces.push(Piece {
                model: SHOP.into(),
                at: Vec3::new(d.shops.0 + SHOP_SIZE.x * (i as f32 + 0.5), lo.y, z),
                rot: Quat::from_rotation_y(yaw),
                scale: Vec3::ONE,
            });
        }
        if rows > 0 {
            let wid = SHOP_SIZE.x * rows as f32;
            out.solids.push(Solid {
                centre: Vec3::new(d.shops.0 + wid / 2.0, lo.y + SHOP_SIZE.y / 2.0, z),
                half: Vec3::new(wid / 2.0, SHOP_SIZE.y / 2.0, depth / 2.0),
                yaw: 0.0,
            });
        }
    }
    // The food court on the floor, and the signs on the walls.
    if let Some((p, _)) = at_x(&line, d.food_court.0) {
        out.pieces.push(Piece {
            model: FOOD.into(),
            at: Vec3::new(d.food_court.0, lo.y, p.z),
            rot: Quat::IDENTITY,
            scale: Vec3::splat(d.food_court.1),
        });
    }
    if let Some((p, _)) = at_x(&line, d.sign.0) {
        let y = p.y + d.sign.1;
        for (z, yaw) in [
            (north_z - depth / 2.0 - 1.0, PI),
            (south_z + depth / 2.0 + 1.0, 0.0),
        ] {
            out.pieces.push(Piece {
                model: SIGN.into(),
                at: Vec3::new(d.sign.0, y, z),
                rot: Quat::from_rotation_y(yaw),
                scale: Vec3::splat(d.sign.2),
            });
        }
    }
    // The catwalks across the hall between the shop blocks.
    let (z_south, z_north) = (south_z + depth / 2.0, north_z - depth / 2.0);
    for c in &d.catwalks {
        let (Some(k), Some((p, _))) = (d.kinds.get(c.kind), at_x(&line, c.x)) else {
            continue;
        };
        let y = p.y + c.lift;
        let (from, to) = match c.span {
            Side::Across => (z_south, z_north),
            Side::North => (p.z + d.stub_reach, z_north),
            Side::South => (z_south, p.z - d.stub_reach),
        };
        let len = to - from;
        if len < 1.0 {
            continue;
        }
        let segs = (len / 10.0).ceil().max(1.0) as usize;
        for i in 0..segs {
            out.pieces.push(Piece {
                model: format!("features/{}", k.segment),
                at: Vec3::new(c.x, y, from + len * (i as f32 + 0.5) / segs as f32),
                rot: Quat::from_rotation_y(FRAC_PI_2),
                scale: Vec3::new(len / segs as f32 / 10.0, 1.0, 1.0),
            });
        }
        out.catwalks.push(Solid {
            centre: Vec3::new(c.x, y + (k.above - k.below) / 2.0, (from + to) / 2.0),
            half: Vec3::new(k.half_width, (k.above + k.below) / 2.0, len / 2.0),
            yaw: 0.0,
        });
    }
    // Lights over the ridge.
    let (mut walked, mut next) = (0.0, 0.0);
    for seg in line.windows(2) {
        walked += seg[0].distance(seg[1]);
        if walked >= next {
            next += d.light_every.max(1.0);
            if (lo.x..=hi.x).contains(&seg[0].x) {
                out.lights.push(seg[0] + Vec3::Y * d.light_up);
            }
        }
    }
    out
}

/// The tallest free stretch (m) of a face's band at a ridge point, minus
/// every catwalk over it: how much room a surfer has to get by.
pub fn free_gap(p: Vec3, half: f32, band: (f32, f32), boxes: &[Solid]) -> f32 {
    let mut cuts: Vec<(f32, f32)> = boxes
        .iter()
        .filter(|b| b.distance_xz(p) < half)
        .map(|b| (b.centre.y - b.half.y, b.centre.y + b.half.y))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::surf::in_hall;

    fn load() -> (std::sync::Arc<Tuning>, std::sync::Arc<Mall>) {
        crate::features::surf_map::load_course_for_tests();
        for path in [crate::rules::surf::TUNING.path(), MALL.path()] {
            let full = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), path);
            let Ok(bytes) = std::fs::read(&full) else {
                panic!("no {full}")
            };
            let ok = if path == MALL.path() {
                MALL.set_bytes(&bytes)
            } else {
                crate::rules::surf::TUNING.set_bytes(&bytes)
            };
            assert!(ok, "{full} doesn't parse");
        }
        (tuning(), MALL.get())
    }

    #[test]
    fn the_data_file_has_every_field() {
        let (_, d) = load();
        assert!(d.wall > 0.0 && d.glass_max.0 > 0.0 && d.light_every > 0.0 && d.gap_needed > 0.0);
        assert!(d.shops.1 > d.shops.0 && !d.kinds.is_empty() && d.catwalks.len() >= 10);
        for c in &d.catwalks {
            assert!(c.kind < d.kinds.len(), "{c:?}");
        }
    }

    #[test]
    fn the_course_winds_through_the_mall_to_the_finish() {
        let (t, d) = load();
        let line = ridge_line_of(&t, d.stage, d.ramp);
        let (lo, hi) = (v3(d.min), v3(d.max));
        let inside = |p: Vec3| p.cmpge(lo).all() && p.cmple(hi).all();
        assert!(line[0].x < lo.x);
        assert!(line.iter().filter(|p| inside(**p)).count() > line.len() * 3 / 4);
        // It winds: the ridge swings across the hall both ways.
        let zs: Vec<f32> = line.iter().map(|p| p.z).collect();
        let (zmin, zmax) = (
            zs.iter().copied().fold(f32::MAX, f32::min),
            zs.iter().copied().fold(f32::MIN, f32::max),
        );
        assert!(zmax - zmin > 60.0, "only swings {} m", zmax - zmin);
        // The finish is inside, just past the ridge's end, and the whole mall
        // is in the hall with its floor under the fall line.
        let end = line[line.len() - 1];
        let (c, _) = pad(t.finish);
        assert!(
            inside(c) && end.y > c.y && end.distance(c) < 40.0,
            "{end} {c}"
        );
        let o = hall_at(&t);
        assert!(in_hall(&t, o + lo).is_some() && in_hall(&t, o + hi).is_some());
        let low = line.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        assert!(lo.y < low - t.stage_fall);
        // The start is a short hop off the previous pad like every stage's.
        let (pc, ps) = pad(t.stages[d.stage].pad);
        let gap = Vec2::new(line[0].x - pc.x, line[0].z - pc.z).length() - ps.x / 2.0;
        assert!((2.0..8.0).contains(&gap), "{gap} m off the pad");
    }

    #[test]
    fn only_catwalks_enter_the_ramps_tunnel() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let line = ridge_line_of(&t, d.stage, d.ramp);
        let hit = in_tunnel_with(&line, &l.solids, &d, t.ramp_height);
        assert!(hit.is_none(), "a solid blocks the ramp at {hit:?}");
    }

    /// `in_tunnel` for this mall's numbers.
    fn in_tunnel_with(line: &[Vec3], solids: &[Solid], d: &Mall, body: f32) -> Option<Vec3> {
        let o = crate::features::surf_office::Office {
            clear_side: d.clear_side,
            clear_up: d.clear_up,
            ..default()
        };
        in_tunnel(line, solids, &o, body)
    }

    #[test]
    fn a_face_is_always_open_under_every_catwalk() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let line = ridge_line_of(&t, d.stage, d.ramp);
        assert!(l.catwalks.len() >= 12);
        for p in &line {
            let gap = free_gap(*p, t.ramp_half_width, d.band, &l.catwalks);
            assert!(gap >= d.gap_needed, "only {gap} m free at {p}");
        }
        // None is inside the ramp's body: over the ridge, or well under it.
        for p in &line {
            for b in l.catwalks.iter().filter(|b| b.distance_xz(*p) < 0.5) {
                let (lo, hi) = (b.centre.y - b.half.y, b.centre.y + b.half.y);
                assert!(
                    lo > p.y || hi < p.y - t.ramp_height,
                    "catwalk in the ramp at {p}"
                );
            }
        }
        // Some to duck, some to go over, some to pass under.
        let rel =
            |b: &Solid| at_x(&line, b.centre.x).map_or(0.0, |(p, _)| b.centre.y - b.half.y - p.y);
        assert!(l.catwalks.iter().any(|b| (0.0..3.0).contains(&rel(b))));
        assert!(l.catwalks.iter().any(|b| rel(b) >= 6.0));
        assert!(l.catwalks.iter().any(|b| rel(b) < -10.0));
    }

    #[test]
    fn the_mall_has_its_shops_food_court_and_roof() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let rows = ((d.shops.1 - d.shops.0) / SHOP_SIZE.x) as usize;
        assert_eq!(
            l.pieces.iter().filter(|p| p.model == SHOP).count(),
            2 * rows
        );
        assert_eq!(l.pieces.iter().filter(|p| p.model == FOOD).count(), 1);
        assert_eq!(l.pieces.iter().filter(|p| p.model == SIGN).count(), 2);
        assert!(l.pieces.iter().filter(|p| p.model == GLASS).count() > 40);
        assert!(l.lights.len() >= 10);
        // The shop tiers rise under the roof.
        assert!(d.min.1 + SHOP_SIZE.y < d.max.1 - d.wall);
        assert_eq!(l, layout(&t, &d));
    }

    #[test]
    fn the_signs_stand_clear_of_the_catwalks() {
        let (_, d) = load();
        for c in &d.catwalks {
            assert!(
                (c.x - d.sign.0).abs() > 30.0,
                "catwalk {c:?} hides the sign"
            );
        }
    }
}
