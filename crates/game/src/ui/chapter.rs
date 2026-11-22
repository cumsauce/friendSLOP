//! The quest card's big brother (Figma 3:2153, made for chapters; the quests
//! replaced them): when the game starts, when your party takes a quest on and
//! when one is done, a card on the left says which quest it is, what it's
//! about, its steps and what it paid. Enter or Esc puts it away; after 6 s it
//! shrinks away on its own into the quest card (which keeps the goal).

use bevy::prelude::*;

use super::kit::{self, Glyph};
use super::pad_nav::PadHint;
use super::pause::PausePanel;
use super::{Overlay, Regular, body, head};
use crate::AppState;
use crate::features::quests::{
    Change, GOAL, QUESTS, QuestLog, QuestNews, giver_name, giver_where, step_line, tier,
};
use crate::theme::kit::{CARD_100, CARD_500, INK, RENT_500, SLOP_400, SLOP_600, TAPE_200, size};

/// How long the card stays before shrinking away.
pub const STAY_SECS: f32 = 6.0;
const SHRINK_SECS: f32 = 0.35;

/// "07", quests done in the big red digits.
pub fn chapter_number(done: usize) -> String {
    format!("{done:02}")
}

/// "/ 20", how many quests there are, beside the big number.
pub fn chapter_total() -> String {
    format!("/ {:02}", QUESTS.get().quests.len())
}

/// The intro flattened onto one paragraph (the source wraps for the old
/// banner).
pub fn blurb(intro: &str) -> String {
    intro.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[derive(Component)]
struct Card;

/// What a card is about.
#[derive(Debug, Clone, PartialEq)]
pub enum About {
    /// The game just started: the goal, and how quests work.
    Start,
    /// Quest `0` was taken on.
    Taken(String),
    /// Quest `0` is done, and paid `1` KoriCoins™ each.
    Done(String, u32),
}

/// Which card is shown and how long it has been up.
#[derive(Resource, Default)]
struct Intro {
    /// The card asked for, and how many have been asked for.
    want: Option<About>,
    asked: u32,
    shown: u32,
    age: f32,
    dismissed: bool,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Intro>()
        .add_observer(news_card)
        .add_systems(OnExit(AppState::InGame), |mut i: ResMut<Intro>| {
            *i = Intro::default();
        })
        .add_systems(Update, show.run_if(in_state(AppState::InGame)))
        .add_systems(
            PostUpdate,
            |i: Res<Intro>,
             mut o: ResMut<Overlay>,
             mut ctx: ResMut<crate::input::InputContexts>| {
                let up = i.shown > 0 && !i.dismissed;
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
    log: Query<&QuestLog>,
    pause: Query<&Node, With<PausePanel>>,
    mut card: Query<(Entity, &mut UiTransform, &mut Visibility), With<Card>>,
) {
    let Ok(l) = log.single() else { return };
    // The first card: the goal, once the log is here.
    if intro.want.is_none() {
        intro.want = Some(About::Start);
        intro.asked += 1;
    }
    if intro.shown != intro.asked {
        for (e, ..) in &card {
            commands.entity(e).despawn();
        }
        let about = intro.want.clone().unwrap_or(About::Start);
        let asked = intro.asked;
        *intro = Intro {
            want: Some(about.clone()),
            asked,
            shown: asked,
            ..default()
        };
        commands.spawn(card_for(l, &about));
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

/// A quest taken on or done brings the card up again.
fn news_card(ev: On<QuestNews>, mut intro: ResMut<Intro>) {
    let about = match ev.change {
        Change::Accepted => About::Taken(ev.quest.clone()),
        Change::Done => About::Done(ev.quest.clone(), ev.reward),
        _ => return,
    };
    intro.want = Some(about);
    intro.asked += 1;
}

/// What the card says: (kicker, title, blurb, steps with done, footnote).
pub fn words(l: &QuestLog, about: &About) -> (String, String, String, Vec<(String, bool)>, String) {
    let book = QUESTS.get();
    match about {
        About::Start => {
            let offered: Vec<(String, bool)> = book
                .quests
                .iter()
                .filter(|q| {
                    matches!(
                        l.state(&q.id),
                        crate::features::quests::QuestState::Available
                    )
                })
                .take(4)
                .map(|q| (format!("{}: ask {}", q.title, giver_name(&q.giver)), false))
                .collect();
            (
                "THE COMMUNE".into(),
                "Everyone needs a hand".into(),
                format!(
                    "{GOAL}. People all over the world have a ! over their head: \
                     talk to them and they'll give your party quests. Some only come out at night."
                ),
                offered,
                "Every quest done pays a KoriCoin™ to everyone".into(),
            )
        }
        About::Taken(id) | About::Done(id, _) => {
            let Some(q) = book.get(id) else {
                return Default::default();
            };
            let variant = match l.state(id) {
                crate::features::quests::QuestState::Active { variant, .. } => variant,
                _ => None,
            };
            let at = match l.state(id) {
                crate::features::quests::QuestState::Active { step, .. } => step as usize,
                crate::features::quests::QuestState::Done => usize::MAX,
                _ => 0,
            };
            let steps = q
                .steps_for(variant.as_deref())
                .iter()
                .enumerate()
                .take(4)
                .map(|(i, s)| (crate::input::fill(&s.text), i < at))
                .collect();
            let (kicker, foot) = match about {
                About::Done(_, n) => (
                    "QUEST DONE".to_string(),
                    match n {
                        0 => String::new(),
                        1 => "+1 KoriCoin™ for everyone".to_string(),
                        n => format!("+{n} KoriCoins™ for everyone"),
                    },
                ),
                _ => (
                    "NEW QUEST".to_string(),
                    step_line(l, id).map_or_else(
                        || format!("from {} {}", giver_name(&q.giver), giver_where(&q.giver)),
                        |s| format!("now: {s}"),
                    ),
                ),
            };
            (kicker, q.title.clone(), q.summary.clone(), steps, foot)
        }
    }
}

/// Longest task line the card shows before it ends in "..." (20 px bold text in a
/// 400 px column, with room to spare).
const TASK_CHARS: usize = 38;

fn card_for(l: &QuestLog, about: &About) -> impl Bundle {
    let (kicker, name, intro, tasks, foot) = words(l, about);
    let number = chapter_number(tier(l).0);
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
            Spawn((Text::new(kicker), body(size::LABEL, INK))),
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
                Text::new(name),
                head(size::H1, INK),
                Node {
                    margin: UiRect::vertical(px(6)),
                    ..default()
                },
            )),
            Spawn((
                Text::new(blurb(&intro)),
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
            Spawn((Text::new(foot), body(size::LABEL, CARD_500))),
            Spawn((
                Node {
                    margin: UiRect::top(px(16)),
                    align_items: AlignItems::Center,
                    justify_content: JustifyContent::SpaceBetween,
                    ..default()
                },
                children![
                    (
                        Node {
                            flex_direction: FlexDirection::Column,
                            row_gap: px(6),
                            ..default()
                        },
                        children![
                            (
                                Text::new("Shrinks to the quest card in 6 s"),
                                body(size::LABEL, CARD_500),
                            ),
                            (
                                Node {
                                    align_items: AlignItems::Center,
                                    column_gap: px(8),
                                    ..default()
                                },
                                crate::input::KeyRow::new(
                                    crate::input::Hint("{back} dismiss".into()),
                                    crate::input::KeyStyle::Card,
                                ),
                            ),
                        ],
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
        assert_eq!(chapter_number(2), "02");
        assert_eq!(
            chapter_total(),
            format!("/ {:02}", QUESTS.get().quests.len())
        );
        assert_eq!(blurb("one two\n   three"), "one two three");
        let book = QUESTS.get();
        let mut l = QuestLog::fresh(&book);
        let (_, _, _, offered, _) = words(&l, &About::Start);
        assert!(!offered.is_empty(), "the first card lists who to ask");
        let first = book
            .quests
            .iter()
            .find(|q| l.accept(&book, &q.id, None))
            .cloned()
            .unwrap_or_default();
        let q = first.id.clone();
        let (kicker, title, _, steps, foot) = words(&l, &About::Taken(q.clone()));
        assert_eq!(kicker, "NEW QUEST");
        assert_eq!(title, first.title);
        assert!(!steps.is_empty() && foot.starts_with("now: "));
        let (_, _, _, _, paid) = words(&l, &About::Done(q, 2));
        assert!(paid.contains("+2 KoriCoins"));
    }
}
