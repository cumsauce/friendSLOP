//! The surf map (#306): where its hall is and how the air feels in it. The
//! walker (`play::fps`) reads [`air_at`]; the feature (`side::surf`) builds
//! the hall and moves bodies in and out.

use bevy::prelude::*;
use serde::Deserialize;

/// Tuning and writing: `assets/data/side/surf.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The brownstone's front door (x, z).
    pub door: (f32, f32),
    /// How close (m) to the door takes you in.
    pub door_reach: f32,
    /// The neon sign over the door (x, y, z).
    pub sign_at: (f32, f32, f32),
    /// Where you come back out (x, z).
    pub street: (f32, f32),
    /// The start pad's top (x, y, z): the hall's origin.
    pub hall_at: (f32, f32, f32),
    /// Fall this far below the start pad and you're back on it.
    pub fall_below: f32,
    /// Air acceleration (per second) inside the hall.
    pub air_accel_hz: f32,
    /// Speed you may add towards where you steer in the air (m/s).
    pub air_max_wish: f32,
    /// Top speed inside the hall (m/s).
    pub max_speed: f32,
    pub enter_line: String,
    /// `{secs}`.
    pub wipeout_line: String,
    /// `{secs}`, `{best}`.
    pub finish_line: String,
    pub exit_label: String,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/surf.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The hall's origin (the start pad's top, middle of its front edge).
pub fn hall_at(t: &Tuning) -> Vec3 {
    let (x, y, z) = t.hall_at;
    Vec3::new(x, y, z)
}

/// The hall's inside, relative to its origin (matches `surf-hall.gltf`).
pub const HALL_MIN: Vec3 = Vec3::new(-22.0, -50.0, -176.0);
pub const HALL_MAX: Vec3 = Vec3::new(22.0, 12.0, 4.0);

/// A point relative to the hall, if it's inside it.
pub fn in_hall(t: &Tuning, p: Vec3) -> Option<Vec3> {
    let local = p - hall_at(t);
    (local.cmpge(HALL_MIN).all() && local.cmple(HALL_MAX).all()).then_some(local)
}

/// The walker's air control for a body with feet at `feet`.
pub fn air_at(feet: Vec3) -> friendslop_kcc::Air {
    let t = tuning();
    if in_hall(&t, feet).is_some() && t.max_speed > 0.0 {
        friendslop_kcc::Air {
            accel_hz: t.air_accel_hz,
            max_wish_speed: t.air_max_wish,
            max_speed: t.max_speed,
        }
    } else {
        friendslop_kcc::Air::NORMAL
    }
}
