//! Headless: a client climbs down a manhole into the tunnels (#127), walks
//! (is moved) to another way in underground and climbs up there.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::fps::Walker;
use friendslop::features::underground::{ENTRANCES, UseHatch, below, down_spot, up_spot};
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

fn remote_body(app: &mut App) -> Vec3 {
    let world = app.world_mut();
    world
        .query::<(&Player, &Walker)>()
        .iter(world)
        .find(|(p, _)| p.id != 0)
        .map(|(_, w)| w.pos)
        .unwrap_or(Vec3::NAN)
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
fn down_one_manhole_and_up_another_way() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);

    // Too far from the manhole: nothing happens.
    put_remote_body(&mut server, Vec3::ZERO);
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(UseHatch { id: 0 });
    exchange(&mut server, &mut client, 3);
    assert!(!below(remote_body(&mut client)));

    // Beside it: down you go, to the foot of its ladder.
    let top = up_spot(0);
    put_remote_body(&mut server, top);
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(UseHatch { id: 0 });
    exchange(&mut server, &mut client, 3);
    let here = remote_body(&mut client);
    assert!(below(here), "{here}");
    assert!(here.xz().distance(down_spot(0).xz()) < 0.5, "{here}");

    // Underground, the cellar's ladder is out of reach from here...
    client.world_mut().client_trigger(UseHatch { id: 2 });
    exchange(&mut server, &mut client, 3);
    assert!(below(remote_body(&mut client)));

    // ...until you get to it: then you come up in the village.
    put_remote_body(&mut server, down_spot(2));
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(UseHatch { id: 2 });
    exchange(&mut server, &mut client, 3);
    let up = remote_body(&mut client);
    assert!(!below(up), "{up}");
    assert!(up.xz().distance(ENTRANCES[2].at) < 2.0, "{up}");
}
