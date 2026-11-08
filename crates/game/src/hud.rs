//! Shared in-game HUD layout: named slots, text styles, a Tab scoreboard, a
//! toast line, an H help overlay and an F3 FPS counter.
//!
//! Features don't position their own text. They spawn it with
//! [`slot`] (plus optionally one of the style helpers) and the HUD moves it into
//! the right flex column, each with a readable semi-opaque panel behind it.
//! Per-player tables go in [`HudSlot::Scoreboard`]: they're shown while Tab is
//! held, and the local player's own lines are mirrored into the normal HUD.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::AppState;
use crate::player::LocalPlayer;

/// Where a piece of HUD text lives.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HudSlot {
    TopLeft,
    /// Objective / status line, top centre.
    TopCenter,
    TopRight,
    BottomLeft,
    BottomRight,
    /// Bottom-centre subtitles (who said what).
    Subtitle,
    /// Big centred banner text.
    CenterBanner,
    /// Per-player tables, only visible while Tab is held.
    Scoreboard,
}

impl HudSlot {
    pub const ALL: [HudSlot; 8] = [
        HudSlot::TopLeft,
        HudSlot::TopCenter,
        HudSlot::TopRight,
        HudSlot::BottomLeft,
        HudSlot::BottomRight,
        HudSlot::Subtitle,
        HudSlot::CenterBanner,
        HudSlot::Scoreboard,
    ];

    fn node(self) -> Node {
        let m = px(12);
        let mut n = Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            padding: UiRect::axes(px(10), px(8)),
            border_radius: BorderRadius::all(px(10)),
            max_width: percent(32),
            ..default()
        };
        match self {
            HudSlot::TopLeft => (n.top, n.left) = (m, m),
            HudSlot::TopRight => {
                (n.top, n.right) = (m, m);
                n.align_items = AlignItems::FlexEnd;
            }
            HudSlot::BottomLeft => (n.bottom, n.left) = (m, m),
            HudSlot::BottomRight => {
                (n.bottom, n.right) = (m, m);
                n.align_items = AlignItems::FlexEnd;
            }
            HudSlot::TopCenter => {
                n.top = m;
                (n.left, n.right) = (percent(34), percent(34));
                n.max_width = Val::Auto;
                n.align_items = AlignItems::Center;
            }
            HudSlot::Subtitle => {
                n.bottom = px(64);
                (n.left, n.right) = (percent(30), percent(30));
                n.max_width = Val::Auto;
                n.align_items = AlignItems::Center;
            }
            HudSlot::CenterBanner => {
                n.top = percent(30);
                (n.left, n.right) = (percent(15), percent(15));
                n.max_width = Val::Auto;
                n.align_items = AlignItems::Center;
            }
            HudSlot::Scoreboard => {
                n.top = percent(15);
                (n.left, n.right) = (percent(20), percent(20));
                n.max_width = Val::Auto;
                n.padding = UiRect::all(px(16));
            }
        }
        n
    }
}

/// Put this on a feature's HUD text instead of a positioned `Node`.
/// If the entity has no `TextFont`/`TextColor`, [`normal`] style is applied.
pub fn slot(slot: HudSlot) -> impl Bundle {
    (InSlot(slot), TextShadow::default())
}

#[derive(Component, Clone, Copy, Debug)]
pub struct InSlot(pub HudSlot);

pub const TEXT: Color = Color::srgb(0.97, 0.96, 0.99);
pub const DIM: Color = Color::srgb(0.75, 0.74, 0.8);
pub const WARN: Color = Color::srgb(1.0, 0.45, 0.4);
const PANEL: Color = Color::srgba(0.05, 0.04, 0.08, 0.62);

pub fn normal() -> (TextFont, TextColor) {
    (TextFont::from_font_size(16.0), TextColor(TEXT))
}
pub fn small() -> (TextFont, TextColor) {
    (TextFont::from_font_size(13.0), TextColor(DIM))
}
pub fn warning() -> (TextFont, TextColor) {
    (TextFont::from_font_size(16.0), TextColor(WARN))
}
pub fn banner() -> (TextFont, TextColor) {
    (TextFont::from_font_size(40.0), TextColor(TEXT))
}

/// One line of client-local feedback ("copied!", "can't do that").
#[derive(Resource, Default, Debug)]
pub struct Toast {
    pub text: String,
    pub secs_left: f32,
}

impl Toast {
    pub const SECS: f32 = 3.0;
    pub fn show(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.secs_left = Self::SECS;
    }
}

/// Every key binding in the game, for the H help overlay. Add yours here.
pub const KEY_BINDINGS: &[(&str, &str)] = &[
    ("WASD", "move"),
    ("Left click", "interact / pay / use what's under the cursor"),
    ("M", "mute / unmute mic"),
    ("F", "fart at the cursor"),
    ("[ ] - = ; '", "fart tone"),
    ("G", "pry at HALT"),
    ("R", "cast a totem spell"),
    ("1-9", "challenge that player to a duel"),
    ("Y / N", "accept / refuse"),
    ("C", "copy the join code again"),
    ("Tab (hold)", "scoreboard"),
    ("H", "this help"),
    ("F3", "FPS counter"),
    ("Esc", "leave to the menu (disconnects)"),
];

pub fn help_text() -> String {
    let mut s = String::from("controls (H to close)\n");
    for (k, v) in KEY_BINDINGS {
        s.push_str(&format!("\n{k:>12}   {v}"));
    }
    s
}

/// Shorten a long join code for display: `a6c9e6…aef0`.
pub fn short_code(code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    if chars.len() <= 12 {
        return code.to_string();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// The lines of a scoreboard table that are about `me` ("player 3" but not "player 31"),
/// with "player N" replaced by "you".
pub fn own_lines(table: &str, me: u64) -> Vec<String> {
    let needle = format!("player {me}");
    table
        .lines()
        .filter_map(|line| {
            let mut out = String::new();
            let mut rest = line;
            let mut hit = false;
            while let Some(i) = rest.find(&needle) {
                let after = &rest[i + needle.len()..];
                let boundary = !after.starts_with(|c: char| c.is_ascii_digit());
                out.push_str(&rest[..i]);
                if boundary {
                    out.push_str("you");
                    hit = true;
                } else {
                    out.push_str(&needle);
                }
                rest = after;
            }
            out.push_str(rest);
            hit.then_some(out)
        })
        .collect()
}

#[derive(Component)]
struct SlotRoot(HudSlot);
#[derive(Component)]
struct ToastText;
#[derive(Component)]
struct OwnLinesText;
#[derive(Component)]
struct HelpOverlay;
#[derive(Component)]
struct FpsText;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        app.init_resource::<Toast>()
            .add_systems(OnEnter(AppState::InGame), spawn_slots)
            .add_systems(
                Update,
                (
                    adopt_slotted,
                    mirror_own_lines,
                    tick_toast,
                    toggle_overlays,
                    show_fps,
                    hide_empty,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn spawn_slots(mut commands: Commands) {
    for s in HudSlot::ALL {
        let mut e = commands.spawn((
            s.node(),
            BackgroundColor(PANEL),
            SlotRoot(s),
            DespawnOnExit(AppState::InGame),
        ));
        if s == HudSlot::Scoreboard {
            e.insert((GlobalZIndex(10), Visibility::Hidden));
            e.with_children(|c| {
                c.spawn((
                    Text::new("scoreboard (hold Tab)"),
                    TextFont::from_font_size(20.0),
                    TextColor(TEXT),
                ));
            });
        }
    }
    commands.spawn((
        Text::new(""),
        OwnLinesText,
        slot(HudSlot::BottomLeft),
        normal(),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        ToastText,
        slot(HudSlot::TopCenter),
        TextFont::from_font_size(18.0),
        TextColor(Color::srgb(1.0, 0.9, 0.5)),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(help_text()),
        HelpOverlay,
        TextFont::from_font_size(17.0),
        TextColor(TEXT),
        Node {
            position_type: PositionType::Absolute,
            top: percent(12),
            left: percent(30),
            padding: UiRect::all(px(20)),
            border_radius: BorderRadius::all(px(12)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.05, 0.04, 0.08, 0.9)),
        GlobalZIndex(20),
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        FpsText,
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.5, 1.0, 0.5)),
        TextShadow::default(),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(2),
            left: percent(50),
            ..default()
        },
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
}

/// Move newly slotted text into its slot column and give it the default style.
fn adopt_slotted(
    mut commands: Commands,
    new: Query<(Entity, &InSlot), Without<ChildOf>>,
    roots: Query<(Entity, &SlotRoot)>,
) {
    for (e, InSlot(s)) in &new {
        let Some((root, _)) = roots.iter().find(|(_, r)| r.0 == *s) else {
            continue;
        };
        commands.entity(e).insert_if_new(normal());
        commands.entity(root).add_child(e);
    }
}

fn mirror_own_lines(
    me: Option<Res<LocalPlayer>>,
    tables: Query<(&Text, &InSlot), Without<OwnLinesText>>,
    mut own: Query<&mut Text, With<OwnLinesText>>,
) {
    let Some(me) = me else { return };
    let mut lines = Vec::new();
    for (t, s) in &tables {
        if s.0 == HudSlot::Scoreboard {
            lines.extend(own_lines(&t.0, me.0));
        }
    }
    let want = lines.join("\n");
    for mut t in &mut own {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

fn tick_toast(
    time: Res<Time>,
    mut toast: ResMut<Toast>,
    mut text: Query<&mut Text, With<ToastText>>,
) {
    if toast.secs_left > 0.0 {
        toast.secs_left -= time.delta_secs();
        if toast.secs_left <= 0.0 {
            toast.text.clear();
        }
    }
    for mut t in &mut text {
        if t.0 != toast.text {
            t.0.clone_from(&toast.text);
        }
    }
}

fn toggle_overlays(
    keys: Res<ButtonInput<KeyCode>>,
    mut roots: Query<(&SlotRoot, &mut Visibility)>,
    mut help: Query<&mut Visibility, (With<HelpOverlay>, Without<SlotRoot>, Without<FpsText>)>,
    mut fps: Query<&mut Visibility, (With<FpsText>, Without<SlotRoot>, Without<HelpOverlay>)>,
) {
    let tab = keys.pressed(KeyCode::Tab);
    for (r, mut v) in &mut roots {
        if r.0 == HudSlot::Scoreboard {
            v.set_if_neq(if tab {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            });
        }
    }
    let flip = |v: &mut Visibility| {
        *v = if *v == Visibility::Hidden {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    };
    if keys.just_pressed(KeyCode::KeyH) {
        help.iter_mut().for_each(|mut v| flip(&mut v));
    }
    if keys.just_pressed(KeyCode::F3) {
        fps.iter_mut().for_each(|mut v| flip(&mut v));
    }
}

fn show_fps(diag: Res<DiagnosticsStore>, mut text: Query<(&mut Text, &Visibility), With<FpsText>>) {
    for (mut t, v) in &mut text {
        if *v == Visibility::Hidden {
            continue;
        }
        let fps = diag
            .get(&FrameTimeDiagnosticsPlugin::FPS)
            .and_then(|d| d.smoothed())
            .unwrap_or(0.0);
        t.0 = format!("{fps:.0} fps");
    }
}

/// Collapse empty lines and hide panels with nothing in them.
fn hide_empty(
    mut texts: Query<(&Text, &mut Node), (With<InSlot>, Changed<Text>)>,
    mut roots: Query<(&SlotRoot, &Children, &mut BackgroundColor)>,
    all_texts: Query<&Text, With<InSlot>>,
) {
    for (t, mut n) in &mut texts {
        let d = if t.0.trim().is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        if n.display != d {
            n.display = d;
        }
    }
    for (r, children, mut bg) in &mut roots {
        if r.0 == HudSlot::Scoreboard {
            continue;
        }
        let any = children
            .iter()
            .any(|c| all_texts.get(c).is_ok_and(|t| !t.0.trim().is_empty()));
        let want = if any { PANEL } else { Color::NONE };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_code_truncates() {
        assert_eq!(short_code("abc"), "abc");
        let long = format!("a6c9e6{}aef0", "0".repeat(54));
        assert_eq!(short_code(&long), "a6c9e6…aef0");
    }

    #[test]
    fn own_lines_match_whole_ids() {
        let t = "stats\nplayer 1: a\nplayer 12: b\ncrystal: held by player 1";
        assert_eq!(own_lines(t, 1), vec!["you: a", "crystal: held by you"]);
        assert!(own_lines(t, 2).is_empty());
    }

    #[test]
    fn help_lists_every_binding() {
        let h = help_text();
        for k in [
            "M", "F", "G", "R", "1-9", "Y / N", "WASD", "Tab", "H", "F3", "Esc",
        ] {
            assert!(h.contains(k), "{k}");
        }
    }
}
