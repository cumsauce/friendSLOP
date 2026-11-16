//! A resonant bus filter (low- or high-pass): a zero-delay-feedback state
//! variable filter whose cutoff and Q are eased per sample, so automation
//! can sweep them (the breakdown filtered, opening on the drop) without
//! zipper noise.

use crate::synth::RATE;

#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    Lp,
    Hp,
}

/// `filter: (kind, cutoff_hz, q)`.
pub struct BusFilter {
    pub kind: FilterKind,
    /// Targets (public so automation can move them); eased per sample.
    pub cutoff: f32,
    pub q: f32,
    /// Eased log2 of the cutoff, and the eased Q.
    lfc: f32,
    qq: f32,
    s: [[f32; 2]; 2],
}

impl BusFilter {
    pub fn new(kind: FilterKind, cutoff: f32, q: f32) -> Self {
        Self {
            kind,
            cutoff,
            q,
            lfc: clamp_hz(cutoff).log2(),
            qq: q.clamp(0.5, 12.0),
            s: [[0.0; 2]; 2],
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let target = clamp_hz(self.cutoff).log2();
        let tq = self.q.clamp(0.5, 12.0);
        // ~10 ms glide.
        let k = 1.0 - (-1.0 / (0.01 * RATE as f32)).exp();
        let hp = self.kind == FilterKind::Hp;
        for (x, y) in l.iter_mut().zip(r.iter_mut()) {
            self.lfc += (target - self.lfc) * k;
            self.qq += (tq - self.qq) * k;
            let g = (std::f32::consts::PI * self.lfc.exp2() / RATE as f32).tan();
            let kk = 1.0 / self.qq;
            let a1 = 1.0 / (1.0 + g * (g + kk));
            for (v, s) in [&mut *x, &mut *y].into_iter().zip(self.s.iter_mut()) {
                let v3 = *v - s[1];
                let v1 = a1 * s[0] + g * a1 * v3;
                let v2 = s[1] + g * v1;
                s[0] = 2.0 * v1 - s[0];
                s[1] = 2.0 * v2 - s[1];
                *v = if hp { *v - kk * v1 - v2 } else { v2 };
            }
        }
    }
}

fn clamp_hz(hz: f32) -> f32 {
    hz.clamp(20.0, 0.45 * RATE as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms_through(kind: FilterKind, cutoff: f32, hz: f32) -> f32 {
        let n = RATE as usize / 2;
        let mut l: Vec<f32> = (0..n)
            .map(|i| (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin())
            .collect();
        let mut r = l.clone();
        BusFilter::new(kind, cutoff, 0.707).process(&mut l, &mut r);
        (l[n / 2..].iter().map(|x| x * x).sum::<f32>() / (n / 2) as f32).sqrt()
    }

    #[test]
    fn lp_and_hp_cut() {
        let pass = std::f32::consts::FRAC_1_SQRT_2;
        assert!(rms_through(FilterKind::Lp, 500.0, 5000.0) < 0.02);
        assert!((rms_through(FilterKind::Lp, 5000.0, 200.0) - pass).abs() < 0.05);
        assert!(rms_through(FilterKind::Hp, 5000.0, 300.0) < 0.02);
        assert!((rms_through(FilterKind::Hp, 300.0, 5000.0) - pass).abs() < 0.05);
    }

    #[test]
    fn sweep_stays_finite() {
        let mut f = BusFilter::new(FilterKind::Lp, 100.0, 8.0);
        let mut noise = crate::dsp::Noise(1);
        for block in 0..400 {
            // Jump the cutoff around every block, resonance high.
            f.cutoff = if block % 2 == 0 { 20_000.0 } else { 30.0 };
            let mut l: Vec<f32> = (0..64).map(|_| noise.sample()).collect();
            let mut r = l.clone();
            f.process(&mut l, &mut r);
            assert!(l.iter().chain(&r).all(|x| x.is_finite() && x.abs() < 50.0));
        }
    }
}
