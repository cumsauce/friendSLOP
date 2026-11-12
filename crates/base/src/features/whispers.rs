//! Creepy whispering at night (#235, catherine: "once a night have a low
//! chance to play creepy whispering and murmuring noises that sound like
//! they're coming from behind you").
//!
//! The game has no day/night cycle, so "night" is the player's own clock:
//! 22:00 to 05:00 local time. The first time you're in the game on a night,
//! that night gets one roll ([`CHANCE`]). If it hits, a few minutes later two
//! whisperers and a low murmur start up just behind your head and stay behind
//! you for a few seconds, even if you turn round. Purely local: only you hear
//! it. The roll is saved (`whispers.txt`), so restarting doesn't re-roll.
//!
//! The voices are made from shaped noise (no recordings): breathy vowels
//! through formant filters, hissed "s"/"sh" bits, and a muffled murmur.

use bevy::prelude::*;

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::synth_audio::{RATE, master};
use crate::spatial::{Carry, Ears, play_wav_at};

/// Tuning numbers: `assets/data/base/whispers.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Chance that a given night has whispers.
    pub chance: f32,
    /// Night is from this local hour...
    pub night_from: u32,
    /// ...to this one (exclusive).
    pub night_until: u32,
    /// How long each voice lasts.
    pub seconds: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/base/whispers.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}
/// The whispers come this many seconds (in game) after the roll, at random.
const DELAY: (f32, f32) = (40.0, 600.0);
/// Where the roll is remembered.
const FILE: &str = "whispers.txt";
/// The voices: offsets from your head in its own space (+Z is behind you),
/// and their loudness.
const VOICES: [(Vec3, f32); 3] = [
    (Vec3::new(-0.45, 0.05, 0.8), 0.55),
    (Vec3::new(0.5, -0.05, 0.9), 0.5),
    (Vec3::new(0.0, -0.25, 1.4), 0.4),
];
/// Close and quiet: right behind your ear, nowhere else.
const CARRY: Carry = Carry {
    near: 0.6,
    far: 8.0,
};

pub fn plugin(app: &mut App) {
    app.init_resource::<Tonight>().add_systems(
        Update,
        (roll_tonight, start_whispers, keep_behind).run_if(in_state(AppState::InGame)),
    );
}

/// Is `hour` (0..24, local) night?
pub fn is_night(hour: u32) -> bool {
    !(tuning().night_until..tuning().night_from).contains(&hour)
}

/// Which night local time `local_secs` (seconds since the epoch, shifted to
/// local time) belongs to: the evening and the small hours after it share a
/// number.
pub fn night_of(local_secs: i64) -> i64 {
    (local_secs - 12 * 3600).div_euclid(86_400)
}

/// The local hour of `local_secs`.
pub fn hour_of(local_secs: i64) -> u32 {
    (local_secs.rem_euclid(86_400) / 3600) as u32
}

/// Does a night whose roll came out as `roll` (0..1) get whispers?
pub fn whispers_tonight(roll: f32) -> bool {
    roll < tuning().chance
}

/// Seconds east of UTC on this machine right now.
fn utc_offset_secs() -> i64 {
    #[cfg(target_arch = "wasm32")]
    return (-js_sys::Date::new_0().get_timezone_offset() * 60.0) as i64;
    #[cfg(not(target_arch = "wasm32"))]
    i64::from(chrono::Local::now().offset().local_minus_utc())
}

fn local_secs() -> i64 {
    crate::settings::since_epoch().as_secs() as i64 + utc_offset_secs()
}

/// This run's state: the night already rolled, and the countdown if it hit.
#[derive(Resource, Default)]
struct Tonight {
    /// `Some` once we've checked the save file.
    rolled: Option<i64>,
    countdown: Option<f32>,
}

/// A whisper riding along behind your head.
#[derive(Component)]
struct Behind(Vec3);

fn roll_tonight(mut tonight: ResMut<Tonight>, mut checked: Local<f32>, time: Res<Time>) {
    // The clock only needs a look every few seconds.
    *checked -= time.delta_secs();
    if *checked > 0.0 {
        return;
    }
    *checked = 5.0;
    let now = local_secs();
    if !is_night(hour_of(now)) {
        return;
    }
    let night = night_of(now);
    if tonight.rolled.is_none() {
        tonight.rolled = Some(
            crate::settings::load_local(FILE)
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(i64::MIN),
        );
    }
    if tonight.rolled == Some(night) {
        return;
    }
    tonight.rolled = Some(night);
    crate::settings::save_local(FILE, &night.to_string());
    let mut rng = Noise(crate::settings::since_epoch().as_nanos() as u64 | 1);
    // Stir the clock bits before using them.
    (0..8).for_each(|_| {
        rng.next();
    });
    if whispers_tonight(rng.next()) {
        let t = rng.next();
        tonight.countdown = Some(DELAY.0 + (DELAY.1 - DELAY.0) * t);
    }
}

fn start_whispers(
    mut commands: Commands,
    mut tonight: ResMut<Tonight>,
    time: Res<Time>,
    ears: Res<Ears>,
) {
    let Some(left) = tonight.countdown.as_mut() else {
        return;
    };
    *left -= time.delta_secs();
    if *left > 0.0 {
        return;
    }
    tonight.countdown = None;
    let seed = crate::settings::since_epoch().subsec_nanos() as u64;
    for (i, (offset, volume)) in VOICES.into_iter().enumerate() {
        let seed = seed.wrapping_add(i as u64 * 0x9E37_79B9);
        let murmur = i == 2;
        let at = ears.pos + ears.rot * offset;
        let e = play_wav_at(&mut commands, at, volume, CARRY, move || {
            let samples = if murmur {
                murmur_samples(seed, RATE)
            } else {
                whisper_samples(seed, RATE)
            };
            wav_bytes(&samples, RATE)
        });
        commands.entity(e).insert(Behind(offset));
    }
}

/// Keep the voices behind you as you turn: you can't turn round to face them.
fn keep_behind(ears: Res<Ears>, mut voices: Query<(&Behind, &mut Transform)>) {
    for (behind, mut t) in &mut voices {
        t.translation = ears.pos + ears.rot * behind.0;
    }
}

/// Small deterministic noise source.
struct Noise(u64);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
    fn signed(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }
}

/// A two-pole resonant band-pass (RBJ, constant peak gain).
#[derive(Default, Clone, Copy)]
struct BandPass {
    b0: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl BandPass {
    fn tune(&mut self, freq: f32, q: f32, rate: u32) {
        let w = std::f32::consts::TAU * freq / rate as f32;
        let alpha = w.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        self.b0 = alpha / a0;
        self.a1 = -2.0 * w.cos() / a0;
        self.a2 = (1.0 - alpha) / a0;
    }
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x - self.b0 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

/// Whispered vowels: (first, second formant) in Hz.
const VOWELS: [(f32, f32); 6] = [
    (730.0, 1090.0),
    (270.0, 2290.0),
    (530.0, 1840.0),
    (570.0, 840.0),
    (300.0, 870.0),
    (440.0, 1020.0),
];

/// One whisperer: bursts of breathy syllables with hissed consonants, in
/// short phrases with pauses between them.
pub fn whisper_samples(seed: u64, rate: u32) -> Vec<f32> {
    let n = (tuning().seconds * rate as f32) as usize;
    let mut out = vec![0.0f32; n];
    let mut rng = Noise(seed | 1);
    let (mut f1, mut f2, mut hiss) = (
        BandPass::default(),
        BandPass::default(),
        BandPass::default(),
    );
    hiss.tune(5200.0, 1.2, rate);
    let mut i = (rng.next() * 0.4 * rate as f32) as usize;
    while i < n {
        // A phrase of 3..8 syllables.
        let syllables = 3 + (rng.next() * 6.0) as usize;
        for _ in 0..syllables {
            let len = ((0.12 + rng.next() * 0.16) * rate as f32) as usize;
            let (a, b) = VOWELS[(rng.next() * VOWELS.len() as f32) as usize % VOWELS.len()];
            f1.tune(a, 6.0, rate);
            f2.tune(b, 8.0, rate);
            // A consonant on about half: "s" or "sh" before the vowel.
            let consonant = if rng.next() < 0.5 {
                (len as f32 * (0.25 + rng.next() * 0.25)) as usize
            } else {
                0
            };
            if rng.next() < 0.5 {
                hiss.tune(5200.0, 1.2, rate);
            } else {
                hiss.tune(2600.0, 1.5, rate);
            }
            let level = 0.6 + rng.next() * 0.4;
            for k in 0..len {
                let Some(s) = out.get_mut(i + k) else { break };
                let x = rng.signed();
                let p = k as f32 / len as f32;
                let env = (p * 6.0).min(1.0) * ((1.0 - p) * 3.0).min(1.0);
                let v = if k < consonant {
                    hiss.run(x) * 0.8
                } else {
                    f1.run(x) + 0.7 * f2.run(x)
                };
                *s += v * env * level;
            }
            i += len + (rng.next() * 0.05 * rate as f32) as usize;
        }
        i += ((0.35 + rng.next() * 0.9) * rate as f32) as usize;
    }
    master(&mut out, rate, 7000.0, 0.7);
    out
}

/// A low, muffled murmur: like voices through a wall, words you can't make
/// out, rising and falling slowly.
pub fn murmur_samples(seed: u64, rate: u32) -> Vec<f32> {
    let n = (tuning().seconds * rate as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut rng = Noise(seed | 1);
    let (mut f1, mut f2) = (BandPass::default(), BandPass::default());
    let mut syllable = 0usize;
    let mut level = 0.0f32;
    let mut target = 0.0f32;
    for i in 0..n {
        if i >= syllable {
            let (a, b) = VOWELS[(rng.next() * VOWELS.len() as f32) as usize % VOWELS.len()];
            f1.tune(a * 0.7, 4.0, rate);
            f2.tune(b * 0.5, 4.0, rate);
            target = if rng.next() < 0.2 {
                0.0
            } else {
                0.4 + rng.next() * 0.6
            };
            syllable = i + ((0.15 + rng.next() * 0.25) * rate as f32) as usize;
        }
        level += (target - level) * 0.002;
        let t = i as f32 / n as f32;
        // Swells in and dies away.
        let swell = (t * 3.0).min(1.0) * ((1.0 - t) * 2.5).min(1.0);
        let x = rng.signed();
        out.push((f1.run(x) + 0.5 * f2.run(x)) * level * swell);
    }
    master(&mut out, rate, 900.0, 0.8);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_is_late_evening_and_small_hours() {
        assert!(is_night(22) && is_night(23) && is_night(0) && is_night(4));
        assert!(!is_night(5) && !is_night(12) && !is_night(21));
    }

    #[test]
    fn evening_and_after_midnight_are_the_same_night() {
        let day = 20_000 * 86_400i64;
        let eleven_pm = day + 23 * 3600;
        let two_am = day + 86_400 + 2 * 3600;
        let next_eleven_pm = eleven_pm + 86_400;
        assert_eq!(night_of(eleven_pm), night_of(two_am));
        assert_ne!(night_of(eleven_pm), night_of(next_eleven_pm));
        assert_eq!(hour_of(two_am), 2);
    }

    #[test]
    fn chance_is_low() {
        assert!(whispers_tonight(0.0));
        assert!(!whispers_tonight(0.5));
    }

    #[test]
    fn voices_are_behind_you() {
        assert!(VOICES.iter().all(|(o, _)| o.z > 0.0));
    }

    #[test]
    fn whispers_are_finite_quiet_and_have_gaps() {
        for samples in [whisper_samples(7, RATE), murmur_samples(7, RATE)] {
            assert_eq!(samples.len(), (tuning().seconds * RATE as f32) as usize);
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
            assert!(samples.iter().any(|s| s.abs() > 0.1));
        }
        // Whispers come in phrases: some 50 ms window is near silent.
        let w = whisper_samples(3, RATE);
        let win = RATE as usize / 20;
        assert!(
            w.chunks(win)
                .any(|c| c.iter().map(|s| s.abs()).sum::<f32>() / (win as f32) < 0.005)
        );
    }
}
