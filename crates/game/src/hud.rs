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
use crate::theme::{self, Pop};

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
            row_gap: px(theme::S1 + 2.0),
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
                n.top = px(150);
                (n.left, n.right) = (percent(30), percent(30));
                n.max_width = Val::Auto;
                n.align_items = AlignItems::Center;
            }
            HudSlot::Help => {
                n.top = percent(10);
                (n.left, n.right) = (percent(28), percent(28));
                n.max_width = Val::Auto;
                n.padding = UiRect::all(px(theme::S4 + 4.0));
                n.row_gap = px(theme::S2);
                n.border = UiRect::all(px(2));
                n.border_radius = BorderRadius::all(px(theme::R_PANEL));
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
                n.padding = UiRect::all(px(theme::S4));
                n.row_gap = px(theme::S2);
                n.border = UiRect::all(px(2));
                n.border_radius = BorderRadius::all(px(theme::R_PANEL));
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

// HUD lines sit on cream chips, so text is plum ink with berry accents.
pub const TEXT: Color = theme::INK;
pub const DIM: Color = theme::SOFT_INK;
pub const WARN: Color = theme::ROSE_RED;
pub const ACCENT: Color = theme::BERRY;
/// Light text for labels floating over the world (no chip behind them).
pub const LABEL: Color = theme::CREAM;
const PANEL: Color = theme::CREAM_GLASS;
const BIG_PANEL: Color = theme::CARD;
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
/// A cute rounded display-font heading (falls back to the HUD font).
pub fn heading(size: f32) -> impl Bundle {
    (
        TextFont::from_font_size(size),
        TextColor(ACCENT),
        crate::assets::DisplayText,
    )
}
/// Section title (the "Commune of Dave" banner, overlay headings).
pub fn title() -> (TextFont, TextColor) {
    (TextFont::from_font_size(13.0), TextColor(ACCENT))
}

/// The rounded chip behind one line of slotted HUD text: cream paper, a thin
/// rosy outline and a soft shadow. Banners and the objective get a bit more air.
fn chip(slot: HudSlot) -> impl Bundle {
    let (pad, radius, line) = match slot {
        HudSlot::CenterBanner => (UiRect::axes(px(26), px(16)), theme::R_PANEL, theme::PINK),
        HudSlot::TopCenter => (
            UiRect::axes(px(theme::S4), px(theme::S2)),
            theme::R_PANEL,
            theme::BUTTER,
        ),
        HudSlot::Toasts => (
            UiRect::axes(px(theme::S3 + 2.0), px(theme::S1 + 2.0)),
            theme::R_PILL,
            theme::LILAC,
        ),
        _ => (
            UiRect::axes(px(theme::S3 - 1.0), px(theme::S1 + 2.0)),
            theme::R_CHIP,
            theme::LINE,
        ),
    };
    let border = if matches!(slot, HudSlot::TopCenter | HudSlot::CenterBanner) {
        2.0
    } else {
        1.5
    };
    (
        Node {
            padding: pad,
            border: UiRect::all(px(border)),
            border_radius: BorderRadius::all(px(radius)),
            ..default()
        },
        BackgroundColor(PANEL),
        BorderColor::all(line),
        theme::shadow(3.0, 10.0),
        ChipLine(line),
    )
}

/// The outline colour a chip was given (fading lines fade it too).
#[derive(Component, Clone, Copy)]
struct ChipLine(Color);

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
            .add_systems(OnExit(AppState::InGame), |mut t: ResMut<Toast>| {
                *t = Toast::default()
            })
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
        let panel = (
            BackgroundColor(BIG_PANEL),
            BorderColor::all(theme::LINE),
            theme::shadow(8.0, 28.0),
            UiTransform::default(),
        );
        // Every root carries a Pop so overlays can bounce in when shown.
        e.insert(Pop::default());
        match s {
            HudSlot::Scoreboard => {
                e.insert((panel, GlobalZIndex(10), Visibility::Hidden));
                e.with_children(|c| {
                    header(c, "scoreboard", theme::MINT);
                });
            }
            HudSlot::Help => {
                e.insert((panel, GlobalZIndex(20), Visibility::Hidden));
                e.with_children(|c| {
                    header(c, "controls  (H to close)", theme::PINK);
                    c.spawn((Text::new(help_text()), small()));
                    header(c, "what's going on", theme::LILAC);
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
        TextFont::from_font_size(15.0),
        TextColor(ACCENT),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        FpsText,
        TextFont::from_font_size(12.0),
        TextColor(theme::INK),
        Node {
            position_type: PositionType::Absolute,
            bottom: px(4),
            left: percent(50),
            padding: UiRect::axes(px(8), px(2)),
            border_radius: BorderRadius::all(px(theme::R_PILL)),
            ..default()
        },
        BackgroundColor(theme::MINT.with_alpha(0.85)),
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

/// An overlay heading: three little pastel dots, then a display-font title.
fn header(c: &mut ChildSpawnerCommands, label: &str, dot: Color) {
    c.spawn(Node {
        column_gap: px(theme::S1 + 1.0),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(|r| {
        for col in [dot, theme::BUTTER, theme::PINK.mix(&dot, 0.5)] {
            r.spawn((
                Node {
                    width: px(9),
                    height: px(9),
                    border_radius: BorderRadius::all(px(5)),
                    ..default()
                },
                BackgroundColor(col),
            ));
        }
        r.spawn((
            Text::new(label),
            heading(17.0),
            Node {
                margin: UiRect::left(px(theme::S1)),
                ..default()
            },
        ));
    });
}

/// Move newly slotted text into its slot column and give it the default style.
fn adopt_slotted(
    mut commands: Commands,
    new: Query<(Entity, &InSlot, Has<First>, Option<&TextColor>), Without<ChildOf>>,
    roots: Query<(Entity, &SlotRoot)>,
) {
    for (e, InSlot(s), first, color) in &new {
        let Some((root, _)) = roots.iter().find(|(_, r)| r.0 == *s) else {
            continue;
        };
        let mut ec = commands.entity(e);
        if s.one_panel() {
            ec.insert_if_new(small());
        } else {
            ec.insert_if_new(normal()).insert(chip(*s));
        }
        // Text is required to have a colour, so unstyled lines arrive white
        // (Bevy's default); give them the readable ink for the cream chips.
        if color.is_none_or(|c| c.0 == Color::WHITE) {
            ec.insert(TextColor(if s.one_panel() { DIM } else { TEXT }));
        }
        if s.fades() {
            ec.insert(Fade::default());
        }
        if matches!(
            s,
            HudSlot::Toasts | HudSlot::CenterBanner | HudSlot::Subtitle
        ) {
            ec.insert((Pop::now(0.7), UiTransform::default()));
        }
        if *s == HudSlot::CenterBanner {
            ec.insert(crate::assets::DisplayText);
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
    mut roots: Query<(&SlotRoot, &mut Visibility, &mut Pop)>,
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
    for (r, mut v, mut pop) in &mut roots {
        let was = *v;
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
        if was == Visibility::Hidden && *v != Visibility::Hidden {
            *pop = Pop::now(0.9);
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
        Option<(&mut BorderColor, &ChipLine, &mut BoxShadow)>,
        Option<&mut Pop>,
    )>,
) {
    for (t, mut f, mut n, mut color, mut bg, mut chip, pop) in &mut texts {
        if f.last != t.0 {
            f.last.clone_from(&t.0);
            f.age = 0.0;
            if let Some(mut pop) = pop
                && !t.0.trim().is_empty()
            {
                *pop = Pop::now(0.7);
            }
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
            if let Some((border, line, shadow)) = chip.as_mut() {
                **border = BorderColor::all(line.0.with_alpha(line.0.alpha() * a));
                for s in shadow.0.iter_mut() {
                    s.color = theme::SHADOW.with_alpha(theme::SHADOW.alpha() * a);
                }
            }
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
