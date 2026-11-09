//! Footsteps and landing thuds for your own body (polish, not a friend idea).
//!
//! Purely local: each peer watches its own replicated `Walker` position, so
//! nothing new goes over the network. The sounds are synthesized once at
//! startup (soft noise crunch over a low thump), a few variants so steps
//! don't sound like a machine gun. Dirt paths sound a little crisper.

use std::sync::Arc;

use bevy::audio::{AudioSource, Volume};
use bevy::prelude::*;

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::duel::Duelist;
use crate::features::fps::{Walker, my_walker};
use crate::features::synth_audio::RATE;
use crate::player::{LocalPlayer, Player};

/// Metres between footfalls while walking (running strides scale with speed).
const STRIDE: f32 = 1.7;
/// Vertical speed (m/s) above which the body counts as jumping or falling.
const AIRBORNE_VY: f32 = 3.5;
/// Falling faster than this, then stopping, is a landing.
const LANDING_VY: f32 = -4.0;

#[derive(Resource)]
struct StepSounds {
    grass: Vec<Handle<AudioSource>>,
    path: Vec<Handle<AudioSource>>,
    land: Handle<AudioSource>,
}

#[derive(Default)]
struct Gait {
    last: Option<Vec3>,
    travelled: f32,
    vy: f32,
    next: usize,
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, make_sounds)
        .add_systems(Update, step_sounds.run_if(in_state(AppState::InGame)));
}

fn make_sounds(mut commands: Commands, audio: Option<ResMut<Assets<AudioSource>>>) {
    let Some(mut audio) = audio else { return };
    let mut add = |samples: Vec<f32>| {
        audio.add(AudioSource {
            bytes: Arc::from(wav_bytes(&samples, RATE)),
        })
    };
    let grass = (0..4).map(|i| add(step(0x1234 + i, 900.0, 0.09))).collect();
    let path = (0..4)
        .map(|i| add(step(0x9876 + i, 2400.0, 0.07)))
        .collect();
    let land = add(step(0x4242, 700.0, 0.16));
    commands.insert_resource(StepSounds { grass, path, land });
}

/// One footfall: low-passed noise (the crunch) over a short sine thump.
pub fn step(seed: u32, cutoff: f32, secs: f32) -> Vec<f32> {
    let n = (RATE as f32 * secs) as usize;
    let mut rng = seed.max(1);
    let a = (-std::f32::consts::TAU * cutoff / RATE as f32).exp();
    let mut lp = 0.0;
    let thump_hz = 70.0 + (seed % 7) as f32 * 6.0;
    (0..n)
        .map(|i| {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let noise = (rng as f32 / u32::MAX as f32) * 2.0 - 1.0;
            lp = lp * a + noise * (1.0 - a);
            let t = i as f32 / RATE as f32;
            let attack = (t / 0.004).min(1.0);
            let env = attack * (-t / (secs * 0.3)).exp() * (1.0 - t / secs);
            let thump = (std::f32::consts::TAU * thump_hz * t).sin() * (-t / 0.025).exp();
            ((lp * 2.5 + thump * 0.5) * env).clamp(-1.0, 1.0)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn step_sounds(
    mut commands: Commands,
    time: Res<Time>,
    sounds: Option<Res<StepSounds>>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    duelists: Query<&Duelist>,
    mut gait: Local<Gait>,
) {
    let Some(sounds) = sounds else { return };
    let Some(w) = my_walker(me.as_deref(), &walkers, &duelists) else {
        gait.last = None;
        return;
    };
    let dt = time.delta_secs();
    let Some(last) = gait.last.replace(w.pos) else {
        return;
    };
    if dt <= 0.0 {
        return;
    }
    let moved = w.pos - last;
    // Teleports (respawn, fate) aren't steps.
    if moved.length() > 3.0 {
        gait.travelled = 0.0;
        return;
    }
    // Replicated positions come in steps, so smooth the vertical speed.
    let vy = moved.y / dt;
    let prev_vy = gait.vy;
    gait.vy = gait.vy + (vy - gait.vy) * (dt * 20.0).min(1.0);
    let mut play = |handle: &Handle<AudioSource>, volume: f32| {
        commands.spawn((
            AudioPlayer(handle.clone()),
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
            DespawnOnExit(AppState::InGame),
        ));
    };
    if prev_vy < LANDING_VY && gait.vy.abs() < 1.0 {
        play(&sounds.land, 0.5);
        gait.travelled = 0.0;
        return;
    }
    if gait.vy.abs() > AIRBORNE_VY {
        return;
    }
    gait.travelled += moved.xz().length();
    if gait.travelled < STRIDE {
        return;
    }
    gait.travelled = 0.0;
    let on_path = crate::terrain::is_path(w.pos.x, w.pos.z);
    let set = if on_path { &sounds.path } else { &sounds.grass };
    gait.next = (gait.next + 1) % set.len().max(1);
    if let Some(handle) = set.get(gait.next) {
        play(handle, 0.22);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_footstep_is_short_quiet_and_fades_out() {
        let s = super::step(7, 900.0, 0.09);
        assert!(!s.is_empty());
        assert!(s.iter().all(|v| v.abs() <= 1.0));
        let tail = s[s.len() - 50..]
            .iter()
            .map(|v| v.abs())
            .fold(0.0, f32::max);
        assert!(tail < 0.05, "tail {tail}");
    }
}
