//! The skybridge slalom (#306, doomy: "an area with lots of sky bridges to
//! dodge"): a stretch of stage 2's ramps runs down a downtown canyon, tall
//! towers both sides and a dense run of sky bridges between them at every
//! height. High ones you pass under, pink-bellied "DUCK" ones skim the ridge
//! so you drop down the face to get under, sunken ones cross below the ramp
//! (you go over), and stubs reach out from one wall over a face so you
//! round them on the other side.
//!
//! Where it all goes is [`layout`], a pure function of the course
//! (`rules::surf`) and `data/side/surf_slalom.ron`; every bridge is solid
//! (static boxes from the layout, never from the models), and
//! [`surfable_gap`] proves a line through stays open.

use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use crate::AppState;
use crate::rules::surf::{Tuning, hall_at, ridge, tuning};

/// Tuning: `assets/data/side/surf_slalom.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Slalom {
    /// Which stage (0-based) and which of its ramps (first, last; 0-based)
    /// the canyon runs along.
    pub stage: usize,
    pub ramps: (usize, usize),
    /// The tower slots: the first one's z (hall coords), then one every
    /// `spacing` m down the course (-z), `slots` of them.
    pub first_z: f32,
    pub spacing: f32,
    pub slots: usize,
    /// Room (m) between the canyon's widest ramp face and a tower's wall.
    pub tower_gap: f32,
    /// The tower section model, its footprint (m, square) and height (m,
    /// stacked); how far towers rise above their highest bridge and reach
    /// down below the ramps.
    pub tower_model: String,
    pub tower_size: f32,
    pub tower_section: f32,
    pub tower_over: f32,
    pub tower_under: f32,
    /// Bridge kinds by index.
    pub kinds: Vec<Kind>,
    /// The bridges.
    pub bridges: Vec<Bridge>,
    /// The sign before every low bridge: model, how far up the course (m)
    /// from the bridge and how high over the ridge.
    pub sign: (String, f32, f32),
    /// How far (m) from the ridge a stub's end stops, across the ground.
    pub stub_reach: f32,
    /// Room (m) a surfer needs in a face to get by, and how far below and
    /// above the ridge (m) the surfable band is.
    pub gap_needed: f32,
    pub band: (f32, f32),
    /// Extra margin (m) around the canyon claimed from surf_props.
    pub claim_margin: f32,
}

/// A kind of bridge: its 10 m tiling segment (along +x) and end collar
/// models, and its solid box's half width and how far it reaches below and
/// above its deck line.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Kind {
    pub segment: String,
    pub end: Option<String>,
    pub half_width: f32,
    pub below: f32,
    pub above: f32,
}

/// Which way a bridge runs.
#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    /// Wall to wall, over (or under) the ramp.
    #[default]
    Across,
    /// From the left (-x) wall out over the left face, stopping short.
    Left,
    /// From the right (+x) wall out over the right face.
    Right,
}

/// One bridge: its tower slot, kind, span and deck height over the ridge
/// (m, negative is below); `duck` puts a sign before it.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Bridge {
    pub slot: usize,
    pub kind: usize,
    pub span: Span,
    pub lift: f32,
    pub duck: bool,
}

pub static SLALOM: friendslop_data::Data<Slalom> =
    friendslop_data::Data::ron("data/side/surf_slalom.ron");

/// A placed model (hall coordinates), stretched along its own x.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    pub model: String,
    pub at: Vec3,
    pub yaw: f32,
    pub stretch: f32,
}

/// A solid box (hall coordinates: centre, full size; axis aligned).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Solid {
    pub centre: Vec3,
    pub size: Vec3,
}

impl Solid {
    fn min(&self) -> Vec3 {
        self.centre - self.size / 2.0
    }
    fn max(&self) -> Vec3 {
        self.centre + self.size / 2.0
    }
}

/// Everything in the canyon.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Layout {
    pub pieces: Vec<Piece>,
    pub solids: Vec<Solid>,
    /// The bridges' boxes alone (also in `solids`), for the line test.
    pub bridges: Vec<Solid>,
    /// The whole canyon (centre, half size), claimed from surf_props.
    pub claim: (Vec3, Vec3),
}

/// The canyon's ridge samples (hall coordinates).
pub fn ridges(t: &Tuning, s: &Slalom) -> Vec<Vec3> {
    let Some(stage) = t.stages.get(s.stage) else {
        return vec![];
    };
    (s.ramps.0..=s.ramps.1)
        .filter_map(|i| stage.ramps.get(i))
        .flat_map(|r| ridge(r, t.ramp_samples * 4))
        .collect()
}

/// The ridge point nearest a z (across the canyon's ramps).
fn ridge_at(line: &[Vec3], z: f32) -> Option<Vec3> {
    line.iter()
        .copied()
        .min_by(|a, b| (a.z - z).abs().total_cmp(&(b.z - z).abs()))
}

/// Where everything goes.
pub fn layout(t: &Tuning, s: &Slalom) -> Layout {
    let mut out = Layout::default();
    let line = ridges(t, s);
    if line.is_empty() || s.kinds.is_empty() {
        return out;
    }
    let half = t.ramp_half_width;
    let lo_x = line.iter().map(|p| p.x).fold(f32::MAX, f32::min) - half - s.tower_gap;
    let hi_x = line.iter().map(|p| p.x).fold(f32::MIN, f32::max) + half + s.tower_gap;
    let slot_z = |k: usize| s.first_z - k as f32 * s.spacing;
    let hs = s.tower_size / 2.0;
    // Each tower's highest bridge end, per slot and side (left, right).
    let mut tops = vec![[None::<f32>; 2]; s.slots];
    for b in &s.bridges {
        let (Some(k), Some(p)) = (s.kinds.get(b.kind), ridge_at(&line, slot_z(b.slot))) else {
            continue;
        };
        if b.slot >= s.slots {
            continue;
        }
        let z = slot_z(b.slot);
        let y = p.y + b.lift;
        let (from, to) = match b.span {
            Span::Across => (lo_x, hi_x),
            Span::Left => (lo_x, p.x - s.stub_reach),
            Span::Right => (p.x + s.stub_reach, hi_x),
        };
        if to - from < 1.0 {
            continue;
        }
        if b.span != Span::Right {
            tops[b.slot][0] = Some(tops[b.slot][0].unwrap_or(f32::MIN).max(y + k.above));
        }
        if b.span != Span::Left {
            tops[b.slot][1] = Some(tops[b.slot][1].unwrap_or(f32::MIN).max(y + k.above));
        }
        let len = to - from;
        let segs = (len / 10.0).ceil().max(1.0);
        for i in 0..segs as usize {
            out.pieces.push(Piece {
                model: k.segment.clone(),
                at: Vec3::new(from + len * (i as f32 + 0.5) / segs, y, z),
                yaw: 0.0,
                stretch: len / segs / 10.0,
            });
        }
        if let Some(end) = &k.end {
            for x in [from + 0.75, to - 0.75] {
                out.pieces.push(Piece {
                    model: end.clone(),
                    at: Vec3::new(x, y, z),
                    yaw: 0.0,
                    stretch: 1.0,
                });
            }
        }
        let solid = Solid {
            centre: Vec3::new((from + to) / 2.0, y + (k.above - k.below) / 2.0, z),
            size: Vec3::new(len, k.above + k.below, k.half_width * 2.0),
        };
        out.solids.push(solid);
        out.bridges.push(solid);
        if b.duck {
            out.pieces.push(Piece {
                model: s.sign.0.clone(),
                at: Vec3::new(p.x, p.y + s.sign.2, z + s.sign.1),
                yaw: 0.0,
                stretch: 1.0,
            });
        }
    }
    // Towers hold the bridges' ends, stacked from below the ramps.
    let floor = line.iter().map(|p| p.y).fold(f32::MAX, f32::min) - s.tower_under;
    let mut top = f32::MIN;
    for (k, sides) in tops.iter().enumerate() {
        for (side, end) in sides.iter().enumerate() {
            let Some(end) = end else { continue };
            let x = if side == 0 { lo_x - hs } else { hi_x + hs };
            let want = end + s.tower_over - floor;
            let floors = (want / s.tower_section.max(1.0)).ceil().max(1.0) as usize;
            let height = floors as f32 * s.tower_section;
            for f in 0..floors {
                out.pieces.push(Piece {
                    model: s.tower_model.clone(),
                    at: Vec3::new(x, floor + f as f32 * s.tower_section, slot_z(k)),
                    yaw: 0.0,
                    stretch: 1.0,
                });
            }
            top = top.max(floor + height);
            out.solids.push(Solid {
                centre: Vec3::new(x, floor + height / 2.0, slot_z(k)),
                size: Vec3::new(s.tower_size, height, s.tower_size),
            });
        }
    }
    let (z0, z1) = (slot_z(0) + hs, slot_z(s.slots.saturating_sub(1)) - hs);
    let m = s.claim_margin;
    let min = Vec3::new(lo_x - s.tower_size - m, floor - m, z1 - m);
    let max = Vec3::new(hi_x + s.tower_size + m, top.max(floor) + m, z0 + m);
    out.claim = ((min + max) / 2.0, (max - min) / 2.0);
    out
}

/// The tallest free stretch (m) of a face's band at a ridge point: the
/// face (`side` -1 left, +1 right) from the ridge out `half` m, in y from
/// `ridge + band.0` to `ridge + band.1`, minus every box over it.
pub fn surfable_gap(p: Vec3, side: f32, half: f32, band: (f32, f32), boxes: &[Solid]) -> f32 {
    let (x0, x1) = if side < 0.0 {
        (p.x - half, p.x)
    } else {
        (p.x, p.x + half)
    };
    let mut cuts: Vec<(f32, f32)> = boxes
        .iter()
        .filter(|b| {
            let (lo, hi) = (b.min(), b.max());
            hi.x > x0 && lo.x < x1 && lo.z <= p.z && hi.z >= p.z
        })
        .map(|b| (b.min().y, b.max().y))
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

pub fn plugin(app: &mut App) {
    app.add_systems(PreUpdate, claim_space)
        .spawn_into("surf", build);
}

/// Keep surf_props' generated pieces out of the canyon (once the data's in).
fn claim_space(mut done: Local<bool>) {
    if *done {
        return;
    }
    let s = SLALOM.get();
    if s.bridges.is_empty() {
        return;
    }
    let (c, h) = layout(&tuning(), &s).claim;
    crate::features::surf_props::claim(c, h);
    *done = true;
}

fn build(mut commands: Commands, assets: Res<AssetServer>) {
    let t = tuning();
    let s = SLALOM.get();
    let o = hall_at(&t);
    let l = layout(&t, &s);
    crate::features::surf_props::claim(l.claim.0, l.claim.1);
    let mut scenes = std::collections::HashMap::new();
    for p in l.pieces {
        let scene = scenes
            .entry(p.model.clone())
            .or_insert_with(|| {
                assets.load(
                    GltfAssetLabel::Scene(0).from_asset(format!("models/features/{}", p.model)),
                )
            })
            .clone();
        commands.spawn((
            WorldAssetRoot(scene),
            Transform::from_translation(o + p.at)
                .with_rotation(Quat::from_rotation_y(p.yaw))
                .with_scale(Vec3::new(p.stretch, 1.0, 1.0)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    for b in l.solids {
        commands.spawn((
            Transform::from_translation(o + b.centre),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(b.size.x, b.size.y, b.size.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> (std::sync::Arc<Tuning>, std::sync::Arc<Slalom>) {
        for path in [crate::rules::surf::TUNING.path(), SLALOM.path()] {
            let full = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), path);
            let Ok(bytes) = std::fs::read(&full) else {
                panic!("no {full}")
            };
            let ok = if path == SLALOM.path() {
                SLALOM.set_bytes(&bytes)
            } else {
                crate::rules::surf::TUNING.set_bytes(&bytes)
            };
            assert!(ok, "{full} doesn't parse");
        }
        (tuning(), SLALOM.get())
    }

    #[test]
    fn the_data_file_has_every_field() {
        let (_, s) = load();
        assert!(!s.kinds.is_empty() && !s.tower_model.is_empty() && !s.sign.0.is_empty());
        assert!(s.spacing > 0.0 && s.slots > 0 && s.tower_section > 0.0 && s.gap_needed > 0.0);
        for b in &s.bridges {
            assert!(b.kind < s.kinds.len() && b.slot < s.slots, "{b:?}");
        }
    }

    #[test]
    fn a_dense_run_of_bridges_at_every_height() {
        let (t, s) = load();
        let l = layout(&t, &s);
        let n = l.bridges.len();
        assert!((8..=14).contains(&n), "{n} bridges");
        let line = ridges(&t, &s);
        let over = |b: &Solid| ridge_at(&line, b.centre.z).map_or(0.0, |p| b.min().y - p.y);
        // Some to duck under (bottom within 3 m over the ridge), some high
        // over, some to go over (under the ramp), some stubs to go round.
        assert!(l.bridges.iter().any(|b| (0.0..3.0).contains(&over(b))));
        assert!(l.bridges.iter().any(|b| over(b) >= 3.0));
        assert!(
            l.bridges
                .iter()
                .any(|b| b.max().y < ridge_at(&line, b.centre.z).map_or(0.0, |p| p.y) - 10.0)
        );
        assert!(s.bridges.iter().any(|b| b.span == Span::Left));
        assert!(s.bridges.iter().any(|b| b.span == Span::Right));
        // A sign before every duck bridge, and towers both sides.
        let signs = l.pieces.iter().filter(|p| p.model == s.sign.0).count();
        assert_eq!(signs, s.bridges.iter().filter(|b| b.duck).count());
        let mid = line.iter().map(|p| p.x).sum::<f32>() / line.len() as f32;
        let towers = l.solids.iter().filter(|b| b.size.x == s.tower_size);
        assert!(
            towers.clone().any(|b| b.centre.x < mid) && towers.clone().any(|b| b.centre.x > mid)
        );
        // Every bridge stands over a ramp, not over a jump between them.
        for b in &l.bridges {
            let near = line.iter().any(|p| (p.z - b.centre.z).abs() < 1.5);
            assert!(near, "bridge at z {} over a gap", b.centre.z);
        }
        // Same data, same layout.
        assert_eq!(l, layout(&t, &s));
    }

    #[test]
    fn there_is_always_a_surfable_line() {
        let (t, s) = load();
        let l = layout(&t, &s);
        for p in ridges(&t, &s) {
            let best = [-1.0, 1.0]
                .map(|side| surfable_gap(p, side, t.ramp_half_width, s.band, &l.solids))
                .into_iter()
                .fold(0.0f32, f32::max);
            assert!(best >= s.gap_needed, "only {best} m free at {p}");
        }
    }

    #[test]
    fn towers_keep_off_the_ramps() {
        let (t, s) = load();
        let l = layout(&t, &s);
        let line = ridges(&t, &s);
        for b in l.solids.iter().filter(|b| b.size.x == s.tower_size) {
            for p in &line {
                let dx = (p.x - b.centre.x).abs() - b.size.x / 2.0;
                let dz = (p.z - b.centre.z).abs() - b.size.z / 2.0;
                assert!(
                    dx > t.ramp_half_width || dz > 0.0,
                    "tower at {} on a ramp",
                    b.centre
                );
            }
        }
    }

    #[test]
    fn a_blocked_face_has_no_gap() {
        let p = Vec3::ZERO;
        let wall = Solid {
            centre: Vec3::new(-5.0, -3.0, 0.0),
            size: Vec3::new(10.0, 14.0, 4.0),
        };
        assert!(surfable_gap(p, -1.0, 6.0, (-10.0, 3.0), &[wall]) < 3.0);
        assert!(surfable_gap(p, 1.0, 6.0, (-10.0, 3.0), &[wall]) >= 3.0);
    }

    #[test]
    fn surf_props_keep_out() {
        let (t, s) = load();
        let l = layout(&t, &s);
        crate::features::surf_props::claim(l.claim.0, l.claim.1);
        for b in &l.bridges {
            assert!(crate::features::surf_props::claimed(b.centre, 0.0));
        }
    }
}
