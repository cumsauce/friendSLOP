//! Chords: keys comping, guitar (bossa, picking), pads, arpeggios, ostinatos and horns.

use super::*;

impl Composer {
    /// Keys comping: each phrase picks one of the comp's rhythms, so the
    /// keys don't hammer the same figure for minutes.
    pub(super) fn write_comp(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Chords { comp, under } = &part.play else {
            return;
        };
        let i = self.intensity;
        let pats = &comp.rhythms;
        if pats.is_empty() {
            return;
        }
        let (lo, hi) = part.register;
        let pat = &pats[(self.plan.bar as usize / 4 + self.comp_var) % pats.len()];
        let steps = self.steps;
        let beat = self.beat;
        // Every chord change in the bar needs a hit at or just before it.
        let mut hits = [(0u8, 0u8, false); 12];
        let mut n = 0;
        for &h in pat.iter() {
            if (h.0 as u32) < steps && n < hits.len() {
                hits[n] = h;
                n += 1;
            }
        }
        for change in (1..self.full / beat).map(|b| b * beat) {
            if self.chord(change) != self.chord(change - 1)
                && !hits[..n]
                    .iter()
                    .any(|h| (h.0 as u32) <= change && h.0 as u32 + 2 >= change)
                && n < hits.len()
            {
                hits[n] = (change as u8, beat as u8, false);
                n += 1;
            }
        }
        // A chord anticipated on the last "and of 4" already rings.
        let pushed = std::mem::take(&mut self.pushed);
        for (k, &(st, ln, push)) in hits[..n].iter().enumerate() {
            if st == 0 && pushed {
                continue;
            }
            if k > 0 && i < 0.45 && !self.on_beat(st as u32) && self.chance(0.5 - i) {
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
            self.ep = voice_lead(ch, &self.ep, lo, hi);
            let accent = if self.on_beat(st as u32) { 0.75 } else { 0.62 };
            let vel = accent * (0.75 + 0.35 * i);
            let base = self.at(st as u32);
            let mut len = ln as f32 * 0.92;
            if push {
                // An anticipation rings over the bar line.
                len += 2.0;
            }
            let len = self.len(len);
            let strum = comp.spread;
            // Under a melody the keys leave its register to it: the top
            // note goes if it reaches into the lead's range.
            let under = under.is_some_and(|b| bar.bus[b.min(BUSES - 1)] > 0.0);
            for (j, &note) in self.ep.iter().enumerate() {
                if under && j == 3 && note >= self.lead_lo + 5 {
                    continue;
                }
                Self::push(
                    bar,
                    Ev {
                        at: base + j as u32 * strum,
                        inst: part.inst,
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
    pub(super) fn write_guitar(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Chords { comp, .. } = &part.play else {
            return;
        };
        let (lo, hi) = part.register;
        let steps = self.steps;
        let mut st = 0;
        while st < steps {
            let ch = self.chord(st);
            self.ep = voice_lead(ch, &self.ep, lo, hi);
            let at = self.at(st);
            if st % (2 * self.beat) == 0 || comp.order.is_empty() {
                let len = self.len(4.0);
                for (j, &n) in self.ep.iter().enumerate() {
                    Self::push(
                        bar,
                        Ev {
                            at: at + j as u32 * comp.spread,
                            inst: part.inst,
                            note: n,
                            vel: 0.8,
                            len,
                            glide: false,
                        },
                    );
                }
            } else {
                let order = &comp.order;
                let n = self.ep[order[(st / 2) as usize % order.len()] % 4] + 12;
                let len = self.len(2.0);
                Self::push(
                    bar,
                    Ev {
                        at,
                        inst: part.inst,
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
    pub(super) fn thumb(&self, ch: Chord) -> u8 {
        ch.root_from(self.style().feel.registers.thumb)
    }
}

impl Composer {
    /// The thumb's alternate bass: the fifth below the root's octave (the
    /// root again on an altered dominant, whose fifth is bent).
    pub(super) fn thumb_fifth(&self, ch: Chord) -> u8 {
        let root = self.thumb(ch);
        if ch.q.is_altered() {
            return root;
        }
        let f = root + ch.q.fifth();
        if f > self.style().feel.registers.thumb_top {
            f - 12
        } else {
            f
        }
    }
}

impl Composer {
    /// Bossa nova: thumb on one and three (root, then fifth), the fingers
    /// pinching a shell on the off-beats, a two-bar figure.
    pub(super) fn write_bossa(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Chords { comp, .. } = &part.play else {
            return;
        };
        let (lo, hi) = part.register;
        let inst = part.inst;
        let i = self.intensity;
        let steps = self.steps;
        let beat = self.beat;
        for st in [0u32, self.full / 2] {
            if st >= steps {
                continue;
            }
            let ch = self.chord(st);
            let note = if st == 0 || self.chord(0) != ch {
                self.thumb(ch)
            } else {
                self.thumb_fifth(ch)
            };
            let at = self.at(st);
            let len = self.len(7.5);
            Self::push(bar, ev(at, inst, note, 0.62 + 0.2 * i, len));
        }
        let second = self.plan.bar % 2 == 1;
        let Some(fig) = comp
            .fingers
            .get(self.plan.bar as usize % comp.fingers.len().max(1))
        else {
            return;
        };
        for (k, &st) in fig.iter().enumerate() {
            let st = st as u32;
            if st >= steps {
                continue;
            }
            // The last pinch before a new chord anticipates it.
            let ch = if st % beat == beat - 1 || st >= self.full - 2 {
                self.chord(st + 1)
            } else {
                self.chord(st)
            };
            self.gtr = voice_lead(ch, &self.gtr, lo, hi);
            let at = self.at(st);
            let len = self.len(2.5);
            let vel = 0.42 + 0.12 * i + if self.on_beat(st) { 0.06 } else { 0.0 };
            // The top string moves in the figure's second bar (a little
            // melody in the comping), back home for the next chord.
            let mut shell = [self.gtr[1], self.gtr[2], self.gtr[3]];
            if second && k >= 3 {
                shell[2] = ch.scale_step(shell[2], if k == 3 { 1 } else { -1 });
            }
            // Fingers together, but not quite.
            for (j, &n) in shell.iter().enumerate() {
                Self::push(bar, ev(at + j as u32 * 40, inst, n, vel, len));
            }
        }
    }
}

impl Composer {
    /// Fingerpicking (p-i-m-a): the thumb on the beat, the fingers rolling
    /// up and down the chord in eighths, each string left to ring.
    pub(super) fn write_picking(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Chords { comp, .. } = &part.play else {
            return;
        };
        if comp.rolls.is_empty() {
            return;
        }
        let (lo, hi) = part.register;
        let inst = part.inst;
        let i = self.intensity;
        let steps = self.steps;
        let half = 2 * self.beat;
        // Which finger plays each eighth: 0 = thumb, 1..=3 = strings up.
        let roll = &comp.rolls[(self.plan.bar as usize / 2 + self.comp_var) % comp.rolls.len()];
        if roll.is_empty() {
            return;
        }
        let mut st = 0;
        while st < steps {
            let ch = self.chord(st);
            if st % half == 0 || self.chord(st.saturating_sub(2)) != ch {
                self.gtr = voice_lead(ch, &self.gtr, lo, hi);
            }
            let finger = roll[(st / 2) as usize % roll.len()] % 4;
            // Rests now and then when calm, so it breathes.
            if finger != 0 && i < 0.4 && self.chance(0.18) {
                st += 2;
                continue;
            }
            let (note, vel, ring) = if finger == 0 {
                let n = if st % half == self.beat && self.chord(st - st % half) == ch {
                    self.thumb_fifth(ch)
                } else {
                    self.thumb(ch)
                };
                (n, 0.6 + 0.2 * i, 8.0)
            } else {
                (self.gtr[finger as usize], 0.42 + 0.15 * i, 6.0)
            };
            let at = self.at(st);
            let len = self.len(ring);
            Self::push(bar, ev(at, inst, note, vel, len));
            // A hammered sixteenth pickup into the next beat, when there's
            // some energy.
            if finger == 3 && i > 0.45 && self.chance(0.25) && st + 1 < steps {
                let n2 = ch.scale_step(self.gtr[2], 1);
                Self::push(bar, ev(self.at(st + 1), inst, n2, vel * 0.8, self.len(3.0)));
            }
            st += 2;
        }
    }
}

impl Composer {
    pub(super) fn write_pad(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Pad { under, after_swell } = &part.play else {
            return;
        };
        if self.swelled && *after_swell {
            return;
        }
        let (lo, hi) = part.register;
        let steps = self.steps;
        let beat = self.beat;
        let mut st = 0;
        while st < steps {
            let ch = self.chord(st);
            // Only re-articulate when the chord actually changes.
            let changed = st > 0 || self.plan.bar == 0 || self.prev_chord() != ch;
            let mut end = st + beat;
            while end < st + 4 * self.full && self.chord(end) == ch {
                end += beat;
            }
            let mut nx = st + beat;
            while nx < steps && self.chord(nx) == ch {
                nx += beat;
            }
            if changed {
                self.pad = voice_lead(ch, &self.pad, lo, hi);
                let at = self.at(st);
                let len = self.len((end - st) as f32 + 1.0);
                // Under a melody the pad thins to its two lowest voices (an
                // octave down, under the lead rather than in it) and sits back;
                // on its own it spreads out up high.
                let under = under.is_some_and(|b| bar.bus[b.min(BUSES - 1)] > 0.0);
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
                            inst: part.inst,
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
            return Chord::new(0, Quality::NONE);
        }
        self.plan.chord_at(self.plan.bar - 1, self.full - 1)
    }
}

impl Composer {
    /// Bells, harp or chimes arpeggiating the chord.
    pub(super) fn write_arp(&mut self, bar: &mut Bar, part: &'static Part) {
        let inst = part.inst;
        let (from, top) = part.register;
        let i = self.intensity;
        let every = if i > 0.5 {
            (self.beat / 2).max(1)
        } else {
            self.beat.max(1)
        };
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
            let n = ch.root_from(from) + tones[idx] + 12 * (k as u8 / 4 % 2);
            let at = self.at(st);
            Self::push(
                bar,
                Ev {
                    at,
                    inst,
                    note: n.min(top),
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
    pub(super) fn write_victory_bells(&mut self, bar: &mut Bar, part: &'static Part) {
        let ch = self.chord(0);
        let (from, top) = part.register;
        for (k, t) in ch.q.voicing().iter().enumerate() {
            let at = self.at(k as u32 * 2);
            let n = ch.root_from(from) + t;
            Self::push(
                bar,
                Ev {
                    at,
                    inst: part.inst,
                    note: n.min(top),
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
    pub(super) fn write_ostinato(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Ostinato { figs } = &part.play else {
            return;
        };
        if figs.is_empty() {
            return;
        }
        let (from, top) = part.register;
        let pair = &figs[self.comp_var % figs.len()];
        let Some(fig) = pair.get(usize::from(self.plan.bar % 2 == 1) % pair.len().max(1)) else {
            return;
        };
        let i = self.intensity;
        for &(st, t) in fig {
            if st as u32 >= self.steps || (i < 0.3 && !self.on_beat(st as u32) && self.chance(0.3))
            {
                continue;
            }
            let ch = self.chord(st as u32);
            let v = ch.q.voicing();
            let tones = [0, v[0], v[1], v[2], v[3]];
            let mut n = ch.root_from(from) + tones[t as usize % 5];
            while n > top.max(from.saturating_add(12)) {
                n -= 12;
            }
            let at = self.at(st as u32);
            let vel = if self.on_beat(st as u32) { 0.8 } else { 0.6 } * (0.85 + 0.3 * self.rand());
            Self::push(
                bar,
                Ev {
                    at,
                    inst: part.inst,
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
    pub(super) fn write_horns(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Horns {
            style,
            stabs,
            swells,
        } = &part.play
        else {
            return;
        };
        let ctx = self.ctx();
        let stab_now = *stabs && library::any(&style.stabs, &ctx);
        let swell_now = *swells && !stab_now && library::any(&style.swells, &ctx);
        if self.plan.kind == Section::Victory {
            self.horn_chord(bar, part, 0, (self.full - 1) as f32, 1.0, false);
        } else if stab_now {
            self.horn_stabs(bar, part, style);
        } else if swell_now {
            self.horn_swells(bar, part);
        }
    }
}

impl Composer {
    /// One section chord at `st`, `len` 16ths long, voice-led from the
    /// last (`top`: block-voice under this melody note instead).
    pub(super) fn horn_chord(
        &mut self,
        bar: &mut Bar,
        part: &'static Part,
        st: u32,
        len: f32,
        vel: f32,
        push: bool,
    ) {
        self.horn_hit(bar, part, st, len, vel, push, None);
    }
}

impl Composer {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn horn_hit(
        &mut self,
        bar: &mut Bar,
        part: &'static Part,
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
            None => no_top_rub(voice_lead(ch, &self.horn, part.register.0, part.register.1)),
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
                    inst: part.inst,
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
    pub(super) fn lead_onsets(&self, bar: &Bar, avoid: &[Inst]) -> u32 {
        let mut mask = 0u32;
        for e in &bar.events {
            if avoid.contains(&e.inst) {
                let st = (e.at as f32 / self.step_len).round() as u32;
                mask |= 1 << st.min(31);
            }
        }
        mask
    }
}

impl Composer {
    pub(super) fn horn_stabs(
        &mut self,
        bar: &mut Bar,
        part: &'static Part,
        h: &library::HornStyle,
    ) {
        // Shouts answer the lead in the bars where it holds or rests;
        // pushes just kick the off-beats elsewhere.
        if h.pushes.is_empty() {
            return;
        }
        let phrase = self.plan.bar % 4;
        let busy = self.lead_onsets(bar, &h.avoid);
        let free = |st: u8| {
            let st = st.min(31);
            busy & (1 << st) == 0 && (st == 0 || busy & (1 << (st - 1)) == 0)
        };
        let mut hits = [(0u8, 0u8); 6];
        let mut n = 0;
        let mut shout = false;
        if phrase % 2 == 1 && !h.shouts.is_empty() {
            let fig = &h.shouts[(self.comp_var + phrase as usize / 2) % h.shouts.len()];
            for &x in fig.iter().filter(|x| free(x.0)) {
                if n < hits.len() {
                    hits[n] = x;
                    n += 1;
                }
            }
            shout = n >= 3;
        }
        if !shout {
            n = 0;
            for &x in h.pushes[self.comp_var % h.pushes.len()]
                .iter()
                .filter(|x| free(x.0))
            {
                if n < hits.len() {
                    hits[n] = x;
                    n += 1;
                }
            }
        }
        let i = self.intensity;
        let flat: &[i8] = &[0];
        let contour: &[i8] = if h.contours.is_empty() {
            flat
        } else {
            &h.contours[(self.comp_var + self.plan.bar as usize) % h.contours.len()]
        };
        let (start, lo, hi) = h.top;
        let mut top = self.chord(hits[0].0 as u32).nearest_strong(start);
        for (k, &(st, ln)) in hits[..n].iter().enumerate() {
            let st = st as u32;
            if st >= self.steps {
                continue;
            }
            let push = st + ln as u32 >= self.full && st >= self.full - 2;
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
                top = chord_tone_step(ch, top, step).clamp(lo, hi.max(lo));
                let t = Some(ch.nearest_strong(top));
                self.horn_hit(bar, part, st, len, vel, push, t);
            } else {
                self.horn_chord(bar, part, st, len, vel, push);
            }
        }
    }
}

impl Composer {
    pub(super) fn horn_swells(&mut self, bar: &mut Bar, part: &'static Part) {
        let steps = self.steps;
        let beat = self.beat;
        let mut st = 0;
        let i = self.intensity;
        while st < steps {
            let ch = self.chord(st);
            let mut end = st + beat;
            while end < st + 2 * self.full && self.chord(end) == ch {
                end += beat;
            }
            let changed = st > 0 || self.plan.bar.is_multiple_of(2) || self.prev_chord() != ch;
            if changed {
                let len = (end - st) as f32 - 0.5;
                self.horn_chord(bar, part, st, len, 0.5 + 0.25 * i, false);
                self.swelled = true;
            }
            let mut nx = st + beat;
            while nx < steps && self.chord(nx) == ch {
                nx += beat;
            }
            st = nx;
        }
    }

    // --- Kimes ----------------------------------------------------------
}
