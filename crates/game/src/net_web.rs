//! Browser stand-in for `net.rs` (#106). Same public API, no transport.
//!
//! `bevy_replicon_iroh` drives iroh through a tokio runtime and `block_on`s
//! every tick, which a browser's single main thread can't do, so the web build
//! can't host or join yet. "Host" starts a solo session instead (this peer is
//! the server with nobody else connected, exactly like a host before friends
//! join), so the whole world is still there to walk around in.

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::AppState;

/// Shown wherever the menu would say how hosting/joining went.
pub const WEB_NOTE: &str =
    "Browser build: multiplayer and voice need the desktop game for now. Playing solo.";

/// Ask the network layer to host or join.
#[derive(Message, Clone, Debug)]
pub enum NetCommand {
    Host,
    Join(String),
}

/// What the UI shows about the connection.
#[derive(Resource, Default, Debug)]
pub struct NetStatus {
    pub join_code: Option<String>,
    pub message: String,
}

/// No clipboard access from the game in the browser (it needs a user gesture
/// and is async); the menu's paste/copy just do nothing.
#[derive(Resource, Default)]
pub struct Clipboard;

impl Clipboard {
    pub fn set(&self, _text: &str) {}
    pub fn get(&self) -> Option<String> {
        None
    }
}

/// Never inserted on the web; only here so `Option<Res<IrohClient>>` compiles.
#[derive(Resource)]
pub struct IrohClient;

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(NetStatus {
            join_code: None,
            message: WEB_NOTE.into(),
        })
        .init_resource::<Clipboard>()
        .add_message::<NetCommand>()
        .add_systems(Update, handle_net_commands);
    }
}

/// Stop the solo "server". No-op if it isn't running.
pub fn teardown_transport(world: &mut World) {
    if let Some(mut next) = world.get_resource_mut::<NextState<ServerState>>() {
        next.set(ServerState::Stopped);
    }
}

fn handle_net_commands(
    mut requests: MessageReader<NetCommand>,
    mut status: ResMut<NetStatus>,
    mut server: ResMut<NextState<ServerState>>,
    mut next: ResMut<NextState<AppState>>,
) {
    for request in requests.read() {
        match request {
            NetCommand::Host => {
                server.set(ServerState::Running);
                status.message = WEB_NOTE.into();
                next.set(AppState::InGame);
            }
            NetCommand::Join(_) => {
                status.message =
                    "Joining isn't possible in the browser yet: use the desktop game.".into();
            }
        }
    }
}
