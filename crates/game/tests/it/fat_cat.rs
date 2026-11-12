//! Headless: everyone gets a fat cat (#145) that grows while its owner isn't
//! looking at it and stops growing once they are.

use std::time::Duration;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::fat_cat::{CatInView, FatCat};

fn peer() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        RepliconPlugins.set(ServerPlugin::new(PostUpdate)),
        ProtocolPlugin,
    ));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        100,
    )));
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

fn cats(app: &mut App) -> Vec<FatCat> {
    let world = app.world_mut();
    let mut all: Vec<FatCat> = world.query::<&FatCat>().iter(world).copied().collect();
    all.sort_by_key(|c| c.owner);
    all
}

#[test]
fn cats_grow_unwatched_and_stop_when_seen() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 5);
    let seen_by_client = cats(&mut client);
    assert_eq!(seen_by_client.len(), 2, "host and client each have a cat");

    // Nobody has looked yet: both cats grow.
    let before = cats(&mut server);
    exchange(&mut server, &mut client, 20);
    let after = cats(&mut server);
    assert!(after[1].size > before[1].size, "unwatched cat grows");

    // The client looks at its cat: it stops growing, the host's keeps going.
    client.world_mut().client_trigger(CatInView { seen: true });
    exchange(&mut server, &mut client, 2);
    let watched = cats(&mut server);
    exchange(&mut server, &mut client, 20);
    let later = cats(&mut server);
    assert_eq!(later[1].size, watched[1].size, "watched cat holds still");
    assert!(later[0].size > watched[0].size, "host's cat still grows");
    assert_eq!(cats(&mut client)[1].size, later[1].size, "size replicates");
}
