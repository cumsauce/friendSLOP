//! Video settings (#106, #131, #132). Purely local, nothing replicated.
//!
//! A preset (Low / Medium / High) sets every option at once, and each option
//! can then be changed on its own, which makes the preset read "Custom"
//! (#132, doomy: "include what's important and maybe have presets").
//! Low is picked automatically in the browser, on a Steam Deck (and with
//! `FRIENDSLOP_LOW=1`) so phones and the Deck can hold 60 fps. The choice is saved locally.
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
//!   on the next page load. On desktop it's the render scale, 50% to 100% of
//!   the window in steps ([`SCALES`]; the Low preset picks [`LOW_SCALE`]): the
//!   world is drawn that size and stretched to fit, live; the UI stays sharp.
//!   A weak GPU (the Steam Deck, #393) spends most of its frame shading
//!   pixels. Whatever the scale, the world is never rendered at more than
//!   `max_megapixels` (a Retina screen has four times the pixels of a plain
//!   one the same size).
//! - Window size (desktop): Auto leaves the window as it is, or pick a
//!   windowed size from [`WINDOW_SIZES`] (fullscreen ignores it).
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
    /// How many samples ambient occlusion takes per pixel.
    pub ssao_quality: SsaoQuality,
    /// How far (meters, in view space) ambient occlusion looks for occluders.
    pub ssao_radius: f32,
    /// How thick an occluder is assumed to be (meters): rays further behind
    /// it than this pass, closer ones are blocked.
    pub ssao_thickness: f32,
    /// Desktop: the most pixels (millions) the world is ever rendered at.
    /// A bigger window is rendered at this many and stretched. 0 = no cap.
    pub max_megapixels: f32,
    /// How many times sharper model textures stay at a slant (anisotropic
    /// filtering, 1 = off, at most 16): floors and walls seen along their
    /// length would otherwise blur a few metres away. The browser caps it at 4.
    pub anisotropy: u16,
    /// The sun's shadow cascades when Shadows is Long (every preset's look).
    pub long_shadows: Cascades,
    /// ... and when it's Short.
    pub short_shadows: Cascades,
    /// Eye adaptation (desktop, with the HDR target): how far exposure follows
    /// a scene that's darker or brighter than daylight.
    pub auto_exposure: AutoExposureTuning,
}

/// How the camera's exposure adapts (Bevy's auto exposure meters the scene's
/// average log2 luminance; these bound what it may do with it). Stops are
/// powers of two.
#[derive(serde::Deserialize, Default, Debug, Clone, PartialEq)]
pub struct AutoExposureTuning {
    /// Daylight's average log2 luminance: no change at this level.
    pub outdoor: f32,
    /// The most a darker scene is brightened, in stops (a shop, Penn).
    pub brighten: f32,
    /// The most a brighter scene is darkened, in stops.
    pub darken: f32,
    /// How many stops under daylight the boost starts to fade, and where it's
    /// gone: places dark on purpose (the warehouse, Fulton St, night) stay dark.
    pub fade_start: f32,
    pub fade_end: f32,
    /// How fast it adapts, stops per second, going brighter and going darker.
    pub speed_brighten: f32,
    pub speed_darken: f32,
}

/// Pure: how many stops exposure moves for a scene of average log2 luminance
/// `l` (positive brightens). Follows the scene fully within `brighten` /
/// `darken` stops of daylight, then the boost fades out for scenes from
/// `fade_start` to `fade_end` stops darker than daylight.
pub fn adapt_stops(t: &AutoExposureTuning, l: f32) -> f32 {
    let under = t.outdoor - l;
    let follow = under.clamp(-t.darken, t.brighten);
    if under <= t.fade_start {
        follow
    } else if under >= t.fade_end {
        0.0
    } else {
        let k = (under - t.fade_start) / (t.fade_end - t.fade_start).max(1e-3);
        follow * (1.0 - k)
    }
}

/// The sun's shadow cascades.
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Cascades {
    /// How many shadow maps (every caster is drawn once per cascade).
    pub count: usize,
    /// How far the first (sharpest) cascade reaches (m).
    pub first: f32,
    /// How far shadows reach at all (m).
    pub distance: f32,
}

impl Cascades {
    fn config(&self) -> bevy::light::CascadeShadowConfig {
        let count = self.count.max(1);
        let distance = self.distance.max(1.0);
        bevy::light::CascadeShadowConfigBuilder {
            num_cascades: count,
            minimum_distance: 0.1,
            first_cascade_far_bound: if count == 1 {
                distance
            } else {
                self.first.clamp(0.2, distance)
            },
            maximum_distance: distance,
            overlap_proportion: 0.2,
        }
        .build()
    }
}

/// Ambient occlusion's sample count, cheapest first.
#[derive(serde::Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsaoQuality {
    #[default]
    Low,
    Medium,
    High,
}

impl SsaoQuality {
    fn level(self) -> bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel {
        use bevy::pbr::ScreenSpaceAmbientOcclusionQualityLevel as Level;
        match self {
            Self::Low => Level::Low,
            Self::Medium => Level::Medium,
            Self::High => Level::High,
        }
    }
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/base/graphics.ron");

/// The Resolution setting the page started with (what index.html applied).
static BOOT_SHARP: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// The desktop render scales the Resolution option steps through (percent of
/// the window's size, each side).
pub const SCALES: [u8; 6] = [50, 60, 70, 80, 90, 100];

/// The render scale the Low preset picks.
pub const LOW_SCALE: u8 = 70;

/// The windowed sizes the Window size option steps through (logical pixels);
/// the first, 0x0, is Auto: leave the window as the player has it.
pub const WINDOW_SIZES: [(u32, u32); 5] =
    [(0, 0), (1280, 720), (1600, 900), (1920, 1080), (2560, 1440)];

/// The UI is laid out for at least this many logical pixels. Smaller screens
/// (a phone held sideways is ~850x390) scale the whole UI down to fit,
/// touch controls included, instead of cutting it off (#191).
const UI_MIN: Vec2 = Vec2::new(960.0, 600.0);

/// Every video option. (The name stays from when it was just Low/High.)
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphicsQuality {
    pub shadows: bool,
    /// Short shadows: fewer cascades over a shorter distance (#6 of the
    /// performance plan; each cascade costs about 0.8-1 ms at 2560x1440).
    pub short_shadows: bool,
    pub ssao: bool,
    pub bloom: bool,
    pub antialias: bool,
    /// Anti-aliasing with MSAA (4x) rather than FXAA, when ambient occlusion
    /// is off (they don't mix). MSAA costs ~1.2 ms more at 2560x1440 and
    /// doesn't smooth alpha-cut leaves, so only High picks it.
    pub msaa: bool,
    pub far: bool,
    pub sharp: bool,
    /// Desktop: the render scale, percent of the window's size each side (one
    /// of [`SCALES`]). Browsers have `sharp` instead.
    pub scale: u8,
    pub dense: bool,
    pub vsync: bool,
    pub fullscreen: bool,
    /// Desktop: which of [`WINDOW_SIZES`] the window is set to (0 = Auto).
    pub window: u8,
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
            short_shadows: false,
            antialias: !low,
            msaa: high,
            far: !low,
            // Desktop Medium stays sharp: only Low trades pixels for speed.
            sharp: high || (!low && !cfg!(target_arch = "wasm32")),
            scale: if low { LOW_SCALE } else { 100 },
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
    WindowSize,
}

impl VideoOption {
    pub const ALL: [VideoOption; 11] = [
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
        VideoOption::WindowSize,
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
            Self::WindowSize => "Window size",
        }
    }
}

/// The text for a render scale.
fn scale_label(scale: u8) -> &'static str {
    match scale {
        50 => "50%",
        60 => "60%",
        70 => "70%",
        80 => "80%",
        90 => "90%",
        _ => "100%",
    }
}

/// The scale after this one (wrapping from 100% back to 50%).
fn next_scale(scale: u8) -> u8 {
    SCALES
        .iter()
        .copied()
        .find(|&s| s > scale)
        .unwrap_or(SCALES[0])
}

/// The text for a window size choice.
fn window_label(i: u8) -> &'static str {
    match i {
        1 => "1280x720",
        2 => "1600x900",
        3 => "1920x1080",
        4 => "2560x1440",
        _ => "Auto",
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
            VideoOption::Shadows if !self.shadows => "Off",
            VideoOption::Shadows if self.short_shadows => "Short",
            VideoOption::Shadows => "Long",
            VideoOption::AmbientOcclusion if !SSAO_SUPPORTED => "N/A",
            VideoOption::Bloom if !HDR_SUPPORTED => "N/A",
            VideoOption::AmbientOcclusion => on_off(self.ssao),
            VideoOption::Bloom => on_off(self.bloom),
            VideoOption::AntiAlias if !self.antialias => "Off",
            VideoOption::AntiAlias if self.ssao || !self.msaa => "FXAA",
            VideoOption::AntiAlias => "MSAA",
            VideoOption::DrawDistance => {
                if self.far {
                    "Far"
                } else {
                    "Near"
                }
            }
            VideoOption::Resolution if !cfg!(target_arch = "wasm32") => scale_label(self.scale),
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
            VideoOption::WindowSize if cfg!(target_arch = "wasm32") => "N/A",
            VideoOption::WindowSize => window_label(self.window),
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
            // Long -> Short -> Off -> Long.
            VideoOption::Shadows => match (self.shadows, self.short_shadows) {
                (true, false) => self.short_shadows = true,
                (true, true) => self.shadows = false,
                (false, _) => (self.shadows, self.short_shadows) = (true, false),
            },
            VideoOption::AmbientOcclusion => self.ssao = !self.ssao && SSAO_SUPPORTED,
            VideoOption::Bloom => self.bloom = !self.bloom && HDR_SUPPORTED,
            VideoOption::AntiAlias => self.antialias = !self.antialias,
            VideoOption::DrawDistance => self.far = !self.far,
            VideoOption::Resolution if cfg!(target_arch = "wasm32") => self.sharp = !self.sharp,
            // 50% -> 60% ... -> 100% -> 50%.
            VideoOption::Resolution => {
                self.scale = next_scale(self.scale);
                self.sharp = self.scale >= 100;
            }
            VideoOption::Grass => self.dense = !self.dense,
            VideoOption::VSync => self.vsync = !self.vsync,
            VideoOption::Fullscreen => self.fullscreen = !self.fullscreen,
            VideoOption::WindowSize if cfg!(target_arch = "wasm32") => {}
            VideoOption::WindowSize => {
                self.window = (self.window + 1) % WINDOW_SIZES.len() as u8;
            }
        }
    }

    /// Saved as `key=0/1` lines.
    pub fn to_text(self) -> String {
        [
            ("shadows", self.shadows),
            ("short_shadows", self.short_shadows),
            ("ssao", self.ssao),
            ("bloom", self.bloom),
            ("antialias", self.antialias),
            ("msaa", self.msaa),
            ("far", self.far),
            ("sharp", self.sharp),
            ("dense", self.dense),
            ("vsync", self.vsync),
            ("fullscreen", self.fullscreen),
        ]
        .iter()
        .map(|(k, v)| format!("{k}={}\n", u8::from(*v)))
        .chain([
            format!("scale={}\n", self.scale),
            format!("window={}\n", self.window),
        ])
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
        let mut scale = None;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            // The two numbers; everything else is a 0/1 flag.
            match k.trim() {
                "scale" => {
                    scale = v.trim().parse::<u8>().ok().filter(|s| SCALES.contains(s));
                    any = true;
                    continue;
                }
                "window" => {
                    if let Ok(w) = v.trim().parse::<u8>()
                        && usize::from(w) < WINDOW_SIZES.len()
                    {
                        q.window = w;
                    }
                    any = true;
                    continue;
                }
                _ => {}
            }
            let v = v.trim() == "1";
            let field = match k.trim() {
                "shadows" => &mut q.shadows,
                "short_shadows" => &mut q.short_shadows,
                "ssao" => &mut q.ssao,
                "bloom" => &mut q.bloom,
                "antialias" => &mut q.antialias,
                "msaa" => &mut q.msaa,
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
        // Files from before the render scale: Sharp was full, Fast was 70%.
        q.scale = scale.unwrap_or(if q.sharp { 100 } else { LOW_SCALE });
        q.ssao &= SSAO_SUPPORTED;
        q.bloom &= HDR_SUPPORTED;
        any.then_some(q)
    }

    /// Browser and Android builds, the Steam Deck (and `FRIENDSLOP_LOW=1`) start on Low,
    /// others on High.
    pub fn platform_default() -> Self {
        let windowed = Self {
            shadows: false,
            short_shadows: false,
            ssao: false,
            bloom: false,
            antialias: false,
            msaa: false,
            far: false,
            sharp: false,
            scale: LOW_SCALE,
            dense: false,
            vsync: true,
            fullscreen: false,
            window: 0,
        };
        if cfg!(any(target_arch = "wasm32", target_os = "android"))
            || on_steam_deck()
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
        let mut q = crate::settings::load_local(FILE)
            .and_then(|t| Self::from_text(&t))
            .unwrap_or_else(Self::platform_default);
        // Settings saved on a Steam Deck before it started on Low were the
        // old High default (#393): move them to Low once, then leave them be.
        if on_steam_deck() && crate::settings::load_local(DECK_FILE).is_none() {
            q = Preset::Low.apply(q);
            crate::settings::save_local(FILE, &q.to_text());
            crate::settings::save_local(DECK_FILE, "low\n");
        }
        BOOT_SHARP.get_or_init(|| q.sharp);
        q
    }
}

/// Marks that a Steam Deck's settings were moved to Low once.
const DECK_FILE: &str = "graphics-deck.txt";

/// Steam sets `SteamDeck=1` for games it starts on a Steam Deck.
fn on_steam_deck() -> bool {
    std::env::var_os("SteamDeck").is_some_and(|v| v == "1")
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
    // Eye adaptation needs compute shaders: not in WebGL2 browsers.
    #[cfg(not(target_arch = "wasm32"))]
    app.add_plugins(bevy::post_process::auto_exposure::AutoExposurePlugin)
        .add_systems(PostUpdate, apply_auto_exposure.after(apply_camera));
    #[cfg(not(target_arch = "wasm32"))]
    patch_auto_exposure(app);
}

/// Replaces Bevy's embedded auto exposure shader with our patched copy
/// (`shaders/auto_exposure.wesl`): the original writes past its histogram
/// on a NaN pixel, which on Metal corrupted GPU memory. Runs after
/// `AutoExposurePlugin` registered its copy, before the pipeline loads it.
#[cfg(not(target_arch = "wasm32"))]
fn patch_auto_exposure(app: &mut App) {
    use bevy::asset::io::embedded::EmbeddedAssetRegistry;
    use std::path::{Path, PathBuf};
    let Some(registry) = app.world().get_resource::<EmbeddedAssetRegistry>() else {
        return;
    };
    registry.insert_asset(
        PathBuf::new(),
        Path::new("bevy_post_process/auto_exposure/auto_exposure.wesl"),
        include_bytes!("shaders/auto_exposure.wesl").as_slice(),
    );
}

/// Desktop: the camera's exposure adapts (only with the HDR target; Low keeps
/// a fixed exposure). Rebuilt when the tuning file changes.
#[cfg(not(target_arch = "wasm32"))]
fn apply_auto_exposure(
    mut commands: Commands,
    mut curves: ResMut<Assets<bevy::post_process::auto_exposure::AutoExposureCompensationCurve>>,
    cameras: Query<
        (
            Entity,
            Has<bevy::camera::Hdr>,
            Has<bevy::post_process::auto_exposure::AutoExposure>,
        ),
        With<MainCamera>,
    >,
    mut last: Local<Option<std::sync::Arc<Tuning>>>,
) {
    use bevy::post_process::auto_exposure::{AutoExposure, AutoExposureCompensationCurve};
    let t = TUNING.get();
    let changed = !last.as_ref().is_some_and(|l| std::sync::Arc::ptr_eq(l, &t));
    for (e, hdr, has) in &cameras {
        if !hdr {
            if has {
                commands.entity(e).remove::<AutoExposure>();
            }
            continue;
        }
        if has && !changed {
            continue;
        }
        let a = &t.auto_exposure;
        // The curve maps the metered luminance to where it should end up
        // (luminance + the stops to move), sampled every half stop.
        let points: Vec<Vec2> = (0..=36)
            .map(|i| {
                let l = a.outdoor - 12.0 + i as f32 * 0.5;
                Vec2::new(l, l + adapt_stops(a, l))
            })
            .collect();
        let Ok(curve) = AutoExposureCompensationCurve::from_curve(
            bevy::curve::cubic_splines::LinearSpline::new(points),
        ) else {
            continue;
        };
        commands.entity(e).insert(AutoExposure {
            range: (a.outdoor - 12.0)..=(a.outdoor + 6.0),
            speed_brighten: a.speed_brighten,
            speed_darken: a.speed_darken,
            compensation_curve: curves.add(curve),
            ..default()
        });
    }
    *last = Some(t);
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
fn apply_window(
    q: Res<GraphicsQuality>,
    mut window: Query<&mut Window, With<PrimaryWindow>>,
    mut sized: Local<Option<(u8, bool)>>,
) {
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
    // A window size, when the choice (or leaving fullscreen) changes it, so
    // dragging the window's edge afterwards sticks until the next pick. Not
    // in a browser: the canvas is the page's.
    let key = (q.window, q.fullscreen);
    if !cfg!(target_arch = "wasm32") && *sized != Some(key) {
        *sized = Some(key);
        if let (false, Some(&(w, h))) = (q.fullscreen, WINDOW_SIZES.get(usize::from(q.window)))
            && w > 0
        {
            window.resolution.set(w as f32, h as f32);
        }
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
    mut last: Local<Option<std::sync::Arc<Tuning>>>,
) {
    // The tuning file applies live, like the settings.
    let t = TUNING.get();
    let tuned = !last.as_ref().is_some_and(|l| std::sync::Arc::ptr_eq(l, &t));
    *last = Some(t.clone());
    for (e, mut projection, cam) in &mut cameras {
        if !q.is_changed() && !cam.is_added() && !tuned {
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
            // Bevy's default (High) took 2.6 ms of a 12.6 ms GPU frame at
            // 2560x1440 in the forest; Low takes 1.4 and its blur hides the
            // difference.
            c.insert(ScreenSpaceAmbientOcclusion {
                quality_level: t.ssao_quality.level(),
                radius: t.ssao_radius.max(0.01),
                constant_object_thickness: t.ssao_thickness.max(0.01),
            });
        } else {
            c.remove::<ScreenSpaceAmbientOcclusion>();
        }
        // Ambient occlusion needs MSAA off, so it smooths edges with FXAA,
        // and so does everything but High (FXAA is cheaper, #5).
        match (q.antialias, q.msaa && !q.ssao) {
            (true, false) => c.insert((Msaa::Off, Fxaa::default())),
            (true, true) => c.remove::<Fxaa>().insert(Msaa::default()),
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

/// Only the sun casts shadows: its cascades come from the tuning file,
/// long or short.
fn apply_shadows(
    mut commands: Commands,
    q: Res<GraphicsQuality>,
    mut lights: Query<(Entity, &mut DirectionalLight, Ref<SunShadows>)>,
) {
    for (e, mut light, sun) in &mut lights {
        if !q.is_changed() && !sun.is_added() {
            continue;
        }
        if light.shadow_maps_enabled != q.shadows {
            light.shadow_maps_enabled = q.shadows;
        }
        let t = TUNING.get();
        let s = if q.short_shadows {
            &t.short_shadows
        } else {
            &t.long_shadows
        };
        commands.entity(e).insert(s.config());
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
    fn render_scale_steps_and_window_sizes_cycle_and_save() {
        let mut q = Preset::High.apply(GraphicsQuality::platform_default());
        assert_eq!(q.value(VideoOption::Resolution), "100%");
        // 100% wraps to the lowest step, then up through every step.
        q.cycle(VideoOption::Resolution);
        assert_eq!(q.value(VideoOption::Resolution), "50%");
        assert!(!q.sharp);
        let mut seen = vec![q.scale];
        for _ in 1..SCALES.len() {
            q.cycle(VideoOption::Resolution);
            seen.push(q.scale);
        }
        assert_eq!(seen, SCALES);
        assert!(q.sharp);
        // Low picks its own scale, and any scale reads as Custom.
        assert_eq!(Preset::Low.apply(q).scale, LOW_SCALE);
        q.cycle(VideoOption::Resolution);
        assert_eq!(q.value(VideoOption::Preset), "Custom");
        // The window size goes round the list, Auto first.
        let mut q = GraphicsQuality::platform_default();
        assert_eq!(q.value(VideoOption::WindowSize), "Auto");
        for want in ["1280x720", "1600x900", "1920x1080", "2560x1440", "Auto"] {
            q.cycle(VideoOption::WindowSize);
            assert_eq!(q.value(VideoOption::WindowSize), want);
        }
        // Both save, and files from before them still load.
        q.scale = 80;
        q.window = 3;
        assert_eq!(GraphicsQuality::from_text(&q.to_text()), Some(q));
        let old = GraphicsQuality::from_text("sharp=1\nfullscreen=0\n");
        assert_eq!(old.map(|o| (o.scale, o.window)), Some((100, 0)));
        let old = GraphicsQuality::from_text("sharp=0\n");
        assert_eq!(old.map(|o| o.scale), Some(LOW_SCALE));
        // A junk number is ignored.
        let junk = GraphicsQuality::from_text("sharp=1\nscale=75\nwindow=9\n");
        assert_eq!(junk.map(|o| (o.scale, o.window)), Some((100, 0)));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn fast_renders_fewer_pixels_and_sharp_none_less() {
        let w = UVec2::new(1280, 800);
        assert_eq!(render_scale::size(w, 1.0, 0.0), None);
        assert_eq!(render_scale::size(w, 0.7, 0.0), Some(UVec2::new(896, 560)));
        assert_eq!(render_scale::size(UVec2::ZERO, 0.5, 0.0), Some(UVec2::ONE));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn big_windows_render_at_the_pixel_budget() {
        // A window under the budget is drawn straight to the screen.
        assert_eq!(render_scale::size(UVec2::new(1920, 1080), 1.0, 2.5), None);
        // A Retina fullscreen is shrunk to the budget, same shape.
        let full = UVec2::new(3024, 1964);
        let Some(got) = render_scale::size(full, 1.0, 2.5) else {
            panic!("5.9 million pixels is over a 2.5 million budget");
        };
        let pixels = got.x as f32 * got.y as f32;
        assert!((pixels - 2.5e6).abs() < 2.0e4, "{got}");
        let shape = |v: UVec2| v.x as f32 / v.y as f32;
        assert!((shape(got) - shape(full)).abs() < 0.01);
        // Fast already under the budget keeps its own size; no cap at 0.
        assert_eq!(
            render_scale::size(UVec2::new(1280, 800), 0.7, 2.5),
            Some(UVec2::new(896, 560))
        );
        assert_eq!(render_scale::size(full, 1.0, 0.0), None);
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
        // The file says which quality it wants (a missing field would not parse).
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("ssao_quality") && text.contains("max_megapixels"));
        assert!(TUNING.get().max_megapixels >= 0.0);
        assert!((1..=16).contains(&TUNING.get().anisotropy));
        assert!(text.contains("long_shadows") && text.contains("short_shadows"));
        assert!(TUNING.get().long_shadows.count >= TUNING.get().short_shadows.count);
        assert!(TUNING.get().short_shadows.count >= 1);
        assert!(text.contains("auto_exposure"));
        let a = &TUNING.get().auto_exposure;
        assert!(a.brighten > 0.0 && a.darken >= 0.0 && a.fade_end > a.fade_start);
        assert!(a.speed_brighten > 0.0 && a.speed_darken > 0.0);
    }

    #[test]
    fn exposure_lifts_dim_rooms_but_leaves_dark_places_dark() {
        let a = AutoExposureTuning {
            outdoor: -1.5,
            brighten: 2.0,
            darken: 0.5,
            fade_start: 3.0,
            fade_end: 5.0,
            speed_brighten: 1.0,
            speed_darken: 1.0,
        };
        assert_eq!(adapt_stops(&a, a.outdoor), 0.0, "daylight stays as it is");
        assert_eq!(
            adapt_stops(&a, a.outdoor - 1.5),
            1.5,
            "a shop comes back up"
        );
        assert_eq!(adapt_stops(&a, a.outdoor - 2.8), 2.0, "capped");
        assert!(adapt_stops(&a, a.outdoor - 4.0) < 2.0, "fading");
        assert_eq!(
            adapt_stops(&a, a.outdoor - 7.0),
            0.0,
            "the warehouse stays dark"
        );
        assert_eq!(
            adapt_stops(&a, a.outdoor + 3.0),
            -0.5,
            "glare only darkens a little"
        );
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

    #[test]
    fn medium_uses_fxaa_and_shadows_cycle_long_short_off() {
        let base = GraphicsQuality::platform_default();
        let mut m = Preset::Medium.apply(base);
        assert_eq!(m.value(VideoOption::AntiAlias), "FXAA");
        assert!(!Preset::High.apply(base).short_shadows);
        assert_eq!(m.value(VideoOption::Shadows), "Long");
        m.cycle(VideoOption::Shadows);
        assert_eq!(m.value(VideoOption::Shadows), "Short");
        assert_eq!(m.preset(), None);
        m.cycle(VideoOption::Shadows);
        assert_eq!(m.value(VideoOption::Shadows), "Off");
        m.cycle(VideoOption::Shadows);
        assert_eq!(m.preset(), Some(Preset::Medium));
    }
}

/// Desktop "Fast" resolution: the main camera draws into a smaller picture,
/// and a 2D camera on the window stretches it over the screen. Being the
/// highest window camera, that one also draws the UI, at full sharpness.
/// Sharp leaves the main camera on the window, unless the window has more
/// pixels than the budget (`max_megapixels`): then it's stretched too.
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
    ///
    /// Each side is shrunk to `scale` (`0..=1`, 1 = full size); then, whatever
    /// the scale, anything over `max_megapixels` million pixels is shrunk to
    /// that many (0 = no cap).
    /// A Retina screen has four times the pixels of its size in points, and
    /// the GPU's frame is mostly shading them.
    pub(super) fn size(window: UVec2, scale: f32, max_megapixels: f32) -> Option<UVec2> {
        let mut factor = if scale > 0.0 && scale < 1.0 {
            scale
        } else {
            1.0
        };
        let pixels = window.x as f32 * window.y as f32 * factor * factor;
        let budget = max_megapixels * 1.0e6;
        if budget > 0.0 && pixels > budget {
            factor *= (budget / pixels).sqrt();
        }
        // Within a hair of the window's own size: not worth a stretch.
        if factor > 0.97 {
            return None;
        }
        Some(
            (window.as_vec2() * factor)
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
            f32::from(q.scale) / 100.0,
            TUNING.get().max_megapixels,
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
                    crate::areas::Global,
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
                    crate::areas::Global,
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
