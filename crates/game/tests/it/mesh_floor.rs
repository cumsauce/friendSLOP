//! Headless: a map's mesh collider (`<map>.collision`, a carve with
//! `collider: "mesh"` in the modeler) is a smooth floor. A walker going down
//! a ramp made of it follows the slope with no ledges: what `Block` boxes
//! couldn't do on curved, sloping tunnels.

use avian3d::prelude::*;
use bevy::prelude::*;
use friendslop_world::collision::CollisionMesh;

use super::walking::{app, feet, settle, spawn, tick};

/// A 20 m wide ramp falling 2 m over 10 m toward -z, with a flat landing
/// at each end.
fn ramp() -> CollisionMesh {
    let v = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
    CollisionMesh {
        verts: vec![
            v(-10.0, 0.0, 4.0),
            v(10.0, 0.0, 4.0),
            v(-10.0, 0.0, 0.0),
            v(10.0, 0.0, 0.0),
            v(-10.0, -2.0, -10.0),
            v(10.0, -2.0, -10.0),
            v(-10.0, -2.0, -16.0),
            v(10.0, -2.0, -16.0),
        ],
        // Counter-clockwise from above: facing up.
        tris: vec![
            [0, 1, 3],
            [0, 3, 2],
            [2, 3, 5],
            [2, 5, 4],
            [4, 5, 7],
            [4, 7, 6],
        ],
    }
}

#[test]
fn walking_down_a_mesh_ramp_follows_the_slope_without_ledges() {
    let mesh = ramp();
    let mut app = app();
    let Some(collider) = mesh.collider() else {
        panic!("a collider");
    };
    app.world_mut()
        .spawn((Transform::IDENTITY, RigidBody::Static, collider));
    settle(&mut app);
    let w = spawn(&mut app, Vec3::new(0.0, 0.2, 2.0));
    // Settle onto the landing.
    for _ in 0..30 {
        tick(&mut app, w, |w, _| w.stick = Vec2::ZERO);
    }
    let start = feet(&app, w);
    assert!(start.y.abs() < 0.1, "standing on the landing: {start}");
    let mut last = start;
    let mut biggest_drop: f32 = 0.0;
    for _ in 0..360 {
        let at = tick(&mut app, w, |w, _| {
            w.stick = Vec2::Y;
            w.speed = 3.0;
        });
        biggest_drop = biggest_drop.max(last.y - at.y);
        // On the ramp, the feet stay on the slope (within a few cm).
        if (-9.0..-1.0).contains(&at.z)
            && let Some(floor) = mesh.floor_at(at.x, at.z, at.y + 0.5)
        {
            assert!((at.y - floor).abs() < 0.08, "on the slope: {at} vs {floor}");
        }
        last = at;
    }
    assert!(
        last.z < -11.0 && (last.y + 2.0).abs() < 0.1,
        "walked all the way down: {last}"
    );
    assert!(
        biggest_drop < 0.1,
        "no ledges: dropped {biggest_drop} in one tick"
    );
}
