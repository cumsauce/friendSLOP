//! friendSLOP game layer: Standalone ideas nothing else depends on.
//!
//! See "Crates" in AGENTS.md. Modules keep their old `crate::` paths:
//! the shared modules below are re-exported from the crates under this one.

#[allow(unused_imports)]
pub(crate) use friendslop_base::{
    AppState, art, assets, buildings, camera, controls, graphics, hud, input, kcc, landmarks,
    music, people, player, rigged, rules, scenery, settings, spatial, surfaces, terrain, textures,
    theme, ui, voice, voice_chat, world, zones,
};

pub mod features {
    #![allow(unused_imports)]
    pub use friendslop_base::features::*;
    pub use friendslop_places::features::*;
    pub use friendslop_play::features::*;
    pub use friendslop_story::features::*;
    pub use friendslop_village::features::*;
    pub mod affair;
    pub mod ambience;
    pub mod boba;
    pub mod broadcast;
    pub mod clues;
    pub mod discord_friends;
    pub mod emotes;
    pub mod fulton;
    pub mod fulton_hunt;
    pub mod fusion;
    pub mod gaslighting;
    pub mod goat;
    pub mod hats;
    pub mod hi;
    pub mod high_places;
    pub mod ice_cream;
    pub mod infinite;
    pub mod kittens;
    pub mod kori;
    pub mod lily_garden;
    pub mod miami;
    pub mod nyc;
    pub mod nyc_traffic;
    pub mod penn;
    pub mod pigeons;
    pub mod rats;
    pub mod sesbian_lex;
    pub mod shipwreck;
    pub mod street_people;
    pub mod surf;
    pub mod toilets;
    pub mod trains;
    pub mod vessel;
    pub mod vessel_jump;
    pub mod wallet;
    pub mod watcher;
}

/// This crate's data files (see `friendslop_data`).
pub fn data_files() -> Vec<&'static dyn friendslop_data::DataFile> {
    vec![
        &fusion::compose::SCORE,
        &fusion::compose::SONGS,
        &fusion::compose::LIBRARY,
        &fusion::synth::MIX,
        &fusion::inst::INSTRUMENTS,
        &features::affair::TUNING,
        &features::ambience::TUNING,
        &features::discord_friends::TUNING,
        &features::gaslighting::TUNING,
        &features::hi::TUNING,
        &features::goat::TUNING,
        &features::infinite::TUNING,
        &features::rats::TUNING,
        &features::sesbian_lex::TUNING,
        &features::shipwreck::TUNING,
        &features::shipwreck::CREW,
        &features::discord_friends::LINES,
        &features::watcher::TUNING,
        &features::nyc::TUNING,
        &features::nyc::MAP,
        &features::nyc_traffic::TUNING,
        &features::fulton::TUNING,
        &features::fulton::TILES,
        &features::fulton_hunt::WRITING,
        &features::clues::TUNING,
        &features::clues::WRITING,
        &features::broadcast::TUNING,
        &features::broadcast::WRITING,
        &features::wallet::TUNING,
        &features::trains::TUNING,
        &features::miami::TUNING,
        &features::boba::TUNING,
        &features::penn::TUNING,
        &features::street_people::TUNING,
        &features::vessel_jump::TUNING,
        &features::pigeons::TUNING,
        &features::high_places::TUNING,
        &features::ice_cream::TUNING,
        &features::hats::TUNING,
        &features::kittens::TUNING,
        &features::toilets::TUNING,
        &features::emotes::TUNING,
        &features::kori::TUNING,
        &features::lily_garden::TUNING,
    ]
}
