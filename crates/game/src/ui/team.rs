//! TeamStrip, top-right: one 116x46 PlayerBadge per player (up to six, gap
//! 10, right edge 24 px in): mic state, name, gods' tears ×N, and a butter
//! highlight plus a waveform while they talk. Badges are keyed by player id,
//! so people leaving or joining never shuffle someone else's state.

use bevy::prelude::*;

use super::{HudRoot, Icons, at, body, glyph};
use crate::AppState;
use crate::features::gods_tears::Transformation;
use crate::player::{LocalPlayer, Player};
use crate::theme::indigo::{BORDER, BUTTER, CREAM, EDGE, INK};
use crate::voice::{MicMuted, VoiceMuted};
use crate::voice_chat::{SpeakerId, VoiceActivity};

/// Most badges the strip shows.
pub const MAX_BADGES: usize = 6;
const BADGE: Vec2 = Vec2::new(116.0, 46.0);
const B: f32 = 1.0;

/// What one badge shows. Kept separate (not one string) so each part has its
/// own style and label.
#[derive(Clone, Debug, PartialEq)]
pub struct BadgeState {
    pub id: u64,
    pub name: String,
    pub tears: u32,
    pub muted: bool,
    pub speaking: bool,
    pub local: bool,
}

/// Display name: "You" for this peer, otherwise "Player N" (the game has no
/// player names yet).
pub fn name_for(id: u64, local: bool) -> String {
    if local {
        "You".to_string()
    } else {
        format!("Player {id}")
    }
}

/// Local player first, then by id; at most [`MAX_BADGES`].
pub fn roster(mut badges: Vec<BadgeState>) -> Vec<BadgeState> {
    badges.sort_by_key(|b| (!b.local, b.id));
    badges.truncate(MAX_BADGES);
    badges
}

#[derive(Component)]
struct Strip;

#[derive(Component, Clone, Copy)]
struct Badge(u64);

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Name,
    Tears,
    Mic,
    Slash,
    Tear,
    Wave,
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (spawn.run_if(not(any_with_component::<Strip>)), update)
            .chain()
            .run_if(in_state(AppState::InGame)),
    );
}

fn spawn(mut commands: Commands, root: Query<Entity, With<HudRoot>>) {
    let Ok(root) = root.single() else { return };
    commands.entity(root).with_child((
        Strip,
        Node {
            position_type: PositionType::Absolute,
            top: px(EDGE),
            right: px(EDGE),
            height: px(BADGE.y),
            column_gap: px(10),
            flex_direction: FlexDirection::Row,
            ..default()
        },
    ));
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut commands: Commands,
    icons: Res<Icons>,
    me: Option<Res<LocalPlayer>>,
    muted_here: Option<Res<VoiceMuted>>,
    activity: Option<Res<VoiceActivity>>,
    players: Query<(&Player, Option<&Transformation>, Option<&MicMuted>)>,
    strip: Query<(Entity, Option<&Children>), With<Strip>>,
    badges: Query<&Badge>,
    mut looks: Query<(&mut BackgroundColor, &mut BorderColor), With<Badge>>,
    mut parts: Query<(
        &Part,
        &ChildOf,
        Option<&mut Text>,
        Option<&mut TextColor>,
        Option<&mut ImageNode>,
        &mut Node,
    )>,
) {
    let Ok((strip, kids)) = strip.single() else {
        return;
    };
    let me = me.map(|m| m.0);
    let want = roster(
        players
            .iter()
            .map(|(p, tears, muted)| {
                let local = Some(p.id) == me;
                BadgeState {
                    id: p.id,
                    name: name_for(p.id, local),
                    tears: tears.map(|t| t.0).unwrap_or(0),
                    muted: if local {
                        muted_here.as_ref().is_some_and(|m| m.0)
                    } else {
                        muted.is_some_and(|m| m.0)
                    },
                    speaking: activity.as_ref().is_some_and(|a| {
                        if local {
                            a.transmitting
                        } else {
                            a.heard.contains(&SpeakerId(p.id))
                        }
                    }),
                    local,
                }
            })
            .collect(),
    );
    // Same people in the same order? Update in place; else rebuild the row.
    let have: Vec<(Entity, u64)> = kids
        .map(|k| {
            k.iter()
                .filter_map(|e| badges.get(e).ok().map(|b| (e, b.0)))
                .collect()
        })
        .unwrap_or_default();
    let same = have.len() == want.len() && have.iter().zip(&want).all(|((_, id), b)| *id == b.id);
    if !same {
        for (e, _) in &have {
            commands.entity(*e).despawn();
        }
        for b in &want {
            let badge = commands.spawn(badge(&icons, b)).id();
            commands.entity(strip).add_child(badge);
        }
        return;
    }
    for ((entity, _), b) in have.iter().zip(&want) {
        let (fill, line, ink) = if b.speaking {
            (BUTTER, INK, INK)
        } else {
            (INK, BORDER, CREAM)
        };
        if let Ok((mut bg, mut border)) = looks.get_mut(*entity) {
            bg.set_if_neq(BackgroundColor(fill));
            border.set_if_neq(BorderColor::all(line));
        }
        for (part, parent, text, color, image, mut node) in &mut parts {
            if parent.parent() != *entity {
                continue;
            }
            if let Some(mut c) = color {
                c.set_if_neq(TextColor(ink));
            }
            if let (Some(mut img), true) = (image, matches!(part, Part::Mic | Part::Tear))
                && img.color != ink
            {
                img.color = ink;
            }
            let show = match part {
                Part::Slash => b.muted,
                Part::Wave => b.speaking,
                _ => true,
            };
            let d = if show { Display::Flex } else { Display::None };
            if node.display != d {
                node.display = d;
            }
            if let Some(mut t) = text {
                let s = match part {
                    Part::Name => b.name.clone(),
                    Part::Tears => super::times(b.tears),
                    _ => continue,
                };
                if t.0 != s {
                    t.0 = s;
                }
            }
        }
    }
}

fn badge(icons: &Icons, b: &BadgeState) -> impl Bundle {
    (
        Badge(b.id),
        Node {
            width: px(BADGE.x),
            height: px(BADGE.y),
            border: UiRect::all(px(B)),
            border_radius: BorderRadius::all(px(12)),
            overflow: Overflow::clip(),
            ..default()
        },
        BackgroundColor(INK),
        BorderColor::all(BORDER),
        children![
            (Part::Mic, glyph(&icons.mic, 16.0, 9.0, 5.0, B, CREAM)),
            (
                Part::Slash,
                glyph(&icons.mic_slash, 16.0, 9.0, 5.0, B, Color::WHITE)
            ),
            (
                Part::Name,
                Text::new(b.name.clone()),
                body(14.0, CREAM),
                TextLayout::no_wrap(),
                Node {
                    max_width: px(BADGE.x - 31.0 - 22.0),
                    overflow: Overflow::clip(),
                    ..at(31.0, 4.0, B)
                },
            ),
            (Part::Tear, glyph(&icons.tear, 16.0, 9.0, 24.0, B, CREAM)),
            (
                Part::Tears,
                Text::new(super::times(b.tears)),
                body(12.0, CREAM),
                at(31.0, 24.0, B)
            ),
            (
                Part::Wave,
                Node {
                    width: px(12),
                    height: px(14),
                    ..at(93.0, 15.0, B)
                },
                children![bar(1.0, 4.0, 6.0), bar(5.0, 1.0, 12.0), bar(9.0, 3.0, 8.0)],
            ),
        ],
    )
}

/// One static bar of the speaking waveform (no moving geometry, per spec).
fn bar(x: f32, y: f32, h: f32) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(x),
            top: px(y),
            width: px(2),
            height: px(h),
            border_radius: BorderRadius::all(px(1)),
            ..default()
        },
        BackgroundColor(INK),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(id: u64, local: bool) -> BadgeState {
        BadgeState {
            id,
            name: name_for(id, local),
            tears: 0,
            muted: false,
            speaking: false,
            local,
        }
    }

    #[test]
    fn local_first_then_by_id_capped_at_six() {
        let r = roster(vec![
            b(4, false),
            b(2, true),
            b(1, false),
            b(9, false),
            b(5, false),
            b(6, false),
            b(7, false),
        ]);
        let ids: Vec<u64> = r.iter().map(|b| b.id).collect();
        assert_eq!(ids, vec![2, 1, 4, 5, 6, 7]);
        assert_eq!(r[0].name, "You");
        assert_eq!(r[1].name, "Player 1");
    }
}
