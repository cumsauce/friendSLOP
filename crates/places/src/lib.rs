//! friendSLOP game layer: The warehouse, the underground and what happens in them: smoking, skills, farts, the helicopter.
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
    pub use friendslop_play::features::*;
    pub mod fart;
    pub mod fart_sound;
    pub mod helicopter;
    pub mod skills;
    pub mod smoking;
    pub mod underground;
    pub mod warehouse;
}

/// This crate's data files (see `friendslop_data`).
pub fn data_files() -> Vec<&'static dyn friendslop_data::DataFile> {
    vec![
        &features::fart::TUNING,
        &features::helicopter::TUNING,
        &features::skills::TUNING,
        &features::smoking::TUNING,
        &features::warehouse::TUNING,
        &features::warehouse::bathroom::TUNING,
        &features::warehouse::wfc::TILES,
        &features::warehouse::visuals::bathroom::TUNING,
    ]
}
