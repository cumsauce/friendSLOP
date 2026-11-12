//! One cute, friend-shaped bear that does scawy (cute) growls. From issue #4 by arseeN.
//!
//! The server spawns exactly one replicated `Bear`. Like the crows, its
//! wandering and growling are decoration, so every peer animates it locally
//! from the clock (no per-frame network traffic). A growl is a little rear-up,
//! a "grrr~" line of text and a tiny procedurally generated growl sound.

use std::sync::Arc;

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;

/// The bear. Just one. `home` is where it ambles about (x, z on the floor).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Bear {
    pub home: Vec2,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Bear>()
        .add_systems(OnEnter(ServerState::Running), spawn_bear);
}

/// Server only: one bear, a little off-centre so it doesn't sit on the crows.
fn spawn_bear(mut commands: Commands) {
    commands.spawn((
        Bear {
            home: Vec2::new(-6.0, -3.0),
        },
        Replicated,
    ));
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        OnEnter(AppState::InGame),
        (spawn_growl_text, load_growl_sound),
    )
    .add_systems(
        Update,
        (dress_bear, animate_bear, growl).run_if(in_state(AppState::InGame)),
    );
}

/// Seconds between growls.
const GROWL_PERIOD: f32 = 13.0;
/// How long a growl lasts (the sound is the same length).
pub const GROWL_LENGTH: f32 = 1.2;

/// Is the bear mid-growl at time `t`?
pub fn is_growling(t: f32) -> bool {
    (t + 4.0) % GROWL_PERIOD < GROWL_LENGTH
}

/// Where the bear is at time `t`: a slow, lazy figure-eight around home.
pub fn bear_position(bear: &Bear, t: f32) -> Vec3 {
    let s = t * 0.12;
    let wander = Vec2::new(s.sin() * 2.5, (2.0 * s).sin() * 1.2);
    Vec3::new(bear.home.x + wander.x, 0.0, bear.home.y + wander.y)
}

/// The rigged plush bear model (clips `idle`, `walk`, `growl`).
pub const BEAR_MODEL: &str = "models/creatures/bear.gltf";

/// Give the bear (on every peer) its rigged plush model. A missing file only
/// makes it invisible: its wandering and growls don't depend on it.
fn dress_bear(mut commands: Commands, new: Query<(Entity, &Bear), Added<Bear>>) {
    for (entity, bear) in &new {
        commands.entity(entity).insert((
            Transform::from_translation(bear_position(bear, 0.0)),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
            crate::rigged::Rigged { path: BEAR_MODEL },
            bear_clip(false),
        ));
    }
}

/// The clip the bear plays: a growl (rear up, paws up, puff) while growling,
/// else its ambling waddle.
pub fn bear_clip(growling: bool) -> crate::rigged::RigClip {
    if growling {
        crate::rigged::RigClip::once("growl").fade(0.15)
    } else {
        crate::rigged::RigClip::looping("walk").fade(0.3)
    }
}

/// Amble along and face where we're going; the clips do the waddle and the
/// growl's rear-up.
fn animate_bear(
    time: Res<Time>,
    offset: Res<super::stillness::SessionOffset>,
    mut bears: Query<(&Bear, &mut Transform, &mut crate::rigged::RigClip)>,
) {
    let t = super::stillness::session_secs(&time, &offset);
    for (bear, mut transform, mut clip) in &mut bears {
        let here = bear_position(bear, t);
        let ahead = bear_position(bear, t + 0.1);
        let look = Vec3::new(ahead.x - here.x, 0.0, ahead.z - here.z);
        if let Ok(dir) = Dir3::new(look) {
            transform.rotation = Transform::default().looking_to(dir, Vec3::Y).rotation;
        }
        transform.translation = here;
        let want = bear_clip(is_growling(t));
        if want.name != clip.name {
            *clip = want;
        }
    }
}

#[derive(Component)]
struct GrowlText;

#[derive(Resource)]
struct GrowlSound(Handle<AudioSource>);

fn spawn_growl_text(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        GrowlText,
        crate::hud::slot(crate::hud::HudSlot::Subtitle),
        DespawnOnExit(AppState::InGame),
    ));
}

fn load_growl_sound(mut commands: Commands, audio: Option<ResMut<Assets<AudioSource>>>) {
    let Some(mut audio) = audio else { return };
    let wav = wav_bytes(&growl_samples(GROWL_SAMPLE_RATE), GROWL_SAMPLE_RATE);
    let handle = audio.add(AudioSource {
        bytes: Arc::from(wav),
    });
    commands.insert_resource(GrowlSound(handle));
}

/// Show the growl text and play the sound once at the start of every growl.
fn growl(
    time: Res<Time>,
    offset: Res<super::stillness::SessionOffset>,
    mut commands: Commands,
    bears: Query<Entity, With<Bear>>,
    sound: Option<Res<GrowlSound>>,
    mut played: Local<u64>,
    mut text: Query<&mut Text, With<GrowlText>>,
) {
    let t = super::stillness::session_secs(&time, &offset);
    let growling = !bears.is_empty() && is_growling(t);
    // Which growl window this is: robust to leaving and re-entering mid-growl.
    let window = ((t + 4.0) / GROWL_PERIOD) as u64 + 1;
    if growling
        && *played != window
        && let Some(sound) = &sound
        && let Some(bear) = bears.iter().next()
    {
        *played = window;
        // From the bear's mouth, following it as it ambles.
        let mouth = Vec3::new(0.0, 1.8, -0.5);
        let carry = crate::spatial::Carry::CALL;
        crate::spatial::play_on(&mut commands, &sound.0, bear, mouth, 1.0, carry);
    }
    let want = if growling {
        "grrr~ (the bear, cutely)"
    } else {
        ""
    };
    for mut text in &mut text {
        if text.0 != want {
            text.0 = want.to_string();
        }
    }
}

const GROWL_SAMPLE_RATE: u32 = crate::features::synth_audio::RATE;

/// A small, rumbly, slightly silly growl: a low buzzy tone that wobbles in
/// pitch, rumbles in volume and swells in and out, plus a little breath noise.
pub fn growl_samples(rate: u32) -> Vec<f32> {
    let n = (GROWL_LENGTH * rate as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f32;
    let mut noise = 0x1234_5678u32;
    let mut brown = 0.0f32;
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let p = t / GROWL_LENGTH;
        // Rises a bit then drops: "grrRRrr".
        let pitch = 85.0 + 35.0 * (p * std::f32::consts::PI).sin() + 6.0 * (t * 9.0).sin();
        let inc = pitch / rate as f32;
        phase = (phase + inc).fract();
        // Band-limited sawtooth through a soft shaper: buzzy but not harsh.
        let saw = crate::features::synth_audio::saw(phase, inc);
        let tone = (saw * 1.8).tanh();
        // Brown-ish noise for breathiness.
        noise ^= noise << 13;
        noise ^= noise >> 17;
        noise ^= noise << 5;
        let white = (noise as f32 / u32::MAX as f32) * 2.0 - 1.0;
        brown = (brown + white * 0.08) * 0.97;
        // The "rrr": fast amplitude flutter.
        let flutter = 0.65 + 0.35 * (t * 2.0 * std::f32::consts::PI * 28.0).sin();
        // Fade in and out.
        let env = (p * 8.0).min(1.0) * ((1.0 - p) * 4.0).min(1.0);
        out.push((tone * 0.7 + brown) * flutter * env);
    }
    crate::features::synth_audio::master(&mut out, rate, 2800.0, 0.6);
    out
}

/// Encode mono `f32` samples as a 16-bit PCM WAV file.
pub fn wav_bytes(samples: &[f32], rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    // TPDF dither: turns 16-bit quantisation grit into a faint, smooth hiss.
    let mut rng = 0x2545_F491u32;
    let mut rand = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng as f32 / u32::MAX as f32
    };
    for s in samples {
        let d = rand() - rand();
        let v = (s.clamp(-1.0, 1.0) * (i16::MAX as f32 - 1.0) + d).round() as i16;
        b.extend_from_slice(&v.to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bear_growls_only_sometimes() {
        let samples = 10_000;
        let growling = (0..samples)
            .filter(|i| is_growling(*i as f32 * 0.01))
            .count();
        assert!(growling > 0, "the bear never growls");
        assert!(growling < samples / 5, "the bear growls too much");
    }

    #[test]
    fn bear_stays_near_home_on_the_ground() {
        let bear = Bear {
            home: Vec2::new(-6.0, -3.0),
        };
        for i in 0..2000 {
            let p = bear_position(&bear, i as f32 * 0.1);
            assert_eq!(p.y, 0.0);
            assert!(Vec2::new(p.x, p.z).distance(bear.home) < 3.0);
        }
    }

    #[test]
    fn bear_growls_once_then_walks_again() {
        assert_eq!(bear_clip(true).name, "growl");
        assert!(bear_clip(true).once);
        assert_eq!(bear_clip(false).name, "walk");
        assert!(!bear_clip(false).once);
    }

    #[test]
    fn growl_is_audible_but_not_clipping() {
        let s = growl_samples(GROWL_SAMPLE_RATE);
        assert_eq!(s.len(), (GROWL_LENGTH * GROWL_SAMPLE_RATE as f32) as usize);
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.1 && peak <= 1.0, "peak {peak}");
        assert!(s.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn wav_header_is_sane() {
        let wav = wav_bytes(&[0.0, 0.5, -0.5], 22_050);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + 6);
    }
}
