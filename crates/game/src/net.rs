//! Peer-to-peer hosting over iroh: one player's game is the server (a listen
//! server), everyone else joins with a code. There is no dedicated server.
//!
//! iroh's `N0` preset does hole punching and falls back to free public relays,
//! so joining works across the internet with nothing but the host's code.

use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Mutex;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_replicon_iroh::iroh::endpoint::{EndpointHooks, presets};
use bevy_replicon_iroh::iroh::{EndpointAddr, EndpointId, SecretKey};
use bevy_replicon_iroh::{
    IrohClient, IrohClientConfig, IrohServer, IrohServerConfig, IrohTokioHandle,
    RepliconIrohPlugins,
};

use crate::AppState;

/// Ask the network layer to host or join.
#[derive(Message, Clone, Debug)]
pub enum NetCommand {
    Host,
    Join(String),
}

/// What the UI shows about the connection.
#[derive(Resource, Default, Debug)]
pub struct NetStatus {
    /// Set while hosting: friends paste this to join.
    pub join_code: Option<String>,
    /// Last human-readable status or error.
    pub message: String,
}

#[derive(Resource)]
struct TokioRuntime(#[allow(dead_code)] tokio::runtime::Runtime);

/// Kept alive so the copied join code stays on the clipboard (Linux drops it otherwise).
#[derive(Resource, Default)]
pub struct Clipboard(Mutex<Option<arboard::Clipboard>>);

impl Clipboard {
    pub fn set(&self, text: &str) {
        let Ok(mut guard) = self.0.lock() else { return };
        if guard.is_none() {
            *guard = arboard::Clipboard::new().ok();
        }
        if let Some(cb) = guard.as_mut()
            && let Err(e) = cb.set_text(text)
        {
            warn!("could not copy to clipboard: {e}");
        }
    }

    pub fn get(&self) -> Option<String> {
        let mut guard = self.0.lock().ok()?;
        if guard.is_none() {
            *guard = arboard::Clipboard::new().ok();
        }
        guard.as_mut()?.get_text().ok()
    }
}

#[derive(Debug, Clone, Copy)]
struct NoHooks;
impl EndpointHooks for NoHooks {}

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        let runtime = match tokio::runtime::Runtime::new() {
            Ok(rt) => rt,
            Err(e) => panic!("failed to start tokio runtime for networking: {e}"),
        };
        app.insert_resource(IrohTokioHandle(runtime.handle().clone()))
            .insert_resource(TokioRuntime(runtime))
            .add_plugins(RepliconIrohPlugins)
            .init_resource::<NetStatus>()
            .init_resource::<Clipboard>()
            .add_message::<NetCommand>()
            .add_systems(Update, handle_net_commands)
            .add_systems(OnEnter(ClientState::Connected), enter_game)
            .add_systems(OnExit(ClientState::Connected), back_to_menu);
    }
}

fn any_addr() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 0))
}

fn handle_net_commands(
    mut commands: Commands,
    mut requests: MessageReader<NetCommand>,
    handle: Res<IrohTokioHandle>,
    clipboard: Res<Clipboard>,
    mut status: ResMut<NetStatus>,
    mut next: ResMut<NextState<AppState>>,
) {
    for request in requests.read() {
        match request {
            NetCommand::Host => {
                match IrohServer::bind_with(
                    &handle,
                    presets::N0,
                    SecretKey::generate(),
                    any_addr(),
                    Vec::new(),
                    NoHooks,
                    IrohServerConfig::default(),
                ) {
                    Ok(server) => {
                        let code = server.id().to_string();
                        info!("hosting, join code: {code}");
                        clipboard.set(&code);
                        status.message = "Hosting! Join code copied to clipboard.".into();
                        status.join_code = Some(code);
                        commands.insert_resource(server);
                        next.set(AppState::InGame);
                    }
                    Err(e) => status.message = format!("Could not host: {e}"),
                }
            }
            NetCommand::Join(code) => {
                let id = match EndpointId::from_str(code.trim()) {
                    Ok(id) => id,
                    Err(e) => {
                        status.message = format!("That join code doesn't look right: {e}");
                        continue;
                    }
                };
                status.message = "Connecting...".into();
                // Blocks up to `connect_timeout` (15s). Fine for now; a feature
                // can move this to a background task if it gets annoying.
                match IrohClient::connect_with(
                    &handle,
                    presets::N0,
                    SecretKey::generate(),
                    any_addr(),
                    EndpointAddr::new(id),
                    Vec::new(),
                    NoHooks,
                    IrohClientConfig::default(),
                ) {
                    Ok(client) => {
                        commands.insert_resource(client);
                        status.message = "Connected!".into();
                    }
                    Err(e) => status.message = format!("Could not join: {e}"),
                }
            }
        }
    }
}

fn enter_game(mut next: ResMut<NextState<AppState>>) {
    next.set(AppState::InGame);
}

fn back_to_menu(mut next: ResMut<NextState<AppState>>, mut status: ResMut<NetStatus>) {
    status.message = "Disconnected.".into();
    next.set(AppState::Menu);
}
