//! Humanizing: phrase breathing, accents and chord spreads.

use super::*;

impl Composer {
    /// Players aren't sequencers: every phrase breathes (builds to its third
    /// bar, eases off on the fourth), downbeats lean in, no two notes land at
    /// quite the same strength, and chord tones spread by a few milliseconds
    /// instead of striking as one. Drums only get the small touch variation.
    pub(super) fn humanize(&mut self, bar: &mut Bar) {
        let h = &self.style().feel.humanize;
        let breath = h
            .breath
            .get(usize::from(self.plan.bar) % h.breath.len().max(1))
            .copied()
            .unwrap_or(1.0);
        let tone = self.mood.tone();
        let beat = self.beat.max(1);
        let step_len = self.step_len.max(1.0);
        let last = bar.len.saturating_sub(1);
        for i in 0..bar.events.len() {
            let r1 = self.rand();
            let r2 = self.rand();
            let e = &mut bar.events[i];
            let bus = e.inst.bus();
            if bus == Bus::Drums || h.kit.contains(&e.inst) {
                // A drummer leans on the kick and snare, and rides the
                // cymbals and shakers well under them.
                let touch = h.touch.get(&e.inst).copied().unwrap_or(1.0);
                e.vel *= touch * (1.0 + h.touch_var * (r1 - 0.5));
                // Feel (per mood): the backbeat drags behind the grid, and
                // the hats wander a little and lean on the beat, lighter on
                // the off-16ths. The kick stays put: it's the anchor.
                let ms = RATE as f32 / 1000.0;
                let step = (e.at as f32 / step_len).round() as u32;
                if h.backbeat.contains(&e.inst) {
                    let lag = tone.lay_back_ms * (h.lay.0 + h.lay.1 * r2);
                    e.at = (e.at + (lag * ms) as u32).min(last);
                } else if h.loose.contains(&e.inst) {
                    let off = tone.loose_ms * (2.0 * r2 - 1.0) * ms;
                    e.at = ((e.at as f32 + off).max(0.0) as u32).min(last);
                    if step % 2 == 1 {
                        e.vel *= h.loose_vel.0;
                    } else if step.is_multiple_of(beat) {
                        e.vel *= h.loose_vel.1;
                    }
                }
                continue;
            }
            let r3 = self.rand();
            // Releases don't all line up: each note lets go a little early or
            // late (sustained pads and strings less so).
            let name = Bus::NAMES[(bus as usize).min(BUSES - 1)];
            let (base, range) = h.hold.get(name).copied().unwrap_or(h.hold_default);
            let hold = base + range * r3;
            e.len = ((e.len as f32 * hold) as u32).max(1);
            // Play inside the range rather than at the top of it: the same
            // shape, with headroom above (0.73 -> 0.68, 1.0 -> 0.9).
            let (c0, c1, cp) = h.curve;
            e.vel = c0 + c1 * e.vel.clamp(0.0, 1.0).powf(cp);
            if let Some(q) = h.quiet.get(&e.inst) {
                e.vel *= q;
            }
            let step = (e.at as f32 / step_len).round() as u32;
            let accent = match step {
                0 => h.accent.0,
                s if s % beat == 0 => h.accent.1,
                s if s % 2 == 1 => h.accent.2,
                _ => 1.0,
            };
            e.vel = (e.vel * breath * accent * (1.0 + h.wobble * (r1 - 0.5))).clamp(0.05, 1.0);
            if !e.glide {
                // Keys and guitar spread their chords most; bass sits tight.
                let spread = h.spread.get(name).copied().unwrap_or(h.spread_default);
                e.at = (e.at + (r2 * spread) as u32).min(last);
            }
        }
    }
}
