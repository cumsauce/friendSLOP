//! Touch and gamepad controls (#106, WeiYa). Both feed the same input the
//! keyboard and mouse do, so every feature works without knowing about them:
//!
//! - Movement, look, jump and run go into [`VirtualInput`], which
//!   `fps::walk_controls` mixes in with WASD / mouse / Space / Shift.
//!   Climbing is "hold forward against a wall", so the stick climbs too.
//! - Every other button presses the matching key (or the left mouse button
//!   for "Use", which clicks whatever is under the crosshair) in
//!   `ButtonInput`, right after Bevy reads the real devices.
//!
//! Touch: put your left thumb down anywhere on the left side for a joystick
//! (it appears under your thumb), drag anywhere else on the right to look,
//! and tap the round buttons. The overlay only shows once the screen has been
//! touched. Gamepad: left stick move, right stick look, A jump, B or RT use,
//! LB (or clicking the left stick) run, X fart, Y help, Back scoreboard,
//! D-pad up camera, down mute, left journal, right answer the phone, right
//! stick click "Enter". Holding RB or LT (#361) switches the face buttons
//! and D-pad to a second / third set of keys so every keyboard-only action
//! reaches a pad (Steam Deck): a cheat sheet shows while one is held
//! ([`PAD_LAYERS`]). Start opens the settings notebook (its "Leave game"
//! button leaves). Menus (the title screen, the notebook) take the pad
//! instead: D-pad / left stick move the focus, A clicks, B backs out
//! (`ui::pad_nav`). Stick feel (look speed, response curve, deadzone) is
//! `assets/data/game/controls.ron`.

use std::collections::HashMap;

use bevy::input::InputSystems;
use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{AccumulatedMouseMotion, MouseButtonInput};
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::AppState;
use crate::features::fps::FpsView;
pub use friendslop_base::controls::*;

/// Radians of look per pixel dragged on a touch screen.
const TOUCH_LOOK: f32 = 0.006;
/// Joystick throw in logical pixels.
const STICK_RADIUS: f32 = 60.0;
/// The gamepad's bindings, for the settings notebook's Controls page (Xbox
/// names: the Steam Deck shows its pad as one).
pub const PAD_BINDINGS: &[(&str, &str)] = &[
    ("Left stick", "walk (click: run)"),
    ("Right stick", "look around"),
    ("A", "jump"),
    ("B / RT", "use what's under the crosshair"),
    ("LB", "run (hold)"),
    ("X", "fart"),
    ("Y", "help, in the journal"),
    ("D-pad up", "first person <-> overview camera"),
    ("View (Back)", "the journal / scoreboard"),
    ("Menu (Start)", "settings (and Leave game)"),
    ("In menus", "D-pad / stick move, A press, B back"),
];

/// Whether a gamepad was the last thing used (#361): prompts and menus name
/// its buttons then.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct PadLast(pub bool);

/// Real devices only: `gather` presses keys for the pad in `ButtonInput`,
/// but never sends keyboard or mouse messages.
fn track_last_input(
    pads: Query<&Gamepad>,
    mut keys: MessageReader<KeyboardInput>,
    mut clicks: MessageReader<MouseButtonInput>,
    motion: Res<AccumulatedMouseMotion>,
    touches: Res<Touches>,
    mut last: ResMut<PadLast>,
) {
    let pad = pads.iter().any(|p| {
        p.get_just_pressed().next().is_some()
            || p.left_stick().length() > 0.5
            || p.right_stick().length() > 0.5
    });
    let other = keys.read().count() > 0
        || clicks.read().count() > 0
        || motion.delta.length() > 4.0
        || touches.any_just_pressed();
    let want = pad || (!other && last.0);
    last.set_if_neq(PadLast(want));
}

/// Touch-stick deadzone (the pad's is a setting, [`PadSettings`]).
const DEADZONE: f32 = 0.15;

/// Gamepad stick feel (#361): `assets/data/game/controls.ron`. The deadzone and
/// a look-speed multiplier are the player's ([`PadSettings`]).
#[derive(serde::Deserialize, Default)]
pub struct PadTuning {
    /// Radians per second of look at full right-stick deflection.
    pub look_speed: f32,
    /// Vertical look speed as a share of horizontal.
    pub look_vertical: f32,
    /// Look response curve exponent (1 = linear; higher = finer aim near centre).
    pub look_curve: f32,
}

pub static PAD: friendslop_data::Data<PadTuning> =
    friendslop_data::Data::ron("data/game/controls.ron");

/// Which set of keys the face buttons and D-pad press.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PadLayer {
    Base,
    /// RB held.
    Shift,
    /// LT held.
    Alt,
}

/// Face buttons then D-pad, in this order, per layer.
const PAD_SLOTS: [GamepadButton; 8] = [
    GamepadButton::South,
    GamepadButton::East,
    GamepadButton::West,
    GamepadButton::North,
    GamepadButton::DPadUp,
    GamepadButton::DPadDown,
    GamepadButton::DPadLeft,
    GamepadButton::DPadRight,
];

/// (key, cheat-sheet label) per slot; `None` = jump / use (base layer).
type Slot = (Option<KeyCode>, &'static str);

/// The three layers. Base A/B are jump and use, handled separately.
pub const PAD_LAYERS: [(PadLayer, [Slot; 8]); 3] = [
    (
        PadLayer::Base,
        [
            (None, "jump"),
            (None, "use"),
            (Some(KeyCode::KeyF), "fart"),
            (Some(KeyCode::KeyH), "help"),
            (Some(KeyCode::KeyV), "camera"),
            (Some(KeyCode::KeyM), "mute"),
            (Some(KeyCode::KeyJ), "journal"),
            (Some(KeyCode::KeyE), "answer"),
        ],
    ),
    (
        PadLayer::Shift,
        [
            (Some(KeyCode::KeyY), "yes"),
            (Some(KeyCode::KeyN), "no"),
            (Some(KeyCode::KeyZ), "sleep"),
            (Some(KeyCode::KeyX), "piss"),
            (Some(KeyCode::KeyR), "cast"),
            (Some(KeyCode::KeyG), "pry"),
            (Some(KeyCode::KeyK), "hide & seek"),
            (Some(KeyCode::KeyL), "lex"),
        ],
    ),
    (
        PadLayer::Alt,
        [
            (Some(KeyCode::KeyP), "smoke"),
            (Some(KeyCode::KeyB), "brag"),
            (Some(KeyCode::KeyI), "insure"),
            (Some(KeyCode::KeyQ), "mute spell"),
            (Some(KeyCode::Digit1), "duel 1"),
            (Some(KeyCode::Digit3), "duel 3"),
            (Some(KeyCode::Digit4), "duel 4"),
            (Some(KeyCode::Digit2), "duel 2"),
        ],
    ),
];

/// The layer for what's held: RB wins over LT.
pub fn pad_layer(rb: bool, lt: bool) -> PadLayer {
    if rb {
        PadLayer::Shift
    } else if lt {
        PadLayer::Alt
    } else {
        PadLayer::Base
    }
}

/// Keys the held slots press on this layer.
pub fn pad_keys(layer: PadLayer, held: [bool; 8]) -> Vec<KeyCode> {
    let Some((_, slots)) = PAD_LAYERS.iter().find(|(l, _)| *l == layer) else {
        return Vec::new();
    };
    slots
        .iter()
        .zip(held)
        .filter_map(|((k, _), h)| if h { *k } else { None })
        .collect()
}

/// Shapes a look stick: deadzone, then `|v|^curve` keeping direction.
pub fn look_response(v: Vec2, deadzone: f32, curve: f32) -> Vec2 {
    let len = v.length();
    if len <= deadzone || deadzone >= 1.0 {
        return Vec2::ZERO;
    }
    let t = ((len.min(1.0) - deadzone) / (1.0 - deadzone)).powf(curve.max(0.1));
    v / len * t
}

/// The cheat sheet's text for a layer.
pub fn cheat_sheet(layer: PadLayer) -> String {
    let names = ["A", "B", "X", "Y", "Up", "Down", "Left", "Right"];
    let Some((_, slots)) = PAD_LAYERS.iter().find(|(l, _)| *l == layer) else {
        return String::new();
    };
    let title = match layer {
        PadLayer::Base => "Pad",
        PadLayer::Shift => "RB +",
        PadLayer::Alt => "LT +",
    };
    let body: Vec<String> = names
        .iter()
        .zip(slots)
        .map(|(n, (_, label))| format!("{n} {label}"))
        .collect();
    format!("{title}  {}", body.join("   "))
}

/// Gamepad feel (#361), changed in the settings notebook's Controls page and
/// saved locally like the video options (`pad.txt`).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct PadSettings {
    /// Radial stick deadzone, `0..=MAX_DEADZONE` of full throw.
    pub deadzone: f32,
    /// Look speed, a multiple of the default, `LOOK_MIN..=LOOK_MAX`.
    pub look: f32,
}

impl PadSettings {
    pub const DEFAULT: Self = Self {
        deadzone: 0.12,
        look: 1.0,
    };
    pub const MAX_DEADZONE: f32 = 0.4;
    pub const LOOK_MIN: f32 = 0.25;
    pub const LOOK_MAX: f32 = 3.0;
    const FILE: &str = "pad.txt";

    pub fn to_text(self) -> String {
        format!("deadzone={}\nlook={}\n", self.deadzone, self.look)
    }

    /// Read [`Self::to_text`]; missing or broken lines keep their defaults.
    pub fn from_text(text: &str) -> Self {
        let mut p = Self::DEFAULT;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let Ok(v) = v.trim().parse::<f32>() else {
                continue;
            };
            match k.trim() {
                "deadzone" => p.deadzone = v,
                "look" => p.look = v,
                _ => {}
            }
        }
        p.clamped()
    }

    pub fn clamped(self) -> Self {
        let ok = |v: f32, lo: f32, hi: f32, d: f32| {
            if v.is_finite() { v.clamp(lo, hi) } else { d }
        };
        Self {
            deadzone: ok(
                self.deadzone,
                0.0,
                Self::MAX_DEADZONE,
                Self::DEFAULT.deadzone,
            ),
            look: ok(
                self.look,
                Self::LOOK_MIN,
                Self::LOOK_MAX,
                Self::DEFAULT.look,
            ),
        }
    }
}

impl Default for PadSettings {
    fn default() -> Self {
        crate::settings::load_local(Self::FILE)
            .map(|t| Self::from_text(&t))
            .unwrap_or(Self::DEFAULT)
    }
}

fn save_pad_settings(p: Res<PadSettings>) {
    if p.is_changed() && !p.is_added() {
        crate::settings::save_local(PadSettings::FILE, &p.to_text());
    }
}

/// Set once the screen is touched: shows the touch overlay.
#[derive(Resource, Default, Debug)]
pub struct TouchMode(pub bool);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    /// Hold a key while the button is held.
    Key(KeyCode),
    /// Click (left mouse) at the crosshair.
    Use,
    Jump,
    /// Tap to toggle running.
    RunToggle,
    /// Esc: open the settings notebook (its "Leave game" button leaves).
    Leave,
}

#[derive(Clone, Copy)]
enum Corner {
    BottomRight,
    TopRight,
    TopLeft,
}

struct TouchButton {
    label: &'static str,
    corner: Corner,
    /// Centre, in logical px from the corner.
    at: Vec2,
    radius: f32,
    action: Action,
}

const BUTTONS: &[TouchButton] = &[
    TouchButton {
        label: "Jump",
        corner: Corner::BottomRight,
        at: Vec2::new(80.0, 80.0),
        radius: 46.0,
        action: Action::Jump,
    },
    TouchButton {
        label: "Use",
        corner: Corner::BottomRight,
        at: Vec2::new(190.0, 62.0),
        radius: 38.0,
        action: Action::Use,
    },
    TouchButton {
        label: "Run",
        corner: Corner::BottomRight,
        at: Vec2::new(70.0, 190.0),
        radius: 32.0,
        action: Action::RunToggle,
    },
    TouchButton {
        label: "Fart",
        corner: Corner::BottomRight,
        at: Vec2::new(170.0, 160.0),
        radius: 30.0,
        action: Action::Key(KeyCode::KeyF),
    },
    TouchButton {
        label: "Help",
        corner: Corner::TopRight,
        at: Vec2::new(40.0, 40.0),
        radius: 26.0,
        action: Action::Key(KeyCode::KeyH),
    },
    TouchButton {
        label: "Score",
        corner: Corner::TopRight,
        at: Vec2::new(105.0, 40.0),
        radius: 26.0,
        action: Action::Key(KeyCode::Tab),
    },
    TouchButton {
        label: "View",
        corner: Corner::TopRight,
        at: Vec2::new(170.0, 40.0),
        radius: 26.0,
        action: Action::Key(KeyCode::KeyV),
    },
    TouchButton {
        label: "Leave",
        corner: Corner::TopLeft,
        at: Vec2::new(40.0, 40.0),
        radius: 26.0,
        action: Action::Leave,
    },
];

impl TouchButton {
    /// Centre in window coordinates (logical px, origin top-left).
    fn centre(&self, window: Vec2) -> Vec2 {
        match self.corner {
            Corner::BottomRight => window - self.at,
            Corner::TopRight => Vec2::new(window.x - self.at.x, self.at.y),
            Corner::TopLeft => self.at,
        }
    }
}

/// What each finger on the screen is doing.
#[derive(Clone, Copy, Debug)]
enum Finger {
    Stick { start: Vec2 },
    Look { last: Vec2 },
    Button(usize),
}

#[derive(Resource, Default)]
struct TouchState {
    fingers: HashMap<u64, Finger>,
    running: bool,
    /// Keys / mouse buttons we pressed last frame, released when no longer held.
    keys: Vec<KeyCode>,
    mouse: bool,
    /// The pad layer held this frame (shows the cheat sheet).
    layer: Option<PadLayer>,
    /// A menu had the pad: ignore A / B / RT until they're let go.
    pad_hold: bool,
}

#[derive(Component)]
struct PadSheet;

#[derive(Component)]
struct TouchOverlay;

#[derive(Component)]
struct StickBase;

#[derive(Component)]
struct StickKnob;

#[derive(Component)]
struct ButtonDot(usize);

pub fn plugin(app: &mut App) {
    app.init_resource::<VirtualInput>()
        .init_resource::<TouchMode>()
        .init_resource::<TouchState>()
        .init_resource::<PadSettings>()
        .init_resource::<PadLast>()
        .add_systems(PreUpdate, track_last_input.after(InputSystems))
        .add_systems(OnEnter(AppState::InGame), (spawn_overlay, spawn_sheet))
        .add_systems(PreUpdate, gather.after(InputSystems))
        .add_systems(
            Update,
            (draw_overlay, draw_sheet).run_if(in_state(AppState::InGame)),
        )
        .add_systems(PostUpdate, save_pad_settings);
}

/// A radial deadzone `dz`, rescaled so the stick still reaches every speed:
/// zero just past the deadzone, full at full throw.
fn axis(v: Vec2, dz: f32) -> Vec2 {
    let len = v.length();
    if len <= dz || len <= 0.0 {
        return Vec2::ZERO;
    }
    let dz = dz.clamp(0.0, 0.95);
    v / len * ((len - dz) / (1.0 - dz)).min(1.0)
}

#[allow(clippy::too_many_arguments)]
fn gather(
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    window: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time>,
    state: Res<State<AppState>>,
    overlay: Res<crate::ui::Overlay>,
    pad_cfg: Res<PadSettings>,
    mut mode: ResMut<TouchMode>,
    mut st: ResMut<TouchState>,
    mut out: ResMut<VirtualInput>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut view: ResMut<FpsView>,
) {
    *out = VirtualInput::default();
    if touches.any_just_pressed() {
        mode.0 = true;
    }
    let in_game = *state.get() == AppState::InGame;
    let size = window.single().map_or(Vec2::ZERO, |w| w.size());

    // Touch: assign new fingers, forget lifted ones.
    let mut want_keys: Vec<KeyCode> = Vec::new();
    let mut want_mouse = false;
    let mut leave = false;
    for t in touches
        .iter_just_released()
        .chain(touches.iter_just_canceled())
    {
        st.fingers.remove(&t.id());
    }
    if in_game {
        for t in touches.iter_just_pressed() {
            let p = t.position();
            let hit = BUTTONS
                .iter()
                .position(|b| b.centre(size).distance(p) <= b.radius + 8.0);
            let finger = match hit {
                Some(i) => {
                    match BUTTONS[i].action {
                        Action::RunToggle => st.running = !st.running,
                        Action::Leave => leave = true,
                        _ => {}
                    }
                    Finger::Button(i)
                }
                None if p.x < size.x * 0.4 => Finger::Stick { start: p },
                None => Finger::Look { last: p },
            };
            st.fingers.insert(t.id(), finger);
        }
    } else {
        st.fingers.clear();
        st.running = false;
    }
    let ids: Vec<u64> = st.fingers.keys().copied().collect();
    for id in ids {
        let Some(t) = touches.get_pressed(id) else {
            st.fingers.remove(&id);
            continue;
        };
        let p = t.position();
        let Some(finger) = st.fingers.get_mut(&id) else {
            continue;
        };
        match finger {
            Finger::Stick { start } => {
                let d = (p - *start) / STICK_RADIUS;
                out.stick = axis(Vec2::new(d.x, -d.y), DEADZONE);
            }
            Finger::Look { last } => {
                out.look += (p - *last) * TOUCH_LOOK;
                *last = p;
            }
            Finger::Button(i) => match BUTTONS[*i].action {
                Action::Key(k) => want_keys.push(k),
                Action::Use => want_mouse = true,
                Action::Jump => out.jump = true,
                Action::RunToggle | Action::Leave => {}
            },
        }
    }
    out.run = st.running;

    // Gamepads: the first connected one(s) all count. While a menu owns the
    // pad (`ui::pad_nav`), A and the D-pad drive it instead, B backs out
    // (Esc), and buttons still held when the menu closes wait for release.
    let dt = time.delta_secs();
    let tune = PAD.get();
    let ui = crate::ui::pad_nav::ui_owns_pad(state.get(), &overlay);
    let held_any = gamepads.iter().any(|p| {
        p.pressed(GamepadButton::South)
            || p.pressed(GamepadButton::East)
            || p.pressed(GamepadButton::RightTrigger)
            || p.pressed(GamepadButton::RightTrigger2)
    });
    st.pad_hold = ui || (st.pad_hold && held_any);
    st.layer = None;
    for pad in &gamepads {
        for (button, key) in [
            (GamepadButton::Select, KeyCode::Tab),
            (GamepadButton::Start, KeyCode::Escape),
        ] {
            if pad.pressed(button) {
                want_keys.push(key);
            }
        }
        if ui {
            if pad.pressed(GamepadButton::East) {
                want_keys.push(KeyCode::Escape);
            }
            continue;
        }
        let move_stick = axis(pad.left_stick(), pad_cfg.deadzone);
        if move_stick != Vec2::ZERO {
            out.stick = move_stick;
        }
        let look = look_response(pad.right_stick(), pad_cfg.deadzone, tune.look_curve);
        out.look +=
            Vec2::new(look.x, -look.y * tune.look_vertical) * tune.look_speed * pad_cfg.look * dt;
        out.run |= pad.pressed(GamepadButton::LeftTrigger) || pad.pressed(GamepadButton::LeftThumb);
        let layer = pad_layer(
            pad.pressed(GamepadButton::RightTrigger),
            pad.pressed(GamepadButton::LeftTrigger2),
        );
        if layer != PadLayer::Base {
            st.layer = Some(layer);
        }
        let held = PAD_SLOTS.map(|b| pad.pressed(b));
        if layer == PadLayer::Base && !st.pad_hold {
            out.jump |= held[0];
            want_mouse |= held[1] || pad.pressed(GamepadButton::RightTrigger2);
        }
        want_keys.extend(pad_keys(layer, held));
        for (button, key) in [
            (GamepadButton::RightThumb, KeyCode::Enter),
        ] {
            if pad.pressed(button) {
                want_keys.push(key);
            }
        }
    }

    if leave {
        // Skip "first Esc frees the cursor": there is no cursor on a phone.
        view.released = true;
        want_keys.push(KeyCode::Escape);
    }
    if want_mouse && !st.mouse && mode.0 {
        // A tap can't re-grab a cursor, so "Use" always uses.
        view.released = false;
    }

    // Press what's newly held, release what we pressed and isn't any more.
    let held = std::mem::take(&mut st.keys);
    for k in &held {
        if !want_keys.contains(k) {
            keys.release(*k);
        }
    }
    for k in &want_keys {
        if !held.contains(k) {
            keys.press(*k);
        }
    }
    st.keys = want_keys;
    if want_mouse && !st.mouse {
        mouse.press(MouseButton::Left);
    } else if !want_mouse && st.mouse {
        mouse.release(MouseButton::Left);
    }
    st.mouse = want_mouse;
    if !in_game {
        out.stick = Vec2::ZERO;
        out.look = Vec2::ZERO;
    }
}

fn spawn_sheet(mut commands: Commands) {
    commands.spawn((
        Name::new("Pad cheat sheet"),
        PadSheet,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(90),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Pickable::IGNORE,
        Visibility::Hidden,
        GlobalZIndex(60),
        DespawnOnExit(AppState::InGame),
        children![(
            Text::new(""),
            TextFont::from_font_size(16.0),
            TextColor(LABEL),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            Node {
                padding: UiRect::axes(px(12), px(6)),
                ..default()
            },
            Pickable::IGNORE,
        )],
    ));
}

/// Shows which keys the held layer's buttons press.
fn draw_sheet(
    st: Res<TouchState>,
    mut sheet: Query<(&mut Visibility, &Children), With<PadSheet>>,
    mut text: Query<&mut Text>,
) {
    let Ok((mut vis, children)) = sheet.single_mut() else {
        return;
    };
    let want = if st.layer.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    vis.set_if_neq(want);
    let Some(layer) = st.layer else { return };
    for c in children {
        if let Ok(mut t) = text.get_mut(*c) {
            let s = cheat_sheet(layer);
            if t.0 != s {
                t.0 = s;
            }
        }
    }
}

const DOT: Color = Color::srgba(1.0, 1.0, 1.0, 0.22);
const DOT_HELD: Color = Color::srgba(1.0, 0.85, 0.4, 0.45);
const LABEL: Color = Color::srgba(1.0, 1.0, 1.0, 0.85);

fn circle(radius: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        width: px(radius * 2.0),
        height: px(radius * 2.0),
        border_radius: BorderRadius::all(px(radius)),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
}

fn spawn_overlay(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Touch controls"),
            TouchOverlay,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                ..default()
            },
            Pickable::IGNORE,
            Visibility::Hidden,
            GlobalZIndex(50),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|ui| {
            ui.spawn((
                StickBase,
                circle(STICK_RADIUS),
                BackgroundColor(DOT),
                Visibility::Hidden,
                Pickable::IGNORE,
            ));
            ui.spawn((
                StickKnob,
                circle(26.0),
                BackgroundColor(DOT_HELD),
                Visibility::Hidden,
                Pickable::IGNORE,
            ));
            for (i, b) in BUTTONS.iter().enumerate() {
                let mut node = circle(b.radius);
                let edge = |v: f32| px(v - b.radius);
                match b.corner {
                    Corner::BottomRight => {
                        node.right = edge(b.at.x);
                        node.bottom = edge(b.at.y);
                    }
                    Corner::TopRight => {
                        node.right = edge(b.at.x);
                        node.top = edge(b.at.y);
                    }
                    Corner::TopLeft => {
                        node.left = edge(b.at.x);
                        node.top = edge(b.at.y);
                    }
                }
                ui.spawn((ButtonDot(i), node, BackgroundColor(DOT), Pickable::IGNORE))
                    .with_child((
                        Text::new(b.label),
                        TextFont::from_font_size(if b.radius > 35.0 { 18.0 } else { 14.0 }),
                        TextColor(LABEL),
                        Pickable::IGNORE,
                    ));
            }
        });
}

fn draw_overlay(
    mode: Res<TouchMode>,
    st: Res<TouchState>,
    touches: Res<Touches>,
    mut overlay: Query<&mut Visibility, With<TouchOverlay>>,
    mut base: Query<
        (&mut Node, &mut Visibility),
        (With<StickBase>, Without<TouchOverlay>, Without<StickKnob>),
    >,
    mut knob: Query<
        (&mut Node, &mut Visibility),
        (With<StickKnob>, Without<TouchOverlay>, Without<StickBase>),
    >,
    mut dots: Query<(&ButtonDot, &mut BackgroundColor)>,
) {
    for mut v in &mut overlay {
        v.set_if_neq(if mode.0 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
    if !mode.0 {
        return;
    }
    let stick = st.fingers.iter().find_map(|(id, f)| match f {
        Finger::Stick { start } => touches.get_pressed(*id).map(|t| (*start, t.position())),
        _ => None,
    });
    let show = |v: &mut Mut<Visibility>, on: bool| {
        v.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    };
    if let Ok((mut node, mut v)) = base.single_mut() {
        show(&mut v, stick.is_some());
        if let Some((start, _)) = stick {
            node.left = px(start.x - STICK_RADIUS);
            node.top = px(start.y - STICK_RADIUS);
        }
    }
    if let Ok((mut node, mut v)) = knob.single_mut() {
        show(&mut v, stick.is_some());
        if let Some((start, now)) = stick {
            let p = start + (now - start).clamp_length_max(STICK_RADIUS);
            node.left = px(p.x - 26.0);
            node.top = px(p.y - 26.0);
        }
    }
    for (dot, mut bg) in &mut dots {
        let held = st
            .fingers
            .values()
            .any(|f| matches!(f, Finger::Button(i) if *i == dot.0))
            || (BUTTONS[dot.0].action == Action::RunToggle && st.running);
        let want = if held { DOT_HELD } else { DOT };
        if bg.0 != want {
            bg.0 = want;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_dont_overlap() {
        let size = Vec2::new(844.0, 390.0); // a landscape phone
        for (i, a) in BUTTONS.iter().enumerate() {
            for b in &BUTTONS[i + 1..] {
                let gap = a.centre(size).distance(b.centre(size));
                assert!(
                    gap > a.radius + b.radius,
                    "{} overlaps {}",
                    a.label,
                    b.label
                );
            }
        }
    }

    #[test]
    fn deadzone_and_clamp() {
        assert_eq!(axis(Vec2::new(0.1, 0.0), 0.15), Vec2::ZERO);
        assert!((axis(Vec2::new(3.0, 4.0), 0.15).length() - 1.0).abs() < 1e-5);
        // Rescaled: just past the deadzone is slow, full throw is full speed.
        assert!(axis(Vec2::new(0.0, 0.2), 0.15).length() < 0.1);
        assert!((axis(Vec2::new(0.0, 1.0), 0.15).y - 1.0).abs() < 1e-5);
        // Radial: a diagonal keeps its direction.
        let d = axis(Vec2::new(0.5, 0.5), 0.15);
        assert!((d.x - d.y).abs() < 1e-6);
        assert_eq!(axis(Vec2::new(0.3, 0.0), 0.0), Vec2::new(0.3, 0.0));
    }

    #[test]
    fn pad_settings_round_trip_and_clamp() {
        let p = PadSettings {
            deadzone: 0.25,
            look: 1.5,
        };
        assert_eq!(PadSettings::from_text(&p.to_text()), p);
        assert_eq!(PadSettings::from_text("junk"), PadSettings::DEFAULT);
        let wild = PadSettings::from_text("deadzone=9\nlook=-1\n");
        assert_eq!(wild.deadzone, PadSettings::MAX_DEADZONE);
        assert_eq!(wild.look, PadSettings::LOOK_MIN);
        assert_eq!(PadSettings::from_text("look=NaN").look, 1.0);
    }

    #[test]
    fn pad_layers() {
        assert_eq!(pad_layer(true, true), PadLayer::Shift);
        assert_eq!(pad_layer(false, true), PadLayer::Alt);
        let mut held = [false; 8];
        held[0] = true; // A: jump on base (no key), "yes" on RB
        held[2] = true;
        assert_eq!(pad_keys(PadLayer::Base, held), vec![KeyCode::KeyF]);
        assert_eq!(
            pad_keys(PadLayer::Shift, held),
            vec![KeyCode::KeyY, KeyCode::KeyZ]
        );
        assert!(cheat_sheet(PadLayer::Alt).contains("smoke"));
    }

    #[test]
    fn no_key_on_two_slots() {
        let mut all: Vec<KeyCode> = PAD_LAYERS
            .iter()
            .flat_map(|(_, s)| s.iter().filter_map(|(k, _)| *k))
            .collect();
        let n = all.len();
        all.sort_by_key(|k| format!("{k:?}"));
        all.dedup();
        assert_eq!(all.len(), n);
    }

    #[test]
    fn look_curve() {
        assert_eq!(look_response(Vec2::new(0.1, 0.0), 0.15, 2.0), Vec2::ZERO);
        let full = look_response(Vec2::new(1.0, 0.0), 0.15, 2.0);
        assert!((full.x - 1.0).abs() < 1e-5);
        let half = look_response(Vec2::new(0.575, 0.0), 0.15, 2.0);
        assert!((half.x - 0.25).abs() < 1e-3);
    }
}
