//! The shared echo send: its time a tempo-synced division, straight
//! (each side repeats itself) or ping-pong (the repeats bounce across).
//! Settings come from `mix.ron`'s `echo`, which a song may override.

use crate::dsp::{Biquad, Eq, Line};
use crate::synth::RATE;

const FS: f32 = RATE as f32;
/// The longest echo, seconds (the line holds a little more).
const MAX_S: f32 = 2.0;

/// A note length the echo repeats at.
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq, Default)]
pub enum Division {
    Sixteenth,
    TripletEighth,
    Eighth,
    TripletQuarter,
    #[default]
    DottedEighth,
    Quarter,
    DottedQuarter,
    Half,
}

impl Division {
    /// Length in beats (quarter notes).
    pub fn beats(self) -> f32 {
        match self {
            Division::Sixteenth => 0.25,
            Division::TripletEighth => 1.0 / 3.0,
            Division::Eighth => 0.5,
            Division::TripletQuarter => 2.0 / 3.0,
            Division::DottedEighth => 0.75,
            Division::Quarter => 1.0,
            Division::DottedQuarter => 1.5,
            Division::Half => 2.0,
        }
    }

    /// Echo time in seconds at `bpm` (capped at two seconds).
    pub fn seconds(self, bpm: f32) -> f32 {
        (self.beats() * 60.0 / bpm.max(30.0)).min(MAX_S)
    }
}

/// The echo's settings.
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq)]
#[serde(default)]
pub struct EchoMix {
    /// Repeat time, as a note length at the song's tempo.
    pub division: Division,
    /// Repeats bounce left-right (true) or each side repeats itself.
    pub pingpong: bool,
    /// How much of each repeat comes back (0..0.95).
    pub feedback: f32,
    /// Brightness of the repeats (0 dark .. 1 untouched).
    pub tone: f32,
}

impl Default for EchoMix {
    fn default() -> Self {
        Self {
            division: Division::DottedEighth,
            pingpong: true,
            feedback: 0.42,
            tone: 0.45,
        }
    }
}

pub struct SyncEcho {
    l: Line,
    r: Line,
    time: f32,
    target: f32,
    set: EchoMix,
    damp_l: f32,
    damp_r: f32,
    hp: Biquad,
}

impl SyncEcho {
    pub fn new() -> Self {
        let n = ((MAX_S + 0.1) * FS) as usize;
        Self {
            l: Line::new(n),
            r: Line::new(n),
            time: 0.4 * FS,
            target: 0.4 * FS,
            set: EchoMix::default(),
            damp_l: 0.0,
            damp_r: 0.0,
            hp: Biquad::new(Eq::Hp(350.0, 0.7)),
        }
    }

    /// Use `set` at `bpm` (the time glides there).
    pub fn configure(&mut self, set: EchoMix, bpm: f32) {
        self.set = set;
        self.target = set.division.seconds(bpm) * FS;
    }

    /// The time it's heading for, in samples.
    pub fn target(&self) -> f32 {
        self.target
    }

    /// Add the echoes of mono `send` into `l`/`r`.
    pub fn process(&mut self, send: &[f32], l: &mut [f32], r: &mut [f32]) {
        let fb = self.set.feedback.clamp(0.0, 0.95);
        let tone = self.set.tone.clamp(0.02, 1.0);
        let pp = self.set.pingpong;
        for ((s, xl), xr) in send.iter().zip(l.iter_mut()).zip(r.iter_mut()) {
            self.time += (self.target - self.time) * 0.0005;
            let a = self.l.read(self.time);
            let b = self.r.read(self.time);
            self.damp_l += tone * (a - self.damp_l);
            self.damp_r += tone * (b - self.damp_r);
            let input = self.hp.tick(*s);
            if pp {
                // Left feeds right and right feeds left.
                self.l.write(input + self.damp_r * fb);
                self.r.write(self.damp_l * fb);
            } else {
                self.l.write(input + self.damp_l * fb);
                self.r.write(input + self.damp_r * fb);
            }
            *xl += a * 0.55;
            *xr += b * 0.55;
        }
    }
}

impl Default for SyncEcho {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synced_time_matches_tempo() {
        assert!((Division::Quarter.seconds(120.0) - 0.5).abs() < 1e-6);
        assert!((Division::DottedEighth.seconds(100.0) - 0.45).abs() < 1e-6);
        assert!((Division::TripletEighth.seconds(174.0) - 60.0 / 174.0 / 3.0).abs() < 1e-6);
        let mut e = SyncEcho::new();
        let set = EchoMix {
            division: Division::Eighth,
            ..EchoMix::default()
        };
        e.configure(set, 174.0);
        assert!((e.target() - 0.5 * 60.0 / 174.0 * FS).abs() < 0.5);
        assert!(Division::Half.seconds(30.0) <= MAX_S);
    }

    /// Where the first few repeats of a click land, per side.
    fn repeats(pingpong: bool) -> Vec<(bool, bool)> {
        let mut e = SyncEcho::new();
        let set = EchoMix {
            division: Division::Quarter,
            pingpong,
            feedback: 0.6,
            tone: 1.0,
        };
        e.configure(set, 120.0);
        e.time = e.target;
        let step = e.target as usize;
        let n = step * 4 + 100;
        let mut send = vec![0.0; n];
        send[0] = 1.0;
        let (mut l, mut r) = (vec![0.0; n], vec![0.0; n]);
        e.process(&send, &mut l, &mut r);
        (1..=3)
            .map(|k| {
                let w = k * step - 20..k * step + 40;
                let e = |x: &[f32]| x[w.clone()].iter().map(|v| v.abs()).sum::<f32>();
                (e(&l) > 0.01, e(&r) > 0.01)
            })
            .collect()
    }

    #[test]
    fn pingpong_alternates() {
        assert_eq!(
            repeats(true),
            vec![(true, false), (false, true), (true, false)]
        );
        assert_eq!(repeats(false), vec![(true, true); 3]);
    }
}
