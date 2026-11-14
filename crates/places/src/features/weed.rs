//! Effects of weed (#370, hadalade): "Weed should cause noticeable visual
//! changes, increasing in magnitude with how much weed a player has consumed.
//! The screen should wobble and the colors pallets of objects should slowly
//! shift."
//!
//! Local to each player: every joint you light (see [`super::smoking`]) adds
//! one to your [`High`], which wears off slowly. While it's up your view
//! sways and rolls, and the camera's colour grading turns the hues of
//! everything round the colour wheel, both scaled by how high you are.

use bevy::prelude::*;
use bevy::render::view::ColorGrading;

use super::smoking::Smoking;
use crate::AppState;
use crate::player::{LocalPlayer, Player};
use crate::world::MainCamera;

/// Tuning numbers: `assets/data/places/weed.ron`.
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// How much one joint adds to your high.
    pub per_joint: f32,
    /// The most your high can stack to.
    pub max_high: f32,
    /// How much high wears off per second.
    pub fade_per_sec: f32,
    /// Wobble roll (radians) at a high of 1.
    pub roll: f32,
    /// Wobble pitch/yaw sway (radians) at a high of 1.
    pub sway: f32,
    /// How fast the wobble rocks (cycles per second).
    pub wobble_hz: f32,
    /// How fast hues turn (radians per second) at a high of 1.
    pub hue_speed: f32,
    /// Extra saturation at a high of 1.
    pub saturation: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/places/weed.ron");

/// How high the local player is (0 = sober), and where the hue has drifted to.
#[derive(Resource, Default, Debug)]
pub struct High {
    pub level: f32,
    hue: f32,
    time: f32,
    /// The wobble last added to the camera, taken off again before it moves.
    applied: Quat,
}

/// A joint adds to the high, up to the cap.
pub fn add_joint(level: f32, t: &Tuning) -> f32 {
    (level + t.per_joint).min(t.max_high)
}

/// The high wears off.
pub fn fade(level: f32, dt: f32, t: &Tuning) -> f32 {
    (level - t.fade_per_sec * dt).max(0.0)
}

/// The stoned meter (#369): how full your high is, 0 (sober) to 1 (the cap).
pub fn meter(level: f32, t: &Tuning) -> f32 {
    if t.max_high <= 0.0 {
        return 0.0;
    }
    (level / t.max_high).clamp(0.0, 1.0)
}

/// The camera's extra rotation at this high and time. Stronger, but less
/// than linearly, the higher you are.
pub fn wobble(level: f32, time: f32, t: &Tuning) -> Quat {
    if level <= 0.0 {
        return Quat::IDENTITY;
    }
    let k = level.sqrt();
    let w = time * t.wobble_hz * std::f32::consts::TAU;
    let roll = t.roll * k * w.sin();
    let pitch = t.sway * k * (w * 0.71 + 1.3).sin();
    let yaw = t.sway * k * (w * 0.53 + 0.4).cos();
    Quat::from_euler(EulerRot::YXZ, yaw, pitch, roll)
}

pub fn plugin(app: &mut App) {
    app.init_resource::<High>()
        .add_systems(OnEnter(AppState::InGame), sober)
        .add_observer(light_up)
        .add_systems(PreUpdate, unwobble.run_if(in_state(AppState::InGame)))
        .add_systems(
            PostUpdate,
            trip.before(bevy::transform::TransformSystems::Propagate)
                .run_if(in_state(AppState::InGame)),
        );
}

fn sober(mut high: ResMut<High>) {
    *high = High::default();
}

/// The local player lit a joint: higher.
fn light_up(
    add: On<Insert<Smoking>>,
    me: Option<Res<LocalPlayer>>,
    smokers: Query<(&Player, &Smoking)>,
    mut high: ResMut<High>,
) {
    let (Some(me), Ok((p, s))) = (me, smokers.get(add.entity)) else {
        return;
    };
    if p.id == me.0 && s.joint {
        high.level = add_joint(high.level, &TUNING.get());
    }
}

/// Take last frame's wobble off before the camera code moves it.
fn unwobble(mut high: ResMut<High>, mut camera: Query<&mut Transform, With<MainCamera>>) {
    let applied = std::mem::replace(&mut high.applied, Quat::IDENTITY);
    if applied == Quat::IDENTITY {
        return;
    }
    if let Ok(mut tf) = camera.single_mut() {
        tf.rotation *= applied.inverse();
    }
}

/// Wobble the view and turn the colours, then fade a little.
fn trip(
    mut commands: Commands,
    time: Res<Time>,
    mut high: ResMut<High>,
    mut camera: Query<(Entity, &mut Transform, Option<&mut ColorGrading>), With<MainCamera>>,
) {
    let t = TUNING.get();
    let dt = time.delta_secs();
    let Ok((cam, mut tf, grading)) = camera.single_mut() else {
        return;
    };
    let was = high.level;
    if was <= 0.0 && high.hue == 0.0 {
        return;
    }
    high.time += dt;
    high.level = fade(was, dt, &t);
    // The hue keeps turning while high and eases back to normal as it wears off.
    if high.level > 0.0 {
        high.hue = (high.hue + t.hue_speed * high.level.sqrt() * dt) % std::f32::consts::TAU;
    } else {
        let back = if high.hue > std::f32::consts::PI {
            high.hue - std::f32::consts::TAU
        } else {
            high.hue
        };
        high.hue = if back.abs() < 0.01 {
            0.0
        } else {
            (back * (1.0 - (2.0 * dt).min(1.0))).rem_euclid(std::f32::consts::TAU)
        };
    }
    let q = wobble(high.level, high.time, &t);
    tf.rotation *= q;
    high.applied = q;
    let saturation = 1.0 + t.saturation * high.level.sqrt();
    match grading {
        Some(mut g) => {
            g.global.hue = high.hue;
            g.global.post_saturation = saturation;
        }
        None => {
            let mut g = ColorGrading::default();
            g.global.hue = high.hue;
            g.global.post_saturation = saturation;
            commands.entity(cam).insert(g);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        Tuning {
            per_joint: 1.0,
            max_high: 4.0,
            fade_per_sec: 0.01,
            roll: 0.05,
            sway: 0.02,
            wobble_hz: 0.3,
            hue_speed: 0.2,
            saturation: 0.3,
        }
    }

    #[test]
    fn joints_stack_up_to_the_cap_and_wear_off() {
        let t = t();
        let mut l = 0.0;
        for _ in 0..10 {
            l = add_joint(l, &t);
        }
        assert_eq!(l, 4.0);
        assert_eq!(fade(0.005, 1.0, &t), 0.0);
    }

    #[test]
    fn the_meter_rises_per_joint_and_drains_over_time() {
        let t = t();
        assert_eq!(meter(0.0, &t), 0.0);
        let one = meter(add_joint(0.0, &t), &t);
        assert_eq!(one, 0.25);
        let two = meter(add_joint(add_joint(0.0, &t), &t), &t);
        assert!(two > one);
        assert_eq!(meter(99.0, &t), 1.0);
        // A joint's worth drains in per_joint / fade_per_sec seconds.
        let mut l = add_joint(0.0, &t);
        let before = meter(l, &t);
        l = fade(l, 10.0, &t);
        assert!(meter(l, &t) < before);
        l = fade(l, 1000.0, &t);
        assert_eq!(meter(l, &t), 0.0);
    }

    #[test]
    fn more_weed_wobbles_more_and_sober_doesnt() {
        let t = t();
        assert_eq!(wobble(0.0, 1.0, &t), Quat::IDENTITY);
        let one = wobble(1.0, 1.0, &t).angle_between(Quat::IDENTITY);
        let four = wobble(4.0, 1.0, &t).angle_between(Quat::IDENTITY);
        assert!(four > one && one > 0.0);
    }
}
