//! A morality system. From issue #21 by Freyja Moth.
//!
//! Every player carries a replicated `Morality` score (starts at 0). Other
//! features report what a player did by triggering a server-side `MoralAct`;
//! the server rolls a random moral value for it, within that act's sensible
//! range, and adds it to the player's score. Harming, in any way, the
//! nobility, capitalists, landlords or billionaires is *always* rewarded
//! (corrected by Freyja Moth in #43).
//!
//! Right now the only act in the game is eating the mushroom. Morality is
//! shown on the HUD; what it changes in play is still an open question.

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::AppState;
use crate::player::Player;
pub use crate::rules::morality::*;

pub fn protocol(app: &mut App) {
    app.replicate::<Morality>()
        .add_observer(reset_morality)
        .init_resource::<MoralDice>()
        .add_observer(give_morality)
        .add_observer(judge);
}

/// Server only: tiny deterministic random source.
#[derive(Resource)]
struct MoralDice(u32);

impl Default for MoralDice {
    fn default() -> Self {
        Self(0x5eed_2121)
    }
}

impl MoralDice {
    fn roll(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

fn give_morality(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() != ServerState::Running {
        return;
    }
    commands.entity(add.entity).insert(Morality::default());
}

fn judge(
    act: On<MoralAct>,
    mut dice: ResMut<MoralDice>,
    mut players: Query<&mut Morality>,
    odds: Query<&crate::rules::house_odds::HouseOdds>,
) {
    let Ok(mut morality) = players.get_mut(act.player) else {
        return;
    };
    let mut value = act.act.value(dice.roll());
    // Marmalade's wheel (#173) scales the reward for harming the landlord,
    // but it's always a reward (#43).
    if act.act == Act::Harm(Victim::Landlord) {
        let scale = crate::rules::house_odds::odds(&odds).landlord_bonus();
        value = ((value as f32 * scale).round() as i32).max(1);
    }
    morality.0 += value;
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_morality_hud)
        .add_systems(
            Update,
            update_morality_hud
                .run_if(crate::world::hud_tick())
                .run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct MoralityHud;

fn spawn_morality_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        MoralityHud,
        crate::hud::slot(crate::hud::HudSlot::Scoreboard),
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_morality_hud(
    players: Query<(&Player, &Morality)>,
    mut hud: Query<&mut Text, With<MoralityHud>>,
) {
    let mut rows: Vec<_> = players.iter().collect();
    rows.sort_by_key(|(p, _)| p.id);
    let mut lines = vec!["morality".to_string()];
    for (player, m) in rows {
        lines.push(format!("player {}: {} ({:+})", player.id, m.label(), m.0));
    }
    let want = lines.join("\n");
    for mut text in &mut hud {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

/// A new campaign: everyone starts morally neutral again. (Your conscience
/// carries over from chapter to chapter.)
fn reset_morality(
    _reset: On<crate::rules::campaign::NewCampaign>,
    mut players: Query<&mut Morality>,
) {
    for mut m in &mut players {
        *m = Morality::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harming_elites_is_always_rewarded() {
        for v in [
            Victim::Nobility,
            Victim::Capitalist,
            Victim::Landlord,
            Victim::Billionaire,
        ] {
            for i in 0..100 {
                assert!(Act::Harm(v).value(i as f32 / 100.0) > 0, "{v:?}");
            }
        }
    }

    #[test]
    fn values_stay_in_range_and_vary() {
        for act in [Act::EatMushroom, Act::Harm(Victim::Crow)] {
            let (lo, hi) = act.range();
            let vals: Vec<i32> = (0..100).map(|i| act.value(i as f32 / 100.0)).collect();
            assert!(vals.iter().all(|v| (lo..=hi).contains(v)));
            assert!(vals.contains(&lo) && vals.contains(&hi));
        }
    }
}
