//! Annotations (#388, bben): in very small type under the Cat card, bottom
//! right, one line explaining each world-wide effect happening right now (the
//! Stillness, night, a mute-spell hunt). The words are a data file
//! (`data/text/annotations.ron`).

use bevy::prelude::*;

use super::body;
use crate::AppState;
use crate::atmosphere::{DAY_NIGHT, DayStart, daylight, sun_height, time_of_day};
use crate::features::mute_spell::Silenced;
use crate::features::stillness::Stillness;
use crate::player::Player;
use crate::theme::indigo::CREAM;
use friendslop_base::ui::StatusMount;

/// The annotation lines: `assets/data/text/annotations.ron` (read at run time).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Lines {
    /// While the world is frozen; `{s}` is seconds left.
    pub frozen: String,
    /// Just before the Stillness; `{s}` is seconds until it.
    pub freeze_soon: String,
    /// How many seconds before the Stillness the warning shows.
    pub freeze_warn_secs: u32,
    /// While it's night.
    pub night: String,
    /// While someone is muted by the mute spell; `{name}` is them.
    pub silenced: String,
}

pub static LINES: friendslop_data::Data<Lines> =
    friendslop_data::Data::ron("data/text/annotations.ron");

/// Everything world-wide going on now, as plain values.
#[derive(Default, Debug, Clone, PartialEq)]
pub struct Effects {
    pub stillness: Option<Stillness>,
    pub night: bool,
    pub silenced: Vec<String>,
}

/// The annotation lines for `fx`, most urgent first.
pub fn annotate(fx: &Effects, lines: &Lines) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(s) = fx.stillness {
        let secs = s.secs_left.to_string();
        if s.frozen {
            out.push(lines.frozen.replace("{s}", &secs));
        } else if s.secs_left <= lines.freeze_warn_secs {
            out.push(lines.freeze_soon.replace("{s}", &secs));
        }
    }
    for name in &fx.silenced {
        out.push(lines.silenced.replace("{name}", name));
    }
    if fx.night {
        out.push(lines.night.clone());
    }
    out
}

#[derive(Component)]
struct Annotations;

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (mount, update).chain().run_if(in_state(AppState::InGame)),
    );
}

/// Put the note text at the bottom of the status stack, under the Cat card.
fn mount(mut commands: Commands, mounts: Query<Entity, Added<StatusMount>>) {
    for m in &mounts {
        commands.entity(m).with_child((
            Annotations,
            Text::new(""),
            body(9.0, CREAM.with_alpha(0.75)),
            TextLayout::justify(Justify::Right),
            Node {
                width: Val::Percent(100.0),
                ..default()
            },
        ));
    }
}

fn update(
    still: Query<&Stillness>,
    started: Query<&DayStart>,
    silenced: Query<&Player, With<Silenced>>,
    mut text: Query<(&mut Text, &mut Node), With<Annotations>>,
) {
    let day = DAY_NIGHT.get();
    let now = crate::settings::since_epoch().as_secs_f64();
    let start = started.iter().next().map_or(now, |d| d.0);
    let t = time_of_day(now - start, day.day_minutes, day.start_hour);
    let fx = Effects {
        stillness: still.iter().next().copied(),
        night: daylight(sun_height(t)) < 0.05,
        silenced: silenced
            .iter()
            .map(|p| format!("player {}", p.id))
            .collect(),
    };
    let s = annotate(&fx, &LINES.get()).join("\n");
    for (mut text, mut node) in &mut text {
        if text.0 != s {
            text.0 = s.clone();
        }
        let d = if s.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        if node.display != d {
            node.display = d;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines() -> Lines {
        Lines {
            frozen: "frozen {s}".into(),
            freeze_soon: "soon {s}".into(),
            freeze_warn_secs: 10,
            night: "night".into(),
            silenced: "{name} muted".into(),
        }
    }

    #[test]
    fn nothing_happening_says_nothing() {
        assert!(annotate(&Effects::default(), &lines()).is_empty());
        let calm = Effects {
            stillness: Some(Stillness {
                frozen: false,
                secs_left: 100,
            }),
            ..default()
        };
        assert!(annotate(&calm, &lines()).is_empty());
    }

    #[test]
    fn each_effect_gets_a_line() {
        let fx = Effects {
            stillness: Some(Stillness {
                frozen: true,
                secs_left: 7,
            }),
            night: true,
            silenced: vec!["player 2".into()],
        };
        assert_eq!(
            annotate(&fx, &lines()),
            vec!["frozen 7", "player 2 muted", "night"]
        );
        let soon = Effects {
            stillness: Some(Stillness {
                frozen: false,
                secs_left: 4,
            }),
            ..default()
        };
        assert_eq!(annotate(&soon, &lines()), vec!["soon 4"]);
    }
}
