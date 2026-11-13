//! ContextPrompt: under the aim point, say what clicking does. In reach it's
//! a card with a hand, the verb ("Pet the cat") and "LMB · in reach" ("B / RT" after
//! a gamepad was used), and
//! the crosshair turns into a hand; out of reach it's a small "Too far · move
//! closer" chip instead (never both).

use bevy::prelude::*;

use super::{HudRoot, Icons, at, body, glyph, head};
use crate::AppState;
use crate::features::fps::WalkCrosshair;
use crate::features::interact::{AimHover, verb};
use crate::theme::indigo::{BORDER, CREAM, INK, PERIWINKLE};

/// Gap between the aim point and the top of the prompt.
const BELOW: f32 = 26.0;
const READY_W: f32 = 248.0;

/// The prompt's text: the verb card, or the too-far line. The binding names
/// the gamepad's buttons when a pad was the last thing used (#361).
pub fn prompt(label: Option<&str>, in_reach: bool, pad: bool) -> (String, String) {
    if in_reach {
        let how = if pad { "B / RT" } else { "LMB" };
        (verb(label), format!("{how}  ·  in reach"))
    } else {
        ("Too far · move closer".to_string(), String::new())
    }
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Card {
    Ready,
    TooFar,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Verb,
    Binding,
    Far,
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            spawn.run_if(not(any_with_component::<Card>)),
            (update, crosshair),
        )
            .chain()
            .run_if(in_state(AppState::InGame)),
    );
}

fn spawn(mut commands: Commands, icons: Res<Icons>, root: Query<Entity, With<HudRoot>>) {
    let Ok(root) = root.single() else { return };
    let b = 2.0;
    commands.entity(root).with_children(|c| {
        c.spawn((
            Card::Ready,
            Node {
                position_type: PositionType::Absolute,
                min_width: px(READY_W),
                height: px(91),
                padding: UiRect::right(px(20)),
                border: UiRect::all(px(b)),
                border_radius: BorderRadius::all(px(18)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(INK),
            BorderColor::all(BORDER),
            GlobalZIndex(30),
            children![
                glyph(&icons.hand, 30.0, 16.0, 15.0, b, CREAM),
                // In flow (not absolute) so a long verb widens the card.
                (
                    Part::Verb,
                    Text::new(""),
                    head(25.0, CREAM),
                    TextLayout::no_wrap(),
                    Node {
                        margin: UiRect {
                            left: px(60.0 - b),
                            top: px(8.0 - b),
                            ..default()
                        },
                        ..default()
                    },
                ),
                (
                    Part::Binding,
                    Text::new(""),
                    body(15.0, PERIWINKLE),
                    at(60.0, 46.0, b)
                ),
            ],
        ));
        c.spawn((
            Card::TooFar,
            Node {
                position_type: PositionType::Absolute,
                height: px(44),
                padding: UiRect::axes(px(10.0 - b), px(10.0 - b)),
                border: UiRect::all(px(b)),
                border_radius: BorderRadius::all(px(12)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(INK),
            BorderColor::all(INK),
            GlobalZIndex(30),
            children![(
                Part::Far,
                Text::new(""),
                body(15.0, CREAM),
                TextLayout::no_wrap()
            )],
        ));
    });
}

fn update(
    hover: Res<AimHover>,
    scale: Res<UiScale>,
    pad: Res<crate::controls::PadLast>,
    mut cards: Query<(&Card, &mut Node, &ComputedNode)>,
    mut parts: Query<(&Part, &mut Text)>,
) {
    let aim = hover.0;
    let lines = aim.map(|a| prompt(a.label, a.in_reach, pad.0));
    for (card, mut node, computed) in &mut cards {
        let show = aim.is_some_and(|a| a.in_reach == (*card == Card::Ready));
        let d = if show { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
        let Some(a) = aim.filter(|_| show) else {
            continue;
        };
        // The aim point is in window pixels; UI px are scaled by UiScale.
        let s = scale.0.max(1e-3);
        let w = computed.size().x * computed.inverse_scale_factor() / s;
        let w = if w > 1.0 { w } else { READY_W };
        let left = px(a.at.x / s - w / 2.0);
        let top = px(a.at.y / s + BELOW);
        if node.left != left {
            node.left = left;
        }
        if node.top != top {
            node.top = top;
        }
    }
    let Some((title, binding)) = lines else {
        return;
    };
    for (part, mut text) in &mut parts {
        let want = match part {
            Part::Verb | Part::Far => &title,
            Part::Binding => &binding,
        };
        if &text.0 != want {
            text.0.clone_from(want);
        }
    }
}

/// Crosshair glyph: a hand (32 px) when what you aim at is in reach.
fn crosshair(
    hover: Res<AimHover>,
    icons: Res<Icons>,
    mut cross: Query<(&mut ImageNode, &mut Node), With<WalkCrosshair>>,
) {
    let hand = hover.0.is_some_and(|a| a.in_reach);
    let (image, size) = if hand {
        (&icons.hand, 32.0)
    } else {
        (&icons.target, 24.0)
    };
    for (mut img, mut node) in &mut cross {
        if img.image != *image {
            img.image = image.clone();
        }
        if node.width != px(size) {
            node.width = px(size);
            node.height = px(size);
            node.margin = UiRect::all(px(-size / 2.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_reach_names_the_verb_and_too_far_never_claims_reach() {
        assert_eq!(
            prompt(Some("pet the cat"), true, false),
            ("Pet the cat".to_string(), "LMB  ·  in reach".to_string())
        );
        assert_eq!(
            prompt(Some("pet the cat"), true, true).1,
            "B / RT  ·  in reach"
        );
        let (far, line) = prompt(Some("pet the cat"), false, true);
        assert_eq!(far, "Too far · move closer");
        assert!(line.is_empty());
    }
}
