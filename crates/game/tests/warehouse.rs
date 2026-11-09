//! Headless: the warehouse (#130) only opens when every player agrees, takes
//! everyone inside, has soul pieces and a monster in it, and closes again
//! once everyone has left.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::fps::Walker;
use friendslop::features::warehouse::{
    GiveToJegg, KnockDoor, LeaveWarehouse, Monster, SOUL_PIECES, SoulPiece, TakeSoul, Warehouse,
    door_at, inside, jegg_at,
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

fn warehouse(app: &mut App) -> Warehouse {
    let world = app.world_mut();
    let Some(w) = world.query::<&Warehouse>().iter(world).next().cloned() else {
        panic!("no warehouse");
    };
    w
}

fn walkers(app: &mut App) -> Vec<(u64, Vec3)> {
    let world = app.world_mut();
    let mut v: Vec<(u64, Vec3)> = world
        .query::<(&Player, &Walker)>()
        .iter(world)
        .map(|(p, w)| (p.id, w.pos))
        .collect();
    v.sort_by_key(|x| x.0);
    v
}

/// Server: put player `id`'s body at `at`.
fn place(server: &mut App, id: u64, at: Vec3) {
    let world = server.world_mut();
    for (p, mut w) in world.query::<(&Player, &mut Walker)>().iter_mut(world) {
        if p.id == id {
            w.pos = at;
        }
    }
}

#[test]
fn everyone_must_agree_then_in_and_out() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    exchange(&mut server, &mut client, 3);
    let ids: Vec<u64> = walkers(&mut server).iter().map(|w| w.0).collect();
    assert_eq!(ids.len(), 2, "host and one friend");
    let host = 0;
    let Some(&friend) = ids.iter().find(|&&i| i != host) else {
        panic!("no friend");
    };
    for &id in &ids {
        place(&mut server, id, door_at() - Vec3::Z * 1.5);
    }
    exchange(&mut server, &mut client, 1);

    // The host agrees: not enough on its own.
    server.world_mut().client_trigger(KnockDoor);
    exchange(&mut server, &mut client, 2);
    let w = warehouse(&mut client);
    assert_eq!(w.votes, vec![host]);
    assert_eq!(w.seed, None);
    assert!(walkers(&mut server).iter().all(|(_, p)| !inside(*p)));

    // The friend agrees too: everyone goes in.
    client.world_mut().client_trigger(KnockDoor);
    exchange(&mut server, &mut client, 3);
    let w = warehouse(&mut client);
    assert!(w.seed.is_some(), "the expedition starts");
    assert!(w.votes.is_empty());
    assert!(
        walkers(&mut server).iter().all(|(_, p)| inside(*p)),
        "everyone is inside: {:?}",
        walkers(&mut server)
    );
    {
        let world = client.world_mut();
        let pieces = world.query::<&SoulPiece>().iter(world).count();
        assert_eq!(pieces, SOUL_PIECES as usize, "the client sees the pieces");
        let monsters = world.query::<&Monster>().iter(world).count();
        assert_eq!(monsters, 1, "and the monster");
    }

    // The host picks a piece up (teleported next to it), walks out and
    // gives it to Jegg.
    let piece = {
        let world = server.world_mut();
        let Some(p) = world.query::<&SoulPiece>().iter(world).next().copied() else {
            panic!("no piece");
        };
        p
    };
    place(
        &mut server,
        host,
        piece.pos.with_y(friendslop::features::warehouse::INSIDE_Y),
    );
    server.world_mut().client_trigger(TakeSoul { id: piece.id });
    exchange(&mut server, &mut client, 2);
    {
        let world = client.world_mut();
        let carried = world
            .query::<&SoulPiece>()
            .iter(world)
            .any(|p| p.carrier == Some(host));
        assert!(carried, "the host carries a piece");
    }
    place(
        &mut server,
        host,
        friendslop::features::warehouse::exit_at() + Vec3::Z,
    );
    server.world_mut().client_trigger(LeaveWarehouse);
    exchange(&mut server, &mut client, 2);
    assert!(!inside(walkers(&mut server)[0].1), "the host is out");
    place(&mut server, host, jegg_at() - Vec3::Z);
    server.world_mut().client_trigger(GiveToJegg);
    exchange(&mut server, &mut client, 2);
    assert_eq!(warehouse(&mut client).delivered, 1);

    // The friend leaves too: nobody's inside, so it closes.
    place(
        &mut server,
        friend,
        friendslop::features::warehouse::exit_at() + Vec3::Z,
    );
    client.world_mut().client_trigger(LeaveWarehouse);
    exchange(&mut server, &mut client, 3);
    let w = warehouse(&mut client);
    assert_eq!(w.seed, None, "closed once empty");
    assert_eq!(w.delivered, 1, "Jegg keeps the piece");
    let world = client.world_mut();
    assert_eq!(world.query::<&Monster>().iter(world).count(), 0);
    assert_eq!(world.query::<&SoulPiece>().iter(world).count(), 0);
}
