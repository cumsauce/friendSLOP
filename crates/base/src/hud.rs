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
use crate::theme::indigo as idg;
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
        // UI direction E: the slots sit around the new widgets (quest card
        // top-left, team strip top-right, status stack bottom-right).
        let m = px(idg::EDGE);
        let mut n = Node {
            position_type: PositionType::Absolute,
            flex_direction: FlexDirection::Column,
            row_gap: px(theme::S1 + 2.0),
            max_width: percent(30),
            ..default()
        };
        match self {
            // Under the quest card.
            HudSlot::TopLeft => (n.top, n.left) = (px(172), m),
            // Under the team strip.
            HudSlot::TopRight => {
                (n.top, n.right) = (px(82), m);
                n.align_items = AlignItems::FlexEnd;
            }
            // Above the journal hint (which is this column's last child).
            HudSlot::BottomLeft => (n.bottom, n.left) = (px(30), m),
            // Above the gas / mic / cat stack (its last child), 5 px up.
            HudSlot::BottomRight => {
                (n.bottom, n.right) = (px(5), m);
                n.align_items = AlignItems::FlexEnd;
                n.row_gap = px(8);
                // A fixed width, so wrapped lines are measured at the width
                // they're drawn at (otherwise the stack runs off the bottom).
                n.width = px(330);
            }
            // The danger lane comes first, then these lines under it.
            HudSlot::TopCenter => {
                n.top = px(100);
                (n.left, n.right) = (percent(30), percent(30));
                n.max_width = Val::Auto;
                n.align_items = AlignItems::Center;
                n.row_gap = px(8);
            }
            // Its lines go to the event feed (`ui::feed`) instead.
            HudSlot::Toasts => {
                n.display = Display::None;
            }
            HudSlot::Help => {
                n.top = percent(10);
                (n.left, n.right) = (percent(28), percent(28));
                n.max_width = Val::Auto;
                n.padding = UiRect::all(px(theme::S4 + 4.0));
                n.row_gap = px(theme::S2);
                n.border = UiRect::all(px(2));
                n.border_radius = BorderRadius::all(px(24));
            }
            HudSlot::Subtitle => {
                n.bottom = px(27);
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
                n.border_radius = BorderRadius::all(px(24));
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

/// Longest a shortened line gets before it's cut short (full text: Tab).
const OWN_LINE_CHARS: usize = 44;

// HUD lines sit on solid indigo-ink chips (UI direction E), so text is cream
// with periwinkle and butter accents.
pub const TEXT: Color = idg::CREAM;
pub const DIM: Color = idg::PERIWINKLE;
pub const WARN: Color = idg::BUTTER;
pub const ACCENT: Color = idg::PERIWINKLE;
/// Light text for labels floating over the world (no chip behind them).
pub const LABEL: Color = theme::CREAM;
const PANEL: Color = idg::INK;
const BIG_PANEL: Color = idg::INK;

/// A feature's text colour, made readable on an ink chip: the old warning
/// reds become butter, anything dark becomes cream, light colours stay.
pub fn on_ink(c: Color) -> Color {
    let s = c.to_srgba();
    let warm_warning = s.red > 0.7 && s.green < 0.45 && s.blue < 0.65;
    if warm_warning {
        return idg::BUTTER.with_alpha(s.alpha);
    }
    let luma = 0.2126 * s.red + 0.7152 * s.green + 0.0722 * s.blue;
    if luma < 0.55 {
        idg::CREAM.with_alpha(s.alpha)
    } else {
        c
    }
}
/// How long a fading line stays fully visible, then how long it takes to go.
const FADE_HOLD: f32 = 5.0;
const FADE_OUT: f32 = 1.2;

pub fn normal() -> (TextFont, TextColor) {
    (TextFont::from_font_size(15.0), TextColor(TEXT))
}
pub fn small() -> (TextFont, TextColor) {
    (TextFont::from_font_size(14.0), TextColor(DIM))
}
pub fn warning() -> (TextFont, TextColor) {
    (TextFont::from_font_size(15.0), TextColor(WARN))
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

/// The card behind one line of slotted HUD text: solid ink, a quiet
/// outline (subtitles: none), rounded corners. Banners get more air.
fn chip(slot: HudSlot) -> impl Bundle {
    let (pad, radius, line) = match slot {
        HudSlot::CenterBanner => (UiRect::axes(px(26), px(16)), 18.0, idg::BORDER),
        HudSlot::Subtitle => (UiRect::axes(px(8), px(8)), 12.0, idg::INK),
        _ => (UiRect::axes(px(8), px(6)), 12.0, idg::BORDER),
    };
    let border = 2.0;
    (
        Node {
            padding: pad,
            border: UiRect::all(px(border)),
            border_radius: BorderRadius::all(px(radius)),
            ..default()
        },
        BackgroundColor(PANEL),
        BorderColor::all(line),
        theme::shadow(2.0, 6.0),
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
    /// Shows `text`, with any `{action}` turned into its button ([`crate::input::fill`]).
    pub fn show(&mut self, text: impl Into<String>) {
        self.text = crate::input::fill(&text.into());
        self.secs_left = Self::SECS;
    }
}

/// Every binding, for the H help overlay and the journal: generated from
/// `assets/data/base/bindings.ron` for the device used last.
pub fn help_text() -> String {
    crate::input::help_text()
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
/// Where `ui::danger` puts the danger lane (first in the top-centre column).
pub use crate::ui::DangerMount;
/// Where `ui::status` puts the gas / mic / cat stack (bottom-right column).
pub use crate::ui::StatusMount;
/// The bottom-left "Tab / J  journal" hint (last in the bottom-left column).
#[derive(Component)]
struct JournalHint;
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
            .init_resource::<ShowAll>()
            .init_resource::<HudClock>()
            .init_resource::<LocalAt>()
            .add_systems(OnEnter(AppState::InGame), spawn_slots)
            .add_systems(
                OnExit(AppState::InGame),
                |mut t: ResMut<Toast>, mut c: ResMut<HudClock>| {
                    *t = Toast::default();
                    c.0 = 0.0;
                },
            )
            .add_systems(
                Update,
                (
                    adopt_slotted,
                    readable_on_ink,
                    tick_toast,
                    toggle_overlays,
                    show_fps,
                    hide_empty,
                    fade_lines,
                    track_local,
                    calm_lines,
                    journal_hint,
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
            BorderColor::all(idg::BORDER),
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
                    header(c, "controls", theme::PINK);
                    c.spawn((
                        Text::new(""),
                        crate::input::Hint("{help} closes this".into()),
                        small(),
                    ));
                    c.spawn((
                        Text::new(help_text()),
                        crate::input::Hint("{@help}".into()),
                        small(),
                    ));
                    header(c, "what's going on", theme::LILAC);
                });
            }
            HudSlot::TopCenter => {
                e.with_child((DangerMount, Node::default()));
            }
            HudSlot::BottomRight => {
                e.with_child((StatusMount, Node::default()));
            }
            HudSlot::BottomLeft => {
                let b = 2.0;
                e.with_child((
                    JournalHint,
                    Node {
                        width: px(188),
                        height: px(40),
                        padding: UiRect::axes(px(12.0 - b), px(8.0 - b)),
                        border: UiRect::all(px(b)),
                        border_radius: BorderRadius::all(px(12)),
                        ..default()
                    },
                    BackgroundColor(idg::INK),
                    BorderColor::all(idg::BORDER),
                    children![(
                        Text::new(""),
                        crate::input::Hint("{journal} / {journal_keep}   journal".into()),
                        crate::ui::body(17.0, idg::CREAM)
                    )],
                ));
            }
            _ => {}
        }
    }
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
            tf.weight = FontWeight(700);
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
        // (Bevy's default); give them the readable cream for the ink chips.
        match color {
            None => {
                ec.insert(TextColor(if s.one_panel() { DIM } else { TEXT }));
            }
            Some(c) if c.0 == Color::WHITE => {
                ec.insert(TextColor(if s.one_panel() { DIM } else { TEXT }));
            }
            Some(c) => {
                ec.insert(TextColor(on_ink(c.0)));
            }
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
        // The bottom columns end in a mount (status stack, journal hint):
        // lines go above it.
        if first || matches!(s, HudSlot::BottomLeft | HudSlot::BottomRight) {
            commands.entity(root).insert_children(0, &[e]);
        } else {
            ec.insert(ChildOf(root));
        }
    }
}

/// Keep slotted text readable when a feature recolours it later.
fn readable_on_ink(mut texts: Query<&mut TextColor, (With<InSlot>, Changed<TextColor>)>) {
    for mut c in &mut texts {
        let want = on_ink(c.0);
        if want != c.0 {
            c.0 = want;
        }
    }
}

/// Count the toast down (the event feed shows it; see `ui::feed`).
fn tick_toast(time: Res<Time>, mut toast: ResMut<Toast>) {
    if toast.secs_left > 0.0 {
        toast.secs_left -= time.delta_secs();
        if toast.secs_left <= 0.0 {
            toast.text.clear();
        }
    }
}

/// The "Tab / J journal" hint shows for the first part of a session only.
fn journal_hint(
    clock: Res<HudClock>,
    all: Res<ShowAll>,
    mut hint: Query<&mut Node, With<JournalHint>>,
) {
    let up = all.0 || hints_up(clock.0, &tuning());
    let d = if up { Display::Flex } else { Display::None };
    for mut n in &mut hint {
        if n.display != d {
            n.display = d;
        }
    }
}

/// F3 flips the FPS counter. The Scoreboard and Help panels stay hidden:
/// the journal (`ui::journal`) shows their lines now.
fn toggle_overlays(
    keys: Res<ButtonInput<KeyCode>>,
    mut fps: Query<&mut Visibility, (With<FpsText>, Without<SlotRoot>)>,
) {
    if keys.just_pressed(KeyCode::F3) {
        for mut v in &mut fps {
            *v = if *v == Visibility::Hidden {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
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
fn hide_empty(
    mut texts: Query<
        (&Text, &mut Node),
        (With<InSlot>, Without<Fade>, Without<Calm>, Changed<Text>),
    >,
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
    for (t, mut f, mut n, mut color, mut bg, chip, pop) in &mut texts {
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
        tint(a, &mut color, &mut bg, chip);
    }
}

// ---- Quiet HUD (declutter): one visibility rule for every non-urgent element.
//
// An element shows for a few seconds when its value changes, stays up while
// it's relevant (you're near its place, or it's urgent), and otherwise hides;
// the journal's "Right now" list still has every one of them.

/// When quiet HUD elements show (`data/base/hud.ron`).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Seconds an element stays fully visible after its value changes.
    pub show_secs: f32,
    /// Seconds it then takes to fade away.
    pub fade_secs: f32,
    /// Metres: within this of an element's place it stays up.
    pub near_metres: f32,
    /// A countdown at or under this many seconds counts as urgent.
    pub urgent_secs: f32,
    /// A meter at or over this percent counts as urgent.
    pub urgent_percent: f32,
    /// The fart meter shows once it's filled past this percent (and when ready).
    pub gas_show_percent: f32,
    /// Seconds from joining that the key hints stay up.
    pub hint_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/base/hud.ron");

/// The current HUD tuning.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// How visible a quiet element is: fully while `near` or `urgent`, else for
/// `show_secs` after its value last changed (`age`), then fading out.
pub fn calm_alpha(age: f32, near: bool, urgent: bool, t: &Tuning) -> f32 {
    if near || urgent {
        return 1.0;
    }
    if t.fade_secs <= 0.0 {
        return if age <= t.show_secs { 1.0 } else { 0.0 };
    }
    (1.0 - (age - t.show_secs) / t.fade_secs).clamp(0.0, 1.0)
}

/// Is `me` within `radius` metres of `at` (on the ground plane)?
pub fn is_near(me: Option<Vec3>, at: Option<Vec3>, radius: f32) -> bool {
    match (me, at) {
        (Some(m), Some(a)) => m.xz().distance(a.xz()) <= radius,
        _ => false,
    }
}

/// Are the key hints still up, `secs` into the session?
pub fn hints_up(secs: f32, t: &Tuning) -> bool {
    secs < t.hint_secs
}

/// Hash anything into a change key for [`Recent`] / [`Calm::key`].
pub fn key_of(v: impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

/// How long ago a value last changed. Widgets that aren't slotted lines keep
/// one of these and feed it to [`calm_alpha`].
#[derive(Component, Clone, Debug, Default)]
pub struct Recent {
    last: Option<u64>,
    age: f32,
}

impl Recent {
    /// Note the value's key this frame; returns seconds since it changed.
    /// The first value seen counts as a change (it shows on arrival).
    pub fn tick(&mut self, key: u64, dt: f32) -> f32 {
        if self.last != Some(key) {
            self.last = Some(key);
            self.age = 0.0;
        } else {
            self.age += dt;
        }
        self.age
    }
}

/// Put on a slotted HUD line to make it quiet: it shows when its value
/// changes, while you're near `near` or while `urgent`, and otherwise lives
/// in the journal under `label`.
#[derive(Component, Clone, Debug, Default)]
pub struct Calm {
    /// What the journal calls it ("Rent", "Skills").
    pub label: &'static str,
    /// The place it's about.
    pub near: Option<Vec3>,
    /// Must be seen right now (a short countdown, a full meter).
    pub urgent: bool,
    /// What counts as a change (default: the text). Set it to leave out a
    /// ticking countdown.
    pub key: Option<u64>,
    recent: Recent,
}

impl Calm {
    pub fn new(label: &'static str) -> Self {
        Self { label, ..default() }
    }
    pub fn near(mut self, at: Vec3) -> Self {
        self.near = Some(at);
        self
    }
}

/// Forces every quiet element up (screenshot checks).
#[derive(Resource, Default, Debug)]
pub struct ShowAll(pub bool);

/// Seconds since this session's HUD appeared (for the key hints).
#[derive(Resource, Default, Debug)]
pub struct HudClock(pub f32);

/// Where the local player stands, if known.
#[derive(Resource, Default, Debug)]
pub struct LocalAt(pub Option<Vec3>);

fn track_local(
    time: Res<Time>,
    me: Option<Res<crate::player::LocalPlayer>>,
    players: Query<(&crate::player::Player, &GlobalTransform)>,
    mut at: ResMut<LocalAt>,
    mut clock: ResMut<HudClock>,
) {
    clock.0 += time.delta_secs();
    let want = me.and_then(|me| {
        players
            .iter()
            .find(|(p, _)| p.id == me.0)
            .map(|(_, t)| t.translation())
    });
    if at.0 != want {
        at.0 = want;
    }
}

/// Apply the quiet rule to every [`Calm`] line.
#[allow(clippy::type_complexity)]
fn calm_lines(
    time: Res<Time>,
    at: Res<LocalAt>,
    all: Res<ShowAll>,
    mut texts: Query<(
        &Text,
        &mut Calm,
        &mut Node,
        &mut TextColor,
        &mut BackgroundColor,
        Option<(&mut BorderColor, &ChipLine, &mut BoxShadow)>,
    )>,
) {
    let t = tuning();
    for (text, mut calm, mut n, mut color, mut bg, chip) in &mut texts {
        let key = calm.key.unwrap_or_else(|| key_of(&text.0));
        let age = calm.recent.tick(key, time.delta_secs());
        let near = is_near(at.0, calm.near, t.near_metres);
        let a = if all.0 {
            1.0
        } else {
            calm_alpha(age, near, calm.urgent, &t)
        };
        let d = if text.0.trim().is_empty() || a <= 0.0 {
            Display::None
        } else {
            Display::Flex
        };
        if n.display != d {
            n.display = d;
        }
        tint(a, &mut color, &mut bg, chip);
    }
}

/// Fade a chip line's text, panel, outline and shadow to `a`.
fn tint(
    a: f32,
    color: &mut TextColor,
    bg: &mut BackgroundColor,
    chip: Option<(Mut<BorderColor>, &ChipLine, Mut<BoxShadow>)>,
) {
    if (color.0.alpha() - a).abs() <= 0.01 {
        return;
    }
    color.0.set_alpha(a);
    bg.0 = PANEL.with_alpha(PANEL.alpha() * a);
    if let Some((mut border, line, mut shadow)) = chip {
        *border = BorderColor::all(line.0.with_alpha(line.0.alpha() * a));
        for s in shadow.0.iter_mut() {
            s.color = theme::SHADOW.with_alpha(theme::SHADOW.alpha() * a);
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

    fn t() -> Tuning {
        Tuning {
            show_secs: 4.0,
            fade_secs: 1.0,
            near_metres: 10.0,
            urgent_secs: 20.0,
            urgent_percent: 75.0,
            gas_show_percent: 50.0,
            hint_secs: 60.0,
        }
    }

    #[test]
    fn quiet_lines_show_on_change_then_fade() {
        let t = t();
        assert_eq!(calm_alpha(0.0, false, false, &t), 1.0);
        assert_eq!(calm_alpha(3.9, false, false, &t), 1.0);
        let mid = calm_alpha(4.5, false, false, &t);
        assert!(mid > 0.0 && mid < 1.0);
        assert_eq!(calm_alpha(5.1, false, false, &t), 0.0);
    }

    #[test]
    fn quiet_lines_stay_while_near_or_urgent() {
        let t = t();
        assert_eq!(calm_alpha(99.0, true, false, &t), 1.0);
        assert_eq!(calm_alpha(99.0, false, true, &t), 1.0);
        let here = Some(Vec3::new(1.0, 5.0, 1.0));
        assert!(is_near(here, Some(Vec3::new(5.0, 0.0, 5.0)), 10.0));
        assert!(!is_near(here, Some(Vec3::new(50.0, 0.0, 5.0)), 10.0));
        assert!(!is_near(None, Some(Vec3::ZERO), 10.0));
        assert!(!is_near(here, None, 10.0));
    }

    #[test]
    fn recent_restarts_on_a_new_value_only() {
        let mut r = Recent::default();
        assert_eq!(r.tick(1, 1.0), 0.0);
        assert_eq!(r.tick(1, 1.0), 1.0);
        assert_eq!(r.tick(1, 1.0), 2.0);
        assert_eq!(r.tick(2, 1.0), 0.0);
        assert_eq!(key_of(("rent", 3)), key_of(("rent", 3)));
    }

    #[test]
    fn key_hints_go_after_a_while() {
        assert!(hints_up(10.0, &t()));
        assert!(!hints_up(61.0, &t()));
    }

    #[test]
    fn help_lists_every_binding() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            crate::input::BINDINGS.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(crate::input::BINDINGS.set_bytes(&bytes));
        let h = crate::input::help_rows(crate::input::InputDevice::KeyboardMouse)
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join("\n");
        for k in [
            "M", "F", "G", "R", "1", "Y", "N", "WASD", "V", "Tab", "H", "F3", "Esc",
        ] {
            assert!(h.contains(k), "{k}");
        }
    }
}
