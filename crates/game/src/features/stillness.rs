//! Every 60 seconds the world freezes for 20 so everyone can socialize and
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

/// Seconds of normal play between freezes.
pub const ACTIVE_SECS: f32 = 60.0;
/// How long a freeze lasts.
pub const FROZEN_SECS: f32 = 20.0;

/// Server-authoritative freeze state, replicated to everyone.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Stillness {
    pub frozen: bool,
    /// Whole seconds until the current phase ends (rounded up).
    pub secs_left: u32,
}

/// Server only: the precise clock behind `Stillness`.
#[derive(Resource, Default)]
pub struct StillClock {
    /// Real seconds since the server started.
    pub elapsed: f32,
}

/// Where in the 80 s cycle we are, `t` seconds after the server started.
pub fn phase_at(t: f32) -> Stillness {
    let cycle = ACTIVE_SECS + FROZEN_SECS;
    let p = t.rem_euclid(cycle);
    if p < ACTIVE_SECS {
        Stillness {
            frozen: false,
            secs_left: (ACTIVE_SECS - p).ceil() as u32,
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
        .init_resource::<StillClock>()
        .add_systems(OnEnter(ServerState::Running), start_clock)
        .add_systems(Update, tick_clock.run_if(in_state(ServerState::Running)))
        .add_systems(Update, apply_freeze.run_if(any_with_component::<Stillness>))
        .add_systems(OnExit(ServerState::Running), stop_clock);
}

fn start_clock(mut commands: Commands, mut clock: ResMut<StillClock>) {
    clock.elapsed = 0.0;
    commands.spawn((phase_at(0.0), Replicated));
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
    mut clock: ResMut<StillClock>,
    mut still: Query<&mut Stillness>,
) {
    clock.elapsed += real.delta_secs();
    let now = phase_at(clock.elapsed);
    for mut s in &mut still {
        // Only touch it when it changes, to avoid replicating every frame.
        s.set_if_neq(now);
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
    app.add_systems(OnEnter(AppState::InGame), spawn_still_text)
        .add_systems(OnExit(AppState::InGame), unfreeze)
        .add_systems(
            Update,
            update_still_text
                .run_if(crate::world::hud_tick())
                .run_if(in_state(AppState::InGame)),
        );
}

fn unfreeze(mut time: ResMut<Time<Virtual>>) {
    time.unpause();
}

#[derive(Component)]
struct StillText;

fn spawn_still_text(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(24.0),
        StillText,
        crate::hud::slot(crate::hud::HudSlot::CenterBanner),
        TextLayout::justify(Justify::Center),
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_still_text(still: Query<&Stillness>, mut text: Query<&mut Text, With<StillText>>) {
    let want = match still.iter().next() {
        Some(s) if s.frozen => format!(
            "The world stands still.\nSocialize, and stare with passion. ({}s)",
            s.secs_left
        ),
        _ => String::new(),
    };
    for mut text in &mut text {
        if text.0 != want {
            text.0 = want.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_on_twenty_off() {
        assert!(!phase_at(0.0).frozen);
        assert_eq!(phase_at(0.0).secs_left, 60);
        assert!(!phase_at(59.9).frozen);
        assert!(phase_at(60.0).frozen);
        assert_eq!(phase_at(60.0).secs_left, 20);
        assert!(phase_at(79.9).frozen);
        assert!(!phase_at(80.0).frozen);
        assert!(phase_at(145.0).frozen);
    }
}
