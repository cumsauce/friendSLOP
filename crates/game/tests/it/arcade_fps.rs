//! Headless: a client walks up to the FRAG cabinet and the host moves their
//! body into the arena, between spawn points, and back out to the cabinet.
//! Far from the cabinet, or outside the arena, the requests do nothing.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::arcade_fps::{ArcadeFps, cabinets, in_arena, level};
use friendslop::features::fps::Walker;
use friendslop::player::Player;

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

fn exchange(server: &mut App, client: &mut App, n: usize) {
    for _ in 0..n {
        server.update();
        server.exchange_with_client(client);
        client.update();
        server.exchange_with_client(client);
    }
}

/// The remote player's body on the server.
fn remote_body(server: &mut App) -> Option<Vec3> {
    let world = server.world_mut();
    world
        .query::<(&Player, &Walker)>()
        .iter(world)
        .find(|(p, _)| p.id != 0)
        .map(|(_, w)| w.pos)
}

fn put_remote_body(server: &mut App, at: Vec3) {
    let world = server.world_mut();
    for (p, mut w) in world.query::<(&Player, &mut Walker)>().iter_mut(world) {
        if p.id != 0 {
            w.pos = at;
        }
    }
}

#[test]
fn walking_into_the_frag_machine_and_back() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);
    let (_, cabinet) = cabinets()
        .first()
        .copied()
        .expect("a FRAG cabinet on the map");
    let level = level();

    // Too far from the cabinet: nothing happens.
    let far = cabinet + Vec3::new(0.0, 0.0, -20.0);
    put_remote_body(&mut server, far);
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(ArcadeFps::Enter);
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote_body(&mut server), Some(far), "still where it was");

    // At the cabinet: in you go, to the first spawn point.
    let front = cabinet + Vec3::new(0.0, 0.0, -1.0);
    put_remote_body(&mut server, front);
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(ArcadeFps::Enter);
    exchange(&mut server, &mut client, 3);
    let inside = remote_body(&mut server).expect("a remote body");
    assert!(in_arena(inside), "{inside:?} is in the arena");
    assert_eq!(inside, level.spawns[0].0);

    // A respawn moves you to the spawn point asked for.
    client.world_mut().client_trigger(ArcadeFps::Respawn(3));
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote_body(&mut server), Some(level.spawns[3].0));

    // Out: back to where you stood.
    client.world_mut().client_trigger(ArcadeFps::Exit);
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote_body(&mut server), Some(front));

    // Outside the arena, respawn and exit do nothing.
    client.world_mut().client_trigger(ArcadeFps::Respawn(1));
    client.world_mut().client_trigger(ArcadeFps::Exit);
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote_body(&mut server), Some(front));
}
