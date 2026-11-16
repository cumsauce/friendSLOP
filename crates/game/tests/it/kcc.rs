//! The walker controller (bevy_ahoy's, as `friendslop::kcc` drives it)
//! against a real physics world: flat ground, a step-up, a steep slope that
//! blocks and a wall that stops you.

use avian3d::character_controller::move_and_slide::MoveAndSlide;
use avian3d::prelude::*;
use bevy::ecs::system::RunSystemOnce;
use bevy::prelude::*;

use super::walking::{self, feet, kcc, spawn, tick, walk};

fn world() -> App {
    let mut app = walking::app();
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

#[test]
fn walks_steps_and_hits_walls() {
    let mut app = world();
    walking::settle(&mut app);

    // Stands on the flat ground without sinking.
    let w = spawn(&mut app, Vec3::new(0.0, 0.05, 2.0));
    let (feet_at, _) = walk(&mut app, w, 0.0, 10);
    assert!(kcc(&app, w).grounded.is_some(), "should be grounded");
    assert!(
        (feet_at.y - 0.0).abs() < 0.05,
        "feet settle on the ground, got {feet_at}"
    );

    // A 0.4 m step is stepped up and crossed, not stopped at. (Past the
    // step's far edge the walker drops the 0.4 m back to the ground, like a
    // kerb, so it is briefly airborne: only position is asserted here.)
    let w = spawn(&mut app, Vec3::new(0.0, 0.05, -2.0));
    let (feet_at, lowest) = walk(&mut app, w, 0.0, 60);
    assert!(feet_at.z < -6.5, "walked across the step, at {feet_at}");
    assert!(
        lowest > -0.05 && feet_at.y < 0.45,
        "never fell or flew, {feet_at}"
    );

    // Standing on top of the step keeps you at its height.
    let w = spawn(&mut app, Vec3::new(0.0, 0.45, -6.0));
    let mut feet_at = Vec3::ZERO;
    for _ in 0..10 {
        feet_at = tick(&mut app, w, |w, _| w.stick = Vec2::ZERO);
    }
    assert!(kcc(&app, w).grounded.is_some(), "standing on the step");
    assert!(
        (feet_at.y - 0.4).abs() < 0.06,
        "feet settle on the step top, got {feet_at}"
    );

    // The steep ramp stops you at its base: you never climb it.
    let w = spawn(&mut app, Vec3::new(0.0, 0.05, -2.0));
    let (feet_at, _) = walk(&mut app, w, 0.0, 600);
    assert!(
        feet_at.y < 1.0,
        "never climbed the steep ramp, got {feet_at}"
    );

    // The wall blocks walking west, and you don't pass through it.
    let w = spawn(&mut app, Vec3::new(0.0, 0.05, -18.0));
    let (feet_at, _) = walk(&mut app, w, -90.0_f32.to_radians(), 600);
    assert!(
        feet_at.x > -4.0 + 0.3,
        "the wall stopped the walk, got {feet_at}"
    );
}

#[test]
fn jumps_and_comes_back_down() {
    let mut app = world();
    walking::settle(&mut app);
    let w = spawn(&mut app, Vec3::new(0.0, 0.05, 2.0));
    // Settle, then jump.
    let still = |w: &mut friendslop::kcc::Walking, _: &mut friendslop::kcc::Kcc| {
        w.stick = Vec2::ZERO;
        w.jump = false;
    };
    for _ in 0..10 {
        tick(&mut app, w, still);
    }
    assert!(kcc(&app, w).grounded.is_some(), "settled on the ground");
    let start_y = feet(&app, w).y;
    let at = tick(&mut app, w, |w, _| w.jump = true);
    assert!(at.y > start_y + 0.05, "left the ground, got {at}");
    // Come back down within a second.
    let mut landed = false;
    let mut at = at;
    for _ in 0..60 {
        at = tick(&mut app, w, still);
        if kcc(&app, w).grounded.is_some() && at.y < start_y + 0.2 {
            landed = true;
            break;
        }
    }
    assert!(landed, "never landed after a jump, at {at}");
}

#[test]
fn holding_jump_hops_again_on_landing_and_a_tap_hops_lower() {
    let mut app = world();
    walking::settle(&mut app);
    let w = spawn(&mut app, Vec3::new(0.0, 0.05, 2.0));
    for _ in 0..10 {
        tick(&mut app, w, |w, _| w.jump = false);
    }
    // Held for two seconds: several hops, no standing about between them.
    let mut takeoffs = 0;
    let mut was_grounded = true;
    let mut highest_held = 0.0_f32;
    for _ in 0..120 {
        let at = tick(&mut app, w, |w, _| w.jump = true);
        highest_held = highest_held.max(at.y);
        let grounded = kcc(&app, w).grounded.is_some();
        if was_grounded && !grounded {
            takeoffs += 1;
        }
        was_grounded = grounded;
    }
    assert!(takeoffs >= 3, "bunny hopped {takeoffs} times");
    // Land, then a one-tick tap: the early release cuts the jump short.
    for _ in 0..60 {
        tick(&mut app, w, |w, _| w.jump = false);
    }
    let mut highest_tap = 0.0_f32;
    tick(&mut app, w, |w, _| w.jump = true);
    for _ in 0..60 {
        highest_tap = highest_tap.max(tick(&mut app, w, |w, _| w.jump = false).y);
    }
    assert!(
        highest_tap > 0.1 && highest_tap < highest_held * 0.8,
        "tap {highest_tap} vs held {highest_held}"
    );
}

#[test]
fn a_teleport_puts_the_body_there_and_stops_it() {
    let mut app = world();
    walking::settle(&mut app);
    let w = spawn(&mut app, Vec3::new(0.0, 0.05, 2.0));
    walk(&mut app, w, 0.0, 30);
    assert!(kcc(&app, w).velocity.length() > 1.0, "walking");
    let to = Vec3::new(5.0, 0.0, 5.0);
    let at = tick(&mut app, w, |w, k| {
        w.feet = to;
        w.stick = Vec2::ZERO;
        k.stop();
    });
    assert!(at.distance(to) < 0.05, "teleported to {to}, at {at}");
    assert!(kcc(&app, w).velocity.length() < 0.1, "stopped dead");
}

/// One host tick with W held. With `space` down, climb if there's a face and
/// jump otherwise (Space is both keys, as in the game); with `space` up, walk.
fn climb_tick(app: &mut App, w: Entity, space: bool) -> bool {
    let at = feet(app, w);
    let step = app
        .world_mut()
        .run_system_once(move |move_and_slide: MoveAndSlide| {
            space
                .then(|| {
                    friendslop::features::climb::climb_step(
                        &move_and_slide,
                        at,
                        0.0,
                        Vec2::Y,
                        1.0 / walking::HZ as f32,
                    )
                })
                .flatten()
        })
        .expect("climb step");
    tick(app, w, |w, k| {
        w.yaw = 0.0;
        w.stick = Vec2::Y;
        w.jump = space;
        w.skip = step.is_some();
        if let Some((feet, v)) = step {
            w.feet = feet;
            k.velocity = v;
            k.grounded = None;
        }
    });
    step.is_some()
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
    walking::settle(&mut app);
    let w = spawn(&mut app, Vec3::new(12.0, 0.0, 1.0));
    // Walk up to it, climb, haul over, and stand on top.
    let mut climbed = false;
    let mut on_top = None;
    for tick in 0..600 {
        climbed |= climb_tick(&mut app, w, true);
        // (Grounded on the lip's edge first, then up onto the top.)
        if climbed && kcc(&app, w).grounded.is_some() && feet(&app, w).y > 3.9 {
            on_top = Some((tick, feet(&app, w)));
            break;
        }
    }
    let Some((tick, at)) = on_top else {
        panic!("never landed after climbing, ended at {}", feet(&app, w));
    };
    assert!(
        at.y > 3.9 && at.y < 4.2,
        "lands on top of the block, got {at}"
    );
    assert!(
        tick < 300,
        "climbing 4 m takes a few seconds, took {tick} ticks"
    );
    // Let go of Space and walk on across the top rather than slipping back
    // down. (Still holding Space just bunnies, since Space is jump too.)
    for _ in 0..20 {
        climb_tick(&mut app, w, false);
    }
    let at = feet(&app, w);
    assert!(at.y > 3.9 && at.z < -2.5, "walking on top, got {at}");
}
