//! Bit and sample-rate crush: lo-fi, jungle grit. `(bits, rate_hz, mix)`.

use crate::synth::RATE;

pub struct Crush {
    step: f32,
    rate: f32,
    mix: f32,
    phase: f32,
    hold: (f32, f32),
}

impl Crush {
    /// New settings (automation), keeping the running state.
    pub fn set(&mut self, bits: f32, rate_hz: f32, mix: f32) {
        let n = Self::new(bits, rate_hz, mix);
        (self.step, self.rate, self.mix) = (n.step, n.rate, n.mix);
    }

    pub fn new(bits: f32, rate_hz: f32, mix: f32) -> Self {
        Self {
            step: 1.0 / 2f32.powf(bits.clamp(1.0, 24.0) - 1.0),
            rate: (rate_hz / RATE as f32).clamp(0.0001, 1.0),
            mix: mix.clamp(0.0, 1.0),
            phase: 1.0,
            hold: (0.0, 0.0),
        }
    }

    /// `x` rounded to the nearest of the crusher's levels.
    pub fn quantise(&self, x: f32) -> f32 {
        (x / self.step).round() * self.step
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            self.phase += self.rate;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
                self.hold = (self.quantise(*x), self.quantise(*y));
            }
            *x += (self.hold.0 - *x) * self.mix;
            *y += (self.hold.1 - *y) * self.mix;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crush_quantises() {
        let mut c = Crush::new(3.0, RATE as f32, 1.0);
        let mut l: Vec<f32> = (0..200).map(|i| (i as f32 * 0.05).sin() * 0.9).collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        // 3 bits: steps of a quarter.
        for v in &l {
            assert!((v * 4.0 - (v * 4.0).round()).abs() < 1e-5, "{v}");
        }
        // Rate: a quarter of the rate holds each value four samples.
        let mut c = Crush::new(16.0, RATE as f32 / 4.0, 1.0);
        let mut l: Vec<f32> = (0..64).map(|i| i as f32 / 64.0).collect();
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let distinct = l.windows(2).filter(|w| w[0] != w[1]).count();
        assert!(distinct <= 16, "{distinct}");
    }
}
