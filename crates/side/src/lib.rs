//! friendSLOP game layer: Standalone ideas nothing else depends on.
//!
//! See "Crates" in AGENTS.md. Modules keep their old `crate::` paths:
//! the shared modules below are re-exported from the crates under this one.

#[allow(unused_imports)]
pub(crate) use friendslop_base::{
    AppState, areas, art, assets, buildings, camera, controls, graphics, hud, input, kcc,
    landmarks, music, people, place, player, rigged, rules, scenery, settings, sites, sound,
    spatial, surfaces, terrain, textures, theme, ui, voice, voice_chat, world, zones,
};

pub mod features {
    #![allow(unused_imports)]
    pub use friendslop_base::features::*;
    pub use friendslop_places::features::*;
    pub use friendslop_play::features::*;
    pub use friendslop_story::features::*;
    pub use friendslop_village::features::*;
    // The NYC ideas moved to their own crate; the names stay here so
    // `super::fulton::` and `friendslop_side::features::trains` keep resolving.
    pub use friendslop_side_nyc::features::*;
    pub mod achievements;
    pub mod affair;
    pub mod ambience;
    pub mod broadcast;
    pub mod clues;
    pub mod discord_friends;
    pub mod emotes;
    pub mod fan_museum;
    pub mod fusion;
    pub mod gaslighting;
    pub mod goat;
    pub mod hi;
    pub mod infinite;
    pub mod kittens;
    pub mod kori;
    pub mod lily_garden;
    pub mod organ_butcher;
    pub mod pigeons;
    pub mod rats;
    pub mod sesbian_lex;
    pub mod shipwreck;
    pub mod surf;
    pub mod surf_mall;
    pub mod surf_map;
    pub mod surf_office;
    pub mod surf_pillars;
    pub mod surf_props;
    pub mod surf_runs;
    pub mod surf_slalom;
    pub mod surf_tower;
    pub mod surf_train;
    pub mod wastes;
    pub mod watcher;
}

/// This crate's data files (see `friendslop_data`). The NYC ones live in
/// `friendslop_side_nyc` (its own `data_files()`).
pub fn data_files() -> Vec<&'static dyn friendslop_data::DataFile> {
    vec![
        &fusion::compose::SCORE,
        &fusion::compose::SONGS,
        &fusion::compose::LIBRARY,
        &fusion::synth::MIX,
        &fusion::inst::INSTRUMENTS,
        &features::affair::TUNING,
        &features::surf_map::MAP,
        &features::surf_train::TRAIN,
        &features::ambience::TUNING,
        &features::discord_friends::TUNING,
        &features::gaslighting::TUNING,
        &features::hi::TUNING,
        &features::goat::TUNING,
        &features::fan_museum::MAP,
        &features::fan_museum::TUNING,
        &features::infinite::TUNING,
        &features::organ_butcher::TUNING,
        &features::rats::TUNING,
        &features::sesbian_lex::TUNING,
        &features::shipwreck::TUNING,
        &features::shipwreck::CREW,
        &features::discord_friends::LINES,
        &features::watcher::TUNING,
        &features::clues::TUNING,
        &features::clues::WRITING,
        &features::broadcast::TUNING,
        &features::broadcast::WRITING,
        &features::pigeons::TUNING,
        &features::kittens::TUNING,
        &features::emotes::TUNING,
        &features::kori::TUNING,
        &features::lily_garden::TUNING,
        &features::achievements::TUNING,
    ]
}
