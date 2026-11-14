//! One `Player` entity per connected person, spawned by the server and
//! replicated to everyone. Deliberately empty: what a player *is* (body,
//! controls, stats...) is up to the issues.

use crate::voice_chat::{HostVoiceSpeaker, SpeakerId, VoiceSenderId};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
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
            .add_systems(OnEnter(ServerState::Running), spawn_host_player)
            .add_systems(OnExit(ClientState::Connected), despawn_all_players)
            .add_systems(
                OnExit(ServerState::Running),
                (clear_host_voice, despawn_all_players),
            );
    }
}

fn spawn_host_player(mut commands: Commands) {
    commands.insert_resource(HostVoiceSpeaker(Some(SpeakerId(0))));
    commands.insert_resource(LocalPlayer(0));
    commands.spawn((Player { id: 0 }, PlayerOwner(None), Replicated));
}

/// Stop hosting: the host's voice speaker is session state and must not
/// outlive it (it would keep tagging voice as the host's in the next one).
fn clear_host_voice(mut commands: Commands) {
    commands.insert_resource(HostVoiceSpeaker(None));
}

fn spawn_client_player(
    add: On<Add<AuthorizedClient>>,
    mut commands: Commands,
    mut next: ResMut<NextPlayerId>,
) {
    next.0 += 1;
    let id = next.0;
    let owner = add.entity;
    commands.entity(owner).insert(VoiceSenderId(SpeakerId(id)));
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(ClientId::Client(owner)),
        message: Welcome { id },
    });
    // Deferred to flush time and skipped if the owner is gone by then: an
    // Add+Remove of `AuthorizedClient` in one tick would otherwise leave a
    // permanent ghost `Player` (the `Remove` observer's query can't see this
    // spawn yet).
    commands.queue(move |world: &mut World| {
        spawn_player_for(world, owner, id);
    });
}

/// The `Player` for one authorized client, spawned only while that client is
/// still around (called from [`spawn_client_player`]'s deferred command).
fn spawn_player_for(world: &mut World, owner: Entity, id: u64) {
    if world.get::<AuthorizedClient>(owner).is_none() {
        return;
    }
    world.spawn((Player { id }, PlayerOwner(Some(owner)), Replicated));
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

/// Session over (however it ended): no `Player` may outlive it, or the next
/// session counts and tags ghosts.
fn despawn_all_players(mut commands: Commands, players: Query<Entity, With<Player>>) {
    for e in &players {
        commands.entity(e).despawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_player_spawns_while_its_owner_is_authorized() {
        let mut world = World::new();
        let owner = world.spawn(AuthorizedClient).id();
        spawn_player_for(&mut world, owner, 7);
        let mut players = world.query::<(&Player, &PlayerOwner)>();
        let spawned: Vec<(u64, Option<Entity>)> =
            players.iter(&world).map(|(p, o)| (p.id, o.0)).collect();
        assert_eq!(spawned, vec![(7, Some(owner))]);
    }

    #[test]
    fn no_player_lingers_when_the_owner_leaves_before_the_spawn_applies() {
        let mut world = World::new();
        // The same-tick race: `Add<AuthorizedClient>` queued the spawn, then
        // `Remove<AuthorizedClient>` fired before the command flush.
        let owner = world.spawn_empty().id();
        spawn_player_for(&mut world, owner, 7);
        let mut players = world.query::<&Player>();
        assert_eq!(players.iter(&world).count(), 0);

        // A despawned client entity counts as gone too.
        let owner = world.spawn(AuthorizedClient).id();
        world.despawn(owner);
        spawn_player_for(&mut world, owner, 7);
        assert_eq!(players.iter(&world).count(), 0);
    }
}
