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
        assert!((1..=10).contains(&s.wombility) && (1..=10).contains(&s.rumpet));
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
    // The vault is locked: nobody can reach the fire totem.
    client.world_mut().client_trigger(GrabTotem { id: fire.id });
    step(&mut server, &mut client);
    assert_eq!(totem(&mut client, Magic::Fire).and_then(|t| t.holder), None);
    // The host takes the wind totem and unlocks the door from the west.
    server.world_mut().client_trigger(GrabTotem { id: wind.id });
    server.world_mut().client_trigger(UseDoor {
        id: wind_door.id,
        side: -1,
    });
    step(&mut server, &mut client);
    // The client can't pull it out from the east side...
    client.world_mut().client_trigger(UseDoor {
        id: wind_door.id,
        side: 1,
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
            [s.spoonth, s.glimbo, s.frumbly]
                .iter()
                .all(|v| (1..=10).contains(v))
        );
    }
}
