//! The 3D space everyone shares: camera, sun, sky haze, grassy floor, meadow detail, HUD.

use avian3d::prelude::*;
use bevy::light::CascadeShadowConfigBuilder;
use bevy::prelude::*;
use bevy_voice_chat::prelude::VoiceListener;

use crate::AppState;
use crate::art;
use crate::net::NetStatus;
use crate::player::Player;
use crate::voice::VoiceMuted;

/// How often HUD text is rebuilt (5 Hz, on real time so it keeps updating
/// while the world stands still). Formatting strings every frame is waste.
pub const HUD_PERIOD: std::time::Duration = std::time::Duration::from_millis(200);

/// Run condition for HUD update systems: true every `HUD_PERIOD`.
pub fn hud_tick() -> impl FnMut(Res<Time<Real>>) -> bool + Clone {
    bevy::time::common_conditions::on_real_timer(HUD_PERIOD)
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), (spawn_world, spawn_hud))
            .add_systems(
                Update,
                update_hud
                    .run_if(in_state(AppState::InGame))
                    .run_if(hud_tick()),
            );
    }
}

#[derive(Component)]
pub struct MainCamera;

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    // Soft distance haze in the (cat-visible) sky colour so the world edge fades away.
    let haze = crate::features::cat_palette::cat_palette(Color::srgb(0.6, 0.82, 1.0));
    commands.spawn((
        Camera3d::default(),
        MainCamera,
        VoiceListener { right_from: None },
        Transform::from_xyz(0.0, 8.0, 14.0).looking_at(Vec3::ZERO, Vec3::Y),
        DistanceFog {
            color: haze,
            directional_light_color: Color::srgba(1.0, 0.95, 0.8, 0.4),
            directional_light_exponent: 20.0,
            falloff: FogFalloff::Linear {
                start: 28.0,
                end: 70.0,
            },
        },
        DespawnOnExit(AppState::InGame),
    ));
    // Bright sky fill so shadows stay soft and cute rather than black.
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.75, 0.85, 1.0),
        brightness: 600.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.96, 0.88),
            illuminance: 9_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        CascadeShadowConfigBuilder {
            num_cascades: 2,
            first_cascade_far_bound: 15.0,
            maximum_distance: 45.0,
            ..default()
        }
        .build(),
        Transform::from_xyz(6.0, 12.0, 3.0).looking_at(Vec3::ZERO, Vec3::Y),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.5, 0.35),
            base_color_texture: Some(art::grass_texture(&mut images)),
            uv_transform: bevy::math::Affine2::from_scale(Vec2::splat(10.0)),
            perceptual_roughness: 0.95,
            reflectance: 0.15,
            ..default()
        })),
        RigidBody::Static,
        Collider::cuboid(50.0, 0.1, 50.0),
        DespawnOnExit(AppState::InGame),
    ));
    scatter_ground_detail(&mut commands, &mut meshes, &mut materials);
}

/// Grass tufts, flowers and pebbles sprinkled around (purely visual, no colliders).
/// Deterministic, so every peer sees the same meadow.
fn scatter_ground_detail(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let blade = meshes.add(Cone::new(0.06, 0.35));
    let petal = meshes.add(Sphere::new(0.07));
    let stem = meshes.add(Cylinder::new(0.012, 0.25));
    let pebble = meshes.add(
        Sphere::new(0.18)
            .mesh()
            .ico(1)
            .unwrap_or_else(|_| Sphere::new(0.18).into()),
    );
    let grass = art::soft(materials, Color::srgb(0.3, 0.55, 0.28));
    let stone = art::soft(materials, Color::srgb(0.6, 0.6, 0.65));
    let flowers = [
        art::soft(materials, Color::srgb(1.0, 0.95, 0.4)),
        art::soft(materials, Color::srgb(0.55, 0.6, 1.0)),
        art::soft(materials, Color::srgb(1.0, 1.0, 1.0)),
    ];
    for i in 0..260 {
        let a = art::hash(i, 0, 7) * std::f32::consts::TAU;
        let r = 2.5 + art::hash(i, 1, 7).sqrt() * 21.5;
        let at = Vec3::new(a.cos() * r, 0.0, a.sin() * r);
        let kind = art::hash(i, 2, 7);
        let yaw = Quat::from_rotation_y(a * 5.0);
        let size = 0.7 + art::hash(i, 3, 7) * 0.6;
        let mut root = commands.spawn((
            Transform::from_translation(at)
                .with_rotation(yaw)
                .with_scale(Vec3::splat(size)),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ));
        if kind < 0.65 {
            root.with_children(|p| {
                for (dx, tilt) in [(-0.07, -0.25), (0.0, 0.05), (0.07, 0.3)] {
                    p.spawn((
                        Mesh3d(blade.clone()),
                        MeshMaterial3d(grass.clone()),
                        Transform::from_xyz(dx, 0.15, 0.0)
                            .with_rotation(Quat::from_rotation_z(tilt)),
                    ));
                }
            });
        } else if kind < 0.88 {
            let colour = flowers[i as usize % flowers.len()].clone();
            root.with_children(|p| {
                p.spawn((
                    Mesh3d(stem.clone()),
                    MeshMaterial3d(grass.clone()),
                    Transform::from_xyz(0.0, 0.12, 0.0),
                ));
                p.spawn((
                    Mesh3d(petal.clone()),
                    MeshMaterial3d(colour),
                    Transform::from_xyz(0.0, 0.27, 0.0).with_scale(Vec3::new(1.2, 0.5, 1.2)),
                ));
            });
        } else {
            root.with_children(|p| {
                p.spawn((
                    Mesh3d(pebble.clone()),
                    MeshMaterial3d(stone.clone()),
                    Transform::from_xyz(0.0, 0.04, 0.0).with_scale(Vec3::new(1.3, 0.5, 1.0)),
                ));
            });
        }
    }
}

#[derive(Component)]
struct HudText;

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        HudText,
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            left: px(12),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_hud(
    status: Res<NetStatus>,
    muted: Res<VoiceMuted>,
    players: Query<(), With<Player>>,
    mut hud: Query<&mut Text, With<HudText>>,
) {
    let mut lines = vec![format!("players: {}", players.iter().count())];
    if let Some(code) = &status.join_code {
        lines.push(format!("join code: {code}"));
    }
    lines.push(format!("[M] mic: {}", if muted.0 { "muted" } else { "on" }));
    let text = lines.join("\n");
    for mut t in &mut hud {
        if t.0 != text {
            t.0.clone_from(&text);
        }
    }
}
