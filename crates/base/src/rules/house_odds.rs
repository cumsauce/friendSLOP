//! The gambling wheel's house odds (gamble.rs), which many features read.

#![allow(unused_imports)]
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// From `features/gamble.rs`.
/// Steps per knob: 0 is the fairest, `STEPS - 1` the cruellest.
pub const STEPS: usize = 5;

/// The normal game.
pub const NORMAL: u8 = 2;

/// A setting the wheel can move. Append only: the replicated levels are
/// indexed by this order.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Knob {
    /// Action failing (#8): chance any action fails.
    FailChance,
    /// Farting (#18): how close an NPC has to be to see you.
    ShameRadius,
    /// Farting x romance (#97): one shameful fart in N makes the NPC fall for you.
    Romance,
    /// Shame x attack mode (#123): catches before the villagers attack.
    Strikes,
    /// Morality x helicopter (#134): how low morality gets before it hunts you.
    Helicopter,
    /// Landlord x morality (#21/#43): how much harming him raises your morality.
    LandlordBonus,
    /// Landlord (#11): how much each rent raise adds.
    Rent,
    /// Warehouse (#130): the monster's hunting speed.
    Monster,
    /// Wendy's (#117): how fast burgers restock.
    Burgers,
}

impl Knob {
    pub const ALL: [Knob; 9] = [
        Knob::FailChance,
        Knob::ShameRadius,
        Knob::Romance,
        Knob::Strikes,
        Knob::Helicopter,
        Knob::LandlordBonus,
        Knob::Rent,
        Knob::Monster,
        Knob::Burgers,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    /// Short name on the wheel and the scoreboard.
    pub fn name(self) -> &'static str {
        match self {
            Knob::FailChance => "fate",
            Knob::ShameRadius => "nosy NPCs",
            Knob::Romance => "fart romance",
            Knob::Strikes => "the commune's patience",
            Knob::Helicopter => "the helicopter's conscience",
            Knob::LandlordBonus => "landlord karma",
            Knob::Rent => "rent",
            Knob::Monster => "the warehouse monster",
            Knob::Burgers => "Wendy's",
        }
    }

    /// The rule at `level`, in plain words.
    pub fn rule(self, level: u8) -> String {
        let o = HouseOdds::default().with(self, level);
        match self {
            Knob::FailChance => {
                format!("{:.0}% of actions fail", o.fail_chance() * 100.0)
            }
            Knob::ShameRadius => format!("NPCs see farts within {} m", o.shame_radius()),
            Knob::Romance => format!(
                "1 shameful fart in {} makes the NPC fall for you",
                o.romance_one_in()
            ),
            Knob::Strikes => format!(
                "{} fart catches set the villagers on you",
                o.strikes_to_attack()
            ),
            Knob::Helicopter => format!(
                "the helicopter comes at morality {} or lower",
                o.hunted_at()
            ),
            Knob::LandlordBonus => format!(
                "harming the landlord gives {:.0}% of the usual morality",
                o.landlord_bonus() * 100.0
            ),
            Knob::Rent => format!("each rent raise adds {}", o.rent_raise()),
            Knob::Monster => format!("the warehouse monster hunts at {} m/s", o.hunt_speed()),
            Knob::Burgers => format!("Wendy's restocks a burger in {} s", o.restock_secs()),
        }
    }
}

/// Values per step, fairest first. The middle is the normal game.
const FAIL: [f32; STEPS] = [0.01, 0.03, 0.05, 0.10, 0.20];

const SHAME: [f32; STEPS] = [2.0, 3.0, 4.0, 5.5, 7.5];

const ROMANCE: [u32; STEPS] = [1, 2, 3, 5, 9];

const STRIKES: [u8; STEPS] = [7, 5, 4, 3, 2];

const HUNTED: [i32; STEPS] = [-15, -10, -6, -4, -2];

const BONUS: [f32; STEPS] = [2.0, 1.5, 1.0, 0.6, 0.3];

const RENT: [u32; STEPS] = [5, 10, 15, 22, 33];

const HUNT: [f32; STEPS] = [4.8, 5.5, 6.2, 7.2, 8.4];

const RESTOCK: [f32; STEPS] = [5.0, 10.0, 15.0, 30.0, 60.0];

/// Replicated singleton: the current step of every knob.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HouseOdds {
    pub levels: [u8; 9],
}

impl Default for HouseOdds {
    fn default() -> Self {
        Self {
            levels: [NORMAL; 9],
        }
    }
}

impl HouseOdds {
    pub fn level(&self, knob: Knob) -> u8 {
        self.levels[knob.index()].min(STEPS as u8 - 1)
    }

    fn step<T: Copy>(&self, knob: Knob, table: &[T; STEPS]) -> T {
        table[self.level(knob) as usize]
    }

    pub fn with(mut self, knob: Knob, level: u8) -> Self {
        self.levels[knob.index()] = level.min(STEPS as u8 - 1);
        self
    }

    pub fn fail_chance(&self) -> f32 {
        self.step(Knob::FailChance, &FAIL)
    }
    pub fn shame_radius(&self) -> f32 {
        self.step(Knob::ShameRadius, &SHAME)
    }
    pub fn romance_one_in(&self) -> u32 {
        self.step(Knob::Romance, &ROMANCE)
    }
    pub fn strikes_to_attack(&self) -> u8 {
        self.step(Knob::Strikes, &STRIKES)
    }
    pub fn hunted_at(&self) -> i32 {
        self.step(Knob::Helicopter, &HUNTED)
    }
    pub fn landlord_bonus(&self) -> f32 {
        self.step(Knob::LandlordBonus, &BONUS)
    }
    pub fn rent_raise(&self) -> u32 {
        self.step(Knob::Rent, &RENT)
    }
    pub fn hunt_speed(&self) -> f32 {
        self.step(Knob::Monster, &HUNT)
    }
    pub fn restock_secs(&self) -> f32 {
        self.step(Knob::Burgers, &RESTOCK)
    }

    /// Knobs that aren't at the normal step: (knob, fairer?).
    pub fn changed(&self) -> Vec<(Knob, bool)> {
        Knob::ALL
            .iter()
            .filter(|k| self.level(**k) != NORMAL)
            .map(|k| (*k, self.level(*k) < NORMAL))
            .collect()
    }
}

/// The house odds in play (the normal game if there's no wheel).
pub fn odds(q: &Query<&HouseOdds>) -> HouseOdds {
    q.iter().next().copied().unwrap_or_default()
}
