//! One module per friend idea. See `features/README.md`.
//!
//! - Gameplay that must be identical on every peer (replicated components,
//!   network messages) is registered in `FeaturesProtocolPlugin`.
//! - Everything else (visuals, input, local UI) goes in `FeaturesPlugin`.

use bevy::prelude::*;

pub mod organ_currency;

pub struct FeaturesProtocolPlugin;

impl Plugin for FeaturesProtocolPlugin {
    fn build(&self, app: &mut App) {
        // Append only: order must match on every peer.
        app.add_plugins(organ_currency::protocol);
    }
}

pub struct FeaturesPlugin;

impl Plugin for FeaturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(organ_currency::plugin);
    }
}
