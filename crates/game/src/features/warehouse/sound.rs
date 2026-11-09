//! The warehouse's sounds, all generated (#130): the low drone of the dark,
//! the monster's breathing, its scream, and a soft chime when Jegg takes a
//! soul piece.

use std::f32::consts::TAU;

pub const RATE: u32 = 22_050;

struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2.0 - 1.0
    }
}

/// Crossfade the last `tail` samples into the start, so it loops seamlessly.
fn make_loop(mut v: Vec<f32>, tail: usize) -> Vec<f32> {
    let n = v.len() - tail;
    for i in 0..tail {
        let g = i as f32 / tail as f32;
        v[i] = v[i] * g + v[n + i] * (1.0 - g);
    }
    v.truncate(n);
    v
}

fn normalise(v: &mut [f32], peak: f32) {
    let top = v.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-5);
    v.iter_mut()
        .for_each(|s| *s = (*s / top * peak).clamp(-1.0, 1.0));
}

/// The dark: a deep, beating hum (two detuned low tones), a rumble of
/// filtered noise, and now and then a long metal groan. Loops.
pub fn drone(rate: u32, secs: f32) -> Vec<f32> {
    let r = rate as f32;
    let tail = (1.5 * r) as usize;
    let n = (secs * r) as usize + tail;
    let mut out = vec![0.0f32; n];
    let mut noise = Noise(0x0ddb_10b0);
    let (mut lp1, mut lp2) = (0.0f32, 0.0f32);
    let a = 1.0 - (-TAU * 90.0 / r).exp();
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / r;
        let swell = 0.7 + 0.3 * (t * TAU / 9.0).sin();
        let hum =
            (t * TAU * 41.0).sin() + (t * TAU * 41.7).sin() * 0.8 + (t * TAU * 61.5).sin() * 0.25;
        lp1 += a * (noise.next() - lp1);
        lp2 += a * (lp1 - lp2);
        *s = hum * 0.3 * swell + lp2 * 3.0;
    }
    // Groans: a slowly bending, rough metallic tone.
    let mut noise = Noise(0x009a_0a11);
    for k in 0..3 {
        let start = (secs * (0.1 + 0.3 * k as f32)) * r;
        let len = 2.5 + k as f32 * 0.7;
        let base = 180.0 + 70.0 * k as f32;
        let mut phase = 0.0f32;
        for j in 0..(len * r) as usize {
            let Some(slot) = out.get_mut(start as usize + j) else {
                break;
            };
            let t = j as f32 / r;
            let env = (t / 0.6).min(1.0) * (1.0 - t / len).max(0.0);
            let f = base * (1.0 - 0.25 * t / len) * (1.0 + 0.01 * (t * TAU * 7.0).sin());
            phase = (phase + f / r).fract();
            let tone = (phase * TAU).sin() + 0.4 * (phase * TAU * 2.76).sin();
            *slot += tone * env * 0.12 * (1.0 + 0.6 * noise.next());
        }
    }
    let mut out = make_loop(out, tail);
    normalise(&mut out, 0.6);
    out
}

/// Slow, wet, ragged breathing, in and out. Loops.
pub fn breathing(rate: u32) -> Vec<f32> {
    let r = rate as f32;
    let secs = 3.2;
    let n = (secs * r) as usize;
    let mut out = vec![0.0f32; n];
    let mut noise = Noise(0x00b4_ea7e);
    let (mut lp, mut bp) = (0.0f32, 0.0f32);
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / r;
        // In (0..1.2 s), out (1.4..2.8 s).
        let env = |a: f32, b: f32| {
            if t < a || t > b {
                0.0
            } else {
                ((t - a) / (b - a) * std::f32::consts::PI)
                    .sin()
                    .max(0.0)
                    .powf(1.5)
            }
        };
        let breath = env(0.0, 1.2) * 0.6 + env(1.4, 2.8);
        let rasp = 0.6 + 0.4 * (t * TAU * 23.0).sin().abs();
        let x = noise.next();
        lp += 0.12 * (x - lp);
        bp += 0.4 * (lp - bp);
        *s = (lp - bp * 0.5) * breath * rasp;
    }
    normalise(&mut out, 0.7);
    out
}

/// A scream: a high, wavering, distorted cry over a noise blast. For when it
/// gets someone, or springs from a lure.
pub fn scream(rate: u32, seed: u32) -> Vec<f32> {
    let r = rate as f32;
    let secs = 1.6;
    let n = (secs * r) as usize;
    let mut out = vec![0.0f32; n];
    let mut noise = Noise(seed | 1);
    let pitch = 520.0 + (seed % 7) as f32 * 40.0;
    let (mut p1, mut p2) = (0.0f32, 0.3f32);
    for (i, s) in out.iter_mut().enumerate() {
        let t = i as f32 / r;
        let env = (t / 0.02).min(1.0) * (1.0 - t / secs).max(0.0).powf(0.7);
        let bend = 1.0 - 0.5 * (t / secs) + 0.04 * (t * TAU * 11.0).sin();
        let f = pitch * bend;
        p1 = (p1 + f / r).fract();
        p2 = (p2 + f * 1.51 / r).fract();
        let tone = (p1 * TAU).sin() + 0.6 * (p2 * TAU).sin() + 0.3 * (p1 * TAU * 3.0).sin();
        let x = tone * 0.6 + noise.next() * 0.5;
        *s = (x * 3.0).tanh() * env;
    }
    normalise(&mut out, 0.85);
    out
}

/// A soft glassy chime (a soul piece going home).
pub fn chime(rate: u32) -> Vec<f32> {
    let r = rate as f32;
    let n = (2.0 * r) as usize;
    let mut out = vec![0.0f32; n];
    for (k, f) in [880.0f32, 1318.5, 1760.0, 2637.0].iter().enumerate() {
        let start = (k as f32 * 0.09 * r) as usize;
        for j in 0..n.saturating_sub(start) {
            let t = j as f32 / r;
            let env = (t / 0.005).min(1.0) * (-t * 2.4).exp();
            out[start + j] += (t * TAU * f).sin() * env / (1.0 + k as f32 * 0.5);
        }
    }
    normalise(&mut out, 0.5);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sane(v: &[f32]) {
        assert!(!v.is_empty());
        assert!(v.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        assert!(v.iter().any(|x| x.abs() > 0.3));
    }

    #[test]
    fn sounds_are_sane() {
        let d = drone(RATE, 10.0);
        assert_eq!(d.len(), 10 * RATE as usize);
        sane(&d);
        sane(&breathing(RATE));
        sane(&scream(RATE, 3));
        sane(&chime(RATE));
    }
}
