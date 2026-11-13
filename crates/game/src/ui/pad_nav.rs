//! Menus by gamepad (#361, Steam Deck): the D-pad or left stick moves a
//! highlighted focus between the buttons on screen, A clicks the focused one
//! and B backs out (it presses Esc, see `controls::gather`).
//!
//! Buttons opt in with [`PadFocusable`]. Only the top layer is reachable: the
//! focusables under the highest `GlobalZIndex` that are shown, so the settings
//! notebook or the join-code picker take the pad while they're open. A focused
//! [`PadSlider`] takes left/right as [`PadNudge`] instead of moving focus.

use std::time::Duration;

use bevy::camera::NormalizedRenderTarget;
use bevy::input::gamepad::{Gamepad, GamepadButton};
use bevy::picking::backend::HitData;
use bevy::picking::events::Pointer;
use bevy::picking::pointer::{Location, PointerButton, PointerId};
use bevy::prelude::*;

use crate::AppState;

/// A button the pad can focus and click.
#[derive(Component, Clone, Copy, Default, Debug)]
pub struct PadFocusable;

/// A focusable slider: left / right nudge it ([`PadNudge`]) instead of moving focus.
#[derive(Component, Clone, Copy, Default, Debug)]
pub struct PadSlider;

/// Left (`steps < 0`) or right on a focused [`PadSlider`].
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct PadNudge {
    pub entity: Entity,
    pub steps: i32,
}

/// What the pad has focused, and whether the UI owns the pad right now.
#[derive(Resource, Default, Debug)]
pub struct PadFocus {
    pub entity: Option<Entity>,
    /// Set for the frame in which A clicked: click observers that act
    /// differently for a pad (the join-code field opens the picker) read it.
    pub pad_click: bool,
}

/// Seconds before a held direction repeats, then between repeats.
const REPEAT_FIRST: f32 = 0.35;
const REPEAT_NEXT: f32 = 0.12;
/// How far the left stick must lean to count as a direction.
const STICK_DIR: f32 = 0.5;

const FOCUS_RING: Color = Color::srgb_u8(0xFF, 0xD2, 0x4A);

pub fn plugin(app: &mut App) {
    app.init_resource::<PadFocus>()
        .add_systems(Update, (navigate, ring).chain());
}

/// Does the pad drive the UI rather than the game? In the menu, and in game
/// while the settings notebook is open.
pub fn ui_owns_pad(state: &AppState, overlay: &crate::ui::Overlay) -> bool {
    *state != AppState::InGame || overlay.settings
}

/// The held direction (x right, y down: screen order), D-pad first.
pub fn held_dir(pad: &Gamepad) -> IVec2 {
    let d = IVec2::new(
        i32::from(pad.pressed(GamepadButton::DPadRight))
            - i32::from(pad.pressed(GamepadButton::DPadLeft)),
        i32::from(pad.pressed(GamepadButton::DPadDown))
            - i32::from(pad.pressed(GamepadButton::DPadUp)),
    );
    if d != IVec2::ZERO {
        return d;
    }
    let s = pad.left_stick();
    if s.length() < STICK_DIR {
        IVec2::ZERO
    } else if s.x.abs() > s.y.abs() {
        IVec2::new(s.x.signum() as i32, 0)
    } else {
        IVec2::new(0, -s.y.signum() as i32)
    }
}

/// The focusable nearest `from` in direction `dir` (screen space, y down),
/// favouring ones straight ahead over ones off to the side.
pub fn next(from: Vec2, dir: IVec2, items: &[(Entity, Vec2)]) -> Option<Entity> {
    let dir = dir.as_vec2().normalize_or_zero();
    if dir == Vec2::ZERO {
        return None;
    }
    let side = dir.perp();
    items
        .iter()
        .filter_map(|&(e, at)| {
            let d = at - from;
            let ahead = d.dot(dir);
            (ahead > 1.0).then(|| (e, ahead + 2.0 * d.dot(side).abs()))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _)| e)
}

/// The first focusable on screen: top-most, then left-most.
pub fn first(items: &[(Entity, Vec2)]) -> Option<Entity> {
    items
        .iter()
        .min_by(|a, b| {
            (a.1.y.round(), a.1.x)
                .partial_cmp(&(b.1.y.round(), b.1.x))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|&(e, _)| e)
}

#[derive(Default)]
struct Repeat {
    dir: IVec2,
    wait: f32,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn navigate(
    mut commands: Commands,
    pads: Query<&Gamepad>,
    time: Res<Time>,
    state: Res<State<AppState>>,
    overlay: Res<crate::ui::Overlay>,
    mut focus: ResMut<PadFocus>,
    mut repeat: Local<Repeat>,
    items: Query<(Entity, &ComputedNode, &UiGlobalTransform, Has<PadSlider>), With<PadFocusable>>,
    up: Query<(Option<&ChildOf>, &Node, Option<&GlobalZIndex>)>,
) {
    focus.pad_click = false;
    if pads.is_empty() || !ui_owns_pad(state.get(), &overlay) {
        focus.entity = None;
        *repeat = Repeat::default();
        return;
    }
    // Shown focusables, by layer: (entity, centre, layer, slider).
    let mut shown: Vec<(Entity, Vec2, i32, bool)> = Vec::new();
    for (e, node, tf, slider) in &items {
        if node.size().min_element() <= 0.0 {
            continue;
        }
        let (mut at, mut layer, mut hidden) = (Some(e), 0, false);
        while let Some(cur) = at {
            let Ok((parent, n, z)) = up.get(cur) else {
                break;
            };
            hidden |= n.display == Display::None;
            if let Some(z) = z {
                layer = layer.max(z.0);
            }
            at = parent.map(ChildOf::parent);
        }
        if !hidden {
            shown.push((e, tf.translation, layer, slider));
        }
    }
    let top = shown.iter().map(|s| s.2).max().unwrap_or(0);
    let list: Vec<(Entity, Vec2)> = shown
        .iter()
        .filter(|s| s.2 == top)
        .map(|s| (s.0, s.1))
        .collect();
    let here = focus
        .entity
        .and_then(|f| shown.iter().find(|s| s.0 == f && s.2 == top));
    if here.is_none() {
        focus.entity = first(&list);
    }

    // A direction fires when pressed, then repeats while held.
    let dir = pads
        .iter()
        .map(held_dir)
        .find(|d| *d != IVec2::ZERO)
        .unwrap_or_default();
    let fire = if dir == IVec2::ZERO {
        false
    } else if dir != repeat.dir {
        repeat.wait = REPEAT_FIRST;
        true
    } else {
        repeat.wait -= time.delta_secs();
        if repeat.wait <= 0.0 {
            repeat.wait = REPEAT_NEXT;
            true
        } else {
            false
        }
    };
    repeat.dir = dir;
    if fire && let Some(&(e, at, _, slider)) = here {
        if slider && dir.x != 0 {
            commands.trigger(PadNudge {
                entity: e,
                steps: dir.x,
            });
        } else if let Some(n) = next(at, dir, &list) {
            focus.entity = Some(n);
        }
    }

    let a = pads.iter().any(|p| p.just_pressed(GamepadButton::South));
    if a && let Some(&(e, at, _, _)) = here {
        focus.pad_click = true;
        commands.trigger(PointerClick {
            entity: e,
            pointer: Pointer::new(
                PointerId::Mouse,
                Location {
                    target: NormalizedRenderTarget::None {
                        width: 1,
                        height: 1,
                    },
                    position: at,
                },
            ),
            button: PointerButton::Primary,
            hit: HitData::new(e, 0.0, None, None),
            duration: Duration::ZERO,
            count: 1,
        });
    }
}

/// Outline the focused button.
fn ring(
    mut commands: Commands,
    focus: Res<PadFocus>,
    mut last: Local<Option<Entity>>,
    outlined: Query<(), With<Outline>>,
) {
    if *last == focus.entity {
        return;
    }
    if let Some(old) = *last
        && outlined.contains(old)
    {
        commands.entity(old).try_remove::<Outline>();
    }
    if let Some(new) = focus.entity {
        commands
            .entity(new)
            .try_insert(Outline::new(px(3), px(3), FOCUS_RING));
    }
    *last = focus.entity;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_moves_to_the_nearest_button_that_way() {
        let e = |i| Entity::from_raw_u32(i).unwrap_or(Entity::PLACEHOLDER);
        // Two buttons stacked on the left, one off to the right.
        let items = [
            (e(1), Vec2::new(100.0, 100.0)),
            (e(2), Vec2::new(100.0, 200.0)),
            (e(3), Vec2::new(400.0, 150.0)),
        ];
        assert_eq!(first(&items), Some(e(1)));
        assert_eq!(next(items[0].1, IVec2::new(0, 1), &items), Some(e(2)));
        assert_eq!(next(items[1].1, IVec2::new(0, -1), &items), Some(e(1)));
        assert_eq!(next(items[0].1, IVec2::new(1, 0), &items), Some(e(3)));
        // Nothing further up: focus stays.
        assert_eq!(next(items[0].1, IVec2::new(0, -1), &items), None);
    }
}
