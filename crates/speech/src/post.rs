//! What a place does to a voice after it's made: a telephone line, a station loudspeaker.

use std::f32::consts::TAU;

/// A second-order filter (RBJ cookbook, Q 0.707).
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    x: [f32; 2],
    y: [f32; 2],
}

impl Biquad {
    fn new(low: bool, hz: f32, rate: f32) -> Self {
        let w = TAU * (hz / rate).min(0.45);
        let (cos, alpha) = (w.cos(), w.sin() / (2.0 * 0.707));
        let a0 = 1.0 + alpha;
        let b = if low {
            [(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0]
        } else {
            [(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0]
        };
        Self {
            b: b.map(|v| v / a0),
            a: [-2.0 * cos / a0, (1.0 - alpha) / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [x, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

fn normalize(samples: &mut [f32]) {
    let peak = samples
        .iter()
        .fold(f32::MIN_POSITIVE, |p, s| p.max(s.abs()));
    for s in samples {
        *s *= 0.9 / peak;
    }
}

/// A telephone: 300 to 3400 Hz, pushed into a soft clip, over a faint hiss.
pub fn phone(samples: &mut [f32], rate: f32) {
    let (mut hp, mut lp) = (
        Biquad::new(false, 300.0, rate),
        Biquad::new(true, 3400.0, rate),
    );
    let mut seed = 0x9e37_79b9u32;
    for s in samples.iter_mut() {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let hiss = (seed as f32 / u32::MAX as f32 * 2.0 - 1.0) * 0.01;
        *s = (lp.run(hp.run(*s)) * 2.2).tanh() + hiss;
    }
    normalize(samples);
}

/// A loudspeaker: thin lows, dulled highs, a little overdrive.
pub fn pa(samples: &mut [f32], rate: f32) {
    let (mut hp, mut lp) = (
        Biquad::new(false, 200.0, rate),
        Biquad::new(true, 6500.0, rate),
    );
    for s in samples.iter_mut() {
        *s = (lp.run(hp.run(*s)) * 1.4).tanh();
    }
    normalize(samples);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, rate: f32) -> Vec<f32> {
        (0..4000)
            .map(|i| (TAU * hz * i as f32 / rate).sin())
            .collect()
    }

    fn energy(x: &[f32]) -> f32 {
        x[1000..].iter().map(|s| s * s).sum()
    }

    #[test]
    fn the_phone_drops_lows_and_highs() {
        let rate = 22_050.0;
        let (mut low, mut mid, mut high) =
            (tone(60.0, rate), tone(1000.0, rate), tone(8000.0, rate));
        let before = energy(&mid);
        for t in [&mut low, &mut mid, &mut high] {
            for s in t.iter_mut() {
                *s *= 0.4;
            }
        }
        let (mut a, mut b, mut c) = (low.clone(), mid.clone(), high.clone());
        // Compare without the final normalize by filtering the same way.
        let (mut hp, mut lp) = (
            Biquad::new(false, 300.0, rate),
            Biquad::new(true, 3400.0, rate),
        );
        for s in a.iter_mut() {
            *s = lp.run(hp.run(*s));
        }
        let (mut hp, mut lp) = (
            Biquad::new(false, 300.0, rate),
            Biquad::new(true, 3400.0, rate),
        );
        for s in b.iter_mut() {
            *s = lp.run(hp.run(*s));
        }
        let (mut hp, mut lp) = (
            Biquad::new(false, 300.0, rate),
            Biquad::new(true, 3400.0, rate),
        );
        for s in c.iter_mut() {
            *s = lp.run(hp.run(*s));
        }
        assert!(energy(&a) < energy(&b) * 0.05 && energy(&c) < energy(&b) * 0.05);
        let _ = before;
    }

    #[test]
    fn effects_stay_bounded() {
        let mut x = tone(500.0, 22_050.0);
        phone(&mut x, 22_050.0);
        assert!(x.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        let mut y = tone(500.0, 22_050.0);
        pa(&mut y, 22_050.0);
        assert!(y.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}
