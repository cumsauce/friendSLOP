//! The shared rules vocabulary: types and constants that features in
//! different crates read. Each still lives at its old path too (`pub use`).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A player's body while they're in the duel arena. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Duelist {
    pub player: u64,
    pub lives: u8,
    pub bullets: u8,
    /// Feet position.
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
}

/// Eye height: the camera sits in your eyes.
pub const EYE: f32 = 1.6;
pub mod boba;
pub mod campaign;
pub mod depths;
pub mod house_odds;
pub mod morality;
pub mod nyc;
pub mod skills;
pub mod surf;
pub mod villagers;
