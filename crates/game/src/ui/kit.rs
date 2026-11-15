//! The commune noticeboard kit as spawnable pieces: buttons, panels, cardboard cards with
//! tape, toasts, chips, input glyphs, tabs and meters. Tokens (colours, type scale, sizes)
//! are `theme::kit`; this file only turns them into UI nodes, so a screen is a few calls.
//!
//! Every piece draws from the tokens and nothing else, and every button is pad-reachable
//! unless it is a [`button_off`].

use bevy::picking::hover::Hovered;
use bevy::prelude::*;

use super::pad_nav::PadFocusable;
use super::{Regular, body, head};
use crate::assets::DisplayText;
use crate::theme::Squishy;
use crate::theme::kit::{self as t, size};

/// What a button looks like. One screen has one [`Look::Primary`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// The one thing to press: slime green.
    Primary,
    /// A second choice: cardboard.
    Cardboard,
    /// Everything else: soot.
    Plain,
    /// Leaving or losing something: landlord red.
    Danger,
    /// Not available yet: soot with dim text, skipped by the pad.
    Off,
}

impl Look {
    fn fill(self) -> Color {
        match self {
            Look::Primary => t::SLOP_400,
            Look::Cardboard => t::CARD_300,
            Look::Plain | Look::Off => t::SOOT_700,
            Look::Danger => t::RENT_500,
        }
    }

    /// The shade layer under the button.
    fn lip(self) -> Color {
        match self {
            Look::Primary => t::SLOP_600,
            Look::Cardboard => t::CARD_500,
            Look::Plain | Look::Off => t::SOOT_900,
            Look::Danger => t::RENT_700,
        }
    }

    fn ink(self) -> Color {
        match self {
            Look::Primary | Look::Cardboard => t::INK,
            Look::Plain | Look::Danger => t::TEXT,
            Look::Off => t::TEXT_DIM,
        }
    }
}

/// A button's resting fill; it lightens while hovered.
#[derive(Component, Clone, Copy, Debug)]
pub struct Fill(pub Color);

/// A button: `label` in Atkinson Bold 22, a lip under it, [`t::BUTTON_H`] tall and `width` wide.
/// Add the click with `.observe(..)`. For [`Look::Off`] use [`button_off`], which the pad skips.
pub fn button(look: Look, label: &str, width: Val) -> impl Bundle {
    (button_base(look, label, width), PadFocusable)
}

/// A button that is not available yet: soot, dim text, skipped by the pad.
pub fn button_off(label: &str, width: Val) -> impl Bundle {
    button_base(Look::Off, label, width)
}

fn button_base(look: Look, label: &str, width: Val) -> impl Bundle {
    (
        Node {
            width,
            height: px(t::BUTTON_H),
            padding: UiRect::horizontal(px(20)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::SpaceBetween,
            border_radius: BorderRadius::all(px(t::R_BUTTON)),
            ..default()
        },
        BackgroundColor(look.fill()),
        BoxShadow::new(look.lip(), px(0), px(t::LIP), px(0), px(0)),
        Fill(look.fill()),
        Hovered::default(),
        Squishy::default(),
        UiTransform::default(),
        children![(Text::new(label.to_string()), body(size::BODY, look.ink()))],
    )
}

/// A soot panel: the backdrop of a menu or a settings page.
pub fn panel() -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(t::EDGE)),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(t::R_PANEL)),
            ..default()
        },
        BackgroundColor(t::SOOT_800),
        BorderColor::all(t::SOOT_500),
    )
}

/// A cardboard card: anything you read. Sits a little crooked on a shade layer and is held on
/// by [`tape`] (spawn that as its child). `fill` is [`t::CARD_100`] or [`t::BUTTER_400`] for a
/// sticky note.
pub fn card(fill: Color, tilt_degrees: f32) -> impl Bundle {
    (
        Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(18)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(fill),
        BoxShadow::new(t::CARD_500, px(5), px(6), px(0), px(0)),
        UiTransform::from_rotation(Rot2::degrees(tilt_degrees)),
    )
}

/// A strip of tape across the top edge of its parent card.
pub fn tape() -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            top: px(-10),
            left: percent(38),
            width: px(86),
            height: px(22),
            ..default()
        },
        BackgroundColor(t::TAPE_200.with_alpha(0.9)),
        UiTransform::from_rotation(Rot2::degrees(-3.0)),
    )
}

/// A line in the feed: a coloured bar, the text, and a `×N` when it repeated.
/// `bar` is [`t::RENT_500`] for something bad, [`t::BUTTER_400`] for news.
pub fn toast(text: &str, bar: Color, repeats: u32) -> impl Bundle {
    (
        Node {
            align_items: AlignItems::Center,
            column_gap: px(12),
            padding: UiRect::new(px(0), px(14), px(10), px(10)),
            border: UiRect::left(px(6)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(t::SOOT_700),
        BorderColor::all(bar),
        children![
            (
                Node {
                    padding: UiRect::left(px(12)),
                    ..default()
                },
                Text::new(text.to_string()),
                body(size::BODY_REG, t::TEXT),
                Regular,
            ),
            (
                Text::new(if repeats > 1 {
                    super::times(repeats)
                } else {
                    String::new()
                }),
                head(size::NUM * 0.8, t::BUTTER_400),
            ),
        ],
    )
}

/// A small pill: a stamp-sized title and an optional timer, on `fill` (ink text).
pub fn chip(label: &str, fill: Color) -> impl Bundle {
    (
        Node {
            padding: UiRect::axes(px(16), px(5)),
            border_radius: BorderRadius::all(px(999)),
            ..default()
        },
        BackgroundColor(fill),
        children![(Text::new(label.to_string()), body(size::LABEL, t::INK))],
    )
}

/// The four pad face buttons and the keys we draw glyphs for. Drawn here, no vendor art.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    A,
    B,
    X,
    Y,
}

impl Glyph {
    fn ring(self) -> Color {
        match self {
            Glyph::A => t::SLOP_400,
            Glyph::B => t::RENT_500,
            Glyph::X => t::SKY_300,
            Glyph::Y => t::BUTTER_400,
        }
    }

    fn letter(self) -> &'static str {
        match self {
            Glyph::A => "A",
            Glyph::B => "B",
            Glyph::X => "X",
            Glyph::Y => "Y",
        }
    }
}

/// A round pad button, 28 px.
pub fn glyph(g: Glyph) -> impl Bundle {
    (
        Node {
            width: px(28),
            height: px(28),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::MAX,
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(t::SOOT_900),
        BorderColor::all(g.ring()),
        children![(Text::new(g.letter()), body(size::LABEL, t::TEXT))],
    )
}

/// One entry of a hint bar: a glyph and its verb.
pub fn hint(g: Glyph, verb: &str) -> impl Bundle {
    (
        Node {
            align_items: AlignItems::Center,
            column_gap: px(8),
            ..default()
        },
        children![
            glyph(g),
            (Text::new(verb.to_string()), body(size::LABEL, t::TEXT))
        ],
    )
}

/// A tab: the open one is cardboard, the others soot. Display face, so tabs read as headings.
pub fn tab(label: &str, open: bool) -> impl Bundle {
    (
        Node {
            padding: UiRect::axes(px(18), px(8)),
            border_radius: BorderRadius::top(px(t::R_BUTTON)),
            ..default()
        },
        BackgroundColor(if open { t::CARD_100 } else { t::SOOT_700 }),
        Fill(if open { t::CARD_100 } else { t::SOOT_700 }),
        Hovered::default(),
        PadFocusable,
        children![(
            Text::new(label.to_string()),
            head(size::H2, if open { t::INK } else { t::TEXT_DIM }),
        )],
    )
}

/// A bar: `share` (0 to 1) of the track filled in slime green. Danger bars pass
/// [`t::RENT_500`].
pub fn meter(share: f32, fill: Color) -> impl Bundle {
    (
        Node {
            width: percent(100),
            height: px(12),
            border_radius: BorderRadius::all(px(6)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(t::SOOT_700),
        children![(
            Node {
                width: percent(share.clamp(0.0, 1.0) * 100.0),
                height: percent(100),
                ..default()
            },
            BackgroundColor(fill),
        )],
    )
}

/// Heading text in Bowlby One at `size`.
pub fn display(text: &str, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(text.to_string()),
        TextFont {
            font_size: FontSize::Px(size),
            ..default()
        },
        TextColor(color),
        DisplayText,
    )
}

/// Lightens a hovered button's fill; settles back when the pointer leaves.
fn hover_fill(mut q: Query<(&Hovered, &Fill, &mut BackgroundColor), Changed<Hovered>>) {
    for (hovered, fill, mut bg) in &mut q {
        bg.0 = if hovered.get() {
            fill.0.mix(&Color::WHITE, 0.18)
        } else {
            fill.0
        };
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, hover_fill);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_buttons_are_the_only_ones_the_pad_skips() {
        for look in [Look::Primary, Look::Cardboard, Look::Plain, Look::Danger] {
            assert_ne!(look, Look::Off);
            assert_ne!(look.fill(), look.lip());
        }
        // Dim text on soot, never ink on soot.
        assert_eq!(Look::Off.ink(), t::TEXT_DIM);
    }

    #[test]
    fn glyph_rings_follow_the_pad_colours() {
        let rings = [Glyph::A, Glyph::B, Glyph::X, Glyph::Y].map(Glyph::ring);
        for (i, a) in rings.iter().enumerate() {
            for b in &rings[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }
}
