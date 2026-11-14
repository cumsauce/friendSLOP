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
//! touched. Gamepad: every button's action comes from
//! `assets/data/base/bindings.ron` ([`crate::input`]): it presses that
//! action's key, so features don't know about pads. Rare actions sit on the
//! action wheel (hold RB, point the right stick, let go; LB flips the page).
//! While an event is on (a duel invite, the phone, a chapter card, smoking,
//! sleeping) its context takes over buttons and a prompt bar says which.
//! Menus (the title screen, the notebook) take the pad instead: D-pad / left
//! stick move the focus, A clicks, B backs out (`ui::pad_nav`). Stick feel
//! (look speed, response curve, deadzone) is
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
/// Every binding for the notebook's Controls page, for `device`.
pub fn bindings_list(device: crate::input::InputDevice) -> Vec<(String, String)> {
    crate::input::help_rows(device)
}

/// Whether a gamepad was the last thing used (#361): prompts and menus name
/// its buttons then.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct PadLast(pub bool);

/// Real devices only: `gather` presses keys for the pad in `ButtonInput`,
/// but never sends keyboard or mouse messages. Also sets
/// [`crate::input::InputDevice`] (which names hints use).
#[allow(clippy::too_many_arguments)]
fn track_last_input(
    pads: Query<&Gamepad>,
    mut keys: MessageReader<KeyboardInput>,
    mut clicks: MessageReader<MouseButtonInput>,
    motion: Res<AccumulatedMouseMotion>,
    touches: Res<Touches>,
    cfg: Res<PadSettings>,
    mut last: ResMut<PadLast>,
    mut device: ResMut<crate::input::InputDevice>,
) {
    use crate::input::{InputDevice, PadFamily};
    let active = pads.iter().find(|p| {
        p.get_just_pressed().next().is_some()
            || p.left_stick().length() > 0.5
            || p.right_stick().length() > 0.5
    });
    let other = keys.read().count() > 0 || clicks.read().count() > 0 || motion.delta.length() > 4.0;
    let want = active.is_some() || (!other && !touches.any_just_pressed() && last.0);
    last.set_if_neq(PadLast(want));
    if forced_device().is_some() {
        return;
    }
    let next = if let Some(p) = active {
        InputDevice::Pad(cfg.labels.unwrap_or(PadFamily::from_vendor(p.vendor_id())))
    } else if touches.any_just_pressed() {
        InputDevice::Touch
    } else if other {
        InputDevice::KeyboardMouse
    } else if let InputDevice::Pad(_) = *device {
        // The label setting changed while on a pad.
        let fam = cfg
            .labels
            .or_else(|| {
                pads.iter()
                    .next()
                    .map(|p| PadFamily::from_vendor(p.vendor_id()))
            })
            .unwrap_or_default();
        InputDevice::Pad(fam)
    } else {
        *device
    };
    if *device != next {
        *device = next;
        crate::input::set_device(next);
    }
}

/// `FRIENDSLOP_INPUT=keyboard|touch|xbox|playstation|nintendo|steam` pins the
/// device hints name (for screenshots).
fn forced_device() -> Option<crate::input::InputDevice> {
    use crate::input::{InputDevice, PadFamily};
    let v = std::env::var("FRIENDSLOP_INPUT").ok()?;
    match v.as_str() {
        "keyboard" => Some(InputDevice::KeyboardMouse),
        "touch" => Some(InputDevice::Touch),
        other => PadFamily::from_name(other).map(InputDevice::Pad),
    }
}

fn force_device(mut device: ResMut<crate::input::InputDevice>) {
    if let Some(d) = forced_device() {
        *device = d;
        crate::input::set_device(d);
    }
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

/// Shapes a look stick: deadzone, then `|v|^curve` keeping direction.
pub fn look_response(v: Vec2, deadzone: f32, curve: f32) -> Vec2 {
    let len = v.length();
    if len <= deadzone || deadzone >= 1.0 {
        return Vec2::ZERO;
    }
    let t = ((len.min(1.0) - deadzone) / (1.0 - deadzone)).powf(curve.max(0.1));
    v / len * t
}

/// Gamepad feel (#361), changed in the settings notebook's Controls page and
/// saved locally like the video options (`pad.txt`).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct PadSettings {
    /// Radial stick deadzone, `0..=MAX_DEADZONE` of full throw.
    pub deadzone: f32,
    /// Look speed, a multiple of the default, `LOOK_MIN..=LOOK_MAX`.
    pub look: f32,
    /// Button names in hints: `None` = from the pad, else this family.
    pub labels: Option<crate::input::PadFamily>,
}

impl PadSettings {
    pub const DEFAULT: Self = Self {
        deadzone: 0.15,
        look: 1.0,
        labels: None,
    };
    pub const MAX_DEADZONE: f32 = 0.4;
    pub const LOOK_MIN: f32 = 0.25;
    pub const LOOK_MAX: f32 = 3.0;
    const FILE: &str = "pad.txt";

    pub fn to_text(self) -> String {
        format!(
            "deadzone={}\nlook={}\nlabels={}\n",
            self.deadzone,
            self.look,
            self.labels.map_or("auto", |f| f.name())
        )
    }

    /// Read [`Self::to_text`]; missing or broken lines keep their defaults.
    pub fn from_text(text: &str) -> Self {
        let mut p = Self::DEFAULT;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            if k.trim() == "labels" {
                p.labels = crate::input::PadFamily::from_name(v.trim());
                continue;
            }
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
            labels: self.labels,
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
    /// The action wheel, while its button is held.
    wheel: Option<Wheel>,
    /// A menu had the pad: ignore A / B / RT until they're let go.
    pad_hold: bool,
}

/// The action wheel's state while it's open.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Wheel {
    pub page: usize,
    /// The slice the right stick points at.
    pub aim: Option<usize>,
}

/// Pure: the slice (0 = top, clockwise) a stick direction points at among
/// `n`, or `None` near the middle.
pub fn wheel_slice(stick: Vec2, n: usize) -> Option<usize> {
    if n == 0 || stick.length() < 0.5 {
        return None;
    }
    // Clockwise from straight up (stick y is up).
    let a = stick.x.atan2(stick.y).rem_euclid(std::f32::consts::TAU);
    let step = std::f32::consts::TAU / n as f32;
    Some(((a + step / 2.0) / step) as usize % n)
}

/// Every pad button the bindings may name.
const PAD_BUTTONS: [GamepadButton; 16] = [
    GamepadButton::South,
    GamepadButton::East,
    GamepadButton::West,
    GamepadButton::North,
    GamepadButton::LeftTrigger,
    GamepadButton::RightTrigger,
    GamepadButton::LeftTrigger2,
    GamepadButton::RightTrigger2,
    GamepadButton::Select,
    GamepadButton::Start,
    GamepadButton::LeftThumb,
    GamepadButton::RightThumb,
    GamepadButton::DPadUp,
    GamepadButton::DPadDown,
    GamepadButton::DPadLeft,
    GamepadButton::DPadRight,
];

#[derive(Component)]
struct WheelRoot;

#[derive(Component)]
struct WheelSlot(usize);

#[derive(Component)]
struct WheelTitle;

#[derive(Component)]
struct PromptBar;

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
        .add_systems(Startup, force_device)
        .add_systems(PreUpdate, track_last_input.after(InputSystems))
        .add_systems(
            OnEnter(AppState::InGame),
            (spawn_overlay, spawn_wheel, spawn_prompt_bar),
        )
        .add_systems(PreUpdate, gather.after(InputSystems))
        .add_systems(
            Update,
            (draw_overlay, draw_wheel, draw_prompt_bar).run_if(in_state(AppState::InGame)),
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
    mut contexts: ResMut<crate::input::InputContexts>,
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
    crate::input::switch(&mut contexts, "menu", ui);
    let binds = crate::input::BINDINGS.get();
    let wheel_buttons = binds.pad_for("wheel", &contexts.0);
    let was_wheel = st.wheel;
    let mut wheel_now: Option<Wheel> = None;
    for pad in &gamepads {
        if ui {
            for (button, key) in [
                (GamepadButton::Select, KeyCode::Tab),
                (GamepadButton::Start, KeyCode::Escape),
                (GamepadButton::East, KeyCode::Escape),
            ] {
                if pad.pressed(button) {
                    want_keys.push(key);
                }
            }
            continue;
        }
        let move_stick = axis(pad.left_stick(), pad_cfg.deadzone);
        if move_stick != Vec2::ZERO {
            out.stick = move_stick;
        }
        // The wheel: hold its button, point the right stick, let go.
        if wheel_buttons.iter().any(|b| pad.pressed(*b)) {
            let mut w = was_wheel.unwrap_or_default();
            if pad.just_pressed(GamepadButton::LeftTrigger) && !binds.wheel.is_empty() {
                w.page = (w.page + 1) % binds.wheel.len();
                w.aim = None;
            }
            let n = binds.wheel.get(w.page).map_or(0, Vec::len);
            if let Some(a) = wheel_slice(pad.right_stick(), n) {
                w.aim = Some(a);
            } else if pad.right_stick().length() < 0.2 && pad.just_pressed(GamepadButton::East) {
                w.aim = None;
            }
            wheel_now = Some(w);
            continue;
        }
        let look = look_response(pad.right_stick(), pad_cfg.deadzone, tune.look_curve);
        out.look +=
            Vec2::new(look.x, -look.y * tune.look_vertical) * tune.look_speed * pad_cfg.look * dt;
        for button in PAD_BUTTONS {
            if !pad.pressed(button) {
                continue;
            }
            let held_over = st.pad_hold
                && matches!(
                    button,
                    GamepadButton::South | GamepadButton::East | GamepadButton::RightTrigger2
                );
            if held_over {
                continue;
            }
            match binds.pad_action(button, &contexts.0) {
                Some("jump") => out.jump = true,
                Some("run") => out.run = true,
                Some("use") => want_mouse = true,
                Some("wheel") | None => {}
                Some(id) => match binds.action(id).and_then(|a| a.keys.first()) {
                    Some(k) if k == "Mouse:Left" => want_mouse = true,
                    Some(k) => want_keys.extend(crate::input::key_code(k)),
                    None => {}
                },
            }
        }
    }
    // Letting go of the wheel fires what it pointed at (one frame's press).
    if wheel_now.is_none()
        && let Some(w) = was_wheel
        && let Some(i) = w.aim
        && let Some(id) = binds.wheel.get(w.page).and_then(|p| p.get(i))
        && let Some(k) = binds.keys_for(id).first()
    {
        want_keys.push(*k);
    }
    st.wheel = wheel_now;

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

/// Radius of the wheel's ring of labels (px).
const WHEEL_R: f32 = 150.0;

fn spawn_wheel(mut commands: Commands) {
    commands
        .spawn((
            Name::new("Action wheel"),
            WheelRoot,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
            Visibility::Hidden,
            GlobalZIndex(60),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|ui| {
            ui.spawn((
                Node {
                    width: px(WHEEL_R * 2.0 + 160.0),
                    height: px(WHEEL_R * 2.0 + 60.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    border_radius: BorderRadius::all(px(WHEEL_R + 60.0)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.45)),
                Pickable::IGNORE,
            ))
            .with_children(|ring| {
                ring.spawn((
                    WheelTitle,
                    Text::new(""),
                    TextFont::from_font_size(15.0),
                    TextColor(LABEL),
                    Pickable::IGNORE,
                ));
                for i in 0..8 {
                    let a = i as f32 / 8.0 * std::f32::consts::TAU;
                    let (x, y) = (a.sin() * WHEEL_R, -a.cos() * WHEEL_R * 0.8);
                    ring.spawn((
                        WheelSlot(i),
                        Node {
                            position_type: PositionType::Absolute,
                            left: px(WHEEL_R + 80.0 + x - 70.0),
                            top: px(WHEEL_R + 30.0 + y - 14.0),
                            width: px(140.0),
                            justify_content: JustifyContent::Center,
                            padding: UiRect::axes(px(8), px(4)),
                            border_radius: BorderRadius::all(px(10)),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                        Pickable::IGNORE,
                        children![(
                            Text::new(""),
                            TextFont::from_font_size(16.0),
                            TextColor(LABEL),
                            TextLayout::justify(Justify::Center),
                            Pickable::IGNORE,
                        )],
                    ));
                }
            });
        });
}

/// Shows the wheel's page while it's open, the aimed slice lit.
fn draw_wheel(
    st: Res<TouchState>,
    device: Res<crate::input::InputDevice>,
    mut root: Query<&mut Visibility, With<WheelRoot>>,
    mut slots: Query<(&WheelSlot, &mut BackgroundColor, &Children)>,
    mut title: Query<&mut Text, With<WheelTitle>>,
    mut texts: Query<&mut Text, Without<WheelTitle>>,
) {
    let want = if st.wheel.is_some() {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut v in &mut root {
        v.set_if_neq(want);
    }
    let Some(w) = st.wheel else { return };
    let b = crate::input::BINDINGS.get();
    let page = b.wheel.get(w.page).cloned().unwrap_or_default();
    for (slot, mut bg, children) in &mut slots {
        let label = page
            .get(slot.0)
            .and_then(|id| b.action(id))
            .map_or(String::new(), |a| a.label.clone());
        let lit = w.aim == Some(slot.0) && !label.is_empty();
        let c = if lit { DOT_HELD } else { Color::NONE };
        if bg.0 != c {
            bg.0 = c;
        }
        for c in children {
            if let Ok(mut t) = texts.get_mut(*c)
                && t.0 != label
            {
                t.0.clone_from(&label);
            }
        }
    }
    let flip = match *device {
        crate::input::InputDevice::Pad(f) => {
            crate::input::button_label(GamepadButton::LeftTrigger, f)
        }
        _ => "LB",
    };
    let want = if b.wheel.len() > 1 {
        format!("page {}/{}\n{flip}: next page", w.page + 1, b.wheel.len())
    } else {
        String::new()
    };
    for mut t in &mut title {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

fn spawn_prompt_bar(mut commands: Commands) {
    commands.spawn((
        Name::new("Prompt bar"),
        PromptBar,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(130),
            width: percent(100),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Pickable::IGNORE,
        Visibility::Hidden,
        GlobalZIndex(55),
        DespawnOnExit(AppState::InGame),
        children![(
            Text::new(""),
            TextFont::from_font_size(18.0),
            TextColor(LABEL),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            Node {
                padding: UiRect::axes(px(14), px(6)),
                border_radius: BorderRadius::all(px(10)),
                ..default()
            },
            Pickable::IGNORE,
        )],
    ));
}

/// Pure: the prompt bar's text for the top context ("A accept   B decline").
pub fn prompt_line(
    b: &crate::input::Bindings,
    device: crate::input::InputDevice,
    contexts: &[String],
) -> String {
    let Some(top) = contexts.last().and_then(|c| b.context(c)) else {
        return String::new();
    };
    top.prompt
        .iter()
        .filter_map(|id| {
            b.action(id).map(|a| {
                format!(
                    "{}  {}",
                    crate::input::label_with(b, id, device, contexts),
                    a.label
                )
            })
        })
        .collect::<Vec<_>>()
        .join("     ")
}

fn draw_prompt_bar(
    contexts: Res<crate::input::InputContexts>,
    device: Res<crate::input::InputDevice>,
    mut bar: Query<(&mut Visibility, &Children), With<PromptBar>>,
    mut text: Query<&mut Text>,
) {
    let Ok((mut vis, children)) = bar.single_mut() else {
        return;
    };
    let line = prompt_line(&crate::input::BINDINGS.get(), *device, &contexts.0);
    vis.set_if_neq(if line.is_empty() {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    for c in children {
        if let Ok(mut t) = text.get_mut(*c)
            && t.0 != line
        {
            t.0.clone_from(&line);
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
            labels: Some(crate::input::PadFamily::PlayStation),
        };
        assert_eq!(PadSettings::from_text(&p.to_text()), p);
        assert_eq!(PadSettings::from_text("junk"), PadSettings::DEFAULT);
        let wild = PadSettings::from_text("deadzone=9\nlook=-1\n");
        assert_eq!(wild.deadzone, PadSettings::MAX_DEADZONE);
        assert_eq!(wild.look, PadSettings::LOOK_MIN);
        assert_eq!(PadSettings::from_text("look=NaN").look, 1.0);
    }

    #[test]
    fn wheel_slices_go_clockwise_from_the_top() {
        assert_eq!(wheel_slice(Vec2::new(0.0, 1.0), 8), Some(0));
        assert_eq!(wheel_slice(Vec2::new(1.0, 0.0), 8), Some(2));
        assert_eq!(wheel_slice(Vec2::new(0.0, -1.0), 8), Some(4));
        assert_eq!(wheel_slice(Vec2::new(-1.0, 0.0), 8), Some(6));
        assert_eq!(wheel_slice(Vec2::new(-0.7, 0.7), 8), Some(7));
        assert_eq!(wheel_slice(Vec2::new(0.1, 0.1), 8), None);
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
