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
/// distance (#151), fading out as it goes (#227): `(near, far)` metres for the Near / Far draw distance.
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

    /// The visibility range for the Far (`far`) or Near draw distance. Things
    /// dither out over the last [`Self::FADE`] of it instead of popping out
    /// (#227: "too much popin ... needs to be invisible to the player").
    pub fn range(self, far: bool) -> VisibilityRange {
        let end = if far { self.far } else { self.near };
        let fade = (end * Self::FADE).max(4.0).min(end);
        VisibilityRange {
            start_margin: 0.0..0.0,
            end_margin: end - fade..end,
            use_aabb: self.from_bounds,
        }
    }

    /// The share of a draw distance spent fading out.
    pub const FADE: f32 = 0.3;
}

/// Marker for scenery props that bend in the wind. The game's ambience
/// system captures the prop's rest rotation the first frame it is seen and
/// writes the sway; this is only the data it keeps.
#[derive(Component, Default)]
pub struct Sway {
    pub base: Option<Quat>,
    pub phase: f32,
    /// Which of the alternating frames this prop is updated on.
    pub odd: bool,
}

/// A tree model: the game's lod system gives its meshes far versions.
/// `shadows: false` also keeps it out of the shadow map (the smaller forest
/// trees).
#[derive(Component, Clone, Copy, Debug)]
pub struct TreeLod {
    pub shadows: bool,
}

/// The overview camera's eye offset from its focus: its fixed starting pose,
/// a constant that scenery and the camera both key off (the game's
/// `CameraRig::default`).
pub const OVERVIEW_OFFSET: Vec3 = Vec3::new(0.0, 5.5, 17.0);

/// Where the overview camera's eye sits on the ground plane (x, z).
/// Scenery keeps a clearing around it so the first view isn't a fern.
pub fn overview_eye() -> Vec2 {
    OVERVIEW_OFFSET.xz()
}

/// How much grass and ambient detail to spawn, `0..=1`, kept in step with
/// the video settings. Absent (headless tests): full detail.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Detail(pub f32);

/// The detail level, full when there's no [`Detail`] resource.
pub fn detail(d: Option<&Detail>) -> f32 {
    d.map_or(1.0, |d| d.0)
}

/// The seed every generated structure (village houses, the sewer, furniture)
/// is built from. The host rolls it and the game replicates it, so every peer
/// builds the same world. Present only once it is known: generators wait for
/// it with `resource_exists::<WorldSeed>` / `resource_changed::<WorldSeed>`.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WorldSeed(pub u32);

impl WorldSeed {
    /// A sub-seed for one generator, so features don't share random streams.
    pub fn for_(self, salt: u32) -> u32 {
        let mut x = self.0 ^ salt.wrapping_mul(0x9E37_79B9);
        x ^= x >> 16;
        x = x.wrapping_mul(0x85EB_CA6B);
        x ^= x >> 13;
        x = x.wrapping_mul(0xC2B2_AE35);
        x ^ (x >> 16)
    }
}
