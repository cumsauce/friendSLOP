//! The few types every friendSLOP crate shares, at the bottom of the stack
//! (see "Crates" in AGENTS.md). Keep this tiny: only what several crates need
//! to agree on. No game code.

use bevy::camera::visibility::VisibilityRange;
use bevy::prelude::*;

/// Top-level flow. Add states here only if a feature truly needs a new screen.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Menu,
    InGame,
}

/// Small detail (flowers, pebbles, grass tufts) that stops drawing past a
/// distance (#151): `(near, far)` metres for the Near / Far draw distance.
/// It's tiny, often half-hidden by fog, and there are thousands of it.
/// The game's video settings turn this into a `VisibilityRange`.
#[derive(Component, Clone, Copy, Debug)]
pub struct DrawRange {
    pub near: f32,
    pub far: f32,
    /// Measure from the mesh's bounds centre rather than its origin (for
    /// big chunk meshes whose origin is the world origin).
    pub from_bounds: bool,
}

impl DrawRange {
    /// A small prop: gone past 35 m (Near) or 60 m (Far).
    pub const SMALL_PROP: Self = Self {
        near: 35.0,
        far: 60.0,
        from_bounds: false,
    };

    /// The visibility range for the Far (`far`) or Near draw distance.
    pub fn range(self, far: bool) -> VisibilityRange {
        let mut r = VisibilityRange::abrupt(0.0, if far { self.far } else { self.near });
        r.use_aabb = self.from_bounds;
        r
    }
}

/// How much grass and ambient detail to spawn, `0..=1`, kept in step with
/// the video settings. Absent (headless tests): full detail.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Detail(pub f32);

/// The detail level, full when there's no [`Detail`] resource.
pub fn detail(d: Option<&Detail>) -> f32 {
    d.map_or(1.0, |d| d.0)
}
