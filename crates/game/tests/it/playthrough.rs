//! Headless: a full six-player playthrough of the campaign (#135, just meat
//! and cheese: "simulate a full 6-player playthrough of the game. Use it to fix
//! game design and make the rules cohesive. Keep every idea").
//!
//! A host and five clients play all five chapters through the real network
//! protocol: six little bots walk to things (their bodies are placed, as in
//! the other headless tests, since there's no physics here) and send exactly
//! the requests a player's clicks and keys send. On the way they also do the
//! things that aren't on the checklist (farting, the lex pile, Abuela, spells),
//! so every idea gets played. The Stillness, failed actions (#8) and the
//! landlord's clock all run as in the game, on a simulated clock.
//!
//! Run it with `cargo test -p friendslop --test playthrough -- --nocapture`
//! to read the play log, with game minutes for every chapter.

use std::time::Duration;

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy::time::TimeUpdateStrategy;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::features::abuela::{Abuela, VisitAbuela, chair_at};
use friendslop::features::campaign::{CHAPTERS, Campaign, Task, heaven_tiers};
use friendslop::features::duel::{AnswerDuel, ChallengeDuel, DuelShoot, Duelist};
use friendslop::features::fart::{Fart, Gas, npc_spots};
use friendslop::features::fps::Walker;
use friendslop::features::gods_tears::{GodsTear, TOTAL_TEARS, Transformation, UseGodsTear};
use friendslop::features::halt::{Halt, Pry};
use friendslop::features::hide_and_seek::{HideSeek, Phase, Role, StartHideSeek, TagHider};
use friendslop::features::landlord::{Act3, BotherLandlord, LANDLORD_AT, Tenancy};
use friendslop::features::morality::Morality;
use friendslop::features::mushroom::{EatMushroom, Mushroom};
use friendslop::features::objective::{Objective, RequestRestart};
use friendslop::features::organ_currency::OrganPurse;
use friendslop::features::pets::{AdoptPet, Pet, PetLure};
use friendslop::features::punishment::{AttackMode, Strikes};
use friendslop::features::sesbian_lex::{SesbianLex, SesbianLexToggle};
use friendslop::features::smoking::{Stash, TakeEgg, VisitPaul, egg_spot, paul_spot};
use friendslop::features::stats::Stats;
use friendslop::features::stillness::Stillness;
use friendslop::features::totems::{CastSpells, Door, GrabTotem, Magic, Totem, UseDoor};
use friendslop::features::warehouse::{
    GiveToJegg, INSIDE_Y, KnockDoor, LeaveWarehouse, SoulPiece, Warehouse, door_at, exit_at,
    inside, jegg_at,
};
use friendslop::features::wendys::{Burger, BurgerBag, TakeBurger, burger_spot};
use friendslop::player::{LocalPlayer, Player};

const PLAYERS: u64 = 6;
/// Simulated seconds per frame.
const DT: f32 = 0.2;

fn peer() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        StatesPlugin,
        RepliconPlugins.set(ServerPlugin::new(PostUpdate)),
        ProtocolPlugin,
    ));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        DT,
    )));
    app.finish();
    app
}

struct Sim {
    server: App,
    /// `clients[i]` plays `Player { id: i + 1 }`.
    clients: Vec<App>,
    /// Game seconds since the host pressed Host.
    t: f32,
    log: Vec<String>,
}

impl Sim {
    fn new() -> Self {
        let mut server = peer();
        let mut clients: Vec<App> = (1..PLAYERS).map(|_| peer()).collect();
        for c in &mut clients {
            server.connect_client(c);
        }
        let mut sim = Self {
            server,
            clients,
            t: 0.0,
            log: Vec::new(),
        };
        sim.run(1.0);
        for (i, c) in sim.clients.iter().enumerate() {
            assert_eq!(
                c.world().get_resource::<LocalPlayer>().map(|l| l.0),
                Some(i as u64 + 1),
                "client {i} plays player {}",
                i + 1
            );
        }
        sim
    }

    fn step(&mut self) {
        self.server.update();
        for c in &mut self.clients {
            self.server.exchange_with_client(c);
            c.update();
            self.server.exchange_with_client(c);
        }
        self.t += DT;
    }

    fn run(&mut self, secs: f32) {
        for _ in 0..(secs / DT).ceil() as usize {
            self.step();
        }
    }

    fn say(&mut self, line: impl Into<String>) {
        let line = format!("[{:>5.1} min] {}", self.t / 60.0, line.into());
        println!("{line}");
        self.log.push(line);
    }

    /// Player `id` presses a key or clicks something.
    fn send(&mut self, id: u64, ev: impl Event) {
        if id == 0 {
            self.server.world_mut().client_trigger(ev);
        } else {
            self.clients[id as usize - 1].world_mut().client_trigger(ev);
        }
    }

    /// Player `id` walks over to `at`.
    fn walk(&mut self, id: u64, at: Vec3) {
        let world = self.server.world_mut();
        for (p, mut w) in world.query::<(&Player, &mut Walker)>().iter_mut(world) {
            if p.id == id {
                w.pos = at;
            }
        }
    }

    fn walk_xz(&mut self, id: u64, at: Vec2) {
        self.walk(id, Vec3::new(at.x, 0.0, at.y));
    }

    /// Everything of one kind, as the server sees it.
    fn all<C: Component + Clone>(&mut self) -> Vec<C> {
        let world = self.server.world_mut();
        world.query::<&C>().iter(world).cloned().collect()
    }

    fn one<C: Component + Clone>(&mut self) -> C {
        let Some(c) = self.all::<C>().into_iter().next() else {
            panic!("no {}", std::any::type_name::<C>());
        };
        c
    }

    fn per_player<C: Component + Clone>(&mut self) -> Vec<(u64, C)> {
        let world = self.server.world_mut();
        let mut v: Vec<(u64, C)> = world
            .query::<(&Player, &C)>()
            .iter(world)
            .map(|(p, c)| (p.id, c.clone()))
            .collect();
        v.sort_by_key(|(id, _)| *id);
        v
    }

    fn campaign(&mut self) -> Campaign {
        self.one::<Campaign>()
    }

    fn frozen(&mut self) -> bool {
        self.all::<Stillness>().iter().any(|s| s.frozen)
    }

    /// Nobody can do anything during the Stillness: chat until it's over.
    fn wait_thaw(&mut self) {
        while self.frozen() {
            self.step();
        }
    }

    /// Keep doing `act` (waiting out the Stillness, retrying failed actions)
    /// until `done`. Panics after `max` game seconds: a stuck playthrough is
    /// a design bug.
    fn until(
        &mut self,
        what: &str,
        max: f32,
        mut act: impl FnMut(&mut Self),
        mut done: impl FnMut(&mut Self) -> bool,
    ) {
        let start = self.t;
        while !done(self) {
            assert!(
                self.t - start < max,
                "stuck for {max}s: {what}\n{}",
                self.log.join("\n")
            );
            self.wait_thaw();
            act(self);
            self.run(0.6);
        }
    }

    fn task_progress(&mut self, task: Task) -> Option<(u32, u32)> {
        let c = self.campaign();
        let i = c.current().tasks.iter().position(|&t| t == task)?;
        Some((c.got(i), task.need()))
    }

    fn task_done(&mut self, task: Task) -> bool {
        let c = self.campaign();
        c.current()
            .tasks
            .iter()
            .position(|&t| t == task)
            .is_none_or(|i| c.task_done(i))
    }
}

// ------------------------------------------------------------------ the bots

fn pack(sim: &mut Sim) {
    let mut pets = sim.all::<Pet>();
    pets.sort_by_key(|p| p.id);
    let owners: Vec<u64> = (0..PLAYERS).take(pets.len()).collect();
    for (&id, pet) in owners.iter().zip(&pets) {
        let pet_id = pet.id;
        sim.until(
            "adopt a pet",
            60.0,
            |s| {
                let Some(at) = s.all::<Pet>().iter().find(|p| p.id == pet_id).map(|p| p.at) else {
                    return;
                };
                s.walk_xz(id, at);
                s.send(id, AdoptPet { id: pet_id });
            },
            |s| {
                s.all::<Pet>()
                    .iter()
                    .any(|p| p.id == pet_id && p.owner == Some(id))
            },
        );
    }
    // Everyone with a pet meets at the pond and waits for the pets to catch up.
    sim.until(
        "leash the pack",
        90.0,
        |s| {
            for (k, &id) in owners.iter().enumerate() {
                let at = Vec2::new(5.0 + k as f32 * 0.6, 5.0);
                s.walk_xz(id, at);
                // Your client keeps telling your pet where you are.
                s.send(id, PetLure { at });
            }
        },
        |s| s.task_done(Task::Pack),
    );
    sim.say(format!(
        "{} pets adopted and leashed into one pack ({} players have no pet: there are only {})",
        owners.len(),
        PLAYERS as usize - owners.len(),
        pets.len()
    ));
}

fn mushrooms(sim: &mut Sim, n: u32) {
    for k in 0..n {
        let eater = (k as u64 * 2 + 1) % PLAYERS;
        let before = sim.task_progress(Task::Mushroom(n)).map_or(0, |p| p.0);
        sim.until(
            "eat the mushroom",
            120.0,
            |s| {
                let Some(at) = s.all::<Mushroom>().first().map(|m| m.at) else {
                    return; // it grows back after a while
                };
                s.walk_xz(eater, at);
                s.send(eater, EatMushroom);
            },
            |s| s.task_progress(Task::Mushroom(n)).map_or(0, |p| p.0) > before,
        );
        sim.say(format!("player {eater} eats the voice mushroom"));
    }
}

fn door_of(sim: &mut Sim, magic: Magic) -> Door {
    let Some(d) = sim.all::<Door>().into_iter().find(|d| d.magic == magic) else {
        panic!("no {magic:?} door");
    };
    d
}

fn totem_of(sim: &mut Sim, magic: Magic) -> Totem {
    let Some(t) = sim.all::<Totem>().into_iter().find(|t| t.magic == magic) else {
        panic!("no {magic:?} totem");
    };
    t
}

fn open_vault(sim: &mut Sim, who: u64) {
    let wind = totem_of(sim, Magic::Wind);
    sim.until(
        "pick up the wind totem",
        60.0,
        |s| {
            s.walk_xz(who, wind.at);
            s.send(who, GrabTotem { id: wind.id });
        },
        |s| totem_of(s, Magic::Wind).holder == Some(who),
    );
    // A spell, for the hell of it.
    sim.send(who, CastSpells);
    let door = door_of(sim, Magic::Wind);
    sim.until(
        "open the wind door",
        60.0,
        |s| {
            s.walk_xz(who, door.at - Vec2::X);
            s.send(
                who,
                UseDoor {
                    id: door.id,
                    side: -1,
                },
            );
        },
        |s| door_of(s, Magic::Wind).is_open(),
    );
    sim.say(format!("player {who} opens the vault's wind door"));
}

fn fire_totem(sim: &mut Sim, who: u64) {
    if !door_of(sim, Magic::Wind).is_open() {
        open_vault(sim, (who + 1) % PLAYERS);
    }
    let fire = totem_of(sim, Magic::Fire);
    sim.until(
        "carry out the fire totem",
        60.0,
        |s| {
            s.walk_xz(who, fire.at);
            s.send(who, GrabTotem { id: fire.id });
        },
        |s| totem_of(s, Magic::Fire).holder == Some(who),
    );
    sim.say(format!(
        "player {who} carries the Fire totem out of the vault"
    ));
}

fn halt(sim: &mut Sim, n: u32) {
    let start = sim.t;
    sim.until(
        "break out of HALT",
        600.0,
        |s| {
            for id in 0..PLAYERS {
                s.send(id, Pry);
            }
        },
        |s| s.task_done(Task::Halt(n)),
    );
    let h = sim.one::<Halt>();
    sim.say(format!(
        "everyone mashes G: broke out of HALT {} time(s) in {:.0}s (HALT dragged them back in)",
        h.escapes,
        sim.t - start
    ));
}

fn tears(sim: &mut Sim, n: u32) {
    let start_left = sim.all::<GodsTear>().len();
    let mut k = 0u64;
    sim.until(
        "use gods tears",
        300.0,
        |s| {
            let Some(tear) = s.all::<GodsTear>().first().copied() else {
                return;
            };
            let who = k % PLAYERS;
            k += 1;
            s.walk_xz(who, tear.at);
            s.send(who, UseGodsTear { id: tear.id });
        },
        |s| s.task_done(Task::Tears(n)),
    );
    let left = sim.all::<GodsTear>().len();
    let forms: Vec<String> = sim
        .per_player::<Transformation>()
        .iter()
        .map(|(id, t)| format!("p{id}: {}", t.label()))
        .collect();
    sim.say(format!(
        "{} gods tears used ({left} of {TOTAL_TEARS} left on the ground); {}",
        start_left - left,
        forms.join(", ")
    ));
}

fn hide_and_seek(sim: &mut Sim, n: u32) {
    for _ in 0..n {
        let before = sim.task_progress(Task::HideSeek(n)).map_or(0, |p| p.0);
        sim.until(
            "start hide and seek",
            60.0,
            |s| s.send(3, StartHideSeek),
            |s| s.one::<HideSeek>().phase != Phase::Idle,
        );
        // (It may already be running from the last chapter or round.)
        // Everyone scatters while the seeker counts.
        for id in 0..PLAYERS {
            let a = id as f32 * 1.1;
            sim.walk_xz(id, Vec2::new(a.cos() * 30.0, a.sin() * 30.0));
        }
        sim.until(
            "the seeker counts",
            120.0,
            |_| {},
            |s| s.one::<HideSeek>().phase == Phase::Seeking,
        );
        sim.until(
            "find everyone",
            240.0,
            |s| {
                let roles = s.per_player::<Role>();
                let Some(&(seeker, _)) = roles.iter().find(|(_, r)| r.seeks()) else {
                    return;
                };
                let Some(&(hider, _)) = roles.iter().find(|(_, r)| *r == Role::Hider) else {
                    return;
                };
                let Some(at) = s
                    .per_player::<Walker>()
                    .into_iter()
                    .find(|(id, _)| *id == hider)
                    .map(|(_, w)| w.pos)
                else {
                    return;
                };
                s.walk(seeker, at + Vec3::X);
                s.send(seeker, TagHider { id: hider });
            },
            |s| {
                s.task_progress(Task::HideSeek(n)).map_or(0, |p| p.0) > before
                    || s.one::<HideSeek>().phase == Phase::Results
            },
        );
        let hs = sim.one::<HideSeek>();
        sim.say(format!(
            "hide and seek round {}: {} found, {}",
            hs.round,
            hs.found,
            if hs.hiders_won {
                "hiders win"
            } else {
                "seekers win"
            }
        ));
        // The results show for a while, then the next game starts on its own.
        sim.until(
            "results",
            30.0,
            |_| {},
            |s| s.one::<HideSeek>().phase != Phase::Results,
        );
    }
}

fn duels(sim: &mut Sim, n: u32) {
    for k in 0..n as u64 {
        let (a, b) = ((k * 2) % PLAYERS, (k * 2 + 1) % PLAYERS);
        let before = sim.task_progress(Task::Duel(n)).map_or(0, |p| p.0);
        sim.until(
            "challenge to a duel",
            60.0,
            |s| {
                s.send(a, ChallengeDuel { target: b });
                s.run(0.6);
                s.send(
                    b,
                    AnswerDuel {
                        from: a,
                        accept: true,
                    },
                );
            },
            |s| !s.all::<Duelist>().is_empty(),
        );
        sim.until(
            "win the duel",
            120.0,
            |s| s.send(a, DuelShoot),
            |s| s.all::<Duelist>().is_empty(),
        );
        assert!(sim.task_progress(Task::Duel(n)).map_or(0, |p| p.0) > before);
        sim.say(format!("player {a} beats player {b} in a duel"));
    }
}

fn landlord(sim: &mut Sim) {
    let start = sim.t;
    let organs: u32 = sim
        .per_player::<OrganPurse>()
        .iter()
        .map(|(_, p)| p.total_value())
        .sum();
    // Everyone goes to the landlord and pays up while they've got organs.
    for id in 0..PLAYERS {
        let a = id as f32;
        sim.walk(
            id,
            LANDLORD_AT + Vec3::new(a.cos() * 1.5, 0.0, a.sin() * 1.5),
        );
    }
    let mut paid_until = None;
    sim.until(
        "beat the landlord",
        900.0,
        |s| {
            let t = s.one::<Tenancy>();
            let purses = s.per_player::<OrganPurse>();
            for (id, purse) in purses {
                // You can't pay with an empty purse (the client says so).
                if t.act != Act3::Legal || !purse.0.is_empty() {
                    s.send(id, BotherLandlord);
                }
            }
            if t.act != Act3::Legal && paid_until.is_none() {
                paid_until = Some(s.t);
            }
        },
        |s| s.one::<Tenancy>().act == Act3::Won,
    );
    let t = sim.one::<Tenancy>();
    let act1 = paid_until.unwrap_or(sim.t) - start;
    sim.say(format!(
        "the landlord is beaten in {:.1} min (Act I took {:.1} min, the group held {organs} \
         in organs and he raised the rent to {})",
        (sim.t - start) / 60.0,
        act1 / 60.0,
        t.rent
    ));
}

/// Abuela's job (#165): somebody climbs up and talks to her.
fn abuela(sim: &mut Sim) {
    let visitor = 1;
    sim.until(
        "visit Abuela",
        60.0,
        |s| {
            s.walk(visitor, chair_at() + Vec3::new(0.0, 0.0, -1.0));
            s.send(visitor, VisitAbuela);
        },
        |s| s.task_done(Task::Abuela),
    );
    sim.say(format!(
        "player {visitor} climbs to heaven for Abuela's blessing"
    ));
    sim.walk_xz(visitor, Vec2::ZERO);
}

/// Wendy's job (#165): grab burgers until the chapter has enough.
fn burgers(sim: &mut Sim, n: u32) {
    let mut k = 0u64;
    sim.until(
        "grab burgers at Wendy's",
        180.0,
        |s| {
            let Some(slot) = s.all::<Burger>().first().map(|b| b.slot) else {
                return; // restocking
            };
            let who = k % PLAYERS;
            k += 1;
            s.walk(who, burger_spot(slot));
            s.send(who, TakeBurger { slot });
        },
        |s| s.task_done(Task::Burgers(n)),
    );
    let bags: u32 = sim.per_player::<BurgerBag>().iter().map(|(_, b)| b.0).sum();
    sim.say(format!(
        "the group grabs {n} human meat burgers ({bags} carried)"
    ));
}

/// Sewer Paul's job (#165): pocket a crow egg, climb down, trade it.
fn paul(sim: &mut Sim, n: u32) {
    let who = 3;
    sim.until(
        "trade an egg with Sewer Paul",
        180.0,
        |s| {
            let eggs = s
                .per_player::<Stash>()
                .into_iter()
                .find(|(id, _)| *id == who)
                .map_or(0, |(_, st)| st.eggs);
            if eggs == 0 {
                for nest in 0..3 {
                    s.walk(who, egg_spot(nest));
                    s.send(who, TakeEgg { nest });
                    s.run(0.4);
                }
            } else {
                s.walk(who, paul_spot());
                s.send(who, VisitPaul);
            }
        },
        |s| s.task_done(Task::Paul(n)),
    );
    sim.say(format!(
        "player {who} trades a crow egg to Sewer Paul for a cig"
    ));
    sim.walk_xz(who, Vec2::ZERO);
}

/// Jegg's job (#165): everyone agrees at the warehouse door, then bots fetch
/// soul pieces one at a time (the monster may get some of them first).
fn jegg(sim: &mut Sim, n: u32) {
    let start = sim.t;
    sim.until(
        "everyone agrees to go into the warehouse",
        120.0,
        |s| {
            if s.one::<Warehouse>().seed.is_some() {
                return;
            }
            for id in 0..PLAYERS {
                s.walk(id, door_at() - Vec3::Z * 1.5);
            }
            s.run(0.4);
            let votes = s.one::<Warehouse>().votes;
            for id in 0..PLAYERS {
                if !votes.contains(&id) {
                    s.send(id, KnockDoor);
                }
            }
        },
        |s| s.one::<Warehouse>().seed.is_some() || s.task_done(Task::Jegg(n)),
    );
    let mut k = 0u64;
    sim.until(
        "bring soul pieces to Jegg",
        600.0,
        |s| {
            let who = k % PLAYERS;
            k += 1;
            let carried = s
                .all::<SoulPiece>()
                .into_iter()
                .find(|p| p.carrier == Some(who));
            let pos = s
                .per_player::<Walker>()
                .into_iter()
                .find(|(id, _)| *id == who)
                .map(|(_, w)| w.pos);
            let Some(pos) = pos else { return };
            if carried.is_some() {
                if inside(pos) {
                    s.walk(who, exit_at() + Vec3::Z);
                    s.send(who, LeaveWarehouse);
                    s.run(0.4);
                }
                s.walk(who, jegg_at() - Vec3::Z);
                s.send(who, GiveToJegg);
                return;
            }
            if !inside(pos) {
                // Back in through the door (or, if it closed, agree again).
                s.walk(who, door_at() - Vec3::Z * 1.5);
                s.send(who, KnockDoor);
                return;
            }
            let Some(piece) = s
                .all::<SoulPiece>()
                .into_iter()
                .find(|p| p.carrier.is_none())
            else {
                return;
            };
            s.walk(who, piece.pos.with_y(INSIDE_Y));
            s.send(
                who,
                friendslop::features::warehouse::TakeSoul { id: piece.id },
            );
        },
        |s| s.task_done(Task::Jegg(n)),
    );
    for id in 0..PLAYERS {
        sim.walk_xz(id, Vec2::new(id as f32, 0.0));
    }
    sim.run(1.0);
    let held = sim.one::<Warehouse>().delivered;
    let mins = (sim.t - start) / 60.0;
    sim.say(format!(
        "into the warehouse: Jegg holds {held} soul pieces after {mins:.1} min"
    ));
}

/// The things nobody has to do, done anyway, so every idea gets played.
fn side_quests(sim: &mut Sim, chapter: usize) {
    // Somebody farts right next to Dave (and the commune) and is shamed.
    let farter = (chapter as u64 + 4) % PLAYERS;
    let dave = npc_spots(0.0, [], [], [])[0];
    sim.until(
        "build up some gas",
        60.0,
        |_| {},
        |s| {
            s.per_player::<Gas>()
                .iter()
                .any(|(id, g)| *id == farter && g.0 >= 50)
        },
    );
    sim.wait_thaw();
    sim.walk_xz(farter, dave);
    sim.send(farter, Fart { at: dave });
    sim.run(1.0);
    let strikes = sim
        .per_player::<Strikes>()
        .into_iter()
        .find(|(id, _)| *id == farter)
        .map_or(0, |(_, s)| s.0);
    let chase = sim.all::<AttackMode>();
    sim.say(format!(
        "player {farter} farts next to Dave: caught {strikes}/4{}",
        if chase.is_empty() {
            String::new()
        } else {
            format!(", ATTACK MODE on player {}", chase[0].target)
        }
    ));

    // Two of them pile in for a smooch.
    let (a, b) = ((chapter as u64) % PLAYERS, (chapter as u64 + 1) % PLAYERS);
    sim.wait_thaw();
    sim.walk_xz(a, Vec2::new(-5.0, 8.0));
    sim.walk_xz(b, Vec2::new(-4.0, 8.5));
    sim.run(0.6);
    sim.send(a, SesbianLexToggle);
    sim.run(0.6);
    sim.send(b, SesbianLexToggle);
    sim.run(4.0);
    let heat = sim.all::<SesbianLex>().first().map_or(0, |p| p.heat);
    sim.send(a, SesbianLexToggle);
    sim.send(b, SesbianLexToggle);
    sim.run(0.6);
    sim.say(format!(
        "players {a} and {b} pile in (heat {heat}), then peel apart"
    ));

    // Somebody climbs to heaven to see Abuela.
    let visitor = (chapter as u64 + 2) % PLAYERS;
    let before = sim.one::<Abuela>().visits;
    sim.wait_thaw();
    sim.walk(visitor, chair_at() + Vec3::new(0.0, 0.0, -1.0));
    sim.run(1.0);
    sim.send(visitor, VisitAbuela);
    sim.run(1.0);
    let visits = sim.one::<Abuela>().visits - before;
    sim.say(format!(
        "player {visitor} visits Abuela in heaven ({visits} hello)"
    ));
    sim.walk_xz(visitor, Vec2::ZERO);
}

fn play_chapter(sim: &mut Sim) {
    let c = sim.campaign();
    let ch = c.current();
    sim.say(format!("=== CHAPTER {}: {} ===", c.chapter + 1, ch.name));
    let start = sim.t;
    side_quests(sim, c.chapter);
    for &task in ch.tasks {
        match task {
            Task::Pack => pack(sim),
            Task::Mushroom(n) => mushrooms(sim, n),
            Task::VaultDoor => open_vault(sim, 2),
            Task::FireTotem => fire_totem(sim, 4),
            Task::Halt(n) => halt(sim, n),
            Task::Tears(n) => tears(sim, n),
            Task::HideSeek(n) => hide_and_seek(sim, n),
            Task::Duel(n) => duels(sim, n),
            Task::Landlord => landlord(sim),
            Task::Abuela => abuela(sim),
            Task::Burgers(n) => burgers(sim, n),
            Task::Paul(n) => paul(sim, n),
            Task::Jegg(n) => jegg(sim, n),
        }
        assert!(
            sim.task_done(task),
            "{task:?} should be ticked\n{}",
            sim.log.join("\n")
        );
    }
    sim.run(1.0);
    assert!(sim.one::<Objective>().over, "the chapter is over");
    let c = sim.campaign();
    let morality: Vec<String> = sim
        .per_player::<Morality>()
        .iter()
        .map(|(id, m)| format!("p{id} {:+}", m.0))
        .collect();
    sim.say(format!(
        "chapter {} done in {:.1} min, Heaven Pass tier {}/{}; morality {}",
        c.chapter + 1,
        (sim.t - start) / 60.0,
        c.tier(),
        heaven_tiers(),
        morality.join(" ")
    ));
}

#[test]
fn six_friends_free_the_commune_and_get_into_heaven() {
    let mut sim = Sim::new();
    assert_eq!(sim.all::<Player>().len(), PLAYERS as usize);
    let stats_at_start = sim.per_player::<Stats>();

    for chapter in 0..CHAPTERS.len() {
        assert_eq!(sim.campaign().chapter, chapter);
        play_chapter(&mut sim);
        if chapter + 1 < CHAPTERS.len() {
            // The tears rained only once (#43): the ones used stay used.
            let left = sim.all::<GodsTear>().len();
            sim.until(
                "press Enter for the next chapter",
                60.0,
                |s| s.send(5, RequestRestart),
                |s| s.campaign().chapter == chapter + 1,
            );
            sim.run(1.0);
            assert_eq!(
                sim.all::<GodsTear>().len(),
                left,
                "no new tears fall between chapters"
            );
        }
    }

    // The finale: the Commune is free and everyone got into heaven.
    let c = sim.campaign();
    assert!(c.finale(), "chapter five ends the campaign");
    assert_eq!(c.tier(), heaven_tiers(), "the Heaven Pass is maxed out");
    assert_eq!(
        sim.per_player::<Stats>(),
        stats_at_start,
        "stats are rolled when you join, and stay yours all campaign"
    );
    assert!(
        sim.per_player::<Morality>().iter().all(|(_, m)| m.0 > 0),
        "everyone harmed the landlord a lot: always virtuous (#43)"
    );
    sim.say("THE COMMUNE IS FREE. Everyone gets into heaven.");

    // Enter starts a brand-new campaign: a fresh world, tears and all.
    sim.until(
        "start again",
        60.0,
        |s| s.send(0, RequestRestart),
        |s| s.campaign().chapter == 0,
    );
    sim.run(1.0);
    assert_eq!(sim.all::<GodsTear>().len(), TOTAL_TEARS as usize);
    assert!(
        sim.per_player::<Transformation>()
            .iter()
            .all(|(_, t)| t.0 == 0)
    );
    assert!(sim.per_player::<Morality>().iter().all(|(_, m)| m.0 == 0));
    sim.say(format!(
        "a new campaign begins; the whole thing took {:.0} game minutes",
        sim.t / 60.0
    ));
}
