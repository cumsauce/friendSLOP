//! Headless: dying (#209). The client dies, goes to heaven, the host begs
//! at their grave and they come back; the second time nobody begs and
//! they're thrown to hell; the Tome of Uncancelling brings them back.

use std::time::Duration;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::afterlife::{
    Dead, Die, Grave, Plead, TakeTome, Tome, UseTome, lectern_at,
};
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
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(
        50,
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

fn dead(app: &mut App, id: u64) -> Option<Dead> {
    let world = app.world_mut();
    world
        .query::<(&Player, &Dead)>()
        .iter(world)
        .find(|(p, _)| p.id == id)
        .map(|(_, d)| *d)
}

fn graves(app: &mut App) -> Vec<Grave> {
    let world = app.world_mut();
    world.query::<&Grave>().iter(world).copied().collect()
}

fn tome(app: &mut App) -> Option<Tome> {
    let world = app.world_mut();
    world.query::<&Tome>().iter(world).next().copied()
}

fn put(app: &mut App, id: u64, at: Vec3) {
    let world = app.world_mut();
    let mut q = world.query::<(&Player, &mut Walker)>();
    for (p, mut w) in q.iter_mut(world) {
        if p.id == id {
            w.pos = at;
        }
    }
}

#[test]
fn begging_brings_you_back_and_hell_waits_for_the_rest() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 5);
    let fell_at = Vec3::new(4.0, 0.0, 4.0);
    put(&mut server, 0, fell_at);
    put(&mut server, 1, fell_at);
    exchange(&mut server, &mut client, 2);

    // The client dies: a grave where they fell, and heaven.
    server.world_mut().trigger(Die { player: 1 });
    exchange(&mut server, &mut client, 2);
    assert!(matches!(dead(&mut client, 1), Some(Dead::Heaven { .. })));
    assert_eq!(graves(&mut client).len(), 1);
    assert_eq!(graves(&mut server)[0].at, fell_at);

    // The dead can't beg for themselves.
    client.world_mut().client_trigger(Plead {
        player: 1,
        aloud: true,
    });
    exchange(&mut server, &mut client, 10);
    assert!(matches!(
        dead(&mut server, 1),
        Some(Dead::Heaven { pleas: 0, .. })
    ));

    // The host begs out loud (2 each) until heaven gives in.
    for _ in 0..friendslop::features::afterlife::tuning().pleas_needed / 2 {
        server.world_mut().client_trigger(Plead {
            player: 1,
            aloud: true,
        });
        exchange(&mut server, &mut client, 10);
    }
    exchange(&mut server, &mut client, 2);
    assert_eq!(dead(&mut client, 1), None, "innocent");
    assert!(graves(&mut client).is_empty());

    // Second death: nobody begs, and after the judgement it's hell.
    server.world_mut().trigger(Die { player: 1 });
    let ticks = (friendslop::features::afterlife::tuning().judgement_secs / 0.05) as usize + 20;
    exchange(&mut server, &mut client, ticks);
    assert_eq!(dead(&mut client, 1), Some(Dead::Hell));
    assert_eq!(graves(&mut server).len(), 1, "the grave stays");

    // Too late to beg; the tome still works. The host fetches it.
    assert_eq!(tome(&mut client), Some(Tome { holder: None }));
    put(&mut server, 0, lectern_at().extend(0.0).xzy());
    exchange(&mut server, &mut client, 2);
    server.world_mut().client_trigger(TakeTome);
    exchange(&mut server, &mut client, 2);
    assert_eq!(tome(&mut client), Some(Tome { holder: Some(0) }));

    // Far from the grave it does nothing; at the grave it does.
    server.world_mut().client_trigger(UseTome { player: 1 });
    exchange(&mut server, &mut client, 2);
    assert_eq!(dead(&mut server, 1), Some(Dead::Hell));
    put(&mut server, 0, fell_at);
    exchange(&mut server, &mut client, 2);
    server.world_mut().client_trigger(UseTome { player: 1 });
    exchange(&mut server, &mut client, 2);
    assert_eq!(dead(&mut client, 1), None, "uncancelled");
    assert_eq!(tome(&mut client), None, "used up");
    assert!(graves(&mut client).is_empty());
}
