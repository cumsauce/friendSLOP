//! Physically modelled, tunable fart noises. From issue #26 by doomy.
//!
//! Every player has a replicated `FartTone` (pitch, length, timbre) they tune
//! with the keyboard. When anyone farts (see `fart.rs`), every peer synthesizes
//! that player's fart from their tone and plays it.
//!
//! The synth is a small physical model of a lip valve, the same family of
//! model used for brass players' lips:
//! - a gas reservoir whose pressure pushes the "lips" open and drains as gas
//!   flows out (so the fart sags in pitch and fades as it empties);
//! - the lips are a self-oscillating mass-spring (a van der Pol oscillator,
//!   driven by the pressure and limited by its own motion), whose natural
//!   frequency is the pitch;
//! - flow only passes while the lips are open, giving buzzy pulses, plus
//!   turbulence noise proportional to the flow (the texture);
//! - the sound is radiated (differentiated) and filtered by a soft "cheek"
//!   resonance. Timbre moves from tight and dry (stiff, little turbulence,
//!   bright) to loose and wet (sloppy damping, pressure sputters, lots of
//!   turbulence, dark).

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::fart::Farted;
use crate::features::synth_audio::play_synth;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// How a player's farts sound.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct FartTone {
    /// Lip resonance in Hz.
    pub pitch: f32,
    /// Seconds, for a full tank. Emptier tanks give shorter farts.
    pub length: f32,
    /// 0 = tight and dry, 1 = loose and wet.
    pub timbre: f32,
}

impl Default for FartTone {
    fn default() -> Self {
        Self {
            pitch: 90.0,
            length: 1.2,
            timbre: 0.4,
        }
    }
}

pub const PITCH_RANGE: (f32, f32) = (30.0, 400.0);
pub const LENGTH_RANGE: (f32, f32) = (0.15, 4.0);

impl FartTone {
    /// Keep every knob in a sane range (and finite).
    pub fn clamped(self) -> Self {
        let fix = |v: f32, lo: f32, hi: f32, d: f32| {
            if v.is_finite() { v.clamp(lo, hi) } else { d }
        };
        let d = Self::default();
        Self {
            pitch: fix(self.pitch, PITCH_RANGE.0, PITCH_RANGE.1, d.pitch),
            length: fix(self.length, LENGTH_RANGE.0, LENGTH_RANGE.1, d.length),
            timbre: fix(self.timbre, 0.0, 1.0, d.timbre),
        }
    }
}

/// Client -> server: "my farts should sound like this now."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SetFartTone(pub FartTone);

pub fn protocol(app: &mut App) {
    app.replicate::<FartTone>()
        .add_client_event::<SetFartTone>(Channel::Ordered)
        .add_observer(give_tone)
        .add_observer(set_tone);
}

fn give_tone(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() != ServerState::Running {
        return;
    }
    commands.entity(add.entity).insert(FartTone::default());
}

fn set_tone(
    set: On<FromClient<SetFartTone>>,
    still: Query<&crate::features::stillness::Stillness>,
    mut players: Query<(&PlayerOwner, &mut FartTone)>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let owner = match set.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    if let Some((_, mut tone)) = players.iter_mut().find(|(o, _)| o.0 == owner) {
        *tone = set.0.clamped();
    }
}

pub fn plugin(app: &mut App) {
    app.add_observer(play_fart)
        .add_systems(OnEnter(AppState::InGame), spawn_tone_text)
        .add_systems(
            Update,
            (tune_fart, show_tone.run_if(crate::world::hud_tick()))
                .run_if(in_state(AppState::InGame)),
        );
}

use crate::features::synth_audio::RATE;

/// Every peer: synthesize and play the farter's own fart.
fn play_fart(
    farted: On<Farted>,
    mut commands: Commands,
    players: Query<(&Player, &FartTone)>,
    audio: Option<Res<Assets<AudioSource>>>,
) {
    if audio.is_none() {
        return;
    }
    let tone = players
        .iter()
        .find(|(p, _)| p.id == farted.player)
        .map(|(_, t)| *t)
        .unwrap_or_default();
    let fullness = farted.pressure as f32 / 100.0;
    let seed = farted.player as u32;
    play_synth(&mut commands, move || {
        wav_bytes(&fart_samples(&tone, fullness, seed, RATE), RATE)
    });
}

/// Tuning keys: [ ] pitch, - = length, ; ' timbre.
fn tune_fart(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    local: Option<Res<LocalPlayer>>,
    players: Query<(&Player, &FartTone)>,
) {
    let Some(local) = local else { return };
    let Some((_, tone)) = players.iter().find(|(p, _)| p.id == local.0) else {
        return;
    };
    let mut new = *tone;
    let pressed = |k| keys.just_pressed(k);
    if pressed(KeyCode::BracketLeft) {
        new.pitch /= 1.12;
    }
    if pressed(KeyCode::BracketRight) {
        new.pitch *= 1.12;
    }
    if pressed(KeyCode::Minus) {
        new.length -= 0.15;
    }
    if pressed(KeyCode::Equal) {
        new.length += 0.15;
    }
    if pressed(KeyCode::Semicolon) {
        new.timbre -= 0.1;
    }
    if pressed(KeyCode::Quote) {
        new.timbre += 0.1;
    }
    let new = new.clamped();
    if new != *tone {
        commands.client_trigger(SetFartTone(new));
    }
}

#[derive(Component)]
struct ToneText;

fn spawn_tone_text(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        ToneText,
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

fn show_tone(
    local: Option<Res<LocalPlayer>>,
    players: Query<(&Player, &FartTone)>,
    mut text: Query<&mut Text, With<ToneText>>,
) {
    let Some(local) = local else { return };
    let Some((_, tone)) = players.iter().find(|(p, _)| p.id == local.0) else {
        return;
    };
    let want = format!(
        "fart: pitch {:.0} Hz [ ]  length {:.2}s - =  timbre {:.0}% wet ; '",
        tone.pitch,
        tone.length,
        tone.timbre * 100.0
    );
    for mut text in &mut text {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

/// Synthesize one fart. `fullness` (0..1) is how much gas there was; `seed`
/// varies the turbulence so no two players' farts are identical.
pub fn fart_samples(tone: &FartTone, fullness: f32, seed: u32, rate: u32) -> Vec<f32> {
    let tone = tone.clamped();
    let fullness = fullness.clamp(0.1, 1.0);
    let secs = tone.length * (0.4 + 0.6 * fullness);
    let n = (secs * rate as f32) as usize;
    let dt = 1.0 / rate as f32;
    let wet = tone.timbre;

    // Lips: van der Pol oscillator x'' - mu*w*(p - x^2)*x' + w^2*x = 0.
    let mu = 0.6 + 1.2 * wet;
    let (mut x, mut v) = (0.05f32, 0.0f32);
    // Reservoir pressure, normalised so 1 = a full, firm push.
    let mut pressure = 0.6 + 0.6 * fullness;
    let mut rng = seed.wrapping_mul(0x9E37_79B9) | 1;
    let mut noise = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        (rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    };
    let mut sputter = 0.0f32;
    let mut last_flow = 0.0f32;
    // Cheek resonance: a two-pole bandpass-ish lowpass.
    let cutoff = 1800.0 - 1300.0 * wet;
    let g = (std::f32::consts::PI * cutoff * dt).tan();
    let q = 0.9;
    let (mut ic1, mut ic2) = (0.0f32, 0.0f32);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let p = i as f32 / n.max(1) as f32;
        // Abdomen: squeeze in quickly, then the push gives out.
        let push = (p * 12.0).min(1.0) * (1.0 - p).powf(0.6);
        // Wet farts sputter: the pressure stutters randomly.
        sputter = sputter * 0.995 + noise() * 0.004 * wet;
        let drive = (pressure * push + sputter).max(0.0);
        // Pitch sags as pressure drops, and wobbles a little.
        let w = std::f32::consts::TAU
            * tone.pitch
            * (0.75 + 0.25 * drive.sqrt())
            * (1.0 + 0.02 * (i as f32 * dt * 7.0).sin());
        let a = mu * w * (drive - x * x) * v - w * w * x;
        v += a * dt;
        x += v * dt;
        // Flow through the opening (only while open), Bernoulli: ~ area * sqrt(p).
        let open = x.max(0.0);
        let flow = open * drive.sqrt();
        pressure = (pressure - flow * dt * 0.15).max(0.0);
        // Turbulence riding on the flow.
        let turb = noise() * flow * (0.3 + 1.6 * wet);
        // Radiation: the far field hears the change in flow.
        let radiated = (flow - last_flow) * rate as f32 / (tone.pitch * 4.0) + turb;
        last_flow = flow;
        // State-variable lowpass for the cheeks.
        let v3 = radiated - ic2;
        let v1 = (ic1 + g * v3) / (1.0 + g * (g + 1.0 / q));
        let v2 = ic2 + g * v1;
        ic1 = 2.0 * v1 - ic1;
        ic2 = 2.0 * v2 - ic2;
        let fade = ((1.0 - p) * 30.0).min(1.0);
        out.push((v2 * 0.8 * fade).tanh() * 0.9);
    }
    // Smooth off the fizz and normalise so quiet tones are still audible.
    crate::features::synth_audio::master(&mut out, rate, 7000.0, 0.75);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rough fundamental: the autocorrelation peak over the loud early part.
    fn freq(s: &[f32]) -> f32 {
        let win = &s[s.len() / 8..s.len() / 8 + 4096];
        let lags = (RATE as usize / 500)..(RATE as usize / 25);
        let corr =
            |lag: usize| -> f32 { win.iter().zip(&win[lag..]).map(|(a, b)| a * b).sum::<f32>() };
        let best = lags
            .max_by(|a, b| corr(*a).total_cmp(&corr(*b)))
            .unwrap_or(1);
        RATE as f32 / best as f32
    }

    fn tone(pitch: f32, length: f32, timbre: f32) -> FartTone {
        FartTone {
            pitch,
            length,
            timbre,
        }
    }

    #[test]
    fn farts_are_finite_audible_and_unclipped() {
        for t in [0.0, 0.5, 1.0] {
            for pitch in [30.0, 90.0, 400.0] {
                let s = fart_samples(&tone(pitch, 1.0, t), 1.0, 1, RATE);
                assert!(s.iter().all(|x| x.is_finite()));
                let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
                assert!(peak > 0.3 && peak <= 1.0, "peak {peak} at {pitch} Hz, {t}");
            }
        }
    }

    #[test]
    fn length_is_controllable_and_emptier_is_shorter() {
        let short = fart_samples(&tone(90.0, 0.5, 0.3), 1.0, 1, RATE).len();
        let long = fart_samples(&tone(90.0, 2.0, 0.3), 1.0, 1, RATE).len();
        let empty = fart_samples(&tone(90.0, 2.0, 0.3), 0.2, 1, RATE).len();
        assert_eq!(long, 2 * RATE as usize);
        assert!(short < long && empty < long);
    }

    #[test]
    fn pitch_is_controllable() {
        let low = freq(&fart_samples(&tone(60.0, 2.0, 0.0), 1.0, 1, RATE));
        let high = freq(&fart_samples(&tone(240.0, 2.0, 0.0), 1.0, 1, RATE));
        assert!(high > low * 2.0, "low {low} Hz, high {high} Hz");
    }

    #[test]
    fn bad_tones_are_clamped() {
        let t = tone(f32::NAN, 99.0, -3.0).clamped();
        assert_eq!(t.pitch, FartTone::default().pitch);
        assert_eq!(t.length, LENGTH_RANGE.1);
        assert_eq!(t.timbre, 0.0);
    }
}
