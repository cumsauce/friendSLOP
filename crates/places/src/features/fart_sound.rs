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

use crate::sound::AudioSample;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::fart::Farted;

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
        .add_hud(spawn_tone_text)
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
    me: Option<Res<LocalPlayer>>,
    players: Query<(&Player, &FartTone)>,
    audio: Option<Res<Assets<AudioSample>>>,
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
    let make = move || wav_bytes(&fart_samples(&tone, fullness, seed, RATE), RATE);
    if me.is_some_and(|m| m.0 == farted.player) {
        // Your own, right where you are: made live in the audio graph.
        let e = play_live(&mut commands, tone, fullness, seed);
        commands.entity(e).insert(DespawnOnExit(AppState::InGame));
    } else {
        // Someone else's: heard from where they let it go.
        let at = Vec3::new(
            farted.at.x,
            crate::terrain::height_at(farted.at.x, farted.at.y) + 0.9,
            farted.at.y,
        );
        crate::spatial::play_wav_at(&mut commands, at, 1.0, crate::spatial::Carry::LOUD, make);
    }
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

/// One fart being made a sample at a time: the lip valve described at the top
/// of this file. Iterating a `FartVoice` gives the raw sound (before the final
/// polish); `fart_samples` renders it all at once, the audio graph's live node
/// ([`play_live`]) pulls it as the speakers want it.
pub struct FartVoice {
    tone: FartTone,
    rate: u32,
    n: usize,
    i: usize,
    dt: f32,
    wet: f32,
    mu: f32,
    x: f32,
    v: f32,
    pressure: f32,
    rng: u32,
    sputter: f32,
    last_flow: f32,
    g: f32,
    q: f32,
    ic1: f32,
    ic2: f32,
}

impl FartVoice {
    /// `fullness` (0..1) is how much gas there was; `seed` varies the
    /// turbulence so no two players' farts are identical.
    pub fn new(tone: &FartTone, fullness: f32, seed: u32, rate: u32) -> Self {
        let tone = tone.clamped();
        let fullness = fullness.clamp(0.1, 1.0);
        let secs = tone.length * (0.4 + 0.6 * fullness);
        let dt = 1.0 / rate as f32;
        let wet = tone.timbre;
        // Cheek resonance: a two-pole bandpass-ish lowpass.
        let cutoff = 1800.0 - 1300.0 * wet;
        Self {
            tone,
            rate,
            n: (secs * rate as f32) as usize,
            i: 0,
            dt,
            wet,
            // Lips: van der Pol oscillator x'' - mu*w*(p - x^2)*x' + w^2*x = 0.
            mu: 0.6 + 1.2 * wet,
            x: 0.05,
            v: 0.0,
            // Reservoir pressure, normalised so 1 = a full, firm push.
            pressure: 0.6 + 0.6 * fullness,
            rng: seed.wrapping_mul(0x9E37_79B9) | 1,
            sputter: 0.0,
            last_flow: 0.0,
            g: (std::f32::consts::PI * cutoff * dt).tan(),
            q: 0.9,
            ic1: 0.0,
            ic2: 0.0,
        }
    }

    /// How many samples the fart lasts.
    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    fn noise(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// The raw sound, a sample at a time, until the gas is gone.
impl Iterator for FartVoice {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.i >= self.n {
            return None;
        }
        let (tone, dt, wet, rate) = (self.tone, self.dt, self.wet, self.rate);
        let p = self.i as f32 / self.n.max(1) as f32;
        // Abdomen: squeeze in quickly, then the push gives out.
        let push = (p * 12.0).min(1.0) * (1.0 - p).powf(0.6);
        // Wet farts sputter: the pressure stutters randomly.
        self.sputter = self.sputter * 0.995 + self.noise() * 0.004 * wet;
        let drive = (self.pressure * push + self.sputter).max(0.0);
        // Pitch sags as pressure drops, and wobbles a little.
        let w = std::f32::consts::TAU
            * tone.pitch
            * (0.75 + 0.25 * drive.sqrt())
            * (1.0 + 0.02 * (self.i as f32 * dt * 7.0).sin());
        let a = self.mu * w * (drive - self.x * self.x) * self.v - w * w * self.x;
        self.v += a * dt;
        self.x += self.v * dt;
        // Flow through the opening (only while open), Bernoulli: ~ area * sqrt(p).
        let open = self.x.max(0.0);
        let flow = open * drive.sqrt();
        self.pressure = (self.pressure - flow * dt * 0.15).max(0.0);
        // Turbulence riding on the flow.
        let turb = self.noise() * flow * (0.3 + 1.6 * wet);
        // Radiation: the far field hears the change in flow.
        let radiated = (flow - self.last_flow) * rate as f32 / (tone.pitch * 4.0) + turb;
        self.last_flow = flow;
        // State-variable lowpass for the cheeks.
        let (g, q) = (self.g, self.q);
        let v3 = radiated - self.ic2;
        let v1 = (self.ic1 + g * v3) / (1.0 + g * (g + 1.0 / q));
        let v2 = self.ic2 + g * v1;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        let fade = ((1.0 - p) * 30.0).min(1.0);
        self.i += 1;
        Some((v2 * 0.8 * fade).tanh() * 0.9)
    }
}

/// Synthesize one fart all at once (other players' farts, and the tests).
pub fn fart_samples(tone: &FartTone, fullness: f32, seed: u32, rate: u32) -> Vec<f32> {
    let mut voice = FartVoice::new(tone, fullness, seed, rate);
    let mut out = Vec::with_capacity(voice.len());
    out.extend(&mut voice);
    // Smooth off the fizz and normalise so quiet tones are still audible.
    crate::features::synth_audio::master(&mut out, rate, 7000.0, 0.75);
    out
}

/// The final polish of `master`, one sample at a time for a live fart: remove
/// DC, a gentle two-pole low-pass at 7 kHz, 4 ms fades and a fixed gain (there
/// is no peak to normalise to before the sound has been made).
struct LivePolish {
    a: f32,
    lp1: f32,
    lp2: f32,
    dc_x: f32,
    dc_y: f32,
    fade: usize,
    i: usize,
    n: usize,
}

/// Raw farts peak near this before the polish; the gain brings them to the
/// level `master` would (0.75).
const LIVE_GAIN: f32 = 0.8;

impl LivePolish {
    fn new(rate: u32, n: usize) -> Self {
        Self {
            a: 1.0 - (-std::f32::consts::TAU * 7000.0 / rate as f32).exp(),
            lp1: 0.0,
            lp2: 0.0,
            dc_x: 0.0,
            dc_y: 0.0,
            fade: ((0.004 * rate as f32) as usize).min(n / 2),
            i: 0,
            n,
        }
    }

    fn process(&mut self, s: f32) -> f32 {
        self.dc_y = s - self.dc_x + 0.995 * self.dc_y;
        self.dc_x = s;
        self.lp1 += self.a * (self.dc_y - self.lp1);
        self.lp2 += self.a * (self.lp1 - self.lp2);
        let mut out = self.lp2 * LIVE_GAIN;
        let (i, n, fade) = (self.i, self.n, self.fade);
        if fade > 0 {
            if i < fade {
                out *= i as f32 / fade as f32;
            }
            if n - 1 - i.min(n - 1) < fade {
                out *= (n - 1 - i.min(n - 1)) as f32 / fade as f32;
            }
        }
        self.i += 1;
        out
    }
}

/// Play a fart on the audio thread: the lip valve runs live in a graph node,
/// a sample at a time, at the graph's own sample rate. Nothing is rendered
/// ahead or loaded.
pub fn play_live(commands: &mut Commands, tone: FartTone, fullness: f32, seed: u32) -> Entity {
    friendslop_base::live_synth::play(commands, 1.0, move |rate| {
        let mut voice = FartVoice::new(&tone, fullness, seed, rate);
        let mut polish = LivePolish::new(rate, voice.len());
        Box::new(move || voice.next().map(|s| polish.process(s)))
    })
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
    fn the_live_voice_is_the_same_fart_and_about_as_loud() {
        let t = tone(90.0, 1.0, 0.4);
        let whole = fart_samples(&t, 1.0, 7, RATE);
        let mut voice = FartVoice::new(&t, 1.0, 7, RATE);
        let mut polish = LivePolish::new(RATE, voice.len());
        let live: Vec<f32> =
            std::iter::from_fn(|| voice.next().map(|s| polish.process(s))).collect();
        assert_eq!(live.len(), whole.len());
        assert!(live.iter().all(|x| x.is_finite()));
        let peak = |v: &[f32]| v.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        let (a, b) = (peak(&live), peak(&whole));
        assert!(
            a > 0.4 && a < 1.2 && (a / b) > 0.5 && (a / b) < 1.6,
            "live {a}, whole {b}"
        );
        // The same shape: the loud part is in the same place.
        let loudest = |v: &[f32]| {
            v.iter()
                .enumerate()
                .fold(
                    (0, 0.0f32),
                    |m, (i, x)| if x.abs() > m.1 { (i, x.abs()) } else { m },
                )
                .0
        };
        let gap = loudest(&live).abs_diff(loudest(&whole));
        assert!(gap < RATE as usize / 4, "loudest samples {gap} apart");
        // Every tone is audible and none clips, with no peak to normalise to.
        for t in [0.0, 0.5, 1.0] {
            for pitch in [30.0, 90.0, 400.0] {
                let tone = tone(pitch, 1.0, t);
                let mut voice = FartVoice::new(&tone, 1.0, 1, RATE);
                let mut polish = LivePolish::new(RATE, voice.len());
                let peak = std::iter::from_fn(|| voice.next().map(|s| polish.process(s)))
                    .fold(0.0f32, |m, x| m.max(x.abs()));
                assert!(peak > 0.25 && peak <= 1.0, "peak {peak} at {pitch} Hz, {t}");
            }
        }
        // Fades at both ends so it doesn't click.
        assert!(live[0].abs() < 0.05 && live[live.len() - 1].abs() < 0.05);
    }

    #[test]
    fn bad_tones_are_clamped() {
        let t = tone(f32::NAN, 99.0, -3.0).clamped();
        assert_eq!(t.pitch, FartTone::default().pitch);
        assert_eq!(t.length, LENGTH_RANGE.1);
        assert_eq!(t.timbre, 0.0);
    }
}
