//! "Make yourself comfortable": the settings notebook (Figma 3:2059; UI
//! polish, not a friend idea). Volumes as sliders, mic and HALT as square
//! checkboxes, video presets as tabs and every video option as a checkbox
//! card (changing one makes the preset "Custom"). Opened from the title
//! screen's Settings link or Tab; Esc or Done closes it. In game Esc opens
//! it too (#317), with a "Leave game" button (Esc no longer leaves).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::pad_nav::{PadFocusable, PadNudge, PadSlider};
use super::{at, body, head};
use crate::AppState;
use crate::controls::PadSettings;
use crate::graphics::{GraphicsQuality, Preset, VideoOption};
use crate::settings::Settings;

/// Column `i` (0 or 1) of the Controls page's bindings: the keyboard's, or
/// the gamepad's when `pad`.
pub fn help_column(i: usize, pad: bool) -> String {
    let list: &[(&str, &str)] = if pad {
        crate::controls::PAD_BINDINGS
    } else {
        crate::hud::KEY_BINDINGS
    };
    let half = list.len().div_ceil(2);
    let part = if i == 0 { &list[..half] } else { &list[half..] };
    part.iter()
        .map(|(k, v)| format!("{k}  ·  {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}
use crate::theme::indigo::{BORDER, CREAM, INDIGO, INK, LAVENDER, PAPER};
use crate::voice::VoiceMuted;

const B: f32 = 2.0;
const TRACK: Color = Color::srgb_u8(0xD8, 0xCC, 0xE7);
const OFF_FILL: Color = Color::srgb_u8(0xE5, 0xDC, 0xE6);
const OFF_LINE: Color = Color::srgb_u8(0xC5, 0xB7, 0xD3);
const SLIDER_W: f32 = 370.0;

/// The video options shown as checkboxes, in grid order.
pub const VIDEO_BOXES: [VideoOption; 9] = [
    VideoOption::Shadows,
    VideoOption::AntiAlias,
    VideoOption::AmbientOcclusion,
    VideoOption::Bloom,
    VideoOption::Grass,
    VideoOption::Resolution,
    VideoOption::DrawDistance,
    VideoOption::VSync,
    VideoOption::Fullscreen,
];

/// Is this option on?
pub fn checked(q: GraphicsQuality, o: VideoOption) -> bool {
    match o {
        VideoOption::Shadows => q.shadows,
        VideoOption::AmbientOcclusion => q.ssao,
        VideoOption::Bloom => q.bloom,
        VideoOption::AntiAlias => q.antialias,
        VideoOption::DrawDistance => q.far,
        VideoOption::Resolution => q.sharp,
        VideoOption::Grass => q.dense,
        VideoOption::VSync => q.vsync,
        VideoOption::Fullscreen => q.fullscreen,
        VideoOption::Preset => false,
    }
}

/// The checkbox's label.
pub fn box_label(q: GraphicsQuality, o: VideoOption) -> String {
    let name = match o {
        VideoOption::DrawDistance => "View distance",
        VideoOption::Resolution => "Sharp resolution",
        VideoOption::VSync => "V-sync",
        VideoOption::Bloom => "Bloom",
        o => o.name(),
    };
    if q.value(o).contains("reload") {
        format!("{name} (reload)")
    } else {
        name.to_string()
    }
}

/// Volume `0..=1` from where on the track you pressed, snapped to 5%.
pub fn volume_at(frac: f32) -> f32 {
    (frac.clamp(0.0, 1.0) * 20.0).round() / 20.0
}

/// The notebook; hidden until opened.
#[derive(Component)]
pub struct SettingsPanel;

/// Open or close the notebook.
pub fn toggle(panel: &mut Query<&mut Node, With<SettingsPanel>>) {
    for mut n in panel.iter_mut() {
        n.display = if n.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Audio,
    Video,
    Controls,
}

#[derive(Resource)]
struct OnPage(Page);

#[derive(Component, Clone, Copy)]
enum Part {
    PageTab(Page),
    PresetTab(Option<Preset>),
    VideoCard,
    ControlsCard,
    Pct(Volume),
    Fill(Volume),
    Handle(Volume),
    Box(Check),
    BoxLabel(VideoOption),
    /// One of the Controls page's two columns of bindings.
    Help(usize),
    /// "Esc close", or "B close" after a gamepad was used.
    CloseHint,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Volume {
    Master,
    Voice,
    /// The gamepad's stick deadzone and look speed (#361).
    Deadzone,
    Look,
}

/// Width of the two gamepad sliders, side by side on the Controls page.
const PAD_SLIDER_W: f32 = 300.0;

impl Volume {
    fn width(self) -> f32 {
        match self {
            Volume::Master | Volume::Voice => SLIDER_W,
            Volume::Deadzone | Volume::Look => PAD_SLIDER_W,
        }
    }

    /// Snap positions along the track.
    fn steps(self) -> f32 {
        match self {
            Volume::Master | Volume::Voice => 20.0,
            // 0.05 apart up to 0.4.
            Volume::Deadzone => 8.0,
            // 0.25 apart from 0.25 to 3 (1x is a step).
            Volume::Look => 11.0,
        }
    }

    /// How full the track is, `0..=1`.
    fn frac(self, s: &Settings, p: &PadSettings) -> f32 {
        match self {
            Volume::Master => s.master_volume,
            Volume::Voice => s.voice_volume,
            Volume::Deadzone => p.deadzone / PadSettings::MAX_DEADZONE,
            Volume::Look => {
                (p.look - PadSettings::LOOK_MIN) / (PadSettings::LOOK_MAX - PadSettings::LOOK_MIN)
            }
        }
    }

    /// Set from a track position, snapped to [`Self::steps`].
    fn set(self, s: &mut Settings, p: &mut PadSettings, frac: f32) {
        let n = self.steps();
        let f = (frac.clamp(0.0, 1.0) * n).round() / n;
        let lerp = |lo: f32, hi: f32| lo + (hi - lo) * f;
        let (field, want) = match self {
            Volume::Master => (&mut s.master_volume, f),
            Volume::Voice => (&mut s.voice_volume, f),
            Volume::Deadzone => (&mut p.deadzone, lerp(0.0, PadSettings::MAX_DEADZONE)),
            Volume::Look => (
                &mut p.look,
                lerp(PadSettings::LOOK_MIN, PadSettings::LOOK_MAX),
            ),
        };
        if (*field - want).abs() > 1e-4 {
            *field = want;
        }
    }

    /// The number beside the track.
    fn shown(self, s: &Settings, p: &PadSettings) -> String {
        match self {
            Volume::Look => format!("{:.2}x", p.look),
            Volume::Deadzone => format!("{}%", (p.deadzone * 100.0).round() as i32),
            v => format!("{}%", (v.frac(s, p) * 100.0).round() as i32),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Check {
    Mic,
    Halt,
    Video(VideoOption),
}

pub fn plugin(app: &mut App) {
    app.insert_resource(OnPage(Page::Video))
        .init_resource::<Freed>()
        .add_systems(OnEnter(AppState::Menu), spawn_in(AppState::Menu))
        .add_systems(OnEnter(AppState::InGame), spawn_in(AppState::InGame))
        .add_systems(Update, close_on_esc.run_if(in_state(AppState::Menu)))
        .add_systems(
            Update,
            (esc_in_game, free_mouse)
                .chain()
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(Update, refresh);
}

/// Whether opening the notebook freed a locked first-person mouse (so
/// closing it locks it again).
#[derive(Resource, Default)]
struct Freed(bool);

fn is_open(panel: &Query<&mut Node, With<SettingsPanel>>) -> bool {
    panel.iter().any(|n| n.display != Display::None)
}

/// Esc in game opens or closes the notebook (unless the journal or chapter
/// intro is up: that Esc is theirs).
fn esc_in_game(
    keys: Res<ButtonInput<KeyCode>>,
    overlay: Res<crate::ui::Overlay>,
    mut panel: Query<&mut Node, With<SettingsPanel>>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    if !is_open(&panel) && (overlay.journal || overlay.intro) {
        return;
    }
    toggle(&mut panel);
}

/// Free the mouse while the notebook is open, and tell clicks and Esc it's up.
fn free_mouse(
    panel: Query<&mut Node, With<SettingsPanel>>,
    mut overlay: ResMut<crate::ui::Overlay>,
    mut view: ResMut<crate::features::fps::FpsView>,
    mut freed: ResMut<Freed>,
) {
    let open = is_open(&panel);
    overlay.settings = open;
    if open && view.first_person && !view.released {
        view.released = true;
        freed.0 = true;
    } else if !open && freed.0 {
        freed.0 = false;
        view.released = false;
    }
}

/// Spawn the (hidden) notebook for `state`, gone when it ends.
fn spawn_in(state: AppState) -> impl Fn(Commands, Option<Res<AssetServer>>) {
    move |commands, assets| spawn(commands, assets, state)
}

fn close_on_esc(keys: Res<ButtonInput<KeyCode>>, mut panel: Query<&mut Node, With<SettingsPanel>>) {
    if keys.just_pressed(KeyCode::Escape) {
        for mut n in &mut panel {
            n.display = Display::None;
        }
    }
}

/// A bordered button `w`x`h` at Figma `x, y` in its (2 px bordered) parent.
fn button(w: f32, h: f32, radius: f32, fill: Color, x: f32, y: f32) -> impl Bundle {
    (
        Node {
            width: px(w),
            height: px(h),
            border: UiRect::all(px(B)),
            border_radius: BorderRadius::all(px(radius)),
            ..at(x, y, B)
        },
        BackgroundColor(fill),
        BorderColor::all(BORDER),
        PadFocusable,
    )
}

fn spawn(mut commands: Commands, assets: Option<Res<AssetServer>>, state: AppState) {
    let check: Handle<Image> = assets.map(|a| a.load("ui/check.png")).unwrap_or_default();
    let mut root = commands.spawn((
        SettingsPanel,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            display: Display::None,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(INK.with_alpha(0.63)),
        GlobalZIndex(30),
        DespawnOnExit(state),
    ));
    root.with_children(|scrim| {
        scrim
            .spawn((
                Node {
                    width: px(1200),
                    height: px(664),
                    border: UiRect::all(px(B)),
                    border_radius: BorderRadius::all(px(24)),
                    ..default()
                },
                BackgroundColor(LAVENDER),
                BorderColor::all(BORDER),
            ))
            .with_children(|p| {
                notebook(p, &check);
                if state == AppState::InGame {
                    p.spawn((
                        button(192.0, 42.0, 12.0, PAPER, 26.0, 594.0),
                        children![label("Leave game", 17.0, INK, 11.0, 8.0, B)],
                    ))
                    .observe(crate::menu::leave_game);
                }
            });
    });
}

/// A label-sized text helper.
fn label(
    s: impl Into<String>,
    size: f32,
    color: Color,
    x: f32,
    y: f32,
    border: f32,
) -> impl Bundle {
    (Text::new(s.into()), body(size, color), at(x, y, border))
}

fn notebook(p: &mut ChildSpawnerCommands, check: &Handle<Image>) {
    p.spawn((
        Text::new("Make yourself comfortable"),
        head(35.0, INK),
        at(26.0, 18.0, B),
    ));
    p.spawn((
        button(148.0, 42.0, 12.0, PAPER, 1018.0, 20.0),
        children![(
            Part::CloseHint,
            label("Esc   close", 17.0, INK, 11.0, 8.0, B)
        )],
    ))
    .observe(close);
    for (i, (page, name)) in [
        (Page::Audio, "Audio + voice"),
        (Page::Video, "Video"),
        (Page::Controls, "Controls"),
    ]
    .into_iter()
    .enumerate()
    {
        p.spawn((
            Part::PageTab(page),
            button(177.0, 42.0, 12.0, PAPER, 26.0 + 190.0 * i as f32, 76.0),
            children![label(name, 17.0, INK, 11.0, 8.0, B)],
        ))
        .observe(move |_: On<PointerClick>, mut on: ResMut<OnPage>| on.0 = page);
    }

    // Left: your ears & your name.
    p.spawn((
        Node {
            width: px(410),
            height: px(436),
            border_radius: BorderRadius::all(px(18)),
            ..at(26.0, 141.0, B)
        },
        BackgroundColor(PAPER),
    ))
    .with_children(|c| {
        c.spawn((
            Text::new("Your ears & your name"),
            head(26.0, INK),
            at(20.0, 14.0, 0.0),
        ));
        for (v, name, y) in [
            (Volume::Master, "Master volume", 69.0),
            (Volume::Voice, "Voice volume", 164.0),
        ] {
            c.spawn(label(name, 19.0, INK, 20.0, y, 0.0));
            c.spawn((Part::Pct(v), label("", 19.0, INDIGO, 313.0, y, 0.0)));
            slider(c, v, 20.0, y + 32.0);
        }
        check_row(c, check, Check::Mic, "Microphone (M)", 20.0, 268.0);
        check_row(
            c,
            check,
            Check::Halt,
            "Let HALT use my username",
            17.0,
            326.0,
        );
        c.spawn(label(
            "Voice is always connected.\nMute your mic, not your friends.",
            15.0,
            INK,
            20.0,
            378.0,
            0.0,
        ));
    });

    // Right: video, or the controls list.
    p.spawn((
        Part::VideoCard,
        Node {
            width: px(710),
            height: px(436),
            border: UiRect::all(px(B)),
            border_radius: BorderRadius::all(px(18)),
            ..at(460.0, 141.0, B)
        },
        BackgroundColor(PAPER),
        BorderColor::all(BORDER),
    ))
    .with_children(|c| {
        c.spawn((
            Text::new("Video / pick your potato"),
            head(26.0, INK),
            at(18.0, 12.0, B),
        ));
        let tabs = [
            (Some(Preset::Low), "Low"),
            (Some(Preset::Medium), "Medium"),
            (Some(Preset::High), "High"),
            (None, "Custom"),
        ];
        for (i, (preset, name)) in tabs.into_iter().enumerate() {
            c.spawn((
                Part::PresetTab(preset),
                button(158.0, 42.0, 12.0, PAPER, 18.0 + 170.0 * i as f32, 55.0),
                children![label(name, 17.0, INK, 11.0, 8.0, B)],
            ))
            .observe(move |_: On<PointerClick>, mut q: ResMut<GraphicsQuality>| {
                if let Some(p) = preset {
                    let want = p.apply(*q);
                    q.set_if_neq(want);
                }
            });
        }
        for (i, o) in VIDEO_BOXES.into_iter().enumerate() {
            let (x, y) = (18.0 + 228.0 * (i % 3) as f32, 118.0 + 90.0 * (i / 3) as f32);
            c.spawn((
                Node {
                    width: px(214),
                    height: px(76),
                    border_radius: BorderRadius::all(px(12)),
                    ..at(x, y, B)
                },
                BackgroundColor(PAPER),
                PadFocusable,
                children![
                    checkbox(check, Check::Video(o), 12.0, 24.0),
                    (
                        Part::BoxLabel(o),
                        Text::new(o.name()),
                        body(16.0, INK),
                        Node {
                            width: px(150),
                            ..at(52.0, 26.0, 0.0)
                        }
                    ),
                ],
            ))
            .observe(move |_: On<PointerClick>, mut q: ResMut<GraphicsQuality>| {
                if q.value(o) != "N/A" {
                    q.cycle(o);
                }
            });
        }
        c.spawn(label(
            "Preset changes apply immediately. Change a checkbox: Custom.",
            14.0,
            INK,
            18.0,
            402.0,
            B,
        ));
    });
    p.spawn((
        Part::ControlsCard,
        Node {
            width: px(710),
            height: px(436),
            border: UiRect::all(px(B)),
            border_radius: BorderRadius::all(px(18)),
            display: Display::None,
            ..at(460.0, 141.0, B)
        },
        BackgroundColor(PAPER),
        BorderColor::all(BORDER),
    ))
    .with_children(|c| {
        c.spawn((Text::new("Controls"), head(26.0, INK), at(18.0, 12.0, B)));
        // The gamepad's feel (#361), side by side.
        for (v, name, x) in [
            (Volume::Deadzone, "Stick deadzone", 18.0),
            (Volume::Look, "Stick look speed", 370.0),
        ] {
            c.spawn(label(name, 17.0, INK, x, 52.0, B));
            c.spawn((
                Part::Pct(v),
                label("", 17.0, INDIGO, x + PAD_SLIDER_W - 54.0, 52.0, B),
            ));
            slider(c, v, x - B, 82.0 - B);
        }
        // The bindings in two columns, so the list fits the card.
        for (i, x) in [18.0, 370.0].into_iter().enumerate() {
            c.spawn((
                Part::Help(i),
                Text::new(help_column(i, false)),
                body(13.0, INK),
                Node {
                    width: px(PAD_SLIDER_W + 30.0),
                    ..at(x, 122.0, B)
                },
            ));
        }
    });

    p.spawn((
        button(178.0, 42.0, 12.0, PAPER, 780.0, 594.0),
        children![label("Reset defaults", 17.0, INK, 11.0, 8.0, B)],
    ))
    .observe(
        |_: On<PointerClick>,
         mut s: ResMut<Settings>,
         mut pad: ResMut<PadSettings>,
         mut q: ResMut<GraphicsQuality>| {
            let keep = s.halt_real_name;
            *s = Settings {
                halt_real_name: keep,
                ..default()
            };
            pad.set_if_neq(PadSettings::DEFAULT);
            let want = GraphicsQuality::platform_default();
            q.set_if_neq(want);
        },
    );
    p.spawn((
        button(192.0, 42.0, 12.0, INDIGO, 978.0, 594.0),
        children![label("Done", 17.0, CREAM, 11.0, 8.0, B)],
    ))
    .observe(close);
}

fn close(_: On<PointerClick>, mut panel: Query<&mut Node, With<SettingsPanel>>) {
    for mut n in &mut panel {
        n.display = Display::None;
    }
}

fn slider(c: &mut ChildSpawnerCommands, v: Volume, x: f32, y: f32) {
    let track = c
        .spawn((
            Node {
                width: px(v.width()),
                height: px(26),
                ..at(x, y, 0.0)
            },
            PadFocusable,
            PadSlider,
            children![
                (
                    Node {
                        width: px(v.width()),
                        height: px(10),
                        border_radius: BorderRadius::all(px(5)),
                        ..at(0.0, 8.0, 0.0)
                    },
                    BackgroundColor(TRACK),
                ),
                (
                    Part::Fill(v),
                    Node {
                        width: px(0),
                        height: px(10),
                        border_radius: BorderRadius::all(px(5)),
                        ..at(0.0, 8.0, 0.0)
                    },
                    BackgroundColor(INDIGO),
                ),
                (
                    Part::Handle(v),
                    Node {
                        width: px(26),
                        height: px(26),
                        border: UiRect::all(px(B)),
                        border_radius: BorderRadius::all(px(13)),
                        ..at(0.0, 0.0, 0.0)
                    },
                    BackgroundColor(PAPER),
                    BorderColor::all(INDIGO),
                    Pickable::IGNORE,
                ),
            ],
        ))
        .id();
    c.commands_mut()
        .entity(track)
        .observe(
            move |_: On<PointerPress>,
                  nodes: SliderNodes,
                  mut s: ResMut<Settings>,
                  mut p: ResMut<PadSettings>| {
                set_from_cursor(track, &nodes, v, &mut s, &mut p);
            },
        )
        .observe(
            move |_: On<PointerDrag>,
                  nodes: SliderNodes,
                  mut s: ResMut<Settings>,
                  mut p: ResMut<PadSettings>| {
                set_from_cursor(track, &nodes, v, &mut s, &mut p);
            },
        )
        .observe(
            move |n: On<PadNudge>, mut s: ResMut<Settings>, mut p: ResMut<PadSettings>| {
                let f = v.frac(&s, &p) + n.steps as f32 / v.steps();
                v.set(&mut s, &mut p, f);
            },
        );
}

type SliderNodes<'w, 's> = (
    Query<'w, 's, (&'static ComputedNode, &'static UiGlobalTransform)>,
    Query<'w, 's, &'static Window, With<PrimaryWindow>>,
);

fn set_from_cursor(
    slider: Entity,
    (nodes, window): &SliderNodes,
    v: Volume,
    s: &mut Settings,
    p: &mut PadSettings,
) {
    let (Ok((node, tf)), Ok(window)) = (nodes.get(slider), window.single()) else {
        return;
    };
    let Some(cursor) = window.physical_cursor_position() else {
        return;
    };
    let w = node.size().x;
    if w <= 0.0 {
        return;
    }
    let left = tf.translation.x - w / 2.0;
    v.set(s, p, (cursor.x - left) / w);
}

fn check_row(
    c: &mut ChildSpawnerCommands,
    check: &Handle<Image>,
    what: Check,
    name: &str,
    size: f32,
    y: f32,
) {
    c.spawn((
        Node {
            width: px(370),
            height: px(36),
            ..at(20.0, y - 4.0, 0.0)
        },
        PadFocusable,
        children![
            label(name, size, INK, 0.0, 4.0, 0.0),
            checkbox(check, what, 330.0, 0.0),
        ],
    ))
    .observe(
        move |_: On<PointerClick>, mut s: ResMut<Settings>, mut muted: ResMut<VoiceMuted>| {
            match what {
                Check::Mic => muted.0 = !muted.0,
                Check::Halt => s.halt_real_name = !s.halt_real_name,
                Check::Video(_) => {}
            }
        },
    );
}

/// A 28 px square checkbox; [`refresh`] fills and ticks it.
fn checkbox(check: &Handle<Image>, what: Check, x: f32, y: f32) -> impl Bundle {
    (
        Part::Box(what),
        Node {
            width: px(28),
            height: px(28),
            border: UiRect::all(px(B)),
            border_radius: BorderRadius::all(px(5)),
            ..at(x, y, 0.0)
        },
        BackgroundColor(PAPER),
        BorderColor::all(BORDER),
        Pickable::IGNORE,
        children![(
            Node {
                width: px(18),
                height: px(18),
                ..at(3.0, 3.0, B)
            },
            ImageNode::new(check.clone()).with_color(CREAM),
            Pickable::IGNORE,
        )],
    )
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn refresh(
    on: Res<OnPage>,
    s: Res<Settings>,
    pad: Res<PadSettings>,
    last: Res<crate::controls::PadLast>,
    muted: Res<VoiceMuted>,
    q: Res<GraphicsQuality>,
    mut nodes: Query<&mut Node>,
    mut parts: Query<(
        Entity,
        &Part,
        Option<&Children>,
        Option<&mut BackgroundColor>,
        Option<&mut BorderColor>,
        Option<&mut Text>,
    )>,
    mut texts: Query<&mut TextColor>,
    mut vis: Query<&mut Visibility>,
) {
    let preset = q.preset();
    for (e, part, kids, bg, line, text) in &mut parts {
        let tab =
            |on: bool, bg: Option<Mut<BackgroundColor>>, texts: &mut Query<&mut TextColor>| {
                if let Some(mut bg) = bg {
                    bg.set_if_neq(BackgroundColor(if on { INDIGO } else { PAPER }));
                }
                for k in kids.into_iter().flatten() {
                    if let Ok(mut c) = texts.get_mut(*k) {
                        c.set_if_neq(TextColor(if on { CREAM } else { INK }));
                    }
                }
            };
        match *part {
            Part::PageTab(p) => tab(p == on.0, bg, &mut texts),
            Part::PresetTab(p) => tab(p == preset, bg, &mut texts),
            Part::VideoCard | Part::ControlsCard => {
                let show = matches!(part, Part::ControlsCard) == (on.0 == Page::Controls);
                if let Ok(mut n) = nodes.get_mut(e) {
                    let d = if show { Display::Flex } else { Display::None };
                    if n.display != d {
                        n.display = d;
                    }
                }
            }
            Part::Pct(v) => {
                let want = v.shown(&s, &pad);
                if let Some(mut t) = text
                    && t.0 != want
                {
                    t.0 = want;
                }
            }
            Part::Help(_) | Part::CloseHint => {
                let want = match *part {
                    Part::Help(i) => help_column(i, last.0),
                    _ if last.0 => "B   close".to_string(),
                    _ => "Esc   close".to_string(),
                };
                if let Some(mut t) = text
                    && t.0 != want
                {
                    t.0 = want;
                }
            }
            Part::Fill(v) | Part::Handle(v) => {
                let (f, w) = (v.frac(&s, &pad), v.width());
                if let Ok(mut n) = nodes.get_mut(e) {
                    let want = if matches!(part, Part::Fill(_)) {
                        (px(w * f), n.left)
                    } else {
                        (n.width, px(w * f - 13.0))
                    };
                    if (n.width, n.left) != want {
                        (n.width, n.left) = want;
                    }
                }
            }
            Part::Box(what) => {
                let (on, disabled) = match what {
                    Check::Mic => (!muted.0, false),
                    Check::Halt => (s.halt_real_name, false),
                    Check::Video(o) => (checked(*q, o), q.value(o) == "N/A"),
                };
                let (fill, edge) = match (disabled, on) {
                    (true, _) => (OFF_FILL, OFF_LINE),
                    (false, true) => (INDIGO, BORDER),
                    (false, false) => (PAPER, BORDER),
                };
                if let Some(mut bg) = bg {
                    bg.set_if_neq(BackgroundColor(fill));
                }
                if let Some(mut l) = line {
                    l.set_if_neq(BorderColor::all(edge));
                }
                for k in kids.into_iter().flatten() {
                    if let Ok(mut v) = vis.get_mut(*k) {
                        v.set_if_neq(if on && !disabled {
                            Visibility::Inherited
                        } else {
                            Visibility::Hidden
                        });
                    }
                }
            }
            Part::BoxLabel(o) => {
                let want = box_label(*q, o);
                if let Some(mut t) = text
                    && t.0 != want
                {
                    t.0 = want;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sliders_snap_and_clamp() {
        assert_eq!(volume_at(0.79), 0.8);
        assert_eq!(volume_at(-1.0), 0.0);
        assert_eq!(volume_at(1.7), 1.0);
    }

    #[test]
    fn every_box_is_a_real_option() {
        assert!(!VIDEO_BOXES.contains(&VideoOption::Preset));
        assert_eq!(VIDEO_BOXES.len(), 9);
    }

    #[test]
    fn pad_sliders_step_through_their_defaults() {
        let mut s = Settings::default();
        let mut p = PadSettings::DEFAULT;
        for v in [Volume::Deadzone, Volume::Look, Volume::Master] {
            // One nudge right then one left comes back to where it was.
            let before = (v.frac(&s, &p), p);
            v.set(&mut s, &mut p, before.0 + 1.0 / v.steps());
            assert!(v.frac(&s, &p) > before.0);
            let back = v.frac(&s, &p) - 1.0 / v.steps();
            v.set(&mut s, &mut p, back);
            assert!((v.frac(&s, &p) - before.0).abs() < 1e-4, "{v:?}");
            assert_eq!(p.clamped(), p);
        }
        Volume::Look.set(&mut s, &mut p, 2.0);
        assert_eq!(p.look, PadSettings::LOOK_MAX);
        assert_eq!(Volume::Deadzone.shown(&s, &PadSettings::DEFAULT), "15%");
        assert_eq!(Volume::Look.shown(&s, &PadSettings::DEFAULT), "1.00x");
        // Every binding lands in one of the two columns.
        let lines = |pad| help_column(0, pad).lines().count() + help_column(1, pad).lines().count();
        assert_eq!(lines(false), crate::hud::KEY_BINDINGS.len());
        assert_eq!(lines(true), crate::controls::PAD_BINDINGS.len());
    }
}
