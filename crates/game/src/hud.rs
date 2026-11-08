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
    /// Transient messages under the objective; they fade out a few seconds
    /// after they last changed.
    Toasts,
    /// Static explanations ("click X to do Y"), only shown in the H overlay.
    Help,
}

impl HudSlot {
    pub const ALL: [HudSlot; 10] = [
        HudSlot::TopLeft,
        HudSlot::TopCenter,
        HudSlot::TopRight,
        HudSlot::BottomLeft,
        HudSlot::BottomRight,
        HudSlot::Subtitle,
        HudSlot::CenterBanner,
        HudSlot::Scoreboard,
        HudSlot::Toasts,
        HudSlot::Help,
    ];

    /// Slots whose lines fade out a while after they last changed.
    pub fn fades(self) -> bool {
        matches!(self, HudSlot::Toasts | HudSlot::Subtitle)
    }

    /// Slots drawn as one big panel (instead of a chip per line).
    fn one_panel(self) -> bool {
        matches!(self, HudSlot::Scoreboard | HudSlot::Help)
    }

    fn node(self) -> Node {
        let m = px(12);
        let mut n = Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            row_gap: px(5),
            max_width: percent(30),
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
                (n.left, n.right) = (percent(30), percent(30));
                n.max_width = Val::Auto;
                n.align_items = AlignItems::Center;
            }
            HudSlot::Toasts => {
                n.top = px(96);
                (n.left, n.right) = (percent(30), percent(30));
                n.max_width = Val::Auto;
                n.align_items = AlignItems::Center;
            }
            HudSlot::Help => {
                n.top = percent(10);
                (n.left, n.right) = (percent(28), percent(28));
                n.max_width = Val::Auto;
                n.padding = UiRect::all(px(18));
                n.row_gap = px(8);
                n.border_radius = BorderRadius::all(px(14));
            }
            HudSlot::Subtitle => {
                n.bottom = px(72);
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
                n.row_gap = px(8);
                n.border_radius = BorderRadius::all(px(14));
            }
        }
        n
    }
}

/// Put this on a feature's HUD text instead of a positioned `Node`.
/// If the entity has no `TextFont`/`TextColor`, [`normal`] style is applied.
pub fn slot(slot: HudSlot) -> impl Bundle {
    InSlot(slot)
}

#[derive(Component, Clone, Copy, Debug)]
pub struct InSlot(pub HudSlot);

/// Put a slotted line at the top of its slot instead of the bottom (titles).
#[derive(Component, Clone, Copy, Debug)]
pub struct First;

/// Longest a mirrored own-stats line gets before it's cut short (full text: Tab).
const OWN_LINE_CHARS: usize = 44;

pub const TEXT: Color = Color::srgb(0.97, 0.96, 0.99);
pub const DIM: Color = Color::srgb(0.75, 0.74, 0.8);
pub const WARN: Color = Color::srgb(1.0, 0.45, 0.4);
pub const ACCENT: Color = Color::srgb(1.0, 0.86, 0.5);
const PANEL: Color = Color::srgba(0.06, 0.05, 0.1, 0.55);
const BIG_PANEL: Color = Color::srgba(0.05, 0.04, 0.08, 0.88);
/// How long a fading line stays fully visible, then how long it takes to go.
const FADE_HOLD: f32 = 5.0;
const FADE_OUT: f32 = 1.2;

pub fn normal() -> (TextFont, TextColor) {
    (TextFont::from_font_size(13.0), TextColor(TEXT))
}
pub fn small() -> (TextFont, TextColor) {
    (TextFont::from_font_size(12.0), TextColor(DIM))
}
pub fn warning() -> (TextFont, TextColor) {
    (TextFont::from_font_size(13.0), TextColor(WARN))
}
pub fn banner() -> (TextFont, TextColor) {
    (TextFont::from_font_size(26.0), TextColor(TEXT))
}
/// Section title (the "Commune of Dave" banner, overlay headings).
pub fn title() -> (TextFont, TextColor) {
    (TextFont::from_font_size(13.0), TextColor(ACCENT))
}

/// The rounded chip behind one line of slotted HUD text.
fn chip() -> (Node, BackgroundColor) {
    (
        Node {
            padding: UiRect::axes(px(9), px(5)),
            border_radius: BorderRadius::all(px(8)),
            ..default()
        },
        BackgroundColor(PANEL),
    )
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
    ("WASD", "walk (first person) / pan (overview)"),
    ("Mouse", "look around (first person)"),
    ("Space / Shift", "jump / run"),
    ("V", "first person <-> overview camera"),
    (
        "Left click",
        "use what's under the crosshair (or cursor), within reach",
    ),
    ("M", "mute / unmute mic"),
    ("F", "fart (where you stand)"),
    ("[ ] - = ; '", "fart tone"),
    ("G", "pry at HALT"),
    ("K", "start hide and seek (2+ players)"),
    ("R", "cast a totem spell"),
    ("1-9", "challenge that player to a duel"),
    ("Y / N", "accept / refuse"),
    ("C", "copy the join code again"),
    ("Tab (hold)", "scoreboard"),
    ("H", "this help"),
    ("F3", "FPS counter"),
    ("Esc", "free the mouse; again to leave (disconnects)"),
];

pub fn help_text() -> String {
    let mut s = String::new();
    for (k, v) in KEY_BINDINGS {
        s.push_str(&format!("{k:>12}   {v}\n"));
    }
    s.trim_end().to_string()
}

/// Shorten a long join code for display: `a6c9e6...aef0`.
pub fn short_code(code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    if chars.len() <= 12 {
        return code.to_string();
    }
    let head: String = chars[..6].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}...{tail}")
}

/// Cut a line down to [`OWN_LINE_CHARS`], ending it with "..." if it was longer.
pub fn clip(line: &str) -> String {
    if line.chars().count() <= OWN_LINE_CHARS {
        return line.to_string();
    }
    let head: String = line.chars().take(OWN_LINE_CHARS - 3).collect();
    format!("{}...", head.trim_end())
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
/// A line in a fading slot: what it said last and how long ago that changed.
#[derive(Component, Default)]
struct Fade {
    last: String,
    age: f32,
}
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
                    fade_lines,
                    apply_font,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

fn spawn_slots(mut commands: Commands) {
    for s in HudSlot::ALL {
        let mut e = commands.spawn((s.node(), SlotRoot(s), DespawnOnExit(AppState::InGame)));
        match s {
            HudSlot::Scoreboard => {
                e.insert((
                    BackgroundColor(BIG_PANEL),
                    GlobalZIndex(10),
                    Visibility::Hidden,
                ));
                e.with_children(|c| {
                    c.spawn((Text::new("scoreboard"), title()));
                });
            }
            HudSlot::Help => {
                e.insert((
                    BackgroundColor(BIG_PANEL),
                    GlobalZIndex(20),
                    Visibility::Hidden,
                ));
                e.with_children(|c| {
                    c.spawn((Text::new("controls  (H to close)"), title()));
                    c.spawn((Text::new(help_text()), small()));
                    c.spawn((Text::new("what's going on"), title()));
                });
            }
            _ => {}
        }
    }
    commands.spawn((
        Text::new(""),
        OwnLinesText,
        slot(HudSlot::BottomLeft),
        small(),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        ToastText,
        slot(HudSlot::Toasts),
        TextFont::from_font_size(14.0),
        TextColor(ACCENT),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        FpsText,
        TextFont::from_font_size(12.0),
        TextColor(Color::srgb(0.5, 1.0, 0.5)),
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

/// Give HUD text the rounded UI font once it has loaded. Text whose font was
/// left at the default is switched; a missing font file keeps the default.
#[allow(clippy::type_complexity)]
fn apply_font(
    font: Option<Res<crate::assets::UiFont>>,
    assets: Option<Res<AssetServer>>,
    mut done_full_pass: Local<bool>,
    mut texts: Query<(&mut TextFont, Has<InSlot>, Has<FpsText>, Option<&ChildOf>)>,
    roots: Query<(), With<SlotRoot>>,
) {
    let (Some(font), Some(assets)) = (font, assets) else {
        return;
    };
    if !font.ready(&assets) {
        return;
    }
    let full = !*done_full_pass;
    *done_full_pass = true;
    let default = FontSource::Handle(Handle::default());
    for (mut tf, slotted, fps, parent) in &mut texts {
        if !full && !tf.is_changed() {
            continue;
        }
        let hud = slotted || fps || parent.is_some_and(|p| roots.contains(p.parent()));
        if hud && tf.font == default {
            tf.font = FontSource::Handle(font.0.clone());
        }
    }
}

/// Move newly slotted text into its slot column and give it the default style.
fn adopt_slotted(
    mut commands: Commands,
    new: Query<(Entity, &InSlot, Has<First>), Without<ChildOf>>,
    roots: Query<(Entity, &SlotRoot)>,
) {
    for (e, InSlot(s), first) in &new {
        let Some((root, _)) = roots.iter().find(|(_, r)| r.0 == *s) else {
            continue;
        };
        let mut ec = commands.entity(e);
        if s.one_panel() {
            ec.insert_if_new(small());
        } else {
            ec.insert_if_new(normal()).insert(chip());
        }
        if s.fades() {
            ec.insert(Fade::default());
        }
        if first {
            commands.entity(root).insert_children(0, &[e]);
        } else {
            ec.insert(ChildOf(root));
        }
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
            lines.extend(own_lines(&t.0, me.0).into_iter().map(|l| clip(&l)));
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
    mut fps: Query<&mut Visibility, (With<FpsText>, Without<SlotRoot>)>,
) {
    let flip = |v: &mut Visibility| {
        *v = if *v == Visibility::Hidden {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    };
    let tab = keys.pressed(KeyCode::Tab);
    for (r, mut v) in &mut roots {
        match r.0 {
            HudSlot::Scoreboard => {
                v.set_if_neq(if tab {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
            }
            HudSlot::Help if keys.just_pressed(KeyCode::KeyH) => flip(&mut v),
            _ => {}
        }
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
        // Only touch the text when the number changes (avoids a relayout per frame).
        let want = format!("{fps:.0} fps");
        if t.0 != want {
            t.0 = want;
        }
    }
}

/// Collapse empty lines so their chips disappear.
fn hide_empty(mut texts: Query<(&Text, &mut Node), (With<InSlot>, Without<Fade>, Changed<Text>)>) {
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
}

/// How visible a fading line is, `age` seconds after it last changed.
pub fn fade_alpha(age: f32) -> f32 {
    (1.0 - (age - FADE_HOLD) / FADE_OUT).clamp(0.0, 1.0)
}

/// Lines in fading slots show up when their text changes, then fade away.
fn fade_lines(
    time: Res<Time>,
    mut texts: Query<(
        &Text,
        &mut Fade,
        &mut Node,
        &mut TextColor,
        &mut BackgroundColor,
    )>,
) {
    for (t, mut f, mut n, mut color, mut bg) in &mut texts {
        if f.last != t.0 {
            f.last.clone_from(&t.0);
            f.age = 0.0;
        } else {
            f.age += time.delta_secs();
        }
        let a = fade_alpha(f.age);
        let d = if t.0.trim().is_empty() || a <= 0.0 {
            Display::None
        } else {
            Display::Flex
        };
        if n.display != d {
            n.display = d;
        }
        if (color.0.alpha() - a).abs() > 0.01 {
            color.0.set_alpha(a);
            bg.0 = PANEL.with_alpha(PANEL.alpha() * a);
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
        assert_eq!(short_code(&long), "a6c9e6...aef0");
    }

    #[test]
    fn own_lines_match_whole_ids() {
        let t = "stats\nplayer 1: a\nplayer 12: b\ncrystal: held by player 1";
        assert_eq!(own_lines(t, 1), vec!["you: a", "crystal: held by you"]);
        assert!(own_lines(t, 2).is_empty());
    }

    #[test]
    fn toasts_fade_out() {
        assert_eq!(fade_alpha(0.0), 1.0);
        assert!(fade_alpha(FADE_HOLD + FADE_OUT / 2.0) < 1.0);
        assert_eq!(fade_alpha(FADE_HOLD + FADE_OUT + 0.1), 0.0);
    }

    #[test]
    fn help_lists_every_binding() {
        let h = help_text();
        for k in [
            "M", "F", "G", "R", "1-9", "Y / N", "WASD", "V", "Tab", "H", "F3", "Esc",
        ] {
            assert!(h.contains(k), "{k}");
        }
    }
}
