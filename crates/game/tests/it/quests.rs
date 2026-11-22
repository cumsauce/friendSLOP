//! Headless: the quest system over the network. Saying yes to a giver from
//! a client puts the quest on everyone's log (only beside them, and only
//! while they're out), signals move it on, and finishing pays every player a
//! KoriCoin™. Michael's theatre tells its quest who watched and who tooted.

use std::time::Duration;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::fps::Walker;
use friendslop::features::quests::{GIVERS, QUESTS, QuestLog, QuestSignal, QuestState, TalkChoice};
use friendslop::player::Player;
use friendslop::rules::kori::KoriCoins;

fn peer() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        RepliconPlugins.set(ServerPlugin::new(PostUpdate)),
        ProtocolPlugin,
    ));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        0.25,
    )));
    app.finish();
    app
}

fn step(server: &mut App, client: &mut App, n: usize) {
    for _ in 0..n {
        server.update();
        server.exchange_with_client(client);
        client.update();
        server.exchange_with_client(client);
    }
}

fn place(server: &mut App, id: u64, at: Vec3) {
    let world = server.world_mut();
    for (p, mut w) in world.query::<(&Player, &mut Walker)>().iter_mut(world) {
        if p.id == id {
            w.pos = at;
        }
    }
}

fn log(app: &mut App) -> QuestLog {
    let world = app.world_mut();
    let Some(l) = world.query::<&QuestLog>().iter(world).next().cloned() else {
        panic!("no quest log");
    };
    l
}

fn ask(giver: &str, quest: &str) -> TalkChoice {
    TalkChoice {
        giver: giver.into(),
        quest: quest.into(),
        variant: None,
    }
}

fn giver_at(id: &str) -> Vec3 {
    GIVERS
        .get()
        .get(id)
        .map(|g| g.place.at())
        .unwrap_or_default()
}

#[test]
fn saying_yes_beside_a_giver_shares_the_quest_and_finishing_pays_everyone() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    step(&mut server, &mut client, 3);
    assert_eq!(log(&mut client).state("burgers"), QuestState::Available);

    // Too far away: nothing.
    place(
        &mut server,
        1,
        giver_at("cashier") + Vec3::new(30.0, 0.0, 0.0),
    );
    step(&mut server, &mut client, 1);
    client.world_mut().client_trigger(ask("cashier", "burgers"));
    step(&mut server, &mut client, 3);
    assert_eq!(log(&mut client).state("burgers"), QuestState::Available);

    // The wrong giver: nothing.
    place(&mut server, 1, giver_at("cashier"));
    step(&mut server, &mut client, 1);
    client
        .world_mut()
        .client_trigger(ask("cashier", "landlord"));
    // Beside them, the right quest: on everyone's list.
    client.world_mut().client_trigger(ask("cashier", "burgers"));
    step(&mut server, &mut client, 3);
    assert!(log(&mut client).is_active("burgers"));
    assert!(log(&mut server).is_active("burgers"));
    assert_eq!(log(&mut client).state("landlord"), QuestState::Locked);

    let coins = |app: &mut App| {
        let world = app.world_mut();
        let mut c: Vec<(u64, u32)> = world
            .query::<(&Player, &KoriCoins)>()
            .iter(world)
            .map(|(p, k)| (p.id, k.0))
            .collect();
        c.sort();
        c
    };
    let before = coins(&mut server);
    assert_eq!(before.len(), 2, "the host and the client have purses");

    // Three burgers (the signal Wendy's sends), and it's done.
    for _ in 0..3 {
        server.world_mut().trigger(QuestSignal::new("burger"));
    }
    step(&mut server, &mut client, 3);
    assert!(log(&mut client).is_done("burgers"));
    let reward = QUESTS.get().reward("burgers", None);
    let after = coins(&mut client);
    for ((id, b), (_, a)) in before.iter().zip(&after) {
        assert_eq!(*a, b + reward, "player {id} got paid");
    }
}

#[test]
fn night_givers_only_give_at_night() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    step(&mut server, &mut client, 3);
    let Some(owl) = GIVERS.get().get("night_owl").cloned() else {
        panic!("the night owl is in the file");
    };
    assert!(owl.hours.is_some());
    place(&mut server, 1, owl.place.at());
    step(&mut server, &mut client, 1);

    // The game starts in the morning: nobody's at the lake.
    client.world_mut().client_trigger(ask("night_owl", "halt"));
    step(&mut server, &mut client, 3);
    assert_eq!(log(&mut client).state("halt"), QuestState::Available);

    // Set the shared clock to the middle of the night.
    let day = friendslop::atmosphere::DAY_NIGHT.get();
    let now = friendslop::settings::since_epoch().as_secs_f64();
    let back = friendslop::atmosphere::elapsed_for(0.0, day.day_minutes, day.start_hour);
    server
        .world_mut()
        .spawn((friendslop::atmosphere::DayStart(now - back), Replicated));
    step(&mut server, &mut client, 2);
    client.world_mut().client_trigger(ask("night_owl", "halt"));
    step(&mut server, &mut client, 3);
    assert!(log(&mut client).is_active("halt"));
}

#[test]
fn michael_notices_who_watched_and_who_tooted() {
    use friendslop::features::fart::{Fart, Gas};
    use friendslop::features::michael::{WATCH_SECS, in_house, stage_at};

    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    step(&mut server, &mut client, 3);
    place(&mut server, 1, giver_at("superfan"));
    step(&mut server, &mut client, 1);
    client
        .world_mut()
        .client_trigger(ask("superfan", "michael"));
    step(&mut server, &mut client, 3);
    assert!(log(&mut client).is_active("michael"));

    // Into the house, in front of the stage, for the whole show.
    let seat = stage_at() + Vec3::new(0.0, 0.7, 10.0);
    assert!(in_house(seat));
    place(&mut server, 1, seat);
    let frames = (WATCH_SECS / 0.25).ceil() as usize + 8;
    for _ in 0..frames {
        place(&mut server, 1, seat);
        step(&mut server, &mut client, 1);
    }
    let book = QUESTS.get();
    let l = log(&mut client);
    assert_eq!(
        l.at_step(&book, "michael").map(|(s, _)| s.signals.clone()),
        Some(vec!["michael_toot".to_string()]),
        "watched the show: now the toot"
    );

    // A full fart in the house.
    {
        let world = server.world_mut();
        for (p, mut g) in world.query::<(&Player, &mut Gas)>().iter_mut(world) {
            if p.id == 1 {
                g.0 = 100;
            }
        }
    }
    client.world_mut().client_trigger(Fart { at: seat.xz() });
    step(&mut server, &mut client, 4);
    assert!(
        log(&mut client).is_done("michael"),
        "{:?}",
        log(&mut client).state("michael")
    );
}
