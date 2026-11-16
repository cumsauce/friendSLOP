//! New York's street dressing: the sidewalks were bare between the lamps, so
//! small clutter (hydrants, trash bags at the curb, newspaper boxes,
//! mailboxes, bike racks, sign poles, planters) is scattered along every curb.
//!
//! The curbs are read off the city map: two street lamps on one curb line
//! close together have sidewalk between them, a wider gap is a crossing and
//! is left clear (and nothing goes in the street, so the cab lanes stay
//! clear). Picks are seeded, so every peer sees the same street. Look only:
//! local models in New York's visuals layer, no colliders, hidden beyond a
//! draw distance. Tuning: `assets/data/side/street_dressing.ron`.

use bevy::prelude::*;
use serde::Deserialize;

use super::street_people::{between, hash};
use crate::AppState;

/// One kind of clutter.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Kind {
    /// The model (from the modeler).
    pub model: String,
    /// How often it's picked, against the others.
    pub weight: f32,
    /// How far back from the curb line (m), towards the buildings.
    pub back: f32,
    /// Extra turn (degrees) on top of facing the street.
    pub turn: f32,
}

/// Tuning: `assets/data/side/street_dressing.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Changes every pick.
    pub seed: u32,
    /// Lamps further apart than this on one curb have a crossing between them.
    pub max_gap: f32,
    /// How many things go in each gap (min, max).
    pub per_gap: (u32, u32),
    /// Distance kept from anything the map placed (m).
    pub clearance: f32,
    /// Distance kept from a gap's end lamps (m).
    pub lamp_margin: f32,
    /// Not drawn beyond this distance from the camera (m).
    pub draw_distance: f32,
    /// What can go there.
    pub kinds: Vec<Kind>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/street_dressing.ron");

const LAMP: &str = "nyc-kit-streetlamp";

/// A curb lamp: where it stands and which way the street is (its facing).
#[derive(Clone, Copy, Debug)]
pub struct Lamp {
    pub at: Vec2,
    pub yaw: f32,
}

/// One placed thing: which kind, where (x, z) and its turn (radians).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub kind: usize,
    pub at: Vec2,
    pub yaw: f32,
}

/// Which way (x, z) a lamp turned `yaw` faces: towards the street.
fn street_dir(yaw: f32) -> Vec2 {
    Vec2::new(yaw.sin(), yaw.cos())
}

fn unit(h: u32) -> f32 {
    between((0.0, 1.0), h)
}

/// Pairs of neighbouring lamps on one curb (same facing, in line) no further than `max_gap`.
pub fn gaps(lamps: &[Lamp], max_gap: f32) -> Vec<(Lamp, Lamp)> {
    let mut out = Vec::new();
    for (i, a) in lamps.iter().enumerate() {
        let out_dir = street_dir(a.yaw);
        let along = out_dir.perp();
        // The nearest lamp ahead along this curb, facing the same way.
        let next = lamps
            .iter()
            .enumerate()
            .filter(|(j, b)| *j != i && street_dir(b.yaw).dot(out_dir) > 0.99)
            .map(|(_, b)| (b, b.at - a.at))
            .filter(|(_, d)| d.dot(out_dir).abs() < 0.5 && d.dot(along) > 0.5)
            .min_by(|x, y| x.1.dot(along).total_cmp(&y.1.dot(along)));
        if let Some((b, d)) = next
            && d.length() <= max_gap
        {
            out.push((*a, *b));
        }
    }
    out
}

/// Every piece of clutter for these lamps, kept `clearance` from `taken` points.
pub fn dress(lamps: &[Lamp], taken: &[Vec2], t: &Tuning) -> Vec<Piece> {
    let total: f32 = t.kinds.iter().map(|k| k.weight.max(0.0)).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    let (lo, hi) = (t.per_gap.0.min(t.per_gap.1), t.per_gap.0.max(t.per_gap.1));
    let mut out: Vec<Piece> = Vec::new();
    for (g, (a, b)) in gaps(lamps, t.max_gap).iter().enumerate() {
        let g = g as u32;
        let len = a.at.distance(b.at);
        let span = len - 2.0 * t.lamp_margin;
        if span <= 0.0 {
            continue;
        }
        let n = lo + hash(t.seed, g * 7 + 1) % (hi - lo + 1);
        let along = (b.at - a.at) / len;
        let out_dir = street_dir(a.yaw);
        for k in 0..n {
            let h = |s: u32| hash(t.seed ^ (g * 131 + k * 17), s);
            // Spread the slots evenly, jittered within each.
            let slot = (k as f32 + 0.2 + 0.6 * unit(h(2))) / n as f32;
            let mut pick = unit(h(3)) * total;
            let kind = t
                .kinds
                .iter()
                .position(|kd| {
                    pick -= kd.weight.max(0.0);
                    pick < 0.0
                })
                .unwrap_or(t.kinds.len() - 1);
            let Some(kd) = t.kinds.get(kind) else {
                continue;
            };
            let at = a.at + along * (t.lamp_margin + slot * span) - out_dir * kd.back;
            if taken
                .iter()
                .chain(out.iter().map(|p| &p.at))
                .any(|p| p.distance(at) < t.clearance)
            {
                continue;
            }
            out.push(Piece {
                kind,
                at,
                yaw: a.yaw + kd.turn.to_radians(),
            });
        }
    }
    out
}

/// The lamps, and the points already taken (every kit prop, every room spot).
fn read_map(map: &friendslop_world::places::MapFile) -> (Vec<Lamp>, Vec<Vec2>) {
    let mut lamps = Vec::new();
    let mut taken = Vec::new();
    for e in &map.entities {
        if e.name.starts_with("room_") {
            taken.push(e.at());
        }
        let Some(g) = &e.gltf else { continue };
        if g.contains(LAMP) {
            lamps.push(Lamp {
                at: e.at(),
                yaw: e.yaw.to_radians(),
            });
        }
        if g.contains("nyc-kit-") && !g.contains("tower") {
            taken.push(e.at());
        }
    }
    (lamps, taken)
}

/// A piece of street dressing (hidden beyond the draw distance).
#[derive(Component)]
struct Clutter;

fn spawn(mut commands: Commands, assets: Res<AssetServer>) {
    let t = TUNING.get();
    let (lamps, taken) = read_map(&super::nyc::MAP.get());
    let y = crate::rules::nyc::CITY_Y;
    for p in dress(&lamps, &taken, &t) {
        let Some(kind) = t.kinds.get(p.kind) else {
            continue;
        };
        commands.spawn((
            Clutter,
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(kind.model.clone()))),
            Transform::from_xyz(p.at.x, y, p.at.y).with_rotation(Quat::from_rotation_y(p.yaw)),
            Visibility::Hidden,
            crate::area::show(),
        ));
    }
}

/// Local: draw only the clutter near this screen's camera.
fn cull(
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    mut clutter: Query<(&Transform, &mut Visibility), With<Clutter>>,
) {
    let Some(at) = camera.iter().next().map(|g| g.translation()) else {
        return;
    };
    let far = TUNING.get().draw_distance;
    for (tf, mut v) in &mut clutter {
        let want = if tf.translation.distance_squared(at) <= far * far {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        v.set_if_neq(want);
    }
}

pub fn plugin(app: &mut App) {
    crate::area::on_show(app, spawn);
    app.add_systems(
        Update,
        cull.run_if(crate::sites::here("nyc"))
            .run_if(in_state(AppState::InGame)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.max_gap > 0.0 && t.draw_distance > 0.0 && t.clearance > 0.0);
        assert!(t.per_gap.1 >= t.per_gap.0);
        assert!(!t.kinds.is_empty());
        for k in &t.kinds {
            assert!(k.model.starts_with("models/") && k.weight >= 0.0);
        }
    }

    #[test]
    fn crossings_stay_clear_and_clutter_stays_on_the_curb() {
        let t = t();
        // One curb at x = -11.5 facing +x: lamps 18 m apart, then a 36 m crossing.
        let yaw = 90f32.to_radians();
        let lamps: Vec<Lamp> = [152.0, 170.0, 206.0]
            .iter()
            .map(|z| Lamp {
                at: Vec2::new(-11.5, *z),
                yaw,
            })
            .collect();
        assert_eq!(gaps(&lamps, t.max_gap).len(), 1);
        let pieces = dress(&lamps, &[], &t);
        assert!(!pieces.is_empty());
        for p in &pieces {
            assert!(p.at.y > 152.0 && p.at.y < 170.0, "{p:?} in the crossing");
            assert!(p.at.x <= -11.0, "{p:?} in the street");
        }
        assert_eq!(pieces, dress(&lamps, &[], &t), "the same on every peer");
    }
}
