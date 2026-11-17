//! The pause screen: Esc in game. Back to the commune, the journal, the settings and
//! (after asking) Leave game, with the invite code and who is here beside them. The game
//! does not stop: everyone else keeps playing, so this is a menu over the world, not a freeze.
//!
//! B / Esc closes the top-most layer: the settings if they are up, otherwise this screen.

use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::prelude::*;
use bevy_replicon::prelude::Replicated;

use super::journal::{Journal, Tab};
use super::kit::{self, Glyph, Look};
use super::pad_nav::PadHint;
use super::settings_panel::SettingsPanel;
use super::team::{BadgeState, MAX_BADGES, name_for, roster};
use crate::AppState;
use crate::hud::Toast;
use crate::net::{Clipboard, NetStatus};
use crate::player::{LocalPlayer, Player};
use crate::theme::kit::{self as t, size};
use crate::voice::{MicMuted, VoiceMuted};

/// How opaque the dark behind the pause screen is. The UI blends in linear light, so an alpha
/// of 0.82 left about 46% of the meadow's brightness on screen (0.18 ^ (1/2.2) is the other way
/// round); the design's "0.82 dark" reads as 0.18 of the brightness, which is 1 - 0.18 ^ 2.2.
const SCRIM_ALPHA: f32 = 0.977;

/// The pause screen's root: shown while Esc has it open.
#[derive(Component)]
pub struct PausePanel;

/// The Leave game button, whose label asks before it leaves.
#[derive(Component)]
struct LeaveButton;

/// Leaving is two presses: the first arms the button, the second leaves. Closing the
/// screen disarms it, so Stay (Back to the commune) is always the safe default.
#[derive(Resource, Default)]
struct LeaveArmed(bool);

/// A press on Leave game: whether to leave now, and the button's new armed state.
pub fn leave_press(armed: bool) -> (bool, bool) {
    if armed { (true, false) } else { (false, true) }
}

/// What Esc does in game.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EscAction {
    CloseSettings,
    ClosePause,
    OpenPause,
    /// The journal or a chapter intro owns this Esc.
    Nothing,
}

/// B / Esc closes the top-most layer; with none up it opens the pause screen.
pub fn esc_action(settings: bool, pause: bool, other_overlay: bool) -> EscAction {
    if settings {
        EscAction::CloseSettings
    } else if pause {
        EscAction::ClosePause
    } else if other_overlay {
        EscAction::Nothing
    } else {
        EscAction::OpenPause
    }
}

/// The invite card (host only) and its code text.
#[derive(Component)]
struct InviteCard;

#[derive(Component)]
struct CodeText;

#[derive(Component)]
struct CopyButton;

/// The member list: a heading and one slot per seat.
#[derive(Component)]
struct Heading;

#[derive(Component, Clone, Copy)]
enum Cell {
    Row,
    Dot,
    Name,
    State,
}

#[derive(Component, Clone, Copy)]
struct Slot(usize, Cell);

pub fn plugin(app: &mut App) {
    app.init_resource::<LeaveArmed>()
        .add_systems(OnEnter(AppState::InGame), spawn)
        .add_systems(
            Update,
            (esc, copy_from_pad, fill, leave_label)
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
}

fn open(panel: &Query<&Node, With<PausePanel>>) -> bool {
    panel.iter().any(|n| n.display != Display::None)
}

fn show(panel: &mut Query<&mut Node, With<PausePanel>>, on: bool) {
    for mut n in panel.iter_mut() {
        n.display = if on { Display::Flex } else { Display::None };
    }
}

fn esc(
    keys: Res<ButtonInput<KeyCode>>,
    overlay: Res<crate::ui::Overlay>,
    mut settings: Query<&mut Node, (With<SettingsPanel>, Without<PausePanel>)>,
    mut pause: Query<&mut Node, With<PausePanel>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    let settings_up = settings.iter().any(|n| n.display != Display::None);
    match esc_action(
        settings_up,
        pause.iter().any(|n| n.display != Display::None),
        overlay.journal || overlay.map || overlay.intro,
    ) {
        EscAction::CloseSettings => {
            for mut n in &mut settings {
                n.display = Display::None;
            }
        }
        EscAction::ClosePause => show(&mut pause, false),
        EscAction::OpenPause => show(&mut pause, true),
        EscAction::Nothing => {}
    }
}

/// While the pause screen or the settings are up the mouse is free and the game keys are
/// the menu's: tell clicks and Esc, and give the mouse back afterwards.
pub fn free_mouse(
    layers: Query<&Node, Or<(With<PausePanel>, With<SettingsPanel>)>>,
    mut overlay: ResMut<crate::ui::Overlay>,
    mut view: ResMut<crate::features::fps::FpsView>,
    mut freed: Local<bool>,
) {
    let up = layers.iter().any(|n| n.display != Display::None);
    overlay.settings = up;
    if up && view.first_person && !view.released {
        view.released = true;
        *freed = true;
    } else if !up && *freed {
        *freed = false;
        view.released = false;
    }
}

fn leave_ask(
    _: On<PointerClick>,
    mut armed: ResMut<LeaveArmed>,
    mut commands: Commands,
    replicated: Query<Entity, With<Replicated>>,
    status: ResMut<NetStatus>,
    next: ResMut<NextState<AppState>>,
) {
    let (leave, now) = leave_press(armed.0);
    armed.0 = now;
    if leave {
        crate::menu::leave(&mut commands, &replicated, status, next);
    }
}

fn copy_code(status: &NetStatus, clip: &Clipboard, toast: &mut Toast) {
    match &status.join_code {
        Some(code) => {
            let share = crate::net::share_text(code);
            clip.set(&share);
            toast.show(if share == *code {
                "join code copied"
            } else {
                "join link copied"
            });
        }
        None => toast.show("only the host has a join code"),
    }
}

fn copy_from_pad(
    pads: Query<&Gamepad>,
    pause: Query<&Node, With<PausePanel>>,
    status: Res<NetStatus>,
    clip: Res<Clipboard>,
    mut toast: ResMut<Toast>,
) {
    if open(&pause) && pads.iter().any(|p| p.just_pressed(GamepadButton::West)) {
        copy_code(&status, &clip, &mut toast);
    }
}

fn spawn(mut commands: Commands) {
    commands
        .spawn((
            PausePanel,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                display: Display::None,
                justify_content: JustifyContent::SpaceBetween,
                padding: UiRect::axes(px(56), px(48)),
                ..default()
            },
            BackgroundColor(t::SOOT_900.with_alpha(SCRIM_ALPHA)),
            GlobalZIndex(28),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|root| {
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(14),
                width: px(360),
                ..default()
            })
            .with_children(|col| {
                col.spawn(kit::display("PAUSED", size::DISPLAY, t::TEXT));
                col.spawn((
                    Text::new("(not really: everyone else keeps playing)"),
                    super::body(size::LABEL, t::TEXT_DIM),
                    Node {
                        margin: UiRect::bottom(px(22)),
                        ..default()
                    },
                ));
                col.spawn(kit::button(
                    Look::Primary,
                    "Back to the commune",
                    percent(100),
                ))
                .observe(
                    |_: On<PointerClick>, mut p: Query<&mut Node, With<PausePanel>>| {
                        show(&mut p, false)
                    },
                );
                col.spawn(kit::button(Look::Cardboard, "Journal", percent(100)))
                    .observe(
                        |_: On<PointerClick>,
                         mut p: Query<&mut Node, With<PausePanel>>,
                         mut journal: ResMut<Journal>| {
                            journal.kept = true;
                            journal.tab = Tab::Party;
                            show(&mut p, false);
                        },
                    );
                col.spawn(kit::button(Look::Cardboard, "Settings", percent(100)))
                    .observe(
                        |_: On<PointerClick>,
                         mut s: Query<&mut Node, (With<SettingsPanel>, Without<PausePanel>)>| {
                            super::settings_panel::toggle_any(&mut s);
                        },
                    );
                let mut leave = col.spawn((
                    LeaveButton,
                    kit::button(Look::Danger, "Leave game", percent(100)),
                ));
                leave
                    .entry::<Node>()
                    .and_modify(|mut n| n.margin = UiRect::top(px(40)));
                leave.observe(leave_ask);
            });
            root.spawn(Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(28),
                width: px(420),
                ..default()
            })
            .with_children(|col| {
                col.spawn((InviteCard, kit::card(t::CARD_100, 1.0)))
                    .with_children(|card| {
                        card.spawn(kit::tape());
                        card.spawn((Text::new("Invite friends"), super::head(size::H2, t::INK)));
                        card.spawn((
                            Text::new("Send them this code. They paste it in Join a friend."),
                            super::body(size::BODY_REG, t::INK),
                            super::Regular,
                        ));
                        card.spawn((
                            Node {
                                margin: UiRect::vertical(px(12)),
                                padding: UiRect::all(px(12)),
                                border_radius: BorderRadius::all(px(4)),
                                ..default()
                            },
                            BackgroundColor(t::CARD_300.with_alpha(0.45)),
                            children![(CodeText, Text::new(""), super::head(size::NUM, t::INK))],
                        ));
                        card.spawn((
                            CopyButton,
                            Node {
                                align_self: AlignSelf::FlexStart,
                                ..default()
                            },
                            children![kit::button(Look::Cardboard, "Copy code", px(220))],
                        ));
                    });
                col.spawn(kit::panel()).with_children(|list| {
                    list.spawn((
                        Heading,
                        Text::new(""),
                        super::body(size::LABEL, t::TEXT_DIM),
                        Node {
                            margin: UiRect::bottom(px(10)),
                            ..default()
                        },
                    ));
                    for i in 0..MAX_BADGES {
                        list.spawn((
                            Slot(i, Cell::Row),
                            Node {
                                align_items: AlignItems::Center,
                                column_gap: px(14),
                                padding: UiRect::axes(px(14), px(8)),
                                margin: UiRect::bottom(px(6)),
                                border_radius: BorderRadius::all(px(6)),
                                display: Display::None,
                                ..default()
                            },
                            BackgroundColor(t::SOOT_700),
                        ))
                        .with_children(|row| {
                            row.spawn((
                                Slot(i, Cell::Dot),
                                Node {
                                    width: px(22),
                                    height: px(22),
                                    border_radius: BorderRadius::MAX,
                                    ..default()
                                },
                                BackgroundColor(t::seat(i)),
                            ));
                            row.spawn((
                                Slot(i, Cell::Name),
                                Node {
                                    flex_grow: 1.0,
                                    ..default()
                                },
                                Text::new(""),
                                super::body(size::BODY, t::TEXT),
                            ));
                            row.spawn((
                                Slot(i, Cell::State),
                                Text::new(""),
                                super::body(size::LABEL, t::TEXT_DIM),
                            ));
                        });
                    }
                });
            });
            root.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: px(56),
                    bottom: px(32),
                    column_gap: px(36),
                    ..default()
                },
                PadHint,
                Visibility::Hidden,
                children![
                    kit::hint(Glyph::A, "Select"),
                    kit::hint(Glyph::X, "Copy code"),
                    kit::hint(Glyph::B, "Back"),
                ],
            ));
        });
}

/// Keep the code, the copy button's click and the member list current while the screen is up.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn fill(
    mut commands: Commands,
    pause: Query<&Node, With<PausePanel>>,
    status: Res<NetStatus>,
    me: Option<Res<LocalPlayer>>,
    muted: Option<Res<VoiceMuted>>,
    players: Query<(&Player, Option<&MicMuted>)>,
    mut cards: Query<&mut Node, (With<InviteCard>, Without<PausePanel>, Without<Slot>)>,
    mut code: Query<&mut Text, (With<CodeText>, Without<Slot>, Without<Heading>)>,
    mut heading: Query<&mut Text, (With<Heading>, Without<Slot>, Without<CodeText>)>,
    mut slots: Query<
        (
            &Slot,
            &mut Node,
            Option<&mut Text>,
            Option<&mut BackgroundColor>,
        ),
        (Without<InviteCard>, Without<PausePanel>),
    >,
    copy: Query<Entity, With<CopyButton>>,
    mut wired: Local<bool>,
) {
    if !open(&pause) {
        return;
    }
    if !*wired && let Ok(e) = copy.single() {
        *wired = true;
        commands.entity(e).observe(
            |_: On<PointerClick>,
             status: Res<NetStatus>,
             clip: Res<Clipboard>,
             mut toast: ResMut<Toast>| copy_code(&status, &clip, &mut toast),
        );
    }
    let hosting = status.join_code.is_some();
    for mut n in &mut cards {
        n.display = if hosting {
            Display::Flex
        } else {
            Display::None
        };
    }
    if let (Ok(mut t), Some(c)) = (code.single_mut(), &status.join_code) {
        let want = super::ellipsize(c, 22);
        if t.0 != want {
            t.0 = want;
        }
    }
    let me = me.map(|m| m.0);
    let here = roster(
        players
            .iter()
            .map(|(p, m)| BadgeState {
                id: p.id,
                name: name_for(p.id, Some(p.id) == me),
                tears: 0,
                muted: if Some(p.id) == me {
                    muted.as_ref().is_some_and(|v| v.0)
                } else {
                    m.is_some()
                },
                speaking: false,
                local: Some(p.id) == me,
            })
            .collect(),
    );
    if let Ok(mut h) = heading.single_mut() {
        let want = format!("IN THE COMMUNE · {} / {MAX_BADGES}", here.len());
        if h.0 != want {
            h.0 = want;
        }
    }
    for (slot, mut node, text, bg) in &mut slots {
        let Slot(i, cell) = *slot;
        let Some(b) = here.get(i) else {
            if matches!(cell, Cell::Row) {
                node.display = Display::None;
            }
            continue;
        };
        match (cell, text, bg) {
            (Cell::Row, ..) => node.display = Display::Flex,
            (Cell::Name, Some(mut t), _) => {
                let want = if b.local && hosting {
                    format!("{} (host)", b.name)
                } else {
                    b.name.clone()
                };
                if t.0 != want {
                    t.0 = want;
                }
            }
            (Cell::State, Some(mut t), _) => {
                let want = if b.muted { "muted" } else { "mic on" };
                if t.0 != want {
                    t.0 = want.into();
                }
            }
            _ => {}
        }
    }
}

/// The Leave game button asks first: its label says so once armed, and the screen
/// closing disarms it.
fn leave_label(
    pause: Query<&Node, With<PausePanel>>,
    mut armed: ResMut<LeaveArmed>,
    button: Query<&Children, With<LeaveButton>>,
    mut texts: Query<&mut Text>,
) {
    if armed.0 && !open(&pause) {
        armed.0 = false;
    }
    let want = if armed.0 {
        "Really leave?"
    } else {
        "Leave game"
    };
    for kids in &button {
        for k in kids {
            if let Ok(mut text) = texts.get_mut(*k)
                && text.0 != want
            {
                text.0 = want.into();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scrim_leaves_a_fifth_of_the_brightness() {
        // Linear blending: what shows through is (1 - alpha) in light, 18% perceived.
        let seen = (1.0 - SCRIM_ALPHA).powf(1.0 / 2.2);
        assert!((seen - 0.18).abs() < 0.01, "{seen}");
    }

    #[test]
    fn leaving_takes_two_presses() {
        assert_eq!(leave_press(false), (false, true));
        assert_eq!(leave_press(true), (true, false));
    }

    #[test]
    fn esc_closes_the_top_layer_then_opens_the_pause_screen() {
        use EscAction::*;
        assert_eq!(esc_action(true, true, false), CloseSettings);
        assert_eq!(esc_action(false, true, false), ClosePause);
        assert_eq!(esc_action(false, false, false), OpenPause);
        // The journal's Esc is the journal's.
        assert_eq!(esc_action(false, false, true), Nothing);
    }
}
