//! DangerLane: timers and threats, centred under the team strip (top 100).
//! The most urgent one is a big 340x78 card, up to two more are compact
//! chips under it. Each has a glyph, a title and an `MM:SS` countdown (when
//! the game has one), not just colour. The fat cat's fuse is not here: it
//! has its own slot in the status stack.

use bevy::prelude::*;

use super::{Icons, at, body, glyph, head, mmss};
use crate::AppState;
use crate::features::caught::Caught;
use crate::features::helicopter::Helicopter;
use crate::features::landlord::{Act3, Tenancy};
use crate::features::punishment::AttackMode;
use crate::features::stillness::Stillness;
use crate::player::LocalPlayer;
use crate::theme::indigo::{BORDER, BUTTER, CREAM, INK};
pub use friendslop_base::ui::DangerMount;

/// Most warnings shown at once (no second HUD wall).
pub const MAX_WARNINGS: usize = 3;
/// The rent raise only warns in its last minute.
pub const RENT_WARN_SECS: f32 = 60.0;
const W: f32 = 340.0;

/// One threat.
#[derive(Clone, Debug, PartialEq)]
pub struct Warning {
    pub title: String,
    /// What to do, or whose problem it is.
    pub note: String,
    pub secs: Option<f32>,
    /// Higher goes first.
    pub urgency: u8,
    /// About you (butter), or about someone else (ink).
    pub mine: bool,
}

/// Everything the game says is dangerous right now, most urgent first,
/// capped at [`MAX_WARNINGS`].
pub fn warnings(
    me: Option<u64>,
    attacks: &[AttackMode],
    still: Option<&Stillness>,
    tenancy: Option<&Tenancy>,
    helis: &[u64],
    caught: &[Caught],
) -> Vec<Warning> {
    let mut out = Vec::new();
    for a in attacks {
        let mine = Some(a.target) == me;
        out.push(Warning {
            title: "ATTACK MODE".into(),
            note: if mine {
                "keep running!".into()
            } else {
                format!("they're after player {}", a.target)
            },
            secs: Some(a.secs_left as f32),
            urgency: if mine { 100 } else { 40 },
            mine,
        });
    }
    for c in caught {
        let held = me.is_some_and(|m| c.held.contains(&m));
        if c.accomplice.is_none() && (held || c.lift > 0.0) {
            out.push(Warning {
                title: "FLOATED".into(),
                note: "locked in the air".into(),
                secs: Some(c.secs_left as f32),
                urgency: if held { 95 } else { 35 },
                mine: held,
            });
        } else if held {
            out.push(Warning {
                title: "CARRIED".into(),
                note: "you're being carried off".into(),
                secs: None,
                urgency: 95,
                mine: true,
            });
        } else if me.is_some() && c.accomplice == me {
            out.push(Warning {
                title: "ACCOMPLICE".into(),
                note: "the poop police is coming".into(),
                secs: None,
                urgency: 85,
                mine: true,
            });
        }
    }
    for &target in helis {
        let mine = Some(target) == me;
        out.push(Warning {
            title: "HELICOPTER".into(),
            note: if mine {
                "be a better person!".into()
            } else {
                format!("hunting player {target}")
            },
            secs: None,
            urgency: if mine { 90 } else { 30 },
            mine,
        });
    }
    if let Some(s) = still.filter(|s| s.frozen) {
        out.push(Warning {
            title: "THE STILLNESS".into(),
            note: "socialize, and stare with passion".into(),
            secs: Some(s.secs_left as f32),
            urgency: 70,
            mine: false,
        });
    }
    if let Some(t) = tenancy.filter(|t| t.act == Act3::Legal && t.next_raise <= RENT_WARN_SECS) {
        out.push(Warning {
            title: "RENT RAISE".into(),
            note: "pay the landlord".into(),
            secs: Some(t.next_raise),
            urgency: 50,
            mine: false,
        });
    }
    out.sort_by_key(|w| std::cmp::Reverse(w.urgency));
    out.truncate(MAX_WARNINGS);
    out
}

#[derive(Component, Clone, Copy)]
struct Slot(usize);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Icon,
    Title,
    Line,
    Time,
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (fill_mount, update)
            .chain()
            .run_if(in_state(AppState::InGame)),
    );
}

fn fill_mount(
    mut commands: Commands,
    icons: Res<Icons>,
    mounts: Query<Entity, Added<DangerMount>>,
) {
    for mount in &mounts {
        commands.entity(mount).insert(Node {
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(8),
            ..default()
        });
        commands.entity(mount).with_children(|c| {
            c.spawn(big(&icons));
            for i in 1..MAX_WARNINGS {
                c.spawn(chip(&icons, i));
            }
        });
    }
}

/// The lead card (Figma "Attack countdown", 340x78).
fn big(icons: &Icons) -> impl Bundle {
    let b = 2.0;
    (
        Slot(0),
        Node {
            width: px(W),
            height: px(78),
            border: UiRect::all(px(b)),
            border_radius: BorderRadius::all(px(18)),
            display: Display::None,
            ..default()
        },
        BackgroundColor(BUTTER),
        BorderColor::all(INK),
        children![
            (Part::Icon, glyph(&icons.warn, 32.0, 14.0, 20.0, b, INK)),
            (
                Part::Title,
                Text::new(""),
                head(23.0, INK),
                at(58.0, 7.0, b)
            ),
            (
                Part::Line,
                Text::new(""),
                body(17.0, INK),
                at(58.0, 41.0, b)
            ),
        ],
    )
}

/// A compact timer chip (Figma "Timer / ...", 46 high), led by the warning
/// glyph (a shape, not just a colour).
fn chip(icons: &Icons, i: usize) -> impl Bundle {
    let b = 2.0;
    (
        Slot(i),
        Node {
            width: px(W),
            height: px(46),
            border: UiRect::all(px(b)),
            border_radius: BorderRadius::all(px(12)),
            display: Display::None,
            ..default()
        },
        BackgroundColor(INK),
        BorderColor::all(BORDER),
        children![
            (Part::Icon, glyph(&icons.warn, 20.0, 10.0, 11.0, b, CREAM)),
            (
                Part::Title,
                Text::new(""),
                body(16.0, CREAM),
                at(38.0, 13.0, b)
            ),
            (
                Part::Time,
                Text::new(""),
                head(22.0, CREAM),
                Node {
                    position_type: PositionType::Absolute,
                    right: px(12.0 - b),
                    top: px(10.0 - b),
                    ..default()
                },
            ),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn update(
    me: Option<Res<LocalPlayer>>,
    attacks: Query<&AttackMode>,
    still: Query<&Stillness>,
    tenancy: Query<&Tenancy>,
    helis: Query<&Helicopter>,
    caught: Query<&Caught>,
    mut slots: Query<(
        &Slot,
        &Children,
        &mut Node,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut parts: Query<
        (
            &Part,
            Option<&mut Text>,
            Option<&mut TextColor>,
            Option<&mut ImageNode>,
        ),
        Without<Slot>,
    >,
) {
    let attacks: Vec<AttackMode> = attacks.iter().copied().collect();
    let helis: Vec<u64> = helis.iter().map(|h| h.target).collect();
    let caught: Vec<Caught> = caught.iter().cloned().collect();
    let list = warnings(
        me.map(|m| m.0),
        &attacks,
        still.iter().next(),
        tenancy.iter().next(),
        &helis,
        &caught,
    );
    for (slot, kids, mut node, mut bg, mut border) in &mut slots {
        let w = list.get(slot.0);
        let d = if w.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != d {
            node.display = d;
        }
        let Some(w) = w else { continue };
        let lead = slot.0 == 0;
        // The lead card is always butter; a chip turns butter when it's yours.
        let (fill, line, ink) = if lead || w.mine {
            (BUTTER, INK, INK)
        } else {
            (INK, BORDER, CREAM)
        };
        bg.set_if_neq(BackgroundColor(fill));
        border.set_if_neq(BorderColor::all(line));
        let time = w.secs.map(mmss);
        for kid in kids.iter() {
            let Ok((part, text, color, image)) = parts.get_mut(kid) else {
                continue;
            };
            if let Some(mut color) = color {
                color.set_if_neq(TextColor(ink));
            }
            if let Some(mut image) = image
                && image.color != ink
            {
                image.color = ink;
            }
            let want = match part {
                Part::Title => w.title.clone(),
                Part::Line => match &time {
                    Some(t) => format!("{t}  ·  {}", w.note),
                    None => w.note.clone(),
                },
                Part::Time => time.clone().unwrap_or_default(),
                Part::Icon => continue,
            };
            if let Some(mut text) = text
                && text.0 != want
            {
                text.0 = want;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn your_attack_beats_the_stillness_and_rent_waits_for_its_last_minute() {
        let attack = AttackMode {
            target: 1,
            secs_left: 52,
        };
        let still = Stillness {
            frozen: true,
            secs_left: 20,
        };
        let list = warnings(Some(1), &[attack], Some(&still), None, &[], &[]);
        assert_eq!(list[0].title, "ATTACK MODE");
        assert!(list[0].mine);
        assert_eq!(list[1].title, "THE STILLNESS");
        let calm = Stillness {
            frozen: false,
            secs_left: 100,
        };
        assert!(warnings(Some(1), &[], Some(&calm), None, &[], &[]).is_empty());
        let others = warnings(Some(2), &[attack], None, None, &[1, 3, 4], &[]);
        assert_eq!(others.len(), MAX_WARNINGS);
        assert!(others.iter().all(|w| !w.mine));
    }
}
