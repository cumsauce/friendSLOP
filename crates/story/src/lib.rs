//! friendSLOP game layer: The campaign and the features that tie the game together.
//!
//! See "Crates" in AGENTS.md. Modules keep their old `crate::` paths:
//! the shared modules below are re-exported from the crates under this one.

#[allow(unused_imports)]
pub(crate) use friendslop_base::{
    AppState, art, assets, buildings, camera, controls, graphics, hud, kcc, landmarks, music,
    people, player, rigged, rules, scenery, settings, spatial, surfaces, terrain, textures, theme,
    ui, voice, voice_chat, world, zones,
};

pub mod features {
    #![allow(unused_imports)]
    pub use friendslop_base::features::*;
    pub use friendslop_places::features::*;
    pub use friendslop_play::features::*;
    pub use friendslop_village::features::*;
    pub mod abuela;
    pub mod campaign;
    pub mod gamble;
    pub mod gods_tears;
    pub mod halt;
    pub mod landlord;
    pub mod mushroom;
    pub mod objective;
    pub mod pets;
    pub mod totems;
}

/// This crate's data files (see `friendslop_data`).
pub fn data_files() -> Vec<&'static dyn friendslop_data::DataFile> {
    vec![
        &features::abuela::TUNING,
        &features::gamble::TUNING,
        &features::gods_tears::TUNING,
        &features::halt::TUNING,
        &features::landlord::TUNING,
        &features::mushroom::TUNING,
        &features::pets::TUNING,
        &features::campaign::CHAPTERS,
        &features::abuela::LINES,
    ]
}
