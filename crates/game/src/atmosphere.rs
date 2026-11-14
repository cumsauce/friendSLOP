//! Sky, sun, ambient fill, distance fog and post-processing: the warm storybook look.
//! Purely local visuals, nothing replicated.
//!
//! - A big unlit sky dome with a vertex-colour gradient (horizon haze to deep blue),
//!   hand-picked to stay a cat-visible (#12) bright blue.
//! - Puffy code-made clouds (clusters of squashed spheres) drifting very slowly.
//! - One low, golden shadow-casting sun (#131) tuned for the ~60 m play area, a cool shadowless
//!   bounce light and a sky-tinted ambient.
//! - Fog in the exact horizon colour, so the edge of the world melts into the sky.
//! - A day/night cycle (#284) counted from when the host started (the game opens
//!   in the morning), so everyone shares the time of day: the sun
//!   swings across the sky and goes golden at dusk; the dome blends day, sunset and night colours,
//!   stars fade in, a moon rises and the clouds go dark (`Darkness` tells the rest of the game).
//! - TonyMcMapface tonemapping and mild bloom so the bonfire and emissives glow.

use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

use bevy_replicon::prelude::*;

use crate::AppState;
use crate::art;
use crate::features::cat_palette::{ExactColors, exact_material};
use crate::world::MainCamera;

const SKY_RADIUS: f32 = 400.0;
/// Sky colours are picked by hand instead of run through `cat_palette`: its
/// shadow lift turns every blue into pale lavender. Red and green stay within
/// 0.1 of each other, the same tolerance the cat palette is tested to, so the
/// sky is still a blue a cat can see (#12).
const HORIZON: Color = Color::srgb(0.74, 0.82, 0.96);
const ZENITH: Color = Color::srgb(0.3, 0.4, 0.95);
/// Clouds wrap around after drifting this far along +X.
const CLOUD_WRAP: f32 = 260.0;

pub fn plugin(app: &mut App) {
    app.add_observer(dress_camera)
        .init_resource::<SkyDrawn>()
        .init_resource::<friendslop_base::rules::daylight::Darkness>()
        .add_systems(OnEnter(AppState::InGame), spawn_atmosphere)
        .add_systems(
            Update,
            (drift_clouds, day_night.after(crate::place::PlaceSet))
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnEnter(ServerState::Running), start_day);
}

/// Registered on every peer (appended last in the features protocol list).
pub fn protocol(app: &mut App) {
    app.replicate::<DayStart>();
}

/// When the host's session began (seconds since the epoch), replicated so every peer
/// counts the day from the same moment and the game opens in the morning.
#[derive(Component, serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct DayStart(pub f64);

fn start_day(mut commands: Commands, existing: Query<(), With<DayStart>>) {
    if existing.is_empty() {
        commands.spawn((
            Replicated,
            DayStart(crate::settings::since_epoch().as_secs_f64()),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Day/night tuning: `assets/data/game/day_night.ron` (read at run time).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct DayNight {
    /// How long one whole day lasts, in minutes.
    pub day_minutes: f32,
    /// How bright the night still is, 0..1.
    pub night_brightness: f32,
    /// The hour of day (0..24) a new game starts at.
    pub start_hour: f32,
    /// Sky colours (sRGB) at the horizon and straight up: by day, at sunset
    /// (strongest on the sun's side) and at night.
    pub day_sky: SkyPair,
    pub sunset_sky: SkyPair,
    pub night_sky: SkyPair,
    /// How many stars, and how big each is (m, on a 390 m shell).
    pub stars: u32,
    pub star_size: f32,
    /// How much of the clouds' daytime glow is left at night, 0..1.
    pub cloud_night: f32,
    /// The moon's radius (m, 380 m away).
    pub moon_size: f32,
}

/// A horizon and a zenith colour (sRGB).
#[derive(serde::Deserialize, Default, Debug, Clone, Copy)]
pub struct SkyPair {
    pub horizon: [f32; 3],
    pub zenith: [f32; 3],
}

pub static DAY_NIGHT: friendslop_data::Data<DayNight> =
    friendslop_data::Data::ron("data/game/day_night.ron");

/// Time of day 0..1 (0 = midnight, 0.25 = dawn, 0.5 = noon, 0.75 = dusk) from the seconds
/// since the session started, beginning at `start_hour` (#284). Every peer counts from the
/// host's [`DayStart`], so they agree.
pub fn time_of_day(elapsed_secs: f64, day_minutes: f32, start_hour: f32) -> f32 {
    let len = (day_minutes.max(0.1) as f64) * 60.0;
    let start = (start_hour as f64 / 24.0).rem_euclid(1.0);
    ((elapsed_secs.rem_euclid(len) / len + start).rem_euclid(1.0)) as f32
}

/// How high the sun is: -1 at midnight, 0 at dawn and dusk, 1 at noon.
pub fn sun_height(t: f32) -> f32 {
    -(t * std::f32::consts::TAU).cos()
}

/// Daylight 0..1 from the sun's height, with a soft twilight band around the horizon.
pub fn daylight(height: f32) -> f32 {
    let x = ((height + 0.15) / 0.45).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// How strongly the sky turns sunset-coloured: 1 with the sun on the horizon,
/// 0 once it's well up or well down.
pub fn dusk(height: f32) -> f32 {
    (1.0 - (height + 0.05).abs() * 3.5).clamp(0.0, 1.0)
}

fn srgb(c: [f32; 3]) -> Color {
    Color::srgb(c[0], c[1], c[2])
}

/// The sky's horizon and zenith colours at time of day `t`, `toward_sun` 0..1
/// saying how much this part of the sky faces the sun (sunsets glow on that side).
pub fn sky_colors(t: f32, toward_sun: f32, tuning: &DayNight) -> (Color, Color) {
    let h = sun_height(t);
    let day = daylight(h);
    let glow = dusk(h) * (0.35 + 0.65 * toward_sun.clamp(0.0, 1.0));
    let pick = |f: fn(&SkyPair) -> [f32; 3], sunset_share: f32| {
        let base = srgb(f(&tuning.night_sky)).mix(&srgb(f(&tuning.day_sky)), day);
        base.mix(&srgb(f(&tuning.sunset_sky)), glow * sunset_share)
    };
    (pick(|p| p.horizon, 1.0), pick(|p| p.zenith, 0.45))
}

/// Seeded star directions (upper sky only) and brightness, the same on every peer.
pub fn stars(count: u32) -> Vec<(Vec3, f32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while out.len() < count as usize && i < count as i32 * 4 {
        let a = art::hash(i, 0, 77) * std::f32::consts::TAU;
        let y = art::hash(i, 1, 77);
        i += 1;
        if y < 0.06 {
            continue;
        }
        let r = (1.0 - y * y).sqrt();
        let bright = 0.35 + 0.65 * art::hash(i, 2, 77).powf(3.0);
        out.push((Vec3::new(r * a.cos(), y, r * a.sin()), bright));
    }
    out
}

/// The scene's base values, so the cycle scales them instead of drifting.
const SUN_LUX: f32 = 15_000.0;
const BOUNCE_LUX: f32 = 2_600.0;
const AMBIENT: f32 = 1_100.0;

#[derive(Component)]
struct Sun;
#[derive(Component)]
struct Bounce;
#[derive(Component)]
struct SkyDome;
#[derive(Component)]
struct Stars;
#[derive(Component)]
struct Moon;
/// The clouds' shared material and its daytime glow.
#[derive(Resource)]
struct CloudLook(Handle<StandardMaterial>, LinearRgba);
/// The time of day the sky's colours were last worked out for.
#[derive(Resource, Default)]
struct SkyDrawn(Option<f32>);

/// Swing the sun over the sky and dim light, sky and fog at night, then lay
/// the look of the place you're in over it (`place.rs`). This is the only
/// system that writes the sun's and bounce's brightness, the ambient light
/// and the camera's fog: nothing is saved and put back, so leaving places in
/// any order always comes back to the right sky.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn day_night(
    mut commands: Commands,
    mut sun: Query<
        (&mut DirectionalLight, &mut Transform),
        (With<Sun>, Without<Bounce>, Without<Moon>),
    >,
    mut bounce: Query<&mut DirectionalLight, (With<Bounce>, Without<Sun>)>,
    ambient: Option<ResMut<GlobalAmbientLight>>,
    mut fog: Query<(Entity, Option<&mut DistanceFog>), With<MainCamera>>,
    started: Query<&DayStart>,
    mut darkness: ResMut<friendslop_base::rules::daylight::Darkness>,
    places: Res<crate::place::PlaceLooks>,
    quality: Option<Res<crate::graphics::GraphicsQuality>>,
    mut sky: SkyParts,
) {
    let Ok((mut light, mut tf)) = sun.single_mut() else {
        return;
    };
    let tuning = DAY_NIGHT.get();
    let now = crate::settings::since_epoch().as_secs_f64();
    let start = started.iter().next().map_or(now, |d| d.0);
    let t = time_of_day(now - start, tuning.day_minutes, tuning.start_hour);
    let h = sun_height(t);
    let day = daylight(h);
    let dark = 1.0 - day;
    if (darkness.0 - dark).abs() > 1e-4 {
        darkness.0 = dark;
    }
    let night = tuning.night_brightness.clamp(0.0, 1.0);
    let lit = night + (1.0 - night) * day;
    // Golden near the horizon, white at noon; the night "sun" is a cool moon from the same side.
    let warm = Color::srgb(1.0, 0.9, 0.55).mix(&Color::srgb(1.0, 0.92, 0.8), h.clamp(0.0, 1.0));
    light.color = warm.mix(&Color::srgb(0.55, 0.65, 1.0), 1.0 - day);
    // Sun travels east to west; at night the moon takes the opposite arc. Never lower than
    // ~10 degrees so shadows stay sane.
    let a = t * std::f32::consts::TAU;
    let dir = if h >= 0.0 {
        Vec3::new(-a.sin(), h.max(0.18), 0.45)
    } else {
        Vec3::new(a.sin(), (-h).max(0.18), 0.45)
    };
    *tf = Transform::from_translation(dir.normalize() * 10.0).looking_at(Vec3::ZERO, Vec3::Y);
    // Outdoors: the fog is the sky's horizon, so the world's edge melts into it at any hour.
    let (horizon, _) = sky_colors(t, 0.5, &tuning);
    let (fog_start, fog_end) = if quality.as_ref().is_none_or(|q| q.far) {
        (60.0, 230.0)
    } else {
        (40.0, 140.0)
    };
    let outdoor = crate::place::Light {
        sun: (SUN_LUX * lit).max(1.0),
        ambient: AMBIENT * lit,
        fog_color: Color::LinearRgba(hdr(horizon)),
        fog_start,
        fog_end,
        indoor_fog: false,
    };
    let looks = crate::place::LOOKS.get();
    let here = crate::place::pick(places.ids(), &looks).map(|(_, l)| l);
    let lights = crate::place::blend(outdoor, here);
    set(&mut light.illuminance, lights.sun);
    // The bounce follows the sun (out with it in a dark place).
    let bounce_lux = BOUNCE_LUX * lit * lights.sun / outdoor.sun;
    for mut b in &mut bounce {
        set(&mut b.illuminance, bounce_lux);
    }
    if let Some(mut ambient) = ambient
        && ambient.brightness != lights.ambient
    {
        ambient.brightness = lights.ambient;
    }
    let want = DistanceFog {
        color: lights.fog_color,
        directional_light_color: if lights.indoor_fog {
            Color::NONE
        } else {
            Color::srgba(1.0, 0.9, 0.7, 0.35)
        },
        directional_light_exponent: 24.0,
        falloff: FogFalloff::Linear {
            start: lights.fog_start,
            end: lights.fog_end,
        },
    };
    for (camera, f) in &mut fog {
        match f {
            Some(mut f) => {
                if f.color != want.color
                    || f.directional_light_color != want.directional_light_color
                    || !matches!(f.falloff, FogFalloff::Linear { start, end }
                        if start == lights.fog_start && end == lights.fog_end)
                {
                    *f = want.clone();
                }
            }
            None => {
                commands.entity(camera).insert(want.clone());
            }
        }
    }
    let sun_dir = Vec3::new(-a.sin(), h, 0.45).normalize();
    sky.draw(t, sun_dir, &tuning);
}

/// Writes `value` only when it changed (so change detection stays quiet).
fn set(slot: &mut f32, value: f32) {
    if *slot != value {
        *slot = value;
    }
}

/// Everything in the sky that changes with the hour.
#[derive(bevy::ecs::system::SystemParam)]
struct SkyParts<'w, 's> {
    dome: Query<'w, 's, &'static Mesh3d, With<SkyDome>>,
    stars: Query<
        'w,
        's,
        (
            &'static MeshMaterial3d<StandardMaterial>,
            &'static mut Visibility,
        ),
        (With<Stars>, Without<Moon>),
    >,
    moon: Query<
        'w,
        's,
        (&'static mut Transform, &'static mut Visibility),
        (With<Moon>, Without<Stars>),
    >,
    clouds: Option<Res<'w, CloudLook>>,
    drawn: ResMut<'w, SkyDrawn>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
}

impl SkyParts<'_, '_> {
    /// Recolour the dome, fade the stars, place the moon and dim the clouds,
    /// once the hour has moved on a little (a full day is ~2000 steps).
    fn draw(&mut self, t: f32, sun_dir: Vec3, tuning: &DayNight) {
        if self.drawn.0.is_some_and(|d| (d - t).abs() < 0.0005) {
            return;
        }
        self.drawn.0 = Some(t);
        let h = sun_height(t);
        let day = daylight(h);
        let sun_flat = Vec2::new(sun_dir.x, sun_dir.z).normalize_or_zero();
        for m in &self.dome {
            let Some(mut mesh) = self.meshes.get_mut(&m.0) else {
                continue;
            };
            let Some(pos) = mesh
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .and_then(|a| a.as_float3())
                .map(|p| p.to_vec())
            else {
                continue;
            };
            let col: Vec<[f32; 4]> = pos
                .iter()
                .map(|p| {
                    let d = Vec3::from_array(*p) / SKY_RADIUS;
                    let toward = Vec2::new(d.x, d.z)
                        .normalize_or_zero()
                        .dot(sun_flat)
                        .max(0.0);
                    let (horizon, zenith) = sky_colors(t, toward * toward, tuning);
                    dome_colour(d.y, horizon, zenith).to_f32_array()
                })
                .collect();
            mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
        }
        let star_alpha = ((1.0 - day) * 1.15 - 0.15).clamp(0.0, 1.0);
        for (m, mut vis) in &mut self.stars {
            let want = if star_alpha > 0.01 {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *vis != want {
                *vis = want;
            }
            if let Some(mut mat) = self.materials.get_mut(&m.0) {
                mat.base_color = Color::srgba(1.0, 1.0, 1.0, star_alpha);
            }
        }
        // The moon rides opposite the sun.
        let moon_dir = -sun_dir;
        for (mut tf, mut vis) in &mut self.moon {
            let up = moon_dir.y > -0.05;
            let want = if up {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if *vis != want {
                *vis = want;
            }
            tf.translation = moon_dir * 380.0;
            tf.scale = Vec3::splat(tuning.moon_size.max(0.1));
        }
        if let Some(clouds) = &self.clouds
            && let Some(mut mat) = self.materials.get_mut(&clouds.0)
        {
            let keep = tuning.cloud_night + (1.0 - tuning.cloud_night) * day;
            let warm = Color::srgb(1.0, 0.72, 0.5).to_linear();
            let w = dusk(h) * 0.8;
            let base = clouds.1;
            let mix = |c: f32, k: f32| c * keep * (1.0 - w + w * k);
            mat.emissive = LinearRgba::rgb(
                mix(base.red, warm.red),
                mix(base.green, warm.green),
                mix(base.blue, warm.blue),
            );
            mat.base_color = Color::srgb(0.03, 0.035, 0.06).mix(&Color::WHITE, day);
        }
    }
}

/// The dome's colour at height `y` (-1..1) between the horizon and the zenith.
fn dome_colour(y: f32, horizon: Color, zenith: Color) -> LinearRgba {
    let mix = y.max(0.0).powf(0.45);
    hdr(horizon).mix(&hdr(zenith), mix)
}

/// The colour where sky meets ground; fog uses it so the world edge disappears.
pub fn horizon_colour() -> Color {
    HORIZON
}

/// Unlit sky and fog colours skip exposure, and TonyMcMapface squashes 1.0 down to a
/// dull grey. Push them into HDR so they come out of tonemapping as the bright
/// storybook colours we asked for.
const SKY_BOOST: f32 = 1.3;

fn hdr(c: Color) -> LinearRgba {
    let l = c.to_linear();
    LinearRgba::new(
        l.red * SKY_BOOST,
        l.green * SKY_BOOST,
        l.blue * SKY_BOOST,
        l.alpha,
    )
}

/// Give the main camera tonemapping, bloom and horizon-matched fog.
fn dress_camera(add: On<Add<MainCamera>>, mut commands: Commands) {
    let mut camera = commands.entity(add.entity);
    // Without `Hdr`, bloom is a silent no-op (tonemapping then runs in the
    // material shaders). Browsers skip both (#242, `graphics::HDR_SUPPORTED`).
    if crate::graphics::HDR_SUPPORTED {
        camera.insert((
            bevy::camera::Hdr,
            Bloom {
                intensity: 0.08,
                ..Bloom::NATURAL
            },
        ));
    }
    camera.insert((
        Tonemapping::TonyMcMapface,
        DistanceFog {
            color: Color::LinearRgba(hdr(horizon_colour())),
            directional_light_color: Color::srgba(1.0, 0.9, 0.7, 0.35),
            directional_light_exponent: 24.0,
            falloff: FogFalloff::Linear {
                start: 60.0,
                end: 230.0,
            },
        },
    ));
}

#[derive(Component)]
struct Cloud {
    speed: f32,
}

fn spawn_atmosphere(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut exact: ResMut<ExactColors>,
) {
    // Warm, sky-tinted ambient so shadowed sides read soft blue, never black.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.78, 0.86, 1.0),
        brightness: 1_100.0,
        ..default()
    });
    // Golden-hour sun (#131): lower and warmer, so shadows stretch long across
    // the meadow and faces facing it glow. Three cascades over 80 m keep shadows crisp up close
    // and cheap (1 shadow-casting light only; every caster is drawn once per cascade, and a
    // fourth cascade cost more frame time than any other video option).
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.88, 0.7),
            illuminance: 15_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 3,
            minimum_distance: 0.1,
            first_cascade_far_bound: 12.0,
            maximum_distance: 80.0,
            overlap_proportion: 0.2,
        }
        .build(),
        crate::graphics::SunShadows,
        Sun,
        Transform::from_xyz(9.0, 6.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        DespawnOnExit(AppState::InGame),
    ));
    // Cool shadowless bounce from the opposite side.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.6, 0.74, 1.0),
            illuminance: 2_600.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-6.0, 3.0, -4.0).looking_at(Vec3::ZERO, Vec3::Y),
        Bounce,
        DespawnOnExit(AppState::InGame),
    ));

    // Sky dome: unlit, fogless, shadowless, viewed from inside.
    let sky = exact_material(
        &mut materials,
        &mut exact,
        StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            fog_enabled: false,
            cull_mode: None,
            ..default()
        },
    );
    commands.spawn((
        Mesh3d(meshes.add(sky_dome())),
        MeshMaterial3d(sky),
        SkyDome,
        NotShadowCaster,
        NotShadowReceiver,
        DespawnOnExit(AppState::InGame),
    ));
    commands.insert_resource(SkyDrawn(None));

    // Stars and the moon: unlit, fogless, faded in by `day_night` as it gets dark.
    let tuning = DAY_NIGHT.get();
    let night_sky = |materials: &mut Assets<StandardMaterial>, alpha_mode| {
        materials.add(StandardMaterial {
            base_color: Color::WHITE,
            unlit: true,
            fog_enabled: false,
            alpha_mode,
            ..default()
        })
    };
    commands.spawn((
        Mesh3d(meshes.add(star_field(tuning.stars, tuning.star_size))),
        MeshMaterial3d(night_sky(&mut materials, AlphaMode::Add)),
        Stars,
        Visibility::Hidden,
        NotShadowCaster,
        NotShadowReceiver,
        DespawnOnExit(AppState::InGame),
    ));
    let moon = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.92, 1.0),
        emissive: LinearRgba::rgb(2.2, 2.3, 2.6),
        unlit: true,
        fog_enabled: false,
        ..default()
    });
    commands.spawn((
        Mesh3d(meshes.add(Sphere::new(1.0).mesh().uv(24, 12))),
        MeshMaterial3d(moon),
        Moon,
        Transform::from_xyz(0.0, -380.0, 0.0),
        Visibility::Hidden,
        NotShadowCaster,
        NotShadowReceiver,
        DespawnOnExit(AppState::InGame),
    ));

    // Puffy clouds: a few spheres per cloud, soft white, lit only by emissive so
    // they stay bright and cost nothing in shadows.
    let puff = meshes.add(
        Sphere::new(1.0)
            .mesh()
            .ico(2)
            .unwrap_or_else(|_| Sphere::new(1.0).into()),
    );
    let white = exact_material(
        &mut materials,
        &mut exact,
        StandardMaterial {
            base_color: Color::srgb(1.0, 1.0, 1.0),
            emissive: LinearRgba::rgb(1.6, 1.65, 1.75),
            perceptual_roughness: 1.0,
            reflectance: 0.0,
            fog_enabled: false,
            ..default()
        },
    );
    if let Some(m) = materials.get(&white) {
        commands.insert_resource(CloudLook(white.clone(), m.emissive));
    }
    for i in 0..14i32 {
        let a = art::hash(i, 0, 31) * std::f32::consts::TAU;
        let r = 130.0 + art::hash(i, 1, 31) * 130.0;
        let at = Vec3::new(a.cos() * r, 30.0 + art::hash(i, 2, 31) * 20.0, a.sin() * r);
        let scale = 6.0 + art::hash(i, 3, 31) * 5.0;
        commands
            .spawn((
                Transform::from_translation(at).with_scale(Vec3::splat(scale)),
                Visibility::default(),
                Cloud {
                    speed: 0.4 + art::hash(i, 4, 31) * 0.5,
                },
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                let n = 4 + (art::hash(i, 5, 31) * 3.0) as i32;
                for k in 0..n {
                    let t = k as f32 / (n - 1) as f32 - 0.5;
                    let s = 1.0 + (1.0 - (2.0 * t).abs()) * 0.8 + art::hash(i, 10 + k, 31) * 0.3;
                    p.spawn((
                        Mesh3d(puff.clone()),
                        MeshMaterial3d(white.clone()),
                        Transform::from_xyz(t * 4.0, s * 0.35, art::hash(i, 20 + k, 31) - 0.5)
                            .with_scale(Vec3::new(s * 1.2, s * 0.85, s)),
                        NotShadowCaster,
                        NotShadowReceiver,
                    ));
                }
            });
    }
}

/// UV-sphere-ish dome with a vertical colour gradient baked into vertex colours.
fn sky_dome() -> Mesh {
    const RINGS: u32 = 16;
    const SEGS: u32 = 32;
    let horizon = hdr(horizon_colour());
    let zenith = hdr(ZENITH);
    let mut pos = Vec::new();
    let mut col = Vec::new();
    let mut nrm = Vec::new();
    for ring in 0..=RINGS {
        // From a little below the horizon (-0.15 rad) up to the zenith.
        let v = ring as f32 / RINGS as f32;
        let elev = -0.15 + v * (std::f32::consts::FRAC_PI_2 + 0.15);
        let h = elev.sin().max(0.0);
        let mix = h.powf(0.45);
        let c = horizon.mix(&zenith, mix);
        for seg in 0..=SEGS {
            let az = seg as f32 / SEGS as f32 * std::f32::consts::TAU;
            let d = Vec3::new(elev.cos() * az.cos(), elev.sin(), elev.cos() * az.sin());
            pos.push((d * SKY_RADIUS).to_array());
            nrm.push((-d).to_array());
            col.push(c.to_f32_array());
        }
    }
    let mut idx = Vec::new();
    for ring in 0..RINGS {
        for seg in 0..SEGS {
            let a = ring * (SEGS + 1) + seg;
            let b = a + SEGS + 1;
            idx.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    // Kept in the main world too: `day_night` recolours it through the day.
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
    .with_inserted_indices(Indices::U32(idx))
}

/// Little quads on a shell just inside the dome, facing in, one per star.
fn star_field(count: u32, size: f32) -> Mesh {
    let mut pos = Vec::new();
    let mut col = Vec::new();
    let mut idx = Vec::new();
    for (d, bright) in stars(count) {
        let side = d.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = side.cross(d);
        let s = size * (0.6 + 0.6 * bright);
        let at = d * (SKY_RADIUS - 10.0);
        let n = pos.len() as u32;
        for (a, b) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            pos.push((at + side * a * s + up * b * s).to_array());
            col.push([bright * 2.0, bright * 2.0, bright * 2.2, 1.0]);
        }
        idx.extend_from_slice(&[n, n + 1, n + 2, n, n + 2, n + 3]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
    .with_inserted_indices(Indices::U32(idx))
}

fn drift_clouds(time: Res<Time>, mut clouds: Query<(&mut Transform, &Cloud)>) {
    let dt = time.delta_secs();
    for (mut tf, cloud) in &mut clouds {
        tf.translation.x += cloud.speed * dt;
        if tf.translation.x > CLOUD_WRAP {
            tf.translation.x -= 2.0 * CLOUD_WRAP;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noon_is_bright_and_midnight_dark() {
        assert!(daylight(sun_height(0.5)) > 0.99);
        assert!(daylight(sun_height(0.0)) < 0.01);
        assert!((time_of_day(90.0, 1.0, 0.0) - 0.5).abs() < 1e-6);
    }

    fn tuning() -> DayNight {
        let pair = |h: [f32; 3], z: [f32; 3]| SkyPair {
            horizon: h,
            zenith: z,
        };
        DayNight {
            day_minutes: 24.0,
            night_brightness: 0.1,
            start_hour: 9.0,
            day_sky: pair([0.74, 0.82, 0.96], [0.3, 0.4, 0.95]),
            sunset_sky: pair([1.0, 0.55, 0.3], [0.5, 0.35, 0.6]),
            night_sky: pair([0.05, 0.07, 0.15], [0.01, 0.015, 0.05]),
            stars: 300,
            star_size: 0.6,
            cloud_night: 0.05,
            moon_size: 9.0,
        }
    }

    #[test]
    fn the_sky_is_blue_by_day_red_at_dusk_and_dark_at_night() {
        let t = tuning();
        let lum = |c: Color| c.to_linear().red + c.to_linear().green + c.to_linear().blue;
        let (noon, _) = sky_colors(0.5, 0.0, &t);
        let (midnight, top) = sky_colors(0.0, 0.0, &t);
        let (sunset, _) = sky_colors(0.75, 1.0, &t);
        assert!(lum(midnight) < lum(noon) * 0.1);
        assert!(lum(top) < lum(midnight));
        let s = sunset.to_srgba();
        assert!(s.red > s.blue, "sunset {s:?}");
        let n = noon.to_srgba();
        assert!(n.blue > n.red);
    }

    #[test]
    fn stars_are_seeded_and_up_in_the_sky() {
        let a = stars(300);
        assert_eq!(a.len(), 300);
        assert!(
            a.iter()
                .all(|(d, _)| d.y > 0.05 && (d.length() - 1.0).abs() < 1e-3)
        );
        assert_eq!(a, stars(300));
    }

    #[test]
    fn a_new_game_starts_at_the_start_hour() {
        assert!((time_of_day(0.0, 24.0, 9.5) - 9.5 / 24.0).abs() < 1e-6);
        // Half a day later it's half past nine at night.
        assert!((time_of_day(12.0 * 60.0, 24.0, 9.5) - 21.5 / 24.0).abs() < 1e-5);
    }
}
