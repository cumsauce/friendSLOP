//! PID 1 can kill you (#238, bben: "pid 1, usually systemd").
//!
//! Every player is a service, and systemd watches them all. Moving or
//! turning your head pings the watchdog. Stand perfectly still for
//! [`WATCHDOG_SECS`] and PID 1 decides you've hung: it warns you
//! [`WARN_SECS`] before, then SIGKILLs you (you die, see `afterlife.rs`).
//! Everyone sees the journal line. The clock stops while the world is frozen
//! (`stillness.rs`) and while you're dead.
//!
//! Server only, apart from the toast. No new art.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::player::{LocalPlayer, Player};

/// Tuning numbers: `assets/data/afterlife/systemd.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// How long you may stand perfectly still before PID 1 kills you.
    pub watchdog_secs: f32,
    /// The warning comes this long before the kill.
    pub warn_secs: f32,
    /// Moving less than this (metres) or turning less than this (radians) isn't a ping.
    pub stir: f32,
    pub turn: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/afterlife/systemd.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// What systemd has to say.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bark {
    /// Your ping is overdue.
    Overdue,
    /// Watchdog timeout: killed.
    Killed,
}

/// Server -> everyone: PID 1 said `bark` about `player`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Journal {
    pub player: u64,
    pub bark: Bark,
}

/// Server only: one player's watchdog.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Watchdog {
    /// Where they were standing and looking at the last ping.
    pub pos: Vec3,
    pub yaw: f32,
    /// Seconds since the last ping.
    pub idle: f32,
    pub warned: bool,
}

impl Watchdog {
    pub fn new(pos: Vec3, yaw: f32) -> Self {
        Self {
            pos,
            yaw,
            idle: 0.0,
            warned: false,
        }
    }

    /// Advance by `dt` with the player now at `pos` facing `yaw`. Returns
    /// what systemd says, if anything. After a kill the dog starts over.
    pub fn tick(&mut self, pos: Vec3, yaw: f32, dt: f32) -> Option<Bark> {
        let turned = (yaw - self.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        if pos.distance(self.pos) > tuning().stir || turned.abs() > tuning().turn {
            *self = Self::new(pos, yaw);
            return None;
        }
        self.idle += dt;
        if self.idle >= tuning().watchdog_secs {
            *self = Self::new(pos, yaw);
            return Some(Bark::Killed);
        }
        if !self.warned && self.idle >= tuning().watchdog_secs - tuning().warn_secs {
            self.warned = true;
            return Some(Bark::Overdue);
        }
        None
    }
}

pub fn protocol(app: &mut App) {
    app.add_server_event::<Journal>(Channel::Ordered)
        .add_systems(Update, watch.run_if(in_state(ServerState::Running)));
}

pub fn plugin(app: &mut App) {
    app.add_observer(journal);
}

/// Server: feed every living player's watchdog; kill the hung ones.
fn watch(
    mut commands: Commands,
    time: Res<Time>,
    still: Query<&super::stillness::Stillness>,
    mut players: Query<(
        Entity,
        &Player,
        &Walker,
        Option<&mut Watchdog>,
        Has<super::afterlife::Dead>,
    )>,
) {
    let frozen = super::stillness::is_frozen(&still);
    let dt = time.delta_secs();
    for (e, player, walker, dog, dead) in &mut players {
        let Some(mut dog) = dog else {
            commands
                .entity(e)
                .insert(Watchdog::new(walker.pos, walker.yaw));
            continue;
        };
        if dead || frozen {
            // Nobody's hung while dead or while the world stands still.
            *dog = Watchdog::new(walker.pos, walker.yaw);
            continue;
        }
        let Some(bark) = dog.tick(walker.pos, walker.yaw, dt) else {
            continue;
        };
        if bark == Bark::Killed {
            commands.trigger(super::afterlife::Die { player: player.id });
        }
        commands.server_trigger(ToClients {
            targets: SendTargets::All,
            message: Journal {
                player: player.id,
                bark,
            },
        });
    }
}

/// The journal line for `player`, from `me`'s point of view.
pub fn journal_text(player: u64, bark: Bark, me: Option<u64>) -> String {
    let unit = format!("player@{player}.service");
    match (bark, me == Some(player)) {
        (Bark::Overdue, true) => format!(
            "systemd[1]: {unit}: watchdog ping overdue. MOVE, or PID 1 kills you in {} s",
            tuning().warn_secs as u32
        ),
        (Bark::Overdue, false) => format!("systemd[1]: {unit}: watchdog ping overdue"),
        (Bark::Killed, mine) => {
            let tail = if mine { " (that was you)" } else { "" };
            format!(
                "systemd[1]: {unit}: Watchdog timeout (limit {}min)! Killing process with signal SIGKILL{tail}",
                (tuning().watchdog_secs / 60.0) as u32
            )
        }
    }
}

/// Local: show what PID 1 said. Warnings only reach the one being warned.
fn journal(
    ev: On<Journal>,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<crate::hud::Toast>>,
    state: Option<Res<State<AppState>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    let me = me.map(|m| m.0);
    if ev.bark == Bark::Overdue && me != Some(ev.player) {
        return;
    }
    if let Some(mut toast) = toast {
        toast.show(journal_text(ev.player, ev.bark, me));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_still_warns_then_kills() {
        let mut dog = Watchdog::new(Vec3::ZERO, 0.0);
        let mut barks = Vec::new();
        for _ in 0..(tuning().watchdog_secs as usize + 1) {
            if let Some(b) = dog.tick(Vec3::ZERO, 0.0, 1.0) {
                barks.push(b);
            }
        }
        assert_eq!(barks, vec![Bark::Overdue, Bark::Killed]);
    }

    #[test]
    fn moving_or_looking_around_pings() {
        let mut dog = Watchdog::new(Vec3::ZERO, 0.0);
        assert_eq!(
            dog.tick(
                Vec3::ZERO,
                0.0,
                tuning().watchdog_secs - tuning().warn_secs - 1.0
            ),
            None
        );
        assert_eq!(dog.tick(Vec3::ZERO, 0.5, 0.5), None);
        assert_eq!(dog.idle, 0.0);
        assert_eq!(
            dog.tick(
                Vec3::ZERO,
                0.5,
                tuning().watchdog_secs - tuning().warn_secs - 1.0
            ),
            None
        );
        assert_eq!(dog.tick(Vec3::X, 0.5, 0.5), None);
        assert_eq!(dog.idle, 0.0);
        // A tiny wobble isn't a ping; wrapping yaw isn't a turn.
        let mut dog = Watchdog::new(Vec3::ZERO, std::f32::consts::PI - 0.001);
        dog.tick(Vec3::splat(0.01), -std::f32::consts::PI + 0.001, 1.0);
        assert_eq!(dog.idle, 1.0);
    }

    #[test]
    fn the_journal_reads_like_one() {
        assert!(journal_text(2, Bark::Killed, Some(1)).contains("player@2.service"));
        assert!(journal_text(2, Bark::Killed, Some(2)).contains("that was you"));
        assert!(journal_text(2, Bark::Overdue, Some(2)).contains("MOVE"));
    }
}
