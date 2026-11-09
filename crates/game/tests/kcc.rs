//! The kinematic walker controller (ported from bevy_ahoy) against a real
//! physics world: flat ground, a step-up, a steep slope that blocks and a
//! wall that stops you.

use avian3d::character_controller::move_and_slide::MoveAndSlide;
use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

fn world() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        AssetPlugin::default(),
        PhysicsPlugins::default(),
    ));
    // Avian tracks mesh asset events for collider caches.
    app.init_asset::<Mesh>();
    // Flat ground with its top at y = 0.
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(40.0, 1.0, 40.0),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    // A 0.4 m step to climb, 6 m north.
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(2.0, 0.4, 1.0),
        Transform::from_xyz(0.0, 0.2, -6.0),
    ));
    // A tall, steep ramp at 60 degrees: too steep to walk, 12 m north.
    let tilt = Quat::from_rotation_x(-60.0_f32.to_radians());
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(2.0, 0.4, 8.0),
        Transform::from_xyz(0.0, 1.6, -12.0).with_rotation(tilt),
    ));
    // A wall facing west, 18 m north: blocks, but you slide along it.
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(0.5, 3.0, 8.0),
        Transform::from_xyz(-4.0, 1.5, -18.0),
    ));
    app
}

/// One controller tick at walking speed, facing -Z (yaw 0).
#[derive(Resource)]
struct Tick {
    kcc: friendslop::kcc::Kcc,
    feet: Vec3,
    jump: bool,
}

fn tick(app: &mut App, kcc: &mut friendslop::kcc::Kcc, feet: Vec3, jump: bool) -> Vec3 {
    app.insert_resource(Tick {
        kcc: *kcc,
        feet,
        jump,
    });
    app.world_mut()
        .run_system_once(|move_and_slide: MoveAndSlide, mut t: ResMut<Tick>| {
            let (feet, jump) = (t.feet, t.jump);
            t.feet = friendslop::kcc::step(
                &mut t.kcc,
                feet,
                0.0,
                Vec2::Y,
                jump,
                friendslop::kcc::WALK_SPEED,
                1.0 / 60.0,
                &move_and_slide,
            );
        })
        .expect("step");
    let Tick {
        kcc: out,
        feet,
        jump: _,
    } = app.world_mut().remove_resource::<Tick>().expect("tick");
    *kcc = out;
    feet
}

/// Walk for `ticks`, then return where we ended up.
fn walk(app: &mut App, feet: Vec3, ticks: usize) -> (Vec3, friendslop::kcc::Kcc) {
    let mut kcc = friendslop::kcc::Kcc::default();
    let mut feet = feet;
    for _ in 0..ticks {
        feet = tick(app, &mut kcc, feet, false);
    }
    (feet, kcc)
}

/// Walk facing `yaw` for `ticks` (a tick system variant that turns).
#[derive(Resource)]
struct WalkYaw {
    kcc: friendslop::kcc::Kcc,
    feet: Vec3,
    yaw: f32,
}

#[test]
fn walks_steps_and_hits_walls() {
    let mut app = world();
    // Let the physics backend pick up the colliders.
    for _ in 0..3 {
        app.update();
    }

    // Stands on the flat ground without sinking.
    let (feet, kcc) = walk(&mut app, Vec3::new(0.0, 0.05, 2.0), 10);
    assert!(kcc.grounded.is_some(), "should be grounded");
    assert!(
        (feet.y - 0.0).abs() < 0.05,
        "feet settle on the ground, got {feet}"
    );

    // A 0.4 m step is stepped up and crossed, not stopped at. (Past the
    // step's far edge the walker drops the 0.4 m back to the ground, like a
    // kerb, so it is briefly airborne: only position is asserted here.)
    let (feet, _) = walk(&mut app, Vec3::new(0.0, 0.05, -2.0), 56);
    assert!(feet.z < -6.5, "walked across the step, at {feet}");
    assert!(
        feet.y > -0.05 && feet.y < 0.45,
        "never fell or flew, {feet}"
    );

    // Standing on top of the step keeps you at its height.
    let (feet, kcc) = walk(&mut app, Vec3::new(0.0, 0.45, -6.0), 10);
    assert!(kcc.grounded.is_some(), "standing on the step");
    assert!(
        (feet.y - 0.4).abs() < 0.06,
        "feet settle on the step top, got {feet}"
    );

    // The steep ramp stops you at its base: you never climb it.
    let (feet, _kcc) = walk(&mut app, Vec3::new(0.0, 0.05, -2.0), 600);
    assert!(feet.y < 1.0, "never climbed the steep ramp, got {feet}");

    // The wall blocks walking west, and you don't pass through it.
    let mut kcc = friendslop::kcc::Kcc::default();
    let mut feet = Vec3::new(0.0, 0.05, -18.0);
    let yaw = -90.0_f32.to_radians();
    for _ in 0..600 {
        app.insert_resource(WalkYaw { kcc, feet, yaw });
        app.world_mut()
            .run_system_once(|move_and_slide: MoveAndSlide, mut w: ResMut<WalkYaw>| {
                let (feet, yaw) = (w.feet, w.yaw);
                w.feet = friendslop::kcc::step(
                    &mut w.kcc,
                    feet,
                    yaw,
                    Vec2::Y,
                    false,
                    friendslop::kcc::WALK_SPEED,
                    1.0 / 60.0,
                    &move_and_slide,
                );
            })
            .expect("step");
        let WalkYaw {
            kcc: out,
            feet: out_feet,
            yaw: _,
        } = app.world_mut().remove_resource::<WalkYaw>().expect("walk");
        kcc = out;
        feet = out_feet;
    }
    assert!(feet.x > -4.0 + 0.3, "the wall stopped the walk, got {feet}");
}

#[test]
fn jumps_and_comes_back_down() {
    let mut app = world();
    for _ in 0..3 {
        app.update();
    }
    let mut kcc = friendslop::kcc::Kcc::default();
    let mut feet = Vec3::new(0.0, 0.05, 2.0);
    // Settle, then jump.
    for _ in 0..10 {
        feet = tick(&mut app, &mut kcc, feet, false);
    }
    assert!(kcc.grounded.is_some(), "settled on the ground");
    let start_y = feet.y;
    feet = tick(&mut app, &mut kcc, feet, true);
    assert!(feet.y > start_y + 0.05, "left the ground, got {feet}");
    // Come back down within a second.
    let mut landed = false;
    for _ in 0..60 {
        feet = tick(&mut app, &mut kcc, feet, false);
        if kcc.grounded.is_some() && feet.y < start_y + 0.2 {
            landed = true;
            break;
        }
    }
    assert!(landed, "never landed after a jump, at {feet}");
}

#[derive(Resource)]
struct Climber {
    kcc: friendslop::kcc::Kcc,
    feet: Vec3,
    climbed: bool,
}

/// One host tick with W held. With `space` down, climb if there's a face and
/// jump otherwise (Space is both keys, as in the game); with `space` up, walk.
fn climb_tick(app: &mut App, c: Climber, space: bool) -> Climber {
    app.insert_resource(c);
    app.world_mut()
        .run_system_once(
            move |move_and_slide: MoveAndSlide, mut c: ResMut<Climber>| {
                let dt = 1.0 / 60.0;
                let feet = c.feet;
                if let Some((feet, v)) = space
                    .then(|| {
                        friendslop::features::climb::climb_step(
                            &move_and_slide,
                            feet,
                            0.0,
                            Vec2::Y,
                            dt,
                        )
                    })
                    .flatten()
                {
                    c.feet = feet;
                    c.kcc.velocity = v;
                    c.kcc.grounded = None;
                    c.climbed = true;
                } else {
                    let mut kcc = c.kcc;
                    c.feet = friendslop::kcc::step(
                        &mut kcc,
                        feet,
                        0.0,
                        Vec2::Y,
                        space,
                        friendslop::kcc::WALK_SPEED,
                        dt,
                        &move_and_slide,
                    );
                    c.kcc = kcc;
                }
            },
        )
        .expect("climb tick");
    app.world_mut()
        .remove_resource::<Climber>()
        .expect("climber")
}

#[test]
fn climbs_a_tall_block_and_hauls_over_the_top() {
    let mut app = world();
    // A 4 m block, 30 m east of the others: far too tall to jump or step.
    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(4.0, 4.0, 4.0),
        Transform::from_xyz(12.0, 2.0, -4.0),
    ));
    for _ in 0..3 {
        app.update();
    }
    let mut c = Climber {
        kcc: friendslop::kcc::Kcc::default(),
        feet: Vec3::new(12.0, 0.0, 1.0),
        climbed: false,
    };
    // Walk up to it, climb, haul over, and stand on top.
    let mut on_top = None;
    for tick in 0..600 {
        c = climb_tick(&mut app, c, true);
        if c.climbed && c.kcc.grounded.is_some() {
            on_top = Some((tick, c.feet));
            break;
        }
    }
    let Some((tick, feet)) = on_top else {
        panic!("never landed after climbing, ended at {}", c.feet);
    };
    assert!(
        feet.y > 3.9 && feet.y < 4.2,
        "lands on top of the block, got {feet}"
    );
    assert!(
        tick < 300,
        "climbing 4 m takes a few seconds, took {tick} ticks"
    );
    // Let go of Space and walk on across the top rather than slipping back
    // down. (Still holding Space just bunnies, since Space is jump too.)
    for _ in 0..20 {
        c = climb_tick(&mut app, c, false);
    }
    assert!(
        c.feet.y > 3.9 && c.feet.z < -2.5,
        "walking on top, got {}",
        c.feet
    );
}
