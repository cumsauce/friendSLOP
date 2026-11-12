//! Headless: a client walks up to Wendy's counter and picks up a human meat
//! burger (#117); from far away it can't.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::fate::FateDice;
use friendslop::features::fps::Walker;
use friendslop::features::wendys::{Burger, BurgerBag, SLOTS, TakeBurger, burger_spot};
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

fn burgers(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&Burger>().iter(world).count()
}

/// The remote client's (non-host) bag, as the client sees it.
fn remote_bag(app: &mut App) -> u32 {
    let world = app.world_mut();
    world
        .query::<(&Player, &BurgerBag)>()
        .iter(world)
        .filter(|(p, _)| p.id != 0)
        .map(|(_, b)| b.0)
        .sum()
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
fn picking_up_a_burger_from_the_counter() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);
    server.world_mut().resource_mut::<FateDice>().chance = 0.0;
    assert_eq!(burgers(&mut client), SLOTS as usize);
    assert_eq!(remote_bag(&mut client), 0);

    // Too far away: nothing happens.
    put_remote_body(&mut server, Vec3::ZERO);
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(TakeBurger { slot: 1 });
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote_bag(&mut client), 0);
    assert_eq!(burgers(&mut client), SLOTS as usize);

    // At the counter: it's yours, once.
    let spot = burger_spot(1);
    put_remote_body(&mut server, Vec3::new(spot.x, 0.0, spot.z - 1.0));
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(TakeBurger { slot: 1 });
    client.world_mut().client_trigger(TakeBurger { slot: 1 });
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote_bag(&mut client), 1);
    assert_eq!(burgers(&mut client), SLOTS as usize - 1);
}
