//! Reflection probes: baked environment maps (`assets/probes/<name>_diffuse.ktx2`
//! and `_specular.ktx2`, written by the game's bake mode,
//! `FRIENDSLOP_BAKE=1 cargo run -- host`). The world map ("world") is the
//! camera's; a place with its own look adds a probe: it asks for a bake with
//! [`ProbeBakes::add`] (where to stand) and spawns [`probe`] over its space,
//! which Bevy uses instead of the world map inside that box. Missing files
//! just mean no reflections there.

use bevy::light::{EnvironmentMapLight, LightProbe};
use bevy::prelude::*;

/// Named places to bake a probe at (where the six pictures are taken from).
#[derive(Resource, Default)]
pub struct ProbeBakes(pub Vec<(String, Vec3)>);

impl ProbeBakes {
    /// Bake the probe called `name` from `at` (eye height, the room's middle).
    pub fn add(&mut self, name: &str, at: Vec3) {
        self.0.retain(|(n, _)| n != name);
        self.0.push((name.to_owned(), at));
    }
}

/// Is this a bake run? (No probes then: they'd show up in their own pictures.)
pub fn baking() -> bool {
    std::env::var("FRIENDSLOP_BAKE").is_ok_and(|v| !v.is_empty() && v != "0")
}

/// A probe's two files, under `assets/`.
pub fn files(name: &str) -> (String, String) {
    (
        format!("probes/{name}_diffuse.ktx2"),
        format!("probes/{name}_specular.ktx2"),
    )
}

/// The environment map baked as `name`.
pub fn env_map(assets: &AssetServer, name: &str, intensity: f32) -> EnvironmentMapLight {
    let (diffuse, specular) = files(name);
    EnvironmentMapLight {
        diffuse_map: assets.load(diffuse),
        specular_map: assets.load(specular),
        intensity,
        ..default()
    }
}

/// A probe over the box at `centre` sized `size` (m), showing the map baked as `name`.
pub fn probe(
    assets: &AssetServer,
    name: &str,
    centre: Vec3,
    size: Vec3,
    intensity: f32,
) -> impl Bundle {
    (
        LightProbe::default(),
        env_map(assets, name, intensity),
        Transform::from_translation(centre).with_scale(size),
    )
}
