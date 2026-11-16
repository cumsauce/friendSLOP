//! Walking bodies, on [bevy_ahoy](https://github.com/janhohenheim/bevy_ahoy)'s
//! kinematic character controller (the crate itself, 0.3).
//!
//! The host stays authoritative: clients only send input, and the walker's
//! feet live in the replicated `Walker` as before. Here an owner entity (a
//! player's) carries a [`Kcc`] and a [`Walking`], and the host gives it a
//! server-only **body**: an ahoy `CharacterController` with our body
//! ([`body`]). Each fixed tick [`drive`] feeds the body what the owner wants
//! (the stick in the body's yaw frame, the jump button, the wish speed and the
//! place's air control) straight into ahoy's input (`AccumulatedInput`, the
//! same thing its `bevy_enhanced_input` actions fill), ahoy moves it, and
//! [`read`] copies the feet and velocity back onto the owner.
//!
//! Anything that moves a walker on its own (a teleport, a climb, the edge of
//! the world) just writes `Walking.feet` / `Walker.pos`: a body whose owner's
//! feet moved since the last tick is put there. [`Kcc::stop`] also kills its
//! velocity and any pending jump.
//!
//! The body's collider is `ColliderDisabled`: like the old hand-ported
//! controller, a walker is pure shape queries against the world, so walkers
//! don't block each other, raycasts (the crosshair, climbing) pass through
//! them and nothing physical is pushed around.
//!
//! Ours on top of ahoy: holding jump hops again the tick you land (bunny
//! hopping, `auto_hop`), letting go while rising cuts the jump short
//! (`release_cut`), and an analog stick walks slower when only half pushed.

use avian3d::character_controller::move_and_slide::{MoveAndSlideConfig, MoveHitData};
use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::Stopwatch;
use bevy_ahoy::input::AccumulatedInput;
use bevy_ahoy::prelude::{AhoySystems, CharacterController, CharacterControllerState};
use bevy_ahoy::{AhoyCameraPlugin, AhoyDynamicPlugin, AhoyPlugins, CharacterLook};
use std::time::Duration;

/// How wide the walker is.
pub const BODY_RADIUS: f32 = 0.35;
/// How tall the walker is.
const BODY_HEIGHT: f32 = 1.6;
/// Body centre above the feet.
pub const FEET_TO_CENTRE: f32 = BODY_HEIGHT / 2.0;
/// Step-up for stairs, kerbs and the walkable ramps.
pub const STEP_SIZE: f32 = 0.5;
/// Surfaces steeper than this are walls, not slopes (45 degrees).
const MIN_WALK_COS: f32 = std::f32::consts::FRAC_1_SQRT_2;
/// How far below the feet the ground still counts as "standing on it".
const GROUND_DISTANCE: f32 = 0.05;
/// Space kept between the body and whatever it touches.
const SKIN_WIDTH: f32 = 0.015;
/// Rising faster than this counts as jumping, not standing. Must stay below
/// the jump speed (`sqrt(2 * GRAVITY * height)` ~ 5.0).
const UNGROUND_SPEED: f32 = 4.0;
/// Room needed in front of a step before we climb it.
const MIN_STEP_LEDGE_SPACE: f32 = 0.2;
/// Free-fall acceleration (matches the old hand-written movement).
const GRAVITY: f32 = 14.0;
/// Jump apex height; gives the old jump speed with this gravity.
const JUMP_HEIGHT: f32 = 0.9;
/// Reaches full speed in ~50 ms.
const ACCELERATION_HZ: f32 = 20.0;
const AIR_ACCELERATION_HZ: f32 = 10.0;
/// Ahoy's `max_air_wish_speed`: the classic Quake air-control limit.
const MAX_AIR_WISH_SPEED: f32 = 0.76;
const FRICTION_HZ: f32 = 12.0;
const STOP_SPEED: f32 = 2.54;
const MAX_SPEED: f32 = 30.0;

/// Walk at 5 m/s.
pub const WALK_SPEED: f32 = 5.0;
/// Sprint at 10.5 m/s.
pub const RUN_SPEED: f32 = 10.5;

/// The shape the walker sweeps, 1.6 m tall: a capsule whose bottom is a
/// shallow cone instead of a half-sphere. Ahoy steps up only onto ground
/// that is walkable where it lands, and a round bottom meets a step's edge
/// at a slant it refuses until it is far over the edge (never, at walking
/// speed, one tick at a time); the cone's faces are walkable, so the edge
/// counts at once (so it can also stand on a narrow edge). Narrow at the
/// feet like a capsule, it still sits low on ramps (headroom under stairs);
/// the round head ducks under sloping ceilings. A convex hull of rings.
pub fn body() -> Collider {
    /// How far the cone comes in at the feet, and how tall it is.
    const FOOT_CHAMFER: (f32, f32) = (0.25, 0.2);
    const SIDES: usize = 16;
    let top = FEET_TO_CENTRE - BODY_RADIUS;
    let mut points = Vec::new();
    let mut ring = |y: f32, r: f32| {
        for i in 0..SIDES {
            let a = i as f32 / SIDES as f32 * std::f32::consts::TAU;
            points.push(Vec3::new(a.cos() * r, y, a.sin() * r));
        }
    };
    // The cone: its faces' normals stay walkable (up part ~0.78 > 1/sqrt 2).
    ring(-FEET_TO_CENTRE, BODY_RADIUS - FOOT_CHAMFER.0);
    ring(-FEET_TO_CENTRE + FOOT_CHAMFER.1, BODY_RADIUS);
    for k in 0..4 {
        let a = k as f32 / 4.0 * std::f32::consts::FRAC_PI_2;
        ring(top + a.sin() * BODY_RADIUS, a.cos() * BODY_RADIUS);
    }
    points.push(Vec3::Y * FEET_TO_CENTRE);
    Collider::convex_hull(&points).unwrap_or_else(|| Collider::cylinder(BODY_RADIUS, BODY_HEIGHT))
}

/// Ahoy's plugins as friendSLOP uses them: everything but its camera (we have
/// our own) and its pushing of dynamic bodies (walkers are shape queries).
/// Add after `PhysicsPlugins`, wherever there is a physics world.
pub fn controller_plugins() -> bevy::app::PluginGroupBuilder {
    AhoyPlugins::default()
        .build()
        .disable::<AhoyCameraPlugin>()
        .disable::<AhoyDynamicPlugin>()
}

/// How a jump feels: apex height, forgiveness windows and the short hop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Jump {
    /// Apex height (m) of a held jump.
    pub height: f32,
    /// Seconds after walking off a ledge that a jump still counts (ahoy's
    /// `coyote_time`).
    pub coyote: f32,
    /// Seconds a press made just before landing is kept for the landing
    /// (ahoy's `jump_input_buffer`).
    pub buffer: f32,
    /// Rising speed kept when the button is let go early (0..=1).
    pub release_cut: f32,
    /// Holding the button jumps again the tick you land (bunny hopping):
    /// no ground tick, so no friction eats your speed.
    pub auto_hop: bool,
}

impl Jump {
    pub const NORMAL: Jump = Jump {
        height: JUMP_HEIGHT,
        coyote: 0.1,
        buffer: 0.1,
        release_cut: 0.5,
        auto_hop: true,
    };
}

impl Default for Jump {
    fn default() -> Self {
        Self::NORMAL
    }
}

/// Air control and the speed cap: [`Air::NORMAL`] everywhere, except places
/// that want more (the surf map cranks air acceleration the way surf servers do).
///
/// Ahoy's air move is Quake/Source's `PM_AirAccelerate` (the wish speed
/// capped at `max_wish_speed` for the "can I add more" test, the full wish
/// speed for how much per tick), and ground steeper than the walkable slope
/// isn't ground: you slide along it in the air, velocity clipped onto its
/// plane, with no friction. That is surfing; the numbers here make it feel
/// like a surf server (see `data/side/surf.ron`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Air {
    /// Air acceleration (Source `sv_airaccelerate`).
    pub accel_hz: f32,
    /// Cap on the wish speed in the air (m/s; Source's 30 u/s).
    pub max_wish_speed: f32,
    pub max_speed: f32,
    /// Free-fall acceleration (m/s^2, `sv_gravity`).
    pub gravity: f32,
    /// Ground acceleration (`sv_accelerate`).
    pub ground_accel_hz: f32,
    /// Ground friction (`sv_friction`, times the surface's own coefficient).
    pub friction_hz: f32,
    /// Below this speed friction acts as if you were going this fast
    /// (m/s, `sv_stopspeed`).
    pub stop_speed: f32,
    /// Wish speed at full stick (m/s) in place of the walker's walk/run
    /// speed; 0 keeps the walker's own.
    pub wish_speed: f32,
}

impl Air {
    pub const NORMAL: Air = Air {
        accel_hz: AIR_ACCELERATION_HZ,
        max_wish_speed: MAX_AIR_WISH_SPEED,
        max_speed: MAX_SPEED,
        gravity: GRAVITY,
        ground_accel_hz: ACCELERATION_HZ,
        friction_hz: FRICTION_HZ,
        stop_speed: STOP_SPEED,
        wish_speed: 0.0,
    };
}

/// The jump button between ticks: fresh presses and the release cut.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct JumpState {
    /// Button state last tick, to find fresh presses.
    held: bool,
    /// A jump was handed to ahoy this tick.
    armed: bool,
    /// In a jump whose rise the button still holds.
    rising: bool,
}

/// A walker's controller state, mirrored from its body after every tick.
/// Writing `velocity` (or clearing `grounded`) hands that to the body on the
/// next tick.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Kcc {
    /// Fully 3D; while grounded the vertical part is zero.
    pub velocity: Vec3,
    /// The ground under the feet, if any.
    pub grounded: Option<MoveHitData>,
    pub jump: JumpState,
    stop: bool,
    synced: Option<Synced>,
}

/// What the body was left at by the last [`read`].
#[derive(Clone, Copy, Debug)]
struct Synced {
    feet: Vec3,
    velocity: Vec3,
    grounded: bool,
}

impl Kcc {
    /// Stop dead: no velocity, no ground, no jump pending (a teleport).
    pub fn stop(&mut self) {
        self.velocity = Vec3::ZERO;
        self.grounded = None;
        self.jump = JumpState::default();
        self.stop = true;
    }
}

/// What a walker wants this tick, and (after [`read`]) where its feet are.
#[derive(Component, Clone, Copy, Debug)]
pub struct Walking {
    /// Feet position: write it to move the walker, read it after the tick.
    pub feet: Vec3,
    pub yaw: f32,
    /// x = right, y = forward; its length (up to 1) scales the speed.
    pub stick: Vec2,
    /// The jump button, held this tick.
    pub jump: bool,
    /// Wish speed at full stick (walk or run, skills and all).
    pub speed: f32,
    pub air: Air,
    pub jump_tuning: Jump,
    /// Leave the body where it is this tick (frozen, climbing, duelling).
    pub skip: bool,
}

impl Default for Walking {
    fn default() -> Self {
        Self {
            feet: Vec3::ZERO,
            yaw: 0.0,
            stick: Vec2::ZERO,
            jump: false,
            speed: WALK_SPEED,
            air: Air::NORMAL,
            jump_tuning: Jump::NORMAL,
            skip: false,
        }
    }
}

/// On a body: the walker it moves for.
#[derive(Component, Debug)]
#[relationship(relationship_target = Body)]
pub struct BodyOf(pub Entity);

/// On a walker: its body (despawned with it).
#[derive(Component, Debug)]
#[relationship_target(relationship = BodyOf, linked_spawn)]
pub struct Body(Entity);

impl Body {
    pub fn get(&self) -> Entity {
        self.0
    }
}

/// Ordering: [`KccSet::Drive`] in `FixedUpdate` hands the input over;
/// [`KccSet::Read`] in `FixedPostUpdate`, right after ahoy, reads the result.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KccSet {
    Drive,
    Read,
}

/// Our controller numbers on ahoy's controller.
pub fn controller(jump: &Jump) -> CharacterController {
    let mut c = CharacterController {
        ground_distance: GROUND_DISTANCE,
        min_walk_cos: MIN_WALK_COS,
        stop_speed: STOP_SPEED,
        friction_hz: FRICTION_HZ,
        acceleration_hz: ACCELERATION_HZ,
        gravity: GRAVITY,
        step_size: STEP_SIZE,
        speed: WALK_SPEED,
        move_and_slide: MoveAndSlideConfig {
            skin_width: SKIN_WIDTH,
            // Avian rejects contacts deeper than this as bogus. A walker put
            // down half inside a wall (a teleport next to one) must still be
            // pushed out, so allow up to the body's whole width.
            penetration_rejection_threshold: 2.0 * BODY_RADIUS + 0.1,
            ..default()
        },
        unground_speed: UNGROUND_SPEED,
        min_step_ledge_space: MIN_STEP_LEDGE_SPACE,
        ..default()
    };
    tune(&mut c, Air::NORMAL, jump);
    c
}

/// The per-place and per-file numbers (air control, jump feel).
fn tune(c: &mut CharacterController, air: Air, jump: &Jump) {
    c.air_acceleration_hz = air.accel_hz;
    c.max_air_wish_speed = air.max_wish_speed;
    c.max_speed = air.max_speed;
    c.gravity = air.gravity;
    c.acceleration_hz = air.ground_accel_hz;
    c.friction_hz = air.friction_hz;
    c.stop_speed = air.stop_speed;
    c.jump_height = jump.height.max(0.0);
    c.coyote_time = Duration::from_secs_f32(jump.coyote.max(0.0));
    c.jump_input_buffer = Duration::from_secs_f32(jump.buffer.max(0.0));
}

fn centre(feet: Vec3) -> Vec3 {
    feet + Vec3::Y * FEET_TO_CENTRE
}

/// Where there's a physics world, walkers get bodies and move with ahoy.
pub fn plugin(app: &mut App) {
    let physics = resource_exists::<avian3d::collider_tree::ColliderTrees>;
    app.configure_sets(
        FixedPostUpdate,
        KccSet::Read
            .after(AhoySystems::MoveCharacters)
            .before(PhysicsSystems::First),
    )
    .add_systems(Update, give_bodies.run_if(physics))
    .add_systems(FixedUpdate, drive.in_set(KccSet::Drive).run_if(physics))
    .add_systems(FixedPostUpdate, read.in_set(KccSet::Read).run_if(physics));
}

fn give_bodies(mut commands: Commands, new: Query<(Entity, &Walking), (With<Kcc>, Without<Body>)>) {
    for (owner, w) in &new {
        commands.spawn((
            Name::new("walker body"),
            BodyOf(owner),
            controller(&w.jump_tuning),
            body(),
            ColliderDisabled,
            Transform::from_translation(centre(w.feet)),
            CharacterLook {
                yaw: w.yaw,
                pitch: 0.0,
            },
            crate::areas::Global,
        ));
    }
}

type BodyParts<'a> = (
    &'a mut Transform,
    &'a mut Position,
    &'a mut LinearVelocity,
    &'a mut CharacterController,
    &'a mut CharacterControllerState,
    &'a mut AccumulatedInput,
    &'a mut CharacterLook,
);

/// Hand each body its walker's wishes (before ahoy runs).
pub fn drive(mut owners: Query<(&mut Kcc, &Walking, &Body)>, mut bodies: Query<BodyParts>) {
    for (mut kcc, w, body) in &mut owners {
        let Ok((mut tf, mut pos, mut vel, mut cfg, mut state, mut input, mut look)) =
            bodies.get_mut(body.get())
        else {
            continue;
        };
        let moved = kcc
            .synced
            .is_none_or(|s| s.feet.distance_squared(w.feet) > 1e-10);
        if moved {
            tf.translation = centre(w.feet);
            pos.0 = tf.translation;
        }
        if kcc.stop {
            kcc.stop = false;
            vel.0 = Vec3::ZERO;
            state.grounded = None;
            input.jumped = None;
        } else {
            let s = kcc.synced;
            if s.is_none_or(|s| s.velocity != kcc.velocity) {
                vel.0 = kcc.velocity;
            }
            if s.is_some_and(|s| s.grounded) && kcc.grounded.is_none() {
                state.grounded = None;
            }
        }
        tune(&mut cfg, w.air, &w.jump_tuning);
        look.yaw = w.yaw;
        look.pitch = 0.0;
        if w.skip {
            input.last_movement = None;
            input.jumped = None;
            kcc.jump.armed = false;
            continue;
        }
        // Ahoy walks at full speed in the stick's direction; scale the speed
        // by how far the stick is pushed.
        let push = w.stick.length().min(1.0);
        let speed = if w.air.wish_speed > 0.0 {
            w.air.wish_speed
        } else {
            w.speed
        };
        cfg.speed = speed * push;
        input.last_movement = Some(w.stick.normalize_or_zero());
        // Jumping: a fresh press, or (auto hop) the button held on landing.
        let pressed = w.jump && !kcc.jump.held;
        kcc.jump.held = w.jump;
        if pressed || (w.jump && w.jump_tuning.auto_hop) {
            input.jumped = Some(Stopwatch::new());
        }
        kcc.jump.armed = input.jumped.is_some();
    }
}

/// Copy where ahoy put each body back onto its walker (right after ahoy).
pub fn read(mut owners: Query<(&mut Kcc, &mut Walking, &Body)>, mut bodies: Query<BodyParts>) {
    for (mut kcc, mut w, body) in &mut owners {
        let Ok((mut tf, mut pos, mut vel, cfg, mut state, input, _)) = bodies.get_mut(body.get())
        else {
            continue;
        };
        if w.skip {
            // Not this tick: back where it was, moving as it was, standing
            // on what it stood on. Off the ground (a climb) means no coyote
            // jump when the controller takes over again.
            tf.translation = centre(w.feet);
            pos.0 = tf.translation;
            vel.0 = kcc.velocity;
            state.grounded = kcc.grounded;
            if kcc.grounded.is_none() {
                let long_ago = cfg.coyote_time + Duration::from_secs(1);
                state.last_ground.set_elapsed(long_ago);
            }
        } else {
            let jumped = kcc.jump.armed && input.jumped.is_none() && state.grounded.is_none();
            if jumped {
                kcc.jump.rising = true;
            } else if kcc.jump.rising {
                if vel.y <= 0.0 {
                    kcc.jump.rising = false;
                } else if !kcc.jump.held {
                    // Let go early: a short hop.
                    vel.y *= w.jump_tuning.release_cut.clamp(0.0, 1.0);
                    kcc.jump.rising = false;
                }
            }
            kcc.jump.armed = false;
            w.feet = tf.translation - Vec3::Y * FEET_TO_CENTRE;
            kcc.velocity = vel.0;
            kcc.grounded = state.grounded;
        }
        kcc.synced = Some(Synced {
            feet: w.feet,
            velocity: kcc.velocity,
            grounded: kcc.grounded.is_some(),
        });
    }
}
