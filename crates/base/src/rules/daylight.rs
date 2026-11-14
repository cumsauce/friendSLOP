//! How dark it is outside, for anything that reacts to night.

use bevy::prelude::*;

/// 0 in full day, 1 at night, in between at dawn and dusk. The game's
/// day/night cycle (`atmosphere.rs`) writes it every frame from the sun's
/// height; in places that black out the sky it keeps its last value. Purely
/// local: every peer works it out from the replicated start of the day.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct Darkness(pub f32);
