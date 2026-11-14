//! Headless regression tests for the overnight hardening bug pass (bugs-a).
//! Each test names the bug it locks down; see `bugpass/fixes/` for the reports.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;

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

fn step(server: &mut App, client: &mut App) {
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(client);
        client.update();
    }
}

/// Every `DuelFought` the server fired, in order.
#[derive(Resource, Default)]
struct Fought(Vec<(u64, u64)>);

fn count_fought(server: &mut App) -> usize {
    server.world().resource::<Fought>().0.len()
}

/// Start a duel between the host (player 0) and the client (player 1) and
/// return after both duelists exist.
fn start_duel(server: &mut App, client: &mut App) {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::duel::{AnswerDuel, ChallengeDuel};

    server
        .world_mut()
        .client_trigger(ChallengeDuel { target: 1 });
    step(server, client);
    client.world_mut().client_trigger(AnswerDuel {
        from: 0,
        accept: true,
    });
    step(server, client);
    let world = server.world_mut();
    assert_eq!(
        world
            .query::<&friendslop::features::duel::Duelist>()
            .iter(world)
            .count(),
        2,
        "the duel started"
    );
}

fn set_lives(server: &mut App, player: u64, lives: u8) {
    let world = server.world_mut();
    for mut d in world
        .query::<&mut friendslop::features::duel::Duelist>()
        .iter_mut(world)
    {
        if d.player == player {
            d.lives = lives;
        }
    }
}

/// play:double-fought — two `DuelShoot`s in one batch must produce one
/// `DuelFought` for one kill. The kill's despawn is deferred (`Commands`), so
/// a second shot can still see the victim with `lives == 0`; that is the
/// synchronous claim that the duel already ended, and only one `DuelFought`
/// may ever fire for it (skills, campaign and the gods' tears all count them).
#[test]
fn duel_two_shots_one_kill_fires_one_fought() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::duel::DuelShoot;

    let (mut server, mut client) = (peer(), peer());
    server.init_resource::<Fought>();
    server.world_mut().add_observer(
        |ev: On<friendslop::features::duel::DuelFought>, mut c: ResMut<Fought>| {
            c.0.push((ev.winner, ev.loser));
        },
    );
    server.connect_client(&mut client);
    step(&mut server, &mut client);
    start_duel(&mut server, &mut client);

    // The victim is on their last life. Both duelists start facing each other,
    // so every shot from the host lands.
    set_lives(&mut server, 1, 1);
    server.world_mut().client_trigger(DuelShoot);
    server.world_mut().client_trigger(DuelShoot);
    step(&mut server, &mut client);

    assert_eq!(
        count_fought(&mut server),
        1,
        "two shots, one kill: one DuelFought"
    );
    let fought = &server.world().resource::<Fought>().0;
    assert_eq!(fought, &[(0, 1)], "player 0 slew player 1");
}

/// play:double-fought (the window) — a shot that lands while the victim is
/// already at `lives == 0` (killed earlier in the batch, despawn still
/// pending) must not fire `DuelFought` again.
#[test]
fn duel_shot_at_dead_victim_fires_no_second_fought() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::duel::DuelShoot;

    let (mut server, mut client) = (peer(), peer());
    server.init_resource::<Fought>();
    server.world_mut().add_observer(
        |ev: On<friendslop::features::duel::DuelFought>, mut c: ResMut<Fought>| {
            c.0.push((ev.winner, ev.loser));
        },
    );
    server.connect_client(&mut client);
    step(&mut server, &mut client);
    start_duel(&mut server, &mut client);

    // The kill already happened this batch: the victim is at zero lives and
    // the despawn has not applied yet (the duelist entities are still here).
    set_lives(&mut server, 1, 0);
    server.world_mut().client_trigger(DuelShoot);
    step(&mut server, &mut client);

    assert_eq!(
        count_fought(&mut server),
        0,
        "the duel was already decided: no second DuelFought"
    );
}

/// play:mishap-attrs — a failure by a client with no player (it just left)
/// must not be blamed on player 0; the board just doesn't record it.
#[test]
fn unknown_client_mishap_not_player_zero() {
    use bevy::ecs::system::RunSystemOnce;
    use friendslop::features::fate::{Fate, FateBoard, FateDice};

    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    step(&mut server, &mut client);
    // Every action fails from here on.
    server.world_mut().resource_mut::<FateDice>().chance = 100.0;
    // A client entity with no `Player`: its failure has no one to blame.
    let ghost = server.world_mut().spawn_empty().id();
    server
        .world_mut()
        .run_system_once(move |mut fate: Fate| {
            assert!(fate.fails(ClientId::Client(ghost)));
        })
        .expect("test setup");
    {
        let world = server.world_mut();
        let board = world
            .query::<&FateBoard>()
            .iter(world)
            .next()
            .expect("test setup");
        assert!(
            board.mishaps.is_empty(),
            "an unknown client's mishap is not blamed on player 0: {:?}",
            board.mishaps
        );
    }
    // The host's own failure is still theirs (player 0).
    server
        .world_mut()
        .run_system_once(|mut fate: Fate| {
            assert!(fate.fails(ClientId::Server));
        })
        .expect("test setup");
    let world = server.world_mut();
    let board = world
        .query::<&FateBoard>()
        .iter(world)
        .next()
        .expect("test setup");
    assert_eq!(
        board.mishaps.iter().map(|m| m.player).collect::<Vec<_>>(),
        vec![0],
        "the host's failure is player 0's"
    );
}
