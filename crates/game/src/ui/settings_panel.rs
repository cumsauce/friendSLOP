//! "Make yourself comfortable": the settings notebook (Figma 3:2059; UI
//! polish, not a friend idea). Volumes as sliders, mic and HALT as square
//! checkboxes, video presets as tabs and every video option as a checkbox
//! card (changing one makes the preset "Custom"). Opened from the title
//! screen's Settings link or Tab; Esc or Done closes it. In game Esc opens
//! it too (#317), with a "Leave game" button (Esc no longer leaves).

use bevy::math::Rot2;
use bevy::prelude::*;
use bevy::ui_widgets::{
    Activate, Button, ButtonPlugin, SetSliderValue, Slider, SliderPlugin, SliderRange, SliderStep,
    SliderThumb, SliderValue, SliderValueChange, TrackClick, ValueChange,
};
use bevy::window::PrimaryWindow;

use super::pad_nav::{PadFocusable, PadNudge, PadSlider};
use super::{at, body, head};
use crate::AppState;
use crate::controls::PadSettings;

use crate::graphics::{GraphicsQuality, Preset, VideoOption};
use crate::settings::Settings;

/// Column `i` (0 or 1) of the Controls page's bindings for `device` (from
/// `bindings.ron`, so they name that device's buttons).
pub fn help_column(i: usize, device: crate::input::InputDevice) -> String {
    let list = crate::controls::bindings_list(device);
    let half = list.len().div_ceil(2);
    let part = if i == 0 { &list[..half] } else { &list[half..] };
    part.iter()
        .map(|(k, v)| format!("{k}  ·  {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}
use crate::theme::kit as t;

const B: f32 = 2.0;
/// Text on soot, rows on a panel, the one slime-green accent, ink on slime, soot borders.
const TEXT: Color = t::TEXT;
const RAISED: Color = t::SOOT_700;
const PANEL: Color = t::SOOT_800;
const ACCENT: Color = t::SLOP_400;
const ON_ACCENT: Color = t::INK;
const LINE: Color = t::SOOT_500;
const TRACK: Color = t::SOOT_900;
const OFF_FILL: Color = t::SOOT_800;
const OFF_LINE: Color = t::SOOT_700;
const SLIDER_W: f32 = 370.0;
/// Where each page's one card stands: centred in the 1200 px notebook.
const CARD_X: f32 = 245.0;
const AUDIO_X: f32 = 395.0;

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
        VideoOption::Preset | VideoOption::WindowSize => false,
    }
}

/// The checkbox's label.
pub fn box_label(q: GraphicsQuality, o: VideoOption) -> String {
    let name = match o {
        VideoOption::DrawDistance => "View distance",
        VideoOption::Resolution => "Sharp resolution",
        VideoOption::VSync => "V-sync",
        VideoOption::Bloom => "Bloom",
        VideoOption::Shadows if q.value(o) == "Short" => "Short shadows",
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
pub fn toggle_any<F: bevy::ecs::query::QueryFilter>(panel: &mut Query<&mut Node, F>) {
    for mut n in panel.iter_mut() {
        n.display = if n.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
    }
}

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

impl Page {
    /// Does this page show `card`? Each page has exactly one card.
    fn shows(self, card: Part) -> bool {
        matches!(
            (self, card),
            (Page::Audio, Part::AudioCard)
                | (Page::Video, Part::VideoCard)
                | (Page::Controls, Part::ControlsCard)
        )
    }
}

#[derive(Resource)]
struct OnPage(Page);

#[derive(Component, Clone, Copy)]
enum Part {
    PageTab(Page),
    PresetTab(Option<Preset>),
    /// The audio page: volumes, mic and the HALT switch.
    AudioCard,
    VideoCard,
    ControlsCard,
    Pct(Volume),
    Fill(Volume),
    Handle(Volume),
    Box(Check),
    BoxLabel(VideoOption),
    /// A dropdown's current value, its list, one row of the list, and the
    /// invisible layer that closes the list when you click anywhere else.
    DropValue(VideoOption),
    DropList(VideoOption),
    DropItem(VideoOption, usize),
    DropBackdrop,
    /// The render scale slider: its value text, the slider, its fill and thumb.
    ScaleValue,
    ScaleSlider,
    ScaleFill,
    ScaleThumb,
    /// One of the Controls page's two columns of bindings.
    Help(usize),
    /// "Esc close", or "B close" after a gamepad was used.
    CloseHint,
    /// The button-names choice ("Button names: Auto").
    Labels,
}

/// Pure: the next button-names choice (Auto, then each pad family).
pub fn next_labels(now: Option<crate::input::PadFamily>) -> Option<crate::input::PadFamily> {
    use crate::input::PadFamily::*;
    match now {
        None => Some(Xbox),
        Some(Xbox) => Some(PlayStation),
        Some(PlayStation) => Some(Nintendo),
        Some(Nintendo) => Some(Steam),
        Some(Steam) => None,
    }
}

fn labels_text(l: Option<crate::input::PadFamily>) -> String {
    use crate::input::PadFamily::*;
    let name = match l {
        None => "Auto",
        Some(Xbox) => "Xbox",
        Some(PlayStation) => "PlayStation",
        Some(Nintendo) => "Nintendo",
        Some(Steam) => "Steam",
    };
    format!("Button names: {name}")
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
    PushToTalk,
    Halt,
    Hrtf,
    Video(VideoOption),
}

pub fn plugin(app: &mut App) {
    // Bevy's headless buttons and slider (the video dropdowns and render
    // scale), and nothing else of its widgets: the rest of the UI hears
    // clicks itself.
    if !app.is_plugin_added::<ButtonPlugin>() {
        app.add_plugins(ButtonPlugin);
    }
    if !app.is_plugin_added::<SliderPlugin>() {
        app.add_plugins(SliderPlugin);
    }
    app.init_resource::<OpenDropdown>()
        .add_systems(
            PreUpdate,
            close_dropdown_on_esc.after(bevy::input::InputSystems),
        )
        .add_systems(Update, forget_dropdown)
        .insert_resource(OnPage(Page::Video))
        .add_systems(OnEnter(AppState::Menu), spawn_in(AppState::Menu))
        .add_systems(OnEnter(AppState::InGame), spawn_in(AppState::InGame))
        .add_systems(
            Update,
            super::pause::free_mouse.run_if(in_state(AppState::InGame)),
        )
        .add_systems(Update, refresh);
}

/// Spawn the (hidden) notebook for `state`, gone when it ends.
fn spawn_in(state: AppState) -> impl Fn(Commands, Option<Res<AssetServer>>) {
    move |commands, assets| spawn(commands, assets, state)
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
        BorderColor::all(LINE),
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
        BackgroundColor(t::SOOT_900.with_alpha(0.8)),
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
                BackgroundColor(PANEL),
                BorderColor::all(LINE),
            ))
            .with_children(|p| {
                notebook(p, &check);
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
        head(35.0, TEXT),
        at(26.0, 18.0, B),
    ));
    p.spawn((
        button(148.0, 42.0, 12.0, RAISED, 1018.0, 20.0),
        children![(
            Part::CloseHint,
            label("Esc   close", 17.0, TEXT, 11.0, 8.0, B)
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
            button(177.0, 42.0, 12.0, RAISED, 26.0 + 190.0 * i as f32, 76.0),
            children![label(name, 17.0, TEXT, 11.0, 8.0, B)],
        ))
        .observe(move |_: On<PointerClick>, mut on: ResMut<OnPage>| on.0 = page);
    }

    // Audio page: your ears & your name.
    p.spawn((
        Part::AudioCard,
        Node {
            width: px(410),
            height: px(436),
            border_radius: BorderRadius::all(px(18)),
            ..at(AUDIO_X, 141.0, B)
        },
        BackgroundColor(PANEL),
    ))
    .with_children(|c| {
        c.spawn((
            Text::new("Your ears & your name"),
            head(26.0, TEXT),
            at(20.0, 14.0, 0.0),
        ));
        for (v, name, y) in [
            (Volume::Master, "Master volume", 69.0),
            (Volume::Voice, "Voice volume", 164.0),
        ] {
            c.spawn(label(name, 19.0, TEXT, 20.0, y, 0.0));
            c.spawn((Part::Pct(v), label("", 19.0, ACCENT, 313.0, y, 0.0)));
            slider(c, v, 20.0, y + 32.0);
        }
        check_row(c, check, Check::Mic, "Microphone", 20.0, 258.0);
        check_row(c, check, Check::PushToTalk, "Push to talk (V)", 17.0, 306.0);
        check_row(
            c,
            check,
            Check::Halt,
            "Let HALT use my username",
            17.0,
            354.0,
        );
        check_row(c, check, Check::Hrtf, "3D sound (headphones)", 17.0, 402.0);
        c.spawn(label(
            "Voice is always connected.\nMute your mic, not your friends.",
            15.0,
            TEXT,
            20.0,
            446.0,
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
            ..at(CARD_X, 141.0, B)
        },
        BackgroundColor(PANEL),
        BorderColor::all(LINE),
    ))
    .with_children(|c| {
        c.spawn((
            Text::new("Video / pick your potato"),
            head(26.0, TEXT),
            at(18.0, 12.0, B),
        ));
        let tabs = [
            (Some(Preset::Low), "Low"),
            (Some(Preset::Medium), "Medium"),
            (Some(Preset::High), "High"),
            (None, "Custom"),
        ];
        for (i, (preset, name)) in tabs.into_iter().enumerate() {
            let mut tab = c.spawn((
                Part::PresetTab(preset),
                button(158.0, 42.0, 12.0, RAISED, 18.0 + 170.0 * i as f32, 55.0),
                children![label(name, 17.0, TEXT, 11.0, 8.0, B)],
            ));
            // "Custom" is what the options add up to, not a preset to pick:
            // nothing to press, so the pad skips it.
            if preset.is_none() {
                tab.remove::<PadFocusable>();
            }
            tab.observe(move |_: On<PointerClick>, mut q: ResMut<GraphicsQuality>| {
                if let Some(p) = preset {
                    let want = p.apply(*q);
                    q.set_if_neq(want);
                }
            });
        }
        video_page(c, check);
        c.spawn(label(
            "Preset changes apply immediately. Change an option: Custom.",
            14.0,
            TEXT,
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
            ..at(CARD_X, 141.0, B)
        },
        BackgroundColor(PANEL),
        BorderColor::all(LINE),
    ))
    .with_children(|c| {
        c.spawn((Text::new("Controls"), head(26.0, TEXT), at(18.0, 12.0, B)));
        // Which pad's button names hints use (Auto: from the pad plugged in).
        c.spawn((
            button(260.0, 34.0, 10.0, RAISED, 430.0, 10.0),
            children![(Part::Labels, label("", 15.0, TEXT, 11.0, 6.0, B))],
        ))
        .observe(|_: On<PointerClick>, mut pad: ResMut<PadSettings>| {
            pad.labels = next_labels(pad.labels);
        });
        // The gamepad's feel (#361), side by side.
        for (v, name, x) in [
            (Volume::Deadzone, "Stick deadzone", 18.0),
            (Volume::Look, "Stick look speed", 370.0),
        ] {
            c.spawn(label(name, 17.0, TEXT, x, 52.0, B));
            c.spawn((
                Part::Pct(v),
                label("", 17.0, ACCENT, x + PAD_SLIDER_W - 54.0, 52.0, B),
            ));
            slider(c, v, x - B, 82.0 - B);
        }
        // The bindings in two columns, so the list fits the card.
        for (i, x) in [18.0, 370.0].into_iter().enumerate() {
            c.spawn((
                Part::Help(i),
                Text::new(help_column(i, crate::input::device())),
                body(13.0, TEXT),
                Node {
                    width: px(PAD_SLIDER_W + 30.0),
                    ..at(x, 122.0, B)
                },
            ));
        }
    });

    p.spawn((
        button(178.0, 42.0, 12.0, RAISED, 780.0, 594.0),
        children![label("Reset defaults", 17.0, TEXT, 11.0, 8.0, B)],
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
        button(192.0, 42.0, 12.0, ACCENT, 978.0, 594.0),
        children![label("Done", 17.0, ON_ACCENT, 11.0, 8.0, B)],
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
                    BackgroundColor(ACCENT),
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
                    BackgroundColor(t::CARD_100),
                    BorderColor::all(t::INK),
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
            label(name, size, TEXT, 0.0, 4.0, 0.0),
            checkbox(check, what, 330.0, 0.0),
        ],
    ))
    .observe(
        move |_: On<PointerClick>, mut s: ResMut<Settings>| match what {
            Check::Mic => s.mic_on = !s.mic_on,
            Check::PushToTalk => s.push_to_talk = !s.push_to_talk,
            Check::Halt => s.halt_real_name = !s.halt_real_name,
            Check::Hrtf => s.hrtf = !s.hrtf,
            Check::Video(_) => {}
        },
    );
}

/// The width of one option card on the Video page, and the gap between them.
const CARD_W: f32 = 214.0;
const CARD_PITCH: f32 = 228.0;
/// A card's height; the render scale's is a little taller (it has a track).
const CARD_H: f32 = 52.0;

/// Which dropdown's list is showing (one at a time).
#[derive(Resource, Default)]
pub struct OpenDropdown(pub Option<VideoOption>);

/// A small group heading on the Video page.
fn group_heading(c: &mut ChildSpawnerCommands, text: &str, y: f32) {
    c.spawn(label(text, 14.0, ACCENT, 18.0, y, B));
}

/// The Video page below the presets, in two groups: the window (its size,
/// fullscreen, V-sync, render scale) and what's drawn in it
/// (shadows, anti-aliasing, ambient occlusion, bloom, grass, view distance).
/// Anything with more than two values is a dropdown, a range is a slider,
/// the rest are checkboxes.
fn video_page(c: &mut ChildSpawnerCommands, check: &Handle<Image>) {
    let col = |i: usize| 18.0 + CARD_PITCH * i as f32;
    let desktop = !cfg!(target_arch = "wasm32");
    group_heading(c, "WINDOW", 104.0);
    if desktop {
        dropdown(c, VideoOption::WindowSize, col(0), 126.0);
    } else {
        // A browser's Resolution is a Sharp / Fast switch, and it has no window.
        option_card(c, check, VideoOption::Resolution, col(0), 126.0);
    }
    option_card(c, check, VideoOption::Fullscreen, col(1), 126.0);
    option_card(c, check, VideoOption::VSync, col(2), 126.0);
    if desktop {
        scale_row(c, 18.0, 186.0, CARD_PITCH * 2.0 + CARD_W);
    }
    group_heading(c, "QUALITY", 252.0);
    dropdown(c, VideoOption::Shadows, col(0), 274.0);
    option_card(c, check, VideoOption::AntiAlias, col(1), 274.0);
    option_card(c, check, VideoOption::AmbientOcclusion, col(2), 274.0);
    option_card(c, check, VideoOption::Bloom, col(0), 334.0);
    option_card(c, check, VideoOption::Grass, col(1), 334.0);
    option_card(c, check, VideoOption::DrawDistance, col(2), 334.0);
    // Behind an open list, catching the click that should only close it.
    c.spawn((
        Part::DropBackdrop,
        Button,
        Node {
            position_type: PositionType::Absolute,
            left: px(0),
            top: px(0),
            width: percent(100),
            height: percent(100),
            display: Display::None,
            ..default()
        },
        BackgroundColor(Color::NONE),
        GlobalZIndex(25),
    ))
    .observe(|_: On<Activate>, mut open: ResMut<OpenDropdown>| open.0 = None);
}

/// A checkbox card: tick, name; a press flips it.
fn option_card(
    c: &mut ChildSpawnerCommands,
    check: &Handle<Image>,
    o: VideoOption,
    x: f32,
    y: f32,
) {
    let mut card = c.spawn((
        Node {
            width: px(CARD_W),
            height: px(CARD_H),
            border_radius: BorderRadius::all(px(12)),
            ..at(x, y, B)
        },
        BackgroundColor(RAISED),
        children![
            checkbox(check, Check::Video(o), 12.0, 12.0),
            (
                Part::BoxLabel(o),
                Text::new(o.name()),
                body(16.0, TEXT),
                Node {
                    width: px(150),
                    ..at(52.0, 16.0, 0.0)
                }
            ),
        ],
    ));
    // An option this build can't do has nothing to press.
    if GraphicsQuality::platform_default().value(o) != "N/A" {
        card.insert(PadFocusable);
    }
    card.observe(move |_: On<PointerClick>, mut q: ResMut<GraphicsQuality>| {
        if q.value(o) != "N/A" {
            q.cycle(o);
        }
    });
}

/// A dropdown card: the name, what it's set to and a chevron; pressing opens
/// a list of the values under it. Bevy's headless buttons all the way down
/// (the pad's A presses them too, and while the list is up it's the only
/// thing the pad can reach: it's on a higher layer).
fn dropdown(c: &mut ChildSpawnerCommands, o: VideoOption, x: f32, y: f32) {
    c.spawn((
        Button,
        Node {
            width: px(CARD_W),
            height: px(CARD_H),
            border_radius: BorderRadius::all(px(12)),
            ..at(x, y, B)
        },
        BackgroundColor(RAISED),
        PadFocusable,
    ))
    .observe(move |_: On<Activate>, mut open: ResMut<OpenDropdown>| {
        open.0 = if open.0 == Some(o) { None } else { Some(o) };
    })
    .with_children(|k| {
        k.spawn((
            Text::new(o.name()),
            body(16.0, TEXT),
            Pickable::IGNORE,
            at(14.0, 16.0, 0.0),
        ));
        k.spawn((
            Part::DropValue(o),
            Text::new(""),
            body(16.0, ACCENT),
            Pickable::IGNORE,
            Node {
                position_type: PositionType::Absolute,
                right: px(36),
                top: px(16),
                ..default()
            },
        ));
        // The chevron: a square's corner, turned.
        k.spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                top: px(18),
                width: px(10),
                height: px(10),
                border: UiRect {
                    right: px(2),
                    bottom: px(2),
                    ..default()
                },
                ..default()
            },
            BorderColor::all(TEXT),
            UiTransform::from_rotation(Rot2::degrees(45.0)),
            Pickable::IGNORE,
        ));
        k.spawn((
            Part::DropList(o),
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                top: px(CARD_H + 4.0),
                width: px(CARD_W),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
                padding: UiRect::all(px(6)),
                border: UiRect::all(px(B)),
                border_radius: BorderRadius::all(px(12)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(LINE),
            GlobalZIndex(30),
        ))
        .with_children(|list| {
            for (i, name) in o.choices().iter().enumerate() {
                list.spawn((
                    Part::DropItem(o, i),
                    Button,
                    Node {
                        width: percent(100),
                        height: px(36),
                        border_radius: BorderRadius::all(px(8)),
                        padding: UiRect::horizontal(px(10)),
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(RAISED),
                    PadFocusable,
                    children![(Text::new(*name), body(16.0, TEXT), Pickable::IGNORE)],
                ))
                .observe(
                    move |_: On<Activate>,
                          mut q: ResMut<GraphicsQuality>,
                          mut open: ResMut<OpenDropdown>| {
                        let mut next = *q;
                        next.set_choice(o, i);
                        q.set_if_neq(next);
                        open.0 = None;
                    },
                );
            }
        });
    });
}

/// The render scale: a row with its name and value over a slider (Bevy's
/// headless one) in the audio sliders' colours, 50% to 100% in steps of 10.
fn scale_row(c: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32) {
    let track = w - 28.0;
    c.spawn((
        Node {
            width: px(w),
            height: px(CARD_H + 6.0),
            border_radius: BorderRadius::all(px(12)),
            ..at(x, y, B)
        },
        BackgroundColor(RAISED),
        children![
            (
                Text::new("Render scale"),
                body(16.0, TEXT),
                at(14.0, 6.0, 0.0)
            ),
            (
                Part::ScaleValue,
                Text::new(""),
                body(16.0, ACCENT),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(14),
                    top: px(6),
                    ..default()
                },
            ),
        ],
    ))
    .with_children(|row| {
        row.spawn((
            Part::ScaleSlider,
            Slider {
                track_click: TrackClick::Snap,
                ..default()
            },
            SliderValue(100.0),
            SliderRange::new(50.0, 100.0),
            SliderStep(10.0),
            Node {
                width: px(track),
                height: px(22),
                ..at(14.0, 30.0, 0.0)
            },
            PadFocusable,
            PadSlider,
            children![
                (
                    Node {
                        width: px(track),
                        height: px(8),
                        border_radius: BorderRadius::all(px(4)),
                        ..at(0.0, 7.0, 0.0)
                    },
                    BackgroundColor(TRACK),
                    Pickable::IGNORE,
                ),
                (
                    Part::ScaleFill,
                    Node {
                        width: px(0),
                        height: px(8),
                        border_radius: BorderRadius::all(px(4)),
                        ..at(0.0, 7.0, 0.0)
                    },
                    BackgroundColor(ACCENT),
                    Pickable::IGNORE,
                ),
                (
                    Part::ScaleThumb,
                    SliderThumb,
                    Node {
                        width: px(SCALE_THUMB),
                        height: px(SCALE_THUMB),
                        border: UiRect::all(px(B)),
                        border_radius: BorderRadius::all(px(SCALE_THUMB / 2.0)),
                        ..at(0.0, 0.0, 0.0)
                    },
                    BackgroundColor(t::CARD_100),
                    BorderColor::all(t::INK),
                ),
            ],
        ))
        .observe(
            |change: On<ValueChange<f32>>, mut q: ResMut<GraphicsQuality>| {
                let mut next = *q;
                next.set_scale(change.value);
                q.set_if_neq(next);
            },
        )
        .observe(|nudge: On<PadNudge>, mut commands: Commands| {
            commands.trigger(SetSliderValue {
                entity: nudge.entity,
                change: SliderValueChange::RelativeStep(nudge.steps as f32),
            });
        });
    });
}

/// The render scale slider's thumb size, and its track's width (the row less
/// its 14 px margins).
const SCALE_THUMB: f32 = 22.0;

fn scale_track_width() -> f32 {
    CARD_PITCH * 2.0 + CARD_W - 28.0
}

/// Esc closes an open dropdown first, and the notebook only the time after:
/// the key is used up here, before the menus and the pause screen see it.
fn close_dropdown_on_esc(mut keys: ResMut<ButtonInput<KeyCode>>, mut open: ResMut<OpenDropdown>) {
    if open.0.is_some() && keys.just_pressed(KeyCode::Escape) {
        keys.clear_just_pressed(KeyCode::Escape);
        open.0 = None;
    }
}

/// A list left open doesn't wait for you when the notebook closes or turns
/// to another page.
fn forget_dropdown(
    mut open: ResMut<OpenDropdown>,
    on: Res<OnPage>,
    panel: Query<&Node, With<SettingsPanel>>,
) {
    let shown = panel.iter().any(|n| n.display != Display::None);
    if open.0.is_some() && (!shown || on.0 != Page::Video) {
        open.0 = None;
    }
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
        BackgroundColor(t::SOOT_900),
        BorderColor::all(LINE),
        Pickable::IGNORE,
        children![(
            Node {
                width: px(18),
                height: px(18),
                ..at(3.0, 3.0, B)
            },
            ImageNode::new(check.clone()).with_color(ON_ACCENT),
            Pickable::IGNORE,
        )],
    )
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn refresh(
    on: Res<OnPage>,
    s: Res<Settings>,
    pad: Res<PadSettings>,
    device: Res<crate::input::InputDevice>,
    q: Res<GraphicsQuality>,
    open: Res<OpenDropdown>,
    values: Query<&SliderValue>,
    mut commands: Commands,
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
                    bg.set_if_neq(BackgroundColor(if on { ACCENT } else { RAISED }));
                }
                for k in kids.into_iter().flatten() {
                    if let Ok(mut c) = texts.get_mut(*k) {
                        c.set_if_neq(TextColor(if on { ON_ACCENT } else { TEXT }));
                    }
                }
            };
        match *part {
            Part::PageTab(p) => tab(p == on.0, bg, &mut texts),
            Part::PresetTab(p) => tab(p == preset, bg, &mut texts),
            Part::AudioCard | Part::VideoCard | Part::ControlsCard => {
                let show = on.0.shows(*part);
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
            Part::Labels => {
                let want = labels_text(pad.labels);
                if let Some(mut t) = text
                    && t.0 != want
                {
                    t.0 = want;
                }
            }
            Part::Help(_) | Part::CloseHint => {
                let want = match *part {
                    Part::Help(i) => help_column(i, *device),
                    _ => format!(
                        "{}   close",
                        crate::input::label_with(
                            &crate::input::BINDINGS.get(),
                            "back",
                            *device,
                            &["menu".to_string()],
                        )
                    ),
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
                    Check::Mic => (s.mic_on, false),
                    Check::PushToTalk => (s.push_to_talk, !s.mic_on),
                    Check::Halt => (s.halt_real_name, false),
                    Check::Hrtf => (s.hrtf, false),
                    Check::Video(o) => (checked(*q, o), q.value(o) == "N/A"),
                };
                let (fill, edge) = match (disabled, on) {
                    (true, _) => (OFF_FILL, OFF_LINE),
                    (false, true) => (ACCENT, LINE),
                    (false, false) => (t::SOOT_900, LINE),
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
            Part::DropValue(o) => {
                let want = o.choices().get(q.choice(o)).copied().unwrap_or("");
                if let Some(mut t) = text
                    && t.0 != want
                {
                    t.0 = want.to_string();
                }
            }
            Part::DropList(o) => {
                if let Ok(mut n) = nodes.get_mut(e) {
                    let want = if open.0 == Some(o) {
                        Display::Flex
                    } else {
                        Display::None
                    };
                    if n.display != want {
                        n.display = want;
                    }
                }
            }
            Part::DropBackdrop => {
                if let Ok(mut n) = nodes.get_mut(e) {
                    let want = if open.0.is_some() {
                        Display::Flex
                    } else {
                        Display::None
                    };
                    if n.display != want {
                        n.display = want;
                    }
                }
            }
            Part::DropItem(o, i) => tab(q.choice(o) == i, bg, &mut texts),
            Part::ScaleValue => {
                let want = q.value(VideoOption::Resolution);
                if let Some(mut t) = text
                    && t.0 != want
                {
                    t.0 = want.to_string();
                }
            }
            Part::ScaleSlider => {
                let want = f32::from(q.scale);
                if values.get(e).is_ok_and(|v| v.0 != want) {
                    commands.entity(e).insert(SliderValue(want));
                }
            }
            Part::ScaleFill | Part::ScaleThumb => {
                // The thumb travels the track less its own width.
                let frac = (f32::from(q.scale) - 50.0) / 50.0;
                if let Ok(mut n) = nodes.get_mut(e) {
                    let travel = scale_track_width() - SCALE_THUMB;
                    let want = if matches!(part, Part::ScaleFill) {
                        (px(travel * frac + SCALE_THUMB / 2.0), n.left)
                    } else {
                        (n.width, px(travel * frac))
                    };
                    if (n.width, n.left) != want {
                        (n.width, n.left) = want;
                    }
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
    fn each_page_shows_only_its_card() {
        let cards = [Part::AudioCard, Part::VideoCard, Part::ControlsCard];
        for (page, want) in [(Page::Audio, 0), (Page::Video, 1), (Page::Controls, 2)] {
            let shown: Vec<usize> = (0..3).filter(|&i| page.shows(cards[i])).collect();
            assert_eq!(shown, vec![want]);
        }
    }

    #[test]
    fn every_box_is_a_real_option() {
        assert!(!VIDEO_BOXES.contains(&VideoOption::Preset));
        assert_eq!(VIDEO_BOXES.len(), 9);
        // Dropdowns are the options with more than two values.
        for o in VideoOption::ALL {
            if o != VideoOption::Preset {
                assert_ne!(o.choices().len(), 2, "{o:?} is a checkbox, or has 3+");
            }
        }
    }

    /// The Video page's dropdowns and slider, pressed the way the widgets
    /// report a press (a click, or the pad's A).
    #[test]
    fn dropdowns_and_the_scale_slider_change_the_settings() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins((ButtonPlugin, SliderPlugin))
            .init_resource::<OpenDropdown>()
            .insert_resource(Preset::High.apply(GraphicsQuality::platform_default()));
        app.world_mut()
            .commands()
            .spawn(Node::default())
            .with_children(|c| video_page(c, &Handle::default()));
        app.world_mut().flush();
        app.update();
        let find = |app: &mut App, want: Part| -> Entity {
            let mut parts = app.world_mut().query::<(Entity, &Part)>();
            let Some((e, _)) = parts.iter(app.world()).find(|(_, p)| {
                std::mem::discriminant(*p) == std::mem::discriminant(&want) && same(**p, want)
            }) else {
                panic!("a part the Video page should have is missing");
            };
            e
        };
        fn same(a: Part, b: Part) -> bool {
            match (a, b) {
                (Part::DropList(x), Part::DropList(y)) => x == y,
                (Part::DropItem(x, i), Part::DropItem(y, j)) => x == y && i == j,
                (Part::ScaleSlider, Part::ScaleSlider) => true,
                _ => false,
            }
        }
        // Pressing the Shadows dropdown opens its list ...
        let list = find(&mut app, Part::DropList(VideoOption::Shadows));
        let button = app
            .world()
            .get::<ChildOf>(list)
            .map(ChildOf::parent)
            .unwrap_or(list);
        app.world_mut().trigger(Activate { entity: button });
        app.update();
        assert_eq!(
            app.world().resource::<OpenDropdown>().0,
            Some(VideoOption::Shadows)
        );
        // ... and pressing "Off" in it sets that and closes it.
        let off = find(&mut app, Part::DropItem(VideoOption::Shadows, 0));
        app.world_mut().trigger(Activate { entity: off });
        app.update();
        let q = *app.world().resource::<GraphicsQuality>();
        assert_eq!(q.value(VideoOption::Shadows), "Off");
        assert_eq!(app.world().resource::<OpenDropdown>().0, None);
        // The window size list has every size.
        if !cfg!(target_arch = "wasm32") {
            let big = find(&mut app, Part::DropItem(VideoOption::WindowSize, 3));
            app.world_mut().trigger(Activate { entity: big });
            app.update();
            let q = *app.world().resource::<GraphicsQuality>();
            assert_eq!(q.value(VideoOption::WindowSize), "1920x1080");
            // The slider moves the render scale, snapped to its steps.
            let slider = find(&mut app, Part::ScaleSlider);
            app.world_mut().trigger(SetSliderValue {
                entity: slider,
                change: SliderValueChange::Absolute(63.0),
            });
            app.update();
            assert_eq!(app.world().resource::<GraphicsQuality>().scale, 60);
            // A pad nudge is one step. (`refresh` keeps the slider on the
            // setting; here, that's done by hand.)
            app.world_mut().entity_mut(slider).insert(SliderValue(60.0));
            app.world_mut().trigger(PadNudge {
                entity: slider,
                steps: 1,
            });
            app.update();
            assert_eq!(app.world().resource::<GraphicsQuality>().scale, 70);
        }
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
        let lines = |d| help_column(0, d).lines().count() + help_column(1, d).lines().count();
        use crate::input::{InputDevice, PadFamily};
        for d in [
            InputDevice::KeyboardMouse,
            InputDevice::Pad(PadFamily::Xbox),
        ] {
            assert_eq!(lines(d), crate::controls::bindings_list(d).len());
        }
    }
}
