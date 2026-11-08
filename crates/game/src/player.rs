//! One `Player` entity per connected person, spawned by the server and
//! replicated to everyone. Deliberately empty: what a player *is* (body,
//! controls, stats...) is up to the issues.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use bevy_voice_chat::prelude::{HostVoiceSpeaker, SpeakerId, VoiceSenderId};
use serde::{Deserialize, Serialize};

/// A person in the session. `id` 0 is the host; it doubles as the voice speaker id.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[require(Transform)]
pub struct Player {
    pub id: u64,
}

/// Server-only: which connection (replicon client entity) owns this player.
/// `None` for the host's own player.
#[derive(Component, Debug)]
pub struct PlayerOwner(pub Option<Entity>);

#[derive(Resource, Default)]
struct NextPlayerId(u64);

/// Which `Player { id }` is "me" on this peer. Set when hosting, or when the
/// server's `Welcome` arrives on a client.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalPlayer(pub u64);

/// Server -> one client: "you are player `id`".
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Welcome {
    pub id: u64,
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.replicate::<Player>()
            .init_resource::<NextPlayerId>()
            .add_observer(spawn_client_player)
            .add_observer(despawn_client_player)
            .add_server_event::<Welcome>(Channel::Ordered)
            .add_observer(|w: On<Welcome>, mut commands: Commands| {
                commands.insert_resource(LocalPlayer(w.id));
            })
            .add_systems(OnEnter(ServerState::Running), spawn_host_player);
    }
}

fn spawn_host_player(mut commands: Commands) {
    commands.insert_resource(HostVoiceSpeaker(Some(SpeakerId(0))));
    commands.insert_resource(LocalPlayer(0));
    commands.spawn((Player { id: 0 }, PlayerOwner(None), Replicated));
}

fn spawn_client_player(
    add: On<Add<AuthorizedClient>>,
    mut commands: Commands,
    mut next: ResMut<NextPlayerId>,
) {
    next.0 += 1;
    let id = next.0;
    commands
        .entity(add.entity)
        .insert(VoiceSenderId(SpeakerId(id)));
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(ClientId::Client(add.entity)),
        message: Welcome { id },
    });
    commands.spawn((Player { id }, PlayerOwner(Some(add.entity)), Replicated));
}

fn despawn_client_player(
    remove: On<Remove<AuthorizedClient>>,
    mut commands: Commands,
    players: Query<(Entity, &PlayerOwner)>,
) {
    for (entity, owner) in &players {
        if owner.0 == Some(remove.entity) {
            commands.entity(entity).despawn();
        }
    }
}
