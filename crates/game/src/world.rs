//! The bare 3D space everyone shares. Camera, light, floor, HUD: nothing else on purpose.

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_voice_chat::prelude::VoiceListener;

use crate::AppState;
use crate::net::NetStatus;
use crate::player::Player;
use crate::voice::VoiceMuted;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), (spawn_world, spawn_hud))
            .add_systems(Update, update_hud.run_if(in_state(AppState::InGame)));
    }
}

#[derive(Component)]
pub struct MainCamera;

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Camera3d::default(),
        MainCamera,
        VoiceListener { right_from: None },
        Transform::from_xyz(0.0, 8.0, 14.0).looking_at(Vec3::ZERO, Vec3::Y),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 10.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0))),
        MeshMaterial3d(materials.add(Color::srgb(0.35, 0.5, 0.35))),
        RigidBody::Static,
        Collider::cuboid(50.0, 0.1, 50.0),
        DespawnOnExit(AppState::InGame),
    ));
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
    lines.push(format!(
        "[M] mic: {}",
        if muted.0 { "muted" } else { "on" }
    ));
    let text = lines.join("\n");
    for mut t in &mut hud {
        if t.0 != text {
            t.0.clone_from(&text);
        }
    }
}
