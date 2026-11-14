//! One module per friend idea. See `features/README.md`.
//!
//! - Gameplay that must be identical on every peer (replicated components,
//!   network messages) is registered in `FeaturesProtocolPlugin`.
//! - Everything else (visuals, input, local UI) goes in `FeaturesPlugin`.

use bevy::prelude::*;

// The features live in the game-layer crates (see "Crates" in AGENTS.md);
// `side` and `afterlife` re-export everything below them.
pub use friendslop_afterlife::features::*;
pub use friendslop_side::features::*;

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
        app.add_plugins(infinite::protocol);
        app.add_plugins(kitty_lemonade::protocol);
        app.add_plugins(discord_friends::protocol);
        app.add_plugins(rats::protocol);
        // The team strip's mic badges (UI direction E): everyone's mute state.
        app.add_plugins(crate::voice::mic_protocol);
        app.add_plugins(world_seed::protocol);
        app.add_plugins(afterlife::protocol);
        app.add_plugins(systemd::protocol);
        app.add_plugins(watcher::protocol);
        app.add_plugins(clues::protocol);
        app.add_plugins(broadcast::protocol);
        app.add_plugins(nyc::protocol);
        app.add_plugins(crate::atmosphere::protocol);
        app.add_plugins(nyc_traffic::protocol);
        app.add_plugins(fulton::protocol);
        app.add_plugins(fulton_hunt::protocol);
        app.add_plugins(surf::protocol);
        app.add_plugins(wallet::protocol);
        app.add_plugins(trains::protocol);
        app.add_plugins(ragdoll::protocol);
        app.add_plugins(trains::protocol_runs);
        app.add_plugins(miami::protocol);
        app.add_plugins(boba::protocol);
        app.add_plugins(vessel_jump::protocol);
        app.add_plugins(high_places::protocol);
        app.add_plugins(ice_cream::protocol);
        app.add_plugins(hats::protocol);
        app.add_plugins(hi::protocol);
        app.add_plugins(kittens::protocol);
        app.add_plugins(toilets::protocol);
        app.add_plugins(emotes::protocol);
        app.add_plugins(kori::protocol);
        app.add_plugins(lily_garden::protocol);
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
        app.add_plugins((babble::plugin, voice_lines::plugin));
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
        app.add_plugins(doorway::plugin);
        app.add_plugins(start::plugin);
        app.add_plugins(abuela::plugin);
        app.add_plugins(sesbian_lex::plugin);
        app.add_plugins(punishment::plugin);
        app.add_plugins(helicopter::plugin);
        app.add_plugins(warehouse::plugin);
        app.add_plugins(wendys::plugin);
        app.add_plugins(underground::plugin);
        app.add_plugins(smoking::plugin);
        app.add_plugins(weed::plugin);
        app.add_plugins(caught::plugin);
        app.add_plugins(affair::plugin);
        app.add_plugins(gamble::plugin);
        app.add_plugins(fat_cat::plugin);
        app.add_plugins(gaslighting::plugin);
        app.add_plugins(skills::plugin);
        app.add_plugins(shipwreck::plugin);
        app.add_plugins(mute_spell::plugin);
        app.add_plugins(infinite::plugin);
        app.add_plugins(discord_friends::plugin);
        app.add_plugins(rats::plugin);
        app.add_plugins(world_seed::plugin);
        app.add_plugins(whispers::plugin);
        app.add_plugins(afterlife::plugin);
        app.add_plugins(systemd::plugin);
        app.add_plugins(watcher::plugin);
        app.add_plugins(nyc::plugin);
        app.add_plugins(nyc_traffic::plugin);
        app.add_plugins(fulton::plugin);
        app.add_plugins(fulton_hunt::plugin);
        app.add_plugins(clues::plugin);
        app.add_plugins(broadcast::plugin);
        app.add_plugins(surf::plugin);
        app.add_plugins(wallet::plugin);
        app.add_plugins(trains::plugin);
        app.add_plugins(vessel::plugin);
        app.add_plugins(ragdoll::plugin);
        app.add_plugins(miami::plugin);
        app.add_plugins(boba::plugin);
        app.add_plugins(penn::plugin);
        app.add_plugins(street_people::plugin);
        app.add_plugins(vessel_jump::plugin);
        app.add_plugins(pigeons::plugin);
        app.add_plugins(high_places::plugin);
        app.add_plugins(ice_cream::plugin);
        app.add_plugins(hats::plugin);
        app.add_plugins(hi::plugin);
        app.add_plugins(kittens::plugin);
        app.add_plugins(toilets::plugin);
        app.add_plugins(emotes::plugin);
        app.add_plugins(kori::plugin);
        app.add_plugins(lily_garden::plugin);
        app.add_plugins(achievements::plugin);
    }
}
