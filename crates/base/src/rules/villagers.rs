//! Who the villagers are (villagers.rs).

#![allow(unused_imports)]
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// From `features/villagers.rs`.
/// Ten commune regulars plus the ten friends from the Discord (#159).
pub const COUNT: usize = 20;

/// Villager ids from here up are the friends (`discord_friends.rs`).
pub const FIRST_FRIEND: u8 = 10;

pub const NAMES: [&str; COUNT] = [
    "Kevin",
    "Brenda",
    "Old Tom",
    "Moss",
    "Pip",
    "Gwen",
    "Barnaby",
    "Juniper",
    "Dot",
    "Hank",
    // The friends who've sent in ideas on the Discord (#159), busiest first.
    "doomy",
    "catherine",
    "Freyja Moth",
    "Kori.exe",
    "just meat and cheese",
    "corvy",
    "bben",
    "WeiYa",
    "arseeN",
    "hadalade",
];

/// Server -> everyone: one villager. `pos` is the feet position on the floor.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Villager {
    pub id: u8,
    pub pos: Vec2,
    pub yaw: f32,
    pub sitting: bool,
}

impl Villager {
    pub fn name(&self) -> &'static str {
        NAMES[self.id as usize % COUNT]
    }

    /// One of the Discord friends (#159), not a commune regular.
    pub fn is_friend(&self) -> bool {
        is_friend(self.id)
    }
}

/// Is villager `id` one of the Discord friends (#159)?
pub fn is_friend(id: u8) -> bool {
    (FIRST_FRIEND..COUNT as u8).contains(&id)
}
