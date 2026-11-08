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

#[test]
fn eating_the_mushroom_changes_only_the_eaters_voice() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::mushroom::{EatMushroom, Mushroom, MushroomVoice};

    #[derive(Resource, Default)]
    struct Got(Vec<MushroomVoice>);

    let (mut server, mut client) = (peer(), peer());
    server.init_resource::<Got>();
    client.init_resource::<Got>();
    for app in [&mut server, &mut client] {
        app.world_mut()
            .add_observer(|v: On<MushroomVoice>, mut got: ResMut<Got>| got.0.push(*v));
    }
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    step(&mut server, &mut client);
    let world = client.world_mut();
    assert_eq!(world.query::<&Mushroom>().iter(world).count(), 1);

    client.world_mut().client_trigger(EatMushroom);
    step(&mut server, &mut client);

    let world = client.world_mut();
    assert_eq!(world.query::<&Mushroom>().iter(world).count(), 0, "eaten");
    let got = &client.world().resource::<Got>().0;
    assert_eq!(got.len(), 1, "the eater is told");
    assert!(got[0].pitch >= 1.5 || got[0].pitch <= 0.7);
    assert!(server.world().resource::<Got>().0.is_empty(), "host is not");
}
