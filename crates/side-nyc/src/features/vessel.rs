//! The Vessel: Hudson Yards' honeycomb of copper stairs, on the plaza west
//! of the bodega, and you can climb it all the way up.
//!
//! Built to the real one's numbers: 16 levels of 5 landings (80 landings),
//! 154 flights, about 46 m tall and 46 m across the top. The foot is wider
//! than the real 15 m (about 22 m) so its flights stay walkable.
//! Each landing has flights up to the two landings on the level above, half a
//! step around to either side, so the flights make the honeycomb's diamonds.
//! As on the real one, every landing has an inner and an outer lane: flights
//! going round one way use the inner lane and the other way the outer, so the
//! flight up on one side never sits over the flight down on that side (one
//! lane each, nobody hits their head). A walkway rings the top, outside the
//! lanes.
//!
//! The model (`models/prefabs/nyc-vessel.gltf`, placed on the city map) is
//! these same boxes: [`pieces`] is both its geometry (`scripts/vessel-model.py`
//! turns them into the `nyc-vessel` modeler source) and the colliders made
//! here. Where it stands is the map's `vessel` spot.

use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;

use crate::AppState;

/// Landings per level.
pub const AROUND: usize = 5;
/// Levels, the ground one included.
pub const LEVELS: usize = 16;
/// Height between levels (m): 15 of them make the real 46 m.
pub const RISE: f32 = 3.05;
/// Radius of the bottom and top rings of landing centres (m).
pub const RADIUS: (f32, f32) = (9.0, 18.2);
/// One lane's width (m): a flight is one lane wide.
pub const WIDTH: f32 = 1.8;
/// Space between a landing's two lanes (m). Wide, because the flare brings
/// one level's outer lane in under the next level's inner one.
const GAP: f32 = 0.7;
/// How far each lane sits from its landing's centre, in or out (m).
pub const LANE: f32 = (WIDTH + GAP) / 2.0;
/// Slab thickness and parapet height (m).
const THICK: f32 = 0.35;
const PARAPET: f32 = 1.1;
const RAIL: f32 = 0.14;
/// A landing's length along the way round (m). Flights stop at its edges.
const LANDING: f32 = 1.2;
/// The top walkway's width, its gap from the outer lane (m) and how many
/// straight pieces make each arc.
const RING: f32 = 2.0;
const RING_GAP: f32 = 0.3;
const ARC: usize = 4;

pub fn plugin(app: &mut App) {
    crate::area::on_sim(app, build);
    crate::features::start::provide(starts);
}

/// A level's ring radius: the Vessel flares out towards the top.
pub fn radius(level: usize) -> f32 {
    let t = level as f32 / (LEVELS - 1) as f32;
    RADIUS.0 + (RADIUS.1 - RADIUS.0) * t.powf(1.15)
}

/// The angle round the Vessel of landing `k` on `level`. Odd levels sit half
/// a step around, which makes the honeycomb.
fn angle(level: usize, k: usize) -> f32 {
    let step = std::f32::consts::TAU / AROUND as f32;
    (k as f32 + 0.5 * (level % 2) as f32) * step
}

/// Landing `k` of `level`'s centre, relative to the Vessel's foot.
pub fn landing(level: usize, k: usize) -> Vec3 {
    let (a, r) = (angle(level, k), radius(level));
    Vec3::new(r * a.cos(), level as f32 * RISE, r * a.sin())
}

/// Straight out from the Vessel's middle at `p`.
fn outward(p: Vec3) -> Vec3 {
    Vec3::new(p.x, 0.0, p.z).normalize_or(Vec3::X)
}

/// The top walkway's centre radius: just outside the top landings' outer lane.
fn ring_radius() -> f32 {
    radius(LEVELS - 1) + LANE + WIDTH / 2.0 + RING_GAP + RING / 2.0
}

/// How far out the Vessel reaches, the top walkway's parapet included (m).
pub fn reach() -> f32 {
    ring_radius() + RING / 2.0 + RAIL
}

/// One flight of stairs: which landings it joins and where its ramp starts
/// and ends, at the edges of those landings (a ramp running on to the
/// landing's middle would pass under the slab, and you'd walk into its edge).
#[derive(Clone, Copy, Debug)]
pub struct Flight {
    pub from: (usize, usize),
    pub to: (usize, usize),
    pub a: Vec3,
    pub b: Vec3,
    /// Where you stand on its lane in the middle of each landing.
    pub stops: (Vec3, Vec3),
}

/// Every flight. Going up anticlockwise (seen from above, +angle) takes the
/// inner lane, clockwise the outer one.
pub fn flights() -> Vec<Flight> {
    let mut out = Vec::new();
    for level in 0..LEVELS - 1 {
        let odd = level % 2;
        for k in 0..AROUND {
            let p = landing(level, k);
            for (up, lane) in [
                ((k + odd) % AROUND, -LANE),
                ((k + odd + AROUND - 1) % AROUND, LANE),
            ] {
                let q = landing(level + 1, up);
                let (a, b) = (p + outward(p) * lane, q + outward(q) * lane);
                let flat = (b - a).with_y(0.0).normalize() * LANDING / 2.0;
                out.push(Flight {
                    from: (level, k),
                    to: (level + 1, up),
                    a: a + flat,
                    b: b - flat,
                    stops: (a, b),
                });
            }
        }
    }
    out
}

/// The top walkway: an arc of straight pieces from each top landing to the
/// next, outside the flights coming up.
pub fn ring() -> Vec<(Vec3, Vec3)> {
    let top = LEVELS - 1;
    let (r, y) = (ring_radius(), top as f32 * RISE);
    let step = std::f32::consts::TAU / AROUND as f32;
    let at = |a: f32| Vec3::new(r * a.cos(), y, r * a.sin());
    let mut out = Vec::new();
    for k in 0..AROUND {
        let a0 = angle(top, k);
        for s in 0..ARC {
            let (t0, t1) = (s as f32 / ARC as f32, (s + 1) as f32 / ARC as f32);
            out.push((at(a0 + t0 * step), at(a0 + t1 * step)));
        }
    }
    out
}

/// Which of the model's three materials a box is drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// A flight's stepped top.
    Steps,
    /// The copper parapets.
    Copper,
    /// Landings and the top walkway.
    Deck,
}

/// A box of the Vessel, relative to its foot.
#[derive(Clone, Copy, Debug)]
pub struct Piece {
    pub part: Part,
    pub transform: Transform,
    pub size: Vec3,
}

/// A box from its centre, its axes and its size along them.
fn block(part: Part, centre: Vec3, side: Vec3, up: Vec3, along: Vec3, size: Vec3) -> Piece {
    let rotation = Quat::from_mat3(&Mat3::from_cols(side, up, along));
    Piece {
        part,
        transform: Transform::from_translation(centre).with_rotation(rotation),
        size,
    }
}

/// A walkway from `a` to `b` (its top surface through both), `width` wide,
/// with parapets on the sides `rails` says (`-1` left, `1` right), each
/// stopping `gap` short of both ends.
fn walk(
    out: &mut Vec<Piece>,
    part: Part,
    (a, b): (Vec3, Vec3),
    width: f32,
    rails: &[f32],
    gap: f32,
) {
    let d = b - a;
    let len = d.length();
    let along = d / len;
    let side = Vec3::Y.cross(along).normalize();
    let up = along.cross(side);
    let mid = (a + b) / 2.0;
    out.push(block(
        part,
        mid - up * THICK / 2.0,
        side,
        up,
        along,
        Vec3::new(width, THICK, len),
    ));
    // A parapet leans back with the slope, so its top would hang over the
    // landing at the bottom: start it far enough up that the top clears.
    let low = gap + (PARAPET + 0.2) * along.y.max(0.0);
    let rail = len - gap - low;
    if rail > 0.4 {
        let centre = mid + along * (low - gap) / 2.0;
        for s in rails {
            out.push(block(
                Part::Copper,
                centre + side * *s * (width / 2.0 + RAIL / 2.0) + up * (PARAPET / 2.0 - 0.2),
                side,
                up,
                along,
                Vec3::new(RAIL, PARAPET + 0.4, rail),
            ));
        }
    }
}

/// Every box of the Vessel, relative to its foot: the model and the colliders.
pub fn pieces() -> Vec<Piece> {
    let mut out = Vec::new();
    // Parapets stop short of the landings so you can turn onto the next flight.
    for f in flights() {
        walk(&mut out, Part::Steps, (f.a, f.b), WIDTH, &[-1.0, 1.0], 0.3);
    }
    // The top walkway keeps its outer parapet all the way round and opens
    // on the inside only where it meets a landing.
    let ring = ring();
    for (i, seg) in ring.iter().enumerate() {
        let mid = (seg.0 + seg.1) / 2.0;
        let side = Vec3::Y.cross((seg.1 - seg.0).normalize());
        let out_side = side.dot(outward(mid)).signum();
        let s = i % ARC;
        let rails: &[f32] = if s == 0 || s == ARC - 1 {
            &[out_side]
        } else {
            &[out_side, -out_side]
        };
        walk(&mut out, Part::Deck, *seg, RING, rails, -0.05);
    }
    // Landings span both lanes; the top ones reach out to the walkway.
    let inner = LANE + WIDTH / 2.0 + 0.1;
    for level in 1..LEVELS {
        let outer = if level == LEVELS - 1 {
            ring_radius() - radius(level) + RING / 2.0
        } else {
            inner
        };
        for k in 0..AROUND {
            let p = landing(level, k);
            let out_dir = outward(p);
            let tangent = out_dir.cross(Vec3::Y);
            out.push(block(
                Part::Deck,
                p + out_dir * (outer - inner) / 2.0 - Vec3::Y * THICK / 2.0,
                out_dir,
                Vec3::Y,
                tangent,
                Vec3::new(inner + outer, THICK, LANDING),
            ));
        }
    }
    out
}

/// Where the map puts the Vessel's foot.
pub fn foot(map: &friendslop_world::places::MapFile) -> Option<Vec3> {
    let e = map.entities.iter().find(|e| e.name == "vessel")?;
    Some(Vec3::from_array(e.translation))
}

fn build(mut commands: Commands) {
    let Some(at) = foot(&super::nyc::MAP.get()) else {
        return;
    };
    for p in pieces() {
        let s = p.size;
        commands.spawn((
            Transform::from_translation(at + p.transform.translation)
                .with_rotation(p.transform.rotation),
            RigidBody::Static,
            Collider::cuboid(s.x, s.y, s.z),
            crate::area::sim(),
        ));
    }
}

/// `FRIENDSLOP_START=vessel` starts you at its foot, `vessel_top` up top.
fn starts() -> Vec<(String, Vec3)> {
    let Some(at) = foot(&super::nyc::MAP.get()) else {
        return Vec::new();
    };
    let foot0 = landing(0, 2);
    let base = at + foot0 + outward(foot0) * (LANE + WIDTH + 1.0) + Vec3::Y * 0.4;
    let top = at + landing(LEVELS - 1, 0) + Vec3::Y * 0.05;
    vec![("vessel".into(), base), ("vessel_top".into(), top)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flight_is_climbable() {
        for f in flights() {
            let run = Vec2::new(f.b.x - f.a.x, f.b.z - f.a.z).length();
            let slope = (f.b.y - f.a.y).atan2(run).to_degrees();
            assert!(slope < 40.0, "a {slope} degree flight {f:?}");
        }
    }

    #[test]
    fn it_has_the_real_ones_landings_flights_and_height() {
        let f = flights();
        assert_eq!(LEVELS * AROUND, 80);
        // 150 flights between levels, plus the top walkway's 5 arcs: 155,
        // the real one's 154 near enough.
        assert_eq!(f.len() + AROUND, 155);
        for level in 0..LEVELS - 1 {
            for k in 0..AROUND {
                let ups = f.iter().filter(|f| f.from == (level, k)).count();
                assert_eq!(ups, 2, "landing {level}/{k}");
            }
        }
        let top = landing(LEVELS - 1, 0);
        assert!((top.y - 45.75).abs() < 1e-3);
        assert!(
            (reach() * 2.0 - 46.0).abs() < 1.5,
            "{} m across",
            reach() * 2.0
        );
        // Wider at the foot than the real 15 m, so the bottom flights stay walkable.
        assert!(radius(0) * 2.0 < 20.0);
    }

    /// Points every 25 cm along every walkway's middle.
    fn walkway_samples() -> Vec<Vec3> {
        let mut ends: Vec<(Vec3, Vec3)> = flights().iter().map(|f| (f.a, f.b)).collect();
        ends.extend(ring());
        let mut out = Vec::new();
        for (a, b) in ends {
            let n = ((b - a).length() / 0.25).ceil() as usize;
            out.extend((0..=n).map(|i| a.lerp(b, i as f32 / n as f32)));
        }
        out
    }

    #[test]
    fn no_walkway_runs_under_another() {
        // Two walkways overlap where a walker on one could be under the
        // other or its parapet. There one must be either joined to the other
        // (no steeper than 45 degrees between them) or clear over your head:
        // a 1.6 m walker plus the slab's thickness on a 40 degree slope.
        let pts = walkway_samples();
        for (i, p) in pts.iter().enumerate() {
            for q in &pts[i + 1..] {
                let flat = Vec2::new(p.x - q.x, p.z - q.z).length();
                let dy = (p.y - q.y).abs();
                if flat < WIDTH + RAIL && dy > flat + 0.15 && dy < 1.6 + THICK / 0.766 {
                    panic!("{p} is {dy} m over/under {q}");
                }
            }
        }
    }

    #[test]
    fn every_flight_starts_and_ends_on_a_landing() {
        let slabs: Vec<Piece> = pieces()
            .into_iter()
            .filter(|p| p.part == Part::Deck && p.size.z == LANDING)
            .collect();
        let on = |x: Vec3| {
            slabs.iter().any(|s| {
                let local = s.transform.rotation.inverse() * (x - s.transform.translation);
                local.x.abs() <= s.size.x / 2.0
                    && local.z.abs() <= s.size.z / 2.0
                    && (local.y - s.size.y / 2.0).abs() < 1e-3
            })
        };
        // Just past each end, towards the landing's middle.
        for f in flights() {
            let flat = (f.b - f.a).with_y(0.0).normalize() * 0.1;
            assert!(
                f.from.0 == 0 || on(f.a - flat),
                "{f:?} starts off a landing"
            );
            assert!(on(f.b + flat), "{f:?} ends off a landing");
        }
    }

    #[test]
    fn the_map_puts_the_vessel_in_the_city() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/maps/nyc.map.json"
        );
        let Ok(text) = std::fs::read_to_string(path) else {
            panic!("no {path}");
        };
        let Ok(map) = serde_json::from_str(&text) else {
            panic!("{path} doesn't parse");
        };
        let Some(at) = foot(&map) else {
            panic!("no vessel spot on the city map");
        };
        // Its whole top stands over the city floor (outside it is the rim's terrain).
        let r = reach();
        for (dx, dz) in [(r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)] {
            assert!(
                crate::rules::nyc::in_city(at.x + dx, at.z + dz),
                "the Vessel at {at} pokes out of the city"
            );
        }
    }

    /// Writes every box as JSON for `scripts/vessel-model.py`:
    /// `VESSEL_BOXES=out.json cargo test -p friendslop-side-nyc vessel -- --ignored`.
    #[test]
    #[ignore]
    fn write_model_boxes() {
        let Ok(path) = std::env::var("VESSEL_BOXES") else {
            return;
        };
        let boxes: Vec<serde_json::Value> = pieces()
            .iter()
            .map(|p| {
                serde_json::json!({
                    "part": format!("{:?}", p.part),
                    "at": p.transform.translation.to_array(),
                    "rotation": p.transform.rotation.to_array(),
                    "size": p.size.to_array(),
                })
            })
            .collect();
        let Ok(text) = serde_json::to_string(&boxes) else {
            panic!("boxes don't serialise");
        };
        if std::fs::write(&path, text).is_err() {
            panic!("can't write {path}");
        }
    }
}
