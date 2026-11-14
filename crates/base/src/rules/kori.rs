//! Kori coins (#382, catherine): the currency you earn from farts, feces and
//! things of that nature. Spent on the claw machine (`side::kori`). Shared here
//! so the features that pay out (fart, toilets) can add to your purse,
//! and the walk can read the speed of a prize in your hand.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A player's Kori coins. Replicated, on the player's body; server-owned.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KoriCoins(pub u32);

/// The claw machine prize in a player's hand (an index into the prize list),
/// and the walking speed it adds (0.1 = +10%). Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct HeldPrize {
    pub prize: Option<u8>,
    pub speed: f32,
}

impl HeldPrize {
    /// Walking speed multiplier: a prize in hand speeds you up a little.
    pub fn speed(&self) -> f32 {
        if self.prize.is_some() {
            1.0 + self.speed
        } else {
            1.0
        }
    }
}

/// What things pay: `assets/data/base/kori.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Earnings {
    /// Coins for a fart (on purpose or not).
    pub fart: u32,
    /// Extra coins when an NPC was close enough to be shamed by it.
    pub shameful_fart: u32,
    /// Coins for eating the feces out of a toilet.
    pub feces: u32,
}

pub static EARNINGS: friendslop_data::Data<Earnings> =
    friendslop_data::Data::ron("data/base/kori.ron");

pub fn earnings() -> std::sync::Arc<Earnings> {
    EARNINGS.get()
}

/// Server: add `n` coins to a purse, if the body has one.
pub fn pay(coins: Option<Mut<KoriCoins>>, n: u32) {
    if let Some(mut c) = coins {
        c.0 = c.0.saturating_add(n);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_held_prize_speeds_you_up() {
        let mut h = HeldPrize {
            prize: None,
            speed: 0.1,
        };
        assert_eq!(h.speed(), 1.0);
        h.prize = Some(2);
        assert!((h.speed() - 1.1).abs() < 1e-6);
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            EARNINGS.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(EARNINGS.set_bytes(&bytes), "{path} doesn't parse");
        let e = EARNINGS.get();
        assert!(e.fart > 0 && e.feces > 0 && e.shameful_fart > 0);
    }
}
