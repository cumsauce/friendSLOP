//! Melody: the lead's motifs and runs, answers, kimes and themes.

use super::library::LeadStyle;
use super::*;

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
    pub(super) fn write_answer(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Answer { chance } = &part.play else {
            return;
        };
        let inst = part.inst;
        if self.frag.n < 2 {
            return;
        }
        let steps = self.steps;
        let from = if self.lead_end > 0 {
            self.lead_end + 1
        } else {
            0
        };
        let from = from.next_multiple_of(2);
        if from + 5 > steps || !self.chance(chance.0 + chance.1 * self.intensity) {
            return;
        }
        let (lo, hi) = part.register;
        let hi = hi.max(lo.saturating_add(11));
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
            if self.on_beat(at_step) || ln >= 3 {
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

    /// The melody doubled in parallel (thirds, sixths): each note it played
    /// this bar moved `steps` scale steps in the chord sounding under it.
    pub(super) fn write_harmony(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Harmony { steps } = &part.play else {
            return;
        };
        if !self.sung {
            return;
        }
        let (lo, hi) = part.register;
        let line = self.frag;
        for &(st, ln, n) in &line.notes[..line.n] {
            let ch = self.chord(st as u32);
            // Only notes well outside its register move an octave, so the
            // interval to the melody holds.
            let note = soft_fold(ch.scale_step(n, *steps as i32), lo, hi);
            let at = self.at(st as u32) + (self.rand() * 120.0) as u32;
            let len = self.len(ln as f32 * 0.95);
            Self::push(
                bar,
                Ev {
                    at,
                    inst: part.inst,
                    note,
                    vel: 0.6 + 0.2 * self.intensity,
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
    pub(super) fn write_kime(&mut self, bar: &mut Bar, part: &'static Part, crash: bool) {
        let G::Kime {
            rhythms,
            kit,
            bass,
            keys,
            horn,
        } = &part.play
        else {
            return;
        };
        let rhythm: &[u8] = if self.steps < self.full {
            &rhythms.odd
        } else if rhythms.rhythms.is_empty() {
            &[]
        } else {
            &rhythms.rhythms[self.pick(rhythms.rhythms.len())]
        };
        if rhythm.is_empty() {
            return;
        }
        let (lead_lo, lead_hi) = (self.lead_lo, self.lead_hi);
        let regs = &self.style().feel.registers;
        let (keys_lo, keys_hi) = regs.keys;
        let (horn_lo, horn_hi) = regs.horn;
        let low = regs.bass;
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
            Self::push(bar, hit(at, kit.kick.0, kit.kick.1, 1.0));
            Self::push(bar, hit(at, kit.snare.0, kit.snare.1, 0.9));
            if k == 0 && crash || lastk {
                Self::push(bar, hit(at, kit.crash.0, kit.crash.1, 0.85));
            }
            // Bass in octaves with the lead's pitch class, else the root.
            let root = ch.root_from(low);
            Self::push(
                bar,
                Ev {
                    at,
                    inst: *bass,
                    note: if k % 2 == 1 { root + 12 } else { root },
                    vel: 1.0,
                    len,
                    glide: false,
                },
            );
            // Keys stab.
            self.ep = voice_lead(ch, &self.ep, keys_lo, keys_hi);
            for &n in &{ self.ep } {
                Self::push(
                    bar,
                    Ev {
                        at,
                        inst: *keys,
                        note: n,
                        vel: 0.9,
                        len,
                        glide: false,
                    },
                );
            }
            // Horns hit with the band.
            if let Some(horn_inst) = *horn {
                self.horn = no_top_rub(voice_lead(ch, &self.horn, horn_lo, horn_hi));
                let horn = self.horn;
                for &n in &horn[1..] {
                    Self::push(
                        bar,
                        Ev {
                            at,
                            inst: horn_inst,
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
            while top > lead_hi.max(lead_lo.saturating_add(11)) {
                top -= 12;
            }
            #[cfg(test)]
            if self.on_beat(st) {
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
            top = next_ch.nearest_strong(top.saturating_sub(2).max(lead_lo + 2));
            if top == self.last_note {
                top = next_ch.nearest_strong(top.saturating_sub(3));
            }
        }
    }

    // --- Lead -----------------------------------------------------------
}

impl Composer {
    /// Who plays the melody now (none without a melody part).
    pub(super) fn lead_inst(&self) -> Option<Inst> {
        let part = self.style().lead_part()?;
        // In solos the band's players trade every two bars.
        let trade = self.plan.kind == Section::Solo && (self.plan.bar / 2) % 2 == 1;
        let traded = match &part.play {
            G::Melody { style, .. } if trade => style.trade.get(&part.inst).copied(),
            _ => None,
        };
        Some(traded.unwrap_or(part.inst))
    }

    /// The melody's style (the band's for a silent lead).
    pub(super) fn lead_style(&self) -> &'static LeadStyle {
        self.style().lead_style
    }
}

impl Composer {
    pub(super) fn new_motif(&mut self) -> Motif {
        let span = self.lead_style().min_span;
        // A motif needs a shape you can hear: retry flat ones.
        let mut m = self.motif_try();
        for _ in 0..4 {
            let cs = m.notes[..m.n].iter().map(|x| x.c);
            let range = cs.clone().max().unwrap_or(0) - cs.min().unwrap_or(0);
            if range >= span {
                break;
            }
            m = self.motif_try();
        }
        m
    }
}

impl Composer {
    pub(super) fn motif_try(&mut self) -> Motif {
        let ls = self.lead_style();
        let mut m = Motif::default();
        if ls.cells.is_empty() {
            return m;
        }
        let cell = &ls.cells[self.pick(ls.cells.len())];
        let mut c: i8 = 0;
        let n = cell.len().min(m.notes.len());
        for (k, &(step, len)) in cell.iter().take(n).enumerate() {
            m.notes[k] = MNote { step, len, c };
            // Arch contour: rise through the first half, fall after.
            let rising = k * 2 < n;
            let leap = ls
                .leaps
                .get(self.pick(ls.leaps.len()))
                .copied()
                .unwrap_or(1);
            let dir = if self.chance(if rising { ls.rise } else { ls.fall }) {
                1
            } else {
                -1
            };
            c = (c + dir * leap).clamp(ls.contour.0, ls.contour.1.max(ls.contour.0));
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
        let anchor = first.nearest_strong(fold(from, self.lead_lo, self.lead_hi));
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
    pub(super) fn write_lead(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Melody {
            style: ls,
            theme,
            solo_runs,
        } = &part.play
        else {
            return;
        };
        let Some(inst) = self.lead_inst() else {
            return;
        };
        let (lo, hi) = (self.lead_lo, self.lead_hi);
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
                let nine = fold(ch.root_from(72) + 2, lo, hi);
                l.push(0, 2, ch.nearest_strong(nine.saturating_sub(2)));
                l.push(2, (self.full - 2) as u8, ch.nearest_strong(nine));
                l
            }
            Section::Hook if theme.is_some_and(|t| !t.bars.is_empty()) => {
                let Some(t) = theme else {
                    return;
                };
                let mut l = Line::new();
                let shift = (self.plan.key + 12 - t.key % 12) % 12;
                let shift = if shift > 6 {
                    shift as i32 - 12
                } else {
                    shift as i32
                };
                if pb == 7 && i > 0.4 {
                    self.pent_run(&mut l, false);
                } else {
                    for &(st, ln, n) in &t.bars[pb as usize % t.bars.len()] {
                        l.push(st, ln, (n as i32 + shift).clamp(0, 127) as u8);
                    }
                }
                l
            }
            Section::Solo => {
                let r = self.rand();
                let mut l = Line::new();
                if *solo_runs || r < ls.solo_run.0 + ls.solo_run.1 * i {
                    let up = if self.climb.abs() > 0.3 {
                        self.climb > 0.0
                    } else {
                        phrase % 2 == 1
                    };
                    self.pent_run(&mut l, up);
                } else if r < ls.solo_bebop {
                    self.bebop_line(&mut l);
                } else if phrase == 3 {
                    self.cadence(&mut l);
                } else {
                    let m = self.motif;
                    let shift = if self.chance(ls.solo_shift) { 2 } else { 0 };
                    let invert = self.chance(ls.solo_invert);
                    l = self.realize(&m, last + 3, shift, invert);
                }
                l
            }
            Section::A | Section::Hook | Section::B => {
                // Calm music breathes: whole phrases left out.
                if kind == Section::A && i < ls.calm && (pb / 4).is_multiple_of(2) {
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
                let arc = ls
                    .arc
                    .get(pb as usize % ls.arc.len().max(1))
                    .copied()
                    .unwrap_or(72);
                // Climbing lifts the line a little higher each phrase.
                let lean = (self.climb * 3.0).round() as i32;
                let height = (arc as i32 + (i * 2.0) as i32 + lean) as u8;
                match phrase {
                    // Call: the motif, restated (sequenced up) in the second
                    // half so the listener recognises it.
                    0 => {
                        let from = if second_half {
                            self.anchor + if self.chance(ls.restate) { 2 } else { 0 }
                        } else {
                            self.anchor = height;
                            height
                        };
                        self.realize(&m, from, 0, false)
                    }
                    // Response: a short answer, or rest.
                    1 => {
                        if self.chance(ls.rest.0 - ls.rest.1 * i) {
                            return;
                        }
                        self.response(&m)
                    }
                    // Development, heading for the phrase's peak.
                    2 if second_half && i > 0.5 && self.chance(ls.dev_run) => {
                        let mut l = Line::new();
                        self.pent_run(&mut l, self.climb > -0.3);
                        l
                    }
                    // On the move up or down: a run that way.
                    2 if self.climb.abs() > 0.5 && self.chance(ls.climb_run) => {
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
                            self.tail_into(&mut l, (self.full / 2) as u8);
                            l
                        }
                    },
                    // Cadence.
                    _ => {
                        let mut l = Line::new();
                        if i > ls.cadence_run.0
                            && kind != Section::Hook
                            && self.chance(ls.cadence_run.1)
                        {
                            self.pent_run(&mut l, true);
                        } else {
                            self.cadence(&mut l);
                        }
                        l
                    }
                }
            }
        };
        let themed = kind == Section::Hook && theme.is_some_and(|t| !t.bars.is_empty());
        self.emit_line(bar, inst, ls, line, !themed);
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
        let (lo, hi) = (self.lead_lo, self.lead_hi);
        let pass = self.lead_style().bebop_pass;
        let mut n = ch0.nearest_strong(fold(self.last_note + 7, lo, hi));
        let mut st = start;
        while st < self.steps.min(self.full) {
            l.push(st as u8, 2, n);
            let ch = self.chord(st + 2);
            let down = ch.scale_step(n, -1);
            // Passing tone between two scale tones a whole step apart; at the
            // bottom of the range, turn back up by a chord tone.
            n = if down < lo + 1 {
                ch.nearest_strong(n + 4)
            } else if !self.on_beat(st + 2) && n - down == 2 && self.chance(pass) {
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
        let (lo, hi) = (self.lead_lo, self.lead_hi);
        let start = if self.chance(0.5) { 0 } else { 4 };
        let len = 8 + self.pick(5) as u32;
        let mut n = fold(
            if up {
                self.last_note.saturating_sub(5)
            } else {
                self.last_note + 7
            },
            lo,
            hi,
        );
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
            let pent = ch_here.q.pent();
            if (up && n + 2 > hi) || (!up && n < lo + 2) {
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
    pub(super) fn emit_line(
        &mut self,
        bar: &mut Bar,
        inst: Inst,
        ls: &LeadStyle,
        mut line: Line,
        approach: bool,
    ) {
        let (lead_lo, lead_hi) = (self.lead_lo, self.lead_hi);
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
        let hi = line.notes[..n].iter().map(|x| x.2).max().unwrap_or(lead_hi);
        let lo = line.notes[..n].iter().map(|x| x.2).min().unwrap_or(lead_lo);
        let shift: i32 = if hi > lead_hi + 2 {
            -12 * ((hi - lead_hi + 9) / 12) as i32
        } else if lo + 2 < lead_lo {
            12 * ((lead_lo - lo + 9) / 12) as i32
        } else {
            0
        };
        let mut prev: Option<u8> = None;
        for k in 0..n {
            let (st, _, raw) = line.notes[k];
            let ch = self.chord(st as u32);
            let note = soft_fold((raw as i32 + shift).clamp(0, 127) as u8, lead_lo, lead_hi);
            let mut out = if self.on_beat(st as u32) {
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
                && self.on_beat(st as u32)
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
                    && !self.on_beat(st as u32)
                    && ln <= 2
                    && self.on_beat(nst as u32)
                    && nst - st <= 2
                    && self.chance(ls.approach)
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
        let shape = ls
            .shape
            .get(self.plan.bar as usize % ls.shape.len().max(1))
            .copied()
            .unwrap_or(1.0);
        self.frag = Line::new();
        self.sung = true;
        for k in 0..n {
            let (st, ln, note) = line.notes[k];
            let ch = self.chord(st as u32);
            let note = soft_fold(note, lead_lo, lead_hi);
            self.frag.push(st, ln, note);
            self.lead_end = self.lead_end.max(st as u32 + ln as u32);
            if self.on_beat(st as u32) {
                debug_assert!(ch.is_strong(note));
                #[cfg(test)]
                self.strong_log.push((note, ch));
            }
            let next_st = if k + 1 < n { line.notes[k + 1].0 } else { 99 };
            let legato = next_st as u32 <= st as u32 + ln as u32;
            let strong = self.on_beat(st as u32) || ln >= 4;
            let vel = (if strong { 0.85 } else { 0.68 }) * (0.8 + 0.3 * i) * shape;
            let jitter = (self.rand() * 120.0) as u32;
            let at = self.at(st as u32) + jitter;
            let len = self.len(ln as f32 * if legato { 1.02 } else { 0.85 });
            let prev = self.last_note;
            let glide = ls.glides.contains(&inst)
                && k > 0
                && (prev as i32 - note as i32).abs() <= 5
                && self.chance(ls.glide);
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
