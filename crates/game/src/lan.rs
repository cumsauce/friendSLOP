//! LAN discovery (#410): while you host, your game shouts its join code on the
//! local network once a second (a UDP broadcast). The menu listens, and shows
//! a "Join <name>'s commune" button for every game it hears, so friends on
//! the same Wi-Fi never type a code. Joining still goes through iroh with that
//! code, so it's the same connection as joining across the internet.
//!
//! Desktop only: browsers can't send or hear UDP, so there the list stays
//! empty. Nothing here is replicated (no protocol registration).

use bevy::prelude::*;

/// The UDP port the beacons go to (and the menu listens on).
pub const PORT: u16 = 47_410;
/// First word of every beacon, so stray packets are ignored.
const MAGIC: &str = "friendslop-lan-1";
/// Seconds between a host's beacons.
pub const EVERY_SECS: f32 = 1.0;
/// A game not heard from for this long drops off the menu.
pub const FORGET_SECS: f32 = 4.0;

/// One game heard on the network.
#[derive(Clone, Debug, PartialEq)]
pub struct LanGame {
    pub code: String,
    pub host: String,
    /// Seconds since we last heard it.
    pub age: f32,
}

/// Games heard on the local network, newest first-heard order.
#[derive(Resource, Default, Debug)]
pub struct LanGames(pub Vec<LanGame>);

impl LanGames {
    /// Note a beacon (refreshes an already known game).
    pub fn heard(&mut self, code: &str, host: &str) {
        if let Some(g) = self.0.iter_mut().find(|g| g.code == code) {
            g.age = 0.0;
            g.host = host.to_string();
        } else {
            self.0.push(LanGame {
                code: code.to_string(),
                host: host.to_string(),
                age: 0.0,
            });
        }
    }

    /// Age every game by `dt` and forget the quiet ones.
    pub fn tick(&mut self, dt: f32) {
        for g in &mut self.0 {
            g.age += dt;
        }
        self.0.retain(|g| g.age < FORGET_SECS);
    }
}

/// The beacon a host sends (pure, for tests).
pub fn beacon(code: &str, host: &str) -> String {
    let host: String = host.chars().filter(|c| !c.is_control()).take(24).collect();
    format!("{MAGIC} {code} {host}")
}

/// Read a beacon: `(code, host name)`, or `None` for anything else.
pub fn parse(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix(MAGIC)?.strip_prefix(' ')?;
    let (code, host) = rest.split_once(' ').unwrap_or((rest, ""));
    let ok = !code.is_empty() && code.len() <= 128 && code.chars().all(|c| c.is_ascii_hexdigit());
    ok.then_some((code, if host.is_empty() { "a friend" } else { host }))
}

/// What the menu's button says for a game.
pub fn button_text(game: &LanGame) -> String {
    format!("Join {}'s commune (LAN)", game.host)
}

pub fn plugin(app: &mut App) {
    app.init_resource::<LanGames>();
    #[cfg(not(target_arch = "wasm32"))]
    native::build(app);
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

    use super::*;
    use crate::AppState;
    use crate::net::NetStatus;

    /// The menu's listening socket (None if the port is taken: a second copy
    /// of the game on the same computer just won't hear anything).
    #[derive(Resource, Default)]
    struct Listener(Option<UdpSocket>);

    /// The host's sending socket and its timer.
    #[derive(Resource, Default)]
    struct Shouter {
        socket: Option<UdpSocket>,
        wait: f32,
    }

    pub(super) fn build(app: &mut App) {
        app.init_resource::<Listener>()
            .init_resource::<Shouter>()
            .add_systems(OnEnter(AppState::Menu), open_listener)
            .add_systems(OnExit(AppState::Menu), close_listener)
            .add_systems(Update, listen.run_if(in_state(AppState::Menu)))
            .add_systems(Update, shout.run_if(in_state(AppState::InGame)));
    }

    fn open_listener(mut listener: ResMut<Listener>) {
        if listener.0.is_some() {
            return;
        }
        listener.0 = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, PORT)))
            .and_then(|s| s.set_nonblocking(true).map(|_| s))
            .inspect_err(|e| info!("LAN discovery not listening: {e}"))
            .ok();
    }

    fn close_listener(mut listener: ResMut<Listener>, mut games: ResMut<LanGames>) {
        listener.0 = None;
        games.0.clear();
    }

    fn listen(time: Res<Time>, listener: Res<Listener>, mut games: ResMut<LanGames>) {
        games.tick(time.delta_secs());
        let Some(socket) = listener.0.as_ref() else {
            return;
        };
        let mut buf = [0u8; 256];
        // Non-blocking: drain whatever arrived since last frame.
        for _ in 0..32 {
            let Ok((n, _)) = socket.recv_from(&mut buf) else {
                break;
            };
            if let Some((code, host)) = std::str::from_utf8(&buf[..n]).ok().and_then(parse) {
                games.heard(code, host);
            }
        }
    }

    fn host_name() -> String {
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_default()
    }

    fn shout(
        time: Res<Time>,
        status: Res<NetStatus>,
        server: Option<Res<bevy_replicon_iroh::IrohServer>>,
        mut shouter: ResMut<Shouter>,
    ) {
        let (Some(code), Some(_)) = (status.join_code.as_deref(), server) else {
            return;
        };
        shouter.wait -= time.delta_secs();
        if shouter.wait > 0.0 {
            return;
        }
        shouter.wait = EVERY_SECS;
        if shouter.socket.is_none() {
            shouter.socket = UdpSocket::bind(SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)))
                .and_then(|s| {
                    s.set_broadcast(true)?;
                    s.set_nonblocking(true)?;
                    Ok(s)
                })
                .inspect_err(|e| info!("LAN discovery can't broadcast: {e}"))
                .ok();
        }
        let Some(socket) = shouter.socket.as_ref() else {
            return;
        };
        let msg = beacon(code, &host_name());
        // Errors (no network) are fine: the code still works by hand.
        let _ = socket.send_to(msg.as_bytes(), (Ipv4Addr::BROADCAST, PORT));
        let _ = socket.send_to(msg.as_bytes(), (Ipv4Addr::LOCALHOST, PORT));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn beacons_round_trip_and_junk_is_ignored() {
        let b = beacon("beef01", "bben");
        assert_eq!(parse(&b), Some(("beef01", "bben")));
        assert_eq!(parse(&beacon("beef", "")), Some(("beef", "a friend")));
        assert_eq!(parse("hello beef bben"), None);
        assert_eq!(parse("friendslop-lan-1 not-hex bben"), None);
        assert_eq!(parse("friendslop-lan-1 "), None);
    }

    #[test]
    fn games_are_refreshed_then_forgotten() {
        let mut g = LanGames::default();
        g.heard("ab", "x");
        g.tick(FORGET_SECS - 1.0);
        g.heard("ab", "y");
        g.heard("cd", "z");
        assert_eq!(g.0.len(), 2);
        assert_eq!(g.0[0].host, "y");
        g.tick(FORGET_SECS - 0.5);
        assert_eq!(g.0.len(), 2);
        g.tick(1.0);
        assert!(g.0.is_empty());
        assert_eq!(
            button_text(&LanGame {
                code: "ab".into(),
                host: "bben".into(),
                age: 0.0
            }),
            "Join bben's commune (LAN)"
        );
    }
}
