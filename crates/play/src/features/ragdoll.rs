//! Getting punted (#320, doomy: "when i get hit by a car ... get punted a bit
//! and ragdoll a little before recovering").
//!
//! Anything that hits a player calls [`knock`] with a velocity. On the host
//! the player's body becomes a real avian rigid body (a capsule thrown with
//! that velocity and a tumble) for a couple of seconds: it flies, bounces off
//! the ground, cars and walls, rolls and comes to rest. Its tilt replicates as
//! [`Ragdoll`] so everyone sees you tumble; your own camera tumbles with it.
//! Then you get back up where you landed and walk on. Headless tests have no
//! physics world, so there the body flies on a plain gravity-and-bounce sum.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::fps::Walker;
use crate::people::{CharacterRig, RigRoot};
use crate::player::Player;

/// Half the body's height: the tumble turns about this point above the feet.
pub const HALF: f32 = 0.9;
/// How high a body lying down is centred above the ground (the capsule's radius).
const LYING: f32 = 0.3;
const GRAVITY: f32 = 9.81;

/// Tuning numbers: `assets/data/play/ragdoll.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// How long a hit makes you a ragdoll (s).
    pub seconds: f32,
    /// The last part of it, spent getting back up (s).
    pub recover: f32,
    /// Tumble speed per m/s of the hit (rad/s), and its cap.
    pub spin: f32,
    pub max_spin: f32,
    /// The body's mass (kg), bounciness and friction.
    pub mass: f32,
    pub bounce: f32,
    pub friction: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/play/ragdoll.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// On a `Player` while they're ragdolling: how their body is tilted, relative
/// to their facing. Replicated; the walk controller leaves them alone meanwhile.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Ragdoll {
    pub tilt: Quat,
}

/// Server: a hit waiting to be turned into a tumble.
#[derive(Component)]
struct Knock {
    vel: Vec3,
    /// A custom ragdoll duration (s), for falls longer than the tuning one.
    seconds: Option<f32>,
}

/// Server: the tumble in progress.
#[derive(Component)]
struct Tumble {
    left: f32,
    /// The physics body flying for us (host with a physics world).
    body: Option<Entity>,
    /// The fallback's own body (no physics world).
    centre: Vec3,
    vel: Vec3,
    spin: Vec3,
    rot: Quat,
    /// Where we last put the feet (anything else moving them ends the tumble).
    feet: Vec3,
    /// Where the feet go while getting up.
    rest: Option<Vec3>,
}

/// Server: the rigid body standing in for a ragdolling player.
#[derive(Component)]
struct RagdollBody;

/// Punt the player `entity` with `velocity` (m/s). Server only.
pub fn knock(commands: &mut Commands, entity: Entity, velocity: Vec3) {
    knock_for(commands, entity, velocity, tuning().seconds);
}

/// Punt the player `entity` with a custom ragdoll duration (s), for falls
/// longer than the tuning one. Server only.
pub fn knock_for(commands: &mut Commands, entity: Entity, velocity: Vec3, seconds: f32) {
    commands.entity(entity).insert(Knock {
        vel: velocity,
        seconds: Some(seconds),
    });
}

/// The tumble a hit with `velocity` starts: head over heels the way you're thrown.
pub fn spin_for(velocity: Vec3, tu: &Tuning) -> Vec3 {
    let flat = velocity.with_y(0.0);
    let axis = Vec3::Y.cross(flat).normalize_or(Vec3::X);
    axis * (flat.length() * tu.spin).min(tu.max_spin) + Vec3::Y * 0.3 * tu.spin
}

/// The fallback's step: gravity, then a bounce off the ground at `floor`.
pub fn fall(centre: Vec3, vel: Vec3, floor: f32, bounce: f32, dt: f32) -> (Vec3, Vec3) {
    let mut vel = vel - Vec3::Y * GRAVITY * dt;
    let mut c = centre + vel * dt;
    if c.y < floor + LYING {
        c.y = floor + LYING;
        if vel.y < 0.0 {
            vel.y = -vel.y * bounce;
        }
        vel.x *= 0.85;
        vel.z *= 0.85;
    }
    (c, vel)
}

/// How far up you are (1 = still a ragdoll, 0 = standing) with `left` seconds to go.
pub fn limp(left: f32, recover: f32) -> f32 {
    if recover <= 0.0 {
        return if left > 0.0 { 1.0 } else { 0.0 };
    }
    (left / recover).clamp(0.0, 1.0)
}

pub fn protocol(app: &mut App) {
    app.replicate::<Ragdoll>().add_systems(
        Update,
        (start_tumbles, tumble)
            .chain()
            .before(super::fps::WalkSet)
            .run_if(in_state(ServerState::Running)),
    );
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, tilt_bodies);
}

fn physics(trees: &Option<Res<avian3d::collider_tree::ColliderTrees>>) -> bool {
    trees.is_some()
}

fn start_tumbles(
    mut commands: Commands,
    trees: Option<Res<avian3d::collider_tree::ColliderTrees>>,
    knocked: Query<(Entity, &Knock, &Walker, Option<&Tumble>)>,
) {
    let tu = tuning();
    for (entity, knock, walker, old) in &knocked {
        if let Some(b) = old.and_then(|t| t.body) {
            commands.entity(b).try_despawn();
        }
        let centre = walker.pos + Vec3::Y * HALF;
        let spin = spin_for(knock.vel, &tu);
        let body = physics(&trees).then(|| {
            commands
                .spawn((
                    RagdollBody,
                    RigidBody::Dynamic,
                    Collider::capsule(LYING, HALF * 2.0 - LYING * 2.0),
                    Mass(tu.mass),
                    Restitution::new(tu.bounce),
                    Friction::new(tu.friction),
                    LinearVelocity(knock.vel),
                    AngularVelocity(spin),
                    Transform::from_translation(centre)
                        .with_rotation(Quat::from_rotation_y(walker.yaw)),
                ))
                .id()
        });
        commands.entity(entity).remove::<Knock>().insert((
            Tumble {
                left: knock.seconds.unwrap_or(tu.seconds),
                body,
                centre,
                vel: knock.vel,
                spin,
                rot: Quat::from_rotation_y(walker.yaw),
                feet: walker.pos,
                rest: None,
            },
            Ragdoll {
                tilt: Quat::IDENTITY,
            },
        ));
    }
}

#[allow(clippy::type_complexity)]
fn tumble(
    time: Res<Time>,
    mut commands: Commands,
    bodies: Query<(&Position, &Rotation), With<RagdollBody>>,
    mut players: Query<(
        Entity,
        &mut Tumble,
        &mut Walker,
        &mut Ragdoll,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    let tu = tuning();
    let dt = time.delta_secs().min(0.1);
    for (entity, mut t, mut walker, mut ragdoll, kcc) in &mut players {
        let moved = walker.pos.distance(t.feet) > 0.5;
        t.left -= dt;
        let up = limp(t.left, tu.recover);
        if moved || t.left <= 0.0 {
            // Done (or something else, a door, a teleport, took you): stand up.
            if let Some(b) = t.body {
                commands.entity(b).try_despawn();
            }
            if !moved && let Some(rest) = t.rest {
                walker.pos = rest;
            }
            if let Some(mut kcc) = kcc {
                kcc.velocity = Vec3::ZERO;
            }
            commands.entity(entity).remove::<(Tumble, Ragdoll)>();
            continue;
        }
        if up < 1.0 {
            // Getting up: the body stops where it lies, the feet find the ground.
            if t.rest.is_none() {
                if let Some(b) = t.body.take() {
                    commands.entity(b).try_despawn();
                }
                let upright = (t.rot * Vec3::Y).y.abs();
                let floor = t.centre.y - LYING - (HALF - LYING) * upright;
                t.rest = Some(t.centre.with_y(floor));
            }
        } else if let Some((pos, rot)) = t.body.and_then(|b| bodies.get(b).ok()) {
            t.centre = pos.0;
            t.rot = rot.0;
        } else if t.body.is_none() {
            let floor = crate::rules::depths::ground_at(t.centre.x, t.centre.z, t.centre.y - HALF);
            let (c, v) = fall(t.centre, t.vel, floor, tu.bounce, dt);
            t.centre = c;
            t.vel = v;
            let spin = t.spin;
            t.rot = Quat::from_scaled_axis(spin * dt) * t.rot;
            if c.y <= floor + LYING + 1e-3 {
                t.spin *= 0.9;
            }
        }
        let yaw = Quat::from_rotation_y(walker.yaw);
        let tilt = yaw.inverse() * t.rot;
        let feet = match t.rest {
            // Rise from lying to standing as you straighten up.
            Some(rest) => rest.lerp(t.centre - Vec3::Y * HALF, up),
            None => t.centre - Vec3::Y * HALF,
        };
        let tilt = Quat::IDENTITY.slerp(tilt, up);
        if walker.pos != feet {
            walker.pos = feet;
        }
        t.feet = feet;
        if ragdoll.tilt != tilt {
            ragdoll.tilt = tilt;
        }
    }
}

/// Everyone: tip ragdolling bodies over about their middle; stand the rest up.
fn tilt_bodies(
    time: Res<Time>,
    players: Query<(Option<&Ragdoll>, &CharacterRig), With<Player>>,
    mut rigs: Query<&mut Transform, With<RigRoot>>,
) {
    let k = 1.0 - (-20.0 * time.delta_secs()).exp();
    for (ragdoll, rig) in &players {
        let Ok(mut tf) = rigs.get_mut(rig.0) else {
            continue;
        };
        let want = ragdoll.map_or(Quat::IDENTITY, |r| r.tilt);
        if tf.rotation == want && ragdoll.is_none() {
            continue;
        }
        let rot = if ragdoll.is_none() && tf.rotation.angle_between(want) < 1e-3 {
            want
        } else {
            tf.rotation.slerp(want, k)
        };
        let pivot = Vec3::Y * HALF;
        tf.rotation = rot;
        tf.translation = pivot - rot * pivot;
    }
}

/// Where your eyes are while ragdolling (feet `pos`, the body tipped by `tilt`
/// about its middle, facing `yaw`), and how the view rolls with it.
pub fn eye(pos: Vec3, yaw: f32, tilt: Quat) -> (Vec3, Quat) {
    let face = Quat::from_rotation_y(yaw);
    let world = face * tilt * face.inverse();
    let at = pos + Vec3::Y * HALF + world * (Vec3::Y * (crate::rules::EYE - HALF));
    (at, world)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_file_has_every_field() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/play/ragdoll.ron"
        );
        let Ok(bytes) = std::fs::read(path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        let t = tuning();
        assert!(t.seconds > t.recover && t.recover > 0.0);
        assert!(t.spin > 0.0 && t.max_spin > 0.0 && t.mass > 0.0);
        assert!(t.bounce > 0.0 && t.friction > 0.0);
    }

    #[test]
    fn a_punt_flies_lands_and_settles() {
        let (mut c, mut v) = (Vec3::new(0.0, HALF, 0.0), Vec3::new(6.0, 7.0, 0.0));
        let mut peak: f32 = 0.0;
        for _ in 0..300 {
            (c, v) = fall(c, v, 0.0, 0.25, 1.0 / 60.0);
            peak = peak.max(c.y);
        }
        assert!(peak > 2.0, "thrown up");
        assert!(c.x > 3.0, "thrown along");
        assert!((c.y - LYING).abs() < 0.05, "lying on the ground");
    }

    #[test]
    fn you_tumble_the_way_you_are_thrown_and_get_back_up() {
        let tu = Tuning {
            spin: 1.0,
            max_spin: 10.0,
            ..default()
        };
        let s = spin_for(Vec3::new(5.0, 3.0, 0.0), &tu);
        // Thrown along +x: you pitch forward about -z.
        assert!(s.z < -4.0);
        assert!(limp(2.0, 0.5) == 1.0 && limp(0.25, 0.5) == 0.5 && limp(-1.0, 0.5) == 0.0);
        let (at, _) = eye(Vec3::ZERO, 0.0, Quat::IDENTITY);
        assert!((at.y - crate::rules::EYE).abs() < 1e-5);
        let (at, _) = eye(
            Vec3::ZERO,
            0.0,
            Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
        );
        assert!(at.y < HALF + 0.01, "lying down, your eyes are low");
    }
}
