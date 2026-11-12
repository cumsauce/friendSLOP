//! Overview camera: pan with WASD / arrow keys, zoom with the scroll wheel.
//!
//! The camera looks at a focus point on the floor from a fixed, fairly low
//! angle so the horizon and sky stay in frame. The focus is clamped to the floor so the vault,
//! the commune and the edges are reachable but you can't wander off. Panning
//! is off while you're in a duel (WASD moves your duelist there, and the duel
//! drives the camera itself), and in first person (#53), where the camera is
//! in your eyes. V swaps between the two.

use bevy::input::mouse::{AccumulatedMouseScroll, MouseScrollUnit};
use bevy::prelude::*;

use crate::AppState;
use crate::features::duel::Duelist;
use crate::player::LocalPlayer;
use crate::world::MainCamera;
pub use friendslop_base::camera::*;

/// Pan speed in metres per second at the default zoom.
const PAN_SPEED: f32 = 14.0;
/// How fast the camera catches up with its target (per second).
const SMOOTHING: f32 = 8.0;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraRig>()
            .add_systems(OnEnter(AppState::InGame), reset_rig)
            .add_systems(
                Update,
                (steer_rig, follow_rig)
                    .chain()
                    .run_if(in_state(AppState::InGame))
                    .run_if(not(dueling))
                    .run_if(|v: Res<crate::features::fps::FpsView>| !v.first_person),
            );
    }
}

fn reset_rig(mut rig: ResMut<CameraRig>) {
    *rig = CameraRig::default();
}

fn dueling(me: Option<Res<LocalPlayer>>, duelists: Query<&Duelist>) -> bool {
    let Some(me) = me else {
        return false;
    };
    duelists.iter().any(|d| d.player == me.0)
}

fn steer_rig(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    mut rig: ResMut<CameraRig>,
) {
    let mut dir = Vec2::ZERO;
    for (a, b, d) in [
        (KeyCode::KeyW, KeyCode::ArrowUp, Vec2::NEG_Y),
        (KeyCode::KeyS, KeyCode::ArrowDown, Vec2::Y),
        (KeyCode::KeyA, KeyCode::ArrowLeft, Vec2::NEG_X),
        (KeyCode::KeyD, KeyCode::ArrowRight, Vec2::X),
    ] {
        if keys.pressed(a) || keys.pressed(b) {
            dir += d;
        }
    }
    let speed = PAN_SPEED * rig.distance / CameraRig::default().distance;
    rig.pan(dir.normalize_or_zero() * speed * time.delta_secs());
    let lines = match scroll.unit {
        MouseScrollUnit::Line => scroll.delta.y,
        MouseScrollUnit::Pixel => scroll.delta.y / 40.0,
    };
    if lines != 0.0 {
        rig.zoom(lines);
    }
}

fn follow_rig(
    time: Res<Time>,
    rig: Res<CameraRig>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
) {
    let Ok(mut cam) = camera.single_mut() else {
        return;
    };
    let target = rig.transform();
    let t = 1.0 - (-SMOOTHING * time.delta_secs()).exp();
    cam.translation = cam.translation.lerp(target.translation, t);
    cam.rotation = cam.rotation.slerp(target.rotation, t);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_view_looks_at_the_centre_from_low_and_south() {
        let tf = CameraRig::default().transform();
        assert!(tf.translation.distance(view_offset()) < 1e-3);
        assert!(tf.translation.y < tf.translation.z);
    }

    #[test]
    fn pan_and_zoom_are_clamped_to_the_floor() {
        let mut rig = CameraRig::default();
        rig.pan(Vec2::splat(1000.0));
        assert_eq!(rig.focus.x, FLOOR_HALF - EDGE_MARGIN);
        assert_eq!(rig.focus.z, FLOOR_HALF - EDGE_MARGIN);
        // The vault (13, -2) is reachable.
        let mut rig = CameraRig::default();
        rig.pan(Vec2::new(13.0, -2.0));
        assert_eq!(rig.focus, Vec3::new(13.0, 0.0, -2.0));
        for _ in 0..100 {
            rig.zoom(5.0);
        }
        assert_eq!(rig.distance, MIN_DIST);
        for _ in 0..100 {
            rig.zoom(-5.0);
        }
        assert_eq!(rig.distance, MAX_DIST);
    }
}
