//! Humanizing: phrase breathing, accents and chord spreads.

use super::*;

impl Composer {
    /// Players aren't sequencers: every phrase breathes (builds to its third
    /// bar, eases off on the fourth), downbeats lean in, no two notes land at
    /// quite the same strength, and chord tones spread by a few milliseconds
    /// instead of striking as one. Drums only get the small touch variation.
    pub(super) fn humanize(&mut self, bar: &mut Bar) {
        const BREATH: [f32; 4] = [0.97, 1.0, 1.05, 0.96];
        let breath = BREATH[usize::from(self.plan.bar) % 4];
        let tone = self.mood.tone();
        let step_len = self.step_len.max(1.0);
        let last = bar.len.saturating_sub(1);
        for i in 0..bar.events.len() {
            let r1 = self.rand();
            let r2 = self.rand();
            let e = &mut bar.events[i];
            let bus = e.inst.bus();
            if bus == Bus::Drums || [pk().shaker, pk().rim, pk().clap].contains(&e.inst) {
                // A drummer leans on the kick and snare, and rides the
                // cymbals and shakers well under them.
                let touch = match e.inst {
                    who if [pk().hat, pk().open_hat, pk().shaker, pk().ride].contains(&who) => 0.8,
                    who if [pk().kick, pk().snare].contains(&who) => 0.92,
                    _ => 1.0,
                };
                e.vel *= touch * (1.0 + 0.1 * (r1 - 0.5));
                // Feel (per mood): the backbeat drags behind the grid, and
                // the hats wander a little and lean on the beat, lighter on
                // the off-16ths. The kick stays put: it's the anchor.
                let ms = RATE as f32 / 1000.0;
                let step = (e.at as f32 / step_len).round() as u32;
                match e.inst {
                    who if [pk().snare, pk().clap, pk().rim, pk().ghost].contains(&who) => {
                        let lag = tone.lay_back_ms * (0.7 + 0.6 * r2);
                        e.at = (e.at + (lag * ms) as u32).min(last);
                    }
                    who if [pk().hat, pk().open_hat, pk().shaker, pk().ride].contains(&who) => {
                        let off = tone.loose_ms * (2.0 * r2 - 1.0) * ms;
                        e.at = ((e.at as f32 + off).max(0.0) as u32).min(last);
                        if step % 2 == 1 {
                            e.vel *= 0.86;
                        } else if step.is_multiple_of(4) {
                            e.vel *= 1.06;
                        }
                    }
                    _ => {}
                }
                continue;
            }
            let r3 = self.rand();
            // Releases don't all line up: each note lets go a little early or
            // late (sustained pads and strings less so).
            let hold = match bus {
                Bus::Pad => 0.9 + 0.2 * r3,
                _ => 0.72 + 0.36 * r3,
            };
            e.len = ((e.len as f32 * hold) as u32).max(1);
            // Play inside the range rather than at the top of it: the same
            // shape, with headroom above (0.73 -> 0.68, 1.0 -> 0.9).
            e.vel = 0.18 + 0.72 * e.vel.clamp(0.0, 1.0).powf(1.15);
            if e.inst == pk().flute {
                // The bottle sits back, a breath behind the band.
                e.vel *= 0.85;
            }
            let step = (e.at as f32 / step_len).round() as u32;
            let accent = match step {
                0 => 1.07,
                s if s % 4 == 0 => 1.03,
                s if s % 2 == 1 => 0.96,
                _ => 1.0,
            };
            e.vel = (e.vel * breath * accent * (1.0 + 0.16 * (r1 - 0.5))).clamp(0.05, 1.0);
            if !e.glide {
                // Keys and guitar spread their chords most; bass sits tight.
                let spread = match bus {
                    Bus::Keys | Bus::Guitar | Bus::Pad => 180.0,
                    Bus::Bass => 50.0,
                    _ => 90.0,
                };
                e.at = (e.at + (r2 * spread) as u32).min(last);
            }
        }
    }
}
