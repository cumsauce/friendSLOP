//! Bus compressor: feed-forward, stereo-linked RMS detector, gain smoothed in
//! dB with separate attack and release, and a wet/dry mix for parallel
//! ("New York") compression.

use crate::synth::RATE;

pub struct Comp {
    thresh: f32,
    /// 1 - 1/ratio: how much of the overshoot is taken off.
    slope: f32,
    att: f32,
    rel: f32,
    /// RMS window coefficient (about 5 ms).
    win: f32,
    mix: f32,
    /// Smoothed mean square of the input.
    ms: f32,
    /// Current gain reduction in dB (>= 0).
    gr: f32,
}

fn coef(ms: f32) -> f32 {
    1.0 - (-1.0 / (ms.max(0.05) * 0.001 * RATE as f32)).exp()
}

impl Comp {
    pub fn new(threshold_db: f32, ratio: f32, attack_ms: f32, release_ms: f32, mix: f32) -> Self {
        Self {
            thresh: threshold_db,
            slope: 1.0 - 1.0 / ratio.max(1.0),
            att: coef(attack_ms),
            rel: coef(release_ms),
            win: coef(5.0),
            mix: mix.clamp(0.0, 1.0),
            ms: 0.0,
            gr: 0.0,
        }
    }

    /// New settings (automation), keeping the level and gain reduction.
    pub fn set(
        &mut self,
        threshold_db: f32,
        ratio: f32,
        attack_ms: f32,
        release_ms: f32,
        mix: f32,
    ) {
        let n = Self::new(threshold_db, ratio, attack_ms, release_ms, mix);
        (self.thresh, self.slope, self.att, self.rel, self.mix) =
            (n.thresh, n.slope, n.att, n.rel, n.mix);
    }

    /// Gain reduction right now, in dB.
    pub fn reduction_db(&self) -> f32 {
        self.gr
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            let p = (*x * *x).max(*y * *y);
            self.ms += self.win * (p - self.ms);
            // 10·log10 of the mean square is the RMS level in dB.
            let level = 10.0 * (self.ms + 1e-12).log10();
            let want = (level - self.thresh).max(0.0) * self.slope;
            let k = if want > self.gr { self.att } else { self.rel };
            self.gr += k * (want - self.gr);
            let g = 10f32.powf(-self.gr / 20.0);
            let w = 1.0 - self.mix + self.mix * g;
            *x *= w;
            *y *= w;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(amp: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| amp * (i as f32 * 220.0 * std::f32::consts::TAU / RATE as f32).sin())
            .collect()
    }

    fn rms_db(x: &[f32]) -> f32 {
        10.0 * (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).log10()
    }

    #[test]
    fn reduces_by_ratio_above_threshold() {
        // A 0 dBFS-peak sine is -3 dB RMS: 17 dB over -20, 4:1 leaves 4.25.
        let mut c = Comp::new(-20.0, 4.0, 5.0, 100.0, 1.0);
        let n = RATE as usize;
        let mut l = sine(1.0, n);
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let out = rms_db(&l[n / 2..]);
        assert!((out - (-20.0 + 17.0 / 4.0)).abs() < 1.0, "{out}");
        // Under the threshold it leaves the signal alone.
        let mut c = Comp::new(-20.0, 4.0, 5.0, 100.0, 1.0);
        let mut l = sine(0.05, n);
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        assert!(c.reduction_db() < 0.01);
    }

    #[test]
    fn recovers_with_release() {
        let mut c = Comp::new(-20.0, 8.0, 1.0, 50.0, 1.0);
        let mut l = sine(1.0, RATE as usize / 4);
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let squashed = c.reduction_db();
        assert!(squashed > 10.0, "{squashed}");
        // 50 ms of silence: about one time constant (allow for the detector).
        let mut l = vec![0.0; RATE as usize / 20];
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        let mid = c.reduction_db();
        assert!(mid < squashed * 0.6 && mid > 0.5, "{mid}");
        let mut l = vec![0.0; RATE as usize / 2];
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        assert!(c.reduction_db() < 0.05);
    }

    #[test]
    fn parallel_mix_keeps_dry() {
        let mut c = Comp::new(-40.0, 20.0, 1.0, 100.0, 0.5);
        let mut l = sine(1.0, RATE as usize / 2);
        let mut r = l.clone();
        c.process(&mut l, &mut r);
        // Half dry: never quieter than -6 dB of the input.
        let out = rms_db(&l[RATE as usize / 4..]);
        assert!(out > -3.0 - 6.1, "{out}");
    }
}
