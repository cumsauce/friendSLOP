//! Sky, sun, ambient fill, distance fog and post-processing: the warm storybook look.
//! Purely local visuals, nothing replicated.
//!
//! - A big unlit sky dome with a vertex-colour gradient (horizon haze to deep blue),
//!   hand-picked to stay a cat-visible (#12) bright blue.
//! - Puffy code-made clouds (clusters of squashed spheres) drifting very slowly.
//! - One warm shadow-casting sun tuned for the ~60 m play area, a cool shadowless
//!   bounce light and a sky-tinted ambient.
//! - Fog in the exact horizon colour, so the edge of the world melts into the sky.
//! - TonyMcMapface tonemapping and mild bloom so the bonfire and emissives glow.

use bevy::asset::RenderAssetUsages;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;

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
        .add_systems(OnEnter(AppState::InGame), spawn_atmosphere)
        .add_systems(Update, drift_clouds.run_if(in_state(AppState::InGame)));
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
    commands.entity(add.entity).insert((
        Tonemapping::TonyMcMapface,
        Bloom {
            intensity: 0.08,
            ..Bloom::NATURAL
        },
        DistanceFog {
            color: Color::LinearRgba(hdr(horizon_colour())),
            directional_light_color: Color::srgba(1.0, 0.9, 0.7, 0.35),
            directional_light_exponent: 24.0,
            falloff: FogFalloff::Linear {
                start: 45.0,
                end: 140.0,
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
    // Warm late-afternoon sun. Three cascades over 80 m keep shadows crisp up close
    // and cheap (1 shadow-casting light only).
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.95, 0.84),
            illuminance: 14_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 3,
            minimum_distance: 0.1,
            first_cascade_far_bound: 14.0,
            maximum_distance: 80.0,
            overlap_proportion: 0.2,
        }
        .build(),
        Transform::from_xyz(8.0, 10.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        DespawnOnExit(AppState::InGame),
    ));
    // Cool shadowless bounce from the opposite side.
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(0.65, 0.78, 1.0),
            illuminance: 2_200.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-6.0, 3.0, -4.0).looking_at(Vec3::ZERO, Vec3::Y),
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
    for i in 0..14i32 {
        let a = art::hash(i, 0, 31) * std::f32::consts::TAU;
        let r = 100.0 + art::hash(i, 1, 31) * 100.0;
        let at = Vec3::new(a.cos() * r, 16.0 + art::hash(i, 2, 31) * 14.0, a.sin() * r);
        let scale = 4.0 + art::hash(i, 3, 31) * 4.0;
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
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
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
