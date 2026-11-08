//! Action failing. From issue #8.
//!
//! Every action has a small chance (5%) of failing. A failed action doesn't
//! happen; instead the server rolls a random result from the list in the issue
//! (the six from the issue plus more in the same spirit from #49)
//! and puts it on the replicated `FateBoard`, which everyone sees on screen.
//! Results last a while (a "month" or an "hour" is squashed into 60 s) and
//! then wear off.
//!
//! Action handlers opt in with one line: add a `Fate` system param and
//! `if fate.fails(client) { return; }` at the point the action would happen.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::player::{Player, PlayerOwner};

/// Chance that any action fails.
pub const FAIL_CHANCE: f32 = 0.05;
/// How long a failure's result lasts.
pub const RESULT_SECS: f32 = 60.0;

/// What a failed action caused.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    WorldDestroyed,
    GovernmentCares,
    OwnLanguages,
    GenderTransed,
    Hampter,
    PottedPetunias,
    // More failures (#49).
    MoonApologises,
    EveryoneIsBees,
    TaxesFiledThemselves,
    GravityForgotYou,
    BecameAGoose,
    TimeWentSideways,
    LandlordsFeelShame,
    SoupEverywhere,
}

impl Outcome {
    pub const ALL: [Outcome; 14] = [
        Outcome::WorldDestroyed,
        Outcome::GovernmentCares,
        Outcome::OwnLanguages,
        Outcome::GenderTransed,
        Outcome::Hampter,
        Outcome::PottedPetunias,
        Outcome::MoonApologises,
        Outcome::EveryoneIsBees,
        Outcome::TaxesFiledThemselves,
        Outcome::GravityForgotYou,
        Outcome::BecameAGoose,
        Outcome::TimeWentSideways,
        Outcome::LandlordsFeelShame,
        Outcome::SoupEverywhere,
    ];

    /// Pick an outcome; `roll` is in `0.0..1.0`.
    pub fn pick(roll: f32) -> Self {
        let i = (roll.clamp(0.0, 0.999_999) * Self::ALL.len() as f32) as usize;
        Self::ALL[i.min(Self::ALL.len() - 1)]
    }

    /// What it looks like on screen, given who failed.
    pub fn describe(self, player: u64) -> String {
        match self {
            Outcome::WorldDestroyed => {
                "the world and everything in it was destroyed (it got better)".into()
            }
            Outcome::GovernmentCares => {
                "the nearest government cares about its citizens this month".into()
            }
            Outcome::OwnLanguages => "everyone speaks their own unique language this hour".into(),
            Outcome::GenderTransed => format!("player {player}'s gender has been transed"),
            Outcome::Hampter => format!("player {player}: hampter"),
            Outcome::PottedPetunias => format!("player {player} is now a pot of petunias"),
            Outcome::MoonApologises => "the moon has publicly apologised for everything".into(),
            Outcome::EveryoneIsBees => "everyone is bees for the next fortnight".into(),
            Outcome::TaxesFiledThemselves => {
                "everyone's taxes filed themselves correctly this year".into()
            }
            Outcome::GravityForgotYou => format!("gravity has forgotten player {player}'s name"),
            Outcome::BecameAGoose => format!("player {player} is now, legally, a goose"),
            Outcome::TimeWentSideways => "time went sideways for a week".into(),
            Outcome::LandlordsFeelShame => "every landlord briefly feels shame".into(),
            Outcome::SoupEverywhere => format!("player {player} has turned the sea into soup"),
        }
    }
}

/// One active failure result.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Mishap {
    pub player: u64,
    pub outcome: Outcome,
    pub secs_left: f32,
}

/// Replicated singleton: every failure result currently in effect.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct FateBoard {
    pub mishaps: Vec<Mishap>,
}

/// Server only: chance of failure (tests can change it) and dice.
#[derive(Resource)]
pub struct FateDice {
    pub chance: f32,
    seed: u32,
}

impl Default for FateDice {
    fn default() -> Self {
        Self {
            chance: FAIL_CHANCE,
            seed: 0x0fa7_e80d,
        }
    }
}

impl FateDice {
    fn roll(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// Server: ask fate whether an action by `client` fails.
#[derive(SystemParam)]
pub struct Fate<'w, 's> {
    dice: ResMut<'w, FateDice>,
    boards: Query<'w, 's, &'static mut FateBoard>,
    players: Query<'w, 's, (&'static Player, &'static PlayerOwner)>,
}

impl Fate<'_, '_> {
    /// True if the action fails (and a random result has been applied).
    pub fn fails(&mut self, client: ClientId) -> bool {
        if self.dice.roll() >= self.dice.chance {
            return false;
        }
        let owner = match client {
            ClientId::Server => None,
            ClientId::Client(e) => Some(e),
        };
        let player = self
            .players
            .iter()
            .find(|(_, o)| o.0 == owner)
            .map_or(0, |(p, _)| p.id);
        let outcome = Outcome::pick(self.dice.roll());
        if let Some(mut board) = self.boards.iter_mut().next() {
            board.mishaps.push(Mishap {
                player,
                outcome,
                secs_left: RESULT_SECS,
            });
        }
        true
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<FateBoard>()
        .init_resource::<FateDice>()
        .add_systems(OnEnter(ServerState::Running), spawn_board)
        .add_systems(Update, wear_off.run_if(in_state(ServerState::Running)));
}

fn spawn_board(mut commands: Commands) {
    commands.spawn((FateBoard::default(), Replicated));
}

fn wear_off(time: Res<Time>, mut boards: Query<&mut FateBoard>) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    for mut board in &mut boards {
        if board.mishaps.is_empty() {
            continue;
        }
        for m in &mut board.mishaps {
            m.secs_left -= dt;
        }
        board.mishaps.retain(|m| m.secs_left > 0.0);
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_fate_hud)
        .add_systems(
            Update,
            update_fate_hud
                .run_if(crate::world::hud_tick())
                .run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct FateHud;

fn spawn_fate_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextColor(Color::srgb(1.0, 0.55, 0.85)),
        FateHud,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(44),
            right: px(12),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_fate_hud(boards: Query<&FateBoard>, mut hud: Query<&mut Text, With<FateHud>>) {
    let mut lines = Vec::new();
    for board in &boards {
        for m in &board.mishaps {
            lines.push(format!(
                "ACTION FAILED: {} ({:.0}s)",
                m.outcome.describe(m.player),
                m.secs_left.ceil()
            ));
        }
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
    fn every_outcome_can_happen() {
        let picked: Vec<Outcome> = (0..140).map(|i| Outcome::pick(i as f32 / 140.0)).collect();
        for o in Outcome::ALL {
            assert!(picked.contains(&o), "{o:?}");
        }
    }

    #[test]
    fn every_outcome_has_its_own_text() {
        let texts: std::collections::HashSet<String> =
            Outcome::ALL.iter().map(|o| o.describe(1)).collect();
        assert_eq!(texts.len(), Outcome::ALL.len());
    }

    #[test]
    fn failure_is_rare_but_happens() {
        let mut dice = FateDice::default();
        let fails = (0..2000).filter(|_| dice.roll() < FAIL_CHANCE).count();
        assert!((40..200).contains(&fails), "{fails}");
    }

    #[test]
    fn the_first_few_actions_succeed() {
        // Keeps other features' multiplayer tests deterministic.
        let mut dice = FateDice::default();
        assert!((0..32).all(|_| dice.roll() >= FAIL_CHANCE));
    }
}
