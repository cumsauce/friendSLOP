//! The chapter intro card (Figma 3:2153; UI polish, not a friend idea): when
//! a chapter starts, an ink card on the left says which one it is, what's
//! going on and what to do. Enter or Esc puts it away; after 6 s it shrinks
//! away on its own into the quest card (which keeps the goal).

use bevy::prelude::*;

use super::{Overlay, at, body, head};
use crate::AppState;
use crate::features::campaign::{CHAPTERS, Campaign};
use crate::theme::indigo::{BORDER, BUTTER, CREAM, INDIGO, INK, PERIWINKLE};

const B: f32 = 2.0;
/// How long the card stays before shrinking away.
pub const STAY_SECS: f32 = 6.0;
const SHRINK_SECS: f32 = 0.35;

/// "CHAPTER 02     /     05".
pub fn chapter_line(chapter: usize) -> String {
    format!("CHAPTER {:02}     /     {:02}", chapter + 1, CHAPTERS.len())
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
        .add_systems(PostUpdate, |i: Res<Intro>, mut o: ResMut<Overlay>| {
            let up = i.shown.is_some() && !i.dismissed;
            if o.intro != up {
                o.intro = up;
            }
        });
}

fn show(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    ready: Option<Res<crate::loading::WorldReady>>,
    mut intro: ResMut<Intro>,
    campaign: Query<&Campaign>,
    mut card: Query<(Entity, &mut UiTransform), With<Card>>,
) {
    let Ok(c) = campaign.single() else { return };
    if intro.shown != Some(c.chapter) {
        for (e, _) in &card {
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
    // The clock starts once the loading curtain has lifted.
    if ready.is_some_and(|r| r.ready) {
        intro.age += time.delta_secs();
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Escape]) {
        intro.age = intro.age.max(STAY_SECS);
    }
    let Ok((e, mut tf)) = card.single_mut() else {
        return;
    };
    let t = ((intro.age - STAY_SECS) / SHRINK_SECS).clamp(0.0, 1.0);
    if t >= 1.0 {
        intro.dismissed = true;
        commands.entity(e).despawn();
        return;
    }
    // Shrink up towards the quest card in the top-left corner.
    let s = 1.0 - t * t;
    let want = Vec2::splat(s.max(0.05));
    if tf.scale != want {
        tf.scale = want;
        tf.translation = Val2::px(-220.0 * t, -300.0 * t);
    }
}

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
    (
        Card,
        Node {
            position_type: PositionType::Absolute,
            left: px(24),
            top: px(198),
            width: px(465),
            height: px(411),
            border: UiRect::all(px(B)),
            border_radius: BorderRadius::all(px(18)),
            overflow: Overflow::clip(),
            ..default()
        },
        UiTransform::default(),
        BackgroundColor(INK),
        BorderColor::all(BORDER),
        GlobalZIndex(25),
        Pickable::IGNORE,
        DespawnOnExit(AppState::InGame),
        Children::spawn((
            Spawn((
                Text::new(chapter_line(c.chapter)),
                body(18.0, PERIWINKLE),
                at(22.0, 18.0, B),
            )),
            Spawn((
                Text::new(ch.name),
                head(40.0, CREAM),
                Node {
                    width: px(417),
                    ..at(22.0, 52.0, B)
                },
            )),
            Spawn((
                Text::new(blurb(ch.intro)),
                body(15.0, CREAM),
                Node {
                    width: px(417),
                    ..at(22.0, 106.0, B)
                },
            )),
            SpawnIter(tasks.into_iter().enumerate().map(|(i, (label, done))| {
                (
                    Node {
                        width: px(417),
                        column_gap: px(10),
                        align_items: AlignItems::Center,
                        ..at(22.0, 218.0 + 27.0 * i as f32, B)
                    },
                    children![
                        (
                            Node {
                                width: px(12),
                                height: px(12),
                                border: UiRect::all(px(2)),
                                border_radius: BorderRadius::all(px(3)),
                                ..default()
                            },
                            BorderColor::all(CREAM),
                            BackgroundColor(if done { BUTTER } else { Color::NONE }),
                        ),
                        (
                            Text::new(label),
                            body(15.0, CREAM),
                            TextLayout::no_wrap(),
                            Node {
                                max_width: px(395),
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
                body(13.0, PERIWINKLE),
                at(22.0, 218.0 + 27.0 * 4.0, B),
            )),
            Spawn((
                Node {
                    width: px(190),
                    height: px(44),
                    border: UiRect::all(px(B)),
                    border_radius: BorderRadius::all(px(12)),
                    ..at(22.0, 342.0, B)
                },
                BackgroundColor(INDIGO),
                BorderColor::all(BORDER),
                children![(
                    Text::new("Enter   continue"),
                    body(17.0, CREAM),
                    at(10.0, 8.0, B)
                )],
            )),
            Spawn((
                Text::new("Shrinks to goal card in 6 s  ·  Esc dismiss"),
                body(13.0, CREAM),
                Node {
                    width: px(215),
                    ..at(228.0, 349.0, B)
                },
            )),
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_line_and_blurb() {
        assert_eq!(chapter_line(1), "CHAPTER 02     /     05");
        assert_eq!(blurb("one two\n   three"), "one two three");
    }
}
