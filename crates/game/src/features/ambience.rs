//! Ambient soundscape (issue #62 by doomy: "the sound rn is crunchy ... make
//! it a bit better"). Local decoration only, nothing replicated.
//!
//! - A quiet birdsong loop (CC0 sample) under everything.
//! - A soft wind bed, synthesized: low-passed noise with slow gusts, built so
//!   it loops seamlessly (no file needed).
//! - A crackling fire loop (CC0 sample) placed at the commune bonfire as a
//!   spatial sound, so it gets louder and pans as you walk up to it.
//!
//! Missing sample files (no LFS data) just stay silent.

use std::sync::Arc;

use bevy::audio::{AudioSource, Volume};
use bevy::prelude::*;

use crate::AppState;
use crate::features::bear::wav_bytes;

#[derive(Resource)]
struct AmbientSounds {
    birds: Handle<AudioSource>,
    fire: Handle<AudioSource>,
    wind: Handle<AudioSource>,
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, load_ambience)
        .add_systems(OnEnter(AppState::InGame), start_ambience)
        .add_systems(Update, district_mix.run_if(in_state(AppState::InGame)));
}

fn load_ambience(
    mut commands: Commands,
    assets: Option<Res<AssetServer>>,
    audio: Option<ResMut<Assets<AudioSource>>>,
) {
    let (Some(assets), Some(mut audio)) = (assets, audio) else {
        return;
    };
    let wind = audio.add(AudioSource {
        bytes: Arc::from(wav_bytes(&wind_loop(WIND_RATE), WIND_RATE)),
    });
    commands.insert_resource(AmbientSounds {
        birds: assets.load("audio/ambience/birds.ogg"),
        fire: assets.load("audio/ambience/fire_crackle.ogg"),
        wind,
    });
}

fn start_ambience(mut commands: Commands, sounds: Option<Res<AmbientSounds>>) {
    let Some(sounds) = sounds else { return };
    let bed = |handle: &Handle<AudioSource>, volume: f32| {
        (
            AudioPlayer(handle.clone()),
            PlaybackSettings::LOOP.with_volume(Volume::Linear(volume)),
            DespawnOnExit(AppState::InGame),
        )
    };
    commands.spawn((bed(&sounds.birds, 0.18), Bed::Birds));
    commands.spawn((bed(&sounds.wind, 0.22), Bed::Wind));
    // Heard where it burns (see `crate::spatial`).
    crate::spatial::loop_at(
        &mut commands,
        &sounds.fire,
        crate::scenery::BONFIRE_AT + Vec3::Y * 0.5,
        0.9,
        crate::spatial::Carry::NEARBY,
    );
}

/// Which looping bed an ambient player is, so the district mix can steer it.
#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Bed {
    Birds,
    Wind,
}

/// Target (birds, wind) volume where the camera is: birdsong thickens in the
/// forest and thins out in the village, wind howls through the ruins and maze,
/// the valley, and higher up (the sky islands).
pub fn district_levels(at: Vec3) -> (f32, f32) {
    use crate::zones::Zone;
    let (birds, wind) = match crate::zones::zone_of(at.xz()) {
        Zone::Forest => (0.38, 0.16),
        Zone::Village => (0.09, 0.18),
        Zone::Ruins => (0.06, 0.4),
        Zone::Valley => (0.16, 0.32),
        Zone::Outside => (0.04, 0.42),
        Zone::Commune | Zone::Meadow => (0.18, 0.22),
    };
    let height = ((at.y - crate::terrain::height_at(at.x, at.z) - 4.0) / 30.0).clamp(0.0, 1.0);
    (birds * (1.0 - 0.7 * height), wind + 0.25 * height)
}

/// Fade the beds towards the district's mix (a few seconds per crossfade).
fn district_mix(
    time: Res<Time>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut beds: Query<(&Bed, &mut AudioSink)>,
) {
    let Some(cam) = camera.iter().next() else {
        return;
    };
    let (birds, wind) = district_levels(cam.translation());
    let t = (time.delta_secs() * 0.6).min(1.0);
    for (bed, mut sink) in &mut beds {
        let want = if *bed == Bed::Birds { birds } else { wind };
        let now = sink.volume().to_linear();
        sink.set_volume(Volume::Linear(now + (want - now) * t));
    }
}

/// Wind is all low frequencies, so a lower rate is plenty (and cheaper).
const WIND_RATE: u32 = 22_050;
/// Seconds per wind loop.
pub const WIND_SECS: f32 = 12.0;

/// A seamlessly looping wind bed: brown noise, gently low-passed, swelling in
/// gusts. Gust rates are whole cycles per loop and the noise tail is
/// cross-faded into its head, so the loop point is inaudible.
pub fn wind_loop(rate: u32) -> Vec<f32> {
    let n = (WIND_SECS * rate as f32) as usize;
    let xfade = rate as usize; // one second
    let mut rng = 0x5EED_1234u32;
    let mut brown = 0.0f32;
    let mut noise: Vec<f32> = (0..n + xfade)
        .map(|_| {
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let white = rng as f32 / u32::MAX as f32 * 2.0 - 1.0;
            brown = (brown + white * 0.02) * 0.998;
            brown
        })
        .collect();
    for i in 0..xfade {
        let g = i as f32 / xfade as f32;
        noise[i] = noise[i] * g + noise[n + i] * (1.0 - g);
    }
    noise.truncate(n);
    let tau = std::f32::consts::TAU;
    for (i, s) in noise.iter_mut().enumerate() {
        let p = i as f32 / n as f32;
        let gust = 0.55
            + 0.25 * (tau * 2.0 * p).sin()
            + 0.15 * (tau * 5.0 * p + 1.3).sin()
            + 0.05 * (tau * 11.0 * p + 0.4).sin();
        *s *= gust;
    }
    // Low-pass (no click-fades here: this must loop), then normalise.
    let mut out = noise;
    let a = 1.0 - (-tau * 900.0 / rate as f32).exp();
    // Run the filter twice around the loop so its state wraps seamlessly.
    let mut lp = 0.0f32;
    for _ in 0..2 {
        for s in out.iter_mut() {
            lp += a * (*s - lp);
        }
    }
    for s in out.iter_mut() {
        lp += a * (*s - lp);
        *s = lp;
    }
    let mean = out.iter().sum::<f32>() / out.len().max(1) as f32;
    out.iter_mut().for_each(|s| *s -= mean);
    let peak = out.iter().fold(0.0f32, |m, s| m.max(s.abs()));
    if peak > 1e-5 {
        out.iter_mut().for_each(|s| *s *= 0.7 / peak);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wind_is_quiet_safe_and_loops() {
        let s = wind_loop(WIND_RATE);
        assert_eq!(s.len(), (WIND_SECS * WIND_RATE as f32) as usize);
        assert!(s.iter().all(|x| x.is_finite()));
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.3 && peak <= 0.71, "peak {peak}");
        // No jump at the loop seam bigger than a normal sample step.
        let seam = (s[0] - s[s.len() - 1]).abs();
        let typical = s
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0, f32::max);
        assert!(seam <= typical * 1.5, "seam {seam} vs {typical}");
    }
}
