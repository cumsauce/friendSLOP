//! Song form: section plans, transitions between moods, and the key ladder (lift).

use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct Plan {
    pub(super) kind: Section,
    pub(super) bars: u16,
    pub(super) bar: u16,
    pub(super) key: u8,
    pub(super) prog: Prog,
    pub(super) fixed: Option<Chord>,
    /// Approach chord over the second half of the last bar.
    pub(super) tail: Option<Chord>,
    /// Steps per bar, and per beat.
    pub(super) steps: u32,
    pub(super) beat: u32,
}

impl Plan {
    pub(super) fn prog_beats(&self) -> u32 {
        self.prog.iter().map(|c| c.2 as u32).sum::<u32>().max(1)
    }

    pub(super) fn chord_at(&self, bar: u16, step: u32) -> Chord {
        if let Some(c) = self.fixed {
            return c;
        }
        if bar + 1 >= self.bars
            && step >= self.steps / 2
            && let Some(t) = self.tail
        {
            return t;
        }
        let per = self.beat.max(1);
        let beats = (self.steps / per).max(1);
        let beat = (bar as u32 * beats + step / per) % self.prog_beats();
        let mut acc = 0;
        for &(off, q, b) in self.prog {
            if beat < acc + b as u32 {
                return Chord::new(self.key + off, q);
            }
            acc += b as u32;
        }
        Chord::new(self.key, Quality::NONE)
    }

    pub(super) fn first(&self) -> Chord {
        self.chord_at(0, 0)
    }
}

/// The highest storey the game reports (the form's `lift` ladder says how
/// far up the key goes on each; past its top it stays there).
pub const LIFT_MAX: u8 = 4;

/// The form's victory cadence.
pub(super) fn victory(form: &'static library::Form) -> Prog {
    if form.victory.is_empty() {
        FALLBACK_PROG
    } else {
        &form.victory
    }
}
impl Composer {
    /// Plan the section after `prev` in mood `m`.
    pub(super) fn plan_section(&mut self, m: Mood, prev: Section, fresh: bool) -> Plan {
        // A new visit picks a song; after a breakdown the band may move on
        // to another one.
        let n = self.song_count(m);
        if n > 1 && fresh {
            let pick = self.pick(n) as u8;
            self.set_song(m, pick);
        } else if n > 1
            && prev == Section::Breakdown
            && self.chance(self.song_style(m, self.song_of(m)).form.new_song)
        {
            let now = self.song_of(m) as usize;
            let next = ((now + 1 + self.pick(n - 1)) % n) as u8;
            self.set_song(m, next);
        }
        let s = self.song_style(m, self.song_of(m));
        let form = s.form;
        let ctx = Ctx {
            section: prev,
            intensity: self.intensity,
            bar: 0,
            lead: s.has_lead(),
            urgent: m.urgent(),
        };
        let kind = if m.hold() {
            Section::Still
        } else if !form.cycle.is_empty() {
            // A fixed song shape: round and round.
            let at = form.cycle.iter().position(|k| *k == prev);
            match at {
                Some(k) if !fresh => form.cycle[(k + 1) % form.cycle.len()],
                _ => form.cycle[0],
            }
        } else if fresh {
            self.follow(&form.start, &ctx).unwrap_or(Section::A)
        } else {
            match form.next.get(&prev) {
                Some(edges) => self.follow(edges, &ctx).unwrap_or(Section::A),
                None => Section::A,
            }
        };
        if fresh {
            self.key_shift = 0;
        } else if kind != Section::Still && self.chance(s.modulate) {
            self.key_shift += if self.chance(form.modulate_half) {
                1
            } else {
                2
            };
            if self.key_shift > form.modulate_top {
                // Back down home (a fourth up is the same pitch classes).
                self.key_shift = 0;
            }
        }
        let key = (s.key + self.key_shift) % 12;
        let plan = form.sections.get(&kind);
        let bars = plan.map_or(8, |p| p.bars.max(1));
        let slots: &[library::Slot] = plan.map_or(&[], |p| &p.prog);
        let slot = match slots.len() {
            0 => library::Slot::A,
            1 => slots[0],
            n => slots[self.pick(n)],
        };
        let prog = match slot {
            library::Slot::A => s.a,
            library::Slot::B => s.b,
            library::Slot::Hook => s.hook,
        };
        let fixed = (kind == Section::Still).then(|| self.chord(0));
        if plan.is_some_and(|p| p.motif) || fresh {
            self.motif = self.new_motif();
        }
        self.comp_var = self.pick(form.variants.max(1));
        if fresh {
            self.hook = self.new_motif();
        }
        Plan {
            kind,
            bars,
            bar: 0,
            key,
            prog,
            fixed,
            tail: None,
            steps: s.steps,
            beat: (s.steps / s.beats).max(1),
        }
    }

    /// The first edge whose condition holds and whose chance comes up.
    fn follow(&mut self, edges: &[library::Edge], ctx: &Ctx) -> Option<Section> {
        for e in edges {
            if !e.when.holds(ctx) {
                continue;
            }
            match e.chance {
                Some(p) if !self.chance(p) => continue,
                _ => return Some(e.to),
            }
        }
        None
    }
}

impl Composer {
    /// An approach chord into `to`: its V7alt, its tritone sub, or (for
    /// minor targets) a V7b9.
    pub(super) fn approach(&mut self, to: Chord) -> Chord {
        let a = &self.style().form.approach;
        let list = if to.q.is_minor() { &a.minor } else { &a.other };
        let (up, q) = match list.len() {
            0 => (7, Quality::NONE),
            1 => list[0],
            n => list[self.pick(n)],
        };
        Chord::new(to.root + up, q)
    }
}

impl Composer {
    /// Follow the player up and down: land a modulation prepared in the
    /// last bar, then, if the player is on another storey, prepare the next
    /// one (a step at a time) over this bar's second half.
    pub(super) fn lift_step(&mut self, want: u8, new_mood: bool) {
        self.landed = false;
        if let Some(to) = self.lift_to.take() {
            self.lift = to;
            self.landed = true;
        }
        let last = self.plan.bar + 1 >= self.plan.bars;
        let free = !matches!(self.plan.kind, Section::Victory | Section::Still)
            && self.mood.flags().lift
            && self.switching.is_none()
            && !new_mood
            && !(last && self.plan.tail.is_some());
        if want == self.lift || !free {
            return;
        }
        let to = if want > self.lift {
            self.lift + 1
        } else {
            self.lift - 1
        };
        let form = self.style().form;
        let d = form.lift(to) as i32 - form.lift(self.lift) as i32;
        let next = self.raw_chord(self.full);
        let target = Chord::new((next.root as i32 + d).rem_euclid(12) as u8, next.q);
        self.bridge = self.approach(target);
        self.lift_to = Some(to);
    }
}

impl Composer {
    /// Play the bar in the key of the storey the player is on. Past a
    /// fourth up the bass goes down instead, to keep its weight, but never
    /// below a bass guitar's low E.
    pub(super) fn transpose(&self, bar: &mut Bar) {
        const LOW_E: u8 = 28;
        let up = self.style().form.lift(self.lift);
        if up == 0 {
            return;
        }
        for e in bar.events.iter_mut() {
            match e.inst.bus() {
                Bus::Drums => {}
                Bus::Bass if up >= 5 && e.note + up >= LOW_E + 12 => e.note = e.note + up - 12,
                _ => e.note = (e.note + up).min(108),
            }
        }
    }
}
