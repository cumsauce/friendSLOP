//! Headless: a fat cat goes and drinks a fresh pee puddle (#213), and the
//! puddle is gone once it's done.

use std::time::Duration;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::fat_cat::{FatCat, Lapping};
use friendslop::features::skills::{PeePuddle, Piss};

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

fn exchange(server: &mut App, client: &mut App) {
    server.update();
    server.exchange_with_client(client);
    client.update();
    server.exchange_with_client(client);
}

fn puddles(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&PeePuddle>().iter(world).count()
}

fn lapping(app: &mut App) -> bool {
    let world = app.world_mut();
    world
        .query_filtered::<(), (With<FatCat>, With<Lapping>)>()
        .iter(world)
        .next()
        .is_some()
}

#[test]
fn the_cat_drinks_the_puddle_and_it_disappears() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..5 {
        exchange(&mut server, &mut client);
    }
    client.world_mut().client_trigger(Piss);
    for _ in 0..3 {
        exchange(&mut server, &mut client);
    }
    assert_eq!(puddles(&mut server), 1, "a puddle on the server");
    assert_eq!(puddles(&mut client), 1, "and everyone sees it");

    // Within a few seconds a cat is drinking it.
    let mut drank = false;
    for _ in 0..50 {
        exchange(&mut server, &mut client);
        drank |= lapping(&mut client);
    }
    assert!(drank, "a cat went to drink");

    // Past its 20 s drying time it's still there while being drunk...
    for _ in 0..170 {
        exchange(&mut server, &mut client);
    }
    // ...and by 30 s it's been drunk up and the cats are back to normal.
    for _ in 0..120 {
        exchange(&mut server, &mut client);
    }
    assert_eq!(puddles(&mut server), 0, "drunk up");
    assert_eq!(puddles(&mut client), 0, "gone for everyone");
    assert!(!lapping(&mut client), "done drinking");
}
