//! Kinematic walking, ported from [bevy_ahoy](https://github.com/janhohenheim/bevy_ahoy)
//! at commit `630a4573` (MIT OR Apache-2.0 — see the LICENSE files in this
//! directory), trimmed to what a friendSLOP walker needs: walking, jumping,
//! stair stepping, a slope limit, ground snapping and sliding along walls.
//! Skipped from upstream: crouching, water, mantling, crane movement, tic
//! tacs, moving platforms, pushing dynamic bodies, and bevy_enhanced_input
//! (our input arrives over the network instead).
//!
//! Upstream drives a real kinematic rigid body; here the controller is pure
//! shape queries (`avian`'s `MoveAndSlide`) against the shared world, and the
//! result is written back into the replicated `Walker` position on the host —
//! clients only ever send input, as everywhere else in friendSLOP.
//!
//! The heart of the port is [`step`], the equivalent of upstream's `run_kcc`:
//! depenetrate, ground the character, apply gravity and friction, then either
//! walk (with a [`STEP_SIZE`] step-up) or fly, and finally snap back down to
//! slopes and stairs. `ground_move`, `step_move`, `snap_to_ground`,
//! `update_grounded`, the accelerate/friction pair and `handle_jump` are all
//! carried over from `kcc.rs` with the platform/mantle/water branches cut.

use avian3d::character_controller::move_and_slide::{
    DepenetrationConfig, MoveAndSlide, MoveAndSlideConfig, MoveAndSlideHitResponse,
    MoveAndSlideOutput, MoveHitData,
};
use avian3d::prelude::*;
use bevy::prelude::*;

/// How wide the walker is (and how tall the capsule is in total).
pub const BODY_RADIUS: f32 = 0.35;
const CYLINDER_LENGTH: f32 = 0.9;
/// Capsule centre above the feet: the bottom hemisphere rests on the ground.
pub const FEET_TO_CENTRE: f32 = CYLINDER_LENGTH / 2.0 + BODY_RADIUS;
/// Step-up for stairs, kerbs and the walkable ramps.
pub const STEP_SIZE: f32 = 0.5;
/// Surfaces steeper than this are walls, not slopes (45 degrees).
const MIN_WALK_COS: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// How far below the feet the ground still counts as "standing on it".
const GROUND_DISTANCE: f32 = 0.05;
/// Space kept between the body and whatever it touches.
const SKIN_WIDTH: f32 = 0.015;
/// Rising faster than this counts as jumping, not standing. Must stay below
/// the jump speed (`sqrt(2 * GRAVITY * JUMP_HEIGHT)` ~ 5.0), or a jumper is
/// re-grounded next to a slope or on a fast tick and the jump is cancelled.
const UNGROUND_SPEED: f32 = 4.0;
/// Room needed in front of a step before we climb it.
const MIN_STEP_LEDGE_SPACE: f32 = 0.2;
/// Free-fall acceleration (matches the old hand-written movement).
const GRAVITY: f32 = 14.0;
/// Jump apex height; gives the old jump speed with this gravity.
const JUMP_HEIGHT: f32 = 0.9;
/// Reaches full speed in ~50 ms; the old hand-written movement was instant.
const ACCELERATION_HZ: f32 = 20.0;
const AIR_ACCELERATION_HZ: f32 = 10.0;
/// Upstream's `max_air_wish_speed`: the classic Quake air-control limit.
const MAX_AIR_WISH_SPEED: f32 = 0.76;
const FRICTION_HZ: f32 = 12.0;
const STOP_SPEED: f32 = 2.54;
const MAX_SPEED: f32 = 30.0;

/// The shape the walker sweeps: 1.6 m tall in total, standing on `feet`.
pub fn body() -> Collider {
    Collider::capsule(BODY_RADIUS, CYLINDER_LENGTH)
}

/// Per-walker controller state (upstream: `CharacterControllerState`, minus
/// crouching, mantling, platforms and tic tacs).
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Kcc {
    /// Fully 3D; while grounded the vertical part is zeroed.
    pub velocity: Vec3,
    /// The ground under the feet, if any.
    pub grounded: Option<MoveHitData>,
}

/// One server tick of the controller for one walker whose feet stand at
/// `feet`. Returns the new feet position; mutates `kcc`'s velocity and
/// grounded state. `speed` is the wish speed (walk or run).
#[allow(clippy::too_many_arguments)]
pub fn step(
    kcc: &mut Kcc,
    feet: Vec3,
    yaw: f32,
    stick: Vec2,
    jump: bool,
    speed: f32,
    dt: f32,
    move_and_slide: &MoveAndSlide,
) -> Vec3 {
    let shape = body();
    let rotation = Quat::from_rotation_y(yaw);
    let config = MoveAndSlideConfig {
        skin_width: SKIN_WIDTH,
        ..default()
    };
    let filter = SpatialQueryFilter::default();
    let cast = |movement: Vec3, translation: Vec3| {
        move_and_slide.cast_move(&shape, translation, rotation, movement, SKIN_WIDTH, &filter)
    };
    let depenetrate = |translation: Vec3| {
        translation
            + move_and_slide.depenetrate(
                &shape,
                translation,
                rotation,
                &DepenetrationConfig::from(&config),
                &filter,
            )
    };

    let mut translation = feet + Vec3::Y * FEET_TO_CENTRE;

    // Wish velocity: stick relative to where we're looking (flat).
    let flat = Quat::from_rotation_y(yaw);
    let wish_velocity = flat * Vec3::new(stick.x, 0.0, -stick.y) * speed;

    // Gravity, in two halves around the move so a mid-tick landing loses only
    // half a tick of fall (upstream `start_gravity` / `finish_gravity`).
    kcc.velocity.y -= GRAVITY * 0.5 * dt;

    // Jumping (upstream `handle_jump`, minus coyote time and tic tacs).
    let mut jumped = false;
    if kcc.grounded.is_some() && jump {
        jumped = true;
        kcc.grounded = None;
        // v = sqrt(2 g h): reach `JUMP_HEIGHT` against linear deceleration.
        kcc.velocity.y += (2.0 * GRAVITY * JUMP_HEIGHT).sqrt();
    }

    // Friction before moving (upstream `friction`, no per-surface friction).
    if kcc.grounded.is_some() {
        let speed_now = kcc.velocity.xz().length();
        if speed_now > 0.001 {
            let control = speed_now.max(STOP_SPEED);
            let drop = control * FRICTION_HZ * dt;
            let scale = ((speed_now - drop).max(0.0)) / speed_now;
            kcc.velocity *= scale;
        }
    }
    kcc.velocity = kcc.velocity.clamp_length_max(MAX_SPEED);

    // Walk or fly (upstream `ground_move` / `air_move`, minus platforms).
    let ground_move = kcc.grounded.is_some();
    if ground_move {
        kcc.velocity.y = 0.0;
        accelerate(&mut kcc.velocity, wish_velocity, ACCELERATION_HZ, None, dt);
        kcc.velocity.y = 0.0;

        let mut movement = kcc.velocity * dt;
        movement.y = 0.0;
        if cast(movement, translation).is_none() {
            translation += movement;
            translation = depenetrate(translation);
        } else {
            // Blocked: try to step up stairs and kerbs first.
            translation = step_move(
                move_and_slide,
                &shape,
                &config,
                &filter,
                kcc,
                translation,
                dt,
                false,
            );
        }
    } else {
        accelerate(
            &mut kcc.velocity,
            wish_velocity,
            AIR_ACCELERATION_HZ,
            Some(MAX_AIR_WISH_SPEED),
            dt,
        );
        // Like upstream's `air_move`, step up ledges while airborne too.
        translation = step_move(
            move_and_slide,
            &shape,
            &config,
            &filter,
            kcc,
            translation,
            dt,
            true,
        );
    }

    // Snap down slopes and stairs after a ground move (before re-grounding,
    // as upstream does: afterwards the ground is already out of snap range),
    // then work out whether we are standing.
    if ground_move && !jumped {
        translation = snap_to_ground(move_and_slide, &shape, rotation, &filter, translation);
    }
    update_grounded(move_and_slide, &shape, rotation, &filter, kcc, translation);
    kcc.velocity.y -= GRAVITY * 0.5 * dt;
    if kcc.grounded.is_some() {
        kcc.velocity.y = 0.0;
    }
    kcc.velocity = kcc.velocity.clamp_length_max(MAX_SPEED);

    translation - Vec3::Y * FEET_TO_CENTRE
}

/// Reach `wish` velocity at `acceleration_hz` (upstream `ground_accelerate`).
/// `max_wish_speed` caps the speed we may *add* towards (air control).
fn accelerate(
    velocity: &mut Vec3,
    wish: Vec3,
    acceleration_hz: f32,
    max_wish_speed: Option<f32>,
    dt: f32,
) {
    let Ok((wish_dir, wish_speed)) = Dir3::new_and_length(wish) else {
        return;
    };
    let target = max_wish_speed.map_or(wish_speed, |m| wish_speed.min(m));
    let add_speed = target - velocity.dot(*wish_dir);
    if add_speed <= 0.0 {
        return;
    }
    let accel_speed = (wish_speed * acceleration_hz * dt).min(add_speed);
    *velocity += accel_speed * *wish_dir;
}

/// Slide along the floor; when that hits something, redo the same move from a
/// step-up and keep whichever went further (upstream `step_move`, minus the
/// touch bookkeeping).
fn step_move(
    move_and_slide: &MoveAndSlide,
    shape: &Collider,
    config: &MoveAndSlideConfig,
    filter: &SpatialQueryFilter,
    kcc: &mut Kcc,
    translation: Vec3,
    dt: f32,
    airborne: bool,
) -> Vec3 {
    let rotation = Quat::IDENTITY;
    let original_position = translation;
    let original_velocity = kcc.velocity;

    // Slide the direct path.
    let down = slide(move_and_slide, shape, config, filter, kcc, translation, dt);

    // Redo it from a step up.
    kcc.velocity = original_velocity;
    let mut translation = original_position;
    let hit = move_and_slide.cast_move(
        shape,
        translation,
        rotation,
        Vec3::Y * STEP_SIZE,
        SKIN_WIDTH,
        filter,
    );
    let dist = hit.map_or(STEP_SIZE, |hit| hit.distance);
    translation += Vec3::Y * dist;

    // Verify we have enough space to stand once up there.
    let probe = kcc.velocity.normalize_or_zero() * MIN_STEP_LEDGE_SPACE;
    let Ok(step_dir) = Dir3::new(probe) else {
        kcc.velocity = down.projected_velocity;
        return down.position;
    };
    if move_and_slide
        .cast_move(shape, translation, rotation, probe, SKIN_WIDTH, filter)
        .is_some()
    {
        kcc.velocity = down.projected_velocity;
        return down.position;
    }

    // Slide from up on the step, then make sure we are properly over the
    // ledge: at fine tick steps the capsule only just crosses the edge, and
    // the cast down below would find nothing under it (upstream moves onto
    // the ledge the same way in its ledge code).
    let mut translation =
        slide(move_and_slide, shape, config, filter, kcc, translation, dt).position;
    let onto_ledge = move_and_slide.cast_move(
        shape,
        translation,
        rotation,
        *step_dir * MIN_STEP_LEDGE_SPACE,
        SKIN_WIDTH,
        filter,
    );
    translation += onto_ledge.map_or(*step_dir * MIN_STEP_LEDGE_SPACE, |hit| {
        *step_dir * hit.distance
    });

    // Come back down onto the step.
    let hit = move_and_slide.cast_move(
        shape,
        translation,
        rotation,
        Vec3::NEG_Y * STEP_SIZE,
        SKIN_WIDTH,
        filter,
    );

    // If we would slide down from up there, this wasn't a step. If there is
    // simply no floor within a step below, we stepped into clear air (say,
    // over a small gap): commit to it and let gravity bring us down.
    // (Deliberate divergence from upstream, which reverts to the blocked
    // slide here: committing is what makes kerb crossing work at 60 Hz, see
    // #98. Don't "fix" it without re-running tests/it/kcc.rs.)
    let hit = hit.filter(|hit| hit.normal1.y >= MIN_WALK_COS);
    let up_position = match hit {
        Some(hit) => translation - Vec3::Y * hit.distance,
        // Airborne there is no floor to commit to: never hover on a ledge
        // lift (it would climb half a metre per tick).
        None if airborne => return finish_down(kcc, &down),
        None => translation,
    };

    // Use whichever path went further.
    let down_dist = down.position.xz().distance_squared(original_position.xz());
    let up_dist = up_position.xz().distance_squared(original_position.xz());
    if down_dist >= up_dist {
        kcc.velocity = down.projected_velocity;
        down.position
    } else {
        kcc.velocity.y = down.projected_velocity.y;
        up_position
    }
}

fn finish_down(kcc: &mut Kcc, down: &MoveAndSlideOutput) -> Vec3 {
    kcc.velocity = down.projected_velocity;
    down.position
}

/// `MoveAndSlide::move_and_slide` with the ground plane pinned, returning the
/// new position and projected velocity.
fn slide(
    move_and_slide: &MoveAndSlide,
    shape: &Collider,
    config: &MoveAndSlideConfig,
    filter: &SpatialQueryFilter,
    kcc: &mut Kcc,
    translation: Vec3,
    dt: f32,
) -> MoveAndSlideOutput {
    let mut config = config.clone();
    if let Some(ground) = kcc.grounded {
        config.planes.push(Dir3::new_unchecked(ground.normal1));
    }
    move_and_slide.move_and_slide(
        shape,
        translation,
        Quat::IDENTITY,
        kcc.velocity,
        std::time::Duration::from_secs_f32(dt),
        &config,
        filter,
        |_| MoveAndSlideHitResponse::Accept,
    )
}

/// Stick to slopes and stairs by dropping back onto them (upstream
/// `snap_to_ground`, minus step-down detection).
fn snap_to_ground(
    move_and_slide: &MoveAndSlide,
    shape: &Collider,
    rotation: Quat,
    filter: &SpatialQueryFilter,
    translation: Vec3,
) -> Vec3 {
    let up_hit = move_and_slide.cast_move(
        shape,
        translation,
        rotation,
        Vec3::Y * GROUND_DISTANCE,
        SKIN_WIDTH,
        filter,
    );
    let up_dist = up_hit.map_or(GROUND_DISTANCE, |hit| hit.distance);
    let start = translation + Vec3::Y * up_dist;
    let hit = move_and_slide.cast_move(
        shape,
        start,
        rotation,
        Vec3::NEG_Y * (up_dist + STEP_SIZE),
        SKIN_WIDTH,
        filter,
    );
    let Some(hit) = hit else {
        return translation;
    };
    if hit.intersects() || hit.normal1.y < MIN_WALK_COS || hit.distance <= GROUND_DISTANCE {
        return translation;
    }
    start - Vec3::Y * hit.distance
}

/// Cast the capsule down and ground or unground the walker accordingly
/// (upstream `update_grounded`, minus water, ladders and platforms).
fn update_grounded(
    move_and_slide: &MoveAndSlide,
    shape: &Collider,
    rotation: Quat,
    filter: &SpatialQueryFilter,
    kcc: &mut Kcc,
    translation: Vec3,
) {
    if kcc.velocity.y > UNGROUND_SPEED {
        kcc.grounded = None;
        return;
    }
    let hit = move_and_slide.cast_move(
        shape,
        translation,
        rotation,
        Vec3::NEG_Y * GROUND_DISTANCE,
        SKIN_WIDTH,
        filter,
    );
    kcc.grounded = hit.filter(|hit| hit.normal1.y >= MIN_WALK_COS);
}

/// Walk at 5 m/s (the old `WALK_SPEED`).
pub const WALK_SPEED: f32 = 5.0;
/// Sprint at 10.5 m/s (the old `RUN_SPEED`).
pub const RUN_SPEED: f32 = 10.5;
