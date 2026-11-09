//! The band's instruments and mixer.
//!
//! - FM voices (DX7 style): electric piano, slap and fretless bass, a
//!   singing FM lead, FM brass lead, a bell pad and a kalimba/marimba.
//! - Subtractive voices (band-limited saws through a swept state-variable
//!   filter): a CS-80/GS-1 style brass section that swells in, Juno-ish
//!   strings, and a Minimoog-style mono lead with glide.
//! - A sample bank: a synthesized kit and hand percussion (see `drums`),
//!   plus `foley-synth` note-block instruments (bells, chimes, xylophone,
//!   flute, harp, guitar) rendered once and replayed at pitch.
//! - A mixer with per-bus EQ (to keep the low mids clear), chorus on the
//!   keys, pads and horns, a tempo-synced ping-pong echo, a room reverb and
//!   a limiter.
//!
//! Everything here runs on the audio thread: no allocation after
//! construction, cheap polynomial sines, fixed voice pools.

use std::f32::consts::TAU;
use std::sync::Arc;

use super::drums;
use super::dsp::{AutoPan, Chorus, Echo, Eq, Reverb, StereoEq, Svf, saw, square};

/// Engine sample rate.
pub const RATE: u32 = 44_100;
const INV_RATE: f32 = 1.0 / RATE as f32;

/// Which instrument (and mixer bus) a note is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Inst {
    // Live FM voices.
    EPiano,
    Bass,
    Fretless,
    Lead,
    Brass,
    Pad,
    Kalimba,
    // Live subtractive voices.
    /// Mono Minimoog-style lead.
    Mini,
    /// The horn section: poly saw brass with a filter swell.
    PolyBrass,
    /// String-machine pad.
    Strings,
    // Sampled: the synthesized kit and percussion...
    Kick,
    Snare,
    Ghost,
    Hat,
    OpenHat,
    Crash,
    Ride,
    Clap,
    Shaker,
    Conga,
    Tom,
    Rim,
    Cowbell,
    /// A reversed-cymbal riser that peaks on the next downbeat.
    Swell,
    // ...and foley note blocks.
    Bell,
    Chime,
    Xylo,
    Flute,
    Harp,
    Guitar,
}

impl Inst {
    pub const ALL: [Inst; 30] = [
        Inst::EPiano,
        Inst::Bass,
        Inst::Fretless,
        Inst::Lead,
        Inst::Brass,
        Inst::Pad,
        Inst::Kalimba,
        Inst::Mini,
        Inst::PolyBrass,
        Inst::Strings,
        Inst::Kick,
        Inst::Snare,
        Inst::Ghost,
        Inst::Hat,
        Inst::OpenHat,
        Inst::Crash,
        Inst::Ride,
        Inst::Clap,
        Inst::Shaker,
        Inst::Conga,
        Inst::Tom,
        Inst::Rim,
        Inst::Cowbell,
        Inst::Swell,
        Inst::Bell,
        Inst::Chime,
        Inst::Xylo,
        Inst::Flute,
        Inst::Harp,
        Inst::Guitar,
    ];
}

/// Mixer buses, faded independently (vertical re-orchestration).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Bus {
    Drums = 0,
    Bass = 1,
    Keys = 2,
    Lead = 3,
    Pad = 4,
    Perc = 5,
    Brass = 6,
}
pub const BUSES: usize = 7;

impl Inst {
    pub fn bus(self) -> Bus {
        use Inst::*;
        match self {
            Kick | Snare | Ghost | Hat | OpenHat | Crash | Ride | Clap | Shaker | Conga | Tom
            | Rim | Cowbell | Swell => Bus::Drums,
            Bass | Fretless => Bus::Bass,
            EPiano | Guitar => Bus::Keys,
            Lead | Brass | Flute | Mini => Bus::Lead,
            Pad | Strings => Bus::Pad,
            Bell | Chime | Xylo | Harp | Kalimba => Bus::Perc,
            PolyBrass => Bus::Brass,
        }
    }

    fn sampled(self) -> bool {
        !matches!(
            self,
            Inst::EPiano
                | Inst::Bass
                | Inst::Fretless
                | Inst::Lead
                | Inst::Brass
                | Inst::Pad
                | Inst::Kalimba
                | Inst::Mini
                | Inst::PolyBrass
                | Inst::Strings
        )
    }

    /// Reverb send.
    fn send(self) -> f32 {
        use Inst::*;
        match self {
            EPiano => 0.2,
            Bass | Kick | Fretless => 0.0,
            Lead | Brass | Mini => 0.26,
            Flute => 0.35,
            Pad | Strings => 0.42,
            PolyBrass => 0.3,
            Snare | Ghost | Tom | Rim | Conga => 0.14,
            Clap => 0.28,
            Hat | OpenHat | Cowbell | Shaker | Ride => 0.07,
            Crash => 0.18,
            Swell => 0.45,
            Bell | Chime | Xylo | Harp | Kalimba => 0.38,
            Guitar => 0.25,
        }
    }

    /// Echo (dotted-eighth delay) send.
    fn echo(self) -> f32 {
        use Inst::*;
        match self {
            Mini => 0.3,
            Lead => 0.26,
            Brass => 0.12,
            Flute => 0.2,
            Kalimba => 0.24,
            Bell | Harp => 0.16,
            PolyBrass => 0.05,
            Clap => 0.06,
            _ => 0.0,
        }
    }

    /// Stereo position, -1 left .. 1 right (some voices spread per note).
    fn pan(self) -> f32 {
        use Inst::*;
        match self {
            Hat | OpenHat => 0.3,
            Ride => -0.35,
            Shaker => 0.45,
            Ghost => -0.05,
            Conga => -0.4,
            Rim | Cowbell => -0.3,
            Crash => -0.25,
            Swell => 0.2,
            Bell | Chime | Xylo => 0.4,
            Harp => -0.3,
            Guitar => -0.3,
            Flute => 0.15,
            _ => 0.0,
        }
    }
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

// --- FM ----------------------------------------------------------------

/// One FM patch: carrier at 1x, modulator 1 (with feedback) and a second
/// modulator, each with its own decaying index, through an ADSR, plus an
/// optional pure sine at the fundamental for weight.
#[derive(Clone, Copy, Debug)]
struct Patch {
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
}

const EP: Patch = Patch {
    // "E.PIANO 1": a warm 1:1 pair plus a 14:1 tine "ding" that dies fast.
    r1: 1.0,
    i1_peak: 1.5,
    i1_sus: 0.22,
    i1_tau: 0.55,
    r2: 14.0,
    i2_peak: 1.25,
    i2_tau: 0.02,
    fb: 0.0,
    attack: 0.002,
    decay: 1.7,
    sustain: 0.0,
    release: 0.2,
    detune: 0.06,
    gain: 0.15,
    vibrato: 0.0,
    swell: false,
    sub: 0.0,
    ks: 0.5,
    i_lfo: 0.0,
};

const BASS: Patch = Patch {
    // FM slap: a hard, fast-decaying 1:1 index (the thumb) with grit from
    // feedback, a 3:1 click, and a clean fundamental under it all.
    r1: 1.0,
    i1_peak: 3.0,
    i1_sus: 0.4,
    i1_tau: 0.06,
    r2: 3.0,
    i2_peak: 1.0,
    i2_tau: 0.01,
    fb: 0.22,
    attack: 0.001,
    decay: 0.8,
    sustain: 0.4,
    release: 0.05,
    detune: 0.0,
    gain: 0.36,
    vibrato: 0.0,
    swell: false,
    sub: 0.55,
    ks: 0.3,
    i_lfo: 0.0,
};

const FRETLESS: Patch = Patch {
    // Fretless "mwah": a round fundamental whose brightness blooms just
    // after the attack, singing a little in the sustain.
    r1: 1.0,
    i1_peak: 1.2,
    i1_sus: 0.5,
    i1_tau: 0.12,
    r2: 2.0,
    i2_peak: 0.25,
    i2_tau: 0.05,
    fb: 0.12,
    attack: 0.012,
    decay: 1.4,
    sustain: 0.55,
    release: 0.09,
    detune: 0.0,
    gain: 0.34,
    vibrato: 0.0035,
    swell: true,
    sub: 0.6,
    ks: 0.2,
    i_lfo: 0.0,
};

const LEAD: Patch = Patch {
    // Singing FM lead: self-feedback makes a soft saw, 2:1 adds body.
    r1: 1.0,
    i1_peak: 1.8,
    i1_sus: 1.1,
    i1_tau: 0.25,
    r2: 2.0,
    i2_peak: 0.35,
    i2_tau: 1.0,
    fb: 0.55,
    attack: 0.012,
    decay: 0.8,
    sustain: 0.8,
    release: 0.14,
    detune: 0.0,
    gain: 0.16,
    vibrato: 0.0045,
    swell: false,
    sub: 0.0,
    ks: 0.35,
    i_lfo: 0.15,
};

const BRASS: Patch = Patch {
    // FM brass lead: the index swells in over the attack, like a horn
    // leaning into a note.
    r1: 1.0,
    i1_peak: 2.4,
    i1_sus: 1.6,
    i1_tau: 0.09,
    r2: 1.0,
    i2_peak: 0.0,
    i2_tau: 1.0,
    fb: 0.35,
    attack: 0.03,
    decay: 0.6,
    sustain: 0.75,
    release: 0.16,
    detune: 0.08,
    gain: 0.15,
    vibrato: 0.004,
    swell: true,
    sub: 0.0,
    ks: 0.35,
    i_lfo: 0.12,
};

const PAD: Patch = Patch {
    // Bell pad: 3.5:1 shimmer over a mellow 1:1, detuned ensemble.
    r1: 1.0,
    i1_peak: 0.6,
    i1_sus: 0.35,
    i1_tau: 1.5,
    r2: 3.5,
    i2_peak: 0.5,
    i2_tau: 0.8,
    fb: 0.0,
    attack: 0.45,
    decay: 2.0,
    sustain: 0.7,
    release: 1.4,
    detune: 0.1,
    gain: 0.06,
    vibrato: 0.0,
    swell: false,
    sub: 0.0,
    ks: 0.4,
    i_lfo: 0.45,
};

const KALIMBA: Patch = Patch {
    // Kalimba/marimba: a woody 3:1 knock and an 8:1 tine click over a
    // pure tone that rings briefly.
    r1: 3.0,
    i1_peak: 1.1,
    i1_sus: 0.0,
    i1_tau: 0.045,
    r2: 8.0,
    i2_peak: 0.9,
    i2_tau: 0.006,
    fb: 0.0,
    attack: 0.001,
    decay: 0.55,
    sustain: 0.0,
    release: 0.3,
    detune: 0.0,
    gain: 0.16,
    vibrato: 0.0,
    swell: false,
    sub: 0.0,
    ks: 0.6,
    i_lfo: 0.0,
};

fn patch(inst: Inst) -> &'static Patch {
    match inst {
        Inst::EPiano => &EP,
        Inst::Bass => &BASS,
        Inst::Fretless => &FRETLESS,
        Inst::Brass => &BRASS,
        Inst::Pad => &PAD,
        Inst::Kalimba => &KALIMBA,
        _ => &LEAD,
    }
}

#[derive(Clone, Copy, Debug)]
struct FmVoice {
    active: bool,
    inst: Inst,
    p: &'static Patch,
    freq: f32,
    target: f32,
    glide: f32,
    pc: f32,
    pc2: f32,
    pm1: f32,
    pm2: f32,
    last: f32,
    lfo: f32,
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
            inst: Inst::EPiano,
            p: &EP,
            freq: 0.0,
            target: 0.0,
            glide: 0.0,
            pc: 0.0,
            pc2: 0.0,
            pm1: 0.0,
            pm2: 0.0,
            last: 0.0,
            lfo: 0.0,
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

    fn start(&mut self, inst: Inst, note: u8, vel: f32, len: u32, pan: f32) {
        let p = patch(inst);
        let f = hz(note as f32);
        // Higher notes on a real tine piano (or a kalimba) die sooner.
        let pitch_k = if matches!(inst, Inst::EPiano | Inst::Kalimba) {
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
        let g = p.gain * (0.35 + 0.65 * vel);
        *self = Self {
            active: true,
            inst,
            p,
            freq: f,
            target: f,
            glide: 0.0,
            amp: 0.0,
            i1: if p.swell {
                p.i1_sus * 0.3
            } else {
                // Harder hits bark: velocity drives the index.
                p.i1_peak * (0.4 + 0.85 * vel)
            },
            i2: p.i2_peak * (0.35 + 0.9 * vel),
            iscale,
            k_dec: per_sample(p.decay * pitch_k),
            k_rel: per_sample(p.release),
            k_i1: per_sample(p.i1_tau),
            k_i2: per_sample(p.i2_tau),
            att_left: ((p.attack * RATE as f32) as u32).max(1),
            att_step: 0.0,
            hold: len.max(1),
            age: 0,
            vel,
            gl: pl * g,
            gr: pr * g,
            send: inst.send(),
            echo: inst.echo(),
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

#[derive(Clone, Copy, Debug)]
struct SubPatch {
    /// Detune of the saws in cents (one saw per entry, up to three).
    saws: &'static [f32],
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

const POLY_BRASS: SubPatch = SubPatch {
    // CS-80 / GS-1 style brass: two detuned saws, the filter blooming open
    // over the attack and settling back, a little scoop into the pitch.
    saws: &[-7.0, 6.0],
    square: 0.0,
    sub_osc: 0.0,
    cut_key: 1.1,
    cut_base: 250.0,
    env_oct: 3.4,
    f_attack: 0.075,
    f_decay: 0.45,
    f_sustain: 0.5,
    k: 1.3,
    four_pole: false,
    attack: 0.025,
    decay: 0.6,
    sustain: 0.85,
    release: 0.22,
    scoop: -0.35,
    scoop_tau: 0.035,
    vibrato: 0.0035,
    vib_rate: 5.2,
    vib_delay: 0.4,
    gain: 0.055,
    f_lfo: 0.25,
};

const STRINGS: SubPatch = SubPatch {
    // String machine: three saws spread wide, mellow and slow.
    saws: &[-11.0, 0.0, 10.0],
    square: 0.0,
    sub_osc: 0.0,
    cut_key: 2.6,
    cut_base: 600.0,
    env_oct: 0.8,
    f_attack: 0.6,
    f_decay: 1.5,
    f_sustain: 0.6,
    k: 1.6,
    four_pole: false,
    attack: 0.38,
    decay: 1.0,
    sustain: 1.0,
    release: 1.1,
    scoop: 0.0,
    scoop_tau: 0.1,
    vibrato: 0.0025,
    vib_rate: 4.6,
    vib_delay: 0.0,
    gain: 0.032,
    f_lfo: 0.5,
};

const MINI: SubPatch = SubPatch {
    // Minimoog lead: saw + square + sub through a resonant 24 dB ladder-ish
    // filter with a snappy envelope, delayed vibrato, glide.
    saws: &[0.0],
    square: 0.55,
    sub_osc: 0.3,
    cut_key: 1.3,
    cut_base: 220.0,
    env_oct: 3.2,
    f_attack: 0.006,
    f_decay: 0.32,
    f_sustain: 0.42,
    k: 0.75,
    four_pole: true,
    attack: 0.005,
    decay: 0.5,
    sustain: 0.85,
    release: 0.12,
    scoop: 0.0,
    scoop_tau: 0.1,
    vibrato: 0.0055,
    vib_rate: 5.5,
    vib_delay: 0.32,
    gain: 0.13,
    f_lfo: 0.15,
};

fn sub_patch(inst: Inst) -> &'static SubPatch {
    match inst {
        Inst::PolyBrass => &POLY_BRASS,
        Inst::Strings => &STRINGS,
        _ => &MINI,
    }
}

#[derive(Clone, Copy, Debug)]
struct SubVoice {
    active: bool,
    inst: Inst,
    p: &'static SubPatch,
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
            inst: Inst::Mini,
            p: &MINI,
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

    fn start(&mut self, inst: Inst, note: u8, vel: f32, len: u32, pan: f32) {
        let p = sub_patch(inst);
        let vel = vel.clamp(0.0, 1.0);
        let f = hz(note as f32);
        let (pl, pr) = pan_gains(pan);
        let g = p.gain * (0.4 + 0.6 * vel);
        // Softer notes open more slowly (a horn section easing in).
        let f_att = p.f_attack * (1.5 - 0.7 * vel);
        let seed = (note as f32 * 0.618).fract();
        *self = Self {
            active: true,
            inst,
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
            send: inst.send(),
            echo: inst.echo(),
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
        for (d, c) in dets.iter_mut().zip(p.saws) {
            *d = exp2_fast(c / 1200.0);
        }
        let nsaw = p.saws.len().min(3);
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

/// Every sampled instrument, multi-sampled every few semitones.
#[derive(Debug, Default)]
pub struct Bank {
    pub samples: Vec<Sample>,
}

/// A foley note-block instrument to render into the bank.
struct Recipe {
    inst: Inst,
    foley: u32,
    /// MIDI notes to render (pitched instruments), each its own sample.
    notes: &'static [u8],
    character: f32,
    secs: f32,
    gain: f32,
}

/// foley's note `n` sounds at base_hz * 2^((n-12)/12); find the n for a MIDI note.
fn foley_note(foley: u32, midi: u8) -> f32 {
    let base = foley_synth::base_hz(foley).unwrap_or(440.0);
    12.0 + 12.0 * (hz(midi as f32) / base).log2()
}

const RECIPES: &[Recipe] = &[
    Recipe {
        inst: Inst::Bell,
        foley: 6,
        notes: &[72, 76, 80, 84, 88],
        character: 0.5,
        secs: 1.6,
        gain: 0.2,
    },
    Recipe {
        inst: Inst::Chime,
        foley: 8,
        notes: &[84, 90, 96],
        character: 0.5,
        secs: 1.4,
        gain: 0.12,
    },
    Recipe {
        inst: Inst::Xylo,
        foley: 9,
        notes: &[72, 78, 84],
        character: 0.5,
        secs: 0.5,
        gain: 0.2,
    },
    Recipe {
        inst: Inst::Flute,
        foley: 5,
        notes: &[64, 68, 72, 76, 80, 84],
        character: 0.6,
        secs: 1.2,
        gain: 0.22,
    },
    Recipe {
        inst: Inst::Harp,
        foley: 0,
        notes: &[60, 66, 72, 78, 84],
        character: 0.55,
        secs: 1.4,
        gain: 0.2,
    },
    Recipe {
        inst: Inst::Guitar,
        foley: 7,
        notes: &[48, 54, 60, 66, 72],
        character: 0.45,
        secs: 1.0,
        gain: 0.18,
    },
];

/// The synthesized kit: (instrument, root note, generator, peak gain).
type DrumGen = fn() -> Vec<f32>;
const KIT: &[(Inst, u8, DrumGen, f32)] = &[
    (Inst::Kick, 36, drums::kick, 0.5),
    (Inst::Snare, 38, || drums::snare(0.15, 1.0), 0.6),
    (Inst::Ghost, 38, || drums::snare(0.06, 0.7), 0.2),
    (Inst::Hat, 42, drums::hat, 0.12),
    (Inst::OpenHat, 46, drums::open_hat, 0.1),
    (Inst::Crash, 49, drums::crash, 0.17),
    (Inst::Ride, 51, drums::ride, 0.11),
    (Inst::Clap, 39, drums::clap, 0.34),
    (Inst::Shaker, 70, drums::shaker, 0.07),
    (
        Inst::Conga,
        53,
        || drums::membrane(175.0, 0.22, 0.08, 0.35),
        0.36,
    ),
    (
        Inst::Conga,
        58,
        || drums::membrane(233.0, 0.18, 0.08, 0.4),
        0.34,
    ),
    (
        Inst::Conga,
        64,
        || drums::membrane(330.0, 0.12, 0.1, 0.6),
        0.32,
    ),
    (
        Inst::Conga,
        71,
        || drums::membrane(494.0, 0.09, 0.12, 0.5),
        0.28,
    ),
    (
        Inst::Tom,
        45,
        || drums::membrane(110.0, 0.32, 0.3, 0.25),
        0.5,
    ),
    (
        Inst::Tom,
        50,
        || drums::membrane(147.0, 0.28, 0.3, 0.25),
        0.48,
    ),
    (
        Inst::Tom,
        55,
        || drums::membrane(196.0, 0.24, 0.3, 0.25),
        0.46,
    ),
    (Inst::Rim, 37, drums::rim, 0.28),
    (Inst::Cowbell, 56, drums::cowbell, 0.16),
    (Inst::Swell, 57, drums::swell, 0.2),
];

/// How many [`Bank::build_step`] calls finish the bank.
pub fn bank_steps() -> usize {
    RECIPES.iter().map(|r| r.notes.len()).sum::<usize>() + KIT.len()
}

impl Bank {
    /// Render the `i`th sample (0..[`bank_steps`]) into the bank. Split
    /// into steps so the web build can spread the work across frames.
    pub fn build_step(&mut self, i: usize) {
        let mut k = i;
        for r in RECIPES {
            if k < r.notes.len() {
                let midi = r.notes[k];
                let n = foley_note(r.foley, midi);
                let seed = 0xF05_10Au64 + i as u64 * 7919;
                let Some(mut data) = foley_synth::note(r.foley, n, r.character, seed, RATE) else {
                    return;
                };
                trim(&mut data, r.secs, r.gain);
                self.samples.push(Sample {
                    inst: r.inst,
                    root: midi,
                    data,
                });
                return;
            }
            k -= r.notes.len();
        }
        if let Some(&(inst, root, make, gain)) = KIT.get(k) {
            let mut data = make();
            trim(&mut data, 10.0, gain);
            self.samples.push(Sample { inst, root, data });
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
    bus: Bus,
    choke: bool,
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

/// Bus trims: the kit is synthesized hot, the band sits up with it.
const BUS_TRIM: [f32; BUSES] = [1.0, 1.55, 1.6, 1.5, 1.6, 1.45, 1.6];

/// Per-bus EQ: carve each part its own space so the low mids (where EP,
/// pads and brass all pile up) stay clear, the bass has weight and the
/// leads have presence.
fn bus_eq(b: usize) -> StereoEq {
    match b {
        0 => StereoEq::new(&[
            Eq::Hp(32.0, 0.7),
            Eq::Peak(420.0, 1.0, -2.0),
            Eq::HighShelf(9000.0, -0.5),
        ]),
        1 => StereoEq::new(&[
            Eq::Hp(30.0, 0.7),
            Eq::LowShelf(95.0, 2.5),
            Eq::Peak(420.0, 1.0, -3.5),
            Eq::Peak(1700.0, 1.2, 1.5),
        ]),
        2 => StereoEq::new(&[
            Eq::Hp(120.0, 0.7),
            Eq::Peak(520.0, 0.9, -2.5),
            Eq::Peak(1800.0, 0.9, 2.0),
            Eq::HighShelf(4000.0, 2.0),
        ]),
        3 => StereoEq::new(&[
            Eq::Hp(170.0, 0.7),
            Eq::Peak(750.0, 1.0, -1.5),
            Eq::Peak(2200.0, 0.8, 3.0),
            Eq::Lp(13000.0, 0.7),
        ]),
        4 => StereoEq::new(&[
            Eq::Hp(220.0, 0.7),
            Eq::Peak(600.0, 0.8, -5.0),
            Eq::HighShelf(5000.0, 2.0),
        ]),
        5 => StereoEq::new(&[Eq::Hp(300.0, 0.7), Eq::Lp(12000.0, 0.7)]),
        _ => StereoEq::new(&[
            Eq::Hp(160.0, 0.7),
            Eq::Peak(500.0, 0.9, -3.0),
            Eq::Peak(2400.0, 0.9, 2.5),
        ]),
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
    sub: [SubVoice; SUB_VOICES],
    smp: [SampleVoice; SAMPLE_VOICES],
    pub bus_gain: [f32; BUSES],
    pub bus_target: [f32; BUSES],
    bl: Vec<[f32; BLOCK]>,
    br: Vec<[f32; BLOCK]>,
    send: Vec<f32>,
    echo_send: Vec<f32>,
    eq: Vec<StereoEq>,
    keys_chorus: Chorus,
    pad_chorus: Chorus,
    brass_chorus: Chorus,
    suitcase: AutoPan,
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
}

/// Longest block rendered at once.
pub const BLOCK: usize = 256;

impl Band {
    pub fn new(bank: Arc<Bank>) -> Self {
        Self {
            bank,
            fm: [FmVoice::idle(); FM_VOICES],
            lead: FmVoice::idle(),
            mini: SubVoice::idle(),
            sub: [SubVoice::idle(); SUB_VOICES],
            smp: [SampleVoice {
                active: false,
                idx: 0,
                pos: 0.0,
                rate: 1.0,
                gl: 0.0,
                gr: 0.0,
                send: 0.0,
                echo: 0.0,
                bus: Bus::Drums,
                choke: false,
            }; SAMPLE_VOICES],
            bus_gain: [0.0; BUSES],
            bus_target: [0.0; BUSES],
            bl: vec![[0.0; BLOCK]; BUSES],
            br: vec![[0.0; BLOCK]; BUSES],
            send: vec![0.0; BLOCK],
            echo_send: vec![0.0; BLOCK],
            eq: (0..BUSES).map(bus_eq).collect(),
            keys_chorus: Chorus::new(0.55, 6.5, 2.2, 0.5),
            pad_chorus: Chorus::new(0.32, 9.0, 3.5, 0.7),
            brass_chorus: Chorus::new(0.8, 5.5, 1.6, 0.45),
            suitcase: AutoPan::new(4.2, 0.22),
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
            + usize::from(self.lead.active)
            + usize::from(self.mini.active)
    }

    pub fn play(&mut self, ev: &Ev) {
        let bus = ev.inst.bus() as usize;
        // Don't spend voices on a bus that is faded out.
        if self.bus_target[bus] < 0.01 && self.bus_gain[bus] < 0.01 {
            return;
        }
        if ev.inst.sampled() {
            self.play_sample(ev);
            return;
        }
        match ev.inst {
            Inst::Lead | Inst::Brass => {
                if ev.glide && self.lead.active && !self.lead.releasing && self.lead.inst == ev.inst
                {
                    self.lead.glide_to(ev.note, ev.vel, ev.len, 0.05);
                } else {
                    self.lead.start(ev.inst, ev.note, ev.vel, ev.len, 0.08);
                    self.lead.iscale *= 0.75 + 0.5 * self.bright;
                }
            }
            Inst::Mini => {
                if ev.glide && self.mini.active && !self.mini.releasing {
                    self.mini.glide_to(ev.note, ev.vel, ev.len, 0.045);
                } else {
                    self.mini.start(ev.inst, ev.note, ev.vel, ev.len, 0.06);
                    self.mini.cut_mul = exp2_fast(self.bright - 0.5);
                }
            }
            Inst::PolyBrass | Inst::Strings => {
                // Horns spread across the section, strings wide.
                let spread = if ev.inst == Inst::Strings { 0.9 } else { 0.6 };
                let pan = ((ev.note as f32 - 68.0) / 14.0).clamp(-1.0, 1.0) * spread;
                let slot = self.free_sub();
                self.sub[slot].start(ev.inst, ev.note, ev.vel, ev.len, pan);
                self.sub[slot].cut_mul = exp2_fast(1.3 * (self.bright - 0.5));
            }
            inst => {
                let slot = self.free_fm(inst);
                let v = &mut self.fm[slot];
                // Fretless slides; the bass is otherwise re-plucked.
                if inst == Inst::Fretless && ev.glide && v.active && v.inst == inst && !v.releasing
                {
                    v.glide_to(ev.note, ev.vel, ev.len, 0.07);
                    return;
                }
                let pan = match inst {
                    Inst::EPiano => (ev.note as f32 - 62.0) / 24.0,
                    Inst::Pad => (self.rand() - 0.5) * 0.8,
                    Inst::Kalimba => 0.25 + (self.rand() - 0.5) * 0.5,
                    _ => 0.0,
                };
                self.fm[slot].start(inst, ev.note, ev.vel, ev.len, pan.clamp(-0.6, 0.6));
                let b = if inst == Inst::Bass { 0.4 } else { 0.8 };
                self.fm[slot].iscale *= 1.0 + b * (self.bright - 0.5);
            }
        }
    }

    fn free_fm(&self, inst: Inst) -> usize {
        // The bass is monophonic: a new bass note replaces the old one.
        if matches!(inst, Inst::Bass | Inst::Fretless)
            && let Some(i) = self
                .fm
                .iter()
                .position(|v| v.active && matches!(v.inst, Inst::Bass | Inst::Fretless))
        {
            return i;
        }
        if let Some(i) = self.fm.iter().position(|v| !v.active) {
            return i;
        }
        // Steal the quietest releasing voice, else the oldest (never the bass).
        let mut best = 0;
        let mut score = f32::MAX;
        for (i, v) in self.fm.iter().enumerate() {
            let bass = if matches!(v.inst, Inst::Bass | Inst::Fretless) {
                10.0
            } else {
                0.0
            };
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

    fn play_sample(&mut self, ev: &Ev) {
        let Some(idx) = self.bank.find(ev.inst, ev.note) else {
            return;
        };
        let root = self.bank.samples[idx].root;
        // Only the toms, congas and note-block instruments play at pitch.
        let pitched = !matches!(
            ev.inst,
            Inst::Kick
                | Inst::Snare
                | Inst::Ghost
                | Inst::Hat
                | Inst::OpenHat
                | Inst::Crash
                | Inst::Ride
                | Inst::Clap
                | Inst::Shaker
                | Inst::Rim
                | Inst::Cowbell
                | Inst::Swell
        );
        let rate = if pitched {
            2f32.powf((ev.note as f32 - root as f32) / 12.0)
        } else {
            1.0
        };
        let pan = match ev.inst {
            // Toms across the kit, high to low.
            Inst::Tom => (50.0 - ev.note as f32) / 14.0,
            Inst::Conga => -0.55 + (ev.note as f32 - 53.0) / 40.0,
            i => i.pan(),
        };
        let vel = ev.vel.clamp(0.0, 1.2);
        // An open hat is choked by the next closed one, like a real kit.
        if ev.inst == Inst::Hat {
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
            send: ev.inst.send(),
            echo: ev.inst.echo(),
            bus: ev.inst.bus(),
            choke: ev.inst == Inst::OpenHat,
        };
    }

    /// Mix `out.len() / 2` stereo frames (at most [`BLOCK`]).
    pub fn render(&mut self, out: &mut [f32]) {
        let n = (out.len() / 2).min(BLOCK);
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
            let b = v.inst.bus() as usize;
            v.render(out!(b), g0[b], g1[b]);
        }
        if self.lead.active {
            let b = self.lead.inst.bus() as usize;
            self.lead.render(out!(b), g0[b], g1[b]);
        }
        if self.mini.active {
            let b = self.mini.inst.bus() as usize;
            self.mini.render(out!(b), g0[b], g1[b]);
        }
        for v in self.sub.iter_mut().filter(|v| v.active) {
            let b = v.inst.bus() as usize;
            v.render(out!(b), g0[b], g1[b]);
        }
        for v in self.smp.iter_mut().filter(|v| v.active) {
            let data = &self.bank.samples[v.idx].data;
            let b = v.bus as usize;
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
                let s = (data[p] + (data[p + 1] - data[p]) * fr) * g;
                l[i] += s * v.gl;
                r[i] += s * v.gr;
                send[i] += s * v.send;
                echo[i] += s * v.echo;
                v.pos += v.rate;
                g += dg;
            }
        }
        // Bus processing.
        for b in 0..BUSES {
            let (l, r) = (&mut bl[b][..n], &mut br[b][..n]);
            self.eq[b].process(l, r);
            let t = BUS_TRIM[b];
            l.iter_mut().for_each(|x| *x *= t);
            r.iter_mut().for_each(|x| *x *= t);
            match b {
                2 => {
                    self.suitcase.process(l, r);
                    self.keys_chorus.process(l, r);
                }
                4 => self.pad_chorus.process(l, r),
                6 => self.brass_chorus.process(l, r),
                _ => {}
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
        let m0 = self.master;
        self.master += (self.master_target - self.master) * k;
        let dm = (self.master - m0) / n as f32;
        for i in 0..n {
            let (wl, wr) = self.verb.process(send[i]);
            let m = m0 + dm * i as f32;
            let xl = (l[i] + wl) * m;
            let xr = (r[i] + wr) * m;
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
        for inst in [
            Inst::EPiano,
            Inst::Bass,
            Inst::Fretless,
            Inst::Lead,
            Inst::Brass,
            Inst::Pad,
            Inst::Kalimba,
            Inst::Mini,
            Inst::PolyBrass,
            Inst::Strings,
        ] {
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
}
