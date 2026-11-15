//! Main menu (#25): a cute clickable menu to host, join with a code, and change
//! a few settings. `friendslop host` / `friendslop join <code>` still skip it.
//!
//! Hosting alone is fine: you're simply the only player until friends join.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use crate::net::IrohClient;
use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::picking::hover::Hovered;
use bevy_replicon::prelude::*;

use crate::AppState;
use crate::hud::Toast;
use crate::net::{Clipboard, NetCommand, NetStatus};
use crate::player::LocalPlayer;
use crate::scenery;
use crate::theme;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JoinCode>()
            .init_resource::<Connecting>()
            .add_systems(OnEnter(AppState::Menu), (spawn_menu, spawn_backdrop))
            .add_systems(
                Update,
                (
                    type_join_code,
                    start_pending_join,
                    connect_timeout,
                    (refresh_labels, refresh_lan, pad_shortcuts),
                    hover_buttons,
                    orbit_camera,
                    backdrop_fallbacks,
                    menu_font,
                    menu_shortcuts,
                    style_status,
                    picker_esc,
                    cancel_join.before(crate::ui::menu_back::back),
                )
                    .run_if(in_state(AppState::Menu)),
            )
            .add_systems(Update, copy_code_again.run_if(in_state(AppState::InGame)));
    }
}

/// Joining in progress. The join is sent a couple of frames after the click so
/// "Connecting..." is drawn first, then the connect runs on a task and we give
/// the host [`Connecting::TIMEOUT_SECS`] to answer.
#[derive(Resource, Default, Debug)]
pub struct Connecting {
    pending: Option<(String, u8)>,
    waited: f32,
}

impl Connecting {
    pub const TIMEOUT_SECS: f32 = 20.0;

    /// A join that stays under way (screenshot mode): the send is held back.
    pub(crate) fn stage(&mut self, code: &str) {
        self.pending = Some((code.to_owned(), u8::MAX));
        self.waited = 0.0;
    }
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
                // Codes are hex: anything else (a stray 'm' from the mute
                // key, say) would only make the code invalid.
                for ch in c.chars().filter(char::is_ascii_hexdigit) {
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
        // Same rule as typing: codes are hex, drop anything else (a pasted
        // share link counts as its code).
        self.0 = code_in(text)
            .trim()
            .chars()
            .filter(char::is_ascii_hexdigit)
            .take(Self::MAX_LEN)
            .collect();
    }
}

/// The join code in `text`: the text itself, or the `join=` part of a share
/// link (`https://.../?join=<code>`).
pub fn code_in(text: &str) -> &str {
    match text.split_once("join=") {
        Some((_, rest)) => rest.split(['&', '#']).next().unwrap_or(rest),
        None => text,
    }
}

pub(crate) const CARD: Color = theme::CARD;
pub(crate) const PINK: Color = theme::PINK;
pub(crate) const LILAC: Color = theme::LILAC;
pub(crate) const MINT: Color = theme::MINT;
pub(crate) const INK: Color = theme::INK;
pub(crate) const SOFT_INK: Color = theme::SOFT_INK;

#[derive(Component, Clone, Copy)]
enum Label {
    Status,
    Code,
    /// The end of the code, in the gamepad's hex picker.
    CodeTail,
    JoinButton,
}

/// The join-code field (the hex picker gives the pad's focus back to it).
#[derive(Component)]
struct CodeField;

/// The gamepad's join-code keypad (#361), opened by A on the code field. It
/// doesn't need Steam's on-screen keyboard.
#[derive(Component)]
pub(crate) struct HexPicker;

/// One key of the [`HexPicker`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum PickKey {
    Hex(char),
    Back,
    Paste,
    Join,
}

/// The picker's keys in rows: 0-f, then delete, paste and join.
const PICKER_ROWS: [&[PickKey]; 5] = [
    &[
        PickKey::Hex('0'),
        PickKey::Hex('1'),
        PickKey::Hex('2'),
        PickKey::Hex('3'),
    ],
    &[
        PickKey::Hex('4'),
        PickKey::Hex('5'),
        PickKey::Hex('6'),
        PickKey::Hex('7'),
    ],
    &[
        PickKey::Hex('8'),
        PickKey::Hex('9'),
        PickKey::Hex('a'),
        PickKey::Hex('b'),
    ],
    &[
        PickKey::Hex('c'),
        PickKey::Hex('d'),
        PickKey::Hex('e'),
        PickKey::Hex('f'),
    ],
    &[PickKey::Back, PickKey::Paste, PickKey::Join],
];

impl PickKey {
    fn label(self) -> String {
        match self {
            PickKey::Hex(c) => c.to_string(),
            PickKey::Back => "Del".into(),
            PickKey::Paste => "Paste".into(),
            PickKey::Join => "Join".into(),
        }
    }

    /// Apply a key to the code. Returns true for Join.
    fn press(self, code: &mut JoinCode, clipboard: Option<String>) -> bool {
        match self {
            PickKey::Hex(c) => {
                code.type_key(&Key::Character(c.to_string().into()));
            }
            PickKey::Back => {
                code.0.pop();
            }
            PickKey::Paste => {
                if let Some(text) = clipboard {
                    code.paste(&text);
                }
            }
            PickKey::Join => return true,
        }
        false
    }
}

/// The last `n` characters of a code, "…" in front if cut.
fn tail(code: &str, n: usize) -> String {
    let len = code.chars().count();
    if len <= n {
        code.to_string()
    } else {
        format!("…{}", code.chars().skip(len - n).collect::<String>())
    }
}

/// Spawn the hex picker over the join card: a soot panel with the keys as kit buttons.
fn spawn_picker(world: &mut World) {
    use crate::theme::kit::{self as t, size};
    use crate::ui::kit::{self, Glyph, Look, hint};
    use crate::ui::{body, head};
    let (key_w, gap) = (80.0, 10.0);
    let width = 4.0 * key_w + 3.0 * gap;
    world
        .spawn((
            HexPicker,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(t::SOOT_900.with_alpha(0.6)),
            GlobalZIndex(40),
            DespawnOnExit(AppState::Menu),
        ))
        .with_children(|scrim| {
            styled_now(scrim.spawn(kit::panel()), |n| n.row_gap = px(gap)).with_children(|p| {
                p.spawn((Text::new("Join code"), head(size::H2, t::TEXT)));
                p.spawn((
                    Text::new(""),
                    body(size::LABEL, t::TEXT_DIM),
                    TextLayout::no_wrap(),
                    Label::CodeTail,
                ));
                for keys in PICKER_ROWS {
                    p.spawn(Node {
                        column_gap: px(gap),
                        ..default()
                    })
                    .with_children(|row| {
                        for &key in keys {
                            // The last row has three keys, each a little wider.
                            let w = if keys.len() == 3 {
                                (width - 2.0 * gap) / 3.0
                            } else {
                                key_w
                            };
                            let look = if key == PickKey::Join {
                                Look::Primary
                            } else {
                                Look::Plain
                            };
                            styled_now(
                                row.spawn((key, kit::button(look, &key.label(), px(w)))),
                                |n| {
                                    n.justify_content = JustifyContent::Center;
                                    n.padding = UiRect::ZERO;
                                },
                            )
                            .observe(press_picker_key);
                        }
                    });
                }
                p.spawn((
                    Node {
                        margin: UiRect::top(px(6)),
                        column_gap: px(28),
                        ..default()
                    },
                    children![hint(Glyph::A, "Press"), hint(Glyph::B, "Close")],
                ));
            });
        });
}

/// Adjust the [`Node`] a kit piece spawned with (its width, gap or placement).
fn styled<'a>(
    mut e: EntityCommands<'a>,
    f: impl FnOnce(&mut Node) + Send + Sync + 'static,
) -> EntityCommands<'a> {
    e.entry::<Node>().and_modify(move |mut n| f(&mut n));
    e
}

/// [`styled`] for a world-spawned piece.
fn styled_now<'a>(mut e: EntityWorldMut<'a>, f: impl FnOnce(&mut Node)) -> EntityWorldMut<'a> {
    if let Some(mut n) = e.get_mut::<Node>() {
        f(&mut n);
    }
    e
}

#[allow(clippy::too_many_arguments)]
fn press_picker_key(
    click: On<PointerClick>,
    keys: Query<&PickKey>,
    mut commands: Commands,
    clip: Res<Clipboard>,
    mut code: ResMut<JoinCode>,
    mut connecting: ResMut<Connecting>,
    mut status: ResMut<NetStatus>,
    client: Option<Res<IrohClient>>,
) {
    let Ok(&key) = keys.get(click.entity) else {
        return;
    };
    let pasted = (key == PickKey::Paste).then(|| clip.get()).flatten();
    if key.press(&mut code, pasted) {
        join(&code, &mut connecting, &mut status, client.is_some());
        commands.queue(close_picker);
    }
}

/// Close the hex picker and put the pad's focus back on the code field.
fn close_picker(world: &mut World) {
    let mut pickers = world.query_filtered::<Entity, With<HexPicker>>();
    let open: Vec<Entity> = pickers.iter(world).collect();
    if open.is_empty() {
        return;
    }
    for e in open {
        world.despawn(e);
    }
    let mut field = world.query_filtered::<Entity, With<CodeField>>();
    let field = field.iter(world).next();
    if let Some(mut focus) = world.get_resource_mut::<crate::ui::pad_nav::PadFocus>() {
        focus.entity = field;
    }
}

/// B (Esc) closes the hex picker.
fn picker_esc(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::Escape) {
        commands.queue(close_picker);
    }
}

/// B (Esc) calls off a join in progress: the button went to "Joining..." and
/// nothing else would end it before the timeout.
pub(crate) fn cancel_join(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    client: Option<Res<IrohClient>>,
    joining: Option<Res<crate::net::JoinTask>>,
    mut connecting: ResMut<Connecting>,
    mut status: ResMut<NetStatus>,
    mut commands: Commands,
) {
    let busy = connecting.pending.is_some() || client.is_some() || joining.is_some();
    if !keys.just_pressed(KeyCode::Escape) || !busy {
        return;
    }
    // This B is spent: the join card stays up behind the cancelled join.
    keys.clear_just_pressed(KeyCode::Escape);
    connecting.pending = None;
    connecting.waited = 0.0;
    commands.queue(crate::net::teardown_transport);
    status.message = "Join cancelled.".into();
}

/// A button's resting color, brightened while hovered.
#[derive(Component, Clone, Copy)]
pub(crate) struct BaseColor(pub(crate) Color);

/// The pill behind the status line (tinted by what the message says).
#[derive(Component)]
struct StatusPill;

/// The backdrop camera, orbiting the little meadow.
#[derive(Component)]
struct MenuCamera;

/// The join card, a layer over the title like the settings: shut until "Join a friend".
#[derive(Component)]
pub(crate) struct JoinPanel;

/// The sticky note naming a game hosted near you.
#[derive(Component)]
struct LanNote;

#[derive(Component)]
struct LanNoteText(Cell);

/// A row of the "open near you" list.
#[derive(Component)]
struct LanRow(usize);

#[derive(Component)]
struct LanRowName(usize);

/// The line shown while nobody near you is hosting.
#[derive(Component)]
struct LanEmpty;

/// Which line of the sticky note.
#[derive(Clone, Copy)]
enum Cell {
    Who,
    Join,
}

/// Open games listed on the join card.
const LAN_ROWS: usize = 3;

/// The title screen (the "commune noticeboard" menu): a soot panel on the left over the meadow
/// with the logo and the buttons, a sticky note for a game hosted near you, and the hint bar.
fn spawn_menu(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    use crate::theme::kit::{self as t, size};
    use crate::ui::kit::{self, Glyph, Look, hint};
    use crate::ui::{body, head};
    let logo: Handle<Image> = assets.map(|a| a.load("ui/logo.png")).unwrap_or_default();
    let (full, half) = (percent(100), percent(48.5));
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(0),
                width: px(440),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                row_gap: px(12),
                padding: UiRect::axes(px(t::EDGE * 2.0), px(t::EDGE * 1.5)),
                ..default()
            },
            BackgroundColor(t::SOOT_900.with_alpha(0.92)),
            DespawnOnExit(AppState::Menu),
        ))
        .with_children(|ui| {
            ui.spawn((
                Text::new("a commune of terrible ideas"),
                body(size::LABEL, t::TEXT_DIM),
            ));
            // The illustrated logo; the wordmark stands in if the file is missing.
            ui.spawn((
                Node {
                    width: px(344),
                    height: px(99.7),
                    ..default()
                },
                ImageNode::new(logo),
            ));
            ui.spawn((
                Text::new("1–6 friends. One very bad landlord."),
                body(size::BODY_REG, t::TEXT_DIM),
                crate::ui::Regular,
                Node {
                    margin: UiRect::bottom(px(12)),
                    ..default()
                },
            ));
            ui.spawn(kit::button(Look::Primary, "Host a commune", full))
                .observe(
                    |_: On<PointerClick>,
                     mut net: MessageWriter<NetCommand>,
                     mut connecting: ResMut<Connecting>| {
                        connecting.pending = None;
                        net.write(NetCommand::Host);
                    },
                );
            ui.spawn(kit::button(Look::Cardboard, "Join a friend", full))
                .observe(
                    |_: On<PointerClick>, mut panel: Query<&mut Node, With<JoinPanel>>| {
                        for mut n in &mut panel {
                            n.display = Display::Flex;
                        }
                    },
                );
            ui.spawn(kit::button(Look::Cardboard, "Character", full))
                .observe(
                    |_: On<PointerClick>, mut open: ResMut<crate::creator::CreatorOpen>| {
                        open.0 = !open.0;
                    },
                );
            ui.spawn(kit::button(Look::Plain, "Settings", full))
                .observe(
                    |_: On<PointerClick>, mut panel: Query<&mut Node, With<SettingsPanel>>| {
                        crate::ui::settings_panel::toggle(&mut panel);
                    },
                );
            ui.spawn(kit::button(Look::Plain, "Whose fault is it", full))
                .observe(
                    |_: On<PointerClick>,
                     mut panel: Query<&mut Node, With<crate::fault::FaultPanel>>| {
                        crate::fault::toggle(&mut panel);
                    },
                );
            ui.spawn(Node {
                justify_content: JustifyContent::SpaceBetween,
                width: full,
                ..default()
            })
            .with_children(|row| {
                row.spawn(kit::button(Look::Plain, "Kori prizes", half))
                    .observe(
                        |_: On<PointerClick>,
                         mut panel: Query<&mut Node, With<crate::kori_shelf::ShelfPanel>>| {
                            crate::kori_shelf::toggle(&mut panel);
                        },
                    );
                row.spawn(kit::button(Look::Plain, "Quit", half)).observe(
                    |_: On<PointerClick>, mut exit: MessageWriter<AppExit>| {
                        exit.write(AppExit::Success);
                    },
                );
            });
            // News ("Left the game.", errors) above the footer.
            ui.spawn(status_pill());
            ui.spawn((
                Text::new("Made by your Discord. Blame accordingly."),
                body(size::LABEL, t::TEXT_DIM),
                Node {
                    margin: UiRect::top(Val::Auto),
                    ..default()
                },
            ));
        });

    // A game hosted near you, as a sticky note top right.
    let note = commands
        .spawn((
            LanNote,
            crate::ui::pad_nav::PadFocusable,
            Node {
                position_type: PositionType::Absolute,
                right: px(t::EDGE * 2.0),
                top: px(t::EDGE * 2.0),
                width: px(320),
                display: Display::None,
                ..default()
            },
            DespawnOnExit(AppState::Menu),
        ))
        .observe(join_first_near)
        .id();
    commands
        .spawn((kit::card(t::BUTTER_400, -2.0), ChildOf(note)))
        .with_children(|note| {
            note.spawn(kit::tape());
            note.spawn((
                LanNoteText(Cell::Who),
                Text::new(""),
                body(size::LABEL, t::INK),
            ));
            note.spawn((
                LanNoteText(Cell::Join),
                Text::new(""),
                head(size::H2, t::INK),
            ));
            note.spawn(Node {
                align_self: AlignSelf::FlexEnd,
                ..default()
            })
            .with_children(|g| {
                g.spawn(kit::glyph(Glyph::Y));
            });
        });

    spawn_join_panel(&mut commands);

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            right: px(t::EDGE * 2.0),
            bottom: px(t::EDGE),
            column_gap: px(36),
            ..default()
        },
        crate::ui::pad_nav::PadHint,
        Visibility::Hidden,
        DespawnOnExit(AppState::Menu),
        children![hint(Glyph::A, "Select")],
    ));
}

/// The pill behind a news line (tinted by what the message says).
fn status_pill() -> impl Bundle {
    use crate::theme::kit::{self as t, size};
    (
        Node {
            padding: UiRect::axes(px(14), px(6)),
            border_radius: BorderRadius::all(px(999)),
            display: Display::None,
            align_self: AlignSelf::FlexStart,
            ..default()
        },
        BackgroundColor(t::SOOT_700),
        StatusPill,
        children![(
            Text::new(""),
            crate::ui::body(size::LABEL, t::TEXT),
            Label::Status
        )],
    )
}

/// Join a friend: a cardboard card with the code box, Paste and Join, and the games open near you.
fn spawn_join_panel(commands: &mut Commands) {
    use crate::theme::kit::{self as t, size};
    use crate::ui::kit::{self, Glyph, Look, hint};
    use crate::ui::{body, head};
    commands
        .spawn((
            JoinPanel,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                display: Display::None,
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(t::SOOT_900.with_alpha(0.7)),
            GlobalZIndex(20),
            DespawnOnExit(AppState::Menu),
        ))
        .with_children(|scrim| {
            styled(scrim.spawn(kit::card(t::CARD_100, 0.0)), |n| {
                n.width = px(640);
                n.row_gap = px(10);
                n.padding = UiRect::all(px(t::EDGE + 8.0));
            })
            .with_children(|card| {
                card.spawn(kit::tape());
                card.spawn(kit::display("Join a friend", size::H1, t::INK));
                card.spawn((
                    Text::new(
                        "Ask the host for their code, or pick a commune that's open near you.",
                    ),
                    body(size::BODY_REG, t::INK),
                    crate::ui::Regular,
                ));
                card.spawn((
                    Text::new("JOIN CODE"),
                    body(size::LABEL, t::INK),
                    Node {
                        margin: UiRect::top(px(8)),
                        ..default()
                    },
                ));
                // The code box: type, Ctrl/Cmd+V, or click to paste; a pad's A opens the hex picker.
                card.spawn(Node {
                    column_gap: px(14),
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        CodeField,
                        crate::ui::pad_nav::PadFocusable,
                        Node {
                            flex_grow: 1.0,
                            height: px(t::BUTTON_H),
                            padding: UiRect::horizontal(px(16)),
                            align_items: AlignItems::Center,
                            border: UiRect::all(px(3)),
                            border_radius: BorderRadius::all(px(t::R_BUTTON)),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        BackgroundColor(t::CARD_100.mix(&Color::WHITE, 0.4)),
                        BorderColor::all(t::BUTTER_400),
                        children![(
                            Text::new(""),
                            body(size::BODY_REG, t::CARD_500),
                            TextLayout::no_wrap(),
                            Label::Code,
                        )],
                    ))
                    .observe(
                        |_: On<PointerClick>,
                         mut commands: Commands,
                         focus: Res<crate::ui::pad_nav::PadFocus>,
                         pickers: Query<(), With<HexPicker>>,
                         clip: Res<Clipboard>,
                         mut code: ResMut<JoinCode>| {
                            if focus.pad_click {
                                if pickers.is_empty() {
                                    commands.queue(spawn_picker);
                                }
                            } else if let Some(text) = clip.get() {
                                code.paste(&text);
                            }
                        },
                    );
                    row.spawn(kit::button(Look::Cardboard, "Paste", px(150)))
                        .observe(
                            |_: On<PointerClick>,
                             clip: Res<Clipboard>,
                             mut code: ResMut<JoinCode>| {
                                if let Some(text) = clip.get() {
                                    code.paste(&text);
                                }
                            },
                        );
                });
                card.spawn((
                    kit::button(Look::Primary, "Join", percent(100)),
                    Label::JoinButton,
                ))
                .observe(
                    |_: On<PointerClick>,
                     code: Res<JoinCode>,
                     mut connecting: ResMut<Connecting>,
                     mut status: ResMut<NetStatus>,
                     client: Option<Res<IrohClient>>| {
                        join(&code, &mut connecting, &mut status, client.is_some())
                    },
                );
                card.spawn((
                    Node {
                        height: px(2),
                        margin: UiRect::vertical(px(10)),
                        ..default()
                    },
                    BackgroundColor(t::CARD_500),
                ));
                card.spawn((Text::new("OPEN NEAR YOU"), body(size::LABEL, t::INK)));
                card.spawn((
                    LanEmpty,
                    Text::new("Nobody near you is hosting yet. Ask a friend to host."),
                    body(size::BODY_REG, t::INK),
                    crate::ui::Regular,
                ));
                for i in 0..LAN_ROWS {
                    card.spawn((
                        LanRow(i),
                        Node {
                            align_items: AlignItems::Center,
                            column_gap: px(14),
                            padding: UiRect::axes(px(14), px(8)),
                            border_radius: BorderRadius::all(px(6)),
                            display: Display::None,
                            ..default()
                        },
                        BackgroundColor(t::CARD_300),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Node {
                                width: px(22),
                                height: px(22),
                                border_radius: BorderRadius::MAX,
                                ..default()
                            },
                            BackgroundColor(t::seat(i + 1)),
                        ));
                        row.spawn((
                            LanRowName(i),
                            Node {
                                flex_grow: 1.0,
                                ..default()
                            },
                            Text::new(""),
                            head(size::H2, t::INK),
                        ));
                        row.spawn(kit::button(Look::Plain, "Join", px(120)))
                            .observe(
                            move |_: On<PointerClick>,
                                  games: Res<crate::lan::LanGames>,
                                  mut code: ResMut<JoinCode>,
                                  mut connecting: ResMut<Connecting>,
                                  mut status: ResMut<NetStatus>,
                                  client: Option<Res<IrohClient>>| {
                                if let Some(game) = games.0.get(i) {
                                    code.0.clone_from(&game.code);
                                    join(&code, &mut connecting, &mut status, client.is_some());
                                }
                            },
                        );
                    });
                }
                card.spawn(status_pill());
            });
            scrim.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    right: px(t::EDGE * 2.0),
                    bottom: px(t::EDGE),
                    column_gap: px(36),
                    ..default()
                },
                crate::ui::pad_nav::PadHint,
                Visibility::Hidden,
                children![
                    hint(Glyph::A, "Select"),
                    hint(Glyph::X, "Paste"),
                    hint(Glyph::B, "Back"),
                ],
            ));
        });
}

/// The sticky note's click: join the first game heard near you.
fn join_first_near(
    _: On<PointerClick>,
    games: Res<crate::lan::LanGames>,
    mut code: ResMut<JoinCode>,
    mut connecting: ResMut<Connecting>,
    mut status: ResMut<NetStatus>,
    client: Option<Res<IrohClient>>,
) {
    let Some(game) = games.0.first() else {
        return;
    };
    code.0.clone_from(&game.code);
    join(&code, &mut connecting, &mut status, client.is_some())
}

/// Pad shortcuts the hint bars promise: X pastes on the join card, Y joins the sticky note's game.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn pad_shortcuts(
    pads: Query<&Gamepad>,
    layers: Query<
        &Node,
        Or<(
            With<JoinPanel>,
            With<SettingsPanel>,
            With<crate::fault::FaultPanel>,
            With<crate::kori_shelf::ShelfPanel>,
        )>,
    >,
    join_panel: Query<&Node, With<JoinPanel>>,
    creator: Res<crate::creator::CreatorOpen>,
    picker: Query<(), With<HexPicker>>,
    clip: Res<Clipboard>,
    games: Res<crate::lan::LanGames>,
    mut code: ResMut<JoinCode>,
    mut connecting: ResMut<Connecting>,
    mut status: ResMut<NetStatus>,
    client: Option<Res<IrohClient>>,
) {
    let up = |n: &Node| n.display != Display::None;
    let pressed = |b| pads.iter().any(|p| p.just_pressed(b));
    if !picker.is_empty() {
        return;
    }
    if join_panel.iter().any(up) {
        if pressed(GamepadButton::West)
            && let Some(text) = clip.get()
        {
            code.paste(&text);
        }
    } else if !layers.iter().any(up)
        && !creator.0
        && pressed(GamepadButton::North)
        && let Some(game) = games.0.first()
    {
        code.0.clone_from(&game.code);
        join(&code, &mut connecting, &mut status, client.is_some());
    }
}

/// Show the games heard near you: the sticky note for the first, a row each on the join card.
#[allow(clippy::type_complexity)]
fn refresh_lan(
    lan: Res<crate::lan::LanGames>,
    join_panel: Query<&Node, With<JoinPanel>>,
    mut note: Query<
        &mut Node,
        (
            With<LanNote>,
            Without<LanRow>,
            Without<LanEmpty>,
            Without<JoinPanel>,
        ),
    >,
    mut note_text: Query<(&LanNoteText, &mut Text), (Without<LanRowName>, Without<LanEmpty>)>,
    mut rows: Query<
        (&LanRow, &mut Node),
        (Without<LanNote>, Without<LanEmpty>, Without<JoinPanel>),
    >,
    mut names: Query<(&LanRowName, &mut Text), (Without<LanNoteText>, Without<LanEmpty>)>,
    mut empty: Query<
        &mut Node,
        (
            With<LanEmpty>,
            Without<LanNote>,
            Without<LanRow>,
            Without<JoinPanel>,
        ),
    >,
) {
    let first = lan.0.first();
    // The sticky note gives way to the join card, which lists the same games.
    let card_up = join_panel.iter().any(|n| n.display != Display::None);
    for mut n in &mut note {
        n.display = if first.is_some() && !card_up {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (which, mut t) in &mut note_text {
        let want = match (which.0, first) {
            (_, None) => String::new(),
            (Cell::Who, Some(g)) => {
                format!("{} is hosting nearby", crate::ui::ellipsize(&g.host, 18))
            }
            (Cell::Join, Some(g)) => crate::lan::button_text(g),
        };
        if t.0 != want {
            t.0 = want;
        }
    }
    for (row, mut n) in &mut rows {
        n.display = if lan.0.get(row.0).is_some() {
            Display::Flex
        } else {
            Display::None
        };
    }
    for (name, mut t) in &mut names {
        let want = lan
            .0
            .get(name.0)
            .map(|g| format!("{}'s commune", crate::ui::ellipsize(&g.host, 20)))
            .unwrap_or_default();
        if t.0 != want {
            t.0 = want;
        }
    }
    for mut n in &mut empty {
        n.display = if lan.0.is_empty() {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// The settings notebook (`ui::settings_panel`).
pub(crate) use crate::ui::settings_panel::SettingsPanel;

fn toggle_settings(panel: &mut Query<&mut Node, With<SettingsPanel>>) {
    crate::ui::settings_panel::toggle(panel);
}

/// A tiny diorama of the meadow behind the menu: the real ground and grass
/// meshes, the pond with lily pads, a ring of the real CC0 trees, bushes and
/// rocks, the campfire with a few commune people, and bunting, seen by a
/// slowly orbiting camera. Shares its pipelines with the game, so it doubles as
/// a shader warm-up.
fn spawn_backdrop(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
) {
    let out = DespawnOnExit(AppState::Menu);
    let horizon = crate::atmosphere::horizon_colour();
    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(horizon),
            ..default()
        },
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        DistanceFog {
            color: horizon,
            falloff: FogFalloff::Linear {
                start: 30.0,
                end: 90.0,
            },
            ..default()
        },
        orbit(0.0),
        MenuCamera,
        // The character preview camera draws on top; the menu UI stays here.
        IsDefaultUiCamera,
        out.clone(),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.78, 0.86, 1.0),
        brightness: 500.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 5_000.0,
            color: Color::srgb(1.0, 0.93, 0.82),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(20.0, 30.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        out.clone(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(crate::terrain::build_mesh())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(crate::art::grass_texture(&mut images)),
            perceptual_roughness: 0.95,
            reflectance: 0.12,
            ..default()
        })),
        out.clone(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(crate::terrain::build_tufts())),
        MeshMaterial3d(materials.add(StandardMaterial {
            perceptual_roughness: 0.9,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        bevy::light::NotShadowCaster,
        out.clone(),
    ));
    let pond = crate::terrain::pond_at();
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(3.4).mesh().resolution(40))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.3, 0.55, 0.65, 0.7),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.08,
            reflectance: 0.6,
            ..default()
        })),
        Transform::from_xyz(pond.x, 0.03, pond.y)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        out.clone(),
    ));

    // Real CC0 trees, bushes and rocks in a loose ring (deterministic), with a
    // primitive stand-in only if a model fails to load (LFS data missing).
    for i in 0..18u32 {
        let a = i as f32 * 2.399;
        let r = 8.0 + (i * 7 % 8) as f32;
        let at = Vec2::new(a.cos() * r, a.sin() * r);
        if at.distance(pond) < 5.0 {
            continue;
        }
        let (file, scale, kind) = match i % 3 {
            0 | 1 => (
                scenery::TREES[i as usize % scenery::TREES.len()],
                0.6 + (i % 4) as f32 * 0.12,
                Stand::Tree,
            ),
            _ => (
                scenery::BUSHES[i as usize % scenery::BUSHES.len()],
                0.9,
                Stand::Bush,
            ),
        };
        backdrop_model(&mut commands, &assets, file, at, a * 3.0, scale, kind);
    }
    for (i, at) in [Vec2::new(-9.0, 6.0), Vec2::new(5.0, -11.0)]
        .into_iter()
        .enumerate()
    {
        let file = scenery::ROCKS[i % scenery::ROCKS.len()];
        backdrop_model(
            &mut commands,
            &assets,
            file,
            at,
            i as f32,
            0.55,
            Stand::Rock,
        );
    }

    // The campfire with its glow, two log seats and three commune people idling.
    scenery::spawn_bonfire(&mut commands, &assets, Vec3::ZERO, AppState::Menu);
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.6, 0.25),
            intensity: 25_000.0,
            range: 9.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 0.8, 0.0),
        out.clone(),
    ));
    // The scenery log seat (r 0.2 x 1.5 m, lying along its Z, origin at its
    // centre), scaled down to the old 0.18 x 1.4 m seat.
    for yaw in [0.6f32, 2.9] {
        let p = Vec2::from_angle(yaw) * 2.0;
        commands.spawn((
            scenery::scenery_model(&assets, "log_seat"),
            Transform::from_xyz(p.x, 0.18, p.y)
                .with_rotation(Quat::from_rotation_y(-yaw))
                .with_scale(Vec3::splat(0.92)),
            out.clone(),
        ));
    }
    for (i, ang) in [0.2f32, 2.2, 4.3].into_iter().enumerate() {
        let p = Vec2::from_angle(ang) * 2.6;
        // Face the fire (bodies look along -Z).
        let facing = Transform::from_xyz(p.x, 0.0, p.y).looking_at(Vec3::ZERO, Vec3::Y);
        commands.spawn((
            crate::people::Gait::default(),
            crate::people::CharacterLook::random(4242 + i as u64 * 7),
            facing,
            Visibility::default(),
            out.clone(),
        ));
    }

    // Bunting between two poles behind the fire.
    let flag = meshes.add(Triangle3d::new(
        Vec3::new(-0.16, 0.0, 0.0),
        Vec3::new(0.16, 0.0, 0.0),
        Vec3::new(0.0, -0.32, 0.0),
    ));
    let flags = [
        Color::srgb(0.95, 0.45, 0.45),
        Color::srgb(0.98, 0.8, 0.35),
        Color::srgb(0.5, 0.75, 0.95),
        Color::srgb(0.6, 0.85, 0.5),
        Color::srgb(0.95, 0.6, 0.85),
    ]
    .map(|c| {
        materials.add(StandardMaterial {
            base_color: c,
            double_sided: true,
            cull_mode: None,
            perceptual_roughness: 0.9,
            ..default()
        })
    });
    let (l, r) = (Vec2::new(-4.0, -3.0), Vec2::new(3.5, -4.5));
    for p in [l, r] {
        // Modeler pole (`models/menu/`), base at its origin.
        commands.spawn((
            WorldAssetRoot(
                assets.load(GltfAssetLabel::Scene(0).from_asset("models/menu/bunting_pole.gltf")),
            ),
            Transform::from_xyz(p.x, 0.0, p.y),
            out.clone(),
        ));
    }
    let along = (r - l).normalize_or(Vec2::X);
    let yaw = Quat::from_rotation_y(-along.to_angle());
    const FLAGS: usize = 14;
    for k in 0..FLAGS {
        let t = (k as f32 + 0.5) / FLAGS as f32;
        let xz = l.lerp(r, t);
        let y = 2.5 - 4.0 * t * (1.0 - t) * 0.5;
        commands.spawn((
            Mesh3d(flag.clone()),
            MeshMaterial3d(flags[k % flags.len()].clone()),
            Transform::from_xyz(xz.x, y, xz.y).with_rotation(yaw),
            bevy::light::NotShadowCaster,
            out.clone(),
        ));
    }

    // Lily pads with a pink bloom on the pond.
    let pad = meshes.add(CircularSector::new(0.35, std::f32::consts::PI * 0.9));
    let pad_mat = crate::art::soft(&mut materials, Color::srgb(0.3, 0.6, 0.25));
    let bloom = meshes.add(Sphere::new(0.09));
    let pink = crate::art::soft(&mut materials, Color::srgb(1.0, 0.6, 0.78));
    for (k, (a, r)) in [(2.4f32, 1.8f32), (3.6, 2.3), (5.2, 1.3)]
        .into_iter()
        .enumerate()
    {
        let p = pond + Vec2::from_angle(a) * r;
        commands.spawn((
            Mesh3d(pad.clone()),
            MeshMaterial3d(pad_mat.clone()),
            Transform::from_xyz(p.x, 0.045, p.y).with_rotation(
                Quat::from_rotation_y(a) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            ),
            bevy::light::NotShadowCaster,
            out.clone(),
        ));
        if k != 1 {
            commands.spawn((
                Mesh3d(bloom.clone()),
                MeshMaterial3d(pink.clone()),
                Transform::from_xyz(p.x, 0.1, p.y),
                bevy::light::NotShadowCaster,
                out.clone(),
            ));
        }
    }
}

/// What primitive stands in for a backdrop model whose file failed to load.
#[derive(Clone, Copy)]
enum Stand {
    Tree,
    Bush,
    Rock,
}

/// A backdrop model still waiting to see whether its file loads.
#[derive(Component)]
struct PendingModel(Stand);

fn backdrop_model(
    commands: &mut Commands,
    assets: &AssetServer,
    file: &str,
    at: Vec2,
    yaw: f32,
    scale: f32,
    kind: Stand,
) {
    let y = crate::terrain::height_at(at.x, at.y);
    commands.spawn((
        scenery::nature_model(assets, file),
        Transform::from_xyz(at.x, y, at.y)
            .with_rotation(Quat::from_rotation_y(yaw))
            .with_scale(Vec3::splat(scale)),
        crate::ambience::Sway::default(),
        PendingModel(kind),
        DespawnOnExit(AppState::Menu),
    ));
}

/// Swap failed backdrop models for primitives so the meadow is never empty.
fn backdrop_fallbacks(
    mut commands: Commands,
    assets: Res<AssetServer>,
    pending: Query<(Entity, &WorldAssetRoot, &PendingModel)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (e, root, kind) in &pending {
        let state = assets.load_state(root.0.id());
        if state.is_loaded() {
            commands.entity(e).remove::<PendingModel>();
            continue;
        }
        if !state.is_failed() {
            continue;
        }
        let (mesh, colour, lift): (Mesh, Color, f32) = match kind.0 {
            Stand::Tree => (Sphere::new(1.4).into(), Color::srgb(0.45, 0.72, 0.4), 2.6),
            Stand::Bush => (Sphere::new(0.8).into(), Color::srgb(0.5, 0.7, 0.4), 0.5),
            Stand::Rock => (
                Cuboid::new(2.4, 1.6, 2.0).into(),
                Color::srgb(0.6, 0.6, 0.62),
                0.6,
            ),
        };
        commands
            .entity(e)
            .remove::<(WorldAssetRoot, PendingModel)>()
            .with_children(|p| {
                p.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(materials.add(colour)),
                    Transform::from_xyz(0.0, lift, 0.0),
                ));
                if matches!(kind.0, Stand::Tree) {
                    p.spawn((
                        Mesh3d(meshes.add(Cylinder::new(0.25, 2.0))),
                        MeshMaterial3d(materials.add(Color::srgb(0.5, 0.36, 0.26))),
                        Transform::from_xyz(0.0, 1.0, 0.0),
                    ));
                }
            });
    }
}

fn orbit(t: f32) -> Transform {
    let a = t * 0.06;
    Transform::from_xyz(a.cos() * 26.0, 10.0, a.sin() * 26.0)
        .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y)
}

fn orbit_camera(time: Res<Time>, mut cams: Query<&mut Transform, With<MenuCamera>>) {
    for mut t in &mut cams {
        *t = orbit(time.elapsed_secs());
    }
}

/// Menu text uses the Atkinson Hyperlegible font once it has loaded (missing file:
/// Bevy's default font stays).
fn menu_font(
    font: Option<Res<crate::assets::UiFont>>,
    assets: Option<Res<AssetServer>>,
    mut texts: Query<&mut TextFont>,
) {
    let (Some(font), Some(assets)) = (font, assets) else {
        return;
    };
    if !font.ready(&assets) {
        return;
    }
    let default = FontSource::Handle(Handle::default());
    for mut tf in &mut texts {
        if tf.font == default {
            tf.font = FontSource::Handle(font.0.clone());
        }
    }
}

/// Tab toggles settings; Ctrl/Cmd+V pastes a join code.
fn menu_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    clip: Res<Clipboard>,
    mut code: ResMut<JoinCode>,
    mut panel: Query<&mut Node, With<SettingsPanel>>,
) {
    if keys.just_pressed(KeyCode::Tab) {
        toggle_settings(&mut panel);
    }
    let modifier = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    if modifier
        && keys.just_pressed(KeyCode::KeyV)
        && let Some(text) = clip.get()
    {
        code.paste(&text);
    }
}

/// Tint the news pills: butter for trouble, slime for good news.
fn style_status(
    status: Res<NetStatus>,
    mut pill: Query<(&mut Node, &mut BackgroundColor), With<StatusPill>>,
    mut label: Query<(&Label, &mut TextColor)>,
) {
    if !status.is_changed() {
        return;
    }
    let m = status.message.to_lowercase();
    let bad = m.contains("could not") || m.contains("doesn't") || m.contains("first");
    let good = m.contains("hosting") || m.contains("connected!");
    let tint = if bad {
        theme::kit::BUTTER_400
    } else if good {
        theme::kit::SLOP_400
    } else {
        theme::kit::TEXT
    };
    for (mut node, mut bg) in &mut pill {
        node.display = if m.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        bg.0 = theme::kit::SOOT_900;
    }
    for (l, mut c) in &mut label {
        if matches!(l, Label::Status) {
            c.0 = tint;
        }
    }
}

fn join(code: &JoinCode, connecting: &mut Connecting, status: &mut NetStatus, busy: bool) {
    if busy {
        return;
    }
    if code.0.is_empty() {
        status.message = "Type or paste a join code first.".into();
    } else if connecting.pending.is_none() {
        status.message = crate::input::fill("Connecting... {back} cancels.");
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
        commands.queue(crate::net::teardown_transport);
        connecting.waited = 0.0;
        status.message =
            "Could not reach that host (timed out). Check the code and try again.".into();
    }
}

/// The settings notebook's "Leave game" button (#317; Esc used to leave):
/// disconnect (or stop hosting) and go back to the menu.
pub(crate) fn leave(
    commands: &mut Commands,
    replicated: &Query<Entity, With<Replicated>>,
    mut status: ResMut<NetStatus>,
    mut next: ResMut<NextState<AppState>>,
) {
    commands.queue(crate::net::teardown_transport);
    commands.remove_resource::<LocalPlayer>();
    for e in replicated {
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

fn hover_buttons(
    mut buttons: Query<(&Hovered, &BaseColor, &mut BackgroundColor), Changed<Hovered>>,
) {
    for (hovered, base, mut bg) in &mut buttons {
        bg.0 = if hovered.get() {
            base.0.mix(&Color::WHITE, 0.2)
        } else {
            base.0
        };
    }
}

pub(crate) fn text(ui: &mut ChildSpawnerCommands, s: &str, size: f32, color: Color) {
    ui.spawn((
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(color),
    ));
}

pub(crate) fn row(ui: &mut ChildSpawnerCommands, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    ui.spawn(Node {
        column_gap: px(12),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(f);
}

fn type_join_code(
    mut keys: MessageReader<KeyboardInput>,
    mut code: ResMut<JoinCode>,
    mut connecting: ResMut<Connecting>,
    mut status: ResMut<NetStatus>,
    client: Option<Res<IrohClient>>,
) {
    let mut enter = false;
    for key in keys.read() {
        if key.state == ButtonState::Pressed {
            enter |= code.type_key(&key.logical_key);
        }
    }
    if enter {
        join(&code, &mut connecting, &mut status, client.is_some());
    }
}

fn refresh_labels(
    status: Res<NetStatus>,
    code: Res<JoinCode>,
    connecting: Res<Connecting>,
    client: Option<Res<IrohClient>>,
    joining: Option<Res<crate::net::JoinTask>>,
    time: Res<Time>,
    mut texts: Query<(&mut Text, &Label, Option<&mut TextColor>)>,
    buttons: Query<(&Label, &Children), Without<Text>>,
    mut child_texts: Query<&mut Text, Without<Label>>,
) {
    for (mut t, label, color) in &mut texts {
        let new = match label {
            Label::Status => status.message.clone(),
            Label::Code if code.0.is_empty() => "paste the host's code here".into(),
            Label::Code => code.0.clone(),
            Label::CodeTail if code.0.is_empty() => "(empty: pick 0-f, or Paste)".into(),
            Label::CodeTail => format!("{}  ({})", tail(&code.0, 22), code.0.len()),
            Label::JoinButton => continue,
        };
        if let (Label::Code, Some(mut c)) = (label, color) {
            c.0 = if code.0.is_empty() {
                theme::kit::CARD_500
            } else {
                theme::kit::INK
            };
        }
        if t.0 != new {
            t.0 = new;
        }
    }
    let dots = &"..."[..(time.elapsed_secs() * 3.0) as usize % 4];
    let join = if connecting.pending.is_some() || client.is_some() || joining.is_some() {
        format!("Joining{dots}")
    } else {
        "Join".to_string()
    };
    for (label, children) in &buttons {
        if !matches!(label, Label::JoinButton) {
            continue;
        }
        for child in children.iter() {
            if let Ok(mut t) = child_texts.get_mut(child)
                && t.0 != join
            {
                t.0.clone_from(&join);
            }
        }
    }
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
        // A share link pastes as its code, not every hex letter in the URL.
        c.paste("https://cafe.example/friendSLOP/?join=0123abcd&x=1");
        assert_eq!(c.0, "0123abcd");
        assert_eq!(code_in("https://a.b/?join=beef#top"), "beef");
        assert_eq!(code_in("beef"), "beef");
    }

    #[test]
    fn hex_picker_types_deletes_pastes_and_joins() {
        let mut c = JoinCode::default();
        for k in [PickKey::Hex('a'), PickKey::Hex('0'), PickKey::Hex('f')] {
            assert!(!k.press(&mut c, None));
        }
        assert_eq!(c.0, "a0f");
        PickKey::Back.press(&mut c, None);
        assert_eq!(c.0, "a0");
        PickKey::Paste.press(&mut c, Some("https://x.y/?join=beef".into()));
        assert_eq!(c.0, "beef");
        // Nothing on the clipboard: the code stays.
        PickKey::Paste.press(&mut c, None);
        assert_eq!(c.0, "beef");
        assert!(PickKey::Join.press(&mut c, None));
        // Every hex digit has a key, once.
        let hex: String = PICKER_ROWS
            .iter()
            .flat_map(|r| r.iter())
            .filter_map(|k| match k {
                PickKey::Hex(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert_eq!(hex, "0123456789abcdef");
        assert_eq!(tail("0123456789", 4), "…6789");
        assert_eq!(tail("abc", 4), "abc");
    }

    #[test]
    fn b_cancels_a_join_and_is_spent() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<NetStatus>()
            .insert_resource(Connecting {
                pending: Some(("beef".into(), 5)),
                waited: 1.0,
            })
            .add_systems(Update, cancel_join);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(app.world().resource::<Connecting>().pending.is_none());
        // The join card behind it must not also see this Esc.
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::Escape)
        );
        assert_eq!(
            app.world().resource::<NetStatus>().message,
            "Join cancelled."
        );
    }
}
