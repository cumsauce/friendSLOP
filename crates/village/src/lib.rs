//! friendSLOP game layer: Villagers and the trouble you get into with them.
//!
//! See "Crates" in AGENTS.md. Modules keep their old `crate::` paths:
//! the shared modules below are re-exported from the crates under this one.

#[allow(unused_imports)]
pub(crate) use friendslop_base::{
    AppState, art, assets, buildings, camera, controls, graphics, hud, kcc, landmarks, people,
    player, rigged, rules, scenery, settings, spatial, surfaces, terrain, textures, theme, ui,
    voice, voice_chat, world, zones,
};

pub mod features {
    #![allow(unused_imports)]
    pub use friendslop_base::features::*;
    pub use friendslop_places::features::*;
    pub use friendslop_play::features::*;
    pub mod caught;
    pub mod hide_and_seek;
    pub mod morality;
    pub mod organ_currency;
    pub mod punishment;
    pub mod romance;
    pub mod villagers;
    pub mod wendys;
}

/// This crate's data files (see `friendslop_data`).
pub fn data_files() -> Vec<&'static dyn friendslop_data::DataFile> {
    vec![
        &features::caught::TUNING,
        &features::hide_and_seek::TUNING,
        &features::punishment::TUNING,
        &features::romance::TUNING,
        &features::villagers::TUNING,
        &features::wendys::TUNING,
    ]
}
