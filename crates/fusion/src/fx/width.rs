//! Stereo width with mono lows: mid/side, with the side signal high-passed
//! (4th-order Linkwitz-Riley) so everything below the crossover comes out
//! identical in both channels. The mid is never filtered, so the bus's
//! mono sum keeps its phase.

use crate::synth::RATE;

/// One 2nd-order Butterworth high-pass (two in a row make the LR4 slope).
#[derive(Clone, Copy, Default)]
struct Hp2 {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Hp2 {
    fn new(hz: f32) -> Self {
        let w = std::f32::consts::TAU * hz.clamp(10.0, 0.45 * RATE as f32) / RATE as f32;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let a0 = 1.0 + alpha;
        let b0 = (1.0 + c) / 2.0 / a0;
        Self {
            b: [b0, -2.0 * b0, b0],
            a: [-2.0 * c / a0, (1.0 - alpha) / a0],
            z: [0.0; 2],
        }
    }

    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// `width: (width, mono_below_hz)`: 0 mono, 1 as is, up to 2 wide; a
/// `mono_below_hz` of 0 leaves the lows alone.
pub struct Width {
    /// The side gain (public so automation can move it; eased per sample).
    pub width: f32,
    w: f32,
    mono_below: bool,
    hp: [Hp2; 2],
}

impl Width {
    pub fn new(width: f32, mono_below_hz: f32) -> Self {
        let width = width.clamp(0.0, 2.0);
        let hp = Hp2::new(mono_below_hz.max(10.0));
        Self {
            width,
            w: width,
            mono_below: mono_below_hz > 0.0,
            hp: [hp; 2],
        }
    }

    /// A new width (automation; it glides there). The mono-below split
    /// stays as built.
    pub fn set(&mut self, width: f32, _mono_below_hz: f32) {
        self.width = width.clamp(0.0, 2.0);
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let target = self.width.clamp(0.0, 2.0);
        let k = 1.0 - (-1.0 / (0.02 * RATE as f32)).exp();
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            self.w += (target - self.w) * k;
            let m = 0.5 * (*x + *y);
            let mut s = 0.5 * (*x - *y);
            if self.mono_below {
                let s1 = self.hp[0].tick(s);
                s = self.hp[1].tick(s1);
            }
            s *= self.w;
            *x = m + s;
            *y = m - s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, n: usize, phase: f32) -> Vec<f32> {
        (0..n)
            .map(|i| (std::f32::consts::TAU * hz * i as f32 / RATE as f32 + phase).sin())
            .collect()
    }

    #[test]
    fn lows_are_mono() {
        let n = RATE as usize;
        // Wildly different channels at 40 Hz (a third of the crossover).
        let mut l = tone(40.0, n, 0.0);
        let mut r: Vec<f32> = tone(40.0, n, 2.0).iter().map(|x| -0.5 * x).collect();
        Width::new(1.5, 120.0).process(&mut l, &mut r);
        let tail = n / 2..n;
        let diff = tail
            .clone()
            .map(|i| (l[i] - r[i]).abs())
            .fold(0.0, f32::max);
        let level = tail.map(|i| l[i].abs()).fold(0.0, f32::max);
        assert!(diff < 0.03 * level, "diff {diff} level {level}");
    }

    #[test]
    fn highs_keep_their_width() {
        let n = RATE as usize / 2;
        let mut l = tone(8000.0, n, 0.0);
        let mut r = vec![0.0; n];
        Width::new(1.0, 120.0).process(&mut l, &mut r);
        let peak = |v: &[f32]| v[n / 2..].iter().fold(0.0f32, |a, x| a.max(x.abs()));
        assert!((peak(&l) - 1.0).abs() < 0.05 && peak(&r) < 0.05);
    }

    #[test]
    fn zero_width_is_mono() {
        let n = 4096;
        let mut l = tone(3000.0, n, 0.0);
        let mut r = tone(500.0, n, 1.0);
        Width::new(0.0, 0.0).process(&mut l, &mut r);
        assert!(l.iter().zip(&r).all(|(a, b)| a == b));
    }
}
