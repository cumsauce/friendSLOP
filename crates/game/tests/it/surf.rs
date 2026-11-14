//! The surf map (#306): with the hall's air control, strafing into a ramp's
//! 59 degree face keeps you gliding along it instead of sliding off.

use avian3d::character_controller::move_and_slide::MoveAndSlide;
use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use friendslop::kcc::{Air, Kcc};

fn world() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        AssetPlugin::default(),
        PhysicsPlugins::default(),
    ));
    app.init_asset::<Mesh>();
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
    app.finish();
    app.cleanup();
    app
}

#[derive(Resource)]
struct Ride {
    kcc: Kcc,
    feet: Vec3,
    air: Air,
}

/// Ride for `ticks` at 60 Hz, facing -z, holding strafe right (into the face).
fn ride(air: Air, ticks: usize) -> Vec3 {
    let mut app = world();
    // Just off the left face, a third of the way in.
    app.insert_resource(Ride {
        kcc: Kcc {
            velocity: Vec3::new(0.0, 0.0, -8.0),
            grounded: None,
            ..Default::default()
        },
        feet: Vec3::new(-3.6, -10.3, -20.0),
        air,
    });
    for _ in 0..ticks {
        app.world_mut()
            .run_system_once(|move_and_slide: MoveAndSlide, mut r: ResMut<Ride>| {
                let (feet, air) = (r.feet, r.air);
                r.feet = friendslop::kcc::step_with(
                    &mut r.kcc,
                    feet,
                    0.0,
                    Vec2::X,
                    false,
                    friendslop::kcc::WALK_SPEED,
                    1.0 / 60.0,
                    &move_and_slide,
                    air,
                    &friendslop::kcc::Jump::NORMAL,
                );
            })
            .expect("step");
    }
    app.world().resource::<Ride>().feet
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
