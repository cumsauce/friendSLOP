//! Planning (durations, stress, hat-pattern pitch) and rendering (glottal source into a formant
//! cascade) of a line of speech.

use std::f32::consts::{PI, TAU};

use serde::Deserialize;

use crate::markup::{self, Chunk};
use crate::text::{self, PARTS, PAUSES, SOUNDS, VOWELS};

/// One way of saying a line. `1.0` (or `0.0` for the additive ones) is a flat, neutral voice.
/// Moods are data (`data/base/speech.ron`); blend two with [`Delivery::lerp`].
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Delivery {
    /// Multiplies the voice's pitch.
    pub pitch_mult: f32,
    /// Multiplies how far the pitch moves for accents and the end of a sentence.
    pub range_mult: f32,
    /// Multiplies the speed.
    pub rate_mult: f32,
    /// Multiplies how much longer stressed sounds are.
    pub stress_len: f32,
    /// Semitones the pitch sinks over a sentence.
    pub declination: f32,
    /// Open quotient of the glottal pulse: low is tense and bright, high is lax and soft.
    pub oq: f32,
    /// How much the source is dulled, 0 (bright) to 1.
    pub tilt: f32,
    /// Breathy noise in the voiced source, 0..1.
    pub breath: f32,
    /// Random change of each pitch period, as a fraction (0.01 is one percent).
    pub jitter: f32,
    /// Random change of each period's loudness, as a fraction.
    pub shimmer: f32,
    /// Slow wander of the pitch, in semitones.
    pub drift: f32,
    /// Vibrato on held vowels: depth in semitones, and rate in Hz.
    pub vibrato_depth: f32,
    pub vibrato_rate: f32,
    /// How far vowels reach their targets: below 1 mumbles, above 1 enunciates.
    pub precision: f32,
    /// Semitones added to where a sentence ends (a question-like lift when positive).
    pub final_rise: f32,
    /// Multiplies the length of pauses.
    pub pause_mult: f32,
    /// Multiplies the loudness of stop bursts.
    pub burst: f32,
    /// Nasal colour on every vowel, 0..1.
    pub nasal: f32,
}

impl Default for Delivery {
    fn default() -> Self {
        Self {
            pitch_mult: 1.0,
            range_mult: 1.0,
            rate_mult: 1.0,
            stress_len: 1.0,
            declination: 3.0,
            oq: 0.6,
            tilt: 0.25,
            breath: 0.08,
            jitter: 0.008,
            shimmer: 0.02,
            drift: 0.5,
            vibrato_depth: 0.0,
            vibrato_rate: 5.5,
            precision: 1.0,
            final_rise: 0.0,
            pause_mult: 1.0,
            burst: 1.0,
            nasal: 0.0,
        }
    }
}

impl Delivery {
    /// `self` at `t == 0`, `other` at `t == 1`.
    pub fn lerp(&self, other: &Delivery, t: f32) -> Delivery {
        let t = t.clamp(0.0, 1.0);
        let m = |a: f32, b: f32| a + (b - a) * t;
        Delivery {
            pitch_mult: m(self.pitch_mult, other.pitch_mult),
            range_mult: m(self.range_mult, other.range_mult),
            rate_mult: m(self.rate_mult, other.rate_mult),
            stress_len: m(self.stress_len, other.stress_len),
            declination: m(self.declination, other.declination),
            oq: m(self.oq, other.oq),
            tilt: m(self.tilt, other.tilt),
            breath: m(self.breath, other.breath),
            jitter: m(self.jitter, other.jitter),
            shimmer: m(self.shimmer, other.shimmer),
            drift: m(self.drift, other.drift),
            vibrato_depth: m(self.vibrato_depth, other.vibrato_depth),
            vibrato_rate: m(self.vibrato_rate, other.vibrato_rate),
            precision: m(self.precision, other.precision),
            final_rise: m(self.final_rise, other.final_rise),
            pause_mult: m(self.pause_mult, other.pause_mult),
            burst: m(self.burst, other.burst),
            nasal: m(self.nasal, other.nasal),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Voice {
    pub sample_rate: u32,
    /// Hz at the start of a sentence, before the delivery's multiplier.
    pub pitch: f32,
    /// 2.0 talks twice as fast.
    pub speed: f32,
    /// Another seed is another throat: the random jitter and breath come out differently.
    pub seed: u32,
    pub delivery: Delivery,
}

impl Default for Voice {
    fn default() -> Self {
        Self::new()
    }
}

// Indices into a sound's targets.
const N: usize = 11;
const DUR: usize = 0;
const F1: usize = 1;
const F2: usize = 2;
const F3: usize = 3;
const VOICE: usize = 4;
const ASP: usize = 5;
const FRIC: usize = 6;
#[allow(dead_code)]
const CENTRE: usize = 7;
const GAIN: usize = 8;
const NASAL: usize = 9;
const ZERO: usize = 10;

/// Formants a relaxed, unstressed vowel drifts toward when the speaker undershoots.
const SCHWA: [f32; 3] = [500.0, 1500.0, 2500.0];
const VOICELESS: [&str; 9] = ["P", "T", "K", "F", "TH", "S", "SH", "CH", "HH"];
const VOICED: [&str; 8] = ["B", "D", "G", "V", "DH", "Z", "ZH", "JH"];
const BURSTS: [&str; 3] = ["PB", "TB", "KB"];

struct Seg {
    t: [f32; N],
    vowel: bool,
    /// Where the pitch wants to be right after a consonant (semitones).
    micro: f32,
}

struct Sentence {
    segs: Vec<Seg>,
    /// Pitch contour: (milliseconds into the sentence, semitones).
    anchors: Vec<(f32, f32)>,
}

struct Tok {
    name: String,
    stressed: bool,
    emph: u8,
}

impl Voice {
    pub fn new() -> Self {
        Self {
            sample_rate: 22_050,
            pitch: 95.0,
            speed: 1.0,
            seed: 1,
            delivery: Delivery::default(),
        }
    }

    fn step(&self) -> f32 {
        1000.0 * (self.speed * self.delivery.rate_mult).max(0.1) / self.sample_rate as f32
    }

    /// How many seconds `speak` will make of `text`, without rendering it.
    pub fn duration(&self, text: &str) -> f32 {
        let step = self.step();
        let samples: usize = self
            .plan(text)
            .iter()
            .flat_map(|s| &s.segs)
            .map(|seg| (seg.t[DUR] / step) as usize)
            .sum();
        samples as f32 / self.sample_rate as f32
    }

    fn tokens(&self, text: &str) -> Vec<Tok> {
        let mut toks = vec![];
        for chunk in markup::parse(text) {
            match chunk {
                Chunk::Pause(ms) => toks.push(Tok {
                    name: format!("~{ms}"),
                    stressed: false,
                    emph: 0,
                }),
                Chunk::Say { text, emph } => {
                    let start = toks.len();
                    for token in text::phonemes(&text).split_whitespace() {
                        let (name, stressed) = token
                            .strip_suffix('1')
                            .map_or((token, false), |n| (n, true));
                        toks.push(Tok {
                            name: name.to_owned(),
                            stressed,
                            emph,
                        });
                    }
                    // A stressed word is stressed even where the rules call it a little word.
                    if emph > 0 && !toks[start..].iter().any(|t| t.stressed) {
                        let first = toks[start..].iter().position(|t| {
                            is_vowel(&t.name) && !["AX", "IX"].contains(&t.name.as_str())
                        });
                        if let Some(i) = first {
                            toks[start + i].stressed = true;
                        }
                    }
                    for tok in &mut toks[start..] {
                        tok.emph = emph;
                    }
                }
            }
        }
        if toks
            .last()
            .is_some_and(|t| !t.name.starts_with(PAUSES) && !t.name.starts_with('~'))
        {
            toks.push(Tok {
                name: ",".into(),
                stressed: false,
                emph: 0,
            });
        }
        toks
    }

    fn plan(&self, text: &str) -> Vec<Sentence> {
        let d = &self.delivery;
        let toks = self.tokens(text);
        let mut sentences = vec![];
        let mut at = 0;
        while at < toks.len() {
            let end = toks[at..]
                .iter()
                .position(|t| t.name.starts_with(['.', '!', '?']))
                .map_or(toks.len(), |i| at + i + 1);
            sentences.push(self.sentence(&toks[at..end], d));
            at = end;
        }
        sentences
    }

    fn sentence(&self, toks: &[Tok], d: &Delivery) -> Sentence {
        let stop = toks
            .last()
            .and_then(|t| t.name.chars().next())
            .filter(|c| ['.', '!', '?'].contains(c));
        // The last vowel before a pause, and what follows it, is drawn out.
        let mut last = true;
        let mut stretch = vec![1.0; toks.len()];
        for (i, tok) in toks.iter().enumerate().rev() {
            let pause = is_pause(&tok.name);
            stretch[i] = if last && !pause { 1.4 } else { 1.0 };
            last = pause || last && !is_vowel(&tok.name);
        }
        let mut segs: Vec<Seg> = vec![];
        let mut accents: Vec<(f32, f32, f32)> = vec![]; // start ms, length ms, weight
        let mut now = 0.0;
        let mut prev = Prev::Other;
        for (tok, stretch) in toks.iter().zip(stretch) {
            if is_pause(&tok.name) {
                let ms = pause_ms(&tok.name) * d.pause_mult;
                segs.push(Seg {
                    t: [ms, 500.0, 1500.0, 2500.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                    vowel: false,
                    micro: 0.0,
                });
                now += ms;
                prev = Prev::Other;
                continue;
            }
            let vowel = is_vowel(&tok.name);
            let (scale, weight) = match (tok.stressed, vowel, tok.emph) {
                (true, _, 2) => (1.3 * d.stress_len * 1.15, 2.0),
                (true, _, 1) => (1.3 * d.stress_len * 1.1, 1.5),
                (true, _, _) => (1.3 * d.stress_len, 1.0),
                (_, true, _) => (0.8, 0.0),
                _ => (1.0, 0.0),
            };
            let gain = match (tok.stressed, vowel, tok.emph) {
                (_, _, 2) => 1.35,
                (_, _, 1) => 1.15,
                (false, true, _) => 0.82,
                _ => 1.0,
            };
            let micro = match prev {
                Prev::Voiceless => 1.2,
                Prev::Voiced => -0.6,
                Prev::Other => 0.0,
            };
            let start = now;
            let mut subs = vec![];
            expand(&tok.name, 1.0, &mut subs);
            for (name, s) in subs {
                let sound_vowel = vowel && !is_consonant_part(name);
                let mut t = [0.0; N];
                t[..8].copy_from_slice(&s);
                t[DUR] *= stretch * scale;
                t[GAIN] = gain;
                if sound_vowel {
                    // Undershoot (or overshoot) toward the relaxed vowel.
                    let reach =
                        (d.precision * if tok.stressed { 1.0 } else { 0.9 }).clamp(0.4, 1.4);
                    for (f, schwa) in [F1, F2, F3].into_iter().zip(SCHWA) {
                        t[f] = schwa + (t[f] - schwa) * reach;
                    }
                    t[NASAL] = d.nasal;
                }
                match name {
                    "M" | "N" | "NG" => {
                        t[NASAL] = 1.0;
                        t[ZERO] = match name {
                            "M" => 1000.0,
                            "N" => 1500.0,
                            _ => 2000.0,
                        };
                    }
                    n if BURSTS.contains(&n) => t[FRIC] *= d.burst,
                    _ => {}
                }
                if t[ZERO] == 0.0 {
                    t[ZERO] = 1500.0;
                }
                now += t[DUR];
                segs.push(Seg {
                    t,
                    vowel: sound_vowel,
                    micro: if sound_vowel { micro } else { 0.0 },
                });
            }
            if vowel && weight > 0.0 {
                accents.push((start, now - start, weight));
            }
            prev = if VOICELESS.contains(&tok.name.as_str()) {
                Prev::Voiceless
            } else if VOICED.contains(&tok.name.as_str()) {
                Prev::Voiced
            } else {
                Prev::Other
            };
        }
        // Every sentence ends in a pause; the contour finishes before it.
        let span: f32 = segs.iter().rev().skip(1).map(|s| s.t[DUR]).sum();
        let anchors = contour(span, &accents, stop, d);
        Sentence { segs, anchors }
    }

    /// Says `text` in this voice: mono samples at `sample_rate`, peaks at 0.9.
    pub fn speak(&self, text: &str) -> Vec<f32> {
        let mut r = Renderer::new(self);
        let mut out = vec![];
        for sentence in self.plan(text) {
            r.sentence(self, &sentence, &mut out);
        }
        let peak = out
            .iter()
            .fold(f32::MIN_POSITIVE, |peak, s| peak.max(s.abs()));
        out.iter().map(|s| s * 0.9 / peak).collect()
    }
}

#[derive(Clone, Copy)]
enum Prev {
    Voiceless,
    Voiced,
    Other,
}

fn is_vowel(name: &str) -> bool {
    name.starts_with(VOWELS)
}

fn is_pause(name: &str) -> bool {
    name.starts_with(PAUSES) || name.starts_with('~')
}

/// The sub-sounds of a stop or diphthong that aren't the vowel part of the token.
fn is_consonant_part(name: &str) -> bool {
    !name.starts_with(VOWELS)
}

fn pause_ms(name: &str) -> f32 {
    match name {
        "," => 150.0,
        n if n.starts_with('~') => n[1..].parse().unwrap_or(300.0),
        _ => 400.0,
    }
}

/// A token's sounds with their names: a stop is closure, burst and breath, a diphthong two vowels.
fn expand(name: &str, scale: f32, out: &mut Vec<(&'static str, [f32; 8])>) {
    if let Some((_, parts)) = PARTS.iter().find(|(part, _)| *part == name) {
        for part in parts.split(' ') {
            expand(part, scale * 0.7, out);
        }
    } else if let Some((n, s)) = SOUNDS.iter().find(|(sound, _)| *sound == name) {
        let mut s = *s;
        s[0] *= scale;
        out.push((n, s));
    }
}

/// The hat pattern: a baseline that sinks over the sentence, a peak on each stressed vowel (each
/// lower than the last, the final one raised), and a tone for how the sentence ends.
fn contour(
    span: f32,
    accents: &[(f32, f32, f32)],
    stop: Option<char>,
    d: &Delivery,
) -> Vec<(f32, f32)> {
    let span = span.max(1.0);
    let base = |t: f32| d.declination * (0.5 - t / span);
    let mut pts: Vec<(f32, f32)> = vec![(0.0, base(0.0))];
    let height = 2.8 * d.range_mult;
    let count = accents.len();
    for (k, (start, len, weight)) in accents.iter().enumerate() {
        let mut h = height * weight * 0.88f32.powi(k as i32);
        if k + 1 == count && count > 1 {
            h *= 1.2;
        }
        let peak = start + 0.7 * len;
        pts.push((start - 25.0, base(start - 25.0)));
        pts.push((peak, base(peak) + h));
        pts.push((peak + 160.0, base(peak + 160.0) + 0.3 * h));
    }
    let r = d.range_mult;
    let tail = |back: f32| (span - back.min(span)).max(0.0);
    match stop {
        Some('?') => {
            pts.push((tail(450.0), base(tail(450.0)) - 0.5));
            pts.push((span, base(span) + 6.0 * r + d.final_rise));
        }
        Some('!') => {
            pts.push((tail(350.0), base(tail(350.0)) + 2.0 * r));
            pts.push((span, base(span) - 4.0 * r + d.final_rise));
        }
        Some(_) => {
            pts.push((tail(300.0), base(tail(300.0))));
            pts.push((span, base(span) - 5.0 * r + d.final_rise));
        }
        None => {
            pts.push((tail(250.0), base(tail(250.0))));
            pts.push((span, base(span) + 1.0 * r + d.final_rise));
        }
    }
    pts.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f32, f32)> = vec![];
    for p in pts {
        if out.last().is_none_or(|l| p.0 > l.0 + 1.0) {
            out.push(p);
        }
    }
    out
}

fn at_anchor(anchors: &[(f32, f32)], cursor: &mut usize, t: f32) -> f32 {
    while *cursor + 1 < anchors.len() && t >= anchors[*cursor + 1].0 {
        *cursor += 1;
    }
    match (anchors.get(*cursor), anchors.get(*cursor + 1)) {
        (Some(a), Some(b)) if t > a.0 => {
            let f = ((t - a.0) / (b.0 - a.0)).clamp(0.0, 1.0);
            a.1 + (b.1 - a.1) * (0.5 - 0.5 * (PI * f).cos())
        }
        (Some(a), _) => a.1,
        _ => 0.0,
    }
}

/// Two-pole resonance at `f` Hz, `bw` Hz wide, passing 0 Hz unchanged.
fn resonate(mem: &mut [f32; 2], x: f32, f: f32, bw: f32, rate: f32) -> f32 {
    let r = (-PI * bw / rate).exp();
    let b = 2.0 * r * (TAU * f / rate).cos();
    let y = (1.0 - b + r * r) * x + b * mem[0] - r * r * mem[1];
    *mem = [y, mem[0]];
    y
}

/// A notch at `f` Hz with unit gain at 0 Hz: the nasals' anti-resonance.
fn notch(mem: &mut [f32; 4], x: f32, f: f32, rate: f32) -> f32 {
    let c = (TAU * f / rate).cos();
    let rp = 0.92f32;
    let k = (1.0 - 2.0 * rp * c + rp * rp) / (2.0 - 2.0 * c);
    let y = k * (x - 2.0 * c * mem[0] + mem[1]) + 2.0 * rp * c * mem[2] - rp * rp * mem[3];
    *mem = [x, mem[0], y, mem[2]];
    y
}

/// Everything that carries from one sample to the next, and across sentences.
struct Renderer {
    rate: f32,
    now: [f32; N],
    tract: [[f32; 2]; 6],
    nasal_mem: [f32; 4],
    phase: f32,
    period: f32,
    amp: f32,
    seed: u32,
    lift: f32,
    drift: f32,
    tilt: f32,
    vib_phase: f32,
    vowel_time: f32,
}

impl Renderer {
    fn new(voice: &Voice) -> Self {
        Self {
            rate: voice.sample_rate as f32,
            now: [0.0; N],
            tract: [[0.0; 2]; 6],
            nasal_mem: [0.0; 4],
            phase: 0.0,
            period: 1.0,
            amp: 1.0,
            seed: voice.seed.max(1),
            lift: 0.0,
            drift: 0.0,
            tilt: 0.0,
            vib_phase: 0.0,
            vowel_time: 0.0,
        }
    }

    fn noise(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        self.seed as f32 / u32::MAX as f32 * 2.0 - 1.0
    }

    fn sentence(&mut self, voice: &Voice, s: &Sentence, out: &mut Vec<f32>) {
        let d = &voice.delivery;
        let rate = self.rate;
        let step = voice.step();
        let (mut elapsed, mut cursor) = (0.0f32, 0usize);
        // A slow low-pass on noise makes the pitch wander; this scales it to about d.drift.
        let drift_a = 2.0 / rate;
        let drift_gain = 1.0 / (0.577 * (drift_a / (2.0 - drift_a)).sqrt());
        let (oq, speed_q) = (d.oq.clamp(0.3, 0.95), 0.6);
        let (rise, fall) = (oq * speed_q, oq * (1.0 - speed_q));
        let tilt_a = 1.0 / (1.0 + d.tilt.max(0.0) * 6.0);
        for seg in &s.segs {
            let target = seg.t;
            for _ in 0..(target[DUR] / step) as usize {
                let t = elapsed;
                elapsed += step;
                let contour = at_anchor(&s.anchors, &mut cursor, t);
                self.lift += (seg.micro - self.lift) * (1000.0 / 30.0) / rate;
                let n = self.noise();
                self.drift += (n - self.drift) * drift_a;
                let wander = (self.drift * drift_gain * d.drift).clamp(-2.5, 2.5);
                self.vowel_time = if seg.vowel {
                    self.vowel_time + 1.0 / rate
                } else {
                    0.0
                };
                let vib = if d.vibrato_depth > 0.0 {
                    self.vib_phase = (self.vib_phase + d.vibrato_rate / rate).fract();
                    let grow = ((self.vowel_time - 0.12) * 6.0).clamp(0.0, 1.0);
                    d.vibrato_depth * grow * (TAU * self.vib_phase).sin()
                } else {
                    0.0
                };
                let hz = voice.pitch.max(0.0)
                    * d.pitch_mult
                    * 2f32.powf((contour + self.lift + wander + vib) / 12.0);
                // Glide every target toward the sound, the source levels faster than the formants.
                for (i, (now, goal)) in self.now.iter_mut().zip(target).enumerate() {
                    if i == DUR {
                        continue;
                    }
                    let k = match i {
                        VOICE | ASP | FRIC => 300.0,
                        GAIN | NASAL | ZERO => 150.0,
                        _ => 100.0,
                    };
                    *now += (goal - *now) * k / rate;
                }
                // One glottal pulse per period, each a little shorter or longer, softer or louder.
                self.phase += hz / rate / self.period;
                if self.phase >= 1.0 {
                    self.phase -= 1.0;
                    let (j, sh) = (self.noise(), self.noise());
                    self.period = (1.0 + d.jitter * j).max(0.5);
                    self.amp = (1.0 + d.shimmer * sh).max(0.3);
                }
                let p = self.phase;
                let (flow, slope) = if p < rise {
                    (
                        0.5 - 0.5 * (PI * p / rise).cos(),
                        0.5 * PI / rise * (PI * p / rise).sin(),
                    )
                } else if p < oq {
                    let q = (p - rise) / fall;
                    (
                        (0.5 * PI * q).cos(),
                        -0.5 * PI / fall * (0.5 * PI * q).sin(),
                    )
                } else {
                    (0.0, 0.0)
                };
                // The differentiated flow, scaled so its sharpest point (the closing) is about 1.
                let raw = slope / (0.5 * PI / fall);
                self.tilt += (raw - self.tilt) * tilt_a;
                let [
                    _,
                    f1,
                    f2,
                    f3,
                    voicing,
                    aspiration,
                    frication,
                    centre,
                    gain,
                    nasal,
                    zero,
                ] = self.now;
                let breath = d.breath * 0.4 * voicing * (0.2 + 0.8 * flow);
                let source = voicing * self.amp * self.tilt + (aspiration + breath) * n;
                let widen = 1.0 + 1.5 * d.breath;
                let voiced = [f1, f2, f3, 3600.0, 3750.0]
                    .into_iter()
                    .zip([60.0 * widen, 90.0 + 40.0 * d.breath, 150.0, 150.0, 80.0])
                    .zip(&mut self.tract)
                    .fold(source, |x, ((f, bw), mem)| resonate(mem, x, f, bw, rate));
                let notched = notch(&mut self.nasal_mem, voiced, zero.max(300.0), rate);
                let voiced = voiced + (notched - voiced) * nasal.clamp(0.0, 1.0) * 0.8;
                let n2 = self.noise();
                let hiss = resonate(
                    &mut self.tract[5],
                    n2,
                    centre.max(200.0),
                    centre.max(200.0) / 4.0,
                    rate,
                );
                out.push(gain * (voiced + frication * hiss));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speech_is_audible_and_bounded() {
        let samples = Voice::new().speak("Hello there! How are you?");
        assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        assert!(samples.iter().any(|s| s.abs() > 0.5));
        assert!(Voice::new().speak("").is_empty());
    }

    #[test]
    fn duration_matches_the_render() {
        let v = Voice::new();
        for text in [
            "Hi.",
            "Next stop, Chambers Street. Stand clear!",
            "well... *no*",
        ] {
            let len = v.speak(text).len() as f32 / v.sample_rate as f32;
            assert!((v.duration(text) - len).abs() < 1e-3, "{text}");
        }
    }

    #[test]
    fn every_phoneme_renders() {
        let v = Voice::new();
        let used = text::RULES
            .iter()
            .map(|(.., p)| *p)
            .chain(text::LETTERS)
            .flat_map(str::split_whitespace);
        for name in used {
            let name = name.trim_end_matches('1');
            let sentence = v.sentence(
                &[
                    Tok {
                        name: name.into(),
                        stressed: true,
                        emph: 0,
                    },
                    Tok {
                        name: ".".into(),
                        stressed: false,
                        emph: 0,
                    },
                ],
                &v.delivery,
            );
            assert!(!sentence.segs.is_empty(), "{name}");
        }
    }

    #[test]
    fn faster_moods_are_shorter_and_higher() {
        let slow = Voice {
            delivery: Delivery {
                rate_mult: 0.8,
                pitch_mult: 0.9,
                ..Delivery::default()
            },
            ..Voice::new()
        };
        let fast = Voice {
            delivery: Delivery {
                rate_mult: 1.2,
                pitch_mult: 1.1,
                ..Delivery::default()
            },
            ..Voice::new()
        };
        let line = "I told you not to call me again.";
        assert!(fast.duration(line) < slow.duration(line));
    }

    #[test]
    fn questions_rise_and_statements_fall() {
        let d = Delivery::default();
        let end = |stop| {
            let a = contour(1500.0, &[(300.0, 120.0, 1.0)], stop, &d);
            a.last().map(|p| p.1).unwrap_or(0.0)
        };
        assert!(end(Some('?')) > end(Some('.')) + 6.0);
        assert!(end(Some('.')) < 0.0);
        assert!(end(None) > end(Some('.')));
    }

    #[test]
    fn contour_peaks_on_stressed_vowels_and_steps_down() {
        let d = Delivery::default();
        let a = contour(
            2000.0,
            &[(200.0, 100.0, 1.0), (900.0, 100.0, 1.0)],
            Some('.'),
            &d,
        );
        let mut cursor = 0;
        let first = at_anchor(&a, &mut cursor, 270.0);
        let mut cursor = 0;
        let second = at_anchor(&a, &mut cursor, 970.0);
        let mut cursor = 0;
        let between = at_anchor(&a, &mut cursor, 600.0);
        assert!(first > between && second > between);
    }

    #[test]
    fn jitter_changes_the_waveform_but_not_the_length() {
        let steady = Voice {
            delivery: Delivery {
                jitter: 0.0,
                shimmer: 0.0,
                drift: 0.0,
                ..Delivery::default()
            },
            ..Voice::new()
        };
        let rough = Voice {
            delivery: Delivery {
                jitter: 0.03,
                shimmer: 0.08,
                drift: 1.0,
                ..Delivery::default()
            },
            ..Voice::new()
        };
        let (a, b) = (steady.speak("aaa"), rough.speak("aaa"));
        assert_eq!(a.len(), b.len());
        assert_ne!(a, b);
    }

    #[test]
    fn breathy_voices_have_more_noise_in_the_open_phase() {
        let clean = Voice {
            delivery: Delivery {
                breath: 0.0,
                ..Delivery::default()
            },
            ..Voice::new()
        };
        let breathy = Voice {
            delivery: Delivery {
                breath: 1.0,
                ..Delivery::default()
            },
            ..Voice::new()
        };
        assert_ne!(clean.speak("oh"), breathy.speak("oh"));
    }

    #[test]
    fn emphasis_stresses_a_little_word() {
        let v = Voice::new();
        let toks = v.tokens("it was *you*");
        assert!(
            toks.iter()
                .any(|t| t.name == "UW" && t.stressed && t.emph == 1)
        );
        assert!(v.duration("it was *you*") > v.duration("it was you"));
    }

    #[test]
    fn lerp_blends_deliveries() {
        let a = Delivery::default();
        let b = Delivery {
            pitch_mult: 1.4,
            ..Delivery::default()
        };
        assert!((a.lerp(&b, 0.5).pitch_mult - 1.2).abs() < 1e-6);
        assert_eq!(a.lerp(&b, 0.0), a);
        assert_eq!(a.lerp(&b, 1.0), b);
    }
}
