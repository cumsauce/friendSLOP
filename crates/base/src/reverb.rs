//! The room's echo: one shared convolution reverb that every world sound
//! sends into (`spatial.rs` gives each sound a send as well as its dry
//! stereo), instead of a small feedback network per sound. Its impulse
//! response is made here from the [`Room`] the listener is in (how long it
//! rings) and the tuning in `data/base/spatial.ron` (pre-delay, how bright it
//! starts and how dark it ends): decorrelated noise for each ear under an
//! exponential decay whose top end dies faster, so a tiled hall is a smooth
//! wash rather than the metallic ring of a few short delay lines. A new
//! [`Room`] makes a new response; nothing is loaded from a file.

use std::f32::consts::TAU;
use std::sync::Arc;

use bevy::prelude::*;
use bevy_seedling::prelude::{Connect, NodeLabel, RegisterNode, SoundEffectsBus, Volume};
use firewheel::collector::ArcGc;
use firewheel::nodes::convolution::{ConvolutionNode, ConvolutionNodeConfig};

use crate::spatial::{Room, TUNING};

/// The shared reverb every world sound sends into (its input is the sends'
/// stereo pair; its output joins the sound effects).
#[derive(NodeLabel, PartialEq, Eq, Debug, Hash, Clone)]
pub struct ReverbBus;

/// The longest response the node holds (a room may not ring longer).
pub const MAX_SECS: f32 = 8.0;

/// How the shared reverb sounds, whatever the room (`data/base/spatial.ron`).
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Reverb {
    /// Milliseconds of silence before the first of the wash arrives.
    pub predelay_ms: f32,
    /// Milliseconds the wash takes to build up after it (no click at the start).
    pub attack_ms: f32,
    /// The top end at the start of the tail (Hz).
    pub bright_hz: f32,
    /// The top end by the time it has died away 60 dB (Hz): lower is darker.
    pub dark_hz: f32,
    /// Overall level of the echo (1 = the old per-sound tail's).
    pub level: f32,
}

impl Default for Reverb {
    fn default() -> Self {
        Self {
            predelay_ms: 12.0,
            attack_ms: 8.0,
            bright_hz: 5000.0,
            dark_hz: 900.0,
            level: 1.0,
        }
    }
}

/// A stereo impulse response for a room ringing `rt60` seconds, at `rate`.
/// Each ear gets its own noise (so the echo is wide), shaped by the decay and
/// a low-pass that closes from `bright_hz` to `dark_hz` over `rt60`. Scaled
/// so its energy is that of the open air's old tail at the same `rt60`, times
/// `level`: the wet level of every sound stays where it was.
pub fn impulse(room: Room, tuning: &Reverb, rate: u32) -> [Vec<f32>; 2] {
    let r = rate.max(1000) as f32;
    let rt60 = room.rt60.clamp(0.2, MAX_SECS / 1.25);
    let pre = (tuning.predelay_ms.max(0.0) * 0.001 * r) as usize;
    let attack = (tuning.attack_ms.max(0.1) * 0.001 * r).max(1.0);
    let len = (pre + (rt60 * 1.2 * r) as usize).min((MAX_SECS * r) as usize);
    let bright = tuning.bright_hz.clamp(100.0, 0.45 * r);
    let dark = tuning.dark_hz.clamp(50.0, bright);
    let mut ears = [Vec::with_capacity(len), Vec::with_capacity(len)];
    for (ear, out) in ears.iter_mut().enumerate() {
        let mut x = 0x9E37_79B9u32 ^ (ear as u32 + 1).wrapping_mul(0x85EB_CA6B);
        let mut noise = move || {
            // Two uniforms summed: a softer, rounder noise than one.
            let mut u = || {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as f32 / u32::MAX as f32 - 0.5
            };
            u() + u()
        };
        let mut lp = [0.0f32; 2];
        out.resize(pre, 0.0);
        for i in 0..len.saturating_sub(pre) {
            let t = i as f32 / r;
            let decay = 10f32.powf(-3.0 * t / rt60);
            let rise = (i as f32 / attack).min(1.0);
            // The top end closes exponentially from bright to dark.
            let fc = bright * (dark / bright).powf((t / rt60).min(1.0));
            let a = 1.0 - (-TAU * fc / r).exp();
            let n = noise();
            lp[0] += a * (n - lp[0]);
            lp[1] += a * (lp[0] - lp[1]);
            out.push(lp[1] * decay * rise);
        }
    }
    let want = old_tail_energy(rate, rt60) * tuning.level.max(0.0).powi(2);
    let have: f32 = ears.iter().flatten().map(|s| s * s).sum::<f32>() / 2.0;
    if have > 0.0 {
        let k = (want / have).sqrt();
        for s in ears.iter_mut().flatten() {
            *s *= k;
        }
    }
    ears
}

/// The energy (per ear) of the per-sound tail this replaced, for one unit
/// impulse: four delay lines of 1031..1871 samples, a Hadamard mix and a
/// 3.2 kHz damping filter, read out half and half to each ear.
fn old_tail_energy(rate: u32, rt60: f32) -> f32 {
    let r = rate as f32;
    let lens = [1031.0f32, 1327.0, 1523.0, 1871.0].map(|l| (l * r / 44_100.0) as usize + 1);
    let mut lines = lens.map(|l| vec![0.0f32; l]);
    let mut at = [0usize; 4];
    let fb = lens.map(|l| 10f32.powf(-3.0 * l as f32 / (r * rt60)));
    let mut lp = [0.0f32; 4];
    let damp = 1.0 - (-TAU * 3200.0 / r).exp();
    let mut e = 0.0;
    for i in 0..(rt60 * 1.2 * r) as usize {
        let x = if i == 0 { 1.0 } else { 0.0 };
        let mut s = [0.0; 4];
        for (k, s) in s.iter_mut().enumerate() {
            let y = lines[k][at[k]];
            lp[k] += damp * (y - lp[k]);
            *s = lp[k];
        }
        let h = [
            s[0] + s[1] + s[2] + s[3],
            s[0] - s[1] + s[2] - s[3],
            s[0] + s[1] - s[2] - s[3],
            s[0] - s[1] - s[2] + s[3],
        ];
        for k in 0..4 {
            lines[k][at[k]] = x + fb[k] * 0.5 * h[k];
            at[k] = (at[k] + 1) % lines[k].len();
        }
        let (l, rr) = (0.5 * (s[0] + s[2]), 0.5 * (s[1] + s[3]));
        e += (l * l + rr * rr) / 2.0;
    }
    e
}

pub fn plugin(app: &mut App) {
    if !app.is_plugin_added::<bevy_seedling::SeedlingCorePlugin>() {
        return;
    }
    app.register_node::<ConvolutionNode>()
        .add_systems(Startup, spawn_reverb)
        .add_systems(PostUpdate, retune);
}

fn spawn_reverb(mut commands: Commands) {
    commands
        .spawn((
            ReverbBus,
            ConvolutionNode {
                impulse_response: None,
                wet_gain: Volume::UNITY_GAIN,
                ..default()
            },
            ConvolutionNodeConfig {
                max_impulse_length_seconds: MAX_SECS as f64,
                ..default()
            },
        ))
        .connect(SoundEffectsBus);
}

/// A new room (or tuning, or output rate) gets a new response.
fn retune(
    room: Option<Res<Room>>,
    rate: Option<Res<bevy_seedling::context::SampleRate>>,
    mut node: Query<&mut ConvolutionNode, With<ReverbBus>>,
    mut made: Local<Option<(Room, Reverb, u32)>>,
) {
    let Ok(mut node) = node.single_mut() else {
        return;
    };
    let room = room.as_deref().copied().unwrap_or_default();
    let tuning = TUNING.get().reverb;
    let rate = crate::sound::stream_rate(rate.as_deref());
    let want = (room, tuning, rate);
    if *made == Some(want) {
        return;
    }
    *made = Some(want);
    let ir: Vec<Vec<f32>> = impulse(room, &tuning, rate).into();
    node.impulse_response = Some(ArcGc::new_unsized(|| Arc::new(ir) as _));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn energy(s: &[f32]) -> f32 {
        s.iter().map(|x| x * x).sum()
    }

    #[test]
    fn a_longer_room_rings_longer_and_dies_away() {
        let t = Reverb::default();
        let short = impulse(Room::OPEN, &t, 44_100);
        let hall = impulse(
            Room {
                wet: 6.0,
                rt60: 2.8,
            },
            &t,
            44_100,
        );
        assert!(hall[0].len() > short[0].len() * 2);
        for ir in [&short, &hall] {
            assert!(ir.iter().flatten().all(|s| s.is_finite()));
            let n = ir[0].len();
            let head = energy(&ir[0][..n / 10]);
            let end = energy(&ir[0][n * 9 / 10..]);
            assert!(end < head * 1e-4, "{end} vs {head}");
            assert!(
                ir[0][..(t.predelay_ms * 44.1) as usize - 1]
                    .iter()
                    .all(|s| *s == 0.0)
            );
        }
    }

    #[test]
    fn the_ears_hear_different_noise() {
        let ir = impulse(Room::OPEN, &Reverb::default(), 48_000);
        let dot: f32 = ir[0].iter().zip(&ir[1]).map(|(a, b)| a * b).sum();
        let corr = dot / (energy(&ir[0]) * energy(&ir[1])).sqrt();
        assert!(corr.abs() < 0.2, "correlation {corr}");
    }

    #[test]
    fn the_echo_is_as_loud_as_the_old_tail() {
        let t = Reverb::default();
        for rt60 in [1.1, 2.8] {
            let ir = impulse(Room { wet: 1.0, rt60 }, &t, 44_100);
            let e = (energy(&ir[0]) + energy(&ir[1])) / 2.0;
            let old = old_tail_energy(44_100, rt60);
            assert!((e / old - 1.0).abs() < 0.01, "{e} vs {old}");
        }
    }

    #[test]
    fn the_tail_darkens_as_it_rings() {
        let ir = impulse(
            Room {
                wet: 1.0,
                rt60: 3.0,
            },
            &Reverb::default(),
            44_100,
        );
        // Sample-to-sample change against level: high frequencies move it more.
        let roughness = |s: &[f32]| {
            let d: f32 = s.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
            d / energy(s)
        };
        let early = &ir[0][1000..11_000];
        let late = &ir[0][80_000..90_000];
        assert!(roughness(late) < roughness(early) * 0.5);
    }
}
