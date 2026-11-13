//! Melody: the lead's motifs and runs, answers, kimes and the signature theme.

use super::*;

/// The signature hook, written out (over "Just the Two of Us" in Db):
/// per bar, (16th step, length in 16ths, MIDI note).
pub(super) const THEME: [&[(u8, u8, u8)]; 4] = [
    &[(0, 2, 70), (2, 2, 73), (4, 5, 77), (10, 2, 75), (12, 4, 73)],
    &[(2, 2, 69), (4, 2, 72), (6, 4, 75), (10, 2, 73), (12, 4, 72)],
    &[(0, 3, 73), (3, 3, 77), (6, 2, 80), (8, 4, 77)],
    &[
        (0, 2, 75),
        (2, 2, 78),
        (4, 2, 80),
        (6, 2, 78),
        (8, 2, 77),
        (10, 2, 75),
        (12, 4, 70),
    ],
];
pub(super) const THEME_KEY: u8 = 1;

/// Syncopated one-bar rhythm cells for motifs: (step, length).
pub(super) const CELLS: [&[(u8, u8)]; 8] = [
    &[(0, 2), (2, 2), (4, 4), (10, 2), (12, 4)],
    &[(2, 1), (3, 1), (4, 2), (6, 4), (12, 3)],
    &[(0, 3), (3, 3), (6, 2), (8, 6)],
    &[(0, 1), (1, 1), (2, 1), (3, 3), (6, 1), (7, 5)],
    &[(4, 2), (6, 2), (8, 2), (10, 6)],
    &[(0, 6), (6, 2), (8, 2), (10, 2), (12, 4)],
    &[(1, 2), (3, 3), (6, 3), (9, 3), (12, 4)],
    &[(0, 2), (3, 2), (6, 2), (8, 8)],
];

/// Unison hit rhythms (steps) for kimes, 4/4 and 7/8.
pub(super) const KIMES: [&[u8]; 3] = [&[0, 3, 6, 10, 12], &[0, 2, 5, 8, 11, 14], &[0, 3, 6, 8, 12]];
pub(super) const KIME_78: &[u8] = &[0, 2, 4, 7, 10, 12];

pub(super) const LEAD_LO: u8 = 62;
pub(super) const LEAD_HI: u8 = 84;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct MNote {
    pub(super) step: u8,
    pub(super) len: u8,
    /// Scale steps from the motif's anchor.
    pub(super) c: i8,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Motif {
    pub(super) n: usize,
    pub(super) notes: [MNote; 10],
}

/// A line of absolute notes for one bar: (step, length, MIDI note).
#[derive(Clone, Copy, Debug)]
pub(super) struct Line {
    pub(super) n: usize,
    pub(super) notes: [(u8, u8, u8); 20],
}

impl Line {
    pub(super) fn new() -> Self {
        Self {
            n: 0,
            notes: [(0, 0, 0); 20],
        }
    }
    pub(super) fn push(&mut self, step: u8, len: u8, note: u8) {
        if self.n < self.notes.len() {
            self.notes[self.n] = (step, len, note);
            self.n += 1;
        }
    }
}

impl Composer {
    /// Call and response: when the lead leaves a gap, a second voice echoes
    /// its last few notes in the same rhythm, a third or a sixth lower and
    /// fitted to the chords now sounding.
    pub(super) fn write_answer(&mut self, bar: &mut Bar) {
        let s = self.style();
        let Some(inst) = s.answer else {
            return;
        };
        let kind = self.plan.kind;
        if !matches!(kind, Section::A | Section::Hook | Section::B) || self.frag.n < 2 {
            return;
        }
        let steps = self.steps;
        let from = if self.lead_end > 0 {
            self.lead_end + 1
        } else {
            0
        };
        let from = from.next_multiple_of(2);
        if from + 5 > steps || !self.chance(0.45 + 0.35 * self.intensity) {
            return;
        }
        let (lo, hi) = match inst {
            who if [pk().poly_brass, pk().epiano, pk().clav, pk().organ].contains(&who) => (60, 76),
            who if who == pk().pan_flute => (64, 84),
            who if who == pk().strings => (57, 74),
            // Blown bottles live lower than a flute.
            who if who == pk().flute => (60, 79),
            _ => (62, 86),
        };
        let k = self.frag.n.min(3);
        let notes = self.frag.notes;
        let first = notes[self.frag.n - k];
        let down = if self.chance(0.6) { -2 } else { -5 };
        let mut prev = 0u8;
        for &(st, ln, n) in &notes[self.frag.n - k..self.frag.n] {
            let at_step = from + (st - first.0) as u32;
            if at_step >= steps {
                break;
            }
            let ch = self.chord(at_step);
            let mut note = ch.scale_step(n, down);
            while note > hi {
                note -= 12;
            }
            while note < lo {
                note += 12;
            }
            if at_step.is_multiple_of(4) || ln >= 3 {
                note = ch.nearest_strong(note);
            }
            if note == prev {
                note = ch.scale_step(note, -1);
            }
            prev = note;
            let at = self.at(at_step);
            let len = self.len((ln as f32).min((steps - at_step) as f32) * 0.95);
            Self::push(
                bar,
                Ev {
                    at,
                    inst,
                    note,
                    vel: 0.62 + 0.15 * self.intensity,
                    len,
                    glide: false,
                },
            );
        }
    }

    // --- Horns ------------------------------------------------------------
}

impl Composer {
    /// A unison hit figure: bass, keys, lead and kit together, ending on a
    /// held chord.
    pub(super) fn write_kime(&mut self, bar: &mut Bar, crash: bool) {
        let rhythm = if self.steps == 14 {
            KIME_78
        } else {
            KIMES[self.pick(KIMES.len())]
        };
        let lead = self.lead_inst();
        // The lead line walks down the chord tones from high up.
        let mut top = self
            .chord(rhythm[0] as u32)
            .nearest_strong(self.last_note.max(74));
        let steps = self.steps;
        for (k, &st) in rhythm.iter().enumerate() {
            let st = st as u32;
            if st >= steps {
                break;
            }
            let lastk = k + 1 == rhythm.len() || rhythm[k + 1] as u32 >= steps;
            let next = if lastk { steps } else { rhythm[k + 1] as u32 };
            let ch = self.chord(st);
            let at = self.at(st);
            let len_steps = if lastk {
                (steps - st) as f32 + 1.0
            } else {
                ((next - st) as f32 * 0.6).max(0.6)
            };
            let len = self.len(len_steps);
            // Kit.
            Self::push(bar, hit(at, pk().kick, 36, 1.0));
            Self::push(bar, hit(at, pk().snare, 38, 0.9));
            if k == 0 && crash || lastk {
                Self::push(bar, hit(at, pk().crash, 49, 0.85));
            }
            // Bass in octaves with the lead's pitch class, else the root.
            let root = ch.root_from(31);
            Self::push(
                bar,
                Ev {
                    at,
                    inst: pk().bass,
                    note: if k % 2 == 1 { root + 12 } else { root },
                    vel: 1.0,
                    len,
                    glide: false,
                },
            );
            // Keys stab.
            self.ep = voice_lead(ch, &self.ep, 50, 62);
            let keys = self.style().keys;
            for &n in &{ self.ep } {
                Self::push(
                    bar,
                    Ev {
                        at,
                        inst: keys,
                        note: n,
                        vel: 0.9,
                        len,
                        glide: false,
                    },
                );
            }
            // Horns hit with the band.
            if self.style().horns != Horns::None {
                self.horn = no_top_rub(voice_lead(ch, &self.horn, 60, 69));
                let horn = self.horn;
                for &n in &horn[1..] {
                    Self::push(
                        bar,
                        Ev {
                            at,
                            inst: pk().poly_brass,
                            note: n,
                            vel: 0.95,
                            len,
                            glide: false,
                        },
                    );
                }
            }
            // Lead.
            top = ch.nearest_strong(top);
            while top > LEAD_HI {
                top -= 12;
            }
            #[cfg(test)]
            if st.is_multiple_of(4) {
                self.strong_log.push((top, ch));
            }
            if let Some(inst) = lead {
                Self::push(
                    bar,
                    Ev {
                        at,
                        inst,
                        note: top,
                        vel: 1.0,
                        len,
                        glide: false,
                    },
                );
            }
            self.last_note = top;
            // Next hit: a chord tone a step or two below.
            let next_ch = self.chord(next.min(steps.saturating_sub(1)));
            top = next_ch.nearest_strong(top.saturating_sub(2).max(LEAD_LO + 2));
            if top == self.last_note {
                top = next_ch.nearest_strong(top.saturating_sub(3));
            }
        }
    }

    // --- Lead -----------------------------------------------------------
}

impl Composer {
    pub(super) fn lead_inst(&self) -> Option<Inst> {
        // In solos the band's two synth players trade every two bars.
        let trade = self.plan.kind == Section::Solo && (self.plan.bar / 2) % 2 == 1;
        match self.style().lead {
            LeadKind::Synth if trade => Some(pk().mini),
            LeadKind::Mini | LeadKind::Brass if trade => Some(pk().lead),
            LeadKind::Synth => Some(pk().lead),
            LeadKind::Brass => Some(pk().brass),
            LeadKind::Flute => Some(pk().flute),
            LeadKind::Bell => Some(pk().bell),
            LeadKind::Mini => Some(pk().mini),
            LeadKind::Silent => None,
        }
    }
}

impl Composer {
    pub(super) fn new_motif(&mut self) -> Motif {
        // A motif needs a shape you can hear: retry flat ones.
        let mut m = self.motif_try();
        for _ in 0..4 {
            let cs = m.notes[..m.n].iter().map(|x| x.c);
            let range = cs.clone().max().unwrap_or(0) - cs.min().unwrap_or(0);
            if range >= 3 {
                break;
            }
            m = self.motif_try();
        }
        m
    }
}

impl Composer {
    pub(super) fn motif_try(&mut self) -> Motif {
        let cell = CELLS[self.pick(CELLS.len())];
        let mut m = Motif::default();
        let mut c: i8 = 0;
        let n = cell.len().min(m.notes.len());
        for (k, &(step, len)) in cell.iter().take(n).enumerate() {
            m.notes[k] = MNote { step, len, c };
            // Arch contour: rise through the first half, fall after.
            let rising = k * 2 < n;
            let leap = [1i8, 1, 2, 2, 3][self.pick(5)];
            let dir = if self.chance(if rising { 0.75 } else { 0.25 }) {
                1
            } else {
                -1
            };
            c = (c + dir * leap).clamp(-5, 6);
        }
        m.n = n;
        m
    }
}

impl Composer {
    /// Play a motif against the current chords, starting near `from`.
    pub(super) fn realize(&mut self, m: &Motif, from: u8, shift: i8, invert: bool) -> Line {
        let mut line = Line::new();
        if m.n == 0 {
            return line;
        }
        let first = self.chord(m.notes[0].step as u32);
        let anchor = first.nearest_strong(fold(from));
        for &mn in m.notes.iter().take(m.n) {
            let step = mn.step as i32 + shift as i32;
            if !(0..self.steps as i32).contains(&step) {
                continue;
            }
            let ch = self.chord(step as u32);
            let c = if invert { -mn.c } else { mn.c };
            let note = ch.scale_step(anchor, c as i32);
            line.push(step as u8, mn.len, note);
        }
        line
    }
}

impl Composer {
    pub(super) fn write_lead(&mut self, bar: &mut Bar) {
        let Some(inst) = self.lead_inst() else {
            return;
        };
        let s = self.style();
        let i = self.intensity;
        let kind = self.plan.kind;
        let pb = self.plan.bar;
        let phrase = pb % 4;
        let last = self.last_note;
        let line = match kind {
            Section::Intro | Section::Breakdown | Section::Still => return,
            Section::Victory => {
                // Hold the 9th of the tonic, swelling in.
                let ch = self.chord(0);
                let mut l = Line::new();
                let nine = fold(ch.root_from(72) + 2);
                l.push(0, 2, ch.nearest_strong(nine.saturating_sub(2)));
                l.push(2, 14, ch.nearest_strong(nine));
                l
            }
            Section::Hook if s.theme => {
                let mut l = Line::new();
                let shift = (self.plan.key + 12 - THEME_KEY) % 12;
                let shift = if shift > 6 {
                    shift as i32 - 12
                } else {
                    shift as i32
                };
                if pb == 7 && i > 0.4 {
                    self.pent_run(&mut l, false);
                } else {
                    for &(st, ln, n) in THEME[pb as usize % 4] {
                        l.push(st, ln, (n as i32 + shift).clamp(0, 127) as u8);
                    }
                }
                l
            }
            Section::Solo => {
                let r = self.rand();
                let mut l = Line::new();
                if s.groove == Groove::Shred || r < 0.25 + 0.3 * i {
                    let up = if self.climb.abs() > 0.3 {
                        self.climb > 0.0
                    } else {
                        phrase % 2 == 1
                    };
                    self.pent_run(&mut l, up);
                } else if r < 0.6 {
                    self.bebop_line(&mut l);
                } else if phrase == 3 {
                    self.cadence(&mut l);
                } else {
                    let m = self.motif;
                    let shift = if self.chance(0.5) { 2 } else { 0 };
                    let invert = self.chance(0.3);
                    l = self.realize(&m, last + 3, shift, invert);
                }
                l
            }
            Section::A | Section::Hook | Section::B => {
                // Calm music breathes: whole phrases left out.
                if kind == Section::A && i < 0.4 && (pb / 4).is_multiple_of(2) {
                    return;
                }
                let m = if kind == Section::Hook {
                    self.hook
                } else {
                    self.motif
                };
                let second_half = pb % 8 >= 4;
                // The story of eight bars: state an idea, answer it, develop
                // it, come home; then state it again a step higher, climb to
                // a peak in bar seven and resolve.
                const ARC: [u8; 8] = [71, 72, 74, 72, 73, 76, 79, 72];
                // Climbing lifts the line a little higher each phrase.
                let lean = (self.climb * 3.0).round() as i32;
                let height = (ARC[pb as usize % 8] as i32 + (i * 2.0) as i32 + lean) as u8;
                match phrase {
                    // Call: the motif, restated (sequenced up) in the second
                    // half so the listener recognises it.
                    0 => {
                        let from = if second_half {
                            self.anchor + if self.chance(0.6) { 2 } else { 0 }
                        } else {
                            self.anchor = height;
                            height
                        };
                        self.realize(&m, from, 0, false)
                    }
                    // Response: a short answer, or rest.
                    1 => {
                        if self.chance(0.45 - 0.2 * i) {
                            return;
                        }
                        self.response(&m)
                    }
                    // Development, heading for the phrase's peak.
                    2 if second_half && i > 0.5 && self.chance(0.4) => {
                        let mut l = Line::new();
                        self.pent_run(&mut l, self.climb > -0.3);
                        l
                    }
                    // On the move up or down: a run that way.
                    2 if self.climb.abs() > 0.5 && self.chance(0.5) => {
                        let mut l = Line::new();
                        self.pent_run(&mut l, self.climb > 0.0);
                        l
                    }
                    2 => match self.pick(4) {
                        0 => self.realize(&m, height.max(last + 2), 0, false), // sequence up
                        1 => self.realize(&m, height, 2, false),               // displaced
                        2 => self.realize(&m, height, 0, true),                // inverted
                        _ => {
                            let mut half = m;
                            half.n = m.n.div_ceil(2);
                            let mut l = self.realize(&half, height, 0, false);
                            self.tail_into(&mut l, 8);
                            l
                        }
                    },
                    // Cadence.
                    _ => {
                        let mut l = Line::new();
                        if i > 0.65 && kind != Section::Hook && self.chance(0.4) {
                            self.pent_run(&mut l, true);
                        } else {
                            self.cadence(&mut l);
                        }
                        l
                    }
                }
            }
        };
        let approach = !(kind == Section::Hook && s.theme);
        self.emit_line(bar, inst, line, approach);
    }
}

impl Composer {
    /// An answer to the call: two or three notes from the motif's end,
    /// inverted, landing on a guide tone.
    pub(super) fn response(&mut self, m: &Motif) -> Line {
        let mut l = Line::new();
        let start = [4u8, 6, 8][self.pick(3)];
        let ch = self.chord(start as u32);
        // The answer moves against the call: a rising call is answered
        // falling, stepwise or by thirds.
        let rose = m.n > 1 && m.notes[m.n - 1].c > m.notes[0].c;
        let dir = if rose { -1 } else { 1 };
        let mut n = ch.scale_step(self.last_note, dir);
        let k = m.n.clamp(2, 3);
        for j in 0..k {
            let st = start + j as u8 * 2;
            let len = if j + 1 == k { 6 } else { 2 };
            l.push(st, len, n);
            let by = 1 + self.pick(2) as i32;
            n = self.chord(st as u32 + 2).scale_step(n, dir * by);
        }
        l
    }
}

impl Composer {
    /// Finish a fragment with two notes into a long chord tone at `at`.
    pub(super) fn tail_into(&mut self, l: &mut Line, at: u8) {
        let ch = self.chord(at as u32);
        let target = ch.nearest_strong(self.last_note);
        l.push(at.saturating_sub(2), 1, ch.scale_step(target, 2));
        l.push(at.saturating_sub(1), 1, target.saturating_sub(1));
        l.push(at, 6, target);
    }
}

impl Composer {
    /// The end of a phrase: a short pickup, then a long guide tone, then rest.
    pub(super) fn cadence(&mut self, l: &mut Line) {
        let ch = self.chord(4);
        let g = ch.q.guide();
        let pick = g[self.pick(2)];
        let target = nearest_pc(self.last_note, (ch.root + pick) % 12);
        let pre = self.chord(2);
        l.push(0, 2, pre.scale_step(target, 2));
        l.push(2, 2, pre.scale_step(target, 1));
        l.push(4, 9, target);
    }
}

impl Composer {
    /// A bebop line: 8ths descending through the scale, chromatic passing
    /// tones on the off-beats so chord tones land on the beats.
    pub(super) fn bebop_line(&mut self, l: &mut Line) {
        let start = if self.chance(0.5) { 0 } else { 2 };
        let ch0 = self.chord(start);
        let mut n = ch0.nearest_strong(fold(self.last_note + 7));
        let mut st = start;
        while st < self.steps.min(16) {
            l.push(st as u8, 2, n);
            let ch = self.chord(st + 2);
            let down = ch.scale_step(n, -1);
            // Passing tone between two scale tones a whole step apart; at the
            // bottom of the range, turn back up by a chord tone.
            n = if down < LEAD_LO + 1 {
                ch.nearest_strong(n + 4)
            } else if (st + 2) % 4 != 0 && n - down == 2 && self.chance(0.6) {
                n - 1
            } else {
                down
            };
            st += 2;
        }
    }
}

impl Composer {
    /// A pentatonic run in 16ths, up or down, ending on a chord tone.
    pub(super) fn pent_run(&mut self, l: &mut Line, up: bool) {
        let mut up = up;
        let pent_of = |ch: Chord| -> [u8; 5] {
            if ch.q.is_minor() {
                [0, 3, 5, 7, 10]
            } else if matches!(ch.q, Dom7alt | Dom7s9) {
                [1, 4, 6, 8, 11] // minor pentatonic a half step up: all altered tones
            } else {
                [0, 2, 4, 7, 9]
            }
        };
        let start = if self.chance(0.5) { 0 } else { 4 };
        let len = 8 + self.pick(5) as u32;
        let mut n = fold(if up {
            self.last_note.saturating_sub(5)
        } else {
            self.last_note + 7
        });
        for k in 0..len {
            let st = start + k;
            if st >= self.steps {
                break;
            }
            let ch_here = self.chord(st);
            let ln = if k + 1 == len { 4 } else { 1 };
            l.push(st as u8, ln, n);
            // Next pentatonic tone of the chord (re-rooted as chords change);
            // a run that reaches the edge of the range turns around rather
            // than jumping an octave.
            let r = ch_here.root;
            let pent = pent_of(ch_here);
            if (up && n + 2 > LEAD_HI) || (!up && n < LEAD_LO + 2) {
                up = !up;
            }
            let mut m = n as i32;
            loop {
                m += if up { 1 } else { -1 };
                let pc = ((m - r as i32).rem_euclid(12)) as u8;
                if pent.contains(&pc) || !(30..110).contains(&m) {
                    break;
                }
            }
            n = m.clamp(0, 127) as u8;
        }
    }
}

impl Composer {
    /// Enforce the rules and write a line out: strong beats land on chord
    /// tones, weak notes on scale tones, chromatic approach tones before
    /// strong targets, everything in the lead's range.
    pub(super) fn emit_line(&mut self, bar: &mut Bar, inst: Inst, mut line: Line, approach: bool) {
        let n = line.n;
        if n == 0 {
            return;
        }
        line.notes[..n].sort_by_key(|x| x.0);
        // One note per step (a later idea at the same step wins).
        let mut m = 0;
        for k in 0..n {
            if m > 0 && line.notes[m - 1].0 == line.notes[k].0 {
                line.notes[m - 1] = line.notes[k];
            } else {
                line.notes[m] = line.notes[k];
                m += 1;
            }
        }
        let n = m;
        line.n = n;
        // Move the whole line by octaves into range, so its shape survives.
        let hi = line.notes[..n].iter().map(|x| x.2).max().unwrap_or(LEAD_HI);
        let lo = line.notes[..n].iter().map(|x| x.2).min().unwrap_or(LEAD_LO);
        let shift: i32 = if hi > LEAD_HI + 2 {
            -12 * ((hi - LEAD_HI + 9) / 12) as i32
        } else if lo + 2 < LEAD_LO {
            12 * ((LEAD_LO - lo + 9) / 12) as i32
        } else {
            0
        };
        let mut prev: Option<u8> = None;
        for k in 0..n {
            let (st, _, raw) = line.notes[k];
            let ch = self.chord(st as u32);
            let note = soft_fold((raw as i32 + shift).clamp(0, 127) as u8);
            let mut out = if st % 4 == 0 {
                ch.nearest_strong(note)
            } else if approach {
                ch.nearest_scale(note)
            } else {
                note
            };
            // What the ear takes as the harmony is the long notes and the
            // first note on a new chord: make those chord tones, and on a
            // change prefer a nearby third or seventh (the guide tones).
            if approach {
                let ln = line.notes[k].1;
                let new_chord = k == 0 || self.chord(line.notes[k - 1].0 as u32) != ch;
                if new_chord && let Some(g) = nearest_guide(ch, out, 2) {
                    out = g;
                } else if ln >= 3 && !ch.is_strong(out) {
                    out = ch.nearest_strong(out);
                }
            }
            // Snapping mustn't stall a moving line on a repeated note: keep
            // going the way the idea went, to the next chord tone.
            if let Some(p) = prev
                && out == p
                && note != p
                && st % 4 == 0
            {
                let up = note > p;
                let mut t = out;
                for _ in 0..12 {
                    t = if up { t + 1 } else { t - 1 };
                    if ch.is_strong(t) {
                        break;
                    }
                }
                if ch.is_strong(t) {
                    out = t;
                }
            }
            line.notes[k].2 = out;
            prev = Some(out);
        }
        if approach {
            // One chromatic approach per bar is spice; more is a trill.
            let mut approached = false;
            for k in 0..n.saturating_sub(1) {
                let (st, ln, _) = line.notes[k];
                let (nst, _, target) = line.notes[k + 1];
                if !approached
                    && st % 4 != 0
                    && ln <= 2
                    && nst % 4 == 0
                    && nst - st <= 2
                    && self.chance(0.35)
                {
                    approached = true;
                    let below = target.saturating_sub(1);
                    let prev = if k > 0 { line.notes[k - 1].2 } else { 0 };
                    line.notes[k].2 = if prev == below { target + 1 } else { below };
                }
            }
        }
        let i = self.intensity;
        // Dynamics follow the phrase's arc: swell into the peak, ease off
        // into the cadence.
        const SHAPE: [f32; 8] = [0.9, 0.92, 0.97, 0.9, 0.95, 1.0, 1.1, 0.9];
        let shape = SHAPE[self.plan.bar as usize % 8];
        self.frag = Line::new();
        self.lead_this_bar = true;
        for k in 0..n {
            let (st, ln, note) = line.notes[k];
            let ch = self.chord(st as u32);
            let note = soft_fold(note);
            self.frag.push(st, ln, note);
            self.lead_end = self.lead_end.max(st as u32 + ln as u32);
            if st % 4 == 0 {
                debug_assert!(ch.is_strong(note));
                #[cfg(test)]
                self.strong_log.push((note, ch));
            }
            let next_st = if k + 1 < n { line.notes[k + 1].0 } else { 99 };
            let legato = next_st as u32 <= st as u32 + ln as u32;
            let strong = st % 4 == 0 || ln >= 4;
            let vel = (if strong { 0.85 } else { 0.68 }) * (0.8 + 0.3 * i) * shape;
            let jitter = (self.rand() * 120.0) as u32;
            let at = self.at(st as u32) + jitter;
            let len = self.len(ln as f32 * if legato { 1.02 } else { 0.85 });
            let prev = self.last_note;
            let glide = [pk().lead, pk().mini].contains(&inst)
                && k > 0
                && (prev as i32 - note as i32).abs() <= 5
                && self.chance(0.3);
            Self::push(
                bar,
                Ev {
                    at,
                    inst,
                    note,
                    vel,
                    len,
                    glide,
                },
            );
            self.last_note = note;
        }
    }
}
