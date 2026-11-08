//! Main menu (#25): a cute clickable menu to host, join with a code, and change
//! a few settings. `friendslop host` / `friendslop join <code>` still skip it.
//!
//! Hosting alone is fine: you're simply the only player until friends join.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use bevy::picking::hover::Hovered;
use bevy_replicon::prelude::*;
use bevy_replicon_iroh::{IrohClient, IrohServer};

use crate::AppState;
use crate::hud::Toast;
use crate::net::{Clipboard, NetCommand, NetStatus};
use crate::player::LocalPlayer;
use crate::settings::Settings;
use crate::voice::VoiceMuted;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JoinCode>()
            .init_resource::<Connecting>()
            .add_systems(OnEnter(AppState::Menu), spawn_menu)
            .add_systems(
                Update,
                (
                    type_join_code,
                    start_pending_join,
                    connect_timeout,
                    refresh_labels,
                    hover_buttons,
                )
                    .run_if(in_state(AppState::Menu)),
            )
            .add_systems(
                Update,
                (leave_on_esc, copy_code_again).run_if(in_state(AppState::InGame)),
            );
    }
}

/// Joining in progress. The join is sent a couple of frames after the click so
/// "Connecting..." is drawn before the (blocking) connect starts, then we give
/// the host [`Connecting::TIMEOUT_SECS`] to answer.
#[derive(Resource, Default, Debug)]
pub struct Connecting {
    pending: Option<(String, u8)>,
    waited: f32,
}

impl Connecting {
    pub const TIMEOUT_SECS: f32 = 20.0;
}

/// What's typed (or pasted) into the join box.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct JoinCode(pub String);

impl JoinCode {
    /// Longest code we accept in the box (iroh ids are 64 hex chars).
    pub const MAX_LEN: usize = 128;

    /// Apply one key press. Returns true if Enter was pressed.
    pub fn type_key(&mut self, key: &Key) -> bool {
        match key {
            Key::Character(c) => {
                for ch in c.chars().filter(|c| !c.is_control() && !c.is_whitespace()) {
                    if self.0.len() < Self::MAX_LEN {
                        self.0.push(ch);
                    }
                }
            }
            Key::Backspace => {
                self.0.pop();
            }
            Key::Enter => return true,
            _ => {}
        }
        false
    }

    /// Replace the box with pasted text, trimmed.
    pub fn paste(&mut self, text: &str) {
        self.0 = text.trim().chars().take(Self::MAX_LEN).collect();
    }
}

const BG: Color = Color::srgb(1.0, 0.92, 0.96);
const CARD: Color = Color::srgb(1.0, 0.98, 1.0);
const PINK: Color = Color::srgb(1.0, 0.62, 0.8);
const LILAC: Color = Color::srgb(0.78, 0.7, 1.0);
const MINT: Color = Color::srgb(0.5, 0.85, 0.75);
const INK: Color = Color::srgb(0.42, 0.24, 0.4);

#[derive(Component, Clone, Copy)]
enum Label {
    Status,
    Code,
    Master,
    Voice,
    Mic,
    HaltName,
}

/// A button's resting color, brightened while hovered.
#[derive(Component, Clone, Copy)]
struct BaseColor(Color);

fn spawn_menu(mut commands: Commands) {
    commands.spawn((Camera2d, DespawnOnExit(AppState::Menu)));
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(BG),
            DespawnOnExit(AppState::Menu),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::all(px(32)),
                    row_gap: px(14),
                    border_radius: BorderRadius::all(px(28)),
                    ..default()
                },
                BackgroundColor(CARD),
            ))
            .with_children(|ui| {
                text(ui, "~ friendSLOP ~", 52.0);
                text(ui, "(=^-^=)  let's play together!  (=^-^=)", 18.0);

                button(ui, "Host a game", PINK, 260.0).observe(
                    |_: On<PointerClick>, mut net: MessageWriter<NetCommand>| {
                        net.write(NetCommand::Host);
                    },
                );
                text(
                    ui,
                    "(works solo too; the join code gets copied for friends)",
                    14.0,
                );

                text(ui, "Join a friend: type or paste their code", 18.0);
                ui.spawn((
                    Node {
                        width: px(560),
                        min_height: px(40),
                        padding: UiRect::all(px(8)),
                        border_radius: BorderRadius::all(px(12)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(BG),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(""),
                        TextFont::from_font_size(13.0),
                        TextColor(INK),
                        Label::Code,
                    ));
                });
                row(ui, |r| {
                    button(r, "Paste", LILAC, 120.0).observe(
                        |_: On<PointerClick>, clip: Res<Clipboard>, mut code: ResMut<JoinCode>| {
                            if let Some(text) = clip.get() {
                                code.paste(&text);
                            }
                        },
                    );
                    button(r, "Clear", LILAC, 120.0)
                        .observe(|_: On<PointerClick>, mut code: ResMut<JoinCode>| code.0.clear());
                    button(r, "Join!", PINK, 160.0).observe(
                        |_: On<PointerClick>,
                         code: Res<JoinCode>,
                         connecting: ResMut<Connecting>,
                         status: ResMut<NetStatus>| {
                            join(&code, connecting, status)
                        },
                    );
                });

                text(ui, "Settings", 24.0);
                volume_row(ui, "Master volume", Label::Master, |s| &mut s.master_volume);
                volume_row(ui, "Voice volume", Label::Voice, |s| &mut s.voice_volume);
                row(ui, |r| {
                    text(r, "Microphone (or press M)", 18.0);
                    button(r, "", MINT, 160.0).insert(Label::Mic).observe(
                        |_: On<PointerClick>, mut muted: ResMut<VoiceMuted>| {
                            muted.0 = !muted.0;
                        },
                    );
                });

                row(ui, |r| {
                    text(r, "Let HALT use my computer username", 18.0);
                    button(r, "", MINT, 160.0).insert(Label::HaltName).observe(
                        |_: On<PointerClick>, mut s: ResMut<Settings>| {
                            s.halt_real_name = !s.halt_real_name;
                        },
                    );
                });

                ui.spawn((
                    Text::new(""),
                    TextFont::from_font_size(16.0),
                    TextColor(INK),
                    Label::Status,
                ));
            });
        });
}

fn join(code: &JoinCode, mut connecting: ResMut<Connecting>, mut status: ResMut<NetStatus>) {
    if code.0.is_empty() {
        status.message = "Type or paste a join code first.".into();
    } else if connecting.pending.is_none() {
        status.message = "Connecting... (this can take a few seconds)".into();
        connecting.pending = Some((code.0.clone(), 2));
        connecting.waited = 0.0;
    }
}

fn start_pending_join(mut connecting: ResMut<Connecting>, mut net: MessageWriter<NetCommand>) {
    let Some((_, frames)) = connecting.pending.as_mut() else {
        return;
    };
    if *frames > 0 {
        *frames -= 1;
        return;
    }
    if let Some((code, _)) = connecting.pending.take() {
        net.write(NetCommand::Join(code));
    }
}

/// Give up if the host never finishes the handshake.
fn connect_timeout(
    mut commands: Commands,
    time: Res<Time>,
    client: Option<Res<IrohClient>>,
    state: Option<Res<State<ClientState>>>,
    mut connecting: ResMut<Connecting>,
    mut status: ResMut<NetStatus>,
) {
    let waiting = client.is_some() && state.is_some_and(|s| *s.get() == ClientState::Connecting);
    if !waiting {
        connecting.waited = 0.0;
        return;
    }
    connecting.waited += time.delta_secs();
    if connecting.waited >= Connecting::TIMEOUT_SECS {
        commands.remove_resource::<IrohClient>();
        connecting.waited = 0.0;
        status.message =
            "Could not reach that host (timed out). Check the code and try again.".into();
    }
}

/// Esc in game: disconnect (or stop hosting) and go back to the menu.
fn leave_on_esc(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    replicated: Query<Entity, With<Replicated>>,
    mut status: ResMut<NetStatus>,
    mut next: ResMut<NextState<AppState>>,
    mut view: ResMut<crate::features::fps::FpsView>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    // First Esc in first person just frees the mouse; the next one leaves.
    if view.first_person && !view.released {
        view.released = true;
        return;
    }
    commands.remove_resource::<IrohClient>();
    commands.remove_resource::<IrohServer>();
    commands.remove_resource::<LocalPlayer>();
    for e in &replicated {
        commands.entity(e).despawn();
    }
    status.join_code = None;
    status.message = "Left the game.".into();
    next.set(AppState::Menu);
}

fn copy_code_again(
    keys: Res<ButtonInput<KeyCode>>,
    status: Res<NetStatus>,
    clip: Res<Clipboard>,
    mut toast: ResMut<Toast>,
) {
    if !keys.just_pressed(KeyCode::KeyC) {
        return;
    }
    match &status.join_code {
        Some(code) => {
            clip.set(code);
            toast.show("join code copied");
        }
        None => toast.show("only the host has a join code"),
    }
}

fn hover_buttons(
    mut buttons: Query<(&Hovered, &BaseColor, &mut BackgroundColor), Changed<Hovered>>,
) {
    for (hovered, base, mut bg) in &mut buttons {
        bg.0 = if hovered.get() {
            base.0.mix(&Color::WHITE, 0.25)
        } else {
            base.0
        };
    }
}

fn text(ui: &mut ChildSpawnerCommands, s: &str, size: f32) {
    ui.spawn((Text::new(s), TextFont::from_font_size(size), TextColor(INK)));
}

fn row(ui: &mut ChildSpawnerCommands, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    ui.spawn(Node {
        column_gap: px(12),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(f);
}

/// A rounded button with a text child (the mic button's text is filled live).
fn button<'a>(
    ui: &'a mut ChildSpawnerCommands,
    label: &str,
    color: Color,
    width: f32,
) -> EntityCommands<'a> {
    let mut e = ui.spawn((
        Node {
            width: px(width),
            padding: UiRect::axes(px(16), px(10)),
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(px(999)),
            ..default()
        },
        BackgroundColor(color),
        BaseColor(color),
        Hovered::default(),
    ));
    e.with_children(|b| {
        b.spawn((
            Text::new(label),
            TextFont::from_font_size(20.0),
            TextColor(Color::WHITE),
        ));
    });
    e
}

fn volume_row(
    ui: &mut ChildSpawnerCommands,
    name: &str,
    label: Label,
    field: fn(&mut Settings) -> &mut f32,
) {
    row(ui, |r| {
        text(r, name, 18.0);
        button(r, "-", LILAC, 50.0).observe(move |_: On<PointerClick>, mut s: ResMut<Settings>| {
            let v = field(&mut s);
            *v = Settings::nudge(*v, -1);
        });
        r.spawn((
            Text::new(""),
            TextFont::from_font_size(18.0),
            TextColor(INK),
            Node {
                width: px(60),
                ..default()
            },
            label,
        ));
        button(r, "+", LILAC, 50.0).observe(move |_: On<PointerClick>, mut s: ResMut<Settings>| {
            let v = field(&mut s);
            *v = Settings::nudge(*v, 1);
        });
    });
}

fn type_join_code(
    mut keys: MessageReader<KeyboardInput>,
    mut code: ResMut<JoinCode>,
    connecting: ResMut<Connecting>,
    status: ResMut<NetStatus>,
) {
    let mut enter = false;
    for key in keys.read() {
        if key.state == ButtonState::Pressed {
            enter |= code.type_key(&key.logical_key);
        }
    }
    if enter {
        join(&code, connecting, status);
    }
}

fn refresh_labels(
    status: Res<NetStatus>,
    code: Res<JoinCode>,
    settings: Res<Settings>,
    muted: Res<VoiceMuted>,
    mut texts: Query<(&mut Text, &Label)>,
    buttons: Query<(&Label, &Children), Without<Text>>,
    mut child_texts: Query<&mut Text, Without<Label>>,
) {
    for (mut t, label) in &mut texts {
        let new = match label {
            Label::Status => status.message.clone(),
            Label::Code if code.0.is_empty() => "(no code yet)".into(),
            Label::Code => code.0.clone(),
            Label::Master => pct(settings.master_volume),
            Label::Voice => pct(settings.voice_volume),
            Label::Mic | Label::HaltName => continue,
        };
        if t.0 != new {
            t.0 = new;
        }
    }
    let mic = if muted.0 { "Muted" } else { "On" };
    let halt = if settings.halt_real_name { "Yes" } else { "No" };
    for (label, children) in &buttons {
        let mic = match label {
            Label::Mic => mic,
            Label::HaltName => halt,
            _ => continue,
        };
        for child in children.iter() {
            if let Ok(mut t) = child_texts.get_mut(child)
                && t.0 != mic
            {
                t.0 = mic.into();
            }
        }
    }
}

fn pct(v: f32) -> String {
    format!("{}%", (v * 100.0).round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_paste() {
        let mut c = JoinCode::default();
        assert!(!c.type_key(&Key::Character("ab".into())));
        c.type_key(&Key::Character(" ".into()));
        c.type_key(&Key::Backspace);
        assert_eq!(c.0, "a");
        assert!(c.type_key(&Key::Enter));
        c.paste("  deadbeef\n");
        assert_eq!(c.0, "deadbeef");
    }
}
