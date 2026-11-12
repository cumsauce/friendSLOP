//! Moral standing and the acts that move it (morality.rs).

#![allow(unused_imports)]
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::player::PlayerOwner;

// From `features/morality.rs`.
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
    /// Flirting with Officer Mei behind your wife's back (#160).
    Cheat,
}

impl Act {
    /// Inclusive range the random moral value is drawn from.
    pub fn range(self) -> (i32, i32) {
        match self {
            Act::EatMushroom => (-2, 2),
            Act::Cheat => (-4, -1),
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
