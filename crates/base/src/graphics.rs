//! Video settings (#106, #131, #132). Purely local, nothing replicated.
//!
//! A preset (Low / Medium / High) sets every option at once, and each option
//! can then be changed on its own, which makes the preset read "Custom"
//! (#132, doomy: "include what's important and maybe have presets").
//! Low is picked automatically in the browser (and with `FRIENDSLOP_LOW=1`)
//! so phones can hold 60 fps. The choice is saved locally.
//!
//! - Shadows: the sun's shadow maps.
//! - Ambient occlusion (#131): soft contact shadows where things meet. Not in
//!   the browser (WebGL2 can't); it needs MSAA off, so anti-aliasing is FXAA
//!   while it's on and MSAA otherwise.
//! - Bloom: the glow around the bonfire and emissives. Off also drops the
//!   HDR render target (#393), which is most of what bloom costs a weak GPU.
//! - Anti-aliasing: smooth edges.
//! - Draw distance: how far the fog starts and the camera draws.
//! - Resolution: in the browser (#191) "Sharp" renders at the screen's pixel
//!   density (up to 2x), "Fast" at 1x (phones are 2-3x, so that's 4-9x fewer
//!   pixels); index.html picks it before the game starts, so a change applies
//!   on the next page load. On desktop "Fast" (the Low preset) renders the
//!   world at `fast_scale` of the window (`data/base/graphics.ron`) and
//!   stretches it to fit, live; the UI stays sharp. A weak GPU (the Steam
//!   Deck, #393) spends most of its frame shading pixels.
//! - Grass & particles: half or full (applies the next time a world is built).
//! - VSync and Fullscreen: the window.
//!
//! Everything else applies live.

use bevy::anti_alias::fxaa::Fxaa;
use bevy::camera::Projection;
use bevy::light::{DirectionalLight, ShadowFilteringMethod};
use bevy::pbr::ScreenSpaceAmbientOcclusion;
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode};

use crate::world::MainCamera;

const FILE: &str = "graphics.txt";

/// Can this platform do ambient occlusion?
pub const SSAO_SUPPORTED: bool = !cfg!(target_arch = "wasm32");

/// Bloom needs an HDR (float) target, which phone browsers' WebGL2 can't
/// always render into (#242: hosting showed only a blue screen on iPhone).
/// The browser renders straight to the screen's 8-bit target instead.
pub const HDR_SUPPORTED: bool = !cfg!(target_arch = "wasm32");

/// Tuning: `assets/data/base/graphics.ron`.
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Desktop "Fast": the world's render size as a fraction of the window's.
    pub fast_scale: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/base/graphics.ron");

/// The Resolution setting the page started with (what index.html applied).
static BOOT_SHARP: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// The UI is laid out for at least this many logical pixels. Smaller screens
/// (a phone held sideways is ~850x390) scale the whole UI down to fit,
/// touch controls included, instead of cutting it off (#191).
const UI_MIN: Vec2 = Vec2::new(960.0, 600.0);

/// Every video option. (The name stays from when it was just Low/High.)
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsQuality {
    pub shadows: bool,
    pub ssao: bool,
    pub bloom: bool,
    pub antialias: bool,
    pub far: bool,
    pub sharp: bool,
    pub dense: bool,
    pub vsync: bool,
    pub fullscreen: bool,
}

/// The presets, from fastest to prettiest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Low,
    Medium,
    High,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Low, Preset::Medium, Preset::High];

    pub fn label(self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }

    /// The options this preset picks. VSync and fullscreen are the window's
    /// business, so presets keep whatever they were.
    pub fn apply(self, q: GraphicsQuality) -> GraphicsQuality {
        let (low, high) = (self == Self::Low, self == Self::High);
        GraphicsQuality {
            shadows: !low,
            ssao: high && SSAO_SUPPORTED,
            bloom: !low && HDR_SUPPORTED,
            antialias: !low,
            far: !low,
            // Desktop Medium stays sharp: only Low trades pixels for speed.
            sharp: high || (!low && !cfg!(target_arch = "wasm32")),
            dense: !low,
            ..q
        }
    }
}

/// One row of the video settings menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoOption {
    Preset,
    Shadows,
    AmbientOcclusion,
    Bloom,
    AntiAlias,
    DrawDistance,
    Resolution,
    Grass,
    VSync,
    Fullscreen,
}

impl VideoOption {
    pub const ALL: [VideoOption; 10] = [
        VideoOption::Preset,
        VideoOption::Shadows,
        VideoOption::AmbientOcclusion,
        VideoOption::Bloom,
        VideoOption::AntiAlias,
        VideoOption::DrawDistance,
        VideoOption::Resolution,
        VideoOption::Grass,
        VideoOption::VSync,
        VideoOption::Fullscreen,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Preset => "Preset",
            Self::Shadows => "Shadows",
            Self::AmbientOcclusion => "Ambient occlusion",
            Self::Bloom => "Bloom (glow)",
            Self::AntiAlias => "Anti-aliasing",
            Self::DrawDistance => "Draw distance",
            Self::Resolution => "Resolution",
            Self::Grass => "Grass & particles",
            Self::VSync => "VSync",
            Self::Fullscreen => "Fullscreen",
        }
    }
}

fn on_off(b: bool) -> &'static str {
    if b { "On" } else { "Off" }
}

impl GraphicsQuality {
    pub fn preset(self) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| p.apply(self) == self)
    }

    /// Low draws less (fog, grass): true for the Low preset or anything with
    /// a near draw distance.
    pub fn is_low(self) -> bool {
        !self.far
    }

    /// How much grass and ambient particles to spawn, `0..=1`.
    pub fn density(self) -> f32 {
        if self.dense { 1.0 } else { 0.5 }
    }

    /// The text on an option's button.
    pub fn value(self, o: VideoOption) -> &'static str {
        match o {
            VideoOption::Preset => self.preset().map_or("Custom", Preset::label),
            VideoOption::Shadows => on_off(self.shadows),
            VideoOption::AmbientOcclusion if !SSAO_SUPPORTED => "N/A",
            VideoOption::Bloom if !HDR_SUPPORTED => "N/A",
            VideoOption::AmbientOcclusion => on_off(self.ssao),
            VideoOption::Bloom => on_off(self.bloom),
            VideoOption::AntiAlias if !self.antialias => "Off",
            VideoOption::AntiAlias if self.ssao => "FXAA",
            VideoOption::AntiAlias => "MSAA",
            VideoOption::DrawDistance => {
                if self.far {
                    "Far"
                } else {
                    "Near"
                }
            }
            VideoOption::Resolution if !cfg!(target_arch = "wasm32") => {
                if self.sharp {
                    "Sharp"
                } else {
                    "Fast"
                }
            }
            VideoOption::Resolution => {
                let boot = BOOT_SHARP.get().copied().unwrap_or(self.sharp);
                match (self.sharp, boot == self.sharp) {
                    (true, true) => "Sharp",
                    (false, true) => "Fast",
                    (true, false) => "Sharp (reload)",
                    (false, false) => "Fast (reload)",
                }
            }
            VideoOption::Grass => {
                if self.dense {
                    "Full"
                } else {
                    "Half"
                }
            }
            VideoOption::VSync => on_off(self.vsync),
            VideoOption::Fullscreen => on_off(self.fullscreen),
        }
    }

    /// Click an option's button: flip it, or step to the next preset.
    pub fn cycle(&mut self, o: VideoOption) {
        match o {
            VideoOption::Preset => {
                let next = match self.preset() {
                    Some(Preset::Low) => Preset::Medium,
                    Some(Preset::Medium) => Preset::High,
                    Some(Preset::High) | None => Preset::Low,
                };
                *self = next.apply(*self);
            }
            VideoOption::Shadows => self.shadows = !self.shadows,
            VideoOption::AmbientOcclusion => self.ssao = !self.ssao && SSAO_SUPPORTED,
            VideoOption::Bloom => self.bloom = !self.bloom && HDR_SUPPORTED,
            VideoOption::AntiAlias => self.antialias = !self.antialias,
            VideoOption::DrawDistance => self.far = !self.far,
            VideoOption::Resolution => self.sharp = !self.sharp,
            VideoOption::Grass => self.dense = !self.dense,
            VideoOption::VSync => self.vsync = !self.vsync,
            VideoOption::Fullscreen => self.fullscreen = !self.fullscreen,
        }
    }

    /// Saved as `key=0/1` lines.
    pub fn to_text(self) -> String {
        [
            ("shadows", self.shadows),
            ("ssao", self.ssao),
            ("bloom", self.bloom),
            ("antialias", self.antialias),
            ("far", self.far),
            ("sharp", self.sharp),
            ("dense", self.dense),
            ("vsync", self.vsync),
            ("fullscreen", self.fullscreen),
        ]
        .iter()
        .map(|(k, v)| format!("{k}={}\n", u8::from(*v)))
        .collect()
    }

    /// Reads [`to_text`](Self::to_text), or the old "low"/"high" files.
    pub fn from_text(text: &str) -> Option<Self> {
        let base = Self::platform_default();
        match text.trim() {
            "low" => return Some(Preset::Low.apply(base)),
            "high" => return Some(Preset::High.apply(base)),
            _ => {}
        }
        let mut q = base;
        let mut any = false;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let v = v.trim() == "1";
            let field = match k.trim() {
                "shadows" => &mut q.shadows,
                "ssao" => &mut q.ssao,
                "bloom" => &mut q.bloom,
                "antialias" => &mut q.antialias,
                "far" => &mut q.far,
                "sharp" => &mut q.sharp,
                "dense" => &mut q.dense,
                "vsync" => &mut q.vsync,
                "fullscreen" => &mut q.fullscreen,
                _ => continue,
            };
            *field = v;
            any = true;
        }
        q.ssao &= SSAO_SUPPORTED;
        q.bloom &= HDR_SUPPORTED;
        any.then_some(q)
    }

    /// Browser builds (and `FRIENDSLOP_LOW=1`) start on Low, others on High.
    pub fn platform_default() -> Self {
        let windowed = Self {
            shadows: false,
            ssao: false,
            bloom: false,
            antialias: false,
            far: false,
            sharp: false,
            dense: false,
            vsync: true,
            fullscreen: false,
        };
        if cfg!(target_arch = "wasm32")
            || std::env::var_os("FRIENDSLOP_LOW").is_some_and(|v| v != "0")
        {
            Preset::Low.apply(windowed)
        } else {
            Preset::High.apply(windowed)
        }
    }
}

impl Default for GraphicsQuality {
    fn default() -> Self {
        let q = crate::settings::load_local(FILE)
            .and_then(|t| Self::from_text(&t))
            .unwrap_or_else(Self::platform_default);
        BOOT_SHARP.get_or_init(|| q.sharp);
        q
    }
}

/// Grass/particle density for spawners that may run without this resource
/// (headless tests): full density then.
pub fn density(q: Option<&GraphicsQuality>) -> f32 {
    q.map_or(1.0, |q| q.density())
}

pub fn plugin(app: &mut App) {
    app.init_resource::<GraphicsQuality>();
    let d = app.world().resource::<GraphicsQuality>().density();
    app.insert_resource(friendslop_core::Detail(d)).add_systems(
        PostUpdate,
        (
            sync_detail,
            save_quality,
            apply_window,
            apply_camera,
            apply_shadows,
            apply_draw_ranges,
        ),
    );
    #[cfg(not(target_arch = "wasm32"))]
    app.add_systems(PostUpdate, render_scale::apply);
}

pub use friendslop_core::DrawRange;

fn apply_draw_ranges(
    mut commands: Commands,
    q: Res<GraphicsQuality>,
    all: Query<(Entity, &DrawRange)>,
    new: Query<(Entity, &DrawRange), Added<DrawRange>>,
) {
    let mut set = |(e, d): (Entity, &DrawRange)| {
        commands.entity(e).try_insert(d.range(q.far));
    };
    if q.is_changed() {
        all.iter().for_each(&mut set);
    } else {
        new.iter().for_each(&mut set);
    }
}

/// Grass and particle spawners (some in other crates) read `Detail`.
fn sync_detail(q: Res<GraphicsQuality>, mut d: ResMut<friendslop_core::Detail>) {
    if q.is_changed() {
        d.set_if_neq(friendslop_core::Detail(q.density()));
    }
}

fn save_quality(q: Res<GraphicsQuality>) {
    if q.is_changed() && !q.is_added() {
        crate::settings::save_local(FILE, &q.to_text());
    }
}

/// UI size, vsync and fullscreen. (The render resolution is index.html's
/// job, see [`RESOLUTION_ADJUSTABLE`]: a scale factor override only changes
/// how big the UI is, never how many pixels get drawn.)
fn apply_window(q: Res<GraphicsQuality>, mut window: Query<&mut Window, With<PrimaryWindow>>) {
    let Ok(mut window) = window.single_mut() else {
        return;
    };
    let want = ui_fit(
        window.resolution.physical_size().as_vec2(),
        window.resolution.base_scale_factor(),
    );
    if window.resolution.scale_factor_override() != want {
        window.resolution.set_scale_factor_override(want);
    }
    if !q.is_changed() {
        return;
    }
    // At startup, VSync on keeps main.rs's choice (the benchmark runs uncapped).
    if !(q.is_added() && q.vsync) {
        window.present_mode = if q.vsync {
            PresentMode::AutoVsync
        } else {
            PresentMode::AutoNoVsync
        };
    }
    let mode = if q.fullscreen {
        WindowMode::BorderlessFullscreen(MonitorSelection::Current)
    } else {
        WindowMode::Windowed
    };
    if window.mode != mode {
        window.mode = mode;
    }
}

/// The scale factor that fits [`UI_MIN`] logical pixels on a screen this many
/// physical pixels big, if that's smaller than the screen's own.
fn ui_fit(physical: Vec2, base: f32) -> Option<f32> {
    let fit = (physical / UI_MIN).min_element();
    (fit > 0.0 && fit < base).then_some(fit)
}

/// Bloom, anti-aliasing, ambient occlusion, shadow filtering, fog and far
/// plane on the main camera, whenever settings change or a camera appears
/// (the atmosphere dresses it on spawn).
fn apply_camera(
    mut commands: Commands,
    q: Res<GraphicsQuality>,
    mut cameras: Query<(Entity, &mut Projection, Ref<MainCamera>), With<MainCamera>>,
) {
    for (e, mut projection, cam) in &mut cameras {
        if !q.is_changed() && !cam.is_added() {
            continue;
        }
        let mut c = commands.entity(e);
        // Bloom is the only reason for the HDR (float) target: without it
        // tonemapping runs in the material shaders into an 8-bit target,
        // half the memory traffic per pixel (#393: the Steam Deck is GPU-bound).
        if q.bloom && HDR_SUPPORTED {
            c.insert(bevy::camera::Hdr);
            c.insert(Bloom {
                intensity: 0.08,
                ..Bloom::NATURAL
            });
        } else {
            c.remove::<(Bloom, bevy::camera::Hdr)>();
        }
        // A depth prepass lets alpha-cut leaves shade only their front layer
        // (discard otherwise shades every overlapping card in full). It pays
        // for itself even on Low, where it halves the main pass.
        c.insert(bevy::core_pipeline::prepass::DepthPrepass);
        if q.ssao {
            c.insert(ScreenSpaceAmbientOcclusion::default());
        } else {
            c.remove::<ScreenSpaceAmbientOcclusion>();
        }
        // Ambient occlusion needs MSAA off, so it smooths edges with FXAA.
        match (q.antialias, q.ssao) {
            (true, true) => c.insert((Msaa::Off, Fxaa::default())),
            (true, false) => c.remove::<Fxaa>().insert(Msaa::default()),
            (false, _) => c.remove::<Fxaa>().insert(Msaa::Off),
        };
        c.insert(if q.shadows {
            ShadowFilteringMethod::Gaussian
        } else {
            ShadowFilteringMethod::Hardware2x2
        });
        // The fog itself (where it starts and ends with `far`) is the
        // day/night cycle's to write (`atmosphere.rs`), with the places' looks.
        if let Projection::Perspective(p) = projection.as_mut() {
            // Nothing past the fog is visible anyway: don't draw it.
            p.far = if q.far { 1000.0 } else { 160.0 };
        }
    }
}

/// Marks the shadow-casting sun (see `atmosphere.rs`).
#[derive(Component)]
pub struct SunShadows;

/// Only the sun casts shadows.
fn apply_shadows(
    q: Res<GraphicsQuality>,
    mut lights: Query<(&mut DirectionalLight, Ref<SunShadows>)>,
) {
    for (mut light, sun) in &mut lights {
        if (q.is_changed() || sun.is_added()) && light.shadow_maps_enabled != q.shadows {
            light.shadow_maps_enabled = q.shadows;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip_and_tweaks_read_custom() {
        let base = GraphicsQuality::platform_default();
        for p in Preset::ALL {
            assert_eq!(p.apply(base).preset(), Some(p));
        }
        let mut q = Preset::High.apply(base);
        q.cycle(VideoOption::Bloom);
        assert_eq!(q.value(VideoOption::Preset), "Custom");
        q.cycle(VideoOption::Preset);
        assert_eq!(q.preset(), Some(Preset::Low));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn fast_renders_fewer_pixels_and_sharp_none_less() {
        let w = UVec2::new(1280, 800);
        assert_eq!(render_scale::size(w, true, 0.7), None);
        assert_eq!(render_scale::size(w, false, 1.0), None);
        assert_eq!(
            render_scale::size(w, false, 0.7),
            Some(UVec2::new(896, 560))
        );
        assert_eq!(
            render_scale::size(UVec2::ZERO, false, 0.5),
            Some(UVec2::ONE)
        );
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        let s = TUNING.get().fast_scale;
        assert!(s > 0.0 && s <= 1.0);
    }

    #[test]
    fn small_screens_shrink_the_ui_to_fit() {
        // A desktop window, at 1x and on a 2x screen: untouched.
        assert_eq!(ui_fit(Vec2::new(1280.0, 720.0), 1.0), None);
        assert_eq!(ui_fit(Vec2::new(2560.0, 1440.0), 2.0), None);
        // A phone sideways at 1x (Fast): 390 px tall shows 600 logical px.
        let s = ui_fit(Vec2::new(844.0, 390.0), 1.0).unwrap_or(1.0);
        assert!((390.0 / s - 600.0).abs() < 0.5);
        // The same phone at 2x (Sharp) lays out the same.
        let s2 = ui_fit(Vec2::new(1688.0, 780.0), 2.0).unwrap_or(2.0);
        assert!((s2 - 2.0 * s).abs() < 1e-4);
    }

    #[test]
    fn settings_save_and_load_including_old_files() {
        let mut q = Preset::Medium.apply(GraphicsQuality::platform_default());
        q.fullscreen = true;
        assert_eq!(GraphicsQuality::from_text(&q.to_text()), Some(q));
        let low = GraphicsQuality::from_text("low");
        assert_eq!(low.and_then(|q| q.preset()), Some(Preset::Low));
        assert_eq!(GraphicsQuality::from_text("garbage"), None);
    }
}

/// Desktop "Fast" resolution: the main camera draws into a smaller picture,
/// and a 2D camera on the window stretches it over the screen. Being the
/// highest window camera, that one also draws the UI, at full sharpness.
/// Sharp leaves the main camera on the window, exactly as before.
#[cfg(not(target_arch = "wasm32"))]
mod render_scale {
    use bevy::camera::visibility::RenderLayers;
    use bevy::camera::{ImageRenderTarget, RenderTarget};
    use bevy::core_pipeline::tonemapping::Tonemapping;
    use bevy::prelude::*;
    use bevy::render::render_resource::TextureFormat;
    use bevy::window::{PrimaryWindow, WindowRef};

    use super::{GraphicsQuality, TUNING};
    use crate::world::MainCamera;

    /// A layer nothing else is on, so the stretch camera draws only the picture.
    const LAYER: usize = 31;

    /// The stretch camera and the sprite showing the picture.
    #[derive(Component)]
    pub(super) struct Stretch;

    /// The world's render size for a window this many physical pixels big,
    /// or `None` to draw straight to the window.
    pub(super) fn size(window: UVec2, sharp: bool, scale: f32) -> Option<UVec2> {
        if sharp || !(scale > 0.0 && scale < 1.0) {
            return None;
        }
        Some(
            (window.as_vec2() * scale)
                .round()
                .as_uvec2()
                .max(UVec2::ONE),
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn apply(
        mut commands: Commands,
        q: Res<GraphicsQuality>,
        mut images: ResMut<Assets<Image>>,
        window: Query<&Window, With<PrimaryWindow>>,
        mut main: Query<&mut RenderTarget, With<MainCamera>>,
        stretch: Query<Entity, With<Stretch>>,
        mut sprites: Query<&mut Sprite, With<Stretch>>,
        mut current: Local<Option<(Handle<Image>, UVec2)>>,
    ) {
        let Ok(window) = window.single() else { return };
        let want = size(
            window.resolution.physical_size(),
            q.sharp,
            TUNING.get().fast_scale,
        );
        let has_main = !main.is_empty();
        let Some(want) = want.filter(|_| has_main) else {
            if current.take().is_some() {
                stretch.iter().for_each(|e| commands.entity(e).despawn());
                for mut t in &mut main {
                    *t = RenderTarget::Window(WindowRef::Primary);
                }
            }
            return;
        };
        let fresh = current.as_ref().is_none_or(|(_, at)| *at != want);
        let stale = main.iter().any(|t| !matches!(t, RenderTarget::Image(_)));
        if fresh || stale {
            let handle = images.add(Image::new_target_texture(
                want.x,
                want.y,
                TextureFormat::Rgba8UnormSrgb,
                None,
            ));
            for mut t in &mut main {
                *t = RenderTarget::Image(ImageRenderTarget::from(handle.clone()));
            }
            if stretch.is_empty() {
                commands.spawn((
                    Stretch,
                    Camera2d,
                    Camera {
                        order: 1,
                        ..default()
                    },
                    Msaa::Off,
                    Tonemapping::None,
                    RenderLayers::layer(LAYER),
                ));
                commands.spawn((
                    Stretch,
                    Sprite::from_image(handle.clone()),
                    RenderLayers::layer(LAYER),
                ));
            } else {
                for mut s in &mut sprites {
                    s.image = handle.clone();
                }
            }
            *current = Some((handle, want));
        }
        let fit = Some(window.resolution.size());
        for mut s in &mut sprites {
            if s.custom_size != fit {
                s.custom_size = fit;
            }
        }
    }
}
