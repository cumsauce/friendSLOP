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
//! D-pad up switches camera, Start frees the cursor / leaves. In the menu, A
//! or Start hosts.

use std::collections::HashMap;

use bevy::input::InputSystems;
use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::input::touch::Touches;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::AppState;
use crate::features::fps::FpsView;
use crate::net::NetCommand;
pub use friendslop_base::controls::*;

/// Radians of look per pixel dragged on a touch screen.
const TOUCH_LOOK: f32 = 0.006;
/// Radians per second of look at full right-stick deflection.
const PAD_LOOK: f32 = 3.2;
/// Joystick throw in logical pixels.
const STICK_RADIUS: f32 = 60.0;
const DEADZONE: f32 = 0.15;

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
    /// Leave the game (Esc, skipping the "free the cursor" step).
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
}

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
        .add_systems(OnEnter(AppState::InGame), spawn_overlay)
        .add_systems(
            PreUpdate,
            (gather, menu_pad.run_if(in_state(AppState::Menu)))
                .chain()
                .after(InputSystems),
        )
        .add_systems(Update, draw_overlay.run_if(in_state(AppState::InGame)));
}

fn axis(v: Vec2) -> Vec2 {
    if v.length() < DEADZONE {
        Vec2::ZERO
    } else {
        v.clamp_length_max(1.0)
    }
}

#[allow(clippy::too_many_arguments)]
fn gather(
    touches: Res<Touches>,
    gamepads: Query<&Gamepad>,
    window: Query<&Window, With<PrimaryWindow>>,
    time: Res<Time>,
    state: Res<State<AppState>>,
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
                out.stick = axis(Vec2::new(d.x, -d.y));
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

    // Gamepads: the first connected one(s) all count.
    let dt = time.delta_secs();
    for pad in &gamepads {
        let move_stick = axis(pad.left_stick());
        if move_stick != Vec2::ZERO {
            out.stick = move_stick;
        }
        let look = axis(pad.right_stick());
        out.look += Vec2::new(look.x, -look.y) * PAD_LOOK * dt;
        out.jump |= pad.pressed(GamepadButton::South);
        out.run |= pad.pressed(GamepadButton::LeftTrigger) || pad.pressed(GamepadButton::LeftThumb);
        want_mouse |= pad.pressed(GamepadButton::East)
            || pad.pressed(GamepadButton::RightTrigger)
            || pad.pressed(GamepadButton::RightTrigger2);
        for (button, key) in [
            (GamepadButton::West, KeyCode::KeyF),
            (GamepadButton::North, KeyCode::KeyH),
            (GamepadButton::Select, KeyCode::Tab),
            (GamepadButton::DPadUp, KeyCode::KeyV),
            (GamepadButton::Start, KeyCode::Escape),
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

/// A gamepad can't point at the menu's buttons: A or Start hosts.
fn menu_pad(gamepads: Query<&Gamepad>, mut net: MessageWriter<NetCommand>) {
    if gamepads
        .iter()
        .any(|p| p.just_pressed(GamepadButton::South) || p.just_pressed(GamepadButton::Start))
    {
        net.write(NetCommand::Host);
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
        assert_eq!(axis(Vec2::new(0.1, 0.0)), Vec2::ZERO);
        assert!((axis(Vec2::new(3.0, 4.0)).length() - 1.0).abs() < 1e-5);
    }
}
