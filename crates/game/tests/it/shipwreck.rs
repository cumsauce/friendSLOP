//! Headless: a client battles the three pirates and plunders the chest once
//! (#185); the chest is shut while any pirate is still up.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::fate::FateDice;
use friendslop::features::fps::Walker;
use friendslop::features::shipwreck::{
    Attack, Doubloons, Loot, PIRATE_HP, Pirate, chest_spot, pirate_spot,
};
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

fn stand_at(server: &mut App, at: Vec3) {
    let world = server.world_mut();
    for (p, mut w) in world.query::<(&Player, &mut Walker)>().iter_mut(world) {
        if p.id != 0 {
            w.pos = at;
        }
    }
}

fn purse(app: &mut App) -> u32 {
    let world = app.world_mut();
    world
        .query::<(&Player, &Doubloons)>()
        .iter(world)
        .find(|(p, _)| p.id != 0)
        .map_or(0, |(_, d)| d.0)
}

#[test]
fn battle_the_crew_then_plunder() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);
    server.world_mut().resource_mut::<FateDice>().chance = 0.0;

    // The chest is guarded.
    stand_at(&mut server, chest_spot());
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(Loot);
    exchange(&mut server, &mut client, 3);
    assert_eq!(purse(&mut client), 0);

    for id in 0..3u8 {
        for _ in 0..PIRATE_HP {
            // Knockbacks push you away, so step back in before each swing.
            stand_at(&mut server, pirate_spot(id));
            exchange(&mut server, &mut client, 1);
            client.world_mut().client_trigger(Attack { id });
            exchange(&mut server, &mut client, 2);
        }
    }
    let world = client.world_mut();
    let down = world
        .query::<&Pirate>()
        .iter(world)
        .filter(|p| !p.up())
        .count();
    assert_eq!(down, 3, "the whole crew yielded");

    stand_at(&mut server, chest_spot());
    exchange(&mut server, &mut client, 2);
    client.world_mut().client_trigger(Loot);
    client.world_mut().client_trigger(Loot);
    exchange(&mut server, &mut client, 3);
    assert_eq!(
        purse(&mut client),
        friendslop::features::shipwreck::tuning().haul,
        "one share each"
    );
}
