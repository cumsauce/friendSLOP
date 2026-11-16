//! Reflection probes: baked environment maps (`assets/probes/<name>_diffuse.ktx2`
//! and `_specular.ktx2`, written by the game's bake mode,
//! `FRIENDSLOP_BAKE=1 cargo run -- host`). The world map ("world") is the
//! camera's; a place with its own look adds a probe: it asks for a bake with
//! [`ProbeBakes::add`] (where to stand) and spawns [`probe`] over its space,
//! which Bevy uses instead of the world map inside that box. Missing files
//! just mean no reflections there.

use bevy::light::EnvironmentMapLight;
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
    probe_at(
        assets,
        name,
        Transform::from_translation(centre).with_scale(size),
        intensity,
    )
}

/// A probe over a turned box: `at`'s translation is its middle, its rotation
/// the box's turn and its scale the box's size (m).
pub fn probe_at(assets: &AssetServer, name: &str, at: Transform, intensity: f32) -> impl Bundle {
    // WebGL2 can't draw light probes anyway: browsers (#242) skip the download.
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (assets, name, intensity);
        at
    }
    #[cfg(not(target_arch = "wasm32"))]
    (
        bevy::light::LightProbe::default(),
        env_map(assets, name, intensity),
        at,
    )
}

/// Ask for the probe called `name` to be baked from `at` (when this is a
/// bake run: otherwise nothing happens). Call it from a place's build.
pub fn register(commands: &mut Commands, name: impl Into<String>, at: Vec3) {
    let name = name.into();
    commands.queue(move |world: &mut World| {
        if let Some(mut b) = world.get_resource_mut::<ProbeBakes>() {
            b.add(&name, at);
        }
    });
}

/// Walk-in interiors' shared tuning: `assets/data/base/interiors.ron`.
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Interiors {
    /// How bright an interior's baked light probe is (1 = as baked).
    pub probe: f32,
}

pub static INTERIORS: friendslop_data::Data<Interiors> =
    friendslop_data::Data::ron("data/base/interiors.ron");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interiors_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            INTERIORS.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(INTERIORS.set_bytes(&bytes), "{path} doesn't parse");
        assert!(INTERIORS.get().probe > 0.0);
    }
}
