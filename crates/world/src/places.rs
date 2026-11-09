//! Where the map's fixed places stand. The map shapes the ground around them
//! (flattened arenas, paths between them), so they live down here; the
//! features that use those places import them from here.

use bevy::prelude::*;

/// The commune's parliament (burned down; Dave's revolt, #3).
pub const PARLIAMENT_AT: Vec2 = Vec2::new(-12.0, -10.0);
/// The commune bonfire, in front of the parliament.
pub const BONFIRE_AT: Vec2 = Vec2::new(PARLIAMENT_AT.x + 3.5, PARLIAMENT_AT.y + 8.5);
/// The duel arena: centre and half its width (a flat square, #20 and on).
pub const ARENA_CENTER: Vec3 = Vec3::new(0.0, 0.0, -12.0);
pub const ARENA_HALF: f32 = 7.0;
