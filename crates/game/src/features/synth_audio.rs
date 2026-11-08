//! Shared plumbing for procedurally generated sounds (not a friend idea).
//!
//! Synthesizing samples and encoding a WAV can take several milliseconds, so
//! it runs on the `AsyncComputeTaskPool` instead of stalling a frame. The
//! entity spawned by [`play_synth`] holds the task; once it's done the sound
//! is added to `Assets<AudioSource>` and the same entity plays it with
//! `PlaybackSettings::DESPAWN`. That entity is the only owner of the strong
//! handle, so the asset is freed as soon as playback ends.

use std::sync::Arc;

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{AsyncComputeTaskPool, Task, TaskPool};

use crate::AppState;

/// A sound still being synthesized: finished WAV bytes.
#[derive(Component)]
pub struct SynthTask(Task<Vec<u8>>);

/// Synthesize `make_wav` off the main thread, then play it once. Returns the
/// entity that will play it (despawn it to cancel or stop the sound).
pub fn play_synth(
    commands: &mut Commands,
    make_wav: impl FnOnce() -> Vec<u8> + Send + 'static,
) -> Entity {
    let pool = AsyncComputeTaskPool::get_or_init(TaskPool::default);
    let task = pool.spawn(async move { make_wav() });
    commands
        .spawn((SynthTask(task), DespawnOnExit(AppState::InGame)))
        .id()
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        start_finished_sounds.run_if(any_with_component::<SynthTask>),
    );
}

fn start_finished_sounds(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut SynthTask)>,
    audio: Option<ResMut<Assets<AudioSource>>>,
) {
    let Some(mut audio) = audio else { return };
    for (entity, mut task) in &mut tasks {
        let Some(wav) = check_ready(&mut task.0) else {
            continue;
        };
        let handle = audio.add(AudioSource {
            bytes: Arc::from(wav),
        });
        commands
            .entity(entity)
            .remove::<SynthTask>()
            .insert((AudioPlayer(handle), PlaybackSettings::DESPAWN));
    }
}

/// Sample rate for every synthesized sound. 44.1 kHz so high partials don't
/// fold back down as aliasing (the old 22.05 kHz sounds were "crunchy", #62).
pub const RATE: u32 = 44_100;

/// PolyBLEP correction: smooths the jump of a naive saw/square so it is
/// band-limited enough not to alias. `phase` in 0..1, `inc` = freq / rate.
fn poly_blep(phase: f32, inc: f32) -> f32 {
    if phase < inc {
        let t = phase / inc;
        2.0 * t - t * t - 1.0
    } else if phase > 1.0 - inc {
        let t = (phase - 1.0) / inc;
        t * t + 2.0 * t + 1.0
    } else {
        0.0
    }
}

/// Band-limited sawtooth (-1..1).
pub fn saw(phase: f32, inc: f32) -> f32 {
    2.0 * phase - 1.0 - poly_blep(phase, inc)
}

/// Band-limited square (-1..1).
pub fn square(phase: f32, inc: f32) -> f32 {
    let naive = if phase < 0.5 { 1.0 } else { -1.0 };
    naive + poly_blep(phase, inc) - poly_blep((phase + 0.5).fract(), inc)
}

/// Final polish shared by every synth: remove DC, a gentle two-pole low-pass
/// at `cutoff` Hz to take the fizz off, 4 ms fades so starts/ends don't click,
/// and normalise to `peak` so nothing clips and sounds sit at similar levels.
pub fn master(samples: &mut [f32], rate: u32, cutoff: f32, peak: f32) {
    let a = 1.0 - (-std::f32::consts::TAU * cutoff / rate as f32).exp();
    let (mut lp1, mut lp2, mut dc_x, mut dc_y) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for s in samples.iter_mut() {
        dc_y = *s - dc_x + 0.995 * dc_y;
        dc_x = *s;
        lp1 += a * (dc_y - lp1);
        lp2 += a * (lp1 - lp2);
        *s = lp2;
    }
    let len = samples.len();
    let fade = ((0.004 * rate as f32) as usize).min(len / 2);
    for i in 0..fade {
        let g = i as f32 / fade as f32;
        samples[i] *= g;
        samples[len - 1 - i] *= g;
    }
    let top = samples.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if top > 1e-5 {
        let gain = peak / top;
        samples.iter_mut().for_each(|s| *s *= gain);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mastered_sound_is_safe() {
        let inc = 110.0 / RATE as f32;
        let mut s: Vec<f32> = (0..RATE)
            .map(|i| {
                square((i as f32 * inc).fract(), inc) * 3.0 + saw((i as f32 * inc).fract(), inc)
            })
            .collect();
        master(&mut s, RATE, 4000.0, 0.6);
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((peak - 0.6).abs() < 1e-3 && s.iter().all(|x| x.is_finite()));
        assert!(s[0].abs() < 1e-3 && s[s.len() - 1].abs() < 1e-3);
    }
}
