//! Headless: a client falls asleep, can't walk, wakes up; pissing earns
//! Pissing XP (#176).

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::skills::{Asleep, Piss, Skill, Skills, ToggleSleep};
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

fn remote(app: &mut App) -> (bool, f32) {
    let world = app.world_mut();
    world
        .query::<(&Player, &Skills, Has<Asleep>)>()
        .iter(world)
        .find(|(p, ..)| p.id != 0)
        .map_or((false, -1.0), |(_, s, a)| (a, s.xp(Skill::Pissing)))
}

#[test]
fn sleeping_and_pissing() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote(&mut client), (false, 0.0));

    client.world_mut().client_trigger(Piss);
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote(&mut client), (false, 3.0));

    client.world_mut().client_trigger(ToggleSleep);
    exchange(&mut server, &mut client, 3);
    assert!(remote(&mut client).0, "asleep");
    // No pissing in bed.
    client.world_mut().client_trigger(Piss);
    exchange(&mut server, &mut client, 3);
    assert_eq!(remote(&mut client).1, 3.0);

    client.world_mut().client_trigger(ToggleSleep);
    exchange(&mut server, &mut client, 3);
    assert!(!remote(&mut client).0, "awake again");
}
