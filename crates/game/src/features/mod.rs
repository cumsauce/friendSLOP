//! One module per friend idea. See `features/README.md`.
//!
//! - Gameplay that must be identical on every peer (replicated components,
//!   network messages) is registered in `FeaturesProtocolPlugin`.
//! - Everything else (visuals, input, local UI) goes in `FeaturesPlugin`.

use bevy::prelude::*;

pub mod abuela;
pub mod affair;
pub mod ambience;
pub mod bear;
pub mod campaign;
pub mod cat_palette;
pub mod caught;
pub mod climb;
pub mod crows;
pub mod dave_commune;
pub mod duel;
pub mod fart;
pub mod fart_sound;
pub mod fat_cat;
pub mod fate;
pub mod fps;
pub mod fusion;
pub mod gamble;
pub mod gaslighting;
pub mod goat;
pub mod gods_tears;
pub mod great_storm;
pub mod halt;
pub mod helicopter;
pub mod hide_and_seek;
pub mod interact;
pub mod landlord;
pub mod morality;
pub mod mushroom;
pub mod mute_spell;
pub mod objective;
pub mod organ_currency;
pub mod pets;
pub mod punishment;
pub mod random_dialogue;
pub mod romance;
pub mod sesbian_lex;
pub mod shipwreck;
pub mod skills;
pub mod smoking;
pub mod stats;
pub mod stillness;
pub mod synth_audio;
pub mod totems;
pub mod underground;
pub mod villagers;
pub mod voice_lines;
pub mod warehouse;
pub mod wendys;

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
        app.add_plugins(fate::protocol);
        app.add_plugins(stats::protocol);
        app.add_plugins(totems::protocol);
        app.add_plugins(pets::protocol);
        app.add_plugins(halt::protocol);
        app.add_plugins(objective::protocol);
        app.add_plugins(fps::protocol);
        app.add_plugins(interact::protocol);
        app.add_plugins(hide_and_seek::protocol);
        app.add_plugins(villagers::protocol);
        app.add_plugins(campaign::protocol);
        app.add_plugins(romance::protocol);
        app.add_plugins(climb::protocol);
        app.add_plugins(crate::people::protocol);
        app.add_plugins(abuela::protocol);
        app.add_plugins(sesbian_lex::protocol);
        app.add_plugins(punishment::protocol);
        app.add_plugins(helicopter::protocol);
        app.add_plugins(warehouse::protocol);
        app.add_plugins(wendys::protocol);
        app.add_plugins(underground::protocol);
        app.add_plugins(smoking::protocol);
        app.add_plugins(caught::protocol);
        app.add_plugins(affair::protocol);
        app.add_plugins(gamble::protocol);
        app.add_plugins(fat_cat::protocol);
        app.add_plugins(gaslighting::protocol);
        app.add_plugins(skills::protocol);
        app.add_plugins(shipwreck::protocol);
        app.add_plugins(mute_spell::protocol);
    }
}

pub struct FeaturesPlugin;

impl Plugin for FeaturesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(synth_audio::plugin);
        app.add_plugins(ambience::plugin);
        app.add_plugins(fusion::plugin);
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
        app.add_plugins(crate::art::plugin);
        app.add_plugins(crate::people::plugin);
        app.add_plugins(landlord::plugin);
        app.add_plugins(fate::plugin);
        app.add_plugins(stats::plugin);
        app.add_plugins(totems::plugin);
        app.add_plugins(pets::plugin);
        app.add_plugins(halt::plugin);
        app.add_plugins(objective::plugin);
        app.add_plugins(fps::plugin);
        app.add_plugins(interact::plugin);
        app.add_plugins(hide_and_seek::plugin);
        app.add_plugins(villagers::plugin);
        app.add_plugins(campaign::plugin);
        app.add_plugins(romance::plugin);
        app.add_plugins(climb::plugin);
        app.add_plugins(abuela::plugin);
        app.add_plugins(sesbian_lex::plugin);
        app.add_plugins(punishment::plugin);
        app.add_plugins(helicopter::plugin);
        app.add_plugins(warehouse::plugin);
        app.add_plugins(wendys::plugin);
        app.add_plugins(underground::plugin);
        app.add_plugins(smoking::plugin);
        app.add_plugins(caught::plugin);
        app.add_plugins(affair::plugin);
        app.add_plugins(gamble::plugin);
        app.add_plugins(fat_cat::plugin);
        app.add_plugins(gaslighting::plugin);
        app.add_plugins(skills::plugin);
        app.add_plugins(shipwreck::plugin);
        app.add_plugins(mute_spell::plugin);
    }
}
