//! A player's six rolled stats, each 1..=10 (issue #7, reworked by #48).
//! The game's `stats.rs` says what they do; features read them from here.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// A player's stats, each 1..=10.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    pub spoonth: u8,
    pub glimbo: u8,
    pub frumbly: u8,
    pub hrumph: u8,
    pub wobblesome: u8,
    pub pelf: u8,
}

impl Stats {
    /// Roll stats from numbers in `0.0..1.0`, one per stat in field order.
    pub fn roll(r: [f32; 6]) -> Self {
        let pick = |r: f32| 1 + (r.clamp(0.0, 0.999_999) * 10.0) as u8;
        Self {
            spoonth: pick(r[0]),
            glimbo: pick(r[1]),
            frumbly: pick(r[2]),
            hrumph: pick(r[3]),
            wobblesome: pick(r[4]),
            pelf: pick(r[5]),
        }
    }

    pub fn all(&self) -> [u8; 6] {
        [
            self.spoonth,
            self.glimbo,
            self.frumbly,
            self.hrumph,
            self.wobblesome,
            self.pelf,
        ]
    }

    /// Earn `amount` more Pelf (the wealth stat): capped at 10.
    pub fn earn_pelf(&mut self, amount: u8) {
        self.pelf = self.pelf.saturating_add(amount).min(10);
    }
}
