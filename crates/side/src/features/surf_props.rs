//! Big set pieces around the surf course (#306, doomy: "more fun stuff on
//! the surf map: BIG stuff like cranes and arches and sky bridges").
//!
//! Nothing here is placed by hand: [`layout`] works it out from the course
//! and the generated city (`rules::surf::city`), so it follows the course
//! when the course changes.
//!
//! - **Arches and gates** straddle the ramps every so often along each
//!   ridge (neon arches, truss portals, floating rings) and a FINISH gantry
//!   stands over the finish pad.
//! - **Rooftops**: water towers, billboards, helipads, broadcast masts,
//!   pools, air-con plant and tower cranes on the glass towers' roofs.
//! - **Sky bridges** join pairs of towers: glass tubes, steel trusses and
//!   catwalks, built from 10 m segments.
//! - **Sky floaters**: blimps, hot-air balloons, giant neon arrows, a disco
//!   ball and floating rest platforms drift beside the course.
//!
//! Nothing may get in a ramp's way: every piece keeps clear of the ramps
//! ([`clear`]). Chances, counts and spacings are `data/side/surf_props.ron`;
//! the models are modeler prefabs `sf-*` (`models/features/surf_*.gltf`),
//! and the colliders come from here, so they never wait on a file.

use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use crate::AppState;
use crate::rules::surf::{Piece, Tuning, city, gap, hall_at, ridge, tuning};

/// Tuning numbers: `assets/data/side/surf_props.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Props {
    /// Seed for every pick (change it for different props).
    pub seed: u32,
    /// Metres of ridge between arches (and none this close to a pad).
    pub arch_every: f32,
    pub arch_pad_clear: f32,
    /// The arch models, picked in turn along each ramp, each with how far
    /// (m) it's lifted above the ridge.
    pub arches: Vec<(String, f32)>,
    /// Chance a tower's roof gets a prop, and the roof props with their
    /// height and reach (m) from the roof's middle.
    pub roof_chance: f32,
    pub roofs: Vec<RoofProp>,
    /// Sky bridges: how many at most, the gap between towers they span
    /// (min, max m), how far under the lower roof they run, and their kinds.
    pub bridges: usize,
    pub bridge_gap: (f32, f32),
    pub bridge_drop: f32,
    pub bridge_kinds: Vec<BridgeKind>,
    /// Things floating beside the course: how many of each.
    pub floaters: Vec<Floater>,
    /// How far (m) floaters keep sideways from a ramp, and their height
    /// above it (min, max).
    pub float_off: (f32, f32),
    pub float_up: (f32, f32),
    /// Clearance (m) every piece keeps from a ramp's body.
    pub margin: f32,
    /// How far floaters bob (m) and how fast (cycles a second); how fast
    /// spinning things turn (radians a second).
    pub bob: (f32, f32),
    pub spin: f32,
    /// The sign over every speed gap (`Tuning::speed_gaps`): its model,
    /// pointing along the jump, and how high (m) over the gap's middle.
    pub speed_sign: (String, f32),
}

/// A roof prop: its model, height and reach from the roof's middle, odds,
/// and its solid box (centre, size; model space) if it has one.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct RoofProp {
    pub model: String,
    pub height: f32,
    pub reach: f32,
    pub weight: f32,
    pub solid: Option<((f32, f32, f32), (f32, f32, f32))>,
}

/// A kind of sky bridge: its segment and end models, and its deck's
/// half width and thickness.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct BridgeKind {
    pub segment: String,
    pub end: Option<String>,
    pub width: f32,
    pub deck: f32,
}

/// Something floating in the sky: model, how many, its size (radius, m),
/// whether it bobs or spins, and its solid box if any.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Floater {
    pub model: String,
    pub count: usize,
    pub radius: f32,
    pub bobs: bool,
    pub spins: bool,
    pub solid: Option<((f32, f32, f32), (f32, f32, f32))>,
}

pub static PROPS: friendslop_data::Data<Props> =
    friendslop_data::Data::ron("data/side/surf_props.ron");

/// One placed piece, in hall coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub model: String,
    pub at: Vec3,
    pub yaw: f32,
    /// Stretch along the piece's own x (bridge segments).
    pub stretch: f32,
    /// Solid boxes (centre, size) in the piece's own space.
    pub solids: Vec<(Vec3, Vec3)>,
    pub motion: Motion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Still,
    Bob,
    Spin,
}

fn hash(seed: u32, i: u32) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9) ^ i.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Does something standing at `p` (its base), `reach` metres across from
/// its middle and rising to `top`, keep out of every ramp's way? A ramp is
/// its ridge line, `half` wide and `tall` deep below it.
pub fn clear(
    ridges: &[Vec3],
    p: Vec3,
    reach: f32,
    top: f32,
    half: f32,
    tall: f32,
    margin: f32,
) -> bool {
    ridges.iter().all(|q| {
        let flat = Vec2::new(q.x - p.x, q.z - p.z).length();
        // Far enough sideways, or wholly under the ramp's body, or well over it
        // (room to jump).
        flat > reach + half + margin || top < q.y - tall - margin || p.y > q.y + tall + margin
    })
}

pub use crate::rules::surf::{claim, claimed};

/// Where everything goes.
pub fn layout(t: &Tuning, p: &Props) -> Vec<Placed> {
    let mut out = Vec::new();
    let mut n = 0u32;
    let mut rnd = || {
        n += 1;
        hash(p.seed, n)
    };
    let ridges: Vec<Vec<Vec3>> = t
        .stages
        .iter()
        .flat_map(|s| s.ramps.iter().map(|r| ridge(r, t.ramp_samples * 4)))
        .collect();
    let all: Vec<Vec3> = ridges.iter().flatten().copied().collect();
    let pads: Vec<Vec3> = t
        .stages
        .iter()
        .map(|s| Vec3::from(s.pad.0))
        .chain(std::iter::once(Vec3::from(t.finish.0)))
        .collect();
    let (half, tall, margin) = (t.ramp_half_width, t.ramp_height, p.margin);

    // Arches over the ramps, every so often along each ridge.
    if !p.arches.is_empty() && p.arch_every > 0.0 {
        let mut k = 0usize;
        for line in &ridges {
            let mut since = p.arch_every * 0.5;
            for w in line.windows(2) {
                since += w[0].distance(w[1]);
                let near_pad = pads.iter().any(|q| q.distance(w[1]) < p.arch_pad_clear);
                if since < p.arch_every || near_pad {
                    continue;
                }
                since = 0.0;
                let d = w[1] - w[0];
                let (model, lift) = &p.arches[k % p.arches.len()];
                out.push(Placed {
                    model: model.clone(),
                    at: w[1] + Vec3::Y * *lift,
                    yaw: d.x.atan2(d.z),
                    stretch: 1.0,
                    solids: vec![],
                    motion: Motion::Still,
                });
                k += 1;
            }
        }
    }
    // A sign over every speed gap, pointing across it.
    for &(stage, ramp) in &t.speed_gaps {
        let Some(g) = gap(t, stage, ramp) else {
            continue;
        };
        out.push(Placed {
            model: p.speed_sign.0.clone(),
            at: (g.from + g.to) / 2.0 + Vec3::Y * p.speed_sign.1,
            yaw: g.dir.x.atan2(g.dir.y),
            stretch: 1.0,
            solids: vec![],
            motion: Motion::Still,
        });
    }
    // The finish gantry over the finish pad.
    let (fc, fs) = (Vec3::from(t.finish.0), Vec3::from(t.finish.1));
    out.push(Placed {
        model: "surf_finish_gantry.gltf".into(),
        at: fc + Vec3::Y * fs.y / 2.0,
        yaw: 0.0,
        stretch: 1.0,
        solids: vec![
            (Vec3::new(-13.0, 5.5, 0.0), Vec3::new(2.0, 13.0, 2.0)),
            (Vec3::new(13.0, 5.5, 0.0), Vec3::new(2.0, 13.0, 2.0)),
        ],
        motion: Motion::Still,
    });

    let blocks = city(t);
    let towers: Vec<_> = blocks
        .iter()
        .filter(|b| matches!(b.piece, Piece::Tower { .. }))
        .collect();
    // Towers that already carry a crane (the city's steel) stay as they are.
    let has_steel = |b: &crate::rules::surf::Block| {
        blocks.iter().any(|s| {
            s.piece == Piece::Steel
                && (s.centre.xz() - b.centre.xz()).length() < b.size.x.min(b.size.z) / 2.0
                && s.centre.y > b.top() - 1.0
        })
    };

    // Roof props.
    let total: f32 = p.roofs.iter().map(|r| r.weight.max(0.0)).sum();
    let mut used = vec![false; towers.len()];
    for (i, b) in towers.iter().enumerate() {
        let (pick, turn) = (rnd(), rnd());
        if total <= 0.0 || pick > p.roof_chance || has_steel(b) {
            continue;
        }
        let mut left = rnd() * total;
        let Some(r) = p.roofs.iter().find(|r| {
            left -= r.weight.max(0.0);
            left < 0.0
        }) else {
            continue;
        };
        let roof = Vec3::new(b.centre.x, b.top(), b.centre.z);
        if !clear(&all, roof, r.reach, roof.y + r.height, half, tall, margin) {
            continue;
        }
        used[i] = true;
        out.push(Placed {
            model: r.model.clone(),
            at: roof,
            yaw: b.yaw + (turn * 4.0).floor() * std::f32::consts::FRAC_PI_2,
            stretch: 1.0,
            solids: r
                .solid
                .iter()
                .map(|&(c, s)| (Vec3::from(c), Vec3::from(s)))
                .collect(),
            motion: Motion::Still,
        });
    }

    // Sky bridges between towers.
    let mut bridges = 0;
    'pairs: for (i, a) in towers.iter().enumerate() {
        for b in towers.iter().skip(i + 1) {
            if bridges >= p.bridges || p.bridge_kinds.is_empty() {
                break 'pairs;
            }
            let ra = a.size.x.min(a.size.z) / 2.0;
            let rb = b.size.x.min(b.size.z) / 2.0;
            let flat = (b.centre.xz() - a.centre.xz()).length();
            let gap = flat - ra - rb;
            if gap < p.bridge_gap.0 || gap > p.bridge_gap.1 {
                continue;
            }
            let y = a.top().min(b.top()) - p.bridge_drop;
            let dir = (b.centre.xz() - a.centre.xz()).normalize();
            let from = Vec3::new(a.centre.x + dir.x * ra, y, a.centre.z + dir.y * ra);
            let to = Vec3::new(b.centre.x - dir.x * rb, y, b.centre.z - dir.y * rb);
            let len = from.distance(to);
            let steps = (len / 5.0).ceil() as usize;
            let ok = (0..=steps).all(|k| {
                let q = from.lerp(to, k as f32 / steps as f32);
                clear(&all, q - Vec3::Y * 2.0, 4.0, q.y + 6.0, half, tall, margin)
            });
            if !ok {
                continue;
            }
            let kind = &p.bridge_kinds
                [(rnd() * p.bridge_kinds.len() as f32) as usize % p.bridge_kinds.len()];
            let yaw = (to - from).x.atan2((to - from).z) - std::f32::consts::FRAC_PI_2;
            let segs = (len / 10.0).ceil().max(1.0);
            let each = len / segs;
            for k in 0..segs as usize {
                out.push(Placed {
                    model: kind.segment.clone(),
                    at: from.lerp(to, (k as f32 + 0.5) / segs),
                    yaw,
                    stretch: each / 10.0,
                    solids: vec![],
                    motion: Motion::Still,
                });
            }
            if let Some(end) = &kind.end {
                // Each collar sits 0.75 m in from its end, at the tower wall.
                let inset = (0.75 / len).min(0.5);
                for q in [from.lerp(to, inset), to.lerp(from, inset)] {
                    out.push(Placed {
                        model: end.clone(),
                        at: q,
                        yaw,
                        stretch: 1.0,
                        solids: vec![],
                        motion: Motion::Still,
                    });
                }
            }
            // One solid deck along the whole bridge.
            out.push(Placed {
                model: String::new(),
                at: (from + to) / 2.0,
                yaw,
                stretch: 1.0,
                solids: vec![(
                    Vec3::new(0.0, -kind.deck / 2.0, 0.0),
                    Vec3::new(len, kind.deck, kind.width * 2.0),
                )],
                motion: Motion::Still,
            });
            bridges += 1;
        }
    }

    // Sky floaters beside the course.
    if !all.is_empty() {
        for f in &p.floaters {
            let mut placed = 0;
            let mut tries = 0;
            while placed < f.count && tries < f.count * 40 {
                tries += 1;
                let q = all[(rnd() * all.len() as f32) as usize % all.len()];
                let side = rnd() * std::f32::consts::TAU;
                let off = p.float_off.0 + rnd() * (p.float_off.1 - p.float_off.0);
                let up = p.float_up.0 + rnd() * (p.float_up.1 - p.float_up.0);
                let at = q + Vec3::new(side.cos() * off, up, side.sin() * off);
                let base = at - Vec3::Y * f.radius;
                if !clear(&all, base, f.radius, at.y + f.radius, half, tall, margin)
                    || out
                        .iter()
                        .any(|o| o.motion != Motion::Still && o.at.distance(at) < f.radius * 3.0)
                {
                    continue;
                }
                out.push(Placed {
                    model: f.model.clone(),
                    at,
                    yaw: rnd() * std::f32::consts::TAU,
                    stretch: 1.0,
                    solids: f
                        .solid
                        .iter()
                        .map(|&(c, s)| (Vec3::from(c), Vec3::from(s)))
                        .collect(),
                    motion: if f.spins {
                        Motion::Spin
                    } else if f.bobs {
                        Motion::Bob
                    } else {
                        Motion::Still
                    },
                });
                placed += 1;
            }
        }
    }
    let _ = used;
    // Nothing in another set piece's space.
    out.retain(|x| !claimed(x.at, 0.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> (std::sync::Arc<Tuning>, std::sync::Arc<Props>) {
        crate::features::surf_map::load_course_for_tests();
        for path in [crate::rules::surf::TUNING.path(), PROPS.path()] {
            let full = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), path);
            let Ok(bytes) = std::fs::read(&full) else {
                panic!("no {full}")
            };
            let ok = if path == PROPS.path() {
                PROPS.set_bytes(&bytes)
            } else {
                crate::rules::surf::TUNING.set_bytes(&bytes)
            };
            assert!(ok, "{full} doesn't parse");
        }
        (tuning(), PROPS.get())
    }

    #[test]
    fn something_stands_beside_a_ramp_but_not_on_it() {
        let ridge = [Vec3::new(0.0, 0.0, 0.0)];
        // Right on the ramp: no.
        assert!(!clear(
            &ridge,
            Vec3::new(0.0, -5.0, 0.0),
            2.0,
            3.0,
            6.0,
            10.0,
            2.0
        ));
        // Far to the side: yes.
        assert!(clear(
            &ridge,
            Vec3::new(30.0, -5.0, 0.0),
            2.0,
            3.0,
            6.0,
            10.0,
            2.0
        ));
        // Well under the ramp's body: yes.
        assert!(clear(
            &ridge,
            Vec3::new(0.0, -40.0, 0.0),
            2.0,
            -20.0,
            6.0,
            10.0,
            2.0
        ));
    }

    #[test]
    fn the_course_gets_arches_rooftops_bridges_and_floaters() {
        let (t, p) = load();
        let all = layout(&t, &p);
        let count = |f: &dyn Fn(&Placed) -> bool| all.iter().filter(|x| f(x)).count();
        let arches = count(&|x| p.arches.iter().any(|a| a.0 == x.model));
        let roofs = count(&|x| p.roofs.iter().any(|r| r.model == x.model));
        let bridges = count(&|x| x.model.is_empty());
        let floaters = count(&|x| x.motion != Motion::Still);
        assert!(arches >= 8, "{arches} arches");
        assert!(roofs >= 10, "{roofs} roof props");
        assert!(bridges >= 2, "{bridges} sky bridges");
        assert!(floaters >= 6, "{floaters} floaters");
        let signs = count(&|x| x.model == p.speed_sign.0 && x.motion == Motion::Still);
        assert_eq!(signs, t.speed_gaps.len(), "a sign over every speed gap");
        // Same seed, same layout.
        assert_eq!(all, layout(&t, &p));
    }

    #[test]
    fn nothing_but_arches_gets_in_a_ramps_way() {
        let (t, p) = load();
        let ridges: Vec<Vec3> = t
            .stages
            .iter()
            .flat_map(|s| s.ramps.iter().flat_map(|r| ridge(r, t.ramp_samples * 4)))
            .collect();
        for x in layout(&t, &p) {
            if p.arches.iter().any(|a| a.0 == x.model) || x.model == "surf_finish_gantry.gltf" {
                continue;
            }
            let near = ridges.iter().any(|q| {
                Vec2::new(q.x - x.at.x, q.z - x.at.z).length() < t.ramp_half_width
                    && (x.at.y - q.y).abs() < t.ramp_height
            });
            assert!(!near, "{} at {} sits on a ramp", x.model, x.at);
        }
    }
}
