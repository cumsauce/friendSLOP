//! Gamepads on Android (#354). gilrs has no Android backend, so Bevy sees no
//! pads there. winit still hands us what Android sends: pad buttons arrive as
//! key presses with Android key codes (`KEYCODE_BUTTON_*`), and the left
//! stick as touch moves with no touch start, its position the stick's axes
//! (winit forwards a joystick `MotionEvent`'s `AXIS_X`/`AXIS_Y` as a pointer).
//! This module turns those into one Bevy gamepad, so the controls and the
//! button hints work as on desktop. The d-pad already arrives as arrow keys.
//!
//! Not covered (winit drops them): the right stick, analog triggers and
//! the pad's name and vendor, so hints default to the Xbox layout (Android's
//! own button naming) until the player picks labels in Controls.

use bevy::input::ButtonState;
use bevy::input::gamepad::{
    GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent,
    RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent,
};
use bevy::input::keyboard::{KeyCode, KeyboardInput, NativeKeyCode};
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;

/// The Bevy button for an Android key code, if it is a pad button.
pub fn pad_button(code: u32) -> Option<GamepadButton> {
    // android/keycodes.h: KEYCODE_BUTTON_A = 96 .. KEYCODE_BUTTON_MODE = 110.
    Some(match code {
        96 => GamepadButton::South,
        97 => GamepadButton::East,
        98 => GamepadButton::C,
        99 => GamepadButton::West,
        100 => GamepadButton::North,
        101 => GamepadButton::Z,
        102 => GamepadButton::LeftTrigger,
        103 => GamepadButton::RightTrigger,
        104 => GamepadButton::LeftTrigger2,
        105 => GamepadButton::RightTrigger2,
        106 => GamepadButton::LeftThumb,
        107 => GamepadButton::RightThumb,
        108 => GamepadButton::Start,
        109 => GamepadButton::Select,
        110 => GamepadButton::Mode,
        _ => return None,
    })
}

/// The pad button a key event stands for (Android key codes only).
pub fn key_pad_button(key: &KeyboardInput) -> Option<GamepadButton> {
    match key.key_code {
        KeyCode::Unidentified(NativeKeyCode::Android(code)) => pad_button(code),
        _ => None,
    }
}

/// A touch move that is really the left stick: no touch began for it, and
/// it sits inside the stick's unit square once scaled back to device pixels.
pub fn stick_from_touch(position: Vec2, scale: f32, began: bool) -> Option<Vec2> {
    if began {
        return None;
    }
    let raw = position * scale;
    (raw.x.abs() <= 1.0 && raw.y.abs() <= 1.0).then(|| Vec2::new(raw.x, -raw.y))
}

/// The one pad, spawned on its first input.
#[derive(Resource, Default)]
struct AndroidPad {
    entity: Option<Entity>,
    /// Touch ids that began (real fingers).
    fingers: Vec<u64>,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<AndroidPad>()
        .add_systems(PreUpdate, bridge.before(bevy::input::InputSystems));
}

fn bridge(
    mut commands: Commands,
    mut pad: ResMut<AndroidPad>,
    mut keys: MessageReader<KeyboardInput>,
    mut touches: MessageReader<TouchInput>,
    windows: Query<&Window>,
    mut raw: MessageWriter<RawGamepadEvent>,
    mut connections: MessageWriter<GamepadConnectionEvent>,
    mut buttons: MessageWriter<RawGamepadButtonChangedEvent>,
    mut axes: MessageWriter<RawGamepadAxisChangedEvent>,
) {
    let mut entity = |pad: &mut AndroidPad| -> Entity {
        if let Some(e) = pad.entity {
            return e;
        }
        let e = commands.spawn_empty().id();
        pad.entity = Some(e);
        let event = GamepadConnectionEvent::new(
            e,
            GamepadConnection::Connected {
                name: "Android gamepad".into(),
                vendor_id: None,
                product_id: None,
            },
        );
        raw.write(event.clone().into());
        connections.write(event);
        e
    };
    let mut changes: Vec<(Entity, GamepadButton, f32)> = Vec::new();
    for key in keys.read() {
        if key.repeat {
            continue;
        }
        let Some(button) = key_pad_button(key) else {
            continue;
        };
        let value = if key.state == ButtonState::Pressed {
            1.0
        } else {
            0.0
        };
        changes.push((entity(&mut pad), button, value));
    }
    let mut sticks: Vec<(Entity, Vec2)> = Vec::new();
    for t in touches.read() {
        match t.phase {
            TouchPhase::Started => pad.fingers.push(t.id),
            TouchPhase::Ended | TouchPhase::Canceled => pad.fingers.retain(|&id| id != t.id),
            TouchPhase::Moved => {
                let scale = windows.get(t.window).map_or(1.0, |w| w.scale_factor());
                let began = pad.fingers.contains(&t.id);
                if let Some(v) = stick_from_touch(t.position, scale, began) {
                    sticks.push((entity(&mut pad), v));
                }
            }
        }
    }
    for (e, button, value) in changes {
        raw.write(RawGamepadButtonChangedEvent::new(e, button, value).into());
        buttons.write(RawGamepadButtonChangedEvent::new(e, button, value));
    }
    for (e, v) in sticks {
        for (axis, value) in [
            (GamepadAxis::LeftStickX, v.x),
            (GamepadAxis::LeftStickY, v.y),
        ] {
            raw.write(RawGamepadAxisChangedEvent::new(e, axis, value).into());
            axes.write(RawGamepadAxisChangedEvent::new(e, axis, value));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_buttons_map_to_pad_buttons() {
        assert_eq!(pad_button(96), Some(GamepadButton::South));
        assert_eq!(pad_button(108), Some(GamepadButton::Start));
        assert_eq!(pad_button(62), None);
    }

    #[test]
    fn stick_only_from_touches_that_never_began() {
        assert_eq!(
            stick_from_touch(Vec2::new(0.25, 0.25), 2.0, false),
            Some(Vec2::new(0.5, -0.5))
        );
        assert_eq!(stick_from_touch(Vec2::new(0.25, 0.25), 2.0, true), None);
        assert_eq!(stick_from_touch(Vec2::new(300.0, 10.0), 2.0, false), None);
    }
}
