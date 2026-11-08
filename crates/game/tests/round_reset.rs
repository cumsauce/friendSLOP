//! Headless: finishing the chapter (beating the landlord and the rest of the
//! checklist) ends the round, and a restart resets it.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::campaign::Campaign;
use friendslop::features::gods_tears::{GodsTear, TOTAL_TEARS, Transformation};
use friendslop::features::halt::Halt;
use friendslop::features::landlord::{Act3, Tenancy};
use friendslop::features::morality::Morality;
use friendslop::features::objective::{Objective, RequestRestart};
use friendslop::features::organ_currency::{OrganPurse, starting_purse};
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

fn objective(app: &mut App) -> Objective {
    let world = app.world_mut();
    let Some(o) = world.query::<&Objective>().iter(world).next().cloned() else {
        panic!("no objective");
    };
    o
}

#[test]
fn defeating_the_landlord_then_restarting_resets_the_round() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);
    assert_eq!(objective(&mut client).round, 1);
    assert!(!objective(&mut client).over);

    // Make a mess of the round, then win it.
    {
        let world = server.world_mut();
        for mut t in world.query::<&mut Tenancy>().iter_mut(world) {
            t.act = Act3::Won;
            t.hp = 0;
            t.rent = 999;
        }
        for mut h in world.query::<&mut Halt>().iter_mut(world) {
            h.escape = 50;
            h.escapes = 3;
        }
        let tears: Vec<Entity> = world
            .query_filtered::<Entity, With<GodsTear>>()
            .iter(world)
            .collect();
        for e in tears.into_iter().take(5) {
            world.despawn(e);
        }
        for mut p in world.query::<&mut OrganPurse>().iter_mut(world) {
            p.0.clear();
        }
        for mut m in world.query::<&mut Morality>().iter_mut(world) {
            m.0 = 42;
        }
        for mut t in world.query::<&mut Transformation>().iter_mut(world) {
            t.0 = 4;
        }
        // The rest of the chapter's checklist (#75).
        for mut c in world.query::<&mut Campaign>().iter_mut(world) {
            c.observe(|t| t.need());
        }
    }
    exchange(&mut server, &mut client, 3);
    assert!(objective(&mut client).over, "client sees the round end");

    client.world_mut().client_trigger(RequestRestart);
    exchange(&mut server, &mut client, 4);

    let o = objective(&mut client);
    assert_eq!(o.round, 2);
    assert!(!o.over);
    assert_eq!(o.act, Act3::Legal);

    let world = client.world_mut();
    let tenancy: Vec<Tenancy> = world.query::<&Tenancy>().iter(world).cloned().collect();
    assert_eq!(tenancy.len(), 1);
    let fresh = Tenancy::default();
    assert_eq!(
        (
            tenancy[0].act,
            tenancy[0].rent,
            tenancy[0].hp,
            tenancy[0].heists
        ),
        (fresh.act, fresh.rent, fresh.hp, fresh.heists)
    );
    let halt: Vec<Halt> = world.query::<&Halt>().iter(world).copied().collect();
    assert_eq!(halt.len(), 1);
    assert_eq!((halt[0].escape, halt[0].escapes), (0, 0));
    let tears = world.query::<&GodsTear>().iter(world).count();
    assert_eq!(tears, TOTAL_TEARS as usize, "the tears fall again");
    let players = world.query::<&Player>().iter(world).count();
    assert_eq!(players, 2);
    assert!(
        world
            .query::<&OrganPurse>()
            .iter(world)
            .all(|p| *p == starting_purse())
    );
    assert!(world.query::<&Morality>().iter(world).all(|m| m.0 == 0));
    assert!(
        world
            .query::<&Transformation>()
            .iter(world)
            .all(|t| t.0 == 0)
    );
}

#[test]
fn restart_is_ignored_mid_round() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);
    client.world_mut().client_trigger(RequestRestart);
    exchange(&mut server, &mut client, 3);
    assert_eq!(objective(&mut client).round, 1);
}
