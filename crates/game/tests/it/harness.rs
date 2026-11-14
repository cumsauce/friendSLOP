//! Shared headless harness: a listen server and N clients, stepped together.
//!
//! Deterministic, no sockets: replicon's test apps exchange messages only when
//! we say so (`step`). Every test that needs multiplayer plumbing should build
//! on [`Sim`] instead of wiring `App`s by hand.

// Not every test uses every helper yet.
#![allow(dead_code)]

use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use bevy_replicon::prelude::*;
use bevy_replicon::test_app::ServerTestAppExt;
use friendslop::ProtocolPlugin;
use friendslop::player::Player;

/// One peer wired the way `replication.rs` does: minimal plugins plus the
/// game's protocol registrations, server plugin in `PostUpdate`.
pub fn peer() -> App {
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

/// Which app to look at: the host/server, or one of the connected clients.
#[derive(Clone, Copy, Debug)]
pub enum Side {
    Server,
    Client(usize),
}

/// A listen server plus `n` clients, stepped in lockstep.
pub struct Sim {
    pub server: App,
    pub clients: Vec<App>,
    pub t: f32,
    pub dt: f32,
}

impl Sim {
    /// Spin up a server and `n_clients` clients, connect them, and step a few
    /// times so the host and client `Player`s replicate.
    pub fn new(n_clients: usize) -> Self {
        let mut server = peer();
        let mut clients = Vec::new();
        for _ in 0..n_clients {
            let mut client = peer();
            server.connect_client(&mut client);
            clients.push(client);
        }
        let mut sim = Sim {
            server,
            clients,
            t: 0.0,
            dt: 1.0 / 60.0,
        };
        for _ in 0..3 {
            sim.step();
        }
        sim
    }

    /// One frame everywhere: server updates, each client exchanges, updates,
    /// and exchanges again (so same-tick replies come back).
    pub fn step(&mut self) {
        self.server.update();
        for client in &mut self.clients {
            self.server.exchange_with_client(client);
            client.update();
            self.server.exchange_with_client(client);
        }
        self.t += self.dt;
    }

    /// Advance `secs` of simulated time.
    pub fn run(&mut self, secs: f32) {
        let steps = (secs / self.dt).round().max(0.0) as usize;
        for _ in 0..steps {
            self.step();
        }
    }

    /// Connect one more client mid-game (after the initial `new`). Returns its
    /// index in `clients` (so player id = index + 1).
    pub fn join_late(&mut self) -> usize {
        let mut client = peer();
        self.server.connect_client(&mut client);
        self.clients.push(client);
        for _ in 0..3 {
            self.step();
        }
        self.clients.len() - 1
    }

    /// Kick client `idx`, then step three times so the server settles.
    pub fn disconnect(&mut self, idx: usize) {
        self.server.disconnect_client(&mut self.clients[idx]);
        for _ in 0..3 {
            self.step();
        }
    }

    /// Trigger `ev` as coming from `player`'s app: player 0 is the host and
    /// plays on the server app, player `n` is `clients[n - 1]`.
    pub fn send<E: Event>(&mut self, player: u64, ev: E)
    where
        for<'a> <E as Event>::Trigger<'a>: Default,
    {
        let app = if player == 0 {
            &mut self.server
        } else {
            &mut self.clients[(player - 1) as usize]
        };
        app.world_mut().trigger(ev);
    }

    /// Trigger `ev_a` from player `a` and `ev_b` from player `b`, then step
    /// once: both land in the same tick (a same-tick batch).
    pub fn send_pair<EA: Event, EB: Event>(&mut self, a: u64, b: u64, ev_a: EA, ev_b: EB)
    where
        for<'x> <EA as Event>::Trigger<'x>: Default,
        for<'y> <EB as Event>::Trigger<'y>: Default,
    {
        self.send(a, ev_a);
        self.send(b, ev_b);
        self.step();
    }

    /// How many entities with `C` one side can see.
    pub fn count<C: Component>(&mut self, side: Side) -> usize {
        let world = match side {
            Side::Server => self.server.world_mut(),
            Side::Client(i) => self.clients[i].world_mut(),
        };
        world.query::<&C>().iter(world).count()
    }

    /// Step until `f` says we're done, at most `secs` of simulated time.
    /// Returns whether `f` ever held.
    pub fn until(&mut self, secs: f32, f: impl Fn(&mut Sim) -> bool) -> bool {
        let max_steps = (secs / self.dt).ceil().max(1.0) as usize;
        for _ in 0..max_steps {
            if f(self) {
                return true;
            }
            self.step();
        }
        f(self)
    }
}

#[test]
fn sim_smoke_two_clients_three_players() {
    let mut sim = Sim::new(2);
    assert_eq!(
        sim.count::<Player>(Side::Server),
        3,
        "host + two clients on the server"
    );
}
