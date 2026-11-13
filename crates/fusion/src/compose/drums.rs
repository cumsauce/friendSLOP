//! The kit: a groove's lanes, fills and the hand percussion beside it.

use super::library::{Fill, Lane};
use super::*;

impl Composer {
    /// Play the groove of a Drums part: kick, snare, hats (or ride) and its
    /// extra lanes, step by step, with every lane part of the song (shaker,
    /// hand drums, percussion) played in the same steps.
    pub(super) fn write_drums(
        &mut self,
        bar: &mut Bar,
        part: &'static Part,
        fill: bool,
        crash: bool,
    ) {
        let G::Drums {
            groove: g,
            kit,
            ride: ride_on,
            clap: clap_on,
        } = &part.play
        else {
            return;
        };
        let (g, kit) = (*g, *kit);
        let s = self.style();
        let i = self.intensity;
        let odd_bar = self.plan.bar % 2 == 1;
        let soft = g.soft;
        // Where the backbeat sits: laid-back grooves drag it a hair.
        let lay = g.lay;
        let full = self.full;
        let beat = self.beat.max(1);
        let fill_from = if fill {
            if i > 0.6 { full / 2 } else { full - beat }
        } else {
            99
        };
        let busy = usize::from(odd_bar && i > 0.3);
        let Some(kick) = g.kick.get(busy).or(g.kick.first()) else {
            return;
        };
        let steps = self.steps.min(full);
        let ctx = self.ctx();
        // The ride takes over from the hats in solos and flat-out duels.
        let ride = *ride_on && library::any(g.ride_when.as_ref().unwrap_or(&kit.ride_when), &ctx);
        let clap = *clap_on && library::any(g.clap_when.as_ref().unwrap_or(&kit.clap_when), &ctx);
        let lanes = self.lanes(&ctx);
        let hat = g.hat.as_bytes();
        for st in 0..steps {
            let k = st as usize;
            let at = self.at(st);
            if st >= fill_from {
                self.fill_step(bar, g, kit, st, fill_from, steps);
                continue;
            }
            if let Some(v) = kit.vel(kick, k) {
                Self::push(bar, hit(at, kit.kick.0, kit.kick.1, v * soft));
            }
            match g.snare.as_bytes().get(k) {
                Some(b'g') if i > 0.4 => {
                    let v = 0.5 + 0.3 * self.rand();
                    Self::push(bar, hit(at + 40, kit.ghost.0, kit.ghost.1, v * soft));
                }
                Some(b'x') | Some(b'o') => {
                    let v = kit.vel(&g.snare, k).unwrap_or(0.8);
                    Self::push(bar, hit(at + lay, kit.snare.0, kit.snare.1, v * soft));
                    if clap && st % (2 * beat) == beat {
                        Self::push(bar, hit(at + lay + 25, kit.clap.0, kit.clap.1, 0.75));
                    }
                }
                _ => {}
            }
            let jitter = (self.rand() * 80.0) as u32;
            match hat.get(k) {
                Some(_) if ride => {
                    // Ride: quarters and the "and"s, bell-ish accents on the
                    // beat, foot hat on two and four.
                    if st % 2 == 0 {
                        let v = if st % beat == 0 { 0.9 } else { 0.62 } * (0.9 + 0.2 * self.rand());
                        Self::push(bar, hit(at + jitter, kit.ride.0, kit.ride.1, v));
                    }
                    if st % (2 * beat) == beat {
                        Self::push(bar, hit(at, kit.hat.0, kit.hat.1, 0.35));
                    }
                }
                Some(b'O') if i > 0.5 => {
                    Self::push(bar, hit(at, kit.open_hat.0, kit.open_hat.1, 0.7 * soft));
                }
                Some(c) if *c != b'.' => {
                    // Below mid intensity, 16th hats thin to 8ths.
                    let thin = st % 2 == 1
                        && i < 0.45
                        && hat.len() == full as usize
                        && hat.get(1).is_some_and(|c| *c != b'.');
                    if !thin {
                        let v = kit.vel(&g.hat, k).unwrap_or(0.6) * (0.85 + 0.3 * self.rand());
                        Self::push(bar, hit(at + jitter, kit.hat.0, kit.hat.1, v * soft));
                    }
                }
                _ => {}
            }
            for (inst, note, pats) in &g.extra {
                let Some(pat) = pats.get(usize::from(odd_bar) % pats.len().max(1)) else {
                    continue;
                };
                if let Some(v) = kit.vel(pat, k) {
                    let quiet = s.lib_hand(*inst).and_then(|h| h.quiet);
                    let note = match quiet {
                        Some(q) if v < 0.5 => q,
                        _ => *note,
                    };
                    Self::push(bar, hit(at, *inst, note, v * soft));
                }
            }
            self.lane_step(bar, &lanes, &ctx, st, at, jitter, soft);
        }
        if crash {
            Self::push(bar, hit(0, kit.crash.0, kit.crash.1, 0.9));
            Self::push(bar, hit(0, kit.kick.0, kit.kick.1, 1.0));
        }
    }

    /// The song's lane parts playing this bar.
    fn lanes(&self, ctx: &Ctx) -> Vec<&'static Part> {
        self.style()
            .parts
            .iter()
            .filter(|p| matches!(p.play, G::Lane { .. }) && p.on(ctx))
            .collect()
    }

    /// Lanes on their own, when the song has no kit to play them in.
    pub(super) fn write_lanes(&mut self, bar: &mut Bar, only: Option<&'static Part>) {
        let lanes: Vec<&'static Part> = only.into_iter().collect();
        let ctx = self.ctx();
        for st in 0..self.steps {
            let at = self.at(st);
            let jitter = (self.rand() * 80.0) as u32;
            self.lane_step(bar, &lanes, &ctx, st, at, jitter, 1.0);
        }
    }

    /// One step of every lane in `lanes`.
    #[allow(clippy::too_many_arguments)]
    fn lane_step(
        &mut self,
        bar: &mut Bar,
        lanes: &[&'static Part],
        ctx: &Ctx,
        st: u32,
        at: u32,
        jitter: u32,
        soft: f32,
    ) {
        let i = self.intensity;
        let odd_bar = self.plan.bar % 2 == 1;
        for part in lanes {
            let G::Lane { lane } = &part.play else {
                continue;
            };
            if !library::any(&lane.when, ctx) {
                continue;
            }
            if st % 2 == 1 && lane.odd_above.is_some_and(|a| i <= a) {
                continue;
            }
            let Some((note, v)) = lane_hit(lane, st as usize, odd_bar) else {
                continue;
            };
            let v = if lane.rand_vel {
                v * (0.85 + 0.3 * self.rand())
            } else {
                v
            };
            let note = if lane.off_note.is_some() && !self.on_beat(st) {
                lane.off_note.unwrap_or(note)
            } else {
                note
            };
            let (inst, note) = if lane.hand {
                self.style().hand_note(part.inst, lane, note)
            } else {
                (part.inst, note)
            };
            let wander = jitter.checked_div(lane.jitter).unwrap_or(0);
            let at = at.saturating_sub(lane.early) + wander;
            let v = if lane.soft { v * soft } else { v };
            Self::push(bar, hit(at, inst, note, v));
        }
    }
}

/// A lane's (note, velocity) at step `st`, if it plays there.
fn lane_hit(lane: &Lane, st: usize, odd_bar: bool) -> Option<(u8, f32)> {
    let n = lane.bars.len();
    if n == 0 {
        return None;
    }
    let pat = lane.bars[usize::from(odd_bar) % n].as_bytes();
    let c = *pat.get(st % lane.steps.max(1) as usize)?;
    lane.hits.get(&(c as char)).copied()
}

impl Style {
    pub(super) fn lib_hand(&self, inst: Inst) -> Option<&'static library::Hand> {
        library::lib().file.hands.get(&inst)
    }

    /// The tumbao's stroke `note` (on a conga) moved to the hand drum `hand`:
    /// its low, high and slap heads.
    pub(super) fn hand_note(&self, hand: Inst, lane: &Lane, note: u8) -> (Inst, u8) {
        let Some(h) = self.lib_hand(hand) else {
            return (hand, note);
        };
        let Some((low, high, slap)) = h.notes else {
            return (hand, note);
        };
        let n = if note >= lane.heads.1 {
            slap
        } else if note >= lane.heads.0 {
            high
        } else {
            low
        };
        (hand, n)
    }
}

impl Composer {
    /// One step of a drum fill: snare 16ths swelling, or toms rolling down.
    pub(super) fn fill_step(
        &mut self,
        bar: &mut Bar,
        g: &library::Groove,
        kit: &library::Kit,
        st: u32,
        from: u32,
        steps: u32,
    ) {
        let span = (steps - from).max(1) as f32;
        let p = (st - from) as f32 / span;
        let at = self.at(st);
        let (snare, sn) = kit.snare;
        let (kick, kn) = kit.kick;
        match g.fill {
            Fill::Ghost => {
                if st.is_multiple_of(2) {
                    Self::push(bar, hit(at, kit.ghost.0, kit.ghost.1, 0.5 + 0.4 * p));
                }
            }
            Fill::Rolls => {
                let kind = (self.plan.bar / 4 + self.bars_played as u16 / 8) % 3;
                match kind {
                    0 => Self::push(bar, hit(at, snare, sn, 0.45 + 0.55 * p)),
                    1 => {
                        let n = kit.rolls.len();
                        let k = ((p * n as f32) as usize).min(n.saturating_sub(1));
                        let note = kit.rolls.get(k).copied().unwrap_or(kit.tom_mid);
                        Self::push(bar, hit(at, kit.tom, note, 0.7 + 0.3 * p));
                    }
                    _ => {
                        if st.is_multiple_of(2) {
                            Self::push(bar, hit(at, snare, sn, 0.8));
                            Self::push(bar, hit(at, kick, kn, 0.8));
                        } else {
                            Self::push(bar, hit(at, kit.tom, kit.tom_mid, 0.6));
                        }
                    }
                }
                if self.on_beat(st) {
                    Self::push(bar, hit(at, kick, kn, 0.7));
                }
            }
        }
    }
}
