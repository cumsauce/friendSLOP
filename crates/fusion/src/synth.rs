//! The band's instruments and mixer.
//!
//! - FM voices (DX7 style): electric piano, slap and fretless bass, a
//!   singing FM lead, FM brass lead, a bell pad, a kalimba, vibes, marimba,
//!   steel drum, and a drawbar organ (additive sines, key click, Leslie).
//! - Subtractive voices (band-limited saws through a swept state-variable
//!   filter): a CS-80/GS-1 style brass section that swells in, Juno-ish
//!   strings, and a Minimoog-style mono lead with glide.
//! - Plucked strings (Karplus-Strong, two polarisations, a body filter):
//!   nylon, steel and overdriven electric guitars, a clavinet through an
//!   envelope filter, upright and slap bass.
//! - Blown pipes (flute/bottle, pan flute) and a jazz choir (detuned saws
//!   through "oo"-to-"ah" formants).
//! - A sample bank: a synthesized kit and hand percussion (see `drums`),
//!   (timbales, bongos, cajón, tabla, tambourine, güiro, claves, woodblock),
//!   plus `foley-synth` note-block instruments (bells, chimes, xylophone,
//!   flute, harp) rendered once and replayed at pitch.
//! - A mixer with per-bus EQ (to keep the low mids clear), chorus on the
//!   keys, pads and horns, a tempo-synced ping-pong echo, a room reverb and
//!   a limiter.
//!
//! Everything here runs on the audio thread: no allocation after
//! construction, cheap polynomial sines, fixed voice pools.

use std::f32::consts::TAU;
use std::sync::Arc;

use super::dsp::{AutoPan, Chorus, Echo, Eq, Reverb, StereoEq, Svf, saw, square};
use crate::inst::{Engine, Foley, Hit, KitSample, Pan, Spec, Table, Voice, table};

/// Engine sample rate.
pub const RATE: u32 = 44_100;
const INV_RATE: f32 = 1.0 / RATE as f32;

pub use crate::inst::Inst;

/// Mixer buses, faded independently (vertical re-orchestration). These are
/// the band's slots, which the composer layers; `mix.ron` sets each one's
/// sound and `instruments.ron` puts instruments on them by name.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bus {
    Drums = 0,
    Bass = 1,
    Keys = 2,
    Lead = 3,
    Pad = 4,
    Perc = 5,
    Brass = 6,
    Guitar = 7,
}
pub const BUSES: usize = 8;

impl Bus {
    pub const ALL: [Bus; BUSES] = [
        Bus::Drums,
        Bus::Bass,
        Bus::Keys,
        Bus::Lead,
        Bus::Pad,
        Bus::Perc,
        Bus::Brass,
        Bus::Guitar,
    ];
    /// The buses' names, as `mix.ron` and `instruments.ron` write them.
    pub const NAMES: [&str; BUSES] = [
        "drums", "bass", "keys", "lead", "pad", "perc", "brass", "guitar",
    ];
}

/// Fast sine of `x` cycles (any range). Parabolic approximation with one
/// refinement: max error ~0.1%, far below what anyone hears in FM.
#[inline]
pub fn sin_cyc(x: f32) -> f32 {
    let x = x - (x + 0.5).floor(); // -0.5..0.5
    let y = 8.0 * x - 16.0 * x * x.abs();
    0.225 * (y * y.abs() - y) + y
}

/// 2^x, good to ~0.1% (filter sweeps every sample).
#[inline]
fn exp2_fast(x: f32) -> f32 {
    let xi = x.floor();
    let f = x - xi;
    let p = 1.0 + f * (0.6951 + f * (0.2262 + f * 0.0782));
    p * f32::from_bits(((xi as i32 + 127).clamp(1, 254) as u32) << 23)
}

/// MIDI note to Hz.
pub fn hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// Decay factor per sample for an exponential with time constant `tau` s.
fn per_sample(tau: f32) -> f32 {
    if tau <= 0.0 {
        0.0
    } else {
        (-INV_RATE / tau).exp()
    }
}

/// Equal-power pan gains.
fn pan_gains(pan: f32) -> (f32, f32) {
    let pan = pan.clamp(-1.0, 1.0);
    ((0.5 * (1.0 - pan)).sqrt(), (0.5 * (1.0 + pan)).sqrt())
}

/// Up to `N` values stored inline, so voices can copy their patch without
/// allocating. Read from a list in the data file (extra values are dropped).
#[derive(Clone, Copy, Debug)]
pub struct Fixed<T, const N: usize> {
    v: [T; N],
    n: usize,
}

impl<T: Copy + Default, const N: usize> Default for Fixed<T, N> {
    fn default() -> Self {
        Self {
            v: [T::default(); N],
            n: 0,
        }
    }
}

impl<T, const N: usize> Fixed<T, N> {
    fn as_slice(&self) -> &[T] {
        &self.v[..self.n]
    }
}

impl<'de, T: serde::Deserialize<'de> + Copy + Default, const N: usize> serde::Deserialize<'de>
    for Fixed<T, N>
{
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let all = Vec::<T>::deserialize(d)?;
        let mut f = Self::default();
        for (slot, x) in f.v.iter_mut().zip(all) {
            *slot = x;
            f.n += 1;
        }
        Ok(f)
    }
}

// --- FM ----------------------------------------------------------------

/// One FM patch: carrier at 1x, modulator 1 (with feedback) and a second
/// modulator, each with its own decaying index, through an ADSR, plus an
/// optional pure sine at the fundamental for weight.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub struct Patch {
    r1: f32,
    i1_peak: f32,
    i1_sus: f32,
    i1_tau: f32,
    r2: f32,
    i2_peak: f32,
    i2_tau: f32,
    fb: f32,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    /// Second carrier detune in semitones (0 = off): chorus/ensemble.
    detune: f32,
    gain: f32,
    vibrato: f32,
    /// Brass swell: the index rises over the attack instead of falling.
    swell: bool,
    /// Level of a clean sine at the fundamental (bass weight).
    sub: f32,
    /// Key scaling: how much the indices fall per octave above middle C,
    /// as on a DX7 (high notes mellower, and no aliasing).
    ks: f32,
    /// Slow drift of the brightness (fraction of the index) on held notes.
    i_lfo: f32,
    /// Extra pure sines over the carrier, (ratio, level): organ drawbars.
    bars: Fixed<(f32, f32), 8>,
    /// Amplitude tremolo depth (0 = off) and rate in Hz: vibes motor, Leslie.
    trem: f32,
    trem_rate: f32,
    /// Higher notes die sooner (tines, bars).
    pitch_decay: bool,
}

#[derive(Clone, Copy, Debug)]
struct FmVoice {
    active: bool,
    inst: Inst,
    /// Plays a bass line: one note at a time, never stolen.
    bass: bool,
    bus: usize,
    p: Patch,
    freq: f32,
    target: f32,
    glide: f32,
    pc: f32,
    pc2: f32,
    pm1: f32,
    pm2: f32,
    last: f32,
    lfo: f32,
    /// Tremolo phase.
    tph: f32,
    amp: f32,
    i1: f32,
    i2: f32,
    /// Index scale from key scaling, the alias guard and the band's
    /// current brightness.
    iscale: f32,
    drift: f32,
    k_dec: f32,
    k_rel: f32,
    k_i1: f32,
    k_i2: f32,
    att_left: u32,
    att_step: f32,
    hold: u32,
    age: u32,
    vel: f32,
    gl: f32,
    gr: f32,
    send: f32,
    echo: f32,
    releasing: bool,
}

impl FmVoice {
    fn idle() -> Self {
        Self {
            active: false,
            inst: Inst::NONE,
            bass: false,
            bus: 0,
            p: Patch::default(),
            freq: 0.0,
            target: 0.0,
            glide: 0.0,
            pc: 0.0,
            pc2: 0.0,
            pm1: 0.0,
            pm2: 0.0,
            last: 0.0,
            lfo: 0.0,
            tph: 0.0,
            amp: 0.0,
            i1: 0.0,
            i2: 0.0,
            iscale: 1.0,
            drift: 0.0,
            k_dec: 0.0,
            k_rel: 0.0,
            k_i1: 0.0,
            k_i2: 0.0,
            att_left: 0,
            att_step: 0.0,
            hold: 0,
            age: 0,
            vel: 0.0,
            gl: 0.0,
            gr: 0.0,
            send: 0.0,
            echo: 0.0,
            releasing: false,
        }
    }

    fn start(&mut self, ev: &Ev, s: &Spec, pan: f32) {
        let Engine::Fm(p) = s.engine else { return };
        let (inst, note, vel, len) = (ev.inst, ev.note, ev.vel, ev.len);
        let f = hz(note as f32);
        // Higher notes on a real tine piano (or a kalimba) die sooner.
        let pitch_k = if p.pitch_decay {
            (1.0 - (note as f32 - 60.0) / 60.0).clamp(0.5, 1.3)
        } else {
            1.0
        };
        let vel = vel.clamp(0.0, 1.0);
        // Key scaling, and keep the strongest sidebands under ~15 kHz.
        let ks = exp2_fast(-p.ks * (note as f32 - 60.0) / 12.0).clamp(0.35, 1.4);
        let top = f * (p.r1.max(p.r2) + 1.0 + p.i1_peak.max(p.i2_peak) * 1.5);
        let iscale = ks * (15_000.0 / top).min(1.0);
        let (pl, pr) = pan_gains(pan);
        let g = p.gain * dyn_gain(vel);
        *self = Self {
            active: true,
            inst,
            bass: s.voice == Voice::Bass,
            bus: s.bus,
            p,
            freq: f,
            target: f,
            glide: 0.0,
            amp: 0.0,
            i1: if p.swell {
                p.i1_sus * 0.3
            } else {
                // Harder hits bark: velocity drives the index.
                p.i1_peak * (0.4 + 0.7 * vel)
            },
            i2: p.i2_peak * (0.35 + 0.75 * vel),
            iscale,
            k_dec: per_sample(p.decay * pitch_k),
            k_rel: per_sample(p.release),
            k_i1: per_sample(p.i1_tau),
            k_i2: per_sample(p.i2_tau),
            // A soft touch speaks a hair slower.
            att_left: ((p.attack * (1.4 - 0.5 * vel) * RATE as f32) as u32).max(1),
            att_step: 0.0,
            hold: len.max(1),
            age: 0,
            vel,
            gl: pl * g,
            gr: pr * g,
            send: s.send,
            echo: s.echo,
            ..Self::idle()
        };
        self.att_step = 1.0 / self.att_left as f32;
        // Random-ish phases so stacked chord tones don't all start in phase
        // (and don't drift in step).
        self.pc2 = (note as f32 * 0.137).fract();
        self.drift = (note as f32 * 0.311).fract();
    }

    /// Legato into a new pitch (portamento), without re-attacking.
    fn glide_to(&mut self, note: u8, vel: f32, len: u32, time: f32) {
        self.target = hz(note as f32);
        self.glide = 1.0 - per_sample(time);
        self.hold = len.max(1);
        self.releasing = false;
        self.vel = vel;
        // A fresh breath of brightness on the new note.
        self.i1 = self.i1.max(self.p.i1_sus * (1.0 + 0.4 * vel));
    }

    #[inline]
    fn render(&mut self, out: Out, g0: f32, g1: f32) {
        let p = self.p;
        let n = out.l.len();
        let dg = (g1 - g0) / n.max(1) as f32;
        let mut g = g0;
        let fb = p.fb;
        let det = if p.detune != 0.0 {
            2f32.powf(p.detune / 12.0)
        } else {
            0.0
        };
        let swell_target = p.i1_peak * (0.6 + 0.5 * self.vel);
        // Slow brightness drift: evaluated once per block, it's ~0.15 Hz.
        if p.i_lfo > 0.0 {
            self.drift += 0.15 * n as f32 * INV_RATE;
        }
        let isc = self.iscale * (1.0 / TAU) * (1.0 + p.i_lfo * sin_cyc(self.drift));
        // Drawbars, leaving out any that would sound above ~15 kHz.
        let mut bars = [(0.0f32, 0.0f32); 8];
        let mut nb = 0;
        for &(r, a) in p.bars.as_slice() {
            if r * self.target < 15_000.0 {
                bars[nb] = (r, a);
                nb += 1;
            }
        }
        let bars = &bars[..nb];
        let trem_inc = p.trem_rate * INV_RATE;
        for i in 0..n {
            // Envelopes.
            if self.att_left > 0 {
                self.amp += self.att_step;
                self.att_left -= 1;
                if self.att_left == 0 {
                    self.amp = 1.0;
                }
            } else if self.releasing {
                self.amp *= self.k_rel;
            } else {
                self.amp = p.sustain + (self.amp - p.sustain) * self.k_dec;
            }
            if self.hold > 0 {
                self.hold -= 1;
                if self.hold == 0 {
                    self.releasing = true;
                }
            }
            if p.swell {
                self.i1 += (swell_target - self.i1) * (1.0 - self.k_i1);
            } else {
                self.i1 = p.i1_sus + (self.i1 - p.i1_sus) * self.k_i1;
            }
            self.i2 *= self.k_i2;
            if self.glide > 0.0 {
                self.freq += (self.target - self.freq) * self.glide;
            }
            let mut f = self.freq;
            if p.vibrato > 0.0 && self.age > RATE / 4 {
                self.lfo += 5.6 * INV_RATE;
                let depth = (((self.age - RATE / 4) as f32) * INV_RATE * 2.0).min(1.0);
                f *= 1.0 + p.vibrato * depth * sin_cyc(self.lfo);
            }
            self.age = self.age.saturating_add(1);
            let inc = f * INV_RATE;
            self.pc += inc;
            self.pm1 += inc * p.r1;
            self.pm2 += inc * p.r2;
            // Keep phases small for float precision.
            if self.pc > 64.0 {
                self.pc -= 64.0;
                self.pm1 -= (64.0 * p.r1).floor();
                self.pm2 -= (64.0 * p.r2).floor();
            }
            let m1 = sin_cyc(self.pm1 + fb * self.last) * self.i1;
            self.last = m1 * (1.0 / TAU);
            let m2 = sin_cyc(self.pm2) * self.i2;
            let pm = (m1 + m2) * isc;
            let mut s = sin_cyc(self.pc + pm);
            if det > 0.0 {
                self.pc2 += inc * det;
                if self.pc2 > 64.0 {
                    self.pc2 -= 64.0;
                }
                s = 0.6 * s + 0.6 * sin_cyc(self.pc2 + pm * 0.8);
            }
            if p.sub > 0.0 {
                s += p.sub * sin_cyc(self.pc);
            }
            for &(r, a) in bars {
                s += a * sin_cyc(self.pc * r);
            }
            if p.trem > 0.0 {
                self.tph += trem_inc;
                if self.tph > 1.0 {
                    self.tph -= 1.0;
                }
                s *= 1.0 - p.trem * (0.5 + 0.5 * sin_cyc(self.tph));
            }
            let s = s * self.amp * g;
            out.l[i] += s * self.gl;
            out.r[i] += s * self.gr;
            let m = s * (self.gl + self.gr);
            out.send[i] += m * self.send;
            out.echo[i] += m * self.echo;
            g += dg;
        }
        if self.releasing && self.amp < 1e-4 {
            self.active = false;
        }
    }
}

// --- Subtractive ---------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub struct SubPatch {
    /// Detune of the saws in cents (one saw per entry, up to three).
    saws: Fixed<f32, 3>,
    square: f32,
    /// Square one octave down.
    sub_osc: f32,
    /// Filter cutoff = freq * key + base, swept up by the envelope.
    cut_key: f32,
    cut_base: f32,
    env_oct: f32,
    f_attack: f32,
    f_decay: f32,
    f_sustain: f32,
    /// SVF damping (2 = none; lower is more resonant).
    k: f32,
    four_pole: bool,
    attack: f32,
    decay: f32,
    sustain: f32,
    release: f32,
    /// Starts this many semitones flat and scoops up (brass "blat").
    scoop: f32,
    scoop_tau: f32,
    vibrato: f32,
    vib_rate: f32,
    vib_delay: f32,
    gain: f32,
    /// Slow filter drift, octaves.
    f_lfo: f32,
}

#[derive(Clone, Copy, Debug)]
struct SubVoice {
    active: bool,
    inst: Inst,
    bus: usize,
    p: SubPatch,
    freq: f32,
    target: f32,
    glide: f32,
    ph: [f32; 3],
    ph_sq: f32,
    ph_sub: f32,
    f1: Svf,
    f2: Svf,
    amp: f32,
    fenv: f32,
    scoop: f32,
    lfo: f32,
    drift: f32,
    /// Cutoff scale from the band's brightness at note-on.
    cut_mul: f32,
    k_dec: f32,
    k_rel: f32,
    k_fdec: f32,
    k_scoop: f32,
    att_step: f32,
    f_att_step: f32,
    attacking: bool,
    f_attacking: bool,
    hold: u32,
    age: u32,
    vel: f32,
    gl: f32,
    gr: f32,
    send: f32,
    echo: f32,
    releasing: bool,
}

impl SubVoice {
    fn idle() -> Self {
        Self {
            active: false,
            inst: Inst::NONE,
            bus: 0,
            p: SubPatch::default(),
            freq: 0.0,
            target: 0.0,
            glide: 0.0,
            ph: [0.0; 3],
            ph_sq: 0.0,
            ph_sub: 0.0,
            f1: Svf::default(),
            f2: Svf::default(),
            amp: 0.0,
            fenv: 0.0,
            scoop: 0.0,
            lfo: 0.0,
            drift: 0.0,
            cut_mul: 1.0,
            k_dec: 0.0,
            k_rel: 0.0,
            k_fdec: 0.0,
            k_scoop: 0.0,
            att_step: 0.0,
            f_att_step: 0.0,
            attacking: false,
            f_attacking: false,
            hold: 0,
            age: 0,
            vel: 0.0,
            gl: 0.0,
            gr: 0.0,
            send: 0.0,
            echo: 0.0,
            releasing: false,
        }
    }

    fn start(&mut self, ev: &Ev, s: &Spec, pan: f32) {
        let Engine::Sub(p) = s.engine else { return };
        let (inst, note, vel, len) = (ev.inst, ev.note, ev.vel, ev.len);
        let vel = vel.clamp(0.0, 1.0);
        let f = hz(note as f32);
        let (pl, pr) = pan_gains(pan);
        let g = p.gain * dyn_gain(vel);
        // Softer notes open more slowly (a horn section easing in).
        let f_att = p.f_attack * (1.5 - 0.7 * vel);
        let seed = (note as f32 * 0.618).fract();
        *self = Self {
            active: true,
            inst,
            bus: s.bus,
            p,
            freq: f,
            target: f,
            ph: [seed, (seed + 0.33).fract(), (seed + 0.71).fract()],
            att_step: INV_RATE / p.attack.max(0.001),
            f_att_step: INV_RATE / f_att.max(0.001),
            attacking: true,
            f_attacking: true,
            k_dec: per_sample(p.decay),
            k_rel: per_sample(p.release),
            k_fdec: per_sample(p.f_decay),
            k_scoop: per_sample(p.scoop_tau),
            scoop: p.scoop,
            drift: (seed * 7.0).fract(),
            hold: len.max(1),
            vel,
            gl: pl * g,
            gr: pr * g,
            send: s.send,
            echo: s.echo,
            ..Self::idle()
        };
    }

    fn glide_to(&mut self, note: u8, vel: f32, len: u32, time: f32) {
        self.target = hz(note as f32);
        self.glide = 1.0 - per_sample(time);
        self.hold = len.max(1);
        self.releasing = false;
        self.vel = vel.clamp(0.0, 1.0);
        // Legato notes re-open the filter a touch, without re-triggering.
        self.fenv = self.fenv.max(0.7);
    }

    #[inline]
    fn render(&mut self, out: Out, g0: f32, g1: f32) {
        let p = self.p;
        let n = out.l.len();
        let dg = (g1 - g0) / n.max(1) as f32;
        let mut g = g0;
        let mut dets = [1.0f32; 3];
        for (d, c) in dets.iter_mut().zip(p.saws.as_slice()) {
            *d = exp2_fast(c / 1200.0);
        }
        let nsaw = p.saws.as_slice().len().min(3);
        let saw_g = 1.0 / (nsaw as f32).sqrt();
        let env_oct = p.env_oct * (0.55 + 0.45 * self.vel);
        let vib_delay = (p.vib_delay * RATE as f32) as u32;
        self.drift += 0.11 * n as f32 * INV_RATE;
        let cut_mul = self.cut_mul * exp2_fast(p.f_lfo * sin_cyc(self.drift));
        for i in 0..n {
            // Amp envelope.
            if self.releasing {
                self.amp *= self.k_rel;
            } else if self.attacking {
                self.amp += self.att_step;
                if self.amp >= 1.0 {
                    self.amp = 1.0;
                    self.attacking = false;
                }
            } else {
                self.amp = p.sustain + (self.amp - p.sustain) * self.k_dec;
            }
            // Filter envelope.
            if self.releasing {
                self.fenv *= self.k_rel;
            } else if self.f_attacking {
                self.fenv += self.f_att_step;
                if self.fenv >= 1.0 {
                    self.fenv = 1.0;
                    self.f_attacking = false;
                }
            } else {
                self.fenv = p.f_sustain + (self.fenv - p.f_sustain) * self.k_fdec;
            }
            if self.hold > 0 {
                self.hold -= 1;
                if self.hold == 0 {
                    self.releasing = true;
                }
            }
            if self.glide > 0.0 {
                self.freq += (self.target - self.freq) * self.glide;
            }
            self.scoop *= self.k_scoop;
            let mut f = self.freq * exp2_fast(self.scoop / 12.0);
            if p.vibrato > 0.0 && self.age > vib_delay {
                self.lfo += p.vib_rate * INV_RATE;
                if self.lfo > 1.0 {
                    self.lfo -= 1.0;
                }
                let depth = (((self.age - vib_delay) as f32) * INV_RATE * 2.0).min(1.0);
                f *= 1.0 + p.vibrato * depth * sin_cyc(self.lfo);
            }
            self.age = self.age.saturating_add(1);
            let dt = f * INV_RATE;
            let mut x = 0.0;
            for (ph, d) in self.ph.iter_mut().zip(dets).take(nsaw) {
                let dtk = dt * d;
                *ph += dtk;
                if *ph >= 1.0 {
                    *ph -= 1.0;
                }
                x += saw(*ph, dtk);
            }
            x *= saw_g;
            if p.square > 0.0 {
                self.ph_sq += dt;
                if self.ph_sq >= 1.0 {
                    self.ph_sq -= 1.0;
                }
                x += p.square * square(self.ph_sq, dt);
            }
            if p.sub_osc > 0.0 {
                let ds = dt * 0.5;
                self.ph_sub += ds;
                if self.ph_sub >= 1.0 {
                    self.ph_sub -= 1.0;
                }
                x += p.sub_osc * square(self.ph_sub, ds);
            }
            let cut = (f * p.cut_key + p.cut_base) * cut_mul * exp2_fast(env_oct * self.fenv);
            let mut y = self.f1.lp(x, cut, p.k);
            if p.four_pole {
                y = self.f2.lp(y, cut, 1.4);
            }
            let s = y * self.amp * g;
            out.l[i] += s * self.gl;
            out.r[i] += s * self.gr;
            let m = s * (self.gl + self.gr);
            out.send[i] += m * self.send;
            out.echo[i] += m * self.echo;
            g += dg;
        }
        if self.releasing && self.amp < 1e-4 {
            self.active = false;
        }
    }
}

// --- Plucked strings ------------------------------------------------------

/// Longest string period in samples (E1 is ~1070; this reaches ~22 Hz).
const STRING_LEN: usize = 2048;

/// How one kind of string sounds on the plucked-string model.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub struct StringPatch {
    /// Ring time (T60, s) at 110 Hz, falling as `(110 / f)^0.4` above,
    /// and its limits.
    t60: f32,
    t60_min: f32,
    t60_max: f32,
    /// Loss-filter pole (higher is duller), less for harder plucks
    /// (`c_vel`) and brighter sections (`c_bright`).
    c: f32,
    c_vel: f32,
    c_bright: f32,
    /// Where on the string it's plucked (fraction from the bridge).
    at: f32,
    /// Nail/pick noise in the pluck, plus this much more per velocity.
    nail: f32,
    nail_vel: f32,
    /// Smoothing passes over the pluck: hard notes, soft notes.
    passes: (u32, u32),
    /// Output low-pass (body, amp cabinet): Hz, plus per velocity and per
    /// brightness.
    lp: f32,
    lp_vel: f32,
    lp_bright: f32,
    /// Overdrive before the cabinet (0 = clean).
    drive: f32,
    /// Envelope filter (clav auto-wah): sweep depth in octaves (0 = off).
    wah: f32,
    gain: f32,
    release: f32,
    /// One note at a time (a bass line).
    mono: bool,
}

/// One string polarisation: a delay line closed through a one-pole loss
/// filter and a fractional-delay allpass that keeps it in tune.
#[derive(Clone, Copy, Debug, Default)]
struct StringLoop {
    delay: usize,
    /// Allpass coefficient for the fractional part of the period.
    eta: f32,
    ap_x: f32,
    ap_y: f32,
    /// Loss filter: pole and per-trip gain.
    c: f32,
    g: f32,
    lp: f32,
}

impl StringLoop {
    /// Tune to `f` Hz with loss-filter pole `c`, ringing for `t60` seconds.
    fn tune(f: f32, c: f32, t60: f32) -> Self {
        const MAX_G: f32 = 0.99995;
        let period = RATE as f32 / f;
        let w = TAU * f * INV_RATE;
        let mag = |c: f32| (1.0 - c) / (1.0 - 2.0 * c * w.cos() + c * c).sqrt();
        // The loss filter alone mustn't lose more per trip than the ring
        // time allows (high notes go round the loop a thousand times a
        // second): open it up until the gain can make up the rest.
        let trip = 10f32.powf(-3.0 / (t60 * f));
        let mut c = c;
        while c > 0.02 && mag(c) * MAX_G < trip {
            c *= 0.85;
        }
        let mag = mag(c);
        // The loss filter's own delay at the fundamental.
        let fd = (c * w.sin()).atan2(1.0 - c * w.cos()) / w;
        let rem = (period - fd).clamp(2.2, STRING_LEN as f32 - 2.0);
        let delay = (rem - 0.1).floor() as usize;
        let frac = rem - delay as f32;
        let g = (trip / mag).min(MAX_G);
        Self {
            delay,
            eta: (1.0 - frac) / (1.0 + frac),
            c,
            g,
            ..Self::default()
        }
    }

    #[inline]
    fn tick(&mut self, buf: &mut [f32], w: usize) -> f32 {
        let r = (w + STRING_LEN - self.delay) % STRING_LEN;
        let x = buf[r];
        self.lp = (1.0 - self.c) * x + self.c * self.lp;
        let y = self.g * self.lp;
        let out = self.eta * y + self.ap_x - self.eta * self.ap_y;
        self.ap_x = y;
        self.ap_y = out;
        buf[w] = out;
        x
    }
}

/// A nylon-string guitar note: two polarisations of one string (the second
/// a hair sharp and duller, so the tone blooms and then settles), excited
/// by a mostly triangular pluck (the flesh of the thumb, a little nail).
#[derive(Clone, Debug)]
struct PluckVoice {
    active: bool,
    inst: Inst,
    bus: usize,
    p: StringPatch,
    note: u8,
    a: StringLoop,
    b: StringLoop,
    /// Both delay lines, `STRING_LEN` each.
    buf: Vec<f32>,
    w: usize,
    amp: f32,
    k_rel: f32,
    hold: u32,
    age: u32,
    releasing: bool,
    /// Recent loudness, to free the voice once it has rung out.
    level: f32,
    gl: f32,
    gr: f32,
    send: f32,
    echo: f32,
    lp: [f32; 2],
    lp_a: f32,
    /// Envelope filter: state, envelope, decay per sample, base cutoff.
    wah: Svf,
    wah_env: f32,
    wah_k: f32,
    wah_fc: f32,
}

const PLUCK_VOICES: usize = 14;

impl PluckVoice {
    fn idle() -> Self {
        Self {
            active: false,
            inst: Inst::NONE,
            bus: 0,
            p: StringPatch::default(),
            note: 0,
            a: StringLoop::default(),
            b: StringLoop::default(),
            buf: vec![0.0; 2 * STRING_LEN],
            w: 0,
            amp: 0.0,
            k_rel: 0.0,
            hold: 0,
            age: 0,
            releasing: false,
            level: 0.0,
            gl: 0.0,
            gr: 0.0,
            send: 0.0,
            echo: 0.0,
            lp: [0.0; 2],
            lp_a: 1.0,
            wah: Svf::default(),
            wah_env: 0.0,
            wah_k: 0.0,
            wah_fc: 1000.0,
        }
    }

    fn start(&mut self, ev: &Ev, s: &Spec, pan: f32, bright: f32, noise: &mut u32) {
        let Engine::String(p) = s.engine else { return };
        let (inst, note, vel, len) = (ev.inst, ev.note, ev.vel, ev.len);
        self.inst = inst;
        self.bus = s.bus;
        self.p = p;
        let vel = vel.clamp(0.05, 1.0);
        let f = hz(note as f32);
        // Low strings ring for seconds, the top of the neck much less.
        let t60 = (p.t60 * (110.0 / f).powf(0.4)).clamp(p.t60_min, p.t60_max);
        // The loss filter: a touch brighter for harder plucks and brighter
        // sections (nylon is mellow, steel and clav bright).
        let c = (p.c - p.c_vel * vel - p.c_bright * bright).clamp(0.15, p.c + 0.08);
        self.a = StringLoop::tune(f, c, t60);
        self.b = StringLoop::tune(f * 1.0002, (c + 0.1).min(0.8), t60 * 0.55);
        // The pluck: a triangle peaking where the thumb meets the string,
        // a little noise for the nail, smoothed more for soft notes.
        let n = self.a.delay.max(2);
        let at = p.at + 0.04 * (note as f32 * 0.37).fract();
        let peak = ((n as f32 * at) as usize).clamp(1, n - 1);
        // Flesh more than nail: a soft, round attack.
        let nail = p.nail + p.nail_vel * vel;
        self.buf.fill(0.0);
        let mut mean = 0.0;
        for (k, slot) in self.buf.iter_mut().enumerate().take(n) {
            let tri = if k < peak {
                k as f32 / peak as f32
            } else {
                (n - k) as f32 / (n - peak) as f32
            };
            *noise ^= *noise << 13;
            *noise ^= *noise >> 17;
            *noise ^= *noise << 5;
            let r = *noise as f32 / u32::MAX as f32 * 2.0 - 1.0;
            let x = tri + nail * r;
            *slot = x;
            mean += x;
        }
        mean /= n as f32;
        let passes = if vel > 0.8 { p.passes.0 } else { p.passes.1 };
        for _ in 0..passes {
            let mut prev = self.buf[n - 1] - mean;
            for slot in self.buf.iter_mut().take(n) {
                let x = *slot - mean;
                *slot = 0.5 * x + 0.5 * prev;
                prev = x;
            }
            mean = 0.0;
        }
        // The second polarisation starts from the same pluck, softer. The
        // lines are read backwards from the write head, so the excitation
        // sits just behind it.
        let (la, lb) = self.buf.split_at_mut(STRING_LEN);
        for k in 0..n {
            let x = la[k];
            lb[k] = 0.45 * x;
        }
        la.rotate_right(STRING_LEN - n);
        lb.rotate_right(STRING_LEN - n);
        self.w = 0;
        let (pl, pr) = pan_gains(pan);
        let g = p.gain * dyn_gain(vel);
        self.gl = pl * g;
        self.gr = pr * g;
        self.send = s.send;
        self.echo = s.echo;
        // The body and the thumb soften the top end; harder plucks open it.
        let fc = (p.lp + p.lp_vel * vel + p.lp_bright * bright).min(0.4 * RATE as f32);
        self.lp_a = 1.0 - (-std::f32::consts::TAU * fc / RATE as f32).exp();
        self.lp = [0.0; 2];
        // The auto-wah opens from just above the note, harder hits further.
        self.wah = Svf::default();
        self.wah_env = p.wah * (0.5 + 0.5 * vel);
        self.wah_k = per_sample(0.09);
        self.wah_fc = (f * 1.5).max(250.0);
        self.note = note;
        self.amp = 1.0;
        self.k_rel = per_sample(p.release);
        self.hold = len.max(RATE / 20);
        self.age = 0;
        self.releasing = false;
        self.level = 1.0;
        self.active = true;
    }

    #[inline]
    fn render(&mut self, out: Out, g0: f32, g1: f32) {
        let n = out.l.len();
        let dg = (g1 - g0) / n.max(1) as f32;
        let mut g = g0;
        let (la, lb) = self.buf.split_at_mut(STRING_LEN);
        let mut loud = 0.0f32;
        for i in 0..n {
            let mut raw = self.a.tick(la, self.w) + self.b.tick(lb, self.w);
            if self.p.drive > 0.0 {
                raw = (self.p.drive * raw).tanh() * (1.6 / self.p.drive.sqrt());
            }
            if self.p.wah > 0.0 {
                let fc = self.wah_fc * exp2_fast(self.wah_env);
                raw = self.wah.lp(raw, fc, 0.45);
                self.wah_env *= self.wah_k;
            }
            self.lp[0] += self.lp_a * (raw - self.lp[0]);
            self.lp[1] += self.lp_a * (self.lp[0] - self.lp[1]);
            let x = self.lp[1] * 1.15;
            self.w = (self.w + 1) % STRING_LEN;
            if self.hold > 0 {
                self.hold -= 1;
                if self.hold == 0 {
                    self.releasing = true;
                }
            }
            if self.releasing {
                self.amp *= self.k_rel;
            }
            let s = x * self.amp * g;
            loud = loud.max(s.abs());
            out.l[i] += s * self.gl;
            out.r[i] += s * self.gr;
            let m = s * (self.gl + self.gr);
            out.send[i] += m * self.send;
            out.echo[i] += m * self.echo;
            g += dg;
        }
        self.age = self.age.saturating_add(n as u32);
        self.level = loud.max(self.level * 0.5);
        if (self.releasing && self.amp < 1e-4) || (self.age > RATE / 4 && self.level < 2e-5) {
            self.active = false;
        }
    }
}

// --- Blown flute / bottle -------------------------------------------------

/// A breathy blown pipe, half FM and half noise. A sine is phase-modulated
/// at twice its pitch, which adds only odd harmonics: the hollow tone of a
/// bottle or an ocarina. Breath (noise through a resonance at the note's
/// pitch, plus a little hiss) rides on top. The modulation index swells with
/// the breath, so a note starts with a "chiff", blooms and settles; harder
/// notes blow brighter, softer ones are mostly air. Low notes are bottles
/// (purer, breathier), high ones flutes (brighter). Vibrato arrives late.
/// How one kind of pipe is blown (flute and bottle; pan flute: purer,
/// airier, scooping into each note).
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub struct BlowPatch {
    /// Breath vibrato depth.
    vibrato: f32,
    /// Starts this many semitones off and slides up over `scoop_time` s.
    scoop: f32,
    scoop_time: f32,
    /// Attack time, plus this much more for a soft note.
    attack: f32,
    attack_soft: f32,
    release: f32,
    /// The "tu" on the onset.
    chiff: f32,
    /// Breath noise: this much, less `air_high` up the range (bottle to
    /// flute) and `air_vel` for harder notes.
    air: f32,
    air_high: f32,
    air_vel: f32,
    /// How much velocity brightens the tone.
    tone_vel: f32,
    gain: f32,
    /// Output level on top of `gain`.
    level: f32,
}

#[derive(Clone, Copy, Debug)]
struct BlowVoice {
    active: bool,
    inst: Inst,
    bus: usize,
    send: f32,
    echo: f32,
    /// Breath vibrato depth.
    vib_amt: f32,
    freq: f32,
    target: f32,
    glide: f32,
    ph: f32,
    mph: f32,
    vib: f32,
    age: u32,
    hold: u32,
    releasing: bool,
    amp: f32,
    att: f32,
    k_rel: f32,
    breath: f32,
    index: f32,
    chiff: f32,
    flutter: f32,
    noise: u32,
    bp: [f32; 2],
    hiss: f32,
    air: f32,
    vel: f32,
    gl: f32,
    gr: f32,
}

const BLOW_VOICES: usize = 6;

impl BlowVoice {
    fn idle() -> Self {
        Self {
            active: false,
            inst: Inst::NONE,
            bus: 0,
            send: 0.0,
            echo: 0.0,
            vib_amt: 0.0025,
            freq: 440.0,
            target: 440.0,
            glide: 0.0,
            ph: 0.0,
            mph: 0.0,
            vib: 0.0,
            age: 0,
            hold: 0,
            releasing: false,
            amp: 0.0,
            att: 0.0,
            k_rel: 0.0,
            breath: 0.0,
            index: 0.0,
            chiff: 0.0,
            flutter: 0.0,
            noise: 0x9E37_79B9,
            bp: [0.0; 2],
            hiss: 0.0,
            air: 0.3,
            vel: 0.0,
            gl: 0.0,
            gr: 0.0,
        }
    }

    fn start(&mut self, ev: &Ev, s: &Spec, pan: f32, seed: u32) {
        let Engine::Blown(p) = s.engine else { return };
        let (note, vel, len) = (ev.note, ev.vel, ev.len);
        let vel = vel.clamp(0.05, 1.0);
        let f = hz(note as f32);
        // 0 = bottle (low), 1 = flute (high).
        let reg = ((note as f32 - 62.0) / 20.0).clamp(0.0, 1.0);
        let (pl, pr) = pan_gains(pan);
        let g = p.gain * dyn_gain(vel);
        *self = Self {
            active: true,
            inst: ev.inst,
            bus: s.bus,
            send: s.send,
            echo: s.echo,
            vib_amt: p.vibrato,
            freq: f * exp2_fast(p.scoop / 12.0),
            target: f,
            glide: per_sample(p.scoop_time),
            ph: 0.0,
            mph: 0.25,
            // Soft notes take longer to speak.
            att: INV_RATE / (p.attack + p.attack_soft * (1.0 - vel)),
            k_rel: per_sample(p.release),
            hold: len.max(RATE / 25),
            chiff: p.chiff,
            noise: seed | 1,
            air: p.air - p.air_high * reg - p.air_vel * vel,
            vel: vel * p.tone_vel,
            gl: pl * g * p.level,
            gr: pr * g * p.level,
            ..Self::idle()
        };
    }

    fn glide_to(&mut self, note: u8, vel: f32, len: u32, time: f32) {
        self.target = hz(note as f32);
        self.glide = 1.0 - per_sample(time);
        self.hold = len.max(1);
        self.releasing = false;
        self.vel = vel.clamp(0.05, 1.0);
        // A little tongue on the new note.
        self.chiff = self.chiff.max(0.35);
    }

    #[inline]
    fn render(&mut self, out: Out, g0: f32, g1: f32) {
        use std::f32::consts::TAU;
        let n = out.l.len();
        let dg = (g1 - g0) / n.max(1) as f32;
        let mut g = g0;
        let k_chiff = per_sample(0.035);
        let k_flutter = 1.0 - per_sample(0.05);
        // A bottle is nearly pure: just a little edge when blown hard.
        let peak = 0.2 + 0.4 * self.vel;
        for i in 0..n {
            if self.glide > 0.0 {
                self.freq = self.target + (self.freq - self.target) * self.glide;
            }
            if self.hold > 0 {
                self.hold -= 1;
                if self.hold == 0 {
                    self.releasing = true;
                }
            }
            if self.releasing {
                self.amp *= self.k_rel;
            } else if self.amp < 1.0 {
                self.amp = (self.amp + self.att).min(1.0);
            }
            self.chiff *= k_chiff;
            // Breath pressure: the envelope with a slow, uneven flutter.
            self.noise ^= self.noise << 13;
            self.noise ^= self.noise >> 17;
            self.noise ^= self.noise << 5;
            let white = self.noise as f32 / u32::MAX as f32 * 2.0 - 1.0;
            self.flutter += (white - self.flutter) * k_flutter;
            self.breath = self.amp * (1.0 + 0.6 * self.flutter);
            // The tone blooms with the breath and spikes at the onset.
            let want_index = self.breath * peak + 0.5 * self.chiff;
            self.index += (want_index - self.index) * 0.002;
            let t = self.age as f32 * INV_RATE;
            let vib_depth = self.vib_amt * ((t - 0.35) / 0.4).clamp(0.0, 1.0);
            self.vib = (self.vib + 5.2 * INV_RATE).fract();
            let f = self.freq * (1.0 + vib_depth * (TAU * self.vib).sin());
            self.ph = (self.ph + f * INV_RATE).fract();
            self.mph = (self.mph + 2.0 * f * INV_RATE).fract();
            let tone = (TAU * self.ph + self.index * (TAU * self.mph).sin()).sin();
            // Breath noise resonating at the note, and a little open hiss.
            let fc = (f * 1.0).min(RATE as f32 / 8.0);
            let k = 2.0 * (std::f32::consts::PI * fc * INV_RATE).sin();
            let q = 0.07;
            let hp = white - self.bp[1] - q * self.bp[0];
            self.bp[0] += k * hp;
            self.bp[1] += k * self.bp[0];
            self.hiss += (white - self.hiss) * 0.35;
            let air = self.air * (1.0 + 2.0 * self.chiff);
            let x = tone * (1.0 - 0.45 * air) + air * (0.5 * self.bp[0] + 0.07 * self.hiss);
            let s = x * self.breath * g;
            out.l[i] += s * self.gl;
            out.r[i] += s * self.gr;
            let m = s * (self.gl + self.gr);
            out.send[i] += m * self.send;
            out.echo[i] += m * self.echo;
            self.age = self.age.saturating_add(1);
            g += dg;
        }
        if self.releasing && self.amp < 1e-4 {
            self.active = false;
        }
    }
}

// --- Jazz choir ---------------------------------------------------------

/// Singers per choir note (each a little out of tune with the others).
const SINGERS: usize = 3;

/// One note of a jazz vocal group: three singers (band-limited saws, the
/// glottal buzz, each slightly detuned with its own vibrato) through three
/// formant band-passes. The vowel opens from "oo" to "ah" as the note
/// speaks, the "doo-ah" of close-harmony backing vocals; low notes sing
/// with men's formants, high ones with women's. A little breath on top.
/// How the choir sings.
#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
pub struct ChoirPatch {
    gain: f32,
    /// Source low-pass: Hz, plus per velocity and per brightness.
    cutoff: f32,
    cutoff_vel: f32,
    cutoff_bright: f32,
    /// Attack time, plus this much more for a soft note.
    attack: f32,
    attack_soft: f32,
    release: f32,
    /// Time constant of the "oo" opening to "ah".
    vowel_time: f32,
    /// From this note up, women's formants (scaled by `high_scale`).
    high_from: u8,
    high_scale: f32,
    /// Formant centres (Hz), damping and level for "oo" and "ah".
    oo: [(f32, f32, f32); 3],
    ah: [(f32, f32, f32); 3],
}

#[derive(Clone, Copy, Debug)]
struct ChoirVoice {
    active: bool,
    bus: usize,
    send: f32,
    echo: f32,
    oo: [(f32, f32, f32); 3],
    ah: [(f32, f32, f32); 3],
    freq: f32,
    ph: [f32; SINGERS],
    vib: [f32; SINGERS],
    det: [f32; SINGERS],
    /// Formant band-passes (Chamberlin: band, low) per formant.
    bp: [[f32; 2]; 3],
    /// Source low-pass state (soft singing is darker).
    src_lp: f32,
    src_a: f32,
    /// Vowel: 0 = "oo", 1 = "ah".
    vowel: f32,
    k_vowel: f32,
    /// Formant scale: 1 = men, ~1.18 = women.
    fscale: f32,
    amp: f32,
    att: f32,
    k_rel: f32,
    hold: u32,
    age: u32,
    releasing: bool,
    noise: u32,
    gl: f32,
    gr: f32,
}

const CHOIR_VOICES: usize = 8;

impl ChoirVoice {
    fn idle() -> Self {
        Self {
            active: false,
            bus: 0,
            send: 0.0,
            echo: 0.0,
            oo: [(0.0, 0.0, 0.0); 3],
            ah: [(0.0, 0.0, 0.0); 3],
            freq: 220.0,
            ph: [0.0; SINGERS],
            vib: [0.0; SINGERS],
            det: [1.0; SINGERS],
            bp: [[0.0; 2]; 3],
            src_lp: 0.0,
            src_a: 0.5,
            vowel: 0.0,
            k_vowel: 0.0,
            fscale: 1.0,
            amp: 0.0,
            att: 0.0,
            k_rel: 0.0,
            hold: 0,
            age: 0,
            releasing: false,
            noise: 0x1234_5679,
            gl: 0.0,
            gr: 0.0,
        }
    }

    fn start(&mut self, ev: &Ev, s: &Spec, pan: f32, bright: f32, seed: u32) {
        let Engine::Choir(p) = s.engine else { return };
        let (note, vel, len) = (ev.note, ev.vel, ev.len);
        let vel = vel.clamp(0.05, 1.0);
        let (pl, pr) = pan_gains(pan);
        let g = p.gain * dyn_gain(vel);
        let fc = p.cutoff + p.cutoff_vel * vel + p.cutoff_bright * bright;
        let r = |k: u32| {
            ((seed.wrapping_mul(2_654_435_761).rotate_left(k * 7)) as f32) / u32::MAX as f32
        };
        *self = Self {
            active: true,
            bus: s.bus,
            send: s.send,
            echo: s.echo,
            oo: p.oo,
            ah: p.ah,
            freq: hz(note as f32),
            // Each singer a few cents off, vibrato starting out of step.
            det: [exp2_fast(-0.08 / 12.0), 1.0, exp2_fast(0.07 / 12.0)],
            vib: [r(1), r(2), r(3)],
            ph: [r(4), r(5), r(6)],
            src_a: 1.0 - (-TAU * fc * INV_RATE).exp(),
            k_vowel: per_sample(p.vowel_time),
            fscale: if note >= p.high_from {
                p.high_scale
            } else {
                1.0
            },
            att: INV_RATE / (p.attack + p.attack_soft * (1.0 - vel)),
            k_rel: per_sample(p.release),
            hold: len.max(RATE / 20),
            noise: seed | 1,
            gl: pl * g,
            gr: pr * g,
            ..Self::idle()
        };
    }

    #[inline]
    fn render(&mut self, out: Out, g0: f32, g1: f32) {
        let n = out.l.len();
        let dg = (g1 - g0) / n.max(1) as f32;
        let mut g = g0;
        let (send, echo) = (self.send, self.echo);
        // Formants are set once per block: the vowel moves slowly.
        let v = self.vowel;
        let mut fk = [(0.0f32, 0.0f32, 0.0f32); 3];
        for (j, slot) in fk.iter_mut().enumerate() {
            let (f0, d0, a0) = self.oo[j];
            let (f1, d1, a1) = self.ah[j];
            let f = (f0 + (f1 - f0) * v) * self.fscale;
            let d = d0 + (d1 - d0) * v;
            let a = a0 + (a1 - a0) * v;
            // Normalise the band-pass's 1/damping peak.
            *slot = (2.0 * (std::f32::consts::PI * f * INV_RATE).sin(), d, a * d);
        }
        let t = self.age as f32 * INV_RATE;
        let vib_depth = 0.004 * ((t - 0.3) / 0.5).clamp(0.0, 1.0);
        for i in 0..n {
            if self.hold > 0 {
                self.hold -= 1;
                if self.hold == 0 {
                    self.releasing = true;
                }
            }
            if self.releasing {
                self.amp *= self.k_rel;
            } else if self.amp < 1.0 {
                self.amp = (self.amp + self.att).min(1.0);
            }
            let mut buzz = 0.0;
            for k in 0..SINGERS {
                self.vib[k] = (self.vib[k] + (5.0 + 0.3 * k as f32) * INV_RATE).fract();
                let f = self.freq * self.det[k] * (1.0 + vib_depth * sin_cyc(self.vib[k]));
                let dt = f * INV_RATE;
                self.ph[k] += dt;
                if self.ph[k] >= 1.0 {
                    self.ph[k] -= 1.0;
                }
                buzz += saw(self.ph[k], dt);
            }
            self.noise ^= self.noise << 13;
            self.noise ^= self.noise >> 17;
            self.noise ^= self.noise << 5;
            let white = self.noise as f32 / u32::MAX as f32 * 2.0 - 1.0;
            self.src_lp +=
                self.src_a * (buzz * (1.0 / SINGERS as f32) + 0.08 * white - self.src_lp);
            let x = self.src_lp;
            let mut y = 0.0;
            for (st, &(k, d, a)) in self.bp.iter_mut().zip(&fk) {
                let hp = x - st[1] - d * st[0];
                st[0] += k * hp;
                st[1] += k * st[0];
                y += a * st[0];
            }
            let s = y * self.amp * g * 2.2;
            out.l[i] += s * self.gl;
            out.r[i] += s * self.gr;
            let m = s * (self.gl + self.gr);
            out.send[i] += m * send;
            out.echo[i] += m * echo;
            g += dg;
        }
        self.age = self.age.saturating_add(n as u32);
        if t > 0.04 {
            self.vowel = 1.0 + (self.vowel - 1.0) * self.k_vowel.powi(n as i32);
        }
        if self.releasing && self.amp < 1e-4 {
            self.active = false;
        }
    }
}

/// Where a voice mixes to: its bus and the effect sends.
struct Out<'a> {
    l: &'a mut [f32],
    r: &'a mut [f32],
    send: &'a mut [f32],
    echo: &'a mut [f32],
}

// --- Sample bank ---------------------------------------------------------

/// One pre-rendered sample: mono data and the note it was rendered at.
#[derive(Debug)]
pub struct Sample {
    pub inst: Inst,
    pub root: u8,
    pub data: Vec<f32>,
}

/// Loudness for a velocity: about 20 dB from a whisper to a full hit (a real
/// player's range), matching the old level at a typical 0.7.
fn dyn_gain(vel: f32) -> f32 {
    let f = |v: f32| 0.1 + 0.9 * v.clamp(0.0, 1.0).powf(1.5);
    f(vel) / f(0.7)
}

/// Every sampled instrument, multi-sampled every few semitones.
#[derive(Debug, Default)]
pub struct Bank {
    pub samples: Vec<Sample>,
}

/// foley's note `n` sounds at base_hz * 2^((n-12)/12); find the n for a MIDI note.
fn foley_note(foley: u32, midi: u8) -> f32 {
    let base = foley_synth::base_hz(foley).unwrap_or(440.0);
    12.0 + 12.0 * (hz(midi as f32) / base).log2()
}

/// One sample to render into the bank.
enum Job<'a> {
    Foley(Inst, &'a Foley, u8),
    Drum(Inst, KitSample),
}

/// Every sample the instruments file asks for: the note blocks first, then
/// the kit.
fn jobs(t: &Table) -> Vec<Job<'_>> {
    let mut out = Vec::new();
    for &i in &t.order {
        if let Some(Engine::Foley { recipe, .. }) = t.get(i).map(|s| &s.engine) {
            out.extend(recipe.notes.iter().map(|&n| Job::Foley(i, recipe, n)));
        }
    }
    for &i in &t.order {
        if let Some(Engine::Kit { samples, .. }) = t.get(i).map(|s| &s.engine) {
            out.extend(samples.iter().map(|&k| Job::Drum(i, k)));
        }
    }
    out
}

/// How many [`Bank::build_step`] calls finish the bank (none until the
/// instruments file is in).
pub fn bank_steps() -> usize {
    jobs(&table()).len()
}

impl Bank {
    /// Render the `i`th sample (0..[`bank_steps`]) into the bank. Split
    /// into steps so the web build can spread the work across frames.
    pub fn build_step(&mut self, i: usize) {
        let t = table();
        match jobs(&t).get(i) {
            Some(&Job::Foley(inst, r, midi)) => {
                let n = foley_note(r.foley, midi);
                let seed = 0xF05_10Au64 + i as u64 * 7919;
                let Some(mut data) = foley_synth::note(r.foley, n, r.character, seed, RATE) else {
                    return;
                };
                trim(&mut data, r.secs, r.gain);
                self.samples.push(Sample {
                    inst,
                    root: midi,
                    data,
                });
            }
            Some(&Job::Drum(inst, k)) => {
                let mut data = k.make.render();
                trim(&mut data, 10.0, k.peak);
                self.samples.push(Sample {
                    inst,
                    root: k.root,
                    data,
                });
            }
            None => {}
        }
    }

    /// Build every sample at once.
    pub fn build() -> Self {
        let mut b = Self::default();
        for i in 0..bank_steps() {
            b.build_step(i);
        }
        b
    }

    /// The best sample for this instrument and note: the nearest root.
    fn find(&self, inst: Inst, note: u8) -> Option<usize> {
        let mut best = None;
        let mut dist = u32::MAX;
        for (i, s) in self.samples.iter().enumerate() {
            if s.inst == inst {
                let d = (s.root as i32 - note as i32).unsigned_abs();
                if d < dist {
                    dist = d;
                    best = Some(i);
                }
            }
        }
        best
    }
}

/// Cut a sample to `secs` with a short fade, and scale its peak to `gain`.
fn trim(data: &mut Vec<f32>, secs: f32, gain: f32) {
    let n = ((secs * RATE as f32) as usize).min(data.len());
    data.truncate(n);
    let fade = (RATE as usize / 50).min(n);
    for i in 0..fade {
        data[n - 1 - i] *= i as f32 / fade as f32;
    }
    let peak = data.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if peak > 1e-6 {
        let g = gain / peak;
        data.iter_mut().for_each(|x| *x *= g);
    }
}

#[derive(Clone, Copy, Debug)]
struct SampleVoice {
    active: bool,
    idx: usize,
    pos: f32,
    rate: f32,
    gl: f32,
    gr: f32,
    send: f32,
    echo: f32,
    bus: usize,
    choke: bool,
    /// Velocity tone: a one-pole low-pass (state, coefficient).
    lp: f32,
    lp_a: f32,
}

/// A note for the engine to play.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ev {
    /// Samples after the bar starts.
    pub at: u32,
    pub inst: Inst,
    pub note: u8,
    pub vel: f32,
    /// Held for this many samples.
    pub len: u32,
    /// Slide into this note from the last one (mono leads, fretless).
    pub glide: bool,
}

// --- Mixer -----------------------------------------------------------------

/// The band's balance and bus sound: `assets/data/music/mix.ron`, one entry
/// per bus, by name ([`Bus::NAMES`]).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Mix {
    pub buses: std::collections::BTreeMap<String, BusMix>,
}

/// One bus: level, ducking, EQ and effects.
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct BusMix {
    /// Bus trim: the kit is synthesized hot, the band sits up with it.
    pub trim: f32,
    /// How deep the bus ducks under a full-strength kick.
    pub pump: f32,
    /// Up to six bands, in order.
    pub eq: Vec<Eq>,
    /// Suitcase tremolo: (rate Hz, depth).
    pub autopan: Option<(f32, f32)>,
    /// Chorus: (rate Hz, centre ms, depth ms, mix).
    pub chorus: Option<(f32, f32, f32, f32)>,
}

pub static MIX: friendslop_data::Data<Mix> = friendslop_data::Data::ron("data/music/mix.ron");

/// The buses' processing, built from [`MIX`]: a bus the file doesn't list
/// passes straight through.
struct BusFx {
    /// Which version of the file this is (0: not loaded yet).
    key: usize,
    eq: Vec<StereoEq>,
    autopan: Vec<Option<AutoPan>>,
    chorus: Vec<Option<Chorus>>,
    trim: [f32; BUSES],
    pump: [f32; BUSES],
}

impl BusFx {
    fn key() -> usize {
        let mix = MIX.get();
        if MIX.loaded() {
            Arc::as_ptr(&mix) as usize
        } else {
            0
        }
    }

    fn build() -> Self {
        let key = Self::key();
        let mix = MIX.get();
        let bus = |b: usize| mix.buses.get(Bus::NAMES[b]).filter(|_| key != 0);
        Self {
            key,
            eq: (0..BUSES)
                .map(|b| StereoEq::new(bus(b).map_or(&[][..], |m| &m.eq)))
                .collect(),
            autopan: (0..BUSES)
                .map(|b| {
                    bus(b)
                        .and_then(|m| m.autopan)
                        .map(|(r, d)| AutoPan::new(r, d))
                })
                .collect(),
            chorus: (0..BUSES)
                .map(|b| {
                    bus(b)
                        .and_then(|m| m.chorus)
                        .map(|(r, c, d, x)| Chorus::new(r, c, d, x))
                })
                .collect(),
            trim: std::array::from_fn(|b| bus(b).map_or(1.0, |m| m.trim)),
            pump: std::array::from_fn(|b| bus(b).map_or(0.0, |m| m.pump)),
        }
    }
}

const FM_VOICES: usize = 16;
const SUB_VOICES: usize = 12;
const SAMPLE_VOICES: usize = 18;

/// Voice pools, buses and effects.
pub struct Band {
    bank: Arc<Bank>,
    fm: [FmVoice; FM_VOICES],
    lead: FmVoice,
    mini: SubVoice,
    blow: [BlowVoice; BLOW_VOICES],
    choir: [ChoirVoice; CHOIR_VOICES],
    sub: [SubVoice; SUB_VOICES],
    pluck: Vec<PluckVoice>,
    smp: [SampleVoice; SAMPLE_VOICES],
    pub bus_gain: [f32; BUSES],
    pub bus_target: [f32; BUSES],
    bl: Vec<[f32; BLOCK]>,
    br: Vec<[f32; BLOCK]>,
    send: Vec<f32>,
    echo_send: Vec<f32>,
    fx: BusFx,
    /// The instruments, refreshed once per block.
    table: Arc<Table>,
    echo: Echo,
    verb: Reverb,
    limiter: f32,
    rng: u32,
    /// How bright the band plays (0..1), set per bar by the composer:
    /// FM indices and filter cutoffs open with it at each note-on.
    pub bright: f32,
    /// Reverb and echo send scales, eased towards their targets.
    wet: f32,
    pub wet_target: f32,
    echo_amt: f32,
    pub echo_target: f32,
    /// Master gain, eased towards `master_target`.
    master: f32,
    pub master_target: f32,
    /// Buses left out of the mix (bit per bus; for analysing stems).
    pub mute: u8,
    /// Ducking under voice chat (1 = none), eased: quick down, slow up.
    duck: f32,
    pub duck_target: f32,
    /// The mood's colour and feel (`compose::Tone`), eased towards the target
    /// so moods blend over a couple of seconds.
    colour: MasterColour,
    pub tone_target: crate::compose::Tone,
    /// Sidechain: how far the kick (and snare) is pushing the band down
    /// right now, 0..1. Set on each hit, falls back over ~130 ms.
    pump: f32,
}

/// The master colour stage (see [`MasterColour::process`]).
struct MasterColour {
    tone: crate::compose::Tone,
    /// Tilt filter state (left, right).
    tilt: [f32; 2],
    /// Tape wobble: a short delay line per channel and its LFO phases.
    wow_buf: Vec<[f32; 2]>,
    wow_pos: usize,
    wow_phase: [f32; 2],
}

impl MasterColour {
    /// Master colour for one stereo frame: tilt EQ (warmth), mid/side width
    /// and a gentle tape wobble (wow plus a faster flutter).
    #[inline]
    fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let t = self.tone;
        // One-pole low-pass at ~1 kHz; the rest is the top end.
        const A: f32 = 0.125;
        self.tilt[0] += A * (l - self.tilt[0]);
        self.tilt[1] += A * (r - self.tilt[1]);
        let top = 1.0 - 0.5 * t.warmth;
        let l = self.tilt[0] + top * (l - self.tilt[0]);
        let r = self.tilt[1] + top * (r - self.tilt[1]);
        let (mid, side) = (0.5 * (l + r), 0.5 * (l - r) * t.width);
        let (l, r) = (mid + side, mid - side);
        // Wobble: read a few ms back, the delay swaying slowly (wow) and a
        // touch faster (flutter). Both channels move together, like tape.
        self.wow_buf[self.wow_pos] = [l, r];
        self.wow_phase[0] = (self.wow_phase[0] + 0.5 * INV_RATE).fract();
        self.wow_phase[1] = (self.wow_phase[1] + 5.3 * INV_RATE).fract();
        let depth = t.wow * RATE as f32 * 0.0011;
        // Only as much delay as the sway needs: with no wobble it's a
        // straight pass-through.
        let d = depth * (1.2 + sin_cyc(self.wow_phase[0]) + 0.12 * sin_cyc(self.wow_phase[1]));
        let at = self.wow_pos as f32 - d;
        let at = if at < 0.0 { at + WOW_LEN as f32 } else { at };
        let i0 = at as usize % WOW_LEN;
        let i1 = (i0 + 1) % WOW_LEN;
        let f = at.fract();
        let a = self.wow_buf[i0];
        let b = self.wow_buf[i1];
        self.wow_pos = (self.wow_pos + 1) % WOW_LEN;
        (a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f)
    }
}

/// Wobble delay line length (samples, a power of two).
const WOW_LEN: usize = 2048;

/// Longest block rendered at once.
pub const BLOCK: usize = 256;

impl Band {
    pub fn new(bank: Arc<Bank>) -> Self {
        Self {
            bank,
            fm: [FmVoice::idle(); FM_VOICES],
            lead: FmVoice::idle(),
            mini: SubVoice::idle(),
            blow: [BlowVoice::idle(); BLOW_VOICES],
            choir: [ChoirVoice::idle(); CHOIR_VOICES],
            sub: [SubVoice::idle(); SUB_VOICES],
            pluck: (0..PLUCK_VOICES).map(|_| PluckVoice::idle()).collect(),
            smp: [SampleVoice {
                active: false,
                idx: 0,
                pos: 0.0,
                rate: 1.0,
                gl: 0.0,
                gr: 0.0,
                send: 0.0,
                echo: 0.0,
                bus: 0,
                choke: false,
                lp: 0.0,
                lp_a: 1.0,
            }; SAMPLE_VOICES],
            bus_gain: [0.0; BUSES],
            bus_target: [0.0; BUSES],
            bl: vec![[0.0; BLOCK]; BUSES],
            br: vec![[0.0; BLOCK]; BUSES],
            send: vec![0.0; BLOCK],
            echo_send: vec![0.0; BLOCK],
            fx: BusFx::build(),
            table: table(),
            echo: Echo::new(),
            verb: Reverb::new(),
            limiter: 1.0,
            rng: 0x9E37_79B9,
            mute: 0,
            master: 1.0,
            master_target: 1.0,
            bright: 0.5,
            wet: 1.0,
            wet_target: 1.0,
            echo_amt: 1.0,
            echo_target: 1.0,
            duck: 1.0,
            duck_target: 1.0,
            colour: MasterColour {
                tone: crate::compose::Tone::NEUTRAL,
                tilt: [0.0; 2],
                wow_buf: vec![[0.0; 2]; WOW_LEN],
                wow_pos: 0,
                wow_phase: [0.0, 0.31],
            },
            tone_target: crate::compose::Tone::NEUTRAL,
            pump: 0.0,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng as f32 / u32::MAX as f32
    }

    /// Sync the echo to the tempo.
    pub fn set_tempo(&mut self, bpm: f32) {
        self.echo.set_tempo(bpm);
    }

    /// Voices currently sounding (for tests and the CPU budget).
    pub fn active_voices(&self) -> usize {
        self.fm.iter().filter(|v| v.active).count()
            + self.sub.iter().filter(|v| v.active).count()
            + self.smp.iter().filter(|v| v.active).count()
            + self.pluck.iter().filter(|v| v.active).count()
            + usize::from(self.lead.active)
            + usize::from(self.mini.active)
            + self.blow.iter().filter(|v| v.active).count()
            + self.choir.iter().filter(|v| v.active).count()
    }

    /// Where an instrument's note sits in the stereo field.
    fn place(&mut self, pan: &Pan, note: u8) -> f32 {
        let mut p = pan.at;
        if let Some((c, w)) = pan.note {
            p += (note as f32 - c) / w;
        }
        if pan.rand != 0.0 {
            p += (self.rand() - 0.5) * pan.rand;
        }
        if let Some(c) = pan.clamp {
            let c = c.abs();
            p = p.max(-c).min(c);
        }
        p * pan.scale
    }

    pub fn play(&mut self, ev: &Ev) {
        let table = self.table.clone();
        let Some(s) = table.get(ev.inst) else {
            return;
        };
        let bus = s.bus;
        // Don't spend voices on a bus that is faded out.
        if self.bus_target[bus] < 0.01 && self.bus_gain[bus] < 0.01 {
            return;
        }
        match (&s.engine, s.voice) {
            (Engine::Kit { hit, .. } | Engine::Foley { hit, .. }, _) => {
                self.play_sample(ev, s, *hit);
            }
            (Engine::Fm(_), Voice::Lead) => {
                if let Some(time) = s.glide
                    && ev.glide
                    && self.lead.active
                    && !self.lead.releasing
                    && self.lead.inst == ev.inst
                {
                    self.lead.glide_to(ev.note, ev.vel, ev.len, time);
                } else {
                    let pan = self.place(&s.pan, ev.note);
                    self.lead.start(ev, s, pan);
                    self.lead.iscale *= (1.0 - 0.5 * s.bright) + s.bright * self.bright;
                }
            }
            (Engine::Sub(_), Voice::Lead) => {
                if let Some(time) = s.glide
                    && ev.glide
                    && self.mini.active
                    && !self.mini.releasing
                    && self.mini.inst == ev.inst
                {
                    self.mini.glide_to(ev.note, ev.vel, ev.len, time);
                } else {
                    let pan = self.place(&s.pan, ev.note);
                    self.mini.start(ev, s, pan);
                    self.mini.cut_mul = exp2_fast(s.bright * (self.bright - 0.5));
                }
            }
            (Engine::Choir(_), _) => {
                let slot = self
                    .choir
                    .iter()
                    .position(|v| !v.active)
                    .unwrap_or_else(|| {
                        let mut best = 0;
                        for (i, v) in self.choir.iter().enumerate() {
                            let s = |v: &ChoirVoice| v.amp - if v.releasing { 1.0 } else { 0.0 };
                            if s(v) < s(&self.choir[best]) {
                                best = i;
                            }
                        }
                        best
                    });
                // Basses left, sopranos right, like a choir on risers.
                let pan = self.place(&s.pan, ev.note);
                let seed = (self.rand() * u32::MAX as f32) as u32;
                let bright = self.bright;
                self.choir[slot].start(ev, s, pan, bright, seed);
            }
            (Engine::Blown(_), _) => {
                if let Some(time) = s.glide
                    && ev.glide
                    && let Some(v) = self
                        .blow
                        .iter_mut()
                        .find(|v| v.active && !v.releasing && v.inst == ev.inst)
                {
                    v.glide_to(ev.note, ev.vel, ev.len, time);
                    return;
                }
                let slot = self.blow.iter().position(|v| !v.active).unwrap_or_else(|| {
                    let mut best = 0;
                    for (i, v) in self.blow.iter().enumerate() {
                        if v.amp < self.blow[best].amp {
                            best = i;
                        }
                    }
                    best
                });
                let pan = self.place(&s.pan, ev.note);
                let seed = (self.rand() * u32::MAX as f32) as u32;
                self.blow[slot].start(ev, s, pan, seed);
            }
            (Engine::String(p), _) => {
                // The same note again re-plucks its string; a bass is one
                // string at a time.
                let mono = p.mono;
                let inst = ev.inst;
                let slot = self
                    .pluck
                    .iter()
                    .position(|v| {
                        v.active && v.inst == inst && (mono || v.note == ev.note) && !v.releasing
                    })
                    .or_else(|| {
                        self.pluck
                            .iter()
                            .position(|v| v.active && v.inst == inst && v.note == ev.note)
                    })
                    .or_else(|| self.pluck.iter().position(|v| !v.active))
                    .unwrap_or_else(|| {
                        // Steal the quietest.
                        let mut best = 0;
                        let mut q = f32::MAX;
                        for (i, v) in self.pluck.iter().enumerate() {
                            let l = v.level * v.amp;
                            if l < q {
                                q = l;
                                best = i;
                            }
                        }
                        best
                    });
                // Lower strings sit a little left, higher ones right (a
                // stereo mic).
                let pan = self.place(&s.pan, ev.note);
                let bright = self.bright;
                let mut noise = self.rng;
                self.pluck[slot].start(ev, s, pan, bright, &mut noise);
                self.rng = noise | 1;
            }
            (Engine::Sub(_), _) => {
                // Horns spread across the section, strings wide.
                let pan = self.place(&s.pan, ev.note);
                let slot = self.free_sub();
                self.sub[slot].start(ev, s, pan);
                self.sub[slot].cut_mul = exp2_fast(s.bright * (self.bright - 0.5));
            }
            (Engine::Fm(_), voice) => {
                let slot = self.free_fm(voice == Voice::Bass);
                let v = &mut self.fm[slot];
                // A fretless slides; the bass is otherwise re-plucked.
                if let Some(time) = s.glide
                    && ev.glide
                    && v.active
                    && v.inst == ev.inst
                    && !v.releasing
                {
                    v.glide_to(ev.note, ev.vel, ev.len, time);
                    return;
                }
                let pan = self.place(&s.pan, ev.note);
                self.fm[slot].start(ev, s, pan);
                self.fm[slot].iscale *= 1.0 + s.bright * (self.bright - 0.5);
            }
        }
    }

    fn free_fm(&self, bass: bool) -> usize {
        // The bass is monophonic: a new bass note replaces the old one.
        if bass && let Some(i) = self.fm.iter().position(|v| v.active && v.bass) {
            return i;
        }
        if let Some(i) = self.fm.iter().position(|v| !v.active) {
            return i;
        }
        // Steal the quietest releasing voice, else the oldest (never the bass).
        let mut best = 0;
        let mut score = f32::MAX;
        for (i, v) in self.fm.iter().enumerate() {
            let bass = if v.bass { 10.0 } else { 0.0 };
            let s = v.amp - if v.releasing { 1.0 } else { 0.0 } - v.age as f32 * 1e-7 + bass;
            if s < score {
                score = s;
                best = i;
            }
        }
        best
    }

    fn free_sub(&self) -> usize {
        if let Some(i) = self.sub.iter().position(|v| !v.active) {
            return i;
        }
        let mut best = 0;
        let mut score = f32::MAX;
        for (i, v) in self.sub.iter().enumerate() {
            let s = v.amp - if v.releasing { 1.0 } else { 0.0 } - v.age as f32 * 1e-7;
            if s < score {
                score = s;
                best = i;
            }
        }
        best
    }

    fn play_sample(&mut self, ev: &Ev, s: &Spec, hit: Hit) {
        let Some(idx) = self.bank.find(ev.inst, ev.note) else {
            return;
        };
        let root = self.bank.samples[idx].root;
        // Only the toms, congas and note-block instruments play at pitch.
        let rate = if hit.pitched {
            2f32.powf((ev.note as f32 - root as f32) / 12.0)
        } else {
            1.0
        };
        // Toms across the kit, high to low.
        let pan = self.place(&s.pan, ev.note);
        let vel = ev.vel.clamp(0.0, 1.2);
        // Sidechain: the kick (and, less, the snare and clap) push the band
        // aside for a moment so they punch through.
        let push = hit.push * vel.min(1.0);
        self.pump = self.pump.max(push);
        // Hit harder, louder (a little more than linear) and brighter: soft
        // hits lose the top end the way a real stick or mallet does.
        let tone = vel.min(1.0).powf(1.5);
        let floor = hit.tone_floor;
        let fc = (floor + (20_000.0 - floor) * tone).min(0.45 * RATE as f32);
        let lp_a = 1.0 - (-std::f32::consts::TAU * fc / RATE as f32).exp();
        let vel = vel * (vel / 0.7).powf(0.3);
        // An open hat is choked by the next closed one, like a real kit.
        if hit.chokes {
            for v in self.smp.iter_mut().filter(|v| v.active && v.choke) {
                v.active = false;
            }
        }
        let slot = self.smp.iter().position(|v| !v.active).unwrap_or_else(|| {
            // Steal the one furthest through its sample.
            let mut best = 0;
            let mut far = 0.0;
            for (i, v) in self.smp.iter().enumerate() {
                let left = v.pos / self.bank.samples[v.idx].data.len().max(1) as f32;
                if left > far {
                    far = left;
                    best = i;
                }
            }
            best
        });
        let (pl, pr) = pan_gains(pan);
        self.smp[slot] = SampleVoice {
            active: true,
            idx,
            pos: 0.0,
            rate,
            gl: pl * vel,
            gr: pr * vel,
            send: s.send,
            echo: s.echo,
            bus: s.bus,
            choke: hit.choked,
            lp: 0.0,
            lp_a,
        };
    }

    /// Mix `out.len() / 2` stereo frames (at most [`BLOCK`]).
    pub fn render(&mut self, out: &mut [f32]) {
        let n = (out.len() / 2).min(BLOCK);
        // Pick up edited data files.
        self.table = table();
        if BusFx::key() != self.fx.key {
            self.fx = BusFx::build();
        }
        for b in 0..BUSES {
            self.bl[b][..n].fill(0.0);
            self.br[b][..n].fill(0.0);
        }
        self.send[..n].fill(0.0);
        self.echo_send[..n].fill(0.0);
        // Bus fades: about 0.6 s time constant, linear within the block.
        let k = 1.0 - (-(n as f32) * INV_RATE / 0.6).exp();
        let mut g0 = self.bus_gain;
        for (g, t) in self.bus_gain.iter_mut().zip(self.bus_target) {
            *g += (t - *g) * k;
        }
        let mut g1 = self.bus_gain;
        for b in 0..BUSES {
            if self.mute & (1 << b) != 0 {
                g0[b] = 0.0;
                g1[b] = 0.0;
            }
        }
        let (bl, br) = (&mut self.bl, &mut self.br);
        let (send, echo) = (&mut self.send[..n], &mut self.echo_send[..n]);
        macro_rules! out {
            ($b:expr) => {
                Out {
                    l: &mut bl[$b][..n],
                    r: &mut br[$b][..n],
                    send: &mut *send,
                    echo: &mut *echo,
                }
            };
        }
        for v in self.fm.iter_mut().filter(|v| v.active) {
            let b = v.bus;
            v.render(out!(b), g0[b], g1[b]);
        }
        if self.lead.active {
            let b = self.lead.bus;
            self.lead.render(out!(b), g0[b], g1[b]);
        }
        if self.mini.active {
            let b = self.mini.bus;
            self.mini.render(out!(b), g0[b], g1[b]);
        }
        for v in self.sub.iter_mut().filter(|v| v.active) {
            let b = v.bus;
            v.render(out!(b), g0[b], g1[b]);
        }
        for v in self.pluck.iter_mut().filter(|v| v.active) {
            let b = v.bus;
            v.render(out!(b), g0[b], g1[b]);
        }
        for v in self.blow.iter_mut().filter(|v| v.active) {
            let b = v.bus;
            v.render(out!(b), g0[b], g1[b]);
        }
        for v in self.choir.iter_mut().filter(|v| v.active) {
            let b = v.bus;
            v.render(out!(b), g0[b], g1[b]);
        }
        for v in self.smp.iter_mut().filter(|v| v.active) {
            let data = &self.bank.samples[v.idx].data;
            let b = v.bus;
            let dg = (g1[b] - g0[b]) / n as f32;
            let mut g = g0[b];
            let (l, r) = (&mut bl[b], &mut br[b]);
            for i in 0..n {
                let p = v.pos as usize;
                if p + 1 >= data.len() {
                    v.active = false;
                    break;
                }
                let fr = v.pos - p as f32;
                v.lp += v.lp_a * (data[p] + (data[p + 1] - data[p]) * fr - v.lp);
                let s = v.lp * g;
                l[i] += s * v.gl;
                r[i] += s * v.gr;
                send[i] += s * v.send;
                echo[i] += s * v.echo;
                v.pos += v.rate;
                g += dg;
            }
        }
        // The sidechain envelope for this block: a hit lands at the block's
        // start (blocks split at events), then falls away exponentially.
        let mut pump = [0.0f32; BLOCK];
        let fall = (-INV_RATE / 0.13).exp();
        for p in pump[..n].iter_mut() {
            *p = self.pump;
            self.pump *= fall;
        }
        // Bus processing.
        let fx = &mut self.fx;
        for b in 0..BUSES {
            let (l, r) = (&mut bl[b][..n], &mut br[b][..n]);
            fx.eq[b].process(l, r);
            let t = fx.trim[b];
            let depth = fx.pump[b];
            if depth > 0.0 {
                for ((x, y), p) in l.iter_mut().zip(r.iter_mut()).zip(&pump[..n]) {
                    // A smooth dip (squared) so the duck doesn't click.
                    let g = t * (1.0 - depth * p * p.sqrt());
                    *x *= g;
                    *y *= g;
                }
            } else {
                l.iter_mut().for_each(|x| *x *= t);
                r.iter_mut().for_each(|x| *x *= t);
            }
            if let Some(a) = &mut fx.autopan[b] {
                a.process(l, r);
            }
            if let Some(c) = &mut fx.chorus[b] {
                c.process(l, r);
            }
        }
        // Sum the buses into the first one, add the echo.
        for b in 1..BUSES {
            let (first, rest) = bl.split_at_mut(b);
            for (x, y) in first[0][..n].iter_mut().zip(&rest[0][..n]) {
                *x += *y;
            }
            let (first, rest) = br.split_at_mut(b);
            for (x, y) in first[0][..n].iter_mut().zip(&rest[0][..n]) {
                *x += *y;
            }
        }
        // Effect sends, scaled by how wet this part of the tune is.
        let (w0, e0) = (self.wet, self.echo_amt);
        self.wet += (self.wet_target - self.wet) * k;
        self.echo_amt += (self.echo_target - self.echo_amt) * k;
        let (dw, de) = ((self.wet - w0) / n as f32, (self.echo_amt - e0) / n as f32);
        for (j, (s, e)) in send.iter_mut().zip(echo.iter_mut()).enumerate() {
            *s *= w0 + dw * j as f32;
            *e *= e0 + de * j as f32;
        }
        let (l, r) = (&mut bl[0][..n], &mut br[0][..n]);
        self.echo.process(echo, l, r);
        // Reverb, then a gentle limiter, then a soft safety clip.
        let rel = 1.0 - (-INV_RATE / 0.15).exp();
        let d0 = self.duck;
        let kd = if self.duck_target < self.duck {
            1.0 - (-(n as f32) * INV_RATE / 0.25).exp()
        } else {
            1.0 - (-(n as f32) * INV_RATE / 1.2).exp()
        };
        self.duck += (self.duck_target - self.duck) * kd;
        let m0 = self.master * d0;
        self.master += (self.master_target - self.master) * k;
        let dm = (self.master * self.duck - m0) / n as f32;
        // Ease the mood's colour (about 2 s to change over).
        let kt = 1.0 - (-(n as f32) * INV_RATE / 0.7).exp();
        let (a, b) = (&mut self.colour.tone, self.tone_target);
        a.warmth += (b.warmth - a.warmth) * kt;
        a.width += (b.width - a.width) * kt;
        a.wow += (b.wow - a.wow) * kt;
        for i in 0..n {
            let (wl, wr) = self.verb.process(send[i]);
            let m = m0 + dm * i as f32;
            let (cl, cr) = self.colour.process(l[i] + wl, r[i] + wr);
            let xl = cl * m;
            let xr = cr * m;
            let peak = xl.abs().max(xr.abs());
            const THRESH: f32 = 0.7;
            let want = if peak > THRESH { THRESH / peak } else { 1.0 };
            if want < self.limiter {
                self.limiter = want; // instant attack
            } else {
                self.limiter += (want - self.limiter) * rel; // slow release
            }
            out[2 * i] = soft_clip(xl * self.limiter);
            out[2 * i + 1] = soft_clip(xr * self.limiter);
        }
    }
}

/// Leaves quiet signals alone, rounds peaks off below 1.
#[inline]
fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a < 0.75 {
        x
    } else {
        let over = a - 0.75;
        x.signum() * (0.75 + 0.24 * over / (0.24 + over))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_sine_is_close() {
        for i in 0..1000 {
            let x = i as f32 / 333.0 - 1.5;
            let e = (sin_cyc(x) - (x * TAU).sin()).abs();
            assert!(e < 0.002, "{x}: {e}");
        }
    }

    #[test]
    fn fast_exp2_is_close() {
        for i in 0..400 {
            let x = i as f32 / 50.0 - 4.0;
            let e = (exp2_fast(x) / x.exp2() - 1.0).abs();
            assert!(e < 0.003, "{x}: {e}");
        }
    }

    /// Every live instrument makes a finite, audible, unclipped note.
    #[test]
    fn every_voice_sounds() {
        let bank = Arc::new(Bank::default());
        let t = table();
        let live: Vec<Inst> = t
            .order
            .iter()
            .copied()
            .filter(|&i| {
                !matches!(
                    t.get(i).map(|s| &s.engine),
                    Some(Engine::Kit { .. } | Engine::Foley { .. })
                )
            })
            .collect();
        assert!(live.len() > 10, "instruments.ron didn't load");
        for inst in live {
            let mut band = Band::new(bank.clone());
            band.bus_target = [1.0; BUSES];
            band.bus_gain = [1.0; BUSES];
            let note = if inst.bus() == Bus::Bass { 40 } else { 67 };
            band.play(&Ev {
                at: 0,
                inst,
                note,
                vel: 0.9,
                len: RATE / 2,
                glide: false,
            });
            let mut buf = vec![0.0f32; 2 * BLOCK];
            let mut peak = 0.0f32;
            for _ in 0..(RATE as usize / BLOCK) {
                band.render(&mut buf);
                for x in &buf {
                    assert!(x.is_finite(), "{inst:?}");
                    peak = peak.max(x.abs());
                }
            }
            assert!(peak > 0.02 && peak < 1.0, "{inst:?}: {peak}");
        }
    }

    /// One guitar note, mono, `secs` long.
    fn pluck(note: u8, vel: f32, secs: f32) -> Vec<f32> {
        let mut band = Band::new(Arc::new(Bank::default()));
        band.bus_target = [1.0; BUSES];
        band.bus_gain = [1.0; BUSES];
        band.wet_target = 0.0;
        band.wet = 0.0;
        band.echo_target = 0.0;
        band.echo_amt = 0.0;
        band.play(&Ev {
            at: 0,
            inst: Inst::named("Guitar"),
            note,
            vel,
            len: RATE * 8,
            glide: false,
        });
        let frames = (secs * RATE as f32) as usize;
        let mut out = vec![0.0f32; frames * 2];
        for c in out.chunks_mut(2 * BLOCK) {
            band.render(c);
        }
        out.as_chunks::<2>().0.iter().map(|f| f[0] + f[1]).collect()
    }

    /// The fundamental of `x` by autocorrelation (with parabolic peak
    /// interpolation), in Hz.
    fn pitch(x: &[f32], lo: f32, hi: f32) -> f32 {
        let (min_lag, max_lag) = ((RATE as f32 / hi) as usize, (RATE as f32 / lo) as usize);
        let ac = |lag: usize| -> f32 { x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum() };
        let mut best = min_lag;
        let mut bv = f32::MIN;
        for lag in min_lag..=max_lag {
            let v = ac(lag);
            if v > bv {
                bv = v;
                best = lag;
            }
        }
        let (a, b, c) = (ac(best - 1), bv, ac(best + 1));
        let off = 0.5 * (a - c) / (a - 2.0 * b + c);
        RATE as f32 / (best as f32 + off)
    }

    /// The nylon guitar is in tune across the neck and rings, then decays.
    #[test]
    fn nylon_is_in_tune_and_rings() {
        for note in [40u8, 45, 52, 57, 64, 69, 76, 83] {
            let x = pluck(note, 0.7, 1.2);
            let f = hz(note as f32);
            let seg = &x[RATE as usize / 5..RATE as usize / 5 + 8192];
            let got = pitch(seg, f * 0.8, f * 1.25);
            let cents = 1200.0 * (got / f).log2();
            assert!(
                cents.abs() < 4.0,
                "note {note}: {got:.2} Hz vs {f:.2} ({cents:+.1} c)"
            );
            let rms = |s: &[f32]| (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt();
            let early = rms(&x[..4410]);
            let late = rms(&x[x.len() - 4410..]);
            assert!(early > 0.01, "note {note} too quiet: {early}");
            assert!(
                late < early && late > early * 0.01,
                "note {note}: {early} -> {late}"
            );
        }
    }
}
