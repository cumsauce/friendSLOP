//! Headless: a listen server and a client agree on who is playing.

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
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

/// Teleport player `id`'s body on the server (tests only; normally it walks).
fn place(server: &mut App, id: u64, at: Vec3) {
    use friendslop::features::fps::Walker;
    let world = server.world_mut();
    for (p, mut w) in world.query::<(&Player, &mut Walker)>().iter_mut(world) {
        if p.id == id {
            w.pos = at;
        }
    }
}

/// Stand player `id` next to the (only) mushroom.
fn to_mushroom(server: &mut App, id: u64) {
    use friendslop::features::mushroom::Mushroom;
    let world = server.world_mut();
    let Some(at) = world.query::<&Mushroom>().iter(world).next().map(|m| m.at) else {
        panic!("no mushroom");
    };
    place(server, id, Vec3::new(at.x, 0.0, at.y));
}

fn player_count(app: &mut App) -> usize {
    let world = app.world_mut();
    world.query::<&Player>().iter(world).count()
}

#[test]
fn host_and_client_players_replicate() {
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    assert_eq!(
        player_count(&mut server),
        2,
        "host + one client on the server"
    );
    assert_eq!(player_count(&mut client), 2, "client sees both players");
}

#[test]
fn everyone_gets_an_organ_purse() {
    use friendslop::features::organ_currency::{OrganPurse, starting_purse};
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let purses: Vec<OrganPurse> = world
        .query_filtered::<&OrganPurse, With<Player>>()
        .iter(world)
        .cloned()
        .collect();
    assert_eq!(purses.len(), 2, "client sees both players' purses");
    assert!(purses.iter().all(|p| *p == starting_purse()));
}

#[test]
fn crows_replicate_to_clients() {
    use friendslop::features::crows::{CROW_COUNT, Crow};
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let n = world.query::<&Crow>().iter(world).count();
    assert_eq!(n, CROW_COUNT as usize, "client sees every crow");
}

#[test]
fn exactly_one_bear_replicates() {
    use friendslop::features::bear::Bear;
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let n = world.query::<&Bear>().iter(world).count();
    assert_eq!(n, 1, "just one bear");
}

#[test]
fn eating_the_mushroom_changes_only_the_eaters_voice() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::mushroom::{EatMushroom, Mushroom, MushroomVoice};

    #[derive(Resource, Default)]
    struct Got(Vec<MushroomVoice>);

    let (mut server, mut client) = (peer(), peer());
    server.init_resource::<Got>();
    client.init_resource::<Got>();
    for app in [&mut server, &mut client] {
        app.world_mut()
            .add_observer(|v: On<MushroomVoice>, mut got: ResMut<Got>| got.0.push(*v));
    }
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    step(&mut server, &mut client);
    let world = client.world_mut();
    assert_eq!(world.query::<&Mushroom>().iter(world).count(), 1);

    to_mushroom(&mut server, 1);
    client.world_mut().client_trigger(EatMushroom);
    step(&mut server, &mut client);

    let world = client.world_mut();
    assert_eq!(world.query::<&Mushroom>().iter(world).count(), 0, "eaten");
    let got = &client.world().resource::<Got>().0;
    assert_eq!(got.len(), 1, "the eater is told");
    assert!(got[0].pitch >= 1.5 || got[0].pitch <= 0.7);
    assert!(server.world().resource::<Got>().0.is_empty(), "host is not");
}

#[test]
fn clients_learn_who_they_are() {
    use friendslop::player::LocalPlayer;
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    assert_eq!(
        server.world().get_resource::<LocalPlayer>(),
        Some(&LocalPlayer(0))
    );
    assert_eq!(
        client.world().get_resource::<LocalPlayer>(),
        Some(&LocalPlayer(1))
    );
}

#[test]
fn challenge_accept_and_shoot_to_win_a_duel() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::duel::{
        AnswerDuel, ChallengeDuel, DuelShoot, Duelist, LIVES, spawn_points,
    };

    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    let duelists = |app: &mut App| {
        let world = app.world_mut();
        let mut v: Vec<Duelist> = world.query::<&Duelist>().iter(world).copied().collect();
        v.sort_by_key(|d| d.player);
        v
    };
    step(&mut server, &mut client);

    // The host challenges player 1, who accepts.
    server
        .world_mut()
        .client_trigger(ChallengeDuel { target: 1 });
    step(&mut server, &mut client);
    assert!(duelists(&mut client).is_empty(), "not until accepted");
    client.world_mut().client_trigger(AnswerDuel {
        from: 0,
        accept: true,
    });
    step(&mut server, &mut client);
    let both = duelists(&mut client);
    assert_eq!(both.len(), 2, "both duelists replicate");
    assert!(both.iter().all(|d| d.lives == LIVES && d.bullets == 1));

    // Both start facing each other, so every shot from the host lands. Each
    // kill refunds the bullet, so the host can win with three shots.
    for shot in 1..=LIVES {
        server.world_mut().client_trigger(DuelShoot);
        step(&mut server, &mut client);
        let left = duelists(&mut client);
        if shot < LIVES {
            assert_eq!(left[1].lives, LIVES - shot);
            assert_eq!(left[0].bullets, 1, "kill refunds the bullet");
            // Put the victim back in the host's sights for the next shot.
            let world = server.world_mut();
            let mut q = world.query::<&mut Duelist>();
            for mut d in q.iter_mut(world) {
                if d.player == 1 {
                    d.pos = spawn_points()[1];
                }
            }
            step(&mut server, &mut client);
        }
    }
    assert!(duelists(&mut client).is_empty(), "duel over after 3 kills");
}

#[test]
fn everyone_starts_morally_neutral() {
    use friendslop::features::morality::Morality;
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let all: Vec<Morality> = world
        .query_filtered::<&Morality, With<Player>>()
        .iter(world)
        .copied()
        .collect();
    assert_eq!(all, vec![Morality(0); 2]);
}

#[test]
fn music_boxes_replicate() {
    use friendslop::features::voice_lines::{MUSIC_BOX_COUNT, MusicBox};
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let n = world.query::<&MusicBox>().iter(world).count();
    assert_eq!(n, MUSIC_BOX_COUNT as usize);
}

#[test]
fn client_sees_the_freeze_clock() {
    use friendslop::features::stillness::Stillness;
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let still: Vec<Stillness> = world.query::<&Stillness>().iter(world).copied().collect();
    assert_eq!(still.len(), 1, "exactly one freeze clock");
    assert!(!still[0].frozen, "the world starts unfrozen");
}

#[test]
fn using_a_gods_tear_transforms_the_user() {
    use friendslop::features::gods_tears::{GodsTear, TOTAL_TEARS, Transformation, UseGodsTear};
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
            server.exchange_with_client(client);
        }
    };
    step(&mut server, &mut client);
    let world = client.world_mut();
    let ids: Vec<u32> = world
        .query::<&GodsTear>()
        .iter(world)
        .map(|t| t.id)
        .collect();
    assert_eq!(
        ids.len() as u32,
        TOTAL_TEARS,
        "the whole supply rained down"
    );
    let world = server.world_mut();
    let Some(at) = world
        .query::<&GodsTear>()
        .iter(world)
        .find(|t| t.id == ids[0])
        .map(|t| t.at)
    else {
        panic!("no tear");
    };
    place(&mut server, 1, Vec3::new(at.x, 0.0, at.y));
    client
        .world_mut()
        .client_trigger(UseGodsTear { id: ids[0] });
    step(&mut server, &mut client);
    let world = client.world_mut();
    let mut changes: Vec<(u64, u32)> = world
        .query::<(&Player, &Transformation)>()
        .iter(world)
        .map(|(p, t)| (p.id, t.0))
        .collect();
    changes.sort();
    assert_eq!(changes, vec![(0, 0), (1, 1)], "only the user changed");
    let world = client.world_mut();
    assert_eq!(
        world.query::<&GodsTear>().iter(world).count(),
        ids.len() - 1
    );
    // The supply is limited: no new tears ever fall.
    for _ in 0..20 {
        step(&mut server, &mut client);
    }
    let world = client.world_mut();
    assert_eq!(
        world.query::<&GodsTear>().iter(world).count(),
        ids.len() - 1,
        "used tears are gone for good"
    );
}

#[test]
fn farting_releases_gas_and_tells_everyone() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::fart::{Fart, Farted, Gas};

    #[derive(Resource, Default)]
    struct Heard(Vec<Farted>);

    let (mut server, mut client) = (peer(), peer());
    for app in [&mut server, &mut client] {
        app.init_resource::<Heard>();
        app.world_mut()
            .add_observer(|f: On<Farted>, mut heard: ResMut<Heard>| heard.0.push(*f));
    }
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    step(&mut server, &mut client);

    // Too little gas: nothing happens.
    client.world_mut().client_trigger(Fart { at: Vec2::ZERO });
    step(&mut server, &mut client);
    assert!(client.world().resource::<Heard>().0.is_empty());

    // Fill everyone up, then fart far from every NPC.
    let world = server.world_mut();
    for mut gas in world.query::<&mut Gas>().iter_mut(world) {
        gas.0 = 100;
    }
    step(&mut server, &mut client);
    client.world_mut().client_trigger(Fart {
        at: Vec2::new(20.0, 20.0),
    });
    step(&mut server, &mut client);

    let heard = &client.world().resource::<Heard>().0;
    assert_eq!(heard.len(), 1, "the client hears its own fart");
    assert_eq!(heard[0].player, 1);
    assert!(!heard[0].shamed, "nobody was near");
    let world = client.world_mut();
    let gases: Vec<(u64, u8)> = world
        .query::<(&Player, &Gas)>()
        .iter(world)
        .map(|(p, g)| (p.id, g.0))
        .collect();
    assert!(gases.contains(&(1, 0)), "farter is empty: {gases:?}");
    assert!(
        gases.iter().any(|(id, g)| *id == 0 && *g >= 100),
        "host still full"
    );
}

#[test]
fn fart_tone_is_tuned_by_its_owner_only() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::fart_sound::{FartTone, SetFartTone};

    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    step(&mut server, &mut client);
    client.world_mut().client_trigger(SetFartTone(FartTone {
        pitch: 200.0,
        length: 9_999.0,
        timbre: 0.9,
    }));
    step(&mut server, &mut client);
    let world = client.world_mut();
    let tones: Vec<(u64, FartTone)> = world
        .query::<(&Player, &FartTone)>()
        .iter(world)
        .map(|(p, t)| (p.id, *t))
        .collect();
    let mine = tones.iter().find(|(id, _)| *id == 1).map(|(_, t)| *t);
    let host = tones.iter().find(|(id, _)| *id == 0).map(|(_, t)| *t);
    assert_eq!(mine.map(|t| t.pitch), Some(200.0));
    assert!(mine.is_some_and(|t| t.length <= 4.0), "length is clamped");
    assert_eq!(host, Some(FartTone::default()), "host's tone untouched");
}

#[test]
fn paying_the_landlord_takes_only_the_payers_organs() {
    use friendslop::features::landlord::{BotherLandlord, Tenancy};
    use friendslop::features::organ_currency::OrganPurse;
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
            server.exchange_with_client(client);
        }
    };
    step(&mut server, &mut client);
    place(&mut server, 1, friendslop::features::landlord::LANDLORD_AT);
    client.world_mut().client_trigger(BotherLandlord);
    step(&mut server, &mut client);
    let world = client.world_mut();
    let tenancy: Vec<Tenancy> = world.query::<&Tenancy>().iter(world).cloned().collect();
    assert_eq!(tenancy.len(), 1, "one landlord story");
    assert_eq!(
        tenancy[0].paid, 21,
        "the client's starting organs were paid"
    );
    let mut worth: Vec<(u64, u32)> = world
        .query::<(&Player, &OrganPurse)>()
        .iter(world)
        .map(|(p, o)| (p.id, o.total_value()))
        .collect();
    worth.sort();
    assert_eq!(worth, vec![(0, 21), (1, 0)]);
}

#[test]
fn a_failed_action_does_not_happen_and_everyone_sees_why() {
    use friendslop::features::fate::{FateBoard, FateDice};
    use friendslop::features::mushroom::{EatMushroom, Mushroom};
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
            server.exchange_with_client(client);
        }
    };
    step(&mut server, &mut client);
    server.world_mut().resource_mut::<FateDice>().chance = 1.0;
    to_mushroom(&mut server, 1);
    client.world_mut().client_trigger(EatMushroom);
    step(&mut server, &mut client);
    let world = client.world_mut();
    assert_eq!(
        world.query::<&Mushroom>().iter(world).count(),
        1,
        "not eaten"
    );
    let boards: Vec<FateBoard> = world.query::<&FateBoard>().iter(world).cloned().collect();
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].mishaps.len(), 1);
    assert_eq!(boards[0].mishaps[0].player, 1);
}

#[test]
fn everyone_gets_nonsensical_stats() {
    use friendslop::features::stats::Stats;
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let stats: Vec<Stats> = world
        .query_filtered::<&Stats, With<Player>>()
        .iter(world)
        .copied()
        .collect();
    assert_eq!(stats.len(), 2, "host and client both have stats");
    for s in stats {
        assert!((1..=10).contains(&s.spoonth) && (1..=10).contains(&s.glimbo));
    }
}

#[test]
fn totems_open_doors_and_stay_on_their_side() {
    use friendslop::features::totems::{Door, GrabTotem, Magic, Totem, UseDoor};
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
            server.exchange_with_client(client);
        }
    };
    let totem = |app: &mut App, magic: Magic| {
        let world = app.world_mut();
        world
            .query::<&Totem>()
            .iter(world)
            .find(|t| t.magic == magic)
            .copied()
    };
    step(&mut server, &mut client);
    let Some(fire) = totem(&mut client, Magic::Fire) else {
        panic!("fire totem replicated");
    };
    let Some(wind) = totem(&mut client, Magic::Wind) else {
        panic!("wind totem replicated");
    };
    let world = client.world_mut();
    let Some(wind_door) = world
        .query::<&Door>()
        .iter(world)
        .find(|d| d.magic == Magic::Wind)
        .copied()
    else {
        panic!("wind door replicated");
    };
    // The vault is locked: nobody can reach the fire totem, even standing by it.
    place(&mut server, 1, Vec3::new(fire.at.x, 0.0, fire.at.y));
    client.world_mut().client_trigger(GrabTotem { id: fire.id });
    step(&mut server, &mut client);
    assert_eq!(totem(&mut client, Magic::Fire).and_then(|t| t.holder), None);
    // The host walks to the wind totem, takes it, and unlocks the door from the west.
    place(&mut server, 0, Vec3::new(wind.at.x, 0.0, wind.at.y));
    server.world_mut().client_trigger(GrabTotem { id: wind.id });
    step(&mut server, &mut client);
    place(
        &mut server,
        0,
        Vec3::new(wind_door.at.x - 1.0, 0.0, wind_door.at.y),
    );
    server.world_mut().client_trigger(UseDoor {
        id: wind_door.id,
        side: 0,
    });
    step(&mut server, &mut client);
    // The client stands east of the door (inside), so it can't pull it out,
    // even if it claims to be on the west side...
    place(
        &mut server,
        1,
        Vec3::new(wind_door.at.x + 1.0, 0.0, wind_door.at.y),
    );
    client.world_mut().client_trigger(UseDoor {
        id: wind_door.id,
        side: -1,
    });
    // ...but can now get into the vault.
    client.world_mut().client_trigger(GrabTotem { id: fire.id });
    step(&mut server, &mut client);
    let Some(wind) = totem(&mut client, Magic::Wind) else {
        panic!("wind totem still there");
    };
    assert_eq!(wind.slotted, Some(wind_door.id), "still in the door");
    assert_eq!(
        totem(&mut client, Magic::Fire).and_then(|t| t.holder),
        Some(1)
    );
}

#[test]
fn adopting_a_pet_gives_one_pet_each() {
    use friendslop::features::pets::{AdoptPet, Pet};
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
            server.exchange_with_client(client);
        }
    };
    step(&mut server, &mut client);
    let world = client.world_mut();
    let mut ids: Vec<u32> = world.query::<&Pet>().iter(world).map(|p| p.id).collect();
    ids.sort();
    assert!(ids.len() >= 2, "strays replicated");
    // The client adopts two; only the first one counts. The host wants the
    // first one too, but it's taken, so it gets nothing.
    let world = server.world_mut();
    let Some(at) = world
        .query::<&Pet>()
        .iter(world)
        .find(|p| p.id == ids[0])
        .map(|p| p.at)
    else {
        panic!("no pet");
    };
    place(&mut server, 0, Vec3::new(at.x, 0.0, at.y));
    place(&mut server, 1, Vec3::new(at.x, 0.0, at.y));
    client.world_mut().client_trigger(AdoptPet { id: ids[0] });
    client.world_mut().client_trigger(AdoptPet { id: ids[1] });
    step(&mut server, &mut client);
    server.world_mut().client_trigger(AdoptPet { id: ids[0] });
    step(&mut server, &mut client);
    let world = client.world_mut();
    let mut owners: Vec<(u32, Option<u64>)> = world
        .query::<&Pet>()
        .iter(world)
        .map(|p| (p.id, p.owner))
        .collect();
    owners.sort();
    assert_eq!(owners[0], (ids[0], Some(1)));
    assert_eq!(owners[1], (ids[1], None));
}

#[test]
fn prying_at_halt_is_shared() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::halt::{Halt, PRY_ESCAPE, Pry};

    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    step(&mut server, &mut client);
    client.world_mut().client_trigger(Pry);
    server.world_mut().client_trigger(Pry);
    step(&mut server, &mut client);
    let world = client.world_mut();
    let halts: Vec<Halt> = world.query::<&Halt>().iter(world).copied().collect();
    assert_eq!(halts.len(), 1);
    assert_eq!(halts[0].escape, PRY_ESCAPE * 2, "both pries count");
}

#[test]
fn everyone_gets_the_newer_stats_too() {
    use friendslop::features::stats::Stats;
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    for _ in 0..3 {
        server.update();
        server.exchange_with_client(&mut client);
        client.update();
    }
    let world = client.world_mut();
    let stats: Vec<Stats> = world
        .query_filtered::<&Stats, With<Player>>()
        .iter(world)
        .copied()
        .collect();
    assert_eq!(stats.len(), 2);
    for s in stats {
        assert!(
            [s.hrumph, s.wobblesome, s.pelf]
                .iter()
                .all(|v| (1..=10).contains(v))
        );
    }
}

#[test]
fn requests_are_ignored_while_the_world_stands_still() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::fart::{Fart, Farted, Gas};
    use friendslop::features::stillness::{ACTIVE_SECS, StillClock};

    #[derive(Resource, Default)]
    struct Heard(usize);

    let (mut server, mut client) = (peer(), peer());
    client.init_resource::<Heard>();
    client
        .world_mut()
        .add_observer(|_: On<Farted>, mut heard: ResMut<Heard>| heard.0 += 1);
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    step(&mut server, &mut client);

    // Freeze the world, with everyone full of gas.
    server.world_mut().resource_mut::<StillClock>().elapsed = ACTIVE_SECS + 1.0;
    let world = server.world_mut();
    for mut gas in world.query::<&mut Gas>().iter_mut(world) {
        gas.0 = 50;
    }
    step(&mut server, &mut client);
    client.world_mut().client_trigger(Fart {
        at: Vec2::new(20.0, 20.0),
    });
    step(&mut server, &mut client);
    assert_eq!(client.world().resource::<Heard>().0, 0, "frozen: no fart");
    let world = server.world_mut();
    assert!(
        world.query::<&Gas>().iter(world).all(|g| g.0 == 50),
        "gas neither released nor built up while frozen"
    );

    // Unfreeze: the same request works.
    server.world_mut().resource_mut::<StillClock>().elapsed = 0.0;
    step(&mut server, &mut client);
    client.world_mut().client_trigger(Fart {
        at: Vec2::new(20.0, 20.0),
    });
    step(&mut server, &mut client);
    assert_eq!(client.world().resource::<Heard>().0, 1, "unfrozen: fart");
}

#[test]
fn everyone_gets_a_body_and_only_walks_their_own() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::fps::{WalkControl, Walker, spawn_point};

    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    let walkers = |app: &mut App| {
        let world = app.world_mut();
        let mut v: Vec<(u64, Walker)> = world
            .query::<(&Player, &Walker)>()
            .iter(world)
            .map(|(p, w)| (p.id, *w))
            .collect();
        v.sort_by_key(|(id, _)| *id);
        v
    };
    step(&mut server, &mut client);
    let start = walkers(&mut client);
    assert_eq!(start.len(), 2, "client sees both bodies");
    assert_eq!(start[1].1.pos, spawn_point(1));

    // Player 1 walks forward (towards -Z at yaw 0) for a while.
    for _ in 0..10 {
        client.world_mut().client_trigger(WalkControl {
            stick: Vec2::Y,
            yaw: 0.0,
            jump: false,
            run: false,
        });
        std::thread::sleep(std::time::Duration::from_millis(5));
        step(&mut server, &mut client);
    }
    let after = walkers(&mut client);
    assert!(
        after[1].1.pos.z < start[1].1.pos.z,
        "player 1 walked forward"
    );
    assert_eq!(after[0].1.pos, start[0].1.pos, "the host stayed put");
}

#[test]
fn out_of_reach_interactions_are_rejected() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::interact::Denied;
    use friendslop::features::mushroom::{EatMushroom, Mushroom};

    #[derive(Resource, Default)]
    struct Told(Vec<String>);

    let (mut server, mut client) = (peer(), peer());
    client.init_resource::<Told>();
    client
        .world_mut()
        .add_observer(|d: On<Denied>, mut told: ResMut<Told>| told.0.push(d.text.clone()));
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    step(&mut server, &mut client);
    let world = server.world_mut();
    let Some(at) = world.query::<&Mushroom>().iter(world).next().map(|m| m.at) else {
        panic!("no mushroom");
    };
    // Stand far away from it.
    let far = Vec3::new(-at.x.signum() * 20.0, 0.0, -at.y.signum() * 20.0);
    place(&mut server, 1, far);
    client.world_mut().client_trigger(EatMushroom);
    step(&mut server, &mut client);
    let world = client.world_mut();
    assert_eq!(
        world.query::<&Mushroom>().iter(world).count(),
        1,
        "not eaten"
    );
    let told = &client.world().resource::<Told>().0;
    assert_eq!(told.len(), 1, "told why: {told:?}");
    assert!(told[0].contains("too far"));
}

#[test]
fn one_click_hits_only_one_target() {
    use friendslop::features::interact::pick;
    let mut world = World::new();
    let ids: Vec<Entity> = (0..3).map(|_| world.spawn_empty().id()).collect();
    // A mushroom, a pet and a tear all stacked under the crosshair.
    let cursor = Vec2::new(400.0, 300.0);
    let under = [
        (ids[0], Vec2::new(410.0, 300.0), 40.0),
        (ids[1], Vec2::new(402.0, 303.0), 40.0),
        (ids[2], Vec2::new(380.0, 290.0), 60.0),
    ];
    assert_eq!(pick(cursor, under), Some(ids[1]), "only the closest one");
}

#[test]
fn hide_and_seek_assigns_roles_tags_and_ends() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::hide_and_seek::{HideSeek, Phase, Role, StartHideSeek, TagHider};

    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    step(&mut server, &mut client);
    client.world_mut().client_trigger(StartHideSeek);
    step(&mut server, &mut client);

    // Roles assigned and replicated: one seeker, one hider.
    let world = client.world_mut();
    let roles: Vec<(u64, Role)> = world
        .query::<(&Player, &Role)>()
        .iter(world)
        .map(|(p, r)| (p.id, *r))
        .collect();
    assert_eq!(roles.len(), 2, "both players have a role: {roles:?}");
    let Some(seeker) = roles.iter().find(|r| r.1 == Role::Seeker).map(|r| r.0) else {
        panic!("no seeker: {roles:?}");
    };
    let hider = 1 - seeker;
    assert!(roles.contains(&(hider, Role::Hider)));
    let world = client.world_mut();
    let phase = world.query::<&HideSeek>().single(world).map(|s| s.phase);
    assert_eq!(phase.ok(), Some(Phase::Hiding));

    // Skip the hiding countdown, stand the two together, and tag.
    let world = server.world_mut();
    for mut s in world.query::<&mut HideSeek>().iter_mut(world) {
        s.phase = Phase::Seeking;
    }
    place(&mut server, seeker, Vec3::new(5.0, 0.0, 5.0));
    place(&mut server, hider, Vec3::new(6.0, 0.0, 5.0));
    if seeker == 0 {
        server.world_mut().client_trigger(TagHider { id: hider });
    } else {
        client.world_mut().client_trigger(TagHider { id: hider });
    }
    step(&mut server, &mut client);

    let world = client.world_mut();
    let found = world
        .query::<(&Player, &Role)>()
        .iter(world)
        .any(|(p, r)| p.id == hider && *r == Role::Found);
    assert!(found, "the hider was found and now seeks");
    let Ok(s) = world.query::<&HideSeek>().single(world).cloned() else {
        panic!("no hide and seek state");
    };
    assert_eq!(s.phase, Phase::Results, "nobody left hiding: round over");
    assert!(!s.hiders_won);
    assert_eq!(s.found, 1);
}

#[test]
fn villagers_replicate_and_fool_seekers() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::hide_and_seek::{HideSeek, Phase, Role, StartHideSeek};
    use friendslop::features::villagers::{COUNT, Fooled, PokeVillager, Villager};

    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    step(&mut server, &mut client);
    let world = client.world_mut();
    assert_eq!(world.query::<&Villager>().iter(world).count(), COUNT);

    client.world_mut().client_trigger(StartHideSeek);
    step(&mut server, &mut client);
    let world = server.world_mut();
    for mut s in world.query::<&mut HideSeek>().iter_mut(world) {
        s.phase = Phase::Seeking;
    }
    let Some(seeker) = world
        .query::<(&Player, &Role)>()
        .iter(world)
        .find(|(_, r)| **r == Role::Seeker)
        .map(|(p, _)| p.id)
    else {
        panic!("no seeker");
    };
    let Some(v) = world.query::<&Villager>().iter(world).next().copied() else {
        panic!("no villager");
    };
    place(&mut server, seeker, Vec3::new(v.pos.x + 1.0, 0.0, v.pos.y));
    let poke = PokeVillager { id: v.id };
    if seeker == 0 {
        server.world_mut().client_trigger(poke);
    } else {
        client.world_mut().client_trigger(poke);
    }
    step(&mut server, &mut client);

    let world = server.world_mut();
    let fooled = world
        .query_filtered::<&Player, With<Fooled>>()
        .iter(world)
        .map(|p| p.id)
        .collect::<Vec<_>>();
    assert_eq!(fooled, vec![seeker], "only the seeker is frozen");
}

#[test]
fn hide_and_seek_hides_name_labels_and_heartbeats_near_seekers() {
    use friendslop::features::hide_and_seek::{
        HEARTBEAT_RANGE, Phase, Role, heartbeat_interval, label_hidden,
    };
    let (h, s, f) = (Some(Role::Hider), Some(Role::Seeker), Some(Role::Found));
    // Outside a round every label shows.
    for phase in [Phase::Idle, Phase::Results] {
        assert!(!label_hidden(phase, false, s, h));
    }
    for phase in [Phase::Hiding, Phase::Seeking] {
        // Seekers (and found hiders) see nobody's label but their own.
        assert!(label_hidden(phase, false, s, h));
        assert!(label_hidden(phase, false, f, s));
        assert!(!label_hidden(phase, true, s, s));
        // Hiders' labels are hidden from other hiders too; they still see seekers.
        assert!(label_hidden(phase, false, h, h));
        assert!(!label_hidden(phase, false, h, s));
        assert!(!label_hidden(phase, true, h, h));
    }
    // Heartbeat only within range, faster when closer.
    assert_eq!(heartbeat_interval(HEARTBEAT_RANGE + 0.1), None);
    assert_eq!(heartbeat_interval(f32::INFINITY), None);
    let (Some(near), Some(far)) = (heartbeat_interval(1.0), heartbeat_interval(5.5)) else {
        panic!("seekers in range should thump");
    };
    assert!(near < far);
}

#[test]
fn finishing_a_chapter_moves_everyone_to_the_next() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::campaign::Campaign;
    use friendslop::features::fate::FateDice;
    use friendslop::features::landlord::{Act3, Tenancy};
    use friendslop::features::mushroom::Mushroom;
    use friendslop::features::objective::{Objective, RequestRestart};
    use friendslop::features::pets::Pet;

    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    server.world_mut().resource_mut::<FateDice>().chance = 0.0;
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    let campaign = |app: &mut App| {
        let world = app.world_mut();
        world.query::<&Campaign>().single(world).ok().cloned()
    };
    step(&mut server, &mut client);
    let Some(c) = campaign(&mut client) else {
        panic!("the campaign replicates");
    };
    assert_eq!(c.chapter, 0);
    assert!(!c.solo);

    // Chapter 1: a pack of two pets, a mushroom eaten, the landlord beaten.
    place(&mut server, 0, Vec3::ZERO);
    place(&mut server, 1, Vec3::new(0.5, 0.0, 0.0));
    {
        let world = server.world_mut();
        let mut pets = world.query::<&mut Pet>();
        for (i, mut p) in pets.iter_mut(world).take(2).enumerate() {
            p.owner = Some(i as u64);
            p.at = Vec2::new(i as f32, 0.0);
        }
        let mushroom: Vec<Entity> = world
            .query_filtered::<Entity, With<Mushroom>>()
            .iter(world)
            .collect();
        for e in mushroom {
            world.despawn(e);
        }
        let mut t = world.query::<&mut Tenancy>();
        for mut t in t.iter_mut(world) {
            t.act = Act3::Won;
        }
    }
    step(&mut server, &mut client);
    let Some(c) = campaign(&mut client) else {
        panic!("still there");
    };
    assert!(c.complete(), "chapter 1 done: {c:?}");
    {
        let world = client.world_mut();
        let over = world.query::<&Objective>().iter(world).any(|o| o.over);
        assert!(over, "the round ends with the chapter");
    }

    client.world_mut().client_trigger(RequestRestart);
    step(&mut server, &mut client);
    let Some(c) = campaign(&mut client) else {
        panic!("still there");
    };
    assert_eq!(c.chapter, 1, "on to chapter 2");
    assert!(!c.complete());
}

#[test]
fn some_shameful_farts_are_romantic() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::fart::{Fart, Gas, npc_spots};
    use friendslop::features::romance::Smitten;

    #[derive(Resource, Default)]
    struct Swoons(Vec<Smitten>);

    let (mut server, mut client) = (peer(), peer());
    client.init_resource::<Swoons>();
    client
        .world_mut()
        .add_observer(|s: On<Smitten>, mut swoons: ResMut<Swoons>| swoons.0.push(*s));
    server.connect_client(&mut client);
    let step = |server: &mut App, client: &mut App| {
        for _ in 0..3 {
            server.update();
            server.exchange_with_client(client);
            client.update();
        }
    };
    step(&mut server, &mut client);
    // Dave never moves: fart right in front of him, again and again.
    let dave = npc_spots(0.0, [], [])[0];
    for _ in 0..12 {
        let world = server.world_mut();
        for mut gas in world.query::<&mut Gas>().iter_mut(world) {
            gas.0 = 100;
        }
        step(&mut server, &mut client);
        client.world_mut().client_trigger(Fart { at: dave });
        step(&mut server, &mut client);
    }
    let swoons = &client.world().resource::<Swoons>().0;
    assert!(
        !swoons.is_empty() && swoons.len() < 12,
        "sometimes, not always: {}",
        swoons.len()
    );
    assert!(swoons.iter().all(|s| s.player == 1));
}

/// Run the server and every client a few ticks.
fn step_all(server: &mut App, clients: &mut [&mut App]) {
    for _ in 0..3 {
        server.update();
        for c in clients.iter_mut() {
            server.exchange_with_client(c);
            c.update();
        }
    }
}

fn look_of(app: &mut App, id: u64) -> Option<friendslop::people::CharacterLook> {
    use friendslop::people::CharacterLook;
    let world = app.world_mut();
    world
        .query::<(&Player, &CharacterLook)>()
        .iter(world)
        .find(|(p, _)| p.id == id)
        .map(|(_, l)| *l)
}

#[test]
fn a_clients_character_look_replicates_to_everyone() {
    use friendslop::people::{CharacterLook, SetLook};
    let (mut server, mut a, mut b) = (peer(), peer(), peer());
    server.connect_client(&mut a);
    server.connect_client(&mut b);
    step_all(&mut server, &mut [&mut a, &mut b]);
    assert!(look_of(&mut b, 1).is_some(), "everyone starts with a look");

    let mine = CharacterLook {
        body: 3,
        height: 0.9,
        horns: 2,
        tail: 3,
        accessory: 6,
        ..CharacterLook::random(99)
    };
    a.world_mut().client_trigger(SetLook(mine));
    step_all(&mut server, &mut [&mut a, &mut b]);
    assert_eq!(look_of(&mut server, 1), Some(mine));
    assert_eq!(look_of(&mut b, 1), Some(mine), "the other client sees it");
    assert_ne!(look_of(&mut b, 2), Some(mine), "nobody else changed");

    // The host (no client entity) sets its own look the same way.
    server
        .world_mut()
        .client_trigger(SetLook(CharacterLook::default()));
    step_all(&mut server, &mut [&mut a, &mut b]);
    assert_eq!(look_of(&mut a, 0), Some(CharacterLook::default()));
}

#[test]
fn the_server_clamps_silly_looks() {
    use friendslop::people::{CharacterLook, EYES, SetLook};
    let (mut server, mut client) = (peer(), peer());
    server.connect_client(&mut client);
    step_all(&mut server, &mut [&mut client]);
    client.world_mut().client_trigger(SetLook(CharacterLook {
        height: 40.0,
        girth: -3.0,
        head: f32::NAN,
        eyes: 250,
        body: 77,
        ..CharacterLook::default()
    }));
    step_all(&mut server, &mut [&mut client]);
    let Some(l) = look_of(&mut client, 1) else {
        panic!("look replicates");
    };
    assert_eq!((l.height, l.girth, l.head), (1.0, 0.0, 0.5));
    assert_eq!(l.eyes as usize, EYES.len() - 1);
    assert_eq!(l.body, 3);
    assert_eq!(l, l.clamped());
}

#[test]
fn visiting_abuela_in_heaven_is_shared() {
    use bevy_replicon::prelude::ClientTriggerExt;
    use friendslop::features::abuela::{Abuela, AbuelaSays, LINES, VisitAbuela, chair_at};

    #[derive(Resource, Default)]
    struct Heard(Vec<AbuelaSays>);

    let (mut server, mut client) = (peer(), peer());
    client.init_resource::<Heard>();
    client
        .world_mut()
        .add_observer(|s: On<AbuelaSays>, mut heard: ResMut<Heard>| heard.0.push(*s));
    server.connect_client(&mut client);
    step_all(&mut server, &mut [&mut client]);
    let visits = |app: &mut App| {
        let world = app.world_mut();
        world
            .query::<&Abuela>()
            .iter(world)
            .next()
            .map(|a| a.visits)
    };
    assert_eq!(visits(&mut client), Some(0), "Abuela replicates");

    // From the ground below heaven she can't hear you.
    place(&mut server, 1, chair_at().with_y(0.0));
    client.world_mut().client_trigger(VisitAbuela);
    step_all(&mut server, &mut [&mut client]);
    assert_eq!(visits(&mut client), Some(0));

    // Up in heaven, next to her chair: she greets you, once.
    place(&mut server, 1, chair_at() + Vec3::new(0.0, 0.0, -1.0));
    step_all(&mut server, &mut [&mut client]);
    assert_eq!(visits(&mut client), Some(1), "greeted on arrival");
    let heard = &client.world().resource::<Heard>().0;
    assert_eq!(heard.len(), 1);
    assert_eq!(heard[0].player, 1);
    assert!((heard[0].line as usize) < LINES.len());
    step_all(&mut server, &mut [&mut client]);
    assert_eq!(
        visits(&mut client),
        Some(1),
        "staying put doesn't repeat it"
    );
}
