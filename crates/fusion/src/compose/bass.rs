//! The bass line.

use super::*;

impl Composer {
    pub(super) fn write_bass(&mut self, bar: &mut Bar) {
        let s = self.style();
        let inst = s.bass_inst;
        let i = self.intensity;
        let pick = if s.bass.len() > 1 && self.chance(0.35) {
            self.pick(s.bass.len())
        } else {
            (self.plan.bar as usize / 4) % s.bass.len()
        };
        let pat = s.bass[pick].as_bytes();
        let steps = self.steps.min(16) as usize;
        // At the end of a phrase the bass sometimes fills the last beat.
        let fill = steps == 16
            && self.plan.bar % 4 == 3
            && i > 0.45
            && self.switching.is_none()
            && self.chance(0.45);
        let fill_at = if fill { 12 } else { steps };
        let mut prev: Option<u8> = None;
        for st in 0..fill_at {
            let c = pat[st];
            if matches!(c, b'.' | b'-') {
                if c == b'.' {
                    prev = None;
                }
                continue;
            }
            let hold = pat[st + 1..steps]
                .iter()
                .take_while(|&&b| b == b'-')
                .count();
            let ch = self.chord(st as u32);
            let root = ch.root_from(31);
            let (note, vel, len) = match c {
                b'R' => (root, 0.8, if hold > 0 { hold as f32 + 0.9 } else { 0.7 }),
                b'P' => (root + 12, 1.0, 0.6),
                b'G' => (root, 0.25, 0.25),
                b'5' => (root + ch.q.fifth(), 0.75, hold as f32 + 0.8),
                b'3' => (root + ch.q.third(), 0.75, hold as f32 + 0.8),
                b'7' => (root + ch.q.seventh(), 0.8, 0.8),
                b'A' => {
                    let next = self.chord(st as u32 + 1).root_from(31);
                    let n = if self.chance(0.5) { next - 1 } else { next + 1 };
                    (n, 0.7, hold as f32 + 0.8)
                }
                _ => continue,
            };
            // A fretless slides into nearby notes instead of re-plucking.
            let glide = inst == pk().fretless
                && prev.is_some_and(|p| (p as i32 - note as i32).abs() <= 5 && p != note)
                && self.chance(0.35);
            prev = Some(note);
            let at = self.at(st as u32);
            let len = self.len(len);
            Self::push(
                bar,
                Ev {
                    at,
                    inst,
                    note,
                    vel: vel * (0.85 + 0.25 * i),
                    len,
                    glide,
                },
            );
        }
        if fill {
            // Four 16ths: up the chord to the octave, or down from it, with
            // the last a half step from the next root.
            let ch = self.chord(12);
            let root = ch.root_from(31);
            let next = self.chord(16).root_from(31);
            let line = if self.chance(0.5) {
                [
                    root + ch.q.fifth(),
                    root + ch.q.seventh(),
                    root + 12,
                    next + 11,
                ]
            } else {
                [
                    root + 12,
                    root + ch.q.seventh(),
                    root + ch.q.fifth(),
                    next + 1,
                ]
            };
            for (k, n) in line.into_iter().enumerate() {
                let st = 12 + k as u32;
                let at = self.at(st);
                let len = self.len(0.85);
                Self::push(
                    bar,
                    Ev {
                        at,
                        inst,
                        note: n,
                        vel: if k == 0 { 0.85 } else { 0.7 },
                        len,
                        glide: inst == pk().fretless && k == 3,
                    },
                );
            }
        }
    }

    // --- Keys -----------------------------------------------------------
}
