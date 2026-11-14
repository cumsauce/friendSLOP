//! friendSLOP game layer: NYC and the ideas that live there (Fulton St,
//! Penn, the trains, the hats).
//!
//! See "Crates" in AGENTS.md. Modules keep their old `crate::` paths:
//! the shared modules below are re-exported from the crates under this one.

#[allow(unused_imports)]
pub(crate) use friendslop_base::{
    AppState, art, assets, buildings, camera, controls, graphics, hud, input, kcc, landmarks,
    music, people, place, player, rigged, rules, scenery, settings, spatial, surfaces, terrain,
    textures, theme, ui, voice, voice_chat, world, zones,
};

pub mod features {
    #![allow(unused_imports)]
    pub use friendslop_base::features::*;
    pub use friendslop_places::features::*;
    pub use friendslop_play::features::*;
    pub use friendslop_story::features::*;
    pub use friendslop_village::features::*;
    pub mod boba;
    pub mod fulton;
    pub mod fulton_hunt;
    pub mod hats;
    pub mod high_places;
    pub mod ice_cream;
    pub mod jokah;
    pub mod miami;
    pub mod nyc;
    pub mod nyc_traffic;
    pub mod penn;
    pub mod street_people;
    pub mod toilets;
    pub mod trains;
    pub mod vessel;
    pub mod vessel_jump;
    pub mod wallet;
}

/// This crate's data files (see `friendslop_data`).
pub fn data_files() -> Vec<&'static dyn friendslop_data::DataFile> {
    vec![
        &features::nyc::TUNING,
        &features::nyc::MAP,
        &features::nyc_traffic::TUNING,
        &features::fulton::TUNING,
        &features::fulton::TILES,
        &features::fulton_hunt::WRITING,
        &features::wallet::TUNING,
        &features::trains::TUNING,
        &features::miami::TUNING,
        &features::boba::TUNING,
        &features::penn::TUNING,
        &features::street_people::TUNING,
        &features::vessel_jump::TUNING,
        &features::high_places::TUNING,
        &features::ice_cream::TUNING,
        &features::hats::TUNING,
        &features::toilets::TUNING,
        &features::jokah::TUNING,
    ]
}
