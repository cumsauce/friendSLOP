//! Headless: in attack mode a villager catches the hunted player, carries
//! them to a random other player (the host here), catches them too and then
//! floats with both (#140).

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::caught::Caught;
use friendslop::features::fps::Walker;
use friendslop::features::punishment::AttackMode;
use friendslop::features::villagers::Villager;
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

fn caught(app: &mut App) -> Vec<Caught> {
    let world = app.world_mut();
    world.query::<&Caught>().iter(world).cloned().collect()
}

fn body(app: &mut App, id: u64) -> Option<Vec3> {
    let world = app.world_mut();
    world
        .query::<(&Player, &Walker)>()
        .iter(world)
        .find(|(p, _)| p.id == id)
        .map(|(_, w)| w.pos)
}

fn put_body(app: &mut App, id: u64, at: Vec3) {
    let world = app.world_mut();
    for (p, mut w) in world.query::<(&Player, &mut Walker)>().iter_mut(world) {
        if p.id == id {
            w.pos = at;
        }
    }
}

#[test]
fn caught_carried_to_an_accomplice_then_floated() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);

    // The host stands far off; the remote player is hunted, with a villager
    // right beside them.
    put_body(&mut server, 0, Vec3::new(20.0, 0.0, 20.0));
    put_body(&mut server, 1, Vec3::new(5.0, 0.0, 5.0));
    let catcher = {
        let world = server.world_mut();
        let mut q = world.query::<&mut Villager>();
        let mut it = q.iter_mut(world);
        let Some(mut v) = it.next() else {
            panic!("no villagers on the server");
        };
        v.pos = Vec2::new(5.5, 5.0);
        v.id
    };
    server.world_mut().spawn((
        AttackMode {
            target: 1,
            secs_left: 60,
        },
        Replicated,
    ));
    exchange(&mut server, &mut client, 3);

    let c = caught(&mut client);
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0].catcher, catcher);
    assert_eq!(c[0].held, vec![1]);
    assert_eq!(c[0].accomplice, Some(0), "the only other player");
    // The chase moved from the caught player to the accomplice.
    let world = client.world_mut();
    let targets: Vec<u64> = world
        .query::<&AttackMode>()
        .iter(world)
        .map(|a| a.target)
        .collect();
    assert_eq!(targets, vec![0]);
    // The remote player is held up in the catcher's arms, whatever they do.
    let held = body(&mut server, 1).unwrap_or_default();
    assert!(held.xz().distance(Vec2::new(5.5, 5.0)) < 1.5, "{held}");

    // The accomplice is reached: both held, floating, no chase left.
    let at = {
        let world = server.world_mut();
        let mut q = world.query::<&Villager>();
        q.iter(world)
            .find(|v| v.id == catcher)
            .map_or(Vec2::ZERO, |v| v.pos)
    };
    put_body(&mut server, 0, Vec3::new(at.x + 0.5, 0.0, at.y));
    exchange(&mut server, &mut client, 4);
    let c = caught(&mut client);
    assert_eq!(c.len(), 1, "{c:?}");
    assert_eq!(c[0].accomplice, None);
    assert_eq!(c[0].held, vec![1, 0]);
    let world = client.world_mut();
    assert_eq!(world.query::<&AttackMode>().iter(world).count(), 0);
}
