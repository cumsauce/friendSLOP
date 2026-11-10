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

/// The four lines for a campaign state.
pub fn lines(c: &Campaign) -> [String; 4] {
    [
        format!("CHAPTER {} / {}", c.chapter + 1, CHAPTERS.len()),
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
    campaign: Query<&Campaign>,
    mut card: Query<&mut Visibility, With<QuestCard>>,
    mut texts: Query<(&Line, &mut Text)>,
) {
    let Ok(mut vis) = card.single_mut() else {
        return;
    };
    let Ok(c) = campaign.single() else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    vis.set_if_neq(Visibility::Inherited);
    let [chapter, title, progress, next] = lines(c);
    for (line, mut text) in &mut texts {
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
}
