//! One module per friend idea. See `features/README.md`.
//!
//! - Gameplay that must be identical on every peer (replicated components,
//!   network messages) is registered in `FeaturesProtocolPlugin`.
//! - Everything else (visuals, input, local UI) goes in `FeaturesPlugin`.

use bevy::prelude::*;

pub mod bear;
pub mod cat_palette;
pub mod crows;
pub mod dave_commune;
pub mod duel;
pub mod fart;
pub mod fart_sound;
pub mod goat;
pub mod gods_tears;
pub mod great_storm;
pub mod landlord;
pub mod morality;
pub mod mushroom;
pub mod organ_currency;
pub mod random_dialogue;
pub mod stillness;
pub mod voice_lines;

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
        app.add_plugins(dave_commune::protocol);
        app.add_plugins(morality::protocol);
        app.add_plugins(goat::protocol);
        app.add_plugins(voice_lines::protocol);
        app.add_plugins(stillness::protocol);
        app.add_plugins(gods_tears::protocol);
        app.add_plugins(fart::protocol);
        app.add_plugins(fart_sound::protocol);
        app.add_plugins(landlord::protocol);
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
        app.add_plugins(dave_commune::plugin);
        app.add_plugins(morality::plugin);
        app.add_plugins(goat::plugin);
        app.add_plugins(voice_lines::plugin);
        app.add_plugins(stillness::plugin);
        app.add_plugins(gods_tears::plugin);
        app.add_plugins(fart::plugin);
        app.add_plugins(fart_sound::plugin);
        app.add_plugins(great_storm::plugin);
        app.add_plugins(landlord::plugin);
    }
}
