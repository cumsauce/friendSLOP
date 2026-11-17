//! Head-related impulse responses (HRIRs): how the head and outer ears
//! filter a sound from a given direction, so a mono sound can be placed in
//! 3D on headphones, including above and below you.
//!
//! The set is a small baked file (`scripts/hrtf-bake.py` makes it from the
//! SADIE II database): minimum-phase responses (the arrival-time difference
//! between the ears is left out, for the caller to add as a delay) on a
//! regular azimuth/elevation grid. [`HrirSet::lookup`] blends the four
//! nearest nodes for any direction, and [`Fir`] runs the result over a
//! signal. No Bevy, no threads, no files: it builds for the browser.

use std::f32::consts::PI;

/// Taps per response. Fixed so a response is a plain array.
pub const TAPS: usize = 48;

/// One direction's filters, left and right ear.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hrir {
    pub ear: [[f32; TAPS]; 2],
}

impl Default for Hrir {
    fn default() -> Self {
        Self {
            ear: [[0.0; TAPS]; 2],
        }
    }
}

impl Hrir {
    /// A response that passes the sound through unchanged.
    pub fn passthrough() -> Self {
        let mut h = Self::default();
        h.ear[0][0] = 1.0;
        h.ear[1][0] = 1.0;
        h
    }

    /// Energy of one ear's response.
    pub fn energy(&self, ear: usize) -> f32 {
        self.ear[ear].iter().map(|x| x * x).sum()
    }
}

/// The grid of responses, resampled to one sample rate.
#[derive(Clone, Debug, PartialEq)]
pub struct HrirSet {
    rate: u32,
    az_n: usize,
    el_n: usize,
    az_step: f32,
    el_min: f32,
    el_step: f32,
    /// `[el][az]`.
    nodes: Vec<Hrir>,
}

const HEADER: usize = 4 + 5 * 4 + 3 * 4;

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn f32_at(b: &[u8], at: usize) -> Option<f32> {
    Some(f32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

impl HrirSet {
    /// Read a baked `.hrir` file; `None` when it isn't one (a Git LFS
    /// pointer, a truncated download).
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.get(..4)? != b"HRIR" || u32_at(bytes, 4)? != 1 {
            return None;
        }
        let rate = u32_at(bytes, 8)?;
        let taps = u32_at(bytes, 12)? as usize;
        let az_n = u32_at(bytes, 16)? as usize;
        let el_n = u32_at(bytes, 20)? as usize;
        let az_step = f32_at(bytes, 24)?;
        let el_min = f32_at(bytes, 28)?;
        let el_step = f32_at(bytes, 32)?;
        if taps != TAPS
            || rate < 8_000
            || az_n == 0
            || el_n == 0
            || az_n > 720
            || el_n > 360
            || !(az_step.is_finite() && el_min.is_finite() && el_step.is_finite())
            || az_step <= 0.0
            || el_step <= 0.0
            || (az_step * az_n as f32 - 360.0).abs() > 0.5
        {
            return None;
        }
        let body = bytes.get(HEADER..)?;
        if body.len() != az_n * el_n * 2 * TAPS * 4 {
            return None;
        }
        let mut floats = body
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| f32::from_le_bytes(*c));
        let mut nodes = Vec::with_capacity(az_n * el_n);
        for _ in 0..az_n * el_n {
            let mut h = Hrir::default();
            for ear in &mut h.ear {
                for tap in ear.iter_mut() {
                    let x = floats.next()?;
                    *tap = if x.is_finite() { x } else { 0.0 };
                }
            }
            nodes.push(h);
        }
        Some(Self {
            rate,
            az_n,
            el_n,
            az_step,
            el_min,
            el_step,
            nodes,
        })
    }

    /// Write the file format back out (tests, tools).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER + self.nodes.len() * 2 * TAPS * 4);
        out.extend_from_slice(b"HRIR");
        for v in [
            1,
            self.rate,
            TAPS as u32,
            self.az_n as u32,
            self.el_n as u32,
        ] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [self.az_step, self.el_min, self.el_step] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for n in &self.nodes {
            for ear in &n.ear {
                for t in ear {
                    out.extend_from_slice(&t.to_le_bytes());
                }
            }
        }
        out
    }

    /// The sample rate the responses are for.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// The same set for sounds at `rate`: the responses are stretched in
    /// time (linear interpolation) and keep their level at low frequencies.
    /// Rates above the set's cut the longest tail off.
    pub fn resampled(&self, rate: u32) -> Self {
        let rate = rate.max(8_000);
        if rate == self.rate {
            return self.clone();
        }
        // Source samples per output sample.
        let r = self.rate as f32 / rate as f32;
        let nodes = self
            .nodes
            .iter()
            .map(|n| {
                let mut out = Hrir::default();
                for (o, i) in out.ear.iter_mut().zip(&n.ear) {
                    for (k, o) in o.iter_mut().enumerate() {
                        let at = k as f32 * r;
                        let a = at.floor() as usize;
                        if a >= TAPS {
                            break;
                        }
                        let f = at - a as f32;
                        let next = i.get(a + 1).copied().unwrap_or(0.0);
                        *o = (i[a] + f * (next - i[a])) * r;
                    }
                }
                out
            })
            .collect();
        Self {
            rate,
            nodes,
            ..self.clone()
        }
    }

    fn node(&self, el: usize, az: usize) -> &Hrir {
        &self.nodes[el.min(self.el_n - 1) * self.az_n + az % self.az_n]
    }

    /// The responses for a sound in direction `dir` from the head (a unit
    /// vector in the listener's space: -Z ahead, +X right, +Y up).
    pub fn lookup(&self, dir: [f32; 3]) -> Hrir {
        let [x, y, z] = dir;
        let len = (x * x + y * y + z * z).sqrt();
        if !(len.is_finite() && len > 1e-4) {
            return Hrir::passthrough();
        }
        // Azimuth grows towards the left, 0 straight ahead.
        let az = (-x).atan2(-z).to_degrees().rem_euclid(360.0);
        let el = (y / len).clamp(-1.0, 1.0).asin().to_degrees();
        let fa = az / self.az_step;
        let a0 = fa.floor();
        let ta = fa - a0;
        let a0 = a0 as usize % self.az_n;
        let a1 = (a0 + 1) % self.az_n;
        let fe = ((el - self.el_min) / self.el_step).clamp(0.0, (self.el_n - 1) as f32);
        let e0 = fe.floor();
        let te = fe - e0;
        let e0 = e0 as usize;
        let e1 = (e0 + 1).min(self.el_n - 1);
        let w = [
            (e0, a0, (1.0 - te) * (1.0 - ta)),
            (e0, a1, (1.0 - te) * ta),
            (e1, a0, te * (1.0 - ta)),
            (e1, a1, te * ta),
        ];
        let mut out = Hrir::default();
        for (e, a, w) in w {
            if w <= 0.0 {
                continue;
            }
            let n = self.node(e, a);
            for (o, i) in out.ear.iter_mut().zip(&n.ear) {
                for (o, i) in o.iter_mut().zip(i) {
                    *o += w * i;
                }
            }
        }
        out
    }
}

/// A FIR filter over one channel: keeps the last [`TAPS`] inputs.
#[derive(Clone, Debug)]
pub struct Fir {
    hist: [f32; TAPS],
    at: usize,
}

impl Default for Fir {
    fn default() -> Self {
        Self {
            hist: [0.0; TAPS],
            at: 0,
        }
    }
}

impl Fir {
    /// Feed one sample, get one filtered sample.
    #[inline]
    pub fn tick(&mut self, x: f32, taps: &[f32; TAPS]) -> f32 {
        self.at = if self.at == 0 { TAPS - 1 } else { self.at - 1 };
        self.hist[self.at] = x;
        let (a, b) = self.hist.split_at(self.at);
        // `b` runs from the newest sample to the end of the ring, `a` is
        // the older rest.
        let mut acc = 0.0;
        for (h, t) in b.iter().zip(taps) {
            acc += h * t;
        }
        for (h, t) in a.iter().zip(&taps[b.len()..]) {
            acc += h * t;
        }
        acc
    }
}

/// Mix `from` towards `to` by `t` (0..=1), tap by tap.
pub fn blend(from: &[f32; TAPS], to: &[f32; TAPS], t: f32) -> [f32; TAPS] {
    let mut out = [0.0; TAPS];
    for ((o, a), b) in out.iter_mut().zip(from).zip(to) {
        *o = a + (b - a) * t;
    }
    out
}

/// A tiny synthetic set (tests, and the stand-in when the file is missing in
/// tests): the right ear louder on the right, and a notch-ish dip above.
pub fn synthetic(rate: u32) -> HrirSet {
    let (az_n, el_n) = (36usize, 14usize);
    let (az_step, el_min, el_step) = (10.0, -40.0, 10.0);
    let mut nodes = Vec::new();
    for e in 0..el_n {
        let el = el_min + e as f32 * el_step;
        for a in 0..az_n {
            let az = (a as f32 * az_step).to_radians();
            let mut h = Hrir::default();
            // Left is +sin(az).
            let left = 1.0 + 0.6 * az.sin();
            let right = 1.0 - 0.6 * az.sin();
            // Higher sounds get a second, delayed echo (a stand-in for the
            // pinna), so the spectrum changes with elevation.
            let echo = (el.max(0.0) / 90.0) * 0.5;
            h.ear[0][0] = left;
            h.ear[1][0] = right;
            h.ear[0][5] = -echo * left;
            h.ear[1][5] = -echo * right;
            nodes.push(h);
        }
    }
    let _ = PI;
    HrirSet {
        rate,
        az_n,
        el_n,
        az_step,
        el_min,
        el_step,
        nodes,
    }
}

#[cfg(test)]
mod tests;
