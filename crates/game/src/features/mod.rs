//! One module per friend idea. See `features/README.md`.
//!
//! - Gameplay that must be identical on every peer (replicated components,
//!   network messages) is registered in `FeaturesProtocolPlugin`.
//! - Everything else (visuals, input, local UI) goes in `FeaturesPlugin`.

use bevy::prelude::*;

pub mod bear;
pub mod cat_palette;
pub mod crows;
pub mod duel;
pub mod mushroom;
pub mod organ_currency;

pub struct FeaturesProtocolPlugin;

impl Plugin for FeaturesProtocolPlugin {
    fn build(&self, app: &mut App) {
        // Append only: order must match on every peer.
        app.add_plugins(organ_currency::protocol);
        app.add_plugins(crows::protocol);
        app.add_plugins(bear::protocol);
        app.add_plugins(mushroom::protocol);
        app.add_plugins(cat_palette::protocol);
        app.add_plugins(duel::protocol);
    }
}

pub struct FeaturesPlugin;

impl Plugin for FeaturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(organ_currency::plugin);
        app.add_plugins(crows::plugin);
        app.add_plugins(bear::plugin);
        app.add_plugins(mushroom::plugin);
        app.add_plugins(cat_palette::plugin);
        app.add_plugins(duel::plugin);
    }
}
