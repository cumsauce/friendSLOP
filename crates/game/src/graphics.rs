//! Graphics quality (#106): a "Low" preset for phones, browsers and weak
//! laptops so the game can hold 60 fps, and "High" (the look everyone had so
//! far). Low is picked automatically in the browser and can be flipped in
//! Settings; the choice is saved locally. Purely local, nothing replicated.
//!
//! Low: no sun shadows, no bloom, no MSAA, closer fog and draw distance,
//! rendering at 1x pixel density (phones are 2-3x), half the grass tufts and
//! ambient particles. Shadows/bloom/fog/MSAA/pixel density apply live; grass
//! and particle counts apply the next time a world is built.
//!
//! High also gets the better lighting of #131: screen-space ambient occlusion
//! (soft contact shadows where things meet the ground; not in the browser,
//! WebGL2 can't), Gaussian-filtered soft sun shadows, and FXAA standing in for
//! MSAA (SSAO needs MSAA off).

use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::Projection;
use bevy::light::DirectionalLight;
use bevy::light::ShadowFilteringMethod;
use bevy::pbr::ScreenSpaceAmbientOcclusion;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::world::MainCamera;

const FILE: &str = "graphics.txt";

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsQuality {
    Low,
    High,
}

impl GraphicsQuality {
    pub fn is_low(self) -> bool {
        self == Self::Low
    }

    /// How much grass and ambient particles to spawn, `0..=1`.
    pub fn density(self) -> f32 {
        match self {
            Self::Low => 0.5,
            Self::High => 1.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::High => "High",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Self::Low => Self::High,
            Self::High => Self::Low,
        }
    }

    /// Browser builds (and `FRIENDSLOP_LOW=1`) start on Low.
    fn platform_default() -> Self {
        if cfg!(target_arch = "wasm32")
            || std::env::var_os("FRIENDSLOP_LOW").is_some_and(|v| v != "0")
        {
            Self::Low
        } else {
            Self::High
        }
    }
}

impl Default for GraphicsQuality {
    fn default() -> Self {
        match crate::settings::load_local(FILE).as_deref().map(str::trim) {
            Some("low") => Self::Low,
            Some("high") => Self::High,
            _ => Self::platform_default(),
        }
    }
}

/// Grass/particle density for spawners that may run without this resource
/// (headless tests): full density then.
pub fn density(q: Option<&GraphicsQuality>) -> f32 {
    q.map_or(1.0, |q| q.density())
}

pub fn plugin(app: &mut App) {
    app.init_resource::<GraphicsQuality>().add_systems(
        PostUpdate,
        (save_quality, apply_window, apply_camera, apply_shadows),
    );
}

fn save_quality(q: Res<GraphicsQuality>) {
    if q.is_changed() && !q.is_added() {
        let text = if q.is_low() { "low" } else { "high" };
        crate::settings::save_local(FILE, text);
    }
}

/// Phones report 2-3 device pixels per CSS pixel: rendering at 1x is the
/// single biggest win for fill rate. UI keeps its logical size.
fn apply_window(q: Res<GraphicsQuality>, mut window: Query<&mut Window, With<PrimaryWindow>>) {
    let Ok(mut window) = window.single_mut() else {
        return;
    };
    let want = q.is_low().then_some(1.0);
    if window.resolution.scale_factor_override() != want {
        window.resolution.set_scale_factor_override(want);
    }
}

/// Bloom, MSAA, fog and far plane on the main camera, whenever the quality
/// changes or a camera appears (the atmosphere dresses it on spawn).
fn apply_camera(
    mut commands: Commands,
    q: Res<GraphicsQuality>,
    mut cameras: Query<
        (
            Entity,
            &mut Projection,
            Option<&mut DistanceFog>,
            Ref<MainCamera>,
        ),
        With<MainCamera>,
    >,
) {
    for (e, mut projection, fog, cam) in &mut cameras {
        if !q.is_changed() && !cam.is_added() {
            continue;
        }
        if q.is_low() {
            commands
                .entity(e)
                .remove::<(Bloom, Fxaa, ScreenSpaceAmbientOcclusion)>()
                .insert((Msaa::Off, ShadowFilteringMethod::Hardware2x2));
        } else {
            let mut cam = commands.entity(e);
            cam.insert((
                Msaa::Off,
                Fxaa::default(),
                ShadowFilteringMethod::Gaussian,
                Bloom {
                    intensity: 0.08,
                    ..Bloom::NATURAL
                },
            ));
            if !cfg!(target_arch = "wasm32") {
                cam.insert(ScreenSpaceAmbientOcclusion::default());
            }
        }
        let (start, end) = if q.is_low() {
            (40.0, 140.0)
        } else {
            (60.0, 230.0)
        };
        if let Some(mut fog) = fog {
            fog.falloff = FogFalloff::Linear { start, end };
        }
        if let Projection::Perspective(p) = projection.as_mut() {
            // Nothing past the fog is visible anyway: don't draw it.
            p.far = if q.is_low() { end + 20.0 } else { 1000.0 };
        }
    }
}

/// Marks the shadow-casting sun (see `atmosphere.rs`).
#[derive(Component)]
pub struct SunShadows;

/// Only the sun casts shadows; Low turns them off.
fn apply_shadows(
    q: Res<GraphicsQuality>,
    mut lights: Query<(&mut DirectionalLight, Ref<SunShadows>)>,
) {
    for (mut light, sun) in &mut lights {
        if (q.is_changed() || sun.is_added()) && light.shadow_maps_enabled == q.is_low() {
            light.shadow_maps_enabled = !q.is_low();
        }
    }
}
