//! Headless: a client walks up to Marmalade's wheel and spins it (#173). The
//! spin is counted, the house odds every peer sees follow the server, and
//! spinning from far away or too soon again does nothing.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::fate::FateDice;
use friendslop::features::fps::Walker;
use friendslop::features::gamble::{Gambler, HouseOdds, SpinWheel, gamble_at};
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

/// The remote client's record, as the client sees it.
fn remote_spins(app: &mut App) -> u32 {
    let world = app.world_mut();
    world
        .query::<(&Player, &Gambler)>()
        .iter(world)
        .filter(|(p, _)| p.id != 0)
        .map(|(_, g)| g.spins)
        .sum()
}

fn odds(app: &mut App) -> Option<HouseOdds> {
    let world = app.world_mut();
    world.query::<&HouseOdds>().iter(world).next().copied()
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
fn spinning_marmalades_wheel() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);
    server.world_mut().resource_mut::<FateDice>().chance = 0.0;
    assert_eq!(odds(&mut client), Some(HouseOdds::default()));

    // Too far away: nothing happens.
    put_remote_body(&mut server, Vec3::new(30.0, 0.0, 30.0));
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(SpinWheel);
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote_spins(&mut client), 0);

    // At the table: one spin counts, the second (too soon) doesn't.
    put_remote_body(
        &mut server,
        Vec3::new(gamble_at().x, 0.0, gamble_at().y - 1.5),
    );
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(SpinWheel);
    client.world_mut().client_trigger(SpinWheel);
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote_spins(&mut client), 1);
    // Everyone sees the server's odds.
    assert_eq!(odds(&mut client), odds(&mut server));
}
