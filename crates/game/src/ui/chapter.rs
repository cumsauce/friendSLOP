//! The chapter intro card (Figma 3:2153; UI polish, not a friend idea): when
//! a chapter starts, an ink card on the left says which one it is, what's
//! going on and what to do. Enter or Esc puts it away; after 6 s it shrinks
//! away on its own into the quest card (which keeps the goal).

use bevy::prelude::*;

use super::kit::{self, Glyph};
use super::pad_nav::PadHint;
use super::pause::PausePanel;
use super::{Overlay, Regular, body, head};
use crate::AppState;
use crate::features::campaign::{CHAPTERS, Campaign};
use crate::theme::kit::{CARD_100, CARD_500, INK, RENT_500, SLOP_400, SLOP_600, TAPE_200, size};

/// How long the card stays before shrinking away.
pub const STAY_SECS: f32 = 6.0;
const SHRINK_SECS: f32 = 0.35;

/// "02", the chapter's number in the big red digits.
pub fn chapter_number(chapter: usize) -> String {
    format!("{:02}", chapter + 1)
}

/// "/ 05", how many chapters there are, beside the big number.
pub fn chapter_total() -> String {
    format!("/ {:02}", CHAPTERS.get().len())
}

/// The intro flattened onto one paragraph (the source wraps for the old
/// banner).
pub fn blurb(intro: &str) -> String {
    intro.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Component)]
struct Card;

/// Which chapter is shown and how long it has been up.
#[derive(Resource, Default)]
struct Intro {
    shown: Option<usize>,
    age: f32,
    dismissed: bool,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Intro>()
        .add_systems(OnExit(AppState::InGame), |mut i: ResMut<Intro>| {
            *i = Intro::default();
        })
        .add_systems(Update, show.run_if(in_state(AppState::InGame)))
        .add_systems(
            PostUpdate,
            |i: Res<Intro>,
             mut o: ResMut<Overlay>,
             mut ctx: ResMut<crate::input::InputContexts>| {
                let up = i.shown.is_some() && !i.dismissed;
                crate::input::switch(&mut ctx, "chapter", up);
                if o.intro != up {
                    o.intro = up;
                }
            },
        );
}

fn show(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    ready: Option<Res<crate::loading::WorldReady>>,
    mut intro: ResMut<Intro>,
    campaign: Query<&Campaign>,
    pause: Query<&Node, With<PausePanel>>,
    mut card: Query<(Entity, &mut UiTransform, &mut Visibility), With<Card>>,
) {
    let Ok(c) = campaign.single() else { return };
    if intro.shown != Some(c.chapter) {
        for (e, ..) in &card {
            commands.entity(e).despawn();
        }
        *intro = Intro {
            shown: Some(c.chapter),
            ..default()
        };
        commands.spawn(card_for(c));
        return;
    }
    if intro.dismissed {
        return;
    }
    let Ok((e, mut tf, mut vis)) = card.single_mut() else {
        return;
    };
    // The pause screen has the stage: the card waits behind it, unseen, not ageing, and
    // Esc (which closes the pause screen) does not also put the card away.
    let paused = pause.iter().any(|n| n.display != Display::None);
    vis.set_if_neq(if paused {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    if paused {
        return;
    }
    // The clock starts once the loading curtain has lifted.
    if ready.is_some_and(|r| r.ready) {
        intro.age += time.delta_secs();
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Escape]) {
        intro.age = intro.age.max(STAY_SECS);
    }
    let t = ((intro.age - STAY_SECS) / SHRINK_SECS).clamp(0.0, 1.0);
    if t >= 1.0 {
        intro.dismissed = true;
        commands.entity(e).despawn();
        return;
    }
    // Shrink up towards the quest card in the top-left corner.
    let s: f32 = 1.0 - t * t;
    let want = Vec2::splat(s.max(0.05));
    if tf.scale != want {
        tf.scale = want;
        tf.translation = Val2::px(-220.0 * t, -300.0 * t);
    }
}

/// Longest task line the card shows before it ends in "..." (20 px bold text in a
/// 400 px column, with room to spare).
const TASK_CHARS: usize = 38;

fn card_for(c: &Campaign) -> impl Bundle {
    let ch = c.current();
    let tasks: Vec<(String, bool)> = ch
        .tasks
        .iter()
        .enumerate()
        .take(4)
        .map(|(i, t)| (t.label().to_string(), c.task_done(i)))
        .collect();
    let more = ch.tasks.len().saturating_sub(4);
    let number = chapter_number(c.chapter);
    (
        Card,
        Node {
            position_type: PositionType::Absolute,
            left: px(24),
            top: px(190),
            width: px(465),
            flex_direction: FlexDirection::Column,
            padding: UiRect::axes(px(26), px(22)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        UiTransform::default(),
        // Cardboard on its shade layer, held on by tape, like the rest of the notice board.
        BackgroundColor(CARD_100),
        BoxShadow::new(CARD_500, px(5), px(6), px(0), px(0)),
        GlobalZIndex(25),
        Pickable::IGNORE,
        DespawnOnExit(AppState::InGame),
        Children::spawn((
            Spawn((
                Node {
                    position_type: PositionType::Absolute,
                    top: px(-10),
                    left: px(150),
                    width: px(90),
                    height: px(22),
                    ..default()
                },
                BackgroundColor(TAPE_200.with_alpha(0.9)),
                UiTransform::from_rotation(Rot2::degrees(-3.0)),
            )),
            Spawn((Text::new("CHAPTER"), body(size::LABEL, INK))),
            Spawn((
                Node {
                    align_items: AlignItems::Baseline,
                    column_gap: px(12),
                    ..default()
                },
                children![
                    kit::display(&number, size::DISPLAY, RENT_500),
                    (Text::new(chapter_total()), head(size::H2, CARD_500),),
                ],
            )),
            Spawn((
                Text::new(ch.name.clone()),
                head(size::H1, INK),
                Node {
                    margin: UiRect::vertical(px(6)),
                    ..default()
                },
            )),
            Spawn((
                Text::new(blurb(&ch.intro)),
                body(size::BODY_REG, INK),
                Regular,
                Node {
                    margin: UiRect::bottom(px(12)),
                    ..default()
                },
            )),
            SpawnIter(tasks.into_iter().map(|(label, done)| {
                (
                    Node {
                        column_gap: px(10),
                        align_items: AlignItems::Center,
                        margin: UiRect::bottom(px(6)),
                        ..default()
                    },
                    children![
                        (
                            Node {
                                width: px(13),
                                height: px(13),
                                border_radius: BorderRadius::MAX,
                                ..default()
                            },
                            BackgroundColor(if done { SLOP_400 } else { SLOP_600 }),
                        ),
                        (
                            Text::new(super::ellipsize(&label, TASK_CHARS)),
                            body(size::BODY_REG, INK),
                            TextLayout::no_wrap(),
                            Node {
                                max_width: px(390),
                                overflow: Overflow::clip(),
                                ..default()
                            },
                        ),
                    ],
                )
            })),
            Spawn((
                Text::new(if more > 0 {
                    format!("+ {more} more on the quest card")
                } else {
                    String::new()
                }),
                body(size::LABEL, CARD_500),
            )),
            Spawn((
                Node {
                    margin: UiRect::top(px(16)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                children![
                    (
                        Text::new(""),
                        crate::input::Hint(
                            "Shrinks to the quest card in 6 s  ·  {back} dismiss".into()
                        ),
                        body(size::LABEL, CARD_500),
                        Node {
                            max_width: px(240),
                            ..default()
                        },
                    ),
                    (PadHint, kit::hint(Glyph::A, "Got it")),
                ],
            )),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_line_and_blurb() {
        assert_eq!(chapter_number(1), "02");
        assert_eq!(chapter_total(), format!("/ {:02}", CHAPTERS.get().len()));
        assert_eq!(blurb("one two\n   three"), "one two three");
    }
}
