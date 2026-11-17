//! More themed stretches of the surf map (#306): the fin chicane, the orb
//! garden and the gate run. Each is a run of neon things planted along one
//! ramp: fins that fill a whole face, floating orbs, and frames over the
//! ridge to surf through.
//!
//! Where they all go is [`layout`], a pure function of the course
//! (`rules::surf`) and `data/side/surf_runs.ron`. Every piece is a modeler
//! model (`surf_runs_*.gltf`) over a solid made here, never from the file,
//! and [`free_side`] proves a face is always open.

use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use crate::AppState;
use crate::features::surf_pillars::{across, ridge_line_of};
use crate::rules::surf::{Tuning, hall_at, tuning};

/// What a run is made of.
#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A wall filling one face, alternating sides.
    #[default]
    Fin,
    /// A floating orb in one face, alternating sides.
    Orb,
    /// A frame over the ridge: two posts off the faces and a high beam.
    Gate,
}

/// One run. See `data/side/surf_runs.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Run {
    pub name: String,
    pub kind: Kind,
    pub stage: usize,
    pub ramp: usize,
    pub from: f32,
    pub to: f32,
    pub spacing: f32,
    pub lateral: f32,
    pub lift: f32,
    pub half: (f32, f32, f32),
    #[serde(default)]
    pub beam_lift: f32,
    #[serde(default)]
    pub beam_half: (f32, f32, f32),
    #[serde(default)]
    pub post_x: f32,
    pub model: String,
}

/// Tuning: `assets/data/side/surf_runs.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Runs {
    pub runs: Vec<Run>,
    pub free_gap: f32,
    pub band: (f32, f32),
    pub lane: f32,
    pub claim_margin: f32,
}

pub static RUNS: friendslop_data::Data<Runs> =
    friendslop_data::Data::ron("data/side/surf_runs.ron");

/// A placed model (hall coordinates).
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub model: String,
    pub at: Vec3,
    pub yaw: f32,
}

/// A solid (hall coordinates): a box turned by `yaw` about y, or a sphere
/// of radius `half.x`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solid {
    pub centre: Vec3,
    pub half: Vec3,
    pub yaw: f32,
    pub sphere: bool,
}

impl Solid {
    /// How far it reaches sideways (m).
    pub fn reach(&self) -> f32 {
        Vec2::new(self.half.x, self.half.z).length()
    }
    fn y_range(&self) -> (f32, f32) {
        (self.centre.y - self.half.y, self.centre.y + self.half.y)
    }
}

/// Everything in every run.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Layout {
    pub pieces: Vec<Piece>,
    pub solids: Vec<Solid>,
}

fn v3((x, y, z): (f32, f32, f32)) -> Vec3 {
    Vec3::new(x, y, z)
}

/// The yaw that turns a model's +x onto the horizontal direction `a`.
fn yaw_of(a: Vec3) -> f32 {
    (-a.z).atan2(a.x)
}

/// The ridge point at each station of a run, with its across direction.
pub fn stations(t: &Tuning, r: &Run) -> Vec<(Vec3, Vec3)> {
    let line = ridge_line_of(t, r.stage, r.ramp);
    if line.len() < 2 || r.spacing <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut walked, mut next) = (0.0f32, r.from);
    for i in 1..line.len() {
        walked += line[i].distance(line[i - 1]);
        if walked >= next && walked <= r.to {
            next += r.spacing;
            out.push((line[i], across(&line, i)));
        }
    }
    out
}

/// Where everything in one run goes.
pub fn run_layout(t: &Tuning, r: &Run) -> Layout {
    let mut out = Layout::default();
    for (n, (p, a)) in stations(t, r).into_iter().enumerate() {
        let side = if n.is_multiple_of(2) { 1.0 } else { -1.0 };
        let yaw = yaw_of(a);
        match r.kind {
            Kind::Fin | Kind::Orb => {
                let centre = p + a * side * r.lateral + Vec3::Y * r.lift;
                out.pieces.push(Piece {
                    model: r.model.clone(),
                    at: centre,
                    yaw,
                });
                out.solids.push(Solid {
                    centre,
                    half: v3(r.half),
                    yaw,
                    sphere: r.kind == Kind::Orb,
                });
            }
            Kind::Gate => {
                out.pieces.push(Piece {
                    model: r.model.clone(),
                    at: p,
                    yaw,
                });
                for s in [-1.0, 1.0] {
                    out.solids.push(Solid {
                        centre: p + a * s * r.post_x + Vec3::Y * r.lift,
                        half: v3(r.half),
                        yaw,
                        sphere: false,
                    });
                }
                out.solids.push(Solid {
                    centre: p + Vec3::Y * r.beam_lift,
                    half: v3(r.beam_half),
                    yaw,
                    sphere: false,
                });
            }
        }
    }
    out
}

/// Every run's layout together.
pub fn layout(t: &Tuning, d: &Runs) -> Layout {
    let mut out = Layout::default();
    for r in &d.runs {
        let l = run_layout(t, r);
        out.pieces.extend(l.pieces);
        out.solids.extend(l.solids);
    }
    out
}

/// Is a face side (`side` -1 left, +1 right) of the ridge sample `p` (with
/// across direction `a`) open for a surfer: nothing in the surfable band
/// within `gap` m of the face's lane?
pub fn free_side(p: Vec3, a: Vec3, side: f32, d: &Runs, solids: &[Solid]) -> bool {
    let lane = p + a * side * d.lane;
    solids.iter().all(|s| {
        let (lo, hi) = s.y_range();
        let in_band = hi > p.y + d.band.0 && lo < p.y + d.band.1;
        !in_band
            || Vec2::new(s.centre.x - lane.x, s.centre.z - lane.z).length() > s.reach() + d.free_gap
    })
}

/// The box (centre, half size) a run's things sit in, for claiming.
pub fn claim_box(l: &Layout, margin: f32) -> Option<(Vec3, Vec3)> {
    let first = l.solids.first()?;
    let (mut min, mut max) = (first.centre, first.centre);
    for s in &l.solids {
        let r = Vec3::new(s.reach(), s.half.y, s.reach());
        min = min.min(s.centre - r);
        max = max.max(s.centre + r);
    }
    let m = Vec3::splat(margin);
    Some(((min + max) / 2.0, (max - min) / 2.0 + m))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> (std::sync::Arc<Tuning>, std::sync::Arc<Runs>) {
        crate::features::surf_map::load_course_for_tests();
        for path in [crate::rules::surf::TUNING.path(), RUNS.path()] {
            let full = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), path);
            let Ok(bytes) = std::fs::read(&full) else {
                panic!("no {full}")
            };
            let ok = if path == RUNS.path() {
                RUNS.set_bytes(&bytes)
            } else {
                crate::rules::surf::TUNING.set_bytes(&bytes)
            };
            assert!(ok, "{full} doesn't parse");
        }
        (tuning(), RUNS.get())
    }

    #[test]
    fn the_data_file_has_every_field() {
        let (_, d) = load();
        assert_eq!(d.runs.len(), 2);
        assert!(d.free_gap > 0.0 && d.lane > 0.0 && d.band.1 > d.band.0);
        for r in &d.runs {
            assert!(!r.model.is_empty() && !r.name.is_empty() && r.spacing > 0.0 && r.to > r.from);
        }
        assert!(d.runs.iter().any(|r| r.kind == Kind::Fin));
        assert!(d.runs.iter().any(|r| r.kind == Kind::Gate));
    }

    #[test]
    fn every_run_has_things_standing_on_its_ramp() {
        let (t, d) = load();
        for r in &d.runs {
            let n = stations(&t, r).len();
            assert!(n >= 4, "{}: only {n} stations", r.name);
            let l = run_layout(&t, r);
            assert!(!l.pieces.is_empty() && !l.solids.is_empty(), "{}", r.name);
            // The run stops short of its ramp's end (a speed gap may follow).
            let line = ridge_line_of(&t, r.stage, r.ramp);
            let len: f32 = line.windows(2).map(|w| w[0].distance(w[1])).sum();
            assert!(r.to <= len - 20.0, "{} runs to {} of {len}", r.name, r.to);
        }
        assert_eq!(layout(&t, &d), layout(&t, &d));
    }

    #[test]
    fn there_is_always_a_free_face() {
        let (t, d) = load();
        for r in &d.runs {
            let l = run_layout(&t, r);
            let line = ridge_line_of(&t, r.stage, r.ramp);
            for i in 1..line.len() {
                let a = across(&line, i);
                let open = [-1.0, 1.0]
                    .into_iter()
                    .any(|s| free_side(line[i], a, s, &d, &l.solids));
                assert!(open, "{}: both faces blocked at {}", r.name, line[i]);
            }
        }
    }

    #[test]
    fn fins_and_orbs_block_a_face_but_gates_block_none() {
        let (t, d) = load();
        for r in &d.runs {
            let l = run_layout(&t, r);
            let st = stations(&t, r);
            let blocked = st
                .iter()
                .filter(|(p, a)| {
                    ![-1.0, 1.0]
                        .into_iter()
                        .all(|s| free_side(*p, *a, s, &d, &l.solids))
                })
                .count();
            if r.kind == Kind::Gate {
                assert_eq!(blocked, 0, "{}", r.name);
            } else {
                assert_eq!(blocked, st.len(), "{}", r.name);
            }
        }
    }

    #[test]
    fn a_gate_has_two_posts_and_a_beam_over_the_ridge() {
        let (t, d) = load();
        let Some(r) = d.runs.iter().find(|r| r.kind == Kind::Gate) else {
            panic!("no gate run")
        };
        let l = run_layout(&t, r);
        assert_eq!(l.solids.len(), 3 * l.pieces.len());
        for (p, _) in stations(&t, r) {
            let beam = l
                .solids
                .iter()
                .find(|s| (s.centre.x - p.x).abs() < 0.01 && s.half == v3(r.beam_half));
            assert!(beam.is_some_and(|b| b.centre.y - b.half.y > p.y + d.band.1));
        }
    }

    #[test]
    fn surf_props_keep_out() {
        let (t, d) = load();
        for r in &d.runs {
            let l = run_layout(&t, r);
            let Some((c, h)) = claim_box(&l, d.claim_margin) else {
                panic!("empty run")
            };
            crate::features::surf_props::claim(c, h);
            for s in &l.solids {
                assert!(crate::features::surf_props::claimed(s.centre, 0.0));
            }
        }
    }
}
