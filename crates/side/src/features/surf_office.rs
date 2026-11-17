//! The office tower at the end of the surf map (#306, doomy: "extend the map
//! past the finish ... inside an office building: a huge lobby with pretty
//! trees and glass you can look out and some executive offices").
//!
//! Stage 6 (`data/side/surf.ron`) starts on what used to be the finish pad and
//! runs one long ramp in through a glass wall, down the length of a huge
//! lobby (giant trees and gardens far below, glass walls and roof all round)
//! and on past two rows of floating glass executive offices to the new
//! finish pad in the corner office.
//!
//! [`layout`] is a pure function of the course (`rules::surf`) and
//! `data/side/surf_office.ron`: the shell's solid walls, floor and roof, the
//! office pods, the garden and the lights. The shell's floor lies below the
//! stage's fall line, so you can never land on it (a fall is a wipe-out).
//! Models are modeler prefabs `surf-office-{glass,slab,column}` (the column is stacked `surf-office-pod`s)
//! (`models/features/surf_office_*.gltf`) and the meadow trees, stretched;
//! the colliders come from here, never the files. [`in_tunnel`] proves
//! nothing solid enters the ramp's way.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use crate::AppState;
use crate::features::surf_pillars::ridge_line_of;
use crate::rules::surf::{Tuning, hall_at, joints, pad, tuning};

/// Tuning: `assets/data/side/surf_office.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Office {
    pub stage: usize,
    pub ramp: usize,
    pub min: (f32, f32, f32),
    pub max: (f32, f32, f32),
    pub wall: f32,
    pub glass_max: (f32, f32),
    /// West wall opening: half width, below and above the ridge (m).
    pub opening: (f32, f32, f32),
    pub garden_x: (f32, f32),
    pub garden_side: (f32, f32),
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
    pub light_every: f32,
    pub light_up: f32,
    pub light_colour: (f32, f32, f32),
    pub light_intensity: f32,
    pub light_range: f32,
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

/// Where everything goes.
pub fn layout(t: &Tuning, d: &Office) -> Layout {
    let mut out = Layout::default();
    let line = ridge_line_of(t, d.stage, d.ramp);
    if line.len() < 2 {
        return out;
    }
    let (lo, hi) = (v3(d.min), v3(d.max));
    let mid = (lo + hi) / 2.0;
    let size = hi - lo;
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
    // North, south and east walls; the west wall around the ramp's opening.
    wall(
        &mut out,
        d.wall,
        d.glass_max,
        true,
        hi.z - d.wall / 2.0,
        (lo.x, hi.x),
        (lo.y, hi.y),
    );
    wall(
        &mut out,
        d.wall,
        d.glass_max,
        true,
        lo.z + d.wall / 2.0,
        (lo.x, hi.x),
        (lo.y, hi.y),
    );
    // The east wall has an opening too when the course goes on (stage 7's ramp).
    let east = hi.x - d.wall / 2.0;
    let next = ridge_line_of(t, d.stage + 1, 0);
    match at_x(&next, hi.x).filter(|(p, _)| (p.x - hi.x).abs() < 30.0) {
        Some((p, _)) => {
            let (z0, z1) = (p.z - d.opening.0, p.z + d.opening.0);
            let (y0, y1) = (p.y - d.opening.1, p.y + d.opening.2);
            wall(
                &mut out,
                d.wall,
                d.glass_max,
                false,
                east,
                (lo.z, z0),
                (lo.y, hi.y),
            );
            wall(
                &mut out,
                d.wall,
                d.glass_max,
                false,
                east,
                (z1, hi.z),
                (lo.y, hi.y),
            );
            wall(
                &mut out,
                d.wall,
                d.glass_max,
                false,
                east,
                (z0, z1),
                (lo.y, y0),
            );
            wall(
                &mut out,
                d.wall,
                d.glass_max,
                false,
                east,
                (z0, z1),
                (y1, hi.y),
            );
        }
        None => wall(
            &mut out,
            d.wall,
            d.glass_max,
            false,
            east,
            (lo.z, hi.z),
            (lo.y, hi.y),
        ),
    }
    let west = lo.x + d.wall / 2.0;
    if let Some((p, _)) = at_x(&line, lo.x) {
        let (z0, z1) = (p.z - d.opening.0, p.z + d.opening.0);
        let (y0, y1) = (p.y - d.opening.1, p.y + d.opening.2);
        wall(
            &mut out,
            d.wall,
            d.glass_max,
            false,
            west,
            (lo.z, z0),
            (lo.y, hi.y),
        );
        wall(
            &mut out,
            d.wall,
            d.glass_max,
            false,
            west,
            (z1, hi.z),
            (lo.y, hi.y),
        );
        wall(
            &mut out,
            d.wall,
            d.glass_max,
            false,
            west,
            (z0, z1),
            (lo.y, y0),
        );
        wall(
            &mut out,
            d.wall,
            d.glass_max,
            false,
            west,
            (z0, z1),
            (y1, hi.y),
        );
    }
    // The pad the office ends on (the next stage's start) and its pillar down
    // to the floor (their colliders are the course's blocks).
    let Some(end_pad) = t.stages.get(d.stage + 1).map(|st| st.pad) else {
        return out;
    };
    let (c, s) = pad(end_pad);
    out.pieces.push(Piece {
        model: SLAB.into(),
        at: c,
        rot: Quat::IDENTITY,
        scale: Vec3::new(s.x / 10.0, s.y, s.z / 10.0),
    });
    let top = lo.y;
    let h = (c.y - s.y / 2.0) - top;
    if h > 0.5 {
        out.pieces.push(Piece {
            model: SLAB.into(),
            at: Vec3::new(c.x, top + h / 2.0, c.z),
            rot: Quat::IDENTITY,
            scale: Vec3::new((s.x - 2.0) / 10.0, h, (s.z - 2.0) / 10.0),
        });
    }
    // The garden: trees (their tops stay under the ramp), bushes, flowers.
    let mut n = 0u32;
    let mut rnd = || {
        n += 1;
        hash(d.seed, n)
    };
    let mut plant = |count: usize, models: &[String], scale: (f32, f32), cap_tree: bool| {
        if models.is_empty() {
            return;
        }
        for _ in 0..count {
            let x = d.garden_x.0 + (d.garden_x.1 - d.garden_x.0) * rnd();
            let side = if rnd() < 0.5 { -1.0 } else { 1.0 };
            let lat = d.garden_side.0 + (d.garden_side.1 - d.garden_side.0) * rnd();
            let model = &models[(rnd() * models.len() as f32) as usize % models.len()];
            let mut s = scale.0 + (scale.1 - scale.0) * rnd();
            let yaw = rnd() * std::f32::consts::TAU;
            let Some((p, a)) = at_x(&line, x) else {
                continue;
            };
            let at = p + a * side * lat;
            // Not in the walls or the office tiers lining them.
            let edge = d.wall + POD_SIZE.z + 4.0;
            let tiers = (d.offices.0..=d.offices.1).contains(&at.x);
            let margin = if tiers { edge } else { 6.0 };
            if at.z < lo.z + margin || at.z > hi.z - margin {
                continue;
            }
            let mut y = top;
            if cap_tree {
                let ridge_y = at_x(&line, at.x).map_or(p.y, |(q, _)| q.y);
                s = s.min((ridge_y - d.tree_below - top) / TREE_HEIGHT);
                if s < 2.5 {
                    continue;
                }
                y += TREE_ROOT * s;
            }
            out.pieces.push(Piece {
                model: model.clone(),
                at: Vec3::new(at.x, y, at.z),
                rot: Quat::from_rotation_y(yaw),
                scale: Vec3::splat(s),
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
    let height = d.storey * COLUMN_STOREYS as f32;
    let depth = POD_SIZE.z;
    for (z, yaw) in [
        (hi.z - d.wall - depth / 2.0, std::f32::consts::PI),
        (lo.z + d.wall + depth / 2.0, 0.0),
    ] {
        for i in 0..rows {
            out.pieces.push(Piece {
                model: COLUMN.into(),
                at: Vec3::new(x0 + POD_SIZE.x * (i as f32 + 0.5), top, z),
                rot: Quat::from_rotation_y(yaw),
                scale: Vec3::ONE,
            });
        }
        if rows > 0 {
            let w = POD_SIZE.x * rows as f32;
            out.solids.push(Solid {
                centre: Vec3::new(x0 + w / 2.0, top + height / 2.0, z),
                half: Vec3::new(w / 2.0, height / 2.0, depth / 2.0),
                yaw: 0.0,
            });
        }
    }
    // Lights over the ridge.
    let mut walked = 0.0;
    let mut next = 0.0;
    for w in line.windows(2) {
        walked += w[0].distance(w[1]);
        if walked >= next {
            next += d.light_every.max(1.0);
            if (lo.x..=hi.x).contains(&w[0].x) {
                out.lights.push(w[0] + Vec3::Y * d.light_up);
            }
        }
    }
    out
}

/// Is any solid in the ramp's tunnel: within `clear_side` m of a ridge
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
    use crate::rules::surf::in_hall;

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
        (tuning(), OFFICE.get())
    }

    #[test]
    fn the_data_file_has_every_field() {
        let (_, d) = load();
        assert!(d.wall > 0.0 && d.glass_max.0 > 0.0 && d.light_every > 0.0);
        assert!(
            !d.tree_models.is_empty() && !d.bush_models.is_empty() && !d.flower_models.is_empty()
        );
        assert!(d.offices.1 > d.offices.0 && d.trees > 0 && d.clear_side > 0.0);
    }

    #[test]
    fn the_course_runs_through_the_building_to_the_finish() {
        let (t, d) = load();
        let line = ridge_line_of(&t, d.stage, d.ramp);
        assert!(line.len() > 10, "no ramp");
        let (lo, hi) = (v3(d.min), v3(d.max));
        let inside = |p: Vec3| p.cmpge(lo).all() && p.cmple(hi).all();
        // The ridge starts outside the west wall, runs inside, and ends in
        // front of the finish pad, inside, a little above it.
        assert!(line[0].x < lo.x);
        assert!(line.iter().filter(|p| inside(**p)).count() > line.len() / 2);
        let end = line[line.len() - 1];
        let (c, s) = pad(t.stages[d.stage + 1].pad);
        assert!(inside(c) && inside(c + Vec3::new(s.x / 2.0, 30.0, s.z / 2.0)));
        assert!(end.y > c.y && end.distance(c) < 40.0, "{end} to {c}");
        // The whole building is inside the hall, and the floor is under the
        // fall line (you can't land on it).
        let o = hall_at(&t);
        assert!(in_hall(&t, o + lo).is_some() && in_hall(&t, o + hi).is_some());
        let low = line.iter().map(|p| p.y).fold(f32::MAX, f32::min);
        assert!(
            lo.y < low - t.stage_fall,
            "floor {} vs fall line {}",
            lo.y,
            low - t.stage_fall
        );
    }

    #[test]
    fn nothing_solid_enters_the_ramps_tunnel() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let line = ridge_line_of(&t, d.stage, d.ramp);
        let hit = in_tunnel(&line, &l.solids, &d, t.ramp_height);
        assert!(hit.is_none(), "a solid blocks the ramp at {hit:?}");
    }

    #[test]
    fn a_solid_in_the_way_is_found() {
        let (t, d) = load();
        let line = ridge_line_of(&t, d.stage, d.ramp);
        let p = line[line.len() / 2];
        let wall = Solid {
            centre: p + Vec3::Y,
            half: Vec3::splat(1.0),
            yaw: 0.0,
        };
        assert!(in_tunnel(&line, &[wall], &d, t.ramp_height).is_some());
    }

    #[test]
    fn the_shell_the_pods_and_the_garden_are_all_there() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let (lo, hi) = (v3(d.min), v3(d.max));
        let columns = l.pieces.iter().filter(|p| p.model == COLUMN).count();
        let rows = ((d.offices.1 - d.offices.0) / POD_SIZE.x) as usize;
        assert_eq!(columns, 2 * rows);
        assert!(rows >= 10, "only {rows} columns a side");
        // Tiers up both walls, face to face across the atrium, under the roof.
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
        assert!(trees >= 10, "only {trees} trees");
        assert!(l.pieces.iter().filter(|p| p.model == GLASS).count() > 20);
        assert!(l.lights.len() >= 8);
        // Every pod floats beside the ramp, never over it.
        for s in l
            .solids
            .iter()
            .filter(|s| (s.half.y - POD_SIZE.y / 2.0).abs() < 4.0)
        {
            assert!(s.half.x >= 9.0);
        }
        // Same data, same building.
        assert_eq!(l, layout(&t, &d));
    }

    #[test]
    fn every_tree_stays_under_the_ramp_and_off_it() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let line = ridge_line_of(&t, d.stage, d.ramp);
        for p in l.pieces.iter().filter(|p| d.tree_models.contains(&p.model)) {
            let top = p.at.y - TREE_ROOT * p.scale.y + TREE_HEIGHT * p.scale.y;
            let ridge = at_x(&line, p.at.x).map_or(0.0, |(r, _)| r.y);
            assert!(top <= ridge - d.tree_below + 0.5, "tree {} too tall", p.at);
            assert!(
                line.iter()
                    .all(|r| Vec2::new(r.x - p.at.x, r.z - p.at.z).length() > 14.0)
            );
        }
    }

    #[test]
    fn the_city_and_props_keep_out() {
        let (t, d) = load();
        let l = layout(&t, &d);
        crate::features::surf_props::claim(l.claim.0, l.claim.1);
        assert!(crate::features::surf_props::claimed(l.claim.0, 0.0));
    }
}
