//! The Vessel: Hudson Yards' honeycomb of copper stairs, on the plaza west
//! of the bodega, and you can climb it all the way up.
//!
//! It is 18 levels of 8 landings. Each landing has flights up to the two
//! landings on the level above, half a step around to either side, so the
//! flights cross into the honeycomb. The levels widen from 15 m across at
//! the bottom to 46 m at the top (about 45 m up), where a ring walkway joins
//! the top landings. The model (`models/prefabs/nyc-vessel.gltf`, placed on
//! the city map) is built from these same numbers. The colliders are made
//! here: a ramp under every flight, a parapet on each side, and a slab at
//! each landing. Where it stands is the map's `vessel` spot.

use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;

use crate::AppState;

/// Landings per level.
pub const AROUND: usize = 8;
/// Levels, the ground one included.
pub const LEVELS: usize = 18;
/// Height between levels (m).
pub const RISE: f32 = 2.5;
/// Radius of the bottom and top rings of landings (m).
pub const RADIUS: (f32, f32) = (7.5, 23.0);
/// Flight width, slab thickness, parapet height and landing size (m).
const WIDTH: f32 = 1.6;
const THICK: f32 = 0.35;
const PARAPET: f32 = 1.1;
const LANDING: f32 = 2.0;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), build);
}

/// A level's ring radius: the Vessel flares out towards the top.
pub fn radius(level: usize) -> f32 {
    let t = level as f32 / (LEVELS - 1) as f32;
    RADIUS.0 + (RADIUS.1 - RADIUS.0) * t.powf(1.15)
}

/// Landing `k` of `level`, relative to the Vessel's foot. Odd levels sit half
/// a step around, which makes the honeycomb.
pub fn landing(level: usize, k: usize) -> Vec3 {
    let step = std::f32::consts::TAU / AROUND as f32;
    let a = (k as f32 + 0.5 * (level % 2) as f32) * step;
    let r = radius(level);
    Vec3::new(r * a.cos(), level as f32 * RISE, r * a.sin())
}

/// Every flight's bottom and top, plus the top ring's walkways.
pub fn flights() -> Vec<(Vec3, Vec3)> {
    let mut out = Vec::new();
    for level in 0..LEVELS - 1 {
        let odd = level % 2;
        for k in 0..AROUND {
            let a = landing(level, k);
            for up in [k + odd, (k + odd + AROUND - 1) % AROUND] {
                out.push((a, landing(level + 1, up % AROUND)));
            }
        }
    }
    let top = LEVELS - 1;
    for k in 0..AROUND {
        out.push((landing(top, k), landing(top, (k + 1) % AROUND)));
    }
    out
}

/// A box from its centre, its axes and its size along them.
fn block(centre: Vec3, side: Vec3, up: Vec3, along: Vec3, size: Vec3) -> (Transform, Collider) {
    let rotation = Quat::from_mat3(&Mat3::from_cols(side, up, along));
    (
        Transform::from_translation(centre).with_rotation(rotation),
        Collider::cuboid(size.x, size.y, size.z),
    )
}

/// The Vessel's colliders, relative to its foot.
pub fn colliders() -> Vec<(Transform, Collider)> {
    let mut out = Vec::new();
    for (a, b) in flights() {
        let d = b - a;
        let len = d.length();
        let along = d / len;
        let side = Vec3::Y.cross(along).normalize();
        let up = along.cross(side);
        let mid = (a + b) / 2.0;
        out.push(block(
            mid - up * THICK / 2.0,
            side,
            up,
            along,
            Vec3::new(WIDTH, THICK, len),
        ));
        // Parapets stop short of the landings so you can turn onto the next flight.
        let rail = len - LANDING - 0.2;
        if rail > 0.4 {
            for s in [-1.0, 1.0] {
                out.push(block(
                    mid + side * s * (WIDTH / 2.0 + 0.07) + up * (PARAPET / 2.0 - 0.2),
                    side,
                    up,
                    along,
                    Vec3::new(0.14, PARAPET + 0.4, rail),
                ));
            }
        }
    }
    for level in 1..LEVELS {
        for k in 0..AROUND {
            let p = landing(level, k);
            let out_dir = Vec3::new(p.x, 0.0, p.z).normalize();
            let tangent = Vec3::Y.cross(out_dir);
            out.push(block(
                p - Vec3::Y * THICK / 2.0,
                out_dir,
                Vec3::Y,
                tangent,
                Vec3::new(LANDING, THICK, LANDING),
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
    for (t, collider) in colliders() {
        commands.spawn((
            Transform::from_translation(at + t.translation).with_rotation(t.rotation),
            RigidBody::Static,
            collider,
            DespawnOnExit(AppState::InGame),
        ));
    }
    // `FRIENDSLOP_START=vessel` starts you at its foot, `vessel_top` up top.
    let base = at + landing(0, 2) + Vec3::new(0.0, 0.4, 2.0);
    let top = at + landing(LEVELS - 1, 0) + Vec3::Y * 0.05;
    commands.queue(move |world: &mut World| {
        if let Some(mut p) = world.get_resource_mut::<crate::features::start::StartPoints>() {
            p.add("vessel", base);
            p.add("vessel_top", top);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flight_is_climbable() {
        for (a, b) in flights() {
            let run = Vec2::new(b.x - a.x, b.z - a.z).length();
            let slope = (b.y - a.y).atan2(run).to_degrees();
            assert!(slope < 42.0, "a {slope} degree flight from {a} to {b}");
        }
    }

    #[test]
    fn every_landing_goes_up_two_ways_and_the_top_is_a_ring() {
        let f = flights();
        assert_eq!(f.len(), (LEVELS - 1) * AROUND * 2 + AROUND);
        for level in 0..LEVELS - 1 {
            for k in 0..AROUND {
                let p = landing(level, k);
                let ups = f.iter().filter(|(a, b)| *a == p && b.y > a.y).count();
                assert_eq!(ups, 2, "landing {level}/{k}");
            }
        }
        let top = landing(LEVELS - 1, 0);
        assert!((top.y - 42.5).abs() < 1e-3);
        assert!(Vec2::new(top.x, top.z).length() > 22.0);
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
        // Its whole top ring stands over the city floor (outside it is the rim's terrain).
        let r = radius(LEVELS - 1);
        for (dx, dz) in [(r, 0.0), (-r, 0.0), (0.0, r), (0.0, -r)] {
            assert!(
                crate::rules::nyc::in_city(at.x + dx, at.z + dz),
                "the Vessel at {at} pokes out of the city"
            );
        }
    }
}
