//! friendSLOP game layer: The shared game layer: players, HUD, people, sound, settings, the shared rules vocabulary and the small features everything else reads.
//!
//! See "Crates" in AGENTS.md. Modules keep their old `crate::` paths:
//! the shared modules below are re-exported from the crates under this one.

pub use friendslop_art::{art, theme};
pub use friendslop_core::AppState;
pub use friendslop_kcc as kcc;
pub use friendslop_world::{buildings, landmarks, surfaces, terrain};

/// `bevy_voice_chat`'s prelude (built for the browser too, #106).
pub mod voice_chat {
    pub use bevy_voice_chat::prelude::*;
}

pub mod assets;
pub mod graphics;
pub mod hud;
pub mod people;
pub mod player;
pub mod rigged;
pub mod rules;
pub mod settings;
pub mod shared;
pub mod spatial;
pub mod textures;
pub mod voice;
#[cfg(target_arch = "wasm32")]
pub mod voice_web;
#[cfg(target_arch = "wasm32")]
pub mod web;
pub use shared::{camera, controls, scenery, ui, world, zones};

pub mod features {
    pub mod babble;
    pub mod bear;
    pub mod cat_palette;
    pub mod crows;
    pub mod dave_commune;
    pub mod great_storm;
    pub mod random_dialogue;
    pub mod stillness;
    pub mod synth_audio;
    pub mod voice_lines;
    pub mod whispers;
    pub mod world_seed;
}

/// This crate's data files (see `friendslop_data`).
pub fn data_files() -> Vec<&'static dyn friendslop_data::DataFile> {
    vec![
        &features::babble::TUNING,
        &features::bear::TUNING,
        &features::crows::TUNING,
        &features::great_storm::TUNING,
        &features::random_dialogue::TUNING,
        &features::stillness::TUNING,
        &features::voice_lines::TUNING,
        &features::whispers::TUNING,
        &features::random_dialogue::RANDOM_WORDS,
        &rules::villagers::NAMES,
    ]
}
