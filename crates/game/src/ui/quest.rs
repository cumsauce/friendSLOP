//! QuestCard, top-left (24, 24, 390x136): how many quests are on your list,
//! the one you're following, the Heaven Pass (quests done) and its next
//! step, all from the one shared [`QuestLog`].

use bevy::prelude::*;

use super::pause::PausePanel;
use super::{HudRoot, at, body, head};
use crate::AppState;
use crate::features::quests::{QUESTS, QuestLog, Tracking, followed, next_line, tier, title};
use crate::theme::kit::{CARD_100, CARD_300, CARD_500, EDGE, INK, SLOP_400, TAPE_200, size};

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Line {
    Chapter,
    Title,
    Progress,
    Next,
}

#[derive(Component)]
struct QuestCard;

/// The fill of the Heaven Pass meter under the title.
#[derive(Component)]
struct JobsFill;

/// The whole meter (hidden while the card is folded).
#[derive(Component)]
struct JobsBar;

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            spawn.run_if(not(any_with_component::<QuestCard>)),
            update.run_if(crate::world::hud_tick()),
        )
            .chain()
            .run_if(in_state(AppState::InGame)),
    );
}

/// Full card or just the "now:" line: the whole card shows for a moment when
/// the quest you follow, its step or the tier change (`hud::calm_alpha`), then
/// folds to one line.
pub fn full_card(age: f32, t: &crate::hud::Tuning) -> bool {
    crate::hud::calm_alpha(age, false, false, t) > 0.0
}

/// Card height and the "now:" line's top, full or folded.
fn shape(full: bool) -> (f32, f32) {
    if full { (156.0, 106.0) } else { (52.0, 8.0) }
}

/// The four lines for the log, following `tracking`.
pub fn lines(log: &QuestLog, tracking: Option<&str>) -> [String; 4] {
    let book = QUESTS.get();
    let on = log.active(&book).len();
    let (done, total) = tier(log);
    [
        match on {
            0 => "NO QUESTS YET".to_string(),
            1 => "QUEST".to_string(),
            n => format!("QUEST  ·  {n} on your list"),
        },
        followed(log, tracking).map_or_else(|| "Ask around".to_string(), |id| title(&id)),
        format!("Heaven Pass {done}/{total}   •   quests done"),
        format!("»  {}", next_line(log, tracking)),
    ]
}

fn spawn(mut commands: Commands, root: Query<Entity, With<HudRoot>>) {
    let Ok(root) = root.single() else { return };
    let b = 0.0;
    commands.entity(root).with_child((
        QuestCard,
        Node {
            position_type: PositionType::Absolute,
            left: px(EDGE),
            top: px(EDGE),
            width: px(390),
            height: px(156),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(CARD_100),
        // Cardboard on its shade layer, held on by a strip of tape.
        BoxShadow::new(CARD_500, px(4), px(5), px(0), px(0)),
        UiTransform::from_rotation(Rot2::degrees(-0.6)),
        Visibility::Hidden,
        children![
            (
                Node {
                    position_type: PositionType::Absolute,
                    top: px(-9),
                    left: px(150),
                    width: px(80),
                    height: px(20),
                    ..default()
                },
                BackgroundColor(TAPE_200.with_alpha(0.9)),
                UiTransform::from_rotation(Rot2::degrees(-3.0)),
            ),
            (
                Line::Chapter,
                Text::new(""),
                body(size::LABEL, INK),
                at(16.0, 12.0, b)
            ),
            (
                Line::Title,
                Text::new(""),
                head(size::H2, INK),
                at(16.0, 32.0, b)
            ),
            (
                Line::Progress,
                Text::new(""),
                body(size::LABEL, INK),
                at(16.0, 68.0, b)
            ),
            (
                JobsBar,
                Node {
                    width: px(390.0 - 32.0),
                    height: px(10),
                    border_radius: BorderRadius::all(px(5)),
                    overflow: Overflow::clip(),
                    ..at(16.0, 92.0, b)
                },
                BackgroundColor(CARD_300),
                children![(
                    JobsFill,
                    Node {
                        width: px(0),
                        height: percent(100),
                        ..default()
                    },
                    BackgroundColor(SLOP_400),
                )],
            ),
            (
                Line::Next,
                Text::new(""),
                body(size::LABEL, INK),
                // Wraps inside the card: two lines fit.
                Node {
                    width: px(390.0 - 32.0),
                    ..at(16.0, 106.0, b)
                }
            ),
        ],
    ));
}

fn update(
    time: Res<Time>,
    all: Res<crate::hud::ShowAll>,
    mut seen: Local<crate::hud::Recent>,
    mut last: Local<f32>,
    log: Query<&QuestLog>,
    tracking: Res<Tracking>,
    pause: Query<&Node, With<PausePanel>>,
    mut card: Query<(&mut Visibility, &mut Node), (With<QuestCard>, Without<PausePanel>)>,
    mut texts: Query<
        (&Line, &mut Text, &mut Node),
        (
            Without<QuestCard>,
            Without<JobsBar>,
            Without<JobsFill>,
            Without<PausePanel>,
        ),
    >,
    mut bar: Query<&mut Node, (With<JobsBar>, Without<QuestCard>, Without<PausePanel>)>,
    mut fill: Query<
        &mut Node,
        (
            With<JobsFill>,
            Without<JobsBar>,
            Without<QuestCard>,
            Without<PausePanel>,
        ),
    >,
) {
    let Ok((mut vis, mut card_node)) = card.single_mut() else {
        return;
    };
    let Ok(c) = log.single() else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    // The pause screen has the stage: no card ghosting under its scrim.
    let paused = pause.iter().any(|n| n.display != Display::None);
    vis.set_if_neq(if paused {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    });
    let [chapter, title, progress, next] = lines(c, tracking.0.as_deref());
    // `hud_tick` runs this a few times a second: count real time between runs.
    let now = time.elapsed_secs();
    let dt = (now - *last).max(0.0);
    *last = now;
    let (done, total) = tier(c);
    let age = seen.tick(crate::hud::key_of((&title, &next, done)), dt);
    let full = all.0 || full_card(age, &crate::hud::tuning());
    let (h, next_top) = shape(full);
    if card_node.height != px(h) {
        card_node.height = px(h);
    }
    for mut node in &mut bar {
        let d = if full { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
    }
    let share = done as f32 / total.max(1) as f32;
    for mut node in &mut fill {
        let w = percent(share.clamp(0.0, 1.0) * 100.0);
        if node.width != w {
            node.width = w;
        }
    }
    for (line, mut text, mut node) in &mut texts {
        let d = if full || *line == Line::Next {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != d {
            node.display = d;
        }
        if *line == Line::Next && node.top != px(next_top) {
            node.top = px(next_top);
        }
        let want = match line {
            Line::Chapter => &chapter,
            Line::Title => &title,
            Line::Progress => &progress,
            Line::Next => &next,
        };
        if &text.0 != want {
            text.0.clone_from(want);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_come_from_one_log() {
        let book = QUESTS.get();
        let mut log = QuestLog::fresh(&book);
        let [head, _, progress, next] = lines(&log, None);
        assert_eq!(head, "NO QUESTS YET");
        assert!(progress.starts_with(&format!("Heaven Pass 0/{}", book.quests.len())));
        assert!(next.contains("now: "));
        let first = book
            .quests
            .iter()
            .find(|q| log.accept(&book, &q.id, None))
            .map(|q| q.title.clone())
            .unwrap_or_default();
        let [head, title, _, _] = lines(&log, None);
        assert_eq!((head.as_str(), title), ("QUEST", first));
    }

    #[test]
    fn card_folds_to_one_line_after_a_while() {
        let t = crate::hud::Tuning {
            show_secs: 4.0,
            fade_secs: 1.0,
            ..Default::default()
        };
        assert!(full_card(0.0, &t));
        assert!(!full_card(6.0, &t));
        assert!(shape(false).0 < shape(true).0);
    }
}
