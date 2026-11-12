//! Skill levels (skills.rs).

#![allow(unused_imports)]
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// From `features/skills.rs`.
/// The top level.
pub const MAX_LEVEL: u32 = 100;

/// XP for level `n` grows with the square: `XP_STEP * (n - 1)^2`.
pub const XP_STEP: f32 = 10.0;

/// What you can get better at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Skill {
    Walking,
    Farting,
    Sleeping,
    Pissing,
    Smoking,
    Dueling,
}

impl Skill {
    pub const ALL: [Skill; 6] = [
        Skill::Walking,
        Skill::Farting,
        Skill::Sleeping,
        Skill::Pissing,
        Skill::Smoking,
        Skill::Dueling,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Skill::Walking => "Walking",
            Skill::Farting => "Farting",
            Skill::Sleeping => "Sleeping",
            Skill::Pissing => "Pissing",
            Skill::Smoking => "Smoking",
            Skill::Dueling => "Dueling",
        }
    }
}

/// A player's XP in each skill (in [`Skill::ALL`] order). Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Skills(pub [f32; 6]);

impl Skills {
    pub fn xp(&self, s: Skill) -> f32 {
        self.0[s as usize]
    }

    pub fn level(&self, s: Skill) -> u32 {
        level(self.xp(s))
    }

    pub fn gain(&mut self, s: Skill, xp: f32) {
        self.0[s as usize] += xp;
    }

    /// Walking and running speed multiplier: up to +20% at the top level.
    pub fn walk_speed(&self) -> f32 {
        1.0 + 0.2 * (self.level(Skill::Walking) - 1) as f32 / (MAX_LEVEL - 1) as f32
    }

    /// Gas fill rate multiplier: up to +50% at the top level.
    pub fn gas_rate(&self) -> f32 {
        1.0 + 0.5 * (self.level(Skill::Farting) - 1) as f32 / (MAX_LEVEL - 1) as f32
    }
}

/// The level for `xp`: 1 at none, then each level needs more.
pub fn level(xp: f32) -> u32 {
    (1 + (xp.max(0.0) / XP_STEP).sqrt() as u32).min(MAX_LEVEL)
}
