//! QuestCard, top-left (24, 24, 390x136): chapter N/5, its title, jobs done,
//! the Heaven Pass tier and the next thing to do, all from one [`Campaign`].

use bevy::prelude::*;

use super::{HudRoot, at, body, head};
use crate::AppState;
use crate::features::campaign::{CHAPTERS, Campaign, heaven_tiers, next_line};
use crate::theme::indigo::{BORDER, CREAM, EDGE, INK, PERIWINKLE};

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Line {
    Chapter,
    Title,
    Progress,
    Next,
}

#[derive(Component)]
struct QuestCard;

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
/// the chapter, jobs or tier change (`hud::calm_alpha`), then folds to one line.
pub fn full_card(age: f32, t: &crate::hud::Tuning) -> bool {
    crate::hud::calm_alpha(age, false, false, t) > 0.0
}

/// Card height and the "now:" line's top, full or folded.
fn shape(full: bool) -> (f32, f32) {
    if full { (136.0, 100.0) } else { (40.0, 8.0) }
}

/// The four lines for a campaign state.
pub fn lines(c: &Campaign) -> [String; 4] {
    [
        format!("CHAPTER {} / {}", c.chapter + 1, CHAPTERS.get().len()),
        c.current().name.to_string(),
        format!(
            "Jobs {}/{}   •   Heaven Pass {}/{}",
            c.done_count(),
            c.current().tasks.len(),
            c.tier(),
            heaven_tiers()
        ),
        format!("»  {}", next_line(c)),
    ]
}

fn spawn(mut commands: Commands, root: Query<Entity, With<HudRoot>>) {
    let Ok(root) = root.single() else { return };
    let b = 2.0;
    commands.entity(root).with_child((
        QuestCard,
        Node {
            position_type: PositionType::Absolute,
            left: px(EDGE),
            top: px(EDGE),
            width: px(390),
            height: px(136),
            border: UiRect::all(px(b)),
            border_radius: BorderRadius::all(px(18)),
            ..default()
        },
        BackgroundColor(INK),
        BorderColor::all(BORDER),
        Visibility::Hidden,
        children![
            (
                Line::Chapter,
                Text::new(""),
                body(16.0, PERIWINKLE),
                at(16.0, 10.0, b)
            ),
            (
                Line::Title,
                Text::new(""),
                head(25.0, CREAM),
                at(16.0, 34.0, b)
            ),
            (
                Line::Progress,
                Text::new(""),
                body(17.0, CREAM),
                at(16.0, 70.0, b)
            ),
            (
                Line::Next,
                Text::new(""),
                body(16.0, PERIWINKLE),
                at(16.0, 100.0, b)
            ),
        ],
    ));
}

fn update(
    time: Res<Time>,
    all: Res<crate::hud::ShowAll>,
    mut seen: Local<crate::hud::Recent>,
    mut last: Local<f32>,
    campaign: Query<&Campaign>,
    mut card: Query<(&mut Visibility, &mut Node), With<QuestCard>>,
    mut texts: Query<(&Line, &mut Text, &mut Node), Without<QuestCard>>,
) {
    let Ok((mut vis, mut card_node)) = card.single_mut() else {
        return;
    };
    let Ok(c) = campaign.single() else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    vis.set_if_neq(Visibility::Inherited);
    let [chapter, title, progress, next] = lines(c);
    // `hud_tick` runs this a few times a second: count real time between runs.
    let now = time.elapsed_secs();
    let dt = (now - *last).max(0.0);
    *last = now;
    let age = seen.tick(
        crate::hud::key_of((c.chapter, c.done_count(), c.tier())),
        dt,
    );
    let full = all.0 || full_card(age, &crate::hud::tuning());
    let (h, next_top) = shape(full);
    if card_node.height != px(h) {
        card_node.height = px(h);
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
        if *line == Line::Next && node.top != px(next_top - 2.0) {
            node.top = px(next_top - 2.0);
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
    fn counts_come_from_one_campaign() {
        let c = Campaign::default();
        let [chapter, _, progress, next] = lines(&c);
        assert_eq!(chapter, "CHAPTER 1 / 5");
        assert!(progress.starts_with("Jobs 0/"), "{progress}");
        assert!(progress.ends_with(&format!("Heaven Pass 0/{}", heaven_tiers())));
        assert!(next.contains("now: "));
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
        assert_eq!(shape(false).0, 40.0);
    }
}
