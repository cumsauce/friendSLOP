//! The shared world seed for generated structures (structures rework, phases 3-4).
//!
//! The host rolls a seed when the server starts (or takes `FRIENDSLOP_SEED`),
//! spawns one replicated [`SharedSeed`] entity, and every peer (host included)
//! mirrors it into the `core` resource [`WorldSeed`] that generators read.
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
pub use friendslop_core::WorldSeed;

/// Replicated carrier of the world seed (resources don't replicate).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SharedSeed(pub u32);

/// Registered on every peer.
pub fn protocol(app: &mut App) {
    app.replicate::<SharedSeed>();
}

/// Rolls the seed on the server and mirrors it everywhere.
pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(ServerState::Running), roll)
        .add_systems(Update, mirror)
        .add_systems(OnExit(AppState::InGame), forget);
}

/// The seed to use: `FRIENDSLOP_SEED` if set (screenshots, bug repros), else
/// one derived from `entropy`.
pub fn choose(env: Option<&str>, entropy: f64) -> u32 {
    if let Some(s) = env.and_then(|s| s.trim().parse::<u32>().ok()) {
        return s;
    }
    let bits = entropy.to_bits();
    (bits ^ (bits >> 32)) as u32 ^ 0x5EED_F00D
}

fn roll(mut commands: Commands, existing: Query<(), With<SharedSeed>>) {
    if !existing.is_empty() {
        return;
    }
    let env = std::env::var("FRIENDSLOP_SEED").ok();
    let seed = choose(env.as_deref(), crate::settings::since_epoch().as_secs_f64());
    commands.spawn((Replicated, SharedSeed(seed)));
}

fn mirror(
    mut commands: Commands,
    seeds: Query<&SharedSeed, Changed<SharedSeed>>,
    current: Option<Res<WorldSeed>>,
) {
    let Some(seed) = seeds.iter().next() else {
        return;
    };
    if current.as_deref() != Some(&WorldSeed(seed.0)) {
        commands.insert_resource(WorldSeed(seed.0));
    }
}

fn forget(mut commands: Commands) {
    commands.remove_resource::<WorldSeed>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_seed_wins_and_entropy_varies() {
        assert_eq!(choose(Some("42"), 1.0), 42);
        assert_ne!(choose(None, 1.0), choose(None, 2.0));
        assert_eq!(choose(Some("nope"), 1.0), choose(None, 1.0));
    }

    #[test]
    fn sub_seeds_differ() {
        let w = WorldSeed(7);
        assert_ne!(w.for_(1), w.for_(2));
        assert_eq!(w.for_(1), WorldSeed(7).for_(1));
    }
}
