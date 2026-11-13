//! The bass line.

use super::*;

impl Composer {
    pub(super) fn write_bass(&mut self, bar: &mut Bar, part: &'static Part) {
        let G::Bass { lines, glide } = &part.play else {
            return;
        };
        if lines.is_empty() {
            return;
        }
        let inst = part.inst;
        let low = part.register.0;
        let i = self.intensity;
        let pick = if lines.len() > 1 && self.chance(0.35) {
            self.pick(lines.len())
        } else {
            (self.plan.bar as usize / 4) % lines.len()
        };
        let pat = lines[pick].as_bytes();
        let full = self.full as usize;
        let beat = self.beat as usize;
        let steps = self.steps.min(self.full) as usize;
        // At the end of a phrase the bass sometimes fills the last beat.
        let fill = steps == full
            && self.plan.bar % 4 == 3
            && i > 0.45
            && self.switching.is_none()
            && self.chance(0.45);
        let fill_at = if fill { full - beat } else { steps };
        let mut prev: Option<u8> = None;
        for st in 0..fill_at {
            // A line shorter than the bar rests at its end.
            let Some(&c) = pat.get(st) else {
                break;
            };
            if matches!(c, b'.' | b'-') {
                if c == b'.' {
                    prev = None;
                }
                continue;
            }
            let hold = pat
                .get(st + 1..steps.min(pat.len()).max(st + 1))
                .unwrap_or(&[])
                .iter()
                .take_while(|&&b| b == b'-')
                .count();
            let ch = self.chord(st as u32);
            let root = ch.root_from(low);
            let (note, vel, len) = match c {
                b'R' => (root, 0.8, if hold > 0 { hold as f32 + 0.9 } else { 0.7 }),
                b'P' => (root + 12, 1.0, 0.6),
                b'G' => (root, 0.25, 0.25),
                b'5' => (root + ch.q.fifth(), 0.75, hold as f32 + 0.8),
                b'3' => (root + ch.q.third(), 0.75, hold as f32 + 0.8),
                b'7' => (root + ch.q.seventh(), 0.8, 0.8),
                b'A' => {
                    let next = self.chord(st as u32 + 1).root_from(low);
                    let n = if self.chance(0.5) { next - 1 } else { next + 1 };
                    (n, 0.7, hold as f32 + 0.8)
                }
                _ => continue,
            };
            // A fretless slides into nearby notes instead of re-plucking.
            let glide = glide.is_some_and(|g| {
                prev.is_some_and(|p| (p as i32 - note as i32).abs() <= 5 && p != note)
                    && self.chance(g)
            });
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
            let from = (full - beat) as u32;
            let ch = self.chord(from);
            let root = ch.root_from(low);
            let next = self.chord(self.full).root_from(low);
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
            for (k, n) in line.into_iter().enumerate().take(beat) {
                let st = from + k as u32;
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
                        glide: glide.is_some() && k == 3,
                    },
                );
            }
        }
    }

    // --- Keys -----------------------------------------------------------
}
