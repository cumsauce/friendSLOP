//! The surf map (#306): with the hall's air control, strafing into a ramp's
//! 59 degree face keeps you gliding along it instead of sliding off.

use avian3d::prelude::*;
use bevy::prelude::*;
use friendslop::kcc::Air;

use super::walking::{self, spawn, tick};

fn world() -> App {
    let mut app = walking::app();
    // A straight ramp from (0, -6, -16) to (0, -14, -76), 6 m to each side
    // and 10 m down: the shape every piece of the course is cut from.
    let points = [(0.0, -6.0, -16.0), (0.0, -14.0, -76.0)];
    let [piece] = friendslop_side::features::surf::pieces(&points, 1, 6.0, 10.0)[..] else {
        panic!("one piece");
    };
    let Some(hull) = Collider::convex_hull(&piece) else {
        panic!("ramp hull");
    };
    app.world_mut()
        .spawn((RigidBody::Static, hull, Transform::default()));
    walking::settle(&mut app);
    app
}

/// Ride for `ticks` at 60 Hz, facing -z, holding strafe right (into the face).
fn ride(air: Air, ticks: usize) -> Vec3 {
    let mut app = world();
    // Just off the left face, a third of the way in.
    let w = spawn(&mut app, Vec3::new(-3.6, -10.3, -20.0));
    let mut at = Vec3::ZERO;
    for i in 0..ticks {
        at = tick(&mut app, w, |w, k| {
            w.yaw = 0.0;
            w.stick = Vec2::X;
            w.air = air;
            if i == 0 {
                w.feet = Vec3::new(-3.6, -10.3, -20.0);
                k.velocity = Vec3::new(0.0, 0.0, -8.0);
                k.grounded = None;
            }
        });
    }
    at
}

#[test]
fn strafing_into_a_ramp_surfs_along_it() {
    let surf = Air {
        accel_hz: 120.0,
        max_wish_speed: 0.76,
        max_speed: 45.0,
    };
    let feet = ride(surf, 90);
    // Still on the left face, well along the ramp.
    assert!(feet.z < -30.0, "{feet}");
    assert!((-6.5..0.0).contains(&feet.x), "{feet}");
}
