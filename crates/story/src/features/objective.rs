//! The goal loop: one "Objective" line, a round-end summary, and restart.
//!
//! The shared goal is the quest log (`quests.rs`): when every quest is done
//! (beating the landlord is one of them) the round is over, everyone sees a
//! summary and anyone can press Enter to start a new campaign. The server
//! still mirrors the `Tenancy` act and progress into a replicated `Objective`.
//!
//! Restarting triggers a server-side [`RoundReset`] event. Each feature that
//! has per-round state observes it and resets itself (landlord, HALT, gods
//! tears, organs, morality, stats), so new features can hook in the same way:
//! `app.add_observer(|_: On<RoundReset>, ...| { ... })` in their `protocol`.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::halt::Halt;
use crate::features::landlord::{Act3, Tenancy};
use crate::features::quests::{QUESTS, QuestLog, ending};
pub use crate::rules::campaign::*;

/// Server -> everyone: where the shared goal stands.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Objective {
    /// Starts at 1, goes up each restart.
    pub round: u32,
    pub act: Act3,
    /// 0..=100 through the current act.
    pub progress: u8,
    /// Every quest is done and the round is waiting for a restart.
    pub over: bool,
}

/// Client -> server: "start a new round" (only honoured once the round is over).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct RequestRestart;

pub fn protocol(app: &mut App) {
    app.replicate::<Objective>()
        .add_client_event::<RequestRestart>(Channel::Ordered)
        .add_observer(request_restart)
        .add_observer(next_round)
        .add_systems(OnEnter(ServerState::Running), spawn_objective)
        .add_systems(
            Update,
            track_objective.run_if(in_state(ServerState::Running)),
        );
}

fn spawn_objective(mut commands: Commands) {
    commands.spawn((
        Objective {
            round: 1,
            ..default()
        },
        Replicated,
    ));
}

/// Progress through the current act, 0..=100.
pub fn act_progress(t: &Tenancy) -> u8 {
    let frac = match t.act {
        Act3::Legal => {
            t.raises as f32 / crate::features::landlord::tuning().raises_before_crime as f32
        }
        Act3::Crime => {
            t.heists as f32 / crate::features::landlord::tuning().heists_before_boss as f32
        }
        Act3::Boss => 1.0 - t.hp as f32 / crate::features::landlord::BOSS_HP as f32,
        Act3::Won => 1.0,
    };
    (frac.clamp(0.0, 1.0) * 100.0).round() as u8
}

fn track_objective(
    tenancy: Query<&Tenancy>,
    log: Query<&QuestLog>,
    mut objective: Query<&mut Objective>,
) {
    let (Ok(t), Ok(mut o)) = (tenancy.single(), objective.single_mut()) else {
        return;
    };
    // Without a quest log (tests that leave it out), beating the landlord ends it.
    let over = match log.single() {
        Ok(l) => l.finale(&QUESTS.get()),
        Err(_) => t.act == Act3::Won,
    };
    let next = Objective {
        round: o.round,
        act: t.act,
        progress: act_progress(t),
        over,
    };
    if *o != next {
        *o = next;
    }
}

fn request_restart(
    _ev: On<FromClient<RequestRestart>>,
    mut objective: Query<&mut Objective>,
    still: Query<&super::stillness::Stillness>,
    mut commands: Commands,
) {
    if super::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    if objective.iter().any(|o| o.over) {
        // Consume the gate now (commands are deferred), or a second request
        // in the same tick would fire another RoundReset.
        for mut o in &mut objective {
            o.over = false;
        }
        commands.trigger(RoundReset);
    }
}

fn next_round(_reset: On<RoundReset>, mut objective: Query<&mut Objective>) {
    for mut o in &mut objective {
        *o = Objective {
            round: o.round + 1,
            ..default()
        };
    }
}

/// The one-line objective for an act.
pub fn objective_line(o: &Objective) -> String {
    let goal = match o.act {
        Act3::Legal => "Act I | keep up with the rent",
        Act3::Crime => "Act II | take it back",
        Act3::Boss => "Act III | defeat the landlord",
        Act3::Won => "the landlord is defeated",
    };
    format!("{goal}  {}%", o.progress)
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_objective_ui)
        .add_systems(
            Update,
            (show_objective.run_if(crate::world::hud_tick()), restart_key)
                .run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct RoundSummary;

fn spawn_objective_ui(mut commands: Commands) {
    // The goal (#75: "make it really obvious") is the HUD's quest card now
    // (`ui::quest`): the quest you're on, the Heaven Pass and what's next.
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(22.0),
        RoundSummary,
        crate::hud::slot(crate::hud::HudSlot::CenterBanner),
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
}

fn show_objective(
    objective: Query<&Objective>,
    log: Query<&QuestLog>,
    halt: Query<&Halt>,
    tenancy: Query<&Tenancy>,
    mut summary: Query<(&mut Text, &mut Visibility), With<RoundSummary>>,
) {
    let Ok(o) = objective.single() else {
        return;
    };
    let Ok((mut t, mut vis)) = summary.single_mut() else {
        return;
    };
    let want = if o.over {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    if *vis != want {
        *vis = want;
    }
    if !o.over {
        return;
    }
    let escapes = halt.single().map(|h| h.escapes).unwrap_or(0);
    let rent = tenancy.single().map(|t| t.rent).unwrap_or(0);
    let stats = format!(
        "Final rent demanded: {rent}\n\
         Times you broke out of HALT: {escapes}"
    );
    let text = match log.single() {
        Ok(_) => format!("{}\n\n{stats}", ending()),
        Err(_) => format!(
            "Round {} over: the landlord is defeated!\n{stats}\n\n\
             Press [Enter] to start a new round",
            o.round
        ),
    };
    if t.0 != text {
        t.0 = text;
    }
}

fn restart_key(
    keys: Res<ButtonInput<KeyCode>>,
    objective: Query<&Objective>,
    mut commands: Commands,
) {
    if keys.just_pressed(KeyCode::Enter) && objective.iter().any(|o| o.over) {
        commands.client_trigger(RequestRestart);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_follows_the_acts() {
        let mut t = Tenancy::default();
        assert_eq!(act_progress(&t), 0);
        t.raises = crate::features::landlord::tuning().raises_before_crime;
        assert_eq!(act_progress(&t), 100);
        t.act = Act3::Boss;
        t.hp = crate::features::landlord::BOSS_HP / 2;
        assert_eq!(act_progress(&t), 50);
    }
}
