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
            && step >= 8
            && let Some(t) = self.tail
        {
            return t;
        }
        let beat = (bar as u32 * 4 + step / 4) % self.prog_beats();
        let mut acc = 0;
        for &(off, q, b) in self.prog {
            if beat < acc + b as u32 {
                return Chord::new(self.key + off, q);
            }
            acc += b as u32;
        }
        Chord::new(self.key, Maj9)
    }

    pub(super) fn first(&self) -> Chord {
        self.chord_at(0, 0)
    }
}

/// The key ladder: semitones above the tune's key for each storey climbed
/// (up the major scale: do, re, mi, fa, sol).
pub(super) const LIFT: [u8; 5] = [0, 2, 4, 5, 7];
/// The highest storey the key follows.
pub const LIFT_MAX: u8 = LIFT.len() as u8 - 1;
impl Composer {
    /// Plan the section after `prev` in mood `m`.
    pub(super) fn plan_section(&mut self, m: Mood, prev: Section, fresh: bool) -> Plan {
        // A new visit picks a song; after a breakdown the band may move on
        // to another one.
        let n = songs(m);
        if n > 1 && fresh {
            self.song[m as usize % Mood::ALL.len()] = self.pick(n) as u8;
        } else if n > 1 && prev == Section::Breakdown && self.chance(0.6) {
            let now = self.song[m as usize % Mood::ALL.len()] as usize;
            self.song[m as usize % Mood::ALL.len()] = ((now + 1 + self.pick(n - 1)) % n) as u8;
        }
        let s = style_song(m, self.song[m as usize % Mood::ALL.len()]);
        let i = self.intensity;
        let kind = if m == Mood::Still {
            Section::Still
        } else if fresh {
            if i < 0.6 && !m.urgent() {
                Section::Intro
            } else {
                Section::A
            }
        } else {
            match prev {
                Section::Intro | Section::Breakdown | Section::Victory | Section::Still => {
                    Section::A
                }
                Section::A => Section::Hook,
                Section::Hook => {
                    if self.chance(0.6) {
                        Section::B
                    } else if i > 0.45 && s.lead != LeadKind::Silent {
                        Section::Solo
                    } else {
                        Section::A
                    }
                }
                Section::B => {
                    if i > 0.45 && s.lead != LeadKind::Silent {
                        Section::Solo
                    } else if i < 0.35 {
                        Section::Breakdown
                    } else {
                        Section::Hook
                    }
                }
                Section::Solo => {
                    if self.chance(0.3) {
                        Section::Breakdown
                    } else {
                        Section::Hook
                    }
                }
            }
        };
        if fresh {
            self.key_shift = 0;
        } else if kind != Section::Still && self.chance(s.modulate) {
            self.key_shift += if self.chance(0.5) { 1 } else { 2 };
            if self.key_shift > 4 {
                // Back down home (a fourth up is the same pitch classes).
                self.key_shift = 0;
            }
        }
        let key = (s.key + self.key_shift) % 12;
        let (prog, bars) = match kind {
            Section::Intro => (s.a, 4),
            Section::A => (s.a, 8),
            Section::Hook => (s.hook, 8),
            Section::B => (s.b, 8),
            Section::Solo => (if self.chance(0.5) { s.a } else { s.b }, 8),
            Section::Breakdown => (s.a, 4),
            Section::Victory => (VICTORY, 2),
            Section::Still => (s.a, 2),
        };
        let fixed = (kind == Section::Still).then(|| self.chord(0));
        if matches!(kind, Section::B | Section::Solo) || fresh {
            self.motif = self.new_motif();
        }
        self.comp_var = self.pick(4);
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
        }
    }
}

impl Composer {
    /// An approach chord into `to`: its V7alt, its tritone sub, or (for
    /// minor targets) a V7b9.
    pub(super) fn approach(&mut self, to: Chord) -> Chord {
        if to.q.is_minor() {
            Chord::new(to.root + 7, Dom7b9)
        } else if self.chance(0.5) {
            Chord::new(to.root + 1, Dom13s11)
        } else {
            Chord::new(to.root + 7, Dom7alt)
        }
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
            && !matches!(self.mood, Mood::Menu | Mood::Still)
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
        let d = LIFT[to as usize] as i32 - LIFT[self.lift as usize] as i32;
        let next = self.raw_chord(16);
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
        let up = LIFT[self.lift as usize];
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
