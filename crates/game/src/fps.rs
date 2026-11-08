//! Developer diagnostics: frame time measurement and an FPS overlay (F3).

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::world::hud_tick;

pub struct FpsPlugin;

impl Plugin for FpsPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.add_systems(Startup, spawn_overlay)
            .add_systems(Update, (toggle_overlay, update_overlay.run_if(hud_tick())));
    }
}

#[derive(Component)]
struct FpsText;

fn spawn_overlay(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(14.0),
        TextColor(Color::srgb(0.4, 1.0, 0.4)),
        FpsText,
        Visibility::Hidden,
        GlobalZIndex(100),
        Node {
            position_type: PositionType::Absolute,
            top: px(4),
            right: px(4),
            ..default()
        },
    ));
}

fn toggle_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    mut overlay: Query<&mut Visibility, With<FpsText>>,
) {
    if !keys.just_pressed(KeyCode::F3) {
        return;
    }
    for mut vis in &mut overlay {
        *vis = match *vis {
            Visibility::Hidden => Visibility::Visible,
            _ => Visibility::Hidden,
        };
    }
}

fn update_overlay(
    diagnostics: Res<DiagnosticsStore>,
    mut overlay: Query<(&mut Text, &Visibility), With<FpsText>>,
) {
    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed());
    let ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed());
    for (mut text, vis) in &mut overlay {
        if *vis == Visibility::Hidden {
            continue;
        }
        text.0 = match (fps, ms) {
            (Some(fps), Some(ms)) => format!("{fps:.0} fps  {ms:.1} ms"),
            _ => "fps: measuring...".into(),
        };
    }
}
