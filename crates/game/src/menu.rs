//! Placeholder main menu: keyboard only, intentionally ugly. Replace freely.

use bevy::prelude::*;

use crate::AppState;
use crate::net::{Clipboard, NetCommand, NetStatus};

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::Menu), spawn_menu)
            .add_systems(
                Update,
                (menu_input, update_status).run_if(in_state(AppState::Menu)),
            );
    }
}

#[derive(Component)]
struct StatusText;

fn spawn_menu(mut commands: Commands) {
    commands.spawn((Camera2d, DespawnOnExit(AppState::Menu)));
    commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(32)),
                row_gap: px(12),
                ..default()
            },
            DespawnOnExit(AppState::Menu),
        ))
        .with_children(|ui| {
            ui.spawn((Text::new("friendSLOP"), TextFont::from_font_size(48.0)));
            ui.spawn(Text::new(
                "[H] Host a game (join code gets copied to your clipboard)",
            ));
            ui.spawn(Text::new("[J] Join: copy a friend's code, then press J"));
            ui.spawn((Text::new(""), StatusText));
        });
}

fn menu_input(
    keys: Res<ButtonInput<KeyCode>>,
    clipboard: Res<Clipboard>,
    mut status: ResMut<NetStatus>,
    mut commands: MessageWriter<NetCommand>,
) {
    if keys.just_pressed(KeyCode::KeyH) {
        commands.write(NetCommand::Host);
    }
    if keys.just_pressed(KeyCode::KeyJ) {
        match clipboard.get() {
            Some(code) => {
                commands.write(NetCommand::Join(code));
            }
            None => status.message = "Clipboard is empty. Copy a join code first.".into(),
        }
    }
}

fn update_status(status: Res<NetStatus>, mut text: Query<&mut Text, With<StatusText>>) {
    if status.is_changed() {
        for mut t in &mut text {
            t.0.clone_from(&status.message);
        }
    }
}
