//! Headless: a wicked player gets a helicopter after them (seen by everyone),
//! and it leaves once they're good again.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::helicopter::{HUNTED_AT, Helicopter};
use friendslop::features::morality::Morality;
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

fn set_morality(app: &mut App, id: u64, value: i32) {
    let world = app.world_mut();
    for (p, mut m) in world.query::<(&Player, &mut Morality)>().iter_mut(world) {
        if p.id == id {
            m.0 = value;
        }
    }
}

fn helis(app: &mut App) -> Vec<u64> {
    let world = app.world_mut();
    world
        .query::<&Helicopter>()
        .iter(world)
        .map(|h| h.target)
        .collect()
}

#[test]
fn wicked_players_get_a_helicopter_that_leaves_when_they_are_good() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);
    assert!(helis(&mut client).is_empty());

    set_morality(&mut server, 0, HUNTED_AT);
    exchange(&mut server, &mut client, 3);
    assert_eq!(helis(&mut client), vec![0]);
    // Only one, however long they stay wicked.
    exchange(&mut server, &mut client, 3);
    assert_eq!(helis(&mut server), vec![0]);

    // Redeemed: it heads home (and despawns once there).
    set_morality(&mut server, 0, 0);
    let world = server.world_mut();
    for mut h in world.query::<&mut Helicopter>().iter_mut(world) {
        h.pos = Vec3::new(-150.0, 40.0, -150.0);
    }
    exchange(&mut server, &mut client, 3);
    assert!(helis(&mut server).is_empty());
    assert!(helis(&mut client).is_empty());
}
