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
