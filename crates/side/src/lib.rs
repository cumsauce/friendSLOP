//! friendSLOP game layer: Standalone ideas nothing else depends on.
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
    pub use friendslop_story::features::*;
    pub use friendslop_village::features::*;
    pub mod affair;
    pub mod ambience;
    pub mod broadcast;
    pub mod clues;
    pub mod discord_friends;
    pub mod fusion;
    pub mod gaslighting;
    pub mod goat;
    pub mod infinite;
    pub mod nyc;
    pub mod rats;
    pub mod sesbian_lex;
    pub mod shipwreck;
    pub mod watcher;
}

/// This crate's data files (see `friendslop_data`).
pub fn data_files() -> Vec<&'static dyn friendslop_data::DataFile> {
    vec![
        &fusion::compose::SCORE,
        &fusion::synth::MIX,
        &features::affair::TUNING,
        &features::ambience::TUNING,
        &features::discord_friends::TUNING,
        &features::gaslighting::TUNING,
        &features::goat::TUNING,
        &features::infinite::TUNING,
        &features::rats::TUNING,
        &features::sesbian_lex::TUNING,
        &features::shipwreck::TUNING,
        &features::shipwreck::CREW,
        &features::discord_friends::LINES,
        &features::watcher::TUNING,
        &features::nyc::TUNING,
        &features::clues::TUNING,
        &features::clues::WRITING,
        &features::broadcast::TUNING,
        &features::broadcast::WRITING,
    ]
}
