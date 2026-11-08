//! Headless: a listen server and a client agree on who is playing.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
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

fn player_count(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&Player>().iter(world).count()
}

#[test]
fn host_and_client_players_replicate() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    assert_eq!(
        player_count(&mut server),
        2,
        "host + one client on the server"
    );
    assert_eq!(player_count(&mut client), 2, "client sees both players");
}

#[test]
fn everyone_gets_an_organ_purse() {
    use friendslop::features::organ_currency::{OrganPurse, starting_purse};
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let purses: Vec<OrganPurse> = world
        .query_filtered::<&OrganPurse, With<Player>>()
        .iter(world)
        .cloned()
        .collect();
    assert_eq!(purses.len(), 2, "client sees both players' purses");
    assert!(purses.iter().all(|p| *p == starting_purse()));
}

#[test]
fn crows_replicate_to_clients() {
    use friendslop::features::crows::{CROW_COUNT, Crow};
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let n = world.query::<&Crow>().iter(world).count();
    assert_eq!(n, CROW_COUNT as usize, "client sees every crow");
}

#[test]
fn exactly_one_bear_replicates() {
    use friendslop::features::bear::Bear;
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let n = world.query::<&Bear>().iter(world).count();
    assert_eq!(n, 1, "just one bear");
}
