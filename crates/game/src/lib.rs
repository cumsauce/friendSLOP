//! friendSLOP: a multiplayer game designed one GitHub issue at a time.
//!
//! See `AGENTS.md` at the repo root before changing anything.

use bevy::prelude::*;

pub mod ambience;
pub mod art;
pub mod assets;
pub mod atmosphere;
pub mod buildings;
pub mod camera;
pub mod controls;
pub mod creator;
pub mod features;
pub mod footsteps;
pub mod graphics;
pub mod hud;
pub mod kcc;
pub mod landmarks;
pub mod loading;
pub mod menu;
// The browser build has no iroh transport yet: same API, solo play (#106).
#[cfg_attr(target_arch = "wasm32", path = "net_web.rs")]
pub mod net;
pub mod people;
pub mod player;
pub mod scenery;
pub mod settings;
pub mod terrain;
pub mod theme;
pub mod voice;
#[cfg(target_arch = "wasm32")]
pub mod web;
pub mod world;
pub mod zones;

/// `bevy_voice_chat`'s prelude natively; inert stand-ins in the browser (#106).
pub mod voice_chat {
    #[cfg(target_arch = "wasm32")]
    pub use crate::web::voice_chat::*;
    #[cfg(not(target_arch = "wasm32"))]
    pub use bevy_voice_chat::prelude::*;
}

/// Top-level flow. Add states here only if a feature truly needs a new screen.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppState {
    #[default]
    Menu,
    InGame,
}

/// Everything that must be registered **identically on every peer**: replicated
/// components, network messages and events. Order matters (protocol hash).
/// Headless tests use this without any window, audio or transport.
pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((player::PlayerPlugin, features::FeaturesProtocolPlugin));
    }
}

/// The full game. Expects `DefaultPlugins`, `RepliconPlugins` and avian's
/// `PhysicsPlugins` to be added first (see `main.rs`).
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .add_plugins((
                net::NetPlugin,
                ProtocolPlugin,
                voice::VoicePlugin,
                settings::SettingsPlugin,
                menu::MenuPlugin,
                assets::plugin,
                world::WorldPlugin,
                terrain::TerrainPlugin,
                atmosphere::plugin,
                camera::CameraPlugin,
                features::FeaturesPlugin,
                hud::HudPlugin,
                scenery::plugin,
                ambience::plugin,
                loading::plugin,
            ))
            .add_plugins((
                landmarks::plugin,
                zones::plugin,
                buildings::plugin,
                creator::plugin,
                footsteps::plugin,
                theme::plugin,
            ));
        app.add_plugins((controls::plugin, graphics::plugin));
    }
}
