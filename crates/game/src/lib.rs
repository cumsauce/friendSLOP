//! friendSLOP: a multiplayer game designed one GitHub issue at a time.
//!
//! See `AGENTS.md` at the repo root before changing anything.

use bevy::prelude::*;

pub mod features;
pub mod menu;
pub mod net;
pub mod player;
pub mod settings;
pub mod voice;
pub mod world;

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
        app.init_state::<AppState>().add_plugins((
            net::NetPlugin,
            ProtocolPlugin,
            voice::VoicePlugin,
            settings::SettingsPlugin,
            menu::MenuPlugin,
            world::WorldPlugin,
            features::FeaturesPlugin,
        ));
    }
}
