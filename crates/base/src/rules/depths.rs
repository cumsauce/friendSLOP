//! Where the tunnels and the warehouse are, below the meadow.

#![allow(unused_imports)]
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// From `features/underground/mod.rs`.
/// The tunnel floor, far under the meadow (the warehouse is further down).
pub const TUNNEL_Y: f32 = -30.0;

/// Is a body with feet at `feet` down in the tunnels?
pub fn below(feet: Vec3) -> bool {
    below_y(feet.y)
}

/// The same, by height alone (the warehouse is below this band).
pub fn below_y(y: f32) -> bool {
    (TUNNEL_Y - 8.0..TUNNEL_Y + 12.0).contains(&y)
}

// From `features/warehouse/mod.rs`.
/// Where the inside is: the floor of a map of its own, far under the meadow.
pub const INSIDE_Y: f32 = -60.0;

/// Is a body with feet at `feet` inside the warehouse?
pub fn inside(feet: Vec3) -> bool {
    feet.y < INSIDE_Y + 20.0
}

/// The ground under a body (for the headless walk, which has no colliders).
pub fn ground_at(x: f32, z: f32, feet_y: f32) -> f32 {
    if feet_y < INSIDE_Y + 20.0 {
        INSIDE_Y
    } else if below_y(feet_y) {
        // Down in the tunnels (#127).
        TUNNEL_Y
    } else {
        crate::terrain::height_at(x, z)
    }
}
