//! P0 multiplayer lifecycle tests: disconnects, reconnects, host teardown,
//! and same-tick race sanity. Headless listen-server + clients, no sockets.
//!
//! These start out self-contained (mirroring `replication.rs`) and are moved
//! onto `harness::Sim` as that harness lands.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::player::Player;
use friendslop::ProtocolPlugin;

fn peer() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        RepliconPlugins.set(ServerPlugin::new(PostUpdate)),
        ProtocolPlugin,
    ));
    app.finish();
    app
}

fn player_ids(app: &mut App) -> Vec<u64> {
    let world = app.world_mut();
    let mut q = world.query::<&Player>();
    let mut ids: Vec<u64> = q.iter(world).map(|p| p.id).collect();
    ids.sort_unstable();
    ids
}

/// Pump server + two clients for a few exchange rounds.
fn exchange3(server: &mut App, c0: &mut App, c1: &mut App, rounds: usize) {
    for _ in 0..rounds {
        server.update();
        server.exchange_with_client(c0);
        c0.update();
        server.exchange_with_client(c1);
        c1.update();
    }
}

/// Disconnecting a client despawns its Player on the server and on the
/// surviving peer; the host's Player (id 0) survives.
#[test]
fn disconnect_despawns_leaver_keeps_host() {
    let (mut server, mut c0, mut c1) = (peer(), peer(), peer());
    server.connect_client(&mut c0);
    server.connect_client(&mut c1);
    exchange3(&mut server, &mut c0, &mut c1, 3);

    let before = player_ids(&mut server);
    assert_eq!(before.len(), 3, "host + two clients on the server");

    server.disconnect_client(&mut c1);
    exchange3(&mut server, &mut c0, &mut c1, 3);

    let after = player_ids(&mut server);
    assert!(after.contains(&0), "host Player id 0 survives");
    let removed: Vec<u64> = before.iter().filter(|id| !after.contains(id)).copied().collect();
    assert_eq!(removed.len(), 1, "exactly the leaver's Player despawned");
    assert!(!removed.contains(&0), "the host is not the leaver");

    let peer_ids = player_ids(&mut c0);
    assert_eq!(peer_ids, after, "surviving peer agrees with the server");
    assert!(!peer_ids.contains(&removed[0]), "leaver is gone on the peer");
}
