//! Headless: the mute spell (#146). The host mutes the client, the client
//! gets an RPG, its rocket kills the host, and then the client is free.

use std::time::Duration;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::fps::{EYE, Walker};
use friendslop::features::mute_spell::{CastMute, FireRpg, Silenced};
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

fn silenced(app: &mut App) -> Vec<(u64, u64)> {
    let world = app.world_mut();
    world
        .query::<(&Player, &Silenced)>()
        .iter(world)
        .map(|(p, s)| (p.id, s.by))
        .collect()
}

fn feet(app: &mut App, id: u64) -> Vec3 {
    let world = app.world_mut();
    world
        .query::<(&Player, &Walker)>()
        .iter(world)
        .find(|(p, _)| p.id == id)
        .map(|(_, w)| w.pos)
        .unwrap_or(Vec3::NAN)
}

/// Stand the host and client 8 m apart so a rocket has to fly.
fn spread_out(server: &mut App) {
    let world = server.world_mut();
    let mut q = world.query::<(&Player, &mut Walker)>();
    for (p, mut w) in q.iter_mut(world) {
        w.pos = Vec3::new(p.id as f32 * 8.0, 0.0, 9.0);
    }
}

#[test]
fn muted_player_rockets_the_caster_and_is_freed() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 5);
    spread_out(&mut server);
    exchange(&mut server, &mut client, 2);

    // A muted player can't fire, a non-muted player has no RPG.
    client
        .world_mut()
        .client_trigger(FireRpg { dir: Vec3::NEG_X });
    exchange(&mut server, &mut client, 2);
    assert!(silenced(&mut server).is_empty());

    // The host says "mute" at the client.
    server.world_mut().client_trigger(CastMute { target: 1 });
    exchange(&mut server, &mut client, 2);
    assert_eq!(silenced(&mut client), vec![(1, 0)], "replicates");

    // The muted client can't mute back.
    client.world_mut().client_trigger(CastMute { target: 0 });
    exchange(&mut server, &mut client, 2);
    assert_eq!(silenced(&mut server), vec![(1, 0)]);

    // A rocket fired away from the host fizzles harmlessly.
    let host_start = feet(&mut server, 0);
    client.world_mut().client_trigger(FireRpg { dir: Vec3::X });
    exchange(&mut server, &mut client, 40);
    assert_eq!(feet(&mut server, 0), host_start, "missed");
    assert_eq!(silenced(&mut server), vec![(1, 0)], "still muted");

    // Aimed at the host's chest: one hit kills, back to spawn, mission over.
    let from = feet(&mut server, 1) + Vec3::Y * EYE;
    let chest = feet(&mut server, 0) + Vec3::Y;
    client
        .world_mut()
        .client_trigger(FireRpg { dir: chest - from });
    exchange(&mut server, &mut client, 20);
    assert_ne!(feet(&mut server, 0), host_start, "the host was blown up");
    assert!(silenced(&mut server).is_empty(), "the RPG is gone");
    assert!(silenced(&mut client).is_empty());
}
