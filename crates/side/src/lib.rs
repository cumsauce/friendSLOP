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
    pub mod discord_friends;
    pub mod fusion;
    pub mod gaslighting;
    pub mod goat;
    pub mod infinite;
    pub mod rats;
    pub mod sesbian_lex;
    pub mod shipwreck;
}
