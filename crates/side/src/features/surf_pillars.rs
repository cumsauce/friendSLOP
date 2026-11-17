//! The pillar run (surf map, #306: another themed section). A stretch of
//! one long ramp is planted with slender neon pillars standing in its
//! faces, left then right then left, so you weave across the ridge between
//! them at speed.
//!
//! Where they stand is [`layout`], a pure function of the course
//! (`rules::surf`) and `data/side/surf_pillars.ron`; each pillar is the modeler's
//! `surf_pillars_pillar.gltf` over a solid cylinder made here (never from the file) and [`free_side`] proves
//! one face is always open. Scenery colliders only: nothing replicates.

use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use crate::AppState;
use crate::rules::surf::{Tuning, hall_at, ridge, tuning};

/// Tuning: `assets/data/side/surf_pillars.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Pillars {
    pub stage: usize,
    pub ramp: usize,
    /// Where along the ridge the pillars start and stop (m), and the gap.
    pub from: f32,
    pub to: f32,
    pub spacing: f32,
    /// How far from the ridge across the face each stands (m).
    pub lateral: f32,
    pub radius: f32,
    pub below: f32,
    pub above: f32,
    pub model: String,
    pub free_gap: f32,
    pub claim_margin: f32,
}

/// The pillar model's own size (m): its shaft radius and height.
pub const MODEL_RADIUS: f32 = 1.2;
pub const MODEL_HEIGHT: f32 = 23.0;

pub static PILLARS: friendslop_data::Data<Pillars> =
    friendslop_data::Data::ron("data/side/surf_pillars.ron");

/// One pillar (hall coordinates): its middle, the ridge point it stands by
/// and which side (-1 left, +1 right) of the ridge it is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pillar {
    pub centre: Vec3,
    pub height: f32,
    pub ridge: Vec3,
    pub side: f32,
}

/// A ramp's ridge samples (hall coordinates), by stage and ramp.
pub fn ridge_line_of(t: &Tuning, stage: usize, ramp: usize) -> Vec<Vec3> {
    t.stages
        .get(stage)
        .and_then(|s| s.ramps.get(ramp))
        .map(|r| ridge(r, t.ramp_samples * 4))
        .unwrap_or_default()
}

/// The pillar ramp's ridge samples.
pub fn ridge_line(t: &Tuning, d: &Pillars) -> Vec<Vec3> {
    ridge_line_of(t, d.stage, d.ramp)
}

/// The ridge's unit direction (flat) at sample `i`, and its left-hand side.
pub(crate) fn across(line: &[Vec3], i: usize) -> Vec3 {
    let a = line[i.saturating_sub(1)];
    let b = line[(i + 1).min(line.len() - 1)];
    let f = (b - a).with_y(0.0).normalize_or(Vec3::NEG_Z);
    Vec3::new(f.z, 0.0, -f.x)
}

/// Where every pillar stands.
pub fn layout(t: &Tuning, d: &Pillars) -> Vec<Pillar> {
    let line = ridge_line(t, d);
    if line.len() < 2 || d.spacing <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut walked, mut next, mut n) = (0.0f32, d.from, 0usize);
    for i in 1..line.len() {
        walked += line[i].distance(line[i - 1]);
        if walked < next || walked > d.to {
            continue;
        }
        next += d.spacing;
        let side = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
        let p = line[i];
        let base = p.y - d.below;
        let height = d.below + d.above;
        out.push(Pillar {
            centre: (p + across(&line, i) * side * d.lateral).with_y(base + height / 2.0),
            height,
            ridge: p,
            side,
        });
        n += 1;
    }
    out
}

/// Is a face side (`side` -1 left, +1 right) of the ridge at `p` open for a
/// surfer: no pillar within `gap` m of its middle? (The face middle is
/// `lateral` m out, where pillars stand.)
pub fn free_side(line: &[Vec3], i: usize, side: f32, d: &Pillars, pillars: &[Pillar]) -> bool {
    let mid = line[i] + across(line, i) * side * d.lateral;
    pillars
        .iter()
        .all(|q| Vec2::new(q.centre.x - mid.x, q.centre.z - mid.z).length() > d.radius + d.free_gap)
}

/// The box (centre, half size) all the pillars sit in, claimed from surf_props.
pub fn claim_box(pillars: &[Pillar], d: &Pillars) -> Option<(Vec3, Vec3)> {
    let first = pillars.first()?;
    let (mut min, mut max) = (first.centre, first.centre);
    for q in pillars {
        min = min.min(q.centre - Vec3::new(0.0, q.height / 2.0, 0.0));
        max = max.max(q.centre + Vec3::new(0.0, q.height / 2.0, 0.0));
    }
    let m = Vec3::splat(d.claim_margin + d.lateral + d.radius);
    Some(((min + max) / 2.0, (max - min) / 2.0 + m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> (std::sync::Arc<Tuning>, std::sync::Arc<Pillars>) {
        crate::features::surf_map::load_course_for_tests();
        for path in [crate::rules::surf::TUNING.path(), PILLARS.path()] {
            let full = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), path);
            let Ok(bytes) = std::fs::read(&full) else {
                panic!("no {full}")
            };
            let ok = if path == PILLARS.path() {
                PILLARS.set_bytes(&bytes)
            } else {
                crate::rules::surf::TUNING.set_bytes(&bytes)
            };
            assert!(ok, "{full} doesn't parse");
        }
        (tuning(), PILLARS.get())
    }

    #[test]
    fn the_data_file_has_every_field() {
        let (_, d) = load();
        assert!(d.spacing > 0.0 && d.to > d.from && d.radius > 0.0 && d.lateral > 0.0);
        assert!(d.below > 0.0 && d.above > 0.0 && !d.model.is_empty());
    }

    #[test]
    fn pillars_weave_left_and_right_along_the_ridge() {
        let (t, d) = load();
        let l = layout(&t, &d);
        assert!(l.len() >= 12, "{} pillars", l.len());
        for w in l.windows(2) {
            assert_ne!(w[0].side, w[1].side);
            assert!(w[0].centre.distance(w[1].centre) >= d.spacing * 0.9);
        }
        // Standing in the faces, not on the ridge or off the ramp.
        for q in &l {
            let off = Vec2::new(q.centre.x - q.ridge.x, q.centre.z - q.ridge.z).length();
            assert!((off - d.lateral).abs() < 0.5, "{off}");
            assert!(off + d.radius < t.ramp_half_width);
        }
        assert_eq!(l, layout(&t, &d));
    }

    #[test]
    fn there_is_always_a_free_face() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let line = ridge_line(&t, &d);
        for i in 0..line.len() {
            let open = [-1.0, 1.0]
                .into_iter()
                .any(|s| free_side(&line, i, s, &d, &l));
            assert!(open, "both faces blocked at {}", line[i]);
        }
    }

    #[test]
    fn a_pillar_blocks_its_own_face() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let line = ridge_line(&t, &d);
        let i = line
            .iter()
            .position(|p| p.distance(l[0].ridge) < 0.01)
            .unwrap_or(0);
        assert!(!free_side(&line, i, l[0].side, &d, &l));
    }

    #[test]
    fn surf_props_keep_out() {
        let (t, d) = load();
        let l = layout(&t, &d);
        let Some((c, h)) = claim_box(&l, &d) else {
            panic!("no pillars")
        };
        crate::features::surf_props::claim(c, h);
        for q in &l {
            assert!(crate::features::surf_props::claimed(q.centre, 0.0));
        }
    }
}
