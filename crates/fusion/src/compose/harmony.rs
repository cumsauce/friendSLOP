//! Chords: keys comping, guitar (bossa, picking), pads, arpeggios, ostinatos and horns.

use super::*;

impl Composer {
    pub(super) fn write_comp(&mut self, bar: &mut Bar) {
        let s = self.style();
        let i = self.intensity;
        if s.comp == Comp::Bolero {
            self.write_guitar(bar);
            return;
        }
        if s.nylon == Nylon::Bossa {
            self.write_bossa(bar);
            return;
        }
        if self.picking() {
            self.write_picking(bar);
            return;
        }
        // A few rhythms per comping style; each phrase picks one, so the
        // keys don't hammer the same figure for minutes.
        type Rh = &'static [(u8, u8, bool)];
        let pats: &[Rh] = match s.comp {
            Comp::City => &[
                &[(0, 3, false), (6, 2, false), (10, 1, false), (14, 2, true)],
                &[(2, 2, false), (6, 1, false), (8, 3, false), (14, 2, true)],
                &[
                    (0, 1, false),
                    (3, 2, false),
                    (7, 1, false),
                    (10, 2, false),
                    (14, 2, true),
                ],
                &[
                    (0, 2, false),
                    (4, 1, false),
                    (7, 3, false),
                    (11, 1, false),
                    (12, 2, false),
                ],
            ],
            Comp::Laid => &[
                &[(0, 6, false), (7, 1, false), (10, 6, false)],
                &[(0, 10, false), (11, 4, false)],
                &[(2, 4, false), (8, 6, false), (14, 2, true)],
            ],
            Comp::Long => &[&[(0, 16, false)], &[(0, 10, false), (10, 6, false)]],
            Comp::Stabs => &[
                &[
                    (0, 1, false),
                    (3, 1, false),
                    (6, 2, false),
                    (10, 1, false),
                    (12, 1, false),
                    (14, 2, true),
                ],
                &[
                    (0, 1, false),
                    (2, 1, false),
                    (6, 1, false),
                    (8, 1, false),
                    (11, 2, false),
                    (14, 2, true),
                ],
                &[
                    (0, 2, false),
                    (3, 1, false),
                    (7, 1, false),
                    (10, 2, false),
                    (13, 1, false),
                ],
            ],
            Comp::Tense | Comp::Bolero => &[&[(0, 12, false), (14, 2, true)]],
        };
        let pat = pats[(self.plan.bar as usize / 4 + self.comp_var) % pats.len()];
        let steps = self.steps;
        // Every chord change in the bar needs a hit at or just before it.
        let mut hits = [(0u8, 0u8, false); 12];
        let mut n = 0;
        for &h in pat {
            if (h.0 as u32) < steps && n < hits.len() {
                hits[n] = h;
                n += 1;
            }
        }
        for change in [4u32, 8, 12] {
            if self.chord(change) != self.chord(change - 1)
                && !hits[..n]
                    .iter()
                    .any(|h| (h.0 as u32) <= change && h.0 as u32 + 2 >= change)
                && n < hits.len()
            {
                hits[n] = (change as u8, 4, false);
                n += 1;
            }
        }
        // A chord anticipated on the last "and of 4" already rings.
        let pushed = std::mem::take(&mut self.pushed);
        for (k, &(st, ln, push)) in hits[..n].iter().enumerate() {
            if st == 0 && pushed {
                continue;
            }
            if k > 0 && i < 0.45 && st % 4 != 0 && self.chance(0.5 - i) {
                continue; // sparser when calm
            }
            if push && st as u32 + 2 >= steps {
                self.pushed = true;
            }
            let ch = if push {
                self.chord(st as u32 + 2)
            } else {
                self.chord(st as u32)
            };
            self.ep = voice_lead(ch, &self.ep, 50, 62);
            let accent = if st % 4 == 0 { 0.75 } else { 0.62 };
            let vel = accent * (0.75 + 0.35 * i);
            let base = self.at(st as u32);
            let mut len = ln as f32 * 0.92;
            if push {
                // An anticipation rings over the bar line.
                len += 2.0;
            }
            let len = self.len(len);
            let strum = if matches!(s.comp, Comp::Laid | Comp::Long) {
                90
            } else {
                12
            };
            // Under a melody the keys leave its register to it: the top
            // note goes if it reaches into the lead's range.
            let under = bar.bus[Bus::Lead as usize] > 0.0;
            for (j, &note) in self.ep.iter().enumerate() {
                if under && j == 3 && note >= LEAD_LO + 5 {
                    continue;
                }
                Self::push(
                    bar,
                    Ev {
                        at: base + j as u32 * strum,
                        inst: self.style().keys,
                        note,
                        vel,
                        len,
                        glide: false,
                    },
                );
            }
        }
    }
}

impl Composer {
    /// Bolero guitar: a strum on one, then arpeggiated 8ths.
    pub(super) fn write_guitar(&mut self, bar: &mut Bar) {
        let steps = self.steps;
        let mut st = 0;
        while st < steps {
            let ch = self.chord(st);
            self.ep = voice_lead(ch, &self.ep, 50, 62);
            let at = self.at(st);
            if st % 8 == 0 {
                let len = self.len(4.0);
                for (j, &n) in self.ep.iter().enumerate() {
                    Self::push(
                        bar,
                        Ev {
                            at: at + j as u32 * 300,
                            inst: pk().guitar,
                            note: n,
                            vel: 0.8,
                            len,
                            glide: false,
                        },
                    );
                }
            } else {
                let order = [0usize, 2, 1, 3, 2, 1, 3, 2];
                let n = self.ep[order[(st / 2) as usize % 8]] + 12;
                let len = self.len(2.0);
                Self::push(
                    bar,
                    Ev {
                        at,
                        inst: pk().guitar,
                        note: n,
                        vel: 0.55,
                        len,
                        glide: false,
                    },
                );
            }
            st += 2;
        }
    }
}

impl Composer {
    /// The guitar's thumb: the chord's root on the low strings (E2 up).
    pub(super) fn thumb(ch: Chord) -> u8 {
        ch.root_from(40)
    }
}

impl Composer {
    /// The thumb's alternate bass: the fifth below the root's octave (the
    /// root again on an altered dominant, whose fifth is bent).
    pub(super) fn thumb_fifth(ch: Chord) -> u8 {
        let root = Self::thumb(ch);
        if ch.q == Dom7alt {
            return root;
        }
        let f = root + ch.q.fifth();
        if f > 55 { f - 12 } else { f }
    }
}

impl Composer {
    /// Bossa nova: thumb on one and three (root, then fifth), the fingers
    /// pinching a shell on the off-beats, a two-bar figure.
    pub(super) fn write_bossa(&mut self, bar: &mut Bar) {
        const FINGERS: [&[u8]; 2] = [&[0, 3, 6, 10, 13], &[2, 5, 8, 12, 14]];
        let i = self.intensity;
        let steps = self.steps;
        for st in [0u32, 8] {
            if st >= steps {
                continue;
            }
            let ch = self.chord(st);
            let note = if st == 0 || self.chord(0) != ch {
                Self::thumb(ch)
            } else {
                Self::thumb_fifth(ch)
            };
            let at = self.at(st);
            let len = self.len(7.5);
            Self::push(bar, ev(at, pk().guitar, note, 0.62 + 0.2 * i, len));
        }
        let second = self.plan.bar % 2 == 1;
        let fig = FINGERS[self.plan.bar as usize % 2];
        for (k, &st) in fig.iter().enumerate() {
            let st = st as u32;
            if st >= steps {
                continue;
            }
            // The last pinch before a new chord anticipates it.
            let ch = if st % 4 == 3 || st >= 14 {
                self.chord(st + 1)
            } else {
                self.chord(st)
            };
            self.gtr = voice_lead(ch, &self.gtr, 52, 60);
            let at = self.at(st);
            let len = self.len(2.5);
            let vel = 0.42 + 0.12 * i + if st.is_multiple_of(4) { 0.06 } else { 0.0 };
            // The top string moves in the figure's second bar (a little
            // melody in the comping), back home for the next chord.
            let mut shell = [self.gtr[1], self.gtr[2], self.gtr[3]];
            if second && k >= 3 {
                shell[2] = ch.scale_step(shell[2], if k == 3 { 1 } else { -1 });
            }
            // Fingers together, but not quite.
            for (j, &n) in shell.iter().enumerate() {
                Self::push(bar, ev(at + j as u32 * 40, pk().guitar, n, vel, len));
            }
        }
    }
}

impl Composer {
    /// Fingerpicking (p-i-m-a): the thumb on the beat, the fingers rolling
    /// up and down the chord in eighths, each string left to ring.
    pub(super) fn write_picking(&mut self, bar: &mut Bar) {
        let i = self.intensity;
        let steps = self.steps;
        // Which finger plays each eighth: 0 = thumb, 1..=3 = strings up.
        const ROLL: [[u8; 8]; 3] = [
            [0, 1, 2, 3, 0, 2, 1, 2],
            [0, 2, 1, 3, 0, 3, 2, 1],
            [0, 1, 3, 2, 0, 1, 2, 3],
        ];
        let roll = ROLL[(self.plan.bar as usize / 2 + self.comp_var) % ROLL.len()];
        let mut st = 0;
        while st < steps {
            let ch = self.chord(st);
            if st % 8 == 0 || self.chord(st.saturating_sub(2)) != ch {
                self.gtr = voice_lead(ch, &self.gtr, 52, 60);
            }
            let finger = roll[(st / 2) as usize % 8];
            // Rests now and then when calm, so it breathes.
            if finger != 0 && i < 0.4 && self.chance(0.18) {
                st += 2;
                continue;
            }
            let (note, vel, ring) = if finger == 0 {
                let n = if st % 8 == 4 && self.chord(st & !7) == ch {
                    Self::thumb_fifth(ch)
                } else {
                    Self::thumb(ch)
                };
                (n, 0.6 + 0.2 * i, 8.0)
            } else {
                (self.gtr[finger as usize], 0.42 + 0.15 * i, 6.0)
            };
            let at = self.at(st);
            let len = self.len(ring);
            Self::push(bar, ev(at, pk().guitar, note, vel, len));
            // A hammered sixteenth pickup into the next beat, when there's
            // some energy.
            if finger == 3 && i > 0.45 && self.chance(0.25) && st + 1 < steps {
                let n2 = ch.scale_step(self.gtr[2], 1);
                Self::push(
                    bar,
                    ev(self.at(st + 1), pk().guitar, n2, vel * 0.8, self.len(3.0)),
                );
            }
            st += 2;
        }
    }
}

impl Composer {
    pub(super) fn write_pad(&mut self, bar: &mut Bar) {
        let s = self.style();
        if self.swelled || (!s.pad && self.plan.kind != Section::Breakdown && self.intensity >= 0.3)
        {
            return;
        }
        let steps = self.steps;
        let mut st = 0;
        while st < steps {
            let ch = self.chord(st);
            // Only re-articulate when the chord actually changes.
            let changed = st > 0 || self.plan.bar == 0 || self.prev_chord() != ch;
            let mut end = st + 4;
            while end < st + 64 && self.chord(end) == ch {
                end += 4;
            }
            let mut nx = st + 4;
            while nx < steps && self.chord(nx) == ch {
                nx += 4;
            }
            if changed {
                self.pad = voice_lead(ch, &self.pad, 55, 67);
                let at = self.at(st);
                let len = self.len((end - st) as f32 + 1.0);
                // Under a melody the pad thins to its two lowest voices (an
                // octave down, under the lead rather than in it) and sits back;
                // on its own it spreads out up high.
                let under = bar.bus[Bus::Lead as usize] > 0.0;
                let pad = self.pad;
                let (notes, lift, vel) = if under {
                    (&pad[..2], 0, 0.42)
                } else {
                    (&pad[..], 12, 0.5)
                };
                for &n in notes {
                    Self::push(
                        bar,
                        Ev {
                            at,
                            inst: s.pad_inst,
                            note: n + lift,
                            vel,
                            len,
                            glide: false,
                        },
                    );
                }
            }
            st = nx;
        }
    }
}

impl Composer {
    pub(super) fn prev_chord(&self) -> Chord {
        if self.plan.bar == 0 {
            return Chord::new(0, Maj9);
        }
        self.plan.chord_at(self.plan.bar - 1, 15)
    }
}

impl Composer {
    /// Bells, harp or chimes arpeggiating the chord.
    pub(super) fn write_arp(&mut self, bar: &mut Bar) {
        let s = self.style();
        let Some(inst) = s.arp else {
            if self.plan.kind == Section::Victory && self.plan.bar == 1 {
                self.write_victory_bells(bar);
            }
            return;
        };
        if self.picking() {
            return;
        }
        if inst == pk().kalimba {
            self.write_ostinato(bar);
            return;
        }
        let i = self.intensity;
        let every = if i > 0.5 { 2 } else { 4 };
        let steps = self.steps;
        let mut st = 0;
        let up = if self.climb.abs() > 0.3 {
            self.climb > 0.0
        } else {
            self.plan.bar.is_multiple_of(2)
        };
        let mut k = 0usize;
        while st < steps {
            if self.chance(0.25 - 0.2 * i) {
                st += every;
                k += 1;
                continue;
            }
            let ch = self.chord(st);
            let tones = ch.q.voicing();
            let idx = if up { k % 4 } else { 3 - k % 4 };
            let n = ch.root_from(60) + tones[idx] + 12 * (k as u8 / 4 % 2);
            let at = self.at(st);
            Self::push(
                bar,
                Ev {
                    at,
                    inst,
                    note: n.min(96),
                    vel: 0.5 + 0.2 * self.rand(),
                    len: 0,
                    glide: false,
                },
            );
            st += every;
            k += 1;
        }
    }
}

impl Composer {
    pub(super) fn write_victory_bells(&mut self, bar: &mut Bar) {
        let ch = self.chord(0);
        for (k, t) in ch.q.voicing().iter().enumerate() {
            let at = self.at(k as u32 * 2);
            let n = ch.root_from(72) + t;
            Self::push(
                bar,
                Ev {
                    at,
                    inst: pk().chime,
                    note: n.min(96),
                    vel: 0.6,
                    len: 0,
                    glide: false,
                },
            );
        }
    }
}

impl Composer {
    /// An interlocking kalimba/marimba ostinato: a two-bar syncopated
    /// figure through the chord's tones that keeps its shape while the
    /// chords move under it.
    pub(super) fn write_ostinato(&mut self, bar: &mut Bar) {
        // (16th step, tone: 0 = root, 1..=4 = the voicing's tones).
        type Fig = &'static [(u8, u8)];
        const FIGS: [[Fig; 2]; 3] = [
            [
                &[(0, 0), (3, 2), (6, 1), (8, 3), (10, 2), (13, 4), (14, 1)],
                &[(0, 0), (3, 2), (6, 3), (8, 4), (11, 2), (14, 3)],
            ],
            [
                &[(0, 1), (2, 3), (4, 0), (7, 2), (10, 4), (12, 3)],
                &[(0, 1), (2, 3), (4, 2), (7, 4), (9, 3), (12, 1), (14, 0)],
            ],
            [
                &[(0, 2), (2, 0), (5, 3), (8, 2), (10, 4), (13, 1)],
                &[(0, 2), (2, 0), (5, 3), (8, 4), (11, 3), (14, 1)],
            ],
        ];
        let fig = FIGS[self.comp_var % 3][usize::from(self.plan.bar % 2 == 1)];
        let i = self.intensity;
        for &(st, t) in fig {
            if st as u32 >= self.steps || (i < 0.3 && st % 4 != 0 && self.chance(0.3)) {
                continue;
            }
            let ch = self.chord(st as u32);
            let v = ch.q.voicing();
            let tones = [0, v[0], v[1], v[2], v[3]];
            let mut n = ch.root_from(65) + tones[t as usize];
            while n > 86 {
                n -= 12;
            }
            let at = self.at(st as u32);
            let vel = if st % 4 == 0 { 0.8 } else { 0.6 } * (0.85 + 0.3 * self.rand());
            Self::push(
                bar,
                Ev {
                    at,
                    inst: pk().kalimba,
                    note: n,
                    vel,
                    len: self.len(1.5),
                    glide: false,
                },
            );
        }
    }

    // --- Answers ----------------------------------------------------------
}

impl Composer {
    /// The horn section: shout figures and pushes in hooks and hot solos,
    /// long blooming chords on the changes in bridges and calm sections.
    pub(super) fn write_horns(&mut self, bar: &mut Bar) {
        let s = self.style();
        if s.horns == Horns::None {
            return;
        }
        let i = self.intensity;
        let kind = self.plan.kind;
        let pb = self.plan.bar;
        let stabs = matches!(s.horns, Horns::Stabs | Horns::Both);
        let swells = matches!(s.horns, Horns::Swells | Horns::Both);
        let stab_now = stabs
            && match kind {
                Section::Hook | Section::Victory => true,
                Section::Solo => i > 0.55 && pb % 2 == 1,
                Section::A => i > 0.7 && pb % 8 >= 4,
                _ => false,
            };
        let swell_now = swells
            && !stab_now
            && match kind {
                Section::B | Section::Breakdown | Section::Hook => true,
                Section::Intro => pb >= 2,
                Section::A => i < 0.5 && pb % 8 >= 4,
                _ => false,
            };
        if kind == Section::Victory {
            self.horn_chord(bar, 0, 15.0, 1.0, false);
        } else if stab_now {
            self.horn_stabs(bar);
        } else if swell_now {
            self.horn_swells(bar);
        }
    }
}

impl Composer {
    /// One section chord at `st`, `len` 16ths long, voice-led from the
    /// last (`top`: block-voice under this melody note instead).
    pub(super) fn horn_chord(&mut self, bar: &mut Bar, st: u32, len: f32, vel: f32, push: bool) {
        self.horn_hit(bar, st, len, vel, push, None);
    }
}

impl Composer {
    pub(super) fn horn_hit(
        &mut self,
        bar: &mut Bar,
        st: u32,
        len: f32,
        vel: f32,
        push: bool,
        top: Option<u8>,
    ) {
        let ch = if push {
            self.chord(st + 2)
        } else {
            self.chord(st)
        };
        self.horn = match top {
            Some(t) => block_voicing(ch, t),
            None => no_top_rub(voice_lead(ch, &self.horn, 60, 69)),
        };
        let at = self.at(st);
        let len = self.len(len);
        // The section's top three voices: the keys already have the bottom.
        let skip = usize::from(top.is_none());
        let horn = self.horn;
        for &n in &horn[skip..] {
            Self::push(
                bar,
                Ev {
                    at,
                    inst: pk().poly_brass,
                    note: n,
                    vel,
                    len,
                    glide: false,
                },
            );
        }
    }
}

impl Composer {
    /// 16th steps where the lead starts a note this bar (horns answer
    /// around them rather than on top of them).
    pub(super) fn lead_onsets(&self, bar: &Bar) -> u32 {
        let mut mask = 0u32;
        for e in &bar.events {
            if [pk().lead, pk().brass, pk().mini, pk().flute, pk().bell].contains(&e.inst) {
                let st = (e.at as f32 / self.step_len).round() as u32;
                mask |= 1 << st.min(31);
            }
        }
        mask
    }
}

impl Composer {
    pub(super) fn horn_stabs(&mut self, bar: &mut Bar) {
        // Shouts answer the lead in the bars where it holds or rests;
        // pushes just kick the off-beats elsewhere.
        type Fig = &'static [(u8, u8)];
        const SHOUTS: [Fig; 4] = [
            &[(4, 1), (6, 1), (8, 2), (11, 1), (14, 2)],
            &[(2, 1), (3, 1), (6, 2), (10, 1), (12, 3)],
            &[(0, 2), (3, 1), (6, 1), (8, 1), (10, 4)],
            &[(6, 1), (7, 1), (10, 2), (14, 2)],
        ];
        // The shout's top line, in chord-tone steps from where it starts.
        const CONTOURS: [&[i8]; 3] = [&[0, 1, 2, 1, 3], &[2, 1, 0, 1, -1], &[0, 2, 1, 3, 2]];
        const PUSHES: [Fig; 3] = [&[(14, 2)], &[(6, 1), (14, 2)], &[(10, 1), (11, 3)]];
        let phrase = self.plan.bar % 4;
        let busy = self.lead_onsets(bar);
        let free = |st: u8| busy & (1 << st) == 0 && (st == 0 || busy & (1 << (st - 1)) == 0);
        let mut hits = [(0u8, 0u8); 6];
        let mut n = 0;
        let mut shout = false;
        if phrase % 2 == 1 {
            let fig = SHOUTS[(self.comp_var + phrase as usize / 2) % SHOUTS.len()];
            for &h in fig.iter().filter(|h| free(h.0)) {
                hits[n] = h;
                n += 1;
            }
            shout = n >= 3;
        }
        if !shout {
            n = 0;
            for &h in PUSHES[self.comp_var % PUSHES.len()]
                .iter()
                .filter(|h| free(h.0))
            {
                hits[n] = h;
                n += 1;
            }
        }
        let i = self.intensity;
        let contour = CONTOURS[(self.comp_var + self.plan.bar as usize) % CONTOURS.len()];
        let mut top = self.chord(hits[0].0 as u32).nearest_strong(70);
        for (k, &(st, ln)) in hits[..n].iter().enumerate() {
            let st = st as u32;
            if st >= self.steps {
                continue;
            }
            let push = st + ln as u32 >= 16 && st >= 14;
            let last = k + 1 == n;
            let vel = (if last { 0.95 } else { 0.78 }) * (0.8 + 0.25 * i);
            // Short notes are clipped tight, the last one rings.
            let len = if last {
                ln as f32 + 0.5
            } else {
                ln as f32 * 0.6
            };
            if shout {
                let ch = self.chord(if push { st + 2 } else { st });
                let step = contour[k % contour.len()] as i32
                    - if k > 0 {
                        contour[(k - 1) % contour.len()] as i32
                    } else {
                        0
                    };
                top = chord_tone_step(ch, top, step).clamp(65, 80);
                let t = Some(ch.nearest_strong(top));
                self.horn_hit(bar, st, len, vel, push, t);
            } else {
                self.horn_chord(bar, st, len, vel, push);
            }
        }
    }
}

impl Composer {
    pub(super) fn horn_swells(&mut self, bar: &mut Bar) {
        let steps = self.steps;
        let mut st = 0;
        let i = self.intensity;
        while st < steps {
            let ch = self.chord(st);
            let mut end = st + 4;
            while end < st + 32 && self.chord(end) == ch {
                end += 4;
            }
            let changed = st > 0 || self.plan.bar.is_multiple_of(2) || self.prev_chord() != ch;
            if changed {
                let len = (end - st) as f32 - 0.5;
                self.horn_chord(bar, st, len, 0.5 + 0.25 * i, false);
                self.swelled = true;
            }
            let mut nx = st + 4;
            while nx < steps && self.chord(nx) == ch {
                nx += 4;
            }
            st = nx;
        }
    }

    // --- Kimes ----------------------------------------------------------
}
