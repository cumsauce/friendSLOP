//! Saturation: a soft clipper run at twice the sample rate, level-matched,
//! with a low-pass "tone" after it (tape-style warmth, a growl on a Reese).
//! A slow loudness follower keeps the output as loud as the input (within
//! ±6 dB of a fixed match at [`REF`]), so turning it up adds grit, not level.

use crate::synth::RATE;

/// The level the drive is matched at: a bus playing about this loud comes out
/// about as loud as it went in, just thicker.
const REF: f32 = 0.25;

/// A rational tanh, exact at 0 and clamped to ±1 beyond |x| = 3.
fn sat(x: f32) -> f32 {
    let x = x.clamp(-3.0, 3.0);
    x * (27.0 + x * x) / (27.0 + 9.0 * x * x)
}

/// One channel's state: the last input (to interpolate the half sample) and
/// the tone filter.
#[derive(Default, Clone, Copy)]
struct Chan {
    last: f32,
    lp: f32,
}

impl Chan {
    fn tick(&mut self, x: f32, pre: f32, post: f32, a: f32) -> f32 {
        // 2x: shape the midpoint and the sample, average them back down
        // (a gentle half-band) so the harmonics fold back less.
        let mid = sat((self.last + x) * 0.5 * pre);
        let now = sat(x * pre);
        self.last = x;
        let y = (mid + now) * 0.5 * post;
        self.lp += a * (y - self.lp);
        self.lp
    }
}

/// Stereo drive: `amount` 0..1 (0 is nearly clean, 1 is about 18 dB into the
/// clipper), `tone` 0..1 (the low-pass after it: 0 dark ~1.5 kHz, 1 open ~18 kHz).
pub struct Drive {
    pre: f32,
    post: f32,
    a: f32,
    ch: [Chan; 2],
    /// Slow mean squares of the input and the shaped output.
    ms_in: f32,
    ms_out: f32,
    follow: f32,
}

/// The auto-trim's range either way (about 6 dB).
const TRIM: f32 = 2.0;

impl Drive {
    pub fn new(amount: f32, tone: f32) -> Self {
        let pre = 1.0 + 7.0 * amount.clamp(0.0, 1.0);
        let post = REF / sat(REF * pre);
        let hz = 1500.0 * 12f32.powf(tone.clamp(0.0, 1.0));
        let a = 1.0 - (-std::f32::consts::TAU * hz / RATE as f32).exp();
        Self {
            pre,
            post,
            a,
            ch: [Chan::default(); 2],
            ms_in: 0.0,
            ms_out: 0.0,
            follow: 1.0 - (-1.0 / (0.3 * RATE as f32)).exp(),
        }
    }

    /// New settings (automation), keeping the filters and level followers.
    pub fn set(&mut self, amount: f32, tone: f32) {
        let n = Self::new(amount, tone);
        (self.pre, self.post, self.a) = (n.pre, n.post, n.a);
    }

    /// The most this ever puts out, whatever goes in.
    pub fn ceiling(&self) -> f32 {
        self.post * TRIM
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let (pre, post, a) = (self.pre, self.post, self.a);
        let [cl, cr] = &mut self.ch;
        let k = self.follow;
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            self.ms_in += k * (*x * *x + *y * *y - self.ms_in);
            let (sx, sy) = (cl.tick(*x, pre, post, a), cr.tick(*y, pre, post, a));
            self.ms_out += k * (sx * sx + sy * sy - self.ms_out);
            let g = ((self.ms_in + 1e-9) / (self.ms_out + 1e-9))
                .sqrt()
                .clamp(1.0 / TRIM, TRIM);
            *x = sx * g;
            *y = sy * g;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms_of_sine(d: &mut Drive, amp: f32) -> (f32, f32) {
        let n = RATE as usize / 2;
        let mut l: Vec<f32> = (0..n)
            .map(|i| amp * (i as f32 * 110.0 * std::f32::consts::TAU / RATE as f32).sin())
            .collect();
        let mut r = l.clone();
        let rin = (l.iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
        d.process(&mut l, &mut r);
        let tail = &l[n / 4..];
        let rout = (tail.iter().map(|x| x * x).sum::<f32>() / tail.len() as f32).sqrt();
        (rin, rout)
    }

    #[test]
    fn level_compensated() {
        for (amount, amp) in [(0.0, REF), (0.3, 0.1), (0.7, REF), (1.0, 0.6)] {
            let mut d = Drive::new(amount, 1.0);
            let (i, o) = rms_of_sine(&mut d, amp);
            let db = 20.0 * (o / i).log10();
            assert!(db.abs() < 1.5, "amount {amount}: {db} dB");
        }
    }

    #[test]
    fn bounded() {
        let mut d = Drive::new(1.0, 0.5);
        let c = d.ceiling();
        let mut l: Vec<f32> = (0..4000)
            .map(|i| if i % 7 < 3 { 40.0 } else { -40.0 })
            .collect();
        let mut r = l.clone();
        d.process(&mut l, &mut r);
        assert!(l.iter().all(|x| x.is_finite() && x.abs() <= c + 1e-5));
    }
}
