//! The 3D space everyone shares: camera, grassy floor, meadow detail, HUD.

use crate::voice_chat::VoiceListener;
use bevy::prelude::*;

use crate::AppState;
use crate::art;
use crate::net::NetStatus;
use crate::player::Player;
pub use friendslop_base::world::*;

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

fn spawn_world(mut commands: Commands, assets: Res<AssetServer>) {
    // Sky, sun, fog and post-processing live in `atmosphere.rs`.
    commands.spawn((
        Camera3d::default(),
        MainCamera,
        VoiceListener { right_from: None },
        crate::camera::CameraRig::default().transform(),
        DespawnOnExit(AppState::InGame),
    ));
    // The ground itself lives in `terrain.rs`.
    scatter_ground_detail(&mut commands, &assets);
}

/// The meadow's flower models, one per petal colour (yellow, blue, white).
const MEADOW_FLOWERS: [&str; 3] = [
    "meadow_flower_yellow.gltf",
    "meadow_flower_blue.gltf",
    "meadow_flower_white.gltf",
];

/// Grass tufts, flowers and pebbles sprinkled around (purely visual, no colliders).
/// Deterministic, so every peer sees the same meadow. Modeler models in
/// `assets/models/nature/` (base at their origin); a missing file draws nothing.
fn scatter_ground_detail(commands: &mut Commands, assets: &AssetServer) {
    for i in 0..260 {
        let a = art::hash(i, 0, 7) * std::f32::consts::TAU;
        let r = 2.5 + art::hash(i, 1, 7).sqrt() * 21.5;
        let at = Vec3::new(a.cos() * r, 0.0, a.sin() * r);
        let kind = art::hash(i, 2, 7);
        let yaw = Quat::from_rotation_y(a * 5.0);
        let size = 0.7 + art::hash(i, 3, 7) * 0.6;
        let file = if kind < 0.65 {
            "meadow_grass.gltf"
        } else if kind < 0.88 {
            MEADOW_FLOWERS[i as usize % MEADOW_FLOWERS.len()]
        } else {
            "pebble_1.gltf"
        };
        commands.spawn((
            crate::scenery::nature_model(assets, file),
            Transform::from_translation(at)
                .with_rotation(yaw)
                .with_scale(Vec3::splat(size)),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

#[derive(Component)]
struct HudText;

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        HudText,
        // On Tab: the team strip shows who's here, the mic chip your mic.
        crate::hud::slot(crate::hud::HudSlot::Scoreboard),
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_hud(
    status: Res<NetStatus>,
    players: Query<(), With<Player>>,
    mut hud: Query<&mut Text, With<HudText>>,
) {
    let n = players.iter().count();
    let mut lines = vec![format!(
        "{n} {}  |  H help",
        if n == 1 { "player" } else { "players" },
    )];
    if let Some(code) = &status.join_code {
        lines.push(format!(
            "join code {}  (C copies)",
            crate::hud::short_code(code)
        ));
    }
    let text = lines.join("\n");
    for mut t in &mut hud {
        if t.0 != text {
            t.0.clone_from(&text);
        }
    }
}
