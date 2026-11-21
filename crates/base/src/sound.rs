//! The audio graph (not a friend idea): every sound in the game goes through
//! one firewheel graph (`bevy_seedling`, vendored for Bevy 0.20), one output
//! stream, with a few buses so a setting can turn a whole kind of sound up or
//! down:
//!
//! ```text
//!  sfx pool ─┐ world sounds (spatial) ─┐
//!            SoundEffectsBus ──────────┤
//!  music (fusion) ── MusicBus ─────────┤
//!  voice chat ────── VoiceBus ─────────┤
//!                                  MainBus (master volume) ── limiter ── out
//! ```
//!
//! Features use this module instead of seedling directly: [`play_sfx`] for a
//! flat (2D) sound, [`play_loop`] for a flat loop whose level a
//! [`SoundVolume`] steers, [`from_wav`] / [`from_samples`] to turn
//! synthesized samples into a playable [`AudioSample`], and [`audible`] to
//! skip sound work in headless apps (tests), which have no audio graph.

use std::num::{NonZeroU32, NonZeroUsize};

use bevy::prelude::*;
use bevy_seedling::prelude::*;
pub use bevy_seedling::prelude::{
    AudioSample, DefaultPool, MainBus, NodeLabel, SamplePlayer, SoundEffectsBus, Volume, VolumeNode,
};
use firewheel::sample_resource::InterleavedResourceF32;

/// A loaded or synthesized sound.
pub type Sound = Handle<AudioSample>;

/// Where the soundtrack plays (its own level under master).
#[derive(NodeLabel, PartialEq, Eq, Debug, Hash, Clone)]
pub struct MusicBus;

/// Where voice chat plays.
#[derive(NodeLabel, PartialEq, Eq, Debug, Hash, Clone)]
pub struct VoiceBus;

/// Present when this app has an audio graph (not in headless tests).
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct Audible;

/// Run condition: is there an audio graph to play into?
pub fn audible(a: Option<Res<Audible>>) -> bool {
    a.is_some()
}

/// The graph's sample rate (the device's), or 48 kHz before it's known.
pub fn stream_rate(rate: Option<&bevy_seedling::context::SampleRate>) -> u32 {
    rate.map(|r| r.get().get()).unwrap_or(48_000)
}

/// The graph's rate as last seen (any thread, e.g. a synth task).
static RATE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(48_000);

/// The graph's sample rate, readable anywhere (48 kHz until it's known).
pub fn rate() -> u32 {
    RATE.load(std::sync::atomic::Ordering::Relaxed)
}

fn track_rate(rate: Option<Res<bevy_seedling::context::SampleRate>>) {
    RATE.store(
        stream_rate(rate.as_deref()),
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// A playable sound from a synth's WAV bytes, at the graph's rate.
pub fn wav(bytes: &[u8]) -> Option<AudioSample> {
    from_wav(bytes, rate())
}

/// [`wav`], or a moment of silence if the bytes aren't a WAV we can read.
pub fn wav_or_silence(bytes: &[u8]) -> AudioSample {
    wav(bytes).unwrap_or_else(|| samples(&[0.0; 64], rate()))
}

/// A playable sound from mono samples at `rate`, at the graph's rate.
pub fn samples(samples: &[f32], sample_rate: u32) -> AudioSample {
    from_samples(samples, sample_rate, rate())
}

/// A flat sound's level, linear (1 = as recorded). Change it any time; the
/// sound glides there.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct SoundVolume(pub f32);

/// The audio graph's plugins, for the game's `main` (desktop, Android and
/// the browser). Headless apps leave them out.
pub fn plugins() -> SeedlingPlugins {
    SeedlingPlugins
}

pub fn plugin(app: &mut App) {
    if !app.is_plugin_added::<bevy_seedling::SeedlingCorePlugin>() {
        return;
    }
    crate::live_synth::install(app);
    app.insert_resource(Audible)
        .add_systems(Startup, spawn_buses)
        .add_systems(First, track_rate)
        .add_systems(PostUpdate, steer_volumes)
        .add_systems(Update, trim_effects);
}

/// Headroom for every sound effect (flat and world): many are mixed at 1.0
/// and clipped the limiter once they overlap. Music and voice skip this bus.
const EFFECTS_GAIN: f32 = 0.5;

fn trim_effects(mut bus: Query<&mut VolumeNode, (With<SoundEffectsBus>, Added<VolumeNode>)>) {
    for mut node in &mut bus {
        node.volume = Volume::Linear(EFFECTS_GAIN);
    }
}

fn spawn_buses(mut commands: Commands) {
    commands
        .spawn((MusicBus, VolumeNode::default()))
        .connect(MainBus);
    commands
        .spawn((VoiceBus, VolumeNode::default()))
        .connect(MainBus);
}

/// Play `sound` once, flat (not placed in the world). Despawn the entity to
/// stop it early.
pub fn play_sfx(commands: &mut Commands, sound: &Sound, volume: f32) -> Entity {
    commands
        .spawn((
            SamplePlayer::new(sound.clone()).with_volume(Volume::Linear(volume)),
            DefaultPool,
        ))
        .id()
}

/// Loop `sound` flat until the entity is despawned. Its [`SoundVolume`]
/// sets the level from then on.
pub fn play_loop(commands: &mut Commands, sound: &Sound, volume: f32) -> Entity {
    commands
        .spawn((
            SamplePlayer::new(sound.clone())
                .looping()
                .with_volume(Volume::Linear(volume)),
            SoundVolume(volume),
            DefaultPool,
        ))
        .id()
}

/// Start (or keep) a flat sound playing, or hold it where it is.
pub fn set_paused(settings: &mut bevy_seedling::prelude::PlaybackSettings, paused: bool) {
    match (paused, *settings.play) {
        (true, true) => settings.pause(),
        (false, false) => settings.play(),
        _ => {}
    }
}

fn steer_volumes(
    players: Query<(&SoundVolume, &SampleEffects), Changed<SoundVolume>>,
    mut volumes: Query<&mut VolumeNode>,
) {
    for (v, effects) in &players {
        if let Ok(mut node) = volumes.get_effect_mut(effects) {
            node.volume = Volume::Linear(v.0.max(0.0));
        }
    }
}

/// Resample mono `samples` from `from` Hz to `to` Hz (cubic Hermite: clean
/// enough for one-off conversions of synthesized clips).
pub fn resample(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || samples.is_empty() || from == 0 || to == 0 {
        return samples.to_vec();
    }
    let step = from as f64 / to as f64;
    let n = ((samples.len() as f64) / step).floor() as usize;
    let at = |i: isize| -> f32 {
        let i = i.clamp(0, samples.len() as isize - 1) as usize;
        samples[i]
    };
    (0..n)
        .map(|k| {
            let x = k as f64 * step;
            let i = x.floor() as isize;
            let t = (x - i as f64) as f32;
            let (y0, y1, y2, y3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            let c1 = 0.5 * (y2 - y0);
            let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
            let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
            ((c3 * t + c2) * t + c1) * t + y1
        })
        .collect()
}

/// A playable sound from mono samples at `rate`, converted to the graph's
/// `out_rate`.
pub fn from_samples(samples: &[f32], rate: u32, out_rate: u32) -> AudioSample {
    // The old path went through 16-bit WAV bytes, which clamped to full scale;
    // raw f32 samples (and the resampler's overshoot) must not exceed it either.
    let mut data = resample(samples, rate, out_rate);
    for s in &mut data {
        *s = s.clamp(-1.0, 1.0);
    }
    let out = NonZeroU32::new(out_rate).unwrap_or(NonZeroU32::MIN);
    let original = NonZeroU32::new(rate).unwrap_or(out);
    AudioSample::new(
        InterleavedResourceF32 {
            data,
            channels: NonZeroUsize::MIN,
            sample_rate: Some(out),
        },
        original,
    )
}

/// A playable sound from a 16-bit PCM WAV (what the synths make, see
/// [`crate::features::bear::wav_bytes`]); `None` if it isn't one.
pub fn from_wav(bytes: &[u8], out_rate: u32) -> Option<AudioSample> {
    let (samples, rate) = parse_wav(bytes)?;
    Some(from_samples(&samples, rate, out_rate))
}

/// Mono samples and the rate of a 16-bit PCM WAV (channels folded down).
pub fn parse_wav(bytes: &[u8]) -> Option<(Vec<f32>, u32)> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12;
    let mut format: Option<(u16, u32, u16)> = None;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?) as usize;
        let body = bytes.get(at + 8..(at + 8 + len).min(bytes.len()))?;
        if id == b"fmt " && body.len() >= 16 {
            let ch = u16::from_le_bytes([body[2], body[3]]).max(1);
            let rate = u32::from_le_bytes(body[4..8].try_into().ok()?);
            let bits = u16::from_le_bytes([body[14], body[15]]);
            format = Some((ch, rate, bits));
        }
        if id == b"data" {
            let (ch, rate, bits) = format?;
            if bits != 16 {
                return None;
            }
            let ch = ch as usize;
            let raw: Vec<f32> = body
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| i16::from_le_bytes(*c) as f32 / 32768.0)
                .collect();
            let mono = raw
                .chunks(ch)
                .map(|f| f.iter().sum::<f32>() / ch as f32)
                .collect();
            return Some((mono, rate));
        }
        at += 8 + len + (len & 1);
    }
    None
}

/// A sound's samples folded to mono, and their rate.
pub fn pcm_mono(sample: &AudioSample) -> Option<(Vec<f32>, u32)> {
    let res = sample.get();
    let ch = res.num_channels().get();
    let len = res.len_frames() as usize;
    let rate = res
        .sample_rate()
        .map(|r| r.get())
        .unwrap_or(sample.original_sample_rate().get());
    if len == 0 {
        return None;
    }
    let mut bufs: Vec<Vec<f32>> = vec![vec![0.0; len]; ch];
    {
        let mut out: Vec<&mut [f32]> = bufs.iter_mut().map(|b| b.as_mut_slice()).collect();
        res.fill_buffers(&mut out, 0..len, 0);
    }
    let mono = (0..len)
        .map(|i| bufs.iter().map(|b| b[i]).sum::<f32>() / ch as f32)
        .collect();
    Some((mono, rate))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_round_trips() {
        let s: Vec<f32> = (0..441).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        let wav = crate::features::bear::wav_bytes(&s, 44_100);
        let (back, rate) = parse_wav(&wav).expect("parses");
        assert_eq!(rate, 44_100);
        assert_eq!(back.len(), s.len());
        assert!(back.iter().zip(&s).all(|(a, b)| (a - b).abs() < 1e-3));
    }

    #[test]
    fn resample_keeps_length_ratio_and_level() {
        let s: Vec<f32> = (0..44_100)
            .map(|i| (i as f32 * 440.0 * std::f32::consts::TAU / 44_100.0).sin())
            .collect();
        let r = resample(&s, 44_100, 48_000);
        assert!((r.len() as i64 - 48_000).abs() <= 2);
        let peak = r.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!((peak - 1.0).abs() < 0.02, "{peak}");
    }

    #[test]
    fn samples_play_back_at_the_out_rate() {
        let a = from_samples(&[0.1, 0.2, 0.3, 0.4], 24_000, 48_000);
        let (pcm, rate) = pcm_mono(&a).expect("pcm");
        assert_eq!(rate, 48_000);
        assert_eq!(pcm.len(), 8);
    }
}
