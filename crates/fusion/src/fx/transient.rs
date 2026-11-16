//! A transient shaper: `(attack, sustain)`, each about -1..1. Positive
//! attack sharpens hits (snare snap), positive sustain lifts the tails.
//! Two pairs of envelope followers on the stereo level: a fast and a slow
//! attack spot onsets, a fast and a slow release spot tails.

use crate::synth::RATE;

fn coef(ms: f32) -> f32 {
    1.0 - (-1000.0 / (ms * RATE as f32)).exp()
}

struct Follow {
    env: f32,
    up: f32,
    down: f32,
}

impl Follow {
    fn new(up_ms: f32, down_ms: f32) -> Self {
        Self {
            env: 0.0,
            up: coef(up_ms),
            down: coef(down_ms),
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let k = if x > self.env { self.up } else { self.down };
        self.env += (x - self.env) * k;
        self.env
    }
}

pub struct Transient {
    attack: f32,
    sustain: f32,
    a_fast: Follow,
    a_slow: Follow,
    s_fast: Follow,
    s_slow: Follow,
}

impl Transient {
    pub fn new(attack: f32, sustain: f32) -> Self {
        Self {
            attack: attack.clamp(-2.0, 2.0),
            sustain: sustain.clamp(-2.0, 2.0),
            a_fast: Follow::new(0.5, 80.0),
            a_slow: Follow::new(15.0, 80.0),
            s_fast: Follow::new(0.5, 25.0),
            s_slow: Follow::new(0.5, 250.0),
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        const EPS: f32 = 1e-5;
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            let lvl = x.abs().max(y.abs());
            let onset = (self.a_fast.tick(lvl) + EPS) / (self.a_slow.tick(lvl) + EPS);
            let tail = (self.s_slow.tick(lvl) + EPS) / (self.s_fast.tick(lvl) + EPS);
            let g = (self.attack * onset.ln() + self.sustain * tail.ln())
                .clamp(-1.4, 1.4)
                .exp();
            *x *= g;
            *y *= g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hit: instant onset, exponential decay.
    fn hit() -> Vec<f32> {
        (0..4000)
            .map(|i| (i as f32 * 0.3).sin() * (-(i as f32) / 600.0).exp())
            .collect()
    }

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0, |m, v| m.max(v.abs()))
    }

    #[test]
    fn shaper_boosts_attacks() {
        let dry = hit();
        let (mut l, mut r) = (dry.clone(), dry.clone());
        Transient::new(1.0, 0.0).process(&mut l, &mut r);
        // The first few ms come up more than the tail.
        let head = peak(&l[..200]) / peak(&dry[..200]);
        let tail = peak(&l[3000..]) / peak(&dry[3000..]);
        assert!(head > 1.3 && head > 1.2 * tail, "{head} {tail}");
        let (mut l, mut r) = (dry.clone(), dry.clone());
        Transient::new(0.0, 1.0).process(&mut l, &mut r);
        assert!(peak(&l[2000..]) > 1.2 * peak(&dry[2000..]));
    }
}
