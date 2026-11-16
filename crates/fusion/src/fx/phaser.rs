//! A phaser for pads: `(rate_hz, depth, feedback, mix)`. Four allpass
//! stages per side, swept by an LFO (the sides a quarter turn apart), with
//! feedback around them. Depth 0..1 sweeps 250 Hz up to about 4 kHz.

use std::f32::consts::{PI, TAU};

use crate::synth::RATE;

const FS: f32 = RATE as f32;
const STAGES: usize = 4;

#[derive(Default, Clone, Copy)]
struct Side {
    z: [f32; STAGES],
    fb: f32,
}

impl Side {
    #[inline]
    fn tick(&mut self, x: f32, a: f32, feedback: f32) -> f32 {
        let mut v = x + self.fb * feedback;
        for z in &mut self.z {
            // First-order allpass: y = a*v + z; z = v - a*y.
            let y = a * v + *z;
            *z = v - a * y;
            v = y;
        }
        self.fb = v.clamp(-4.0, 4.0);
        v
    }
}

pub struct Phaser {
    rate: f32,
    depth: f32,
    feedback: f32,
    mix: f32,
    phase: f32,
    l: Side,
    r: Side,
}

fn coef(f: f32) -> f32 {
    let t = (PI * f.clamp(20.0, 0.45 * FS) / FS).tan();
    (t - 1.0) / (t + 1.0)
}

impl Phaser {
    pub fn new(rate_hz: f32, depth: f32, feedback: f32, mix: f32) -> Self {
        Self {
            rate: rate_hz.max(0.0) / FS,
            depth: depth.clamp(0.0, 1.0),
            feedback: feedback.clamp(-0.9, 0.9),
            mix: mix.clamp(0.0, 1.0),
            phase: 0.0,
            l: Side::default(),
            r: Side::default(),
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let span = self.depth * 4.0;
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            self.phase = (self.phase + self.rate).fract();
            let sweep = |p: f32| 0.5 + 0.5 * (TAU * p).sin();
            let al = coef(250.0 * (span * sweep(self.phase)).exp2());
            let ar = coef(250.0 * (span * sweep(self.phase + 0.25)).exp2());
            let wl = self.l.tick(*x, al, self.feedback);
            let wr = self.r.tick(*y, ar, self.feedback);
            // Half dry plus half wet makes the notches; mix blends that in.
            *x += (0.5 * (*x + wl) - *x) * self.mix;
            *y += (0.5 * (*y + wr) - *y) * self.mix;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phaser_stays_bounded() {
        let mut p = Phaser::new(0.3, 1.0, 0.9, 1.0);
        let mut l: Vec<f32> = (0..RATE as usize * 2)
            .map(|i| ((i * 7919) % 2000) as f32 / 1000.0 - 1.0)
            .collect();
        let mut r = l.clone();
        p.process(&mut l, &mut r);
        let peak = l.iter().chain(&r).fold(0.0f32, |m, v| m.max(v.abs()));
        assert!(peak.is_finite() && peak < 4.0, "{peak}");
        // And it changes the sound.
        let mut p = Phaser::new(0.3, 1.0, 0.5, 1.0);
        let dry: Vec<f32> = (0..4000).map(|i| (i as f32 * 0.07).sin()).collect();
        let (mut l, mut r) = (dry.clone(), dry.clone());
        p.process(&mut l, &mut r);
        assert!(l.iter().zip(&dry).any(|(a, b)| (a - b).abs() > 0.05));
    }
}
