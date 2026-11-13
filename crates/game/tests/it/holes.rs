//! Holes in the ground: the kinematic walker goes down Fulton St's entrance
//! stairs through the hole in New York's street, past where the street's
//! slab used to be solid, and back up again.

use avian3d::character_controller::move_and_slide::MoveAndSlide;
use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;
use friendslop::kcc::Kcc;
use friendslop_side::features::{fulton, nyc};

fn tuning() -> fulton::Tuning {
    let d = &fulton::TUNING;
    let path = format!("{}/assets/{}", env!("CARGO_MANIFEST_DIR"), d.path());
    let Ok(bytes) = std::fs::read(&path) else {
        panic!("no {path}");
    };
    assert!(d.set_bytes(&bytes), "{path} doesn't parse");
    (*d.get()).clone()
}

fn world(t: &fulton::Tuning) -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        AssetPlugin::default(),
        PhysicsPlugins::default(),
    ));
    app.init_asset::<Mesh>();
    friendslop_world::holes::cut(fulton::entrance_hole(t));
    for (at, c) in nyc::city_floor() {
        app.world_mut().spawn((RigidBody::Static, c, at));
    }
    for b in fulton::entrance_colliders(t) {
        app.world_mut().spawn((
            RigidBody::Static,
            Collider::cuboid(b.size.x, b.size.y, b.size.z),
            Transform::from_translation(b.centre).with_rotation(b.rotation),
        ));
    }
    app.finish();
    app.cleanup();
    for _ in 0..3 {
        app.update();
    }
    app
}

#[derive(Resource)]
struct Walk {
    kcc: Kcc,
    feet: Vec3,
    yaw: f32,
}

/// Walk facing `yaw` for `ticks` at 60 Hz; the lowest feet on the way, and the end.
fn walk(app: &mut App, kcc: &mut Kcc, feet: Vec3, yaw: f32, ticks: usize) -> (Vec3, f32) {
    app.insert_resource(Walk {
        kcc: *kcc,
        feet,
        yaw,
    });
    let mut lowest = feet.y;
    for _ in 0..ticks {
        let Ok(y) = app
            .world_mut()
            .run_system_once(|mas: MoveAndSlide, mut w: ResMut<Walk>| {
                let (feet, yaw) = (w.feet, w.yaw);
                w.feet = friendslop::kcc::step(
                    &mut w.kcc,
                    feet,
                    yaw,
                    Vec2::Y,
                    false,
                    friendslop::kcc::WALK_SPEED,
                    1.0 / 60.0,
                    &mas,
                );
                w.feet.y
            })
        else {
            panic!("step");
        };
        lowest = lowest.min(y);
    }
    let Some(w) = app.world_mut().remove_resource::<Walk>() else {
        panic!("walk");
    };
    *kcc = w.kcc;
    (w.feet, lowest)
}

#[test]
fn walks_down_the_entrance_stairs_and_back_up() {
    let t = tuning();
    let mut app = world(&t);
    let mut kcc = Kcc::default();
    // On the sidewalk in front of the entrance, walking into it (the open
    // end faces the entrance's yaw).
    let start = fulton::street_at(&t) + Vec3::Y * 0.05;
    let into = t.entrance_yaw.to_radians();
    let (feet, lowest) = walk(&mut app, &mut kcc, start, into, 200);
    assert!(
        lowest < -2.8,
        "never got down the stairs: lowest {lowest}, at {feet}"
    );
    // At the foot of the stairs, on the landing: the server takes you in from here.
    assert!(feet.y > -3.3, "fell through the landing: {feet}");
    assert!(fulton::entering(&t, feet), "not in the way in: {feet}");
    // Turn round and climb back up to the street.
    let (back, _) = walk(&mut app, &mut kcc, feet, into + std::f32::consts::PI, 200);
    assert!(back.y > -0.1 && back.y < 0.3, "didn't get back up: {back}");
    assert!(!fulton::entrance_hole(&t).contains(back.xz()));
    // And the street next to the entrance is still solid.
    let (beside, low) = walk(
        &mut app,
        &mut Kcc::default(),
        fulton::entrance_at(&t) + Vec3::new(0.0, 0.05, 3.0),
        -std::f32::consts::FRAC_PI_2,
        30,
    );
    assert!(low > -0.1, "fell through the street beside it: {beside}");
}
