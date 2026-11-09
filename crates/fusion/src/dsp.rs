//! Signal-processing building blocks for the band: biquad EQ, a
//! state-variable filter, band-limited oscillators, a Juno-style chorus, a
//! tempo-synced ping-pong delay and a small room reverb. All of it is
//! allocation-free after construction.

use std::f32::consts::PI;

use super::synth::RATE;

const FS: f32 = RATE as f32;

/// Cheap xorshift noise in -1..1.
#[derive(Clone, Copy, Debug)]
pub struct Noise(pub u32);

impl Noise {
    #[inline]
    pub fn sample(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 * (2.0 / u32::MAX as f32) - 1.0
    }
}

/// The PolyBLEP residual that rounds off a saw or square's jump at phase
/// `t` (0..1) for a phase step of `dt`, so the oscillators don't alias.
#[inline]
pub fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let t = t / dt;
        t + t - t * t - 1.0
    } else if t > 1.0 - dt {
        let t = (t - 1.0) / dt;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// A band-limited sawtooth at phase `t` (0..1).
#[inline]
pub fn saw(t: f32, dt: f32) -> f32 {
    2.0 * t - 1.0 - blep(t, dt)
}

/// A band-limited square at phase `t` (0..1).
#[inline]
pub fn square(t: f32, dt: f32) -> f32 {
    let s = if t < 0.5 { 1.0 } else { -1.0 };
    let t2 = if t < 0.5 { t + 0.5 } else { t - 0.5 };
    s + blep(t, dt) - blep(t2, dt)
}

/// tan(x) for 0..~1.1 (the SVF's warped cutoff), good to ~1%.
#[inline]
fn tan_approx(x: f32) -> f32 {
    let x2 = x * x;
    x * (1.0 + x2 * (1.0 / 3.0 + x2 * (2.0 / 15.0 + x2 * (17.0 / 315.0))))
}

/// A Cytomic/Simper trapezoidal state-variable low-pass: stays stable
/// while the cutoff sweeps every sample.
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

impl Svf {
    /// Low-pass `x` at `fc` Hz with damping `k` (2 = no resonance, ~0.3 =
    /// singing).
    #[inline]
    pub fn lp(&mut self, x: f32, fc: f32, k: f32) -> f32 {
        let g = tan_approx(PI * fc.clamp(20.0, 0.36 * FS) / FS);
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        v2
    }
}

/// Fixed-coefficient RBJ biquad (transposed direct form II).
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

/// One EQ band.
#[derive(Clone, Copy, Debug)]
pub enum Eq {
    Hp(f32, f32),
    Lp(f32, f32),
    /// Frequency, Q, gain dB.
    Peak(f32, f32, f32),
    LowShelf(f32, f32),
    HighShelf(f32, f32),
    Bp(f32, f32),
}

impl Biquad {
    pub const IDENTITY: Biquad = Biquad {
        b0: 1.0,
        b1: 0.0,
        b2: 0.0,
        a1: 0.0,
        a2: 0.0,
        z1: 0.0,
        z2: 0.0,
    };

    pub fn new(eq: Eq) -> Self {
        let (f, q) = match eq {
            Eq::Hp(f, q) | Eq::Lp(f, q) | Eq::Peak(f, q, _) | Eq::Bp(f, q) => (f, q),
            Eq::LowShelf(f, _) | Eq::HighShelf(f, _) => (f, 0.707),
        };
        let w = 2.0 * PI * f.min(0.45 * FS) / FS;
        let (sn, cs) = w.sin_cos();
        let alpha = sn / (2.0 * q);
        let (b0, b1, b2, a0, a1, a2) = match eq {
            Eq::Hp(..) => (
                (1.0 + cs) / 2.0,
                -(1.0 + cs),
                (1.0 + cs) / 2.0,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            Eq::Lp(..) => (
                (1.0 - cs) / 2.0,
                1.0 - cs,
                (1.0 - cs) / 2.0,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            Eq::Bp(..) => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cs, 1.0 - alpha),
            Eq::Peak(_, _, db) => {
                let a = 10f32.powf(db / 40.0);
                (
                    1.0 + alpha * a,
                    -2.0 * cs,
                    1.0 - alpha * a,
                    1.0 + alpha / a,
                    -2.0 * cs,
                    1.0 - alpha / a,
                )
            }
            Eq::LowShelf(_, db) | Eq::HighShelf(_, db) => {
                let a = 10f32.powf(db / 40.0);
                let sq = 2.0 * a.sqrt() * alpha;
                let low = matches!(eq, Eq::LowShelf(..));
                let s = if low { 1.0 } else { -1.0 };
                // RBJ shelves; the high shelf is the low one mirrored.
                (
                    a * ((a + 1.0) - s * (a - 1.0) * cs + sq),
                    s * 2.0 * a * ((a - 1.0) - s * (a + 1.0) * cs),
                    a * ((a + 1.0) - s * (a - 1.0) * cs - sq),
                    (a + 1.0) + s * (a - 1.0) * cs + sq,
                    -s * 2.0 * ((a - 1.0) + s * (a + 1.0) * cs),
                    (a + 1.0) + s * (a - 1.0) * cs - sq,
                )
            }
        };
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline]
    pub fn tick(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Up to four EQ bands on a stereo signal.
#[derive(Clone, Copy, Debug)]
pub struct StereoEq {
    l: [Biquad; 4],
    r: [Biquad; 4],
    n: usize,
}

impl StereoEq {
    pub fn new(bands: &[Eq]) -> Self {
        let mut l = [Biquad::IDENTITY; 4];
        for (slot, b) in l.iter_mut().zip(bands) {
            *slot = Biquad::new(*b);
        }
        Self {
            l,
            r: l,
            n: bands.len().min(4),
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        for k in 0..self.n {
            let (bl, br) = (&mut self.l[k], &mut self.r[k]);
            for x in l.iter_mut() {
                *x = bl.tick(*x);
            }
            for x in r.iter_mut() {
                *x = br.tick(*x);
            }
        }
    }
}

/// A fractional delay line.
struct Line {
    buf: Vec<f32>,
    w: usize,
}

impl Line {
    fn new(n: usize) -> Self {
        Self {
            buf: vec![0.0; n],
            w: 0,
        }
    }

    #[inline]
    fn write(&mut self, x: f32) {
        self.buf[self.w] = x;
        self.w += 1;
        if self.w == self.buf.len() {
            self.w = 0;
        }
    }

    /// Read `d` samples back (linear interpolation), `d < len - 1`.
    #[inline]
    fn read(&self, d: f32) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(1.0, (n - 2) as f32);
        let di = d as usize;
        let fr = d - di as f32;
        let i0 = (self.w + n - di) % n;
        let i1 = (i0 + n - 1) % n;
        self.buf[i0] + (self.buf[i1] - self.buf[i0]) * fr
    }
}

/// A Juno-60 style stereo chorus: one delay line read by two taps whose
/// times swing in opposite phase, so the left and right drift apart.
pub struct Chorus {
    line: Line,
    phase: f32,
    rate: f32,
    centre: f32,
    depth: f32,
    mix: f32,
}

impl Chorus {
    /// `rate` Hz, centre and depth in ms, `mix` 0..1 wet.
    pub fn new(rate: f32, centre_ms: f32, depth_ms: f32, mix: f32) -> Self {
        Self {
            line: Line::new(2048),
            phase: 0.0,
            rate: rate / FS,
            centre: centre_ms * 0.001 * FS,
            depth: depth_ms * 0.001 * FS,
            mix,
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        let dry = 1.0 - 0.45 * self.mix;
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            self.line.write(0.5 * (*xl + *xr));
            self.phase += self.rate;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
            // Triangle LFO, like the Juno's.
            let tri = 4.0 * (self.phase - 0.5).abs() - 1.0;
            let a = self.line.read(self.centre + self.depth * tri);
            let b = self.line.read(self.centre - self.depth * tri);
            *xl = *xl * dry + a * self.mix;
            *xr = *xr * dry + b * self.mix;
        }
    }
}

/// A slow stereo auto-pan (the Rhodes Suitcase "vibrato").
#[derive(Clone, Copy, Debug)]
pub struct AutoPan {
    phase: f32,
    rate: f32,
    depth: f32,
}

impl AutoPan {
    pub fn new(rate: f32, depth: f32) -> Self {
        Self {
            phase: 0.0,
            rate: rate / FS,
            depth,
        }
    }

    pub fn process(&mut self, l: &mut [f32], r: &mut [f32]) {
        for (xl, xr) in l.iter_mut().zip(r.iter_mut()) {
            self.phase += self.rate;
            if self.phase >= 1.0 {
                self.phase -= 1.0;
            }
            let s = super::synth::sin_cyc(self.phase) * self.depth;
            *xl *= 1.0 - s;
            *xr *= 1.0 + s;
        }
    }
}

/// Ping-pong echo with a darkening feedback path, its time set from the
/// tempo (and glided, so tempo changes don't click).
pub struct Echo {
    l: Line,
    r: Line,
    time: f32,
    target: f32,
    fb: f32,
    damp_l: f32,
    damp_r: f32,
    hp: Biquad,
}

impl Echo {
    pub fn new() -> Self {
        Self {
            l: Line::new((1.3 * FS) as usize),
            r: Line::new((1.3 * FS) as usize),
            time: 0.4 * FS,
            target: 0.4 * FS,
            fb: 0.42,
            damp_l: 0.0,
            damp_r: 0.0,
            hp: Biquad::new(Eq::Hp(350.0, 0.7)),
        }
    }

    /// Dotted eighths at `bpm`.
    pub fn set_tempo(&mut self, bpm: f32) {
        self.target = (0.75 * 60.0 / bpm.max(40.0) * FS).min(1.2 * FS);
    }

    /// Add the echoes of mono `send` into `l`/`r`.
    pub fn process(&mut self, send: &[f32], l: &mut [f32], r: &mut [f32]) {
        for ((s, xl), xr) in send.iter().zip(l.iter_mut()).zip(r.iter_mut()) {
            self.time += (self.target - self.time) * 0.0005;
            let a = self.l.read(self.time);
            let b = self.r.read(self.time);
            self.damp_l += 0.45 * (a - self.damp_l);
            self.damp_r += 0.45 * (b - self.damp_r);
            let input = self.hp.tick(*s);
            // Left feeds right and right feeds left: ping-pong.
            self.l.write(input + self.damp_r * self.fb);
            self.r.write(self.damp_l * self.fb);
            *xl += a * 0.55;
            *xr += b * 0.55;
        }
    }
}

impl Default for Echo {
    fn default() -> Self {
        Self::new()
    }
}

/// A Freeverb-style room: four damped combs and two allpasses per side,
/// fed through a pre-delay and a high-pass so the tail doesn't mud up the
/// low mids.
pub struct Reverb {
    combs: [Comb; 8],
    aps: [Allpass; 4],
    pre: Line,
    hp: Biquad,
    lp: Biquad,
}

struct Comb {
    buf: Vec<f32>,
    i: usize,
    store: f32,
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

impl Comb {
    fn new(n: usize) -> Self {
        Self {
            buf: vec![0.0; n],
            i: 0,
            store: 0.0,
        }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.buf[self.i];
        self.store = y * 0.62 + self.store * 0.38;
        self.buf[self.i] = x + self.store * 0.82;
        self.i += 1;
        if self.i == self.buf.len() {
            self.i = 0;
        }
        y
    }
}

impl Allpass {
    fn new(n: usize) -> Self {
        Self {
            buf: vec![0.0; n],
            i: 0,
        }
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let b = self.buf[self.i];
        self.buf[self.i] = x + b * 0.5;
        self.i += 1;
        if self.i == self.buf.len() {
            self.i = 0;
        }
        b - x
    }
}

impl Reverb {
    pub fn new() -> Self {
        let c = [1116, 1188, 1277, 1356];
        let s = 23;
        Self {
            combs: [
                Comb::new(c[0]),
                Comb::new(c[1]),
                Comb::new(c[2]),
                Comb::new(c[3]),
                Comb::new(c[0] + s),
                Comb::new(c[1] + s),
                Comb::new(c[2] + s),
                Comb::new(c[3] + s),
            ],
            aps: [
                Allpass::new(556),
                Allpass::new(441),
                Allpass::new(556 + s),
                Allpass::new(441 + s),
            ],
            pre: Line::new(2048),
            hp: Biquad::new(Eq::Hp(320.0, 0.7)),
            lp: Biquad::new(Eq::Lp(7500.0, 0.7)),
        }
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> (f32, f32) {
        self.pre.write(x);
        // ~22 ms pre-delay keeps the dry attacks clear of the tail.
        let x = self.pre.read(970.0);
        let x = self.lp.tick(self.hp.tick(x)) * 0.25;
        let mut l = 0.0;
        let mut r = 0.0;
        for c in &mut self.combs[..4] {
            l += c.tick(x);
        }
        for c in &mut self.combs[4..] {
            r += c.tick(x);
        }
        let [a, b, c, d] = &mut self.aps;
        let l = b.tick(a.tick(l));
        let r = d.tick(c.tick(r));
        (l, r)
    }
}

impl Default for Reverb {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone_gain(eq: Eq, f: f32) -> f32 {
        let mut b = Biquad::new(eq);
        let mut peak = 0.0f32;
        for i in 0..(FS as usize / 2) {
            let y = b.tick((i as f32 * f / FS * 2.0 * PI).sin());
            if i > FS as usize / 4 {
                peak = peak.max(y.abs());
            }
        }
        20.0 * peak.log10()
    }

    #[test]
    fn eq_bands_do_what_they_say() {
        assert!((tone_gain(Eq::Peak(500.0, 1.0, -4.0), 500.0) + 4.0).abs() < 0.3);
        assert!(tone_gain(Eq::Peak(500.0, 1.0, -4.0), 5000.0).abs() < 0.3);
        assert!((tone_gain(Eq::LowShelf(100.0, 3.0), 40.0) - 3.0).abs() < 0.4);
        assert!(tone_gain(Eq::LowShelf(100.0, 3.0), 2000.0).abs() < 0.3);
        assert!((tone_gain(Eq::HighShelf(3000.0, 3.0), 12000.0) - 3.0).abs() < 0.4);
        assert!(tone_gain(Eq::HighShelf(3000.0, 3.0), 200.0).abs() < 0.3);
        assert!(tone_gain(Eq::Hp(200.0, 0.7), 50.0) < -18.0);
    }

    #[test]
    fn oscillators_and_filter_are_bounded() {
        let mut f = Svf::default();
        let mut t = 0.0;
        let dt = 1234.0 / FS;
        for i in 0..20000 {
            t = (t + dt) % 1.0;
            let s = saw(t, dt) + square(t, dt);
            assert!(s.abs() < 2.5);
            let y = f.lp(s, 200.0 + (i % 5000) as f32 * 2.0, 0.4);
            assert!(y.is_finite() && y.abs() < 8.0);
        }
    }
}
