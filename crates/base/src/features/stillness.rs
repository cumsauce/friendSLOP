//! Every 4 minutes (was 60 s) the world freezes for 20 so everyone can socialize and
//! stare with passion. From issue #17 by just meat and cheese.
//!
//! The server owns the clock and keeps one replicated `Stillness` entity that
//! says whether the world is frozen and how many seconds are left. Every peer
//! pauses its virtual time while frozen, so everything driven by game time
//! (crows, the bear, mushroom timers, duel movement, physics) stops in place,
//! and the server ignores actions (eating the mushroom, shooting) until it's
//! over. Voice chat runs on real time, so talking keeps working.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;

/// Tuning numbers: `assets/data/base/stillness.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Seconds of normal play between freezes (was 60; less often, asked by the owner).
    pub active_secs: f32,
    /// How long a freeze lasts.
    pub frozen_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/base/stillness.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Server-authoritative freeze state, replicated to everyone.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Stillness {
    pub frozen: bool,
    /// Whole seconds until the current phase ends (rounded up).
    pub secs_left: u32,
}

/// The server's game-time clock, replicated on the `Stillness` entity (and
/// refreshed whenever the phase flips) so every peer can line its local NPC
/// animations up with what the server judges (fart shame). Local `Time`
/// starts at process start, so raw elapsed time differs per peer.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct SessionClock {
    pub elapsed: f32,
}

/// Client only: server game time minus our own, from the last `SessionClock`.
#[derive(Resource, Default, Debug)]
pub struct SessionOffset(pub f32);

/// Game time shared by every peer, for animations the server must agree with.
pub fn session_secs(time: &Time, offset: &SessionOffset) -> f32 {
    time.elapsed_secs() + offset.0
}

/// Server only: the precise clock behind `Stillness`.
#[derive(Resource, Default)]
pub struct StillClock {
    /// Real seconds since the server started.
    pub elapsed: f32,
}

/// Where in the 80 s cycle we are, `t` seconds after the server started.
pub fn phase_at(t: f32) -> Stillness {
    let cycle = tuning().active_secs + tuning().frozen_secs;
    let p = t.rem_euclid(cycle);
    if p < tuning().active_secs {
        Stillness {
            frozen: false,
            secs_left: (tuning().active_secs - p).ceil() as u32,
        }
    } else {
        Stillness {
            frozen: true,
            secs_left: (cycle - p).ceil() as u32,
        }
    }
}

/// True while the world is frozen. Use to ignore actions.
pub fn is_frozen(still: &Query<&Stillness>) -> bool {
    still.iter().any(|s| s.frozen)
}

pub fn protocol(app: &mut App) {
    app.replicate::<Stillness>()
        .replicate::<SessionClock>()
        .init_resource::<StillClock>()
        .init_resource::<SessionOffset>()
        .add_systems(OnEnter(ServerState::Running), reset_offset)
        .add_systems(
            Update,
            sync_offset.run_if(not(in_state(ServerState::Running))),
        )
        .add_systems(OnEnter(ServerState::Running), start_clock)
        .add_systems(Update, tick_clock.run_if(in_state(ServerState::Running)))
        .add_systems(Update, apply_freeze.run_if(any_with_component::<Stillness>))
        .add_systems(OnExit(ServerState::Running), stop_clock);
}

fn start_clock(mut commands: Commands, mut clock: ResMut<StillClock>) {
    clock.elapsed = 0.0;
    commands.spawn((phase_at(0.0), SessionClock::default(), Replicated));
}

fn stop_clock(
    mut commands: Commands,
    still: Query<Entity, With<Stillness>>,
    mut time: ResMut<Time<Virtual>>,
) {
    for e in &still {
        commands.entity(e).despawn();
    }
    time.unpause();
}

/// Server: advance on real time (virtual time is paused while frozen).
fn tick_clock(
    real: Res<Time<Real>>,
    time: Res<Time>,
    mut clock: ResMut<StillClock>,
    mut still: Query<(&mut Stillness, &mut SessionClock)>,
) {
    clock.elapsed += real.delta_secs();
    let now = phase_at(clock.elapsed);
    for (mut s, mut session) in &mut still {
        // Only touch it when it changes, to avoid replicating every frame.
        if s.set_if_neq(now) {
            session.elapsed = time.elapsed_secs();
        }
    }
}

fn reset_offset(mut offset: ResMut<SessionOffset>) {
    offset.0 = 0.0;
}

/// Client: line our game time up with the server's whenever it sends its clock.
fn sync_offset(
    time: Res<Time>,
    clocks: Query<&SessionClock, Changed<SessionClock>>,
    mut offset: ResMut<SessionOffset>,
) {
    if let Some(clock) = clocks.iter().next() {
        offset.0 = clock.elapsed - time.elapsed_secs();
    }
}

/// Every peer: pause game time while frozen.
fn apply_freeze(still: Query<&Stillness>, mut time: ResMut<Time<Virtual>>) {
    let frozen = is_frozen(&still);
    if frozen && !time.is_paused() {
        time.pause();
    } else if !frozen && time.is_paused() {
        time.unpause();
    }
}

pub fn plugin(app: &mut App) {
    // The frozen countdown shows in the HUD's danger lane (`ui::danger`).
    app.add_systems(OnExit(AppState::InGame), unfreeze);
}

fn unfreeze(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_minutes_on_twenty_seconds_off() {
        assert!(!phase_at(0.0).frozen);
        assert_eq!(phase_at(0.0).secs_left, 240);
        assert!(!phase_at(239.9).frozen);
        assert!(phase_at(240.0).frozen);
        assert_eq!(phase_at(240.0).secs_left, 20);
        assert!(phase_at(259.9).frozen);
        assert!(!phase_at(260.0).frozen);
        assert!(phase_at(505.0).frozen);
    }
}
