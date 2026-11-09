//! The band's instruments: 90s FM synth voices (DX7-style electric piano,
//! FM slap bass, synth brass/lead with portamento, bell pad) computed live,
//! plus a sample bank of note-block instruments (drums, bells, flute,
//! guitar...) rendered once by `foley-synth` and replayed at pitch.
//!
//! Everything here runs on the audio thread: no allocation after
//! construction, cheap polynomial sines, a fixed voice pool.

use std::f32::consts::TAU;
use std::sync::Arc;

/// Engine sample rate.
pub const RATE: u32 = 44_100;
const INV_RATE: f32 = 1.0 / RATE as f32;

/// Which instrument (and mixer bus) a note is for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Inst {
    // Live FM voices.
    EPiano,
    Bass,
    Lead,
    Brass,
    Pad,
    // Sampled (foley note blocks + a synthesized crash).
    Kick,
    Snare,
    Ghost,
    Hat,
    OpenHat,
    Crash,
    Tom,
    Rim,
    Cowbell,
    Bell,
    Chime,
    Xylo,
    Flute,
    Harp,
    Guitar,
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
}
pub const BUSES: usize = 6;

impl Inst {
    pub fn bus(self) -> Bus {
        use Inst::*;
        match self {
            Kick | Snare | Ghost | Hat | OpenHat | Crash | Tom | Rim | Cowbell => Bus::Drums,
            Bass => Bus::Bass,
            EPiano | Guitar => Bus::Keys,
            Lead | Brass | Flute => Bus::Lead,
            Pad => Bus::Pad,
            Bell | Chime | Xylo | Harp => Bus::Perc,
        }
    }

    fn sampled(self) -> bool {
        !matches!(
            self,
            Inst::EPiano | Inst::Bass | Inst::Lead | Inst::Brass | Inst::Pad
        )
    }

    /// Reverb send.
    fn send(self) -> f32 {
        use Inst::*;
        match self {
            EPiano => 0.22,
            Bass | Kick => 0.0,
            Lead | Brass => 0.28,
            Flute => 0.35,
            Pad => 0.45,
            Snare | Ghost | Tom | Rim => 0.12,
            Hat | OpenHat | Cowbell => 0.06,
            Crash => 0.2,
            Bell | Chime | Xylo | Harp => 0.4,
            Guitar => 0.25,
        }
    }

    /// Stereo position, -1 left .. 1 right (some voices spread per note).
    fn pan(self) -> f32 {
        use Inst::*;
        match self {
            Hat | OpenHat => 0.3,
            Ghost => -0.1,
            Tom => -0.2,
            Rim | Cowbell => -0.35,
            Crash => -0.25,
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

/// One FM patch: carrier at 1x, modulator 1 (with feedback) and a second
/// modulator, each with its own decaying index, through an ADSR.
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
}

const EP: Patch = Patch {
    // "E.PIANO 1": a warm 1:1 pair plus a 14:1 tine "ding" that dies fast.
    r1: 1.0,
    i1_peak: 1.6,
    i1_sus: 0.25,
    i1_tau: 0.6,
    r2: 14.0,
    i2_peak: 1.1,
    i2_tau: 0.025,
    fb: 0.0,
    attack: 0.002,
    decay: 1.6,
    sustain: 0.0,
    release: 0.18,
    detune: 0.07,
    gain: 0.16,
    vibrato: 0.0,
    swell: false,
};

const BASS: Patch = Patch {
    // FM slap: a hard, fast-decaying 1:1 index (the thumb) with grit from
    // feedback, plus a 3:1 click.
    r1: 1.0,
    i1_peak: 3.2,
    i1_sus: 0.5,
    i1_tau: 0.07,
    r2: 3.0,
    i2_peak: 0.9,
    i2_tau: 0.012,
    fb: 0.25,
    attack: 0.001,
    decay: 0.7,
    sustain: 0.35,
    release: 0.05,
    detune: 0.0,
    gain: 0.42,
    vibrato: 0.0,
    swell: false,
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
    release: 0.12,
    detune: 0.0,
    gain: 0.17,
    vibrato: 0.0045,
    swell: false,
};

const BRASS: Patch = Patch {
    // Synth brass: the index swells in over the attack, like a horn
    // section leaning into a note.
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
};

const PAD: Patch = Patch {
    // Bell pad: 3.5:1 shimmer over a mellow 1:1, detuned ensemble.
    r1: 1.0,
    i1_peak: 0.7,
    i1_sus: 0.45,
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
    gain: 0.07,
    vibrato: 0.0,
    swell: false,
};

fn patch(inst: Inst) -> &'static Patch {
    match inst {
        Inst::EPiano => &EP,
        Inst::Bass => &BASS,
        Inst::Brass => &BRASS,
        Inst::Pad => &PAD,
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
            releasing: false,
        }
    }

    fn start(&mut self, inst: Inst, note: u8, vel: f32, len: u32, pan: f32) {
        let p = patch(inst);
        let f = hz(note as f32);
        // Higher notes on a real tine piano die sooner.
        let pitch_k = if inst == Inst::EPiano {
            (1.0 - (note as f32 - 60.0) / 60.0).clamp(0.5, 1.3)
        } else {
            1.0
        };
        let vel = vel.clamp(0.0, 1.0);
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
                p.i1_peak * (0.55 + 0.6 * vel)
            },
            i2: p.i2_peak * (0.4 + 0.8 * vel),
            k_dec: per_sample(p.decay * pitch_k),
            k_rel: per_sample(p.release),
            k_i1: per_sample(p.i1_tau),
            k_i2: per_sample(p.i2_tau),
            att_left: ((p.attack * RATE as f32) as u32).max(1),
            att_step: 0.0,
            hold: len.max(1),
            age: 0,
            vel,
            gl: (0.5 * (1.0 - pan)).sqrt() * p.gain * (0.35 + 0.65 * vel),
            gr: (0.5 * (1.0 + pan)).sqrt() * p.gain * (0.35 + 0.65 * vel),
            send: inst_send(inst),
            ..Self::idle()
        };
        self.att_step = 1.0 / self.att_left as f32;
        // Random-ish phases so stacked chord tones don't all start in phase.
        self.pc2 = (note as f32 * 0.137).fract();
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
    fn render(&mut self, l: &mut [f32], r: &mut [f32], send: &mut [f32], g0: f32, g1: f32) {
        let p = self.p;
        let n = l.len();
        let dg = (g1 - g0) / n.max(1) as f32;
        let mut g = g0;
        let fb = p.fb;
        let det = if p.detune != 0.0 {
            2f32.powf(p.detune / 12.0)
        } else {
            0.0
        };
        let swell_target = p.i1_peak * (0.6 + 0.5 * self.vel);
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
            let pm = (m1 + m2) * (1.0 / TAU);
            let mut s = sin_cyc(self.pc + pm);
            if det > 0.0 {
                self.pc2 += inc * det;
                if self.pc2 > 64.0 {
                    self.pc2 -= 64.0;
                }
                s = 0.6 * s + 0.6 * sin_cyc(self.pc2 + pm * 0.8);
            }
            let s = s * self.amp * g;
            l[i] += s * self.gl;
            r[i] += s * self.gr;
            send[i] += s * self.send * (self.gl + self.gr);
            g += dg;
        }
        if self.releasing && self.amp < 1e-4 {
            self.active = false;
        }
    }
}

fn inst_send(inst: Inst) -> f32 {
    inst.send()
}

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

/// What to render into the bank: (instrument, foley index, foley note
/// number, MIDI note it sounds as, character, max seconds, gain).
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
        inst: Inst::Kick,
        foley: 1,
        notes: &[36],
        character: 0.35,
        secs: 0.45,
        gain: 0.95,
    },
    Recipe {
        inst: Inst::Snare,
        foley: 2,
        notes: &[52],
        character: 0.6,
        secs: 0.35,
        gain: 0.62,
    },
    Recipe {
        inst: Inst::Ghost,
        foley: 2,
        notes: &[54],
        character: 0.2,
        secs: 0.12,
        gain: 0.22,
    },
    Recipe {
        inst: Inst::Hat,
        foley: 3,
        notes: &[96],
        character: 0.25,
        secs: 0.06,
        gain: 0.22,
    },
    Recipe {
        inst: Inst::OpenHat,
        foley: 3,
        notes: &[96],
        character: 0.9,
        secs: 0.32,
        gain: 0.2,
    },
    Recipe {
        inst: Inst::Tom,
        foley: 1,
        notes: &[45, 50, 55],
        character: 0.55,
        secs: 0.4,
        gain: 0.6,
    },
    Recipe {
        inst: Inst::Rim,
        foley: 9,
        notes: &[84],
        character: 0.1,
        secs: 0.08,
        gain: 0.3,
    },
    Recipe {
        inst: Inst::Cowbell,
        foley: 11,
        notes: &[80],
        character: 0.3,
        secs: 0.25,
        gain: 0.18,
    },
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

/// How many [`Bank::build_step`] calls finish the bank.
pub fn bank_steps() -> usize {
    RECIPES.iter().map(|r| r.notes.len()).sum::<usize>() + 1
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
        if k == 0 {
            self.samples.push(Sample {
                inst: Inst::Crash,
                root: 60,
                data: crash(),
            });
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

/// A cymbal crash: six detuned square waves (the 808 recipe), high-passed,
/// plus a noise wash, ringing out over a second and a half.
fn crash() -> Vec<f32> {
    let n = (1.6 * RATE as f32) as usize;
    let freqs = [205.3, 304.4, 369.6, 522.7, 540.0, 800.0];
    let mut ph = [0.0f32; 6];
    let mut rng = 0x1234_5678u32;
    let (mut hp_x, mut hp_y, mut lp) = (0.0f32, 0.0f32, 0.0f32);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 * INV_RATE;
        let mut metal = 0.0;
        for (p, f) in ph.iter_mut().zip(freqs) {
            *p = (*p + f * 2.3 * INV_RATE).fract();
            metal += if *p < 0.5 { 1.0 } else { -1.0 };
        }
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        let noise = rng as f32 / u32::MAX as f32 * 2.0 - 1.0;
        let x = metal * 0.12 + noise * 0.6;
        // One-pole high-pass, then a gentle low-pass to take the fizz off.
        hp_y = 0.97 * (hp_y + x - hp_x);
        hp_x = x;
        lp += 0.55 * (hp_y - lp);
        let env = (-t / 0.45).exp() * (1.0 - (-t / 0.002).exp());
        out.push(lp * env);
    }
    trim(&mut out, 1.6, 0.26);
    out
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
    /// Lead only: slide into this note from the last one (portamento).
    pub glide: bool,
}

/// Voice pools, buses and effects.
pub struct Band {
    bank: Arc<Bank>,
    fm: [FmVoice; 14],
    lead: FmVoice,
    smp: [SampleVoice; 10],
    pub bus_gain: [f32; BUSES],
    pub bus_target: [f32; BUSES],
    l: Vec<f32>,
    r: Vec<f32>,
    send: Vec<f32>,
    verb: Reverb,
    limiter: f32,
    rng: u32,
}

/// Longest block rendered at once.
pub const BLOCK: usize = 256;

impl Band {
    pub fn new(bank: Arc<Bank>) -> Self {
        Self {
            bank,
            fm: [FmVoice::idle(); 14],
            lead: FmVoice::idle(),
            smp: [SampleVoice {
                active: false,
                idx: 0,
                pos: 0.0,
                rate: 1.0,
                gl: 0.0,
                gr: 0.0,
                send: 0.0,
                bus: Bus::Drums,
                choke: false,
            }; 10],
            bus_gain: [0.0; BUSES],
            bus_target: [0.0; BUSES],
            l: vec![0.0; BLOCK],
            r: vec![0.0; BLOCK],
            send: vec![0.0; BLOCK],
            verb: Reverb::new(),
            limiter: 1.0,
            rng: 0x9E37_79B9,
        }
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        self.rng as f32 / u32::MAX as f32
    }

    /// Voices currently sounding (for tests and the CPU budget).
    pub fn active_voices(&self) -> usize {
        self.fm.iter().filter(|v| v.active).count()
            + self.smp.iter().filter(|v| v.active).count()
            + usize::from(self.lead.active)
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
                }
            }
            inst => {
                let pan = match inst {
                    Inst::EPiano => (ev.note as f32 - 62.0) / 24.0,
                    Inst::Pad => (self.rand() - 0.5) * 0.8,
                    _ => 0.0,
                };
                let slot = self.free_fm(inst);
                self.fm[slot].start(inst, ev.note, ev.vel, ev.len, pan.clamp(-0.6, 0.6));
            }
        }
    }

    fn free_fm(&self, inst: Inst) -> usize {
        // The bass is monophonic: a new bass note replaces the old one.
        if inst == Inst::Bass
            && let Some(i) = self
                .fm
                .iter()
                .position(|v| v.active && v.inst == Inst::Bass)
        {
            return i;
        }
        if let Some(i) = self.fm.iter().position(|v| !v.active) {
            return i;
        }
        // Steal the quietest releasing voice, else the oldest.
        let mut best = 0;
        let mut score = f32::MAX;
        for (i, v) in self.fm.iter().enumerate() {
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
        let rate = 2f32.powf((ev.note as f32 - root as f32) / 12.0);
        let pan = ev.inst.pan();
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
        self.smp[slot] = SampleVoice {
            active: true,
            idx,
            pos: 0.0,
            rate,
            gl: (0.5 * (1.0 - pan)).sqrt() * vel,
            gr: (0.5 * (1.0 + pan)).sqrt() * vel,
            send: ev.inst.send(),
            bus: ev.inst.bus(),
            choke: ev.inst == Inst::OpenHat,
        };
    }

    /// Mix `out.len() / 2` stereo frames (at most [`BLOCK`]).
    pub fn render(&mut self, out: &mut [f32]) {
        let n = (out.len() / 2).min(BLOCK);
        let (l, r, send) = (&mut self.l[..n], &mut self.r[..n], &mut self.send[..n]);
        l.fill(0.0);
        r.fill(0.0);
        send.fill(0.0);
        // Bus fades: about 0.6 s time constant, linear within the block.
        let k = 1.0 - (-(n as f32) * INV_RATE / 0.6).exp();
        let g0 = self.bus_gain;
        for (g, t) in self.bus_gain.iter_mut().zip(self.bus_target) {
            *g += (t - *g) * k;
        }
        let g1 = self.bus_gain;
        for v in self.fm.iter_mut().filter(|v| v.active) {
            let b = v.inst.bus() as usize;
            v.render(l, r, send, g0[b], g1[b]);
        }
        if self.lead.active {
            let b = self.lead.inst.bus() as usize;
            self.lead.render(l, r, send, g0[b], g1[b]);
        }
        for v in self.smp.iter_mut().filter(|v| v.active) {
            let data = &self.bank.samples[v.idx].data;
            let b = v.bus as usize;
            let dg = (g1[b] - g0[b]) / n as f32;
            let mut g = g0[b];
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
                v.pos += v.rate;
                g += dg;
            }
        }
        // Reverb, then a gentle limiter, then a soft safety clip.
        let rel = per_sample_fast(n);
        for i in 0..n {
            let (wl, wr) = self.verb.process(send[i]);
            let xl = l[i] + wl;
            let xr = r[i] + wr;
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

/// Limiter release per sample (~150 ms).
fn per_sample_fast(_n: usize) -> f32 {
    1.0 - (-INV_RATE / 0.15).exp()
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

/// A small Schroeder/Freeverb-style room: four damped combs and two
/// allpasses per side, a few thousand floats of state in all.
struct Reverb {
    combs: [Comb; 8],
    aps: [Allpass; 4],
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
        self.store = y * 0.7 + self.store * 0.3;
        self.buf[self.i] = x + self.store * 0.8;
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
    fn new() -> Self {
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
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> (f32, f32) {
        let x = x * 0.25;
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
    fn crash_is_safe() {
        let c = crash();
        assert!(c.iter().all(|x| x.is_finite() && x.abs() <= 0.3));
    }
}
