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
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::player::{Player, PlayerOwner};

/// A player's moral standing. Positive = good, negative = wicked.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Morality(pub i32);

impl Morality {
    pub fn label(self) -> &'static str {
        match self.0 {
            i32::MIN..=-20 => "reprehensible",
            -19..=-6 => "dubious",
            -5..=5 => "neutral",
            6..=19 => "decent",
            _ => "saintly",
        }
    }
}

/// Someone who can be on the receiving end of harm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Victim {
    Crow,
    Bear,
    Player,
    Nobility,
    Capitalist,
    Landlord,
    Billionaire,
}

impl Victim {
    /// The nobility, capitalists, landlords and billionaires.
    pub fn is_elite(self) -> bool {
        matches!(
            self,
            Victim::Nobility | Victim::Capitalist | Victim::Landlord | Victim::Billionaire
        )
    }
}

/// Something a player did that has moral weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    EatMushroom,
    Harm(Victim),
}

impl Act {
    /// Inclusive range the random moral value is drawn from.
    pub fn range(self) -> (i32, i32) {
        match self {
            Act::EatMushroom => (-2, 2),
            Act::Harm(v) if v.is_elite() => (6, 10),
            Act::Harm(_) => (-5, 3),
        }
    }

    /// Pick a value in range; `roll` is in `0.0..1.0`.
    pub fn value(self, roll: f32) -> i32 {
        let (lo, hi) = self.range();
        let span = (hi - lo + 1) as f32;
        (lo + (roll.clamp(0.0, 0.999_999) * span) as i32).min(hi)
    }
}

/// Server-only: `player` (a `Player` entity) did `act`.
#[derive(Event, Clone, Copy, Debug)]
pub struct MoralAct {
    pub player: Entity,
    pub act: Act,
}

impl MoralAct {
    /// Trigger a moral act for whichever player belongs to `client`.
    pub fn report(
        commands: &mut Commands,
        players: &Query<(Entity, &PlayerOwner)>,
        client: ClientId,
        act: Act,
    ) {
        let owner = match client {
            ClientId::Server => None,
            ClientId::Client(e) => Some(e),
        };
        if let Some((player, _)) = players.iter().find(|(_, o)| o.0 == owner) {
            commands.trigger(MoralAct { player, act });
        }
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<Morality>()
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

fn judge(act: On<MoralAct>, mut dice: ResMut<MoralDice>, mut players: Query<&mut Morality>) {
    let Ok(mut morality) = players.get_mut(act.player) else {
        return;
    };
    morality.0 += act.act.value(dice.roll());
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_morality_hud)
        .add_systems(
            Update,
            update_morality_hud.run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct MoralityHud;

fn spawn_morality_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        MoralityHud,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(12),
            left: px(12),
            ..default()
        },
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
