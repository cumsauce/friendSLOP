//! friendSLOP: a multiplayer game designed one GitHub issue at a time.
//!
//! See `AGENTS.md` at the repo root before changing anything.

use bevy::prelude::*;

pub mod ambience;
pub use friendslop_art::art;
pub use friendslop_base::assets;
pub mod atmosphere;
pub use friendslop_world::buildings;
pub mod camera;
pub mod controls;
pub mod creator;
pub mod fault;
pub mod features;
pub mod footsteps;
pub mod gz_assets;
pub use friendslop_base::graphics;
pub use friendslop_base::hud;
pub use friendslop_kcc as kcc;
pub use friendslop_world::landmarks;
pub mod loading;
pub mod lod;
pub mod menu;
pub use friendslop_base::rigged;
// The browser has its own iroh transport (relay-only, async), same API (#106).
#[cfg_attr(target_arch = "wasm32", path = "net_web.rs")]
pub mod net;
pub mod probes;
pub use friendslop_base::people;
pub use friendslop_base::player;
/// `bevy_replicon_iroh`'s wire format, shared by the browser transport.
pub use friendslop_wire as net_wire;
pub mod scenery;
/// Screenshot mode for Discord posts (#167); off unless `FRIENDSLOP_SCREENSHOT` is set.
pub mod screenshot;
pub use friendslop_art::theme;
pub use friendslop_base::settings;
pub use friendslop_base::spatial;
pub use friendslop_base::textures;
pub use friendslop_world::surfaces;
pub use friendslop_world::terrain;
pub mod ui;
pub use friendslop_base::voice;
/// Voice chat audio through Web Audio (#166).
#[cfg(target_arch = "wasm32")]
pub use friendslop_base::voice_web;
#[cfg(target_arch = "wasm32")]
pub use friendslop_base::web;
pub mod world;
pub mod zones;
pub use friendslop_base::rules;

pub use friendslop_base::voice_chat;

/// Top-level flow (in `friendslop-core` so every crate can use it).
pub use friendslop_core::AppState;

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
            .add_plugins(probes::plugin)
            .add_plugins((
                landmarks::plugin,
                zones::plugin,
                buildings::plugin,
                creator::plugin,
                footsteps::plugin,
                spatial::plugin,
                theme::plugin,
                friendslop_world::map::MapPlugin,
                friendslop_world::holes::HolesPlugin,
            ));
        // Every data file the game reads (maps, tuning, writing): loaded at
        // run time, re-read when edited (see `friendslop_data`).
        app.add_plugins(friendslop_data::DataPlugin {
            files: data_files(),
        });
        app.add_plugins((
            controls::plugin,
            graphics::plugin,
            surfaces::plugin,
            fault::plugin,
            ui::plugin,
            textures::plugin,
            lod::plugin,
            rigged::plugin,
            // Live wind sway for the rigged trees (lod.rs steers it).
            modeler_bevy::ModelerPlugin,
        ));
    }
}

/// Every data file the game reads, from every crate.
pub fn data_files() -> Vec<&'static dyn friendslop_data::DataFile> {
    let mut files = friendslop_world::data_files();
    files.extend(friendslop_base::data_files());
    files.extend(friendslop_play::data_files());
    files.extend(friendslop_places::data_files());
    files.extend(friendslop_village::data_files());
    files.extend(friendslop_story::data_files());
    files.extend(friendslop_afterlife::data_files());
    files.extend(friendslop_side::data_files());
    files.push(&atmosphere::DAY_NIGHT);
    files.push(&probes::TUNING);
    files.push(&controls::PAD);
    files
}
