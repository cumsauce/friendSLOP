//! Who the villagers are (villagers.rs).

#![allow(unused_imports)]
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// From `features/villagers.rs`.
/// Ten commune regulars plus the ten friends from the Discord (#159).
pub const COUNT: usize = 20;

/// Villager ids from here up are the friends (`discord_friends.rs`).
pub const FIRST_FRIEND: u8 = 10;

/// Villager names by id (`data/text/villager_names.ron`).
pub static NAMES: friendslop_data::Data<Vec<String>> =
    friendslop_data::Data::ron("data/text/villager_names.ron");

/// Villager `id`'s name.
pub fn name(id: usize) -> String {
    let names = NAMES.get();
    names
        .get(id % names.len().max(1))
        .cloned()
        .unwrap_or_else(|| format!("villager {id}"))
}

/// Server -> everyone: one villager. `pos` is the feet position on the floor.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Villager {
    pub id: u8,
    pub pos: Vec2,
    pub yaw: f32,
    pub sitting: bool,
}

impl Villager {
    pub fn name(&self) -> String {
        name(self.id as usize)
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
