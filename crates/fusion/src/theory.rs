//! Jazz harmony for the fusion score: chord qualities, their scales and
//! colour tones, and voice-led keyboard voicings.
//!
//! Notes are MIDI numbers (60 = middle C); pitch classes are `0..12`.

/// A chord quality, each with the extensions fusion actually uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Quality {
    /// Major 9 (ionian).
    Maj9,
    /// Major 7 #11 (lydian), the dreamy one.
    Maj7s11,
    /// Major 6/9.
    Maj69,
    /// Minor 9 (dorian).
    Min9,
    /// Minor 11 (dorian), the open fourths sound.
    Min11,
    /// Minor 6/9 (melodic minor): the "minor iv" tear-jerker.
    Min6,
    /// Minor-major 9: tense, film-noir.
    MinMaj9,
    /// Dominant 13 (mixolydian).
    Dom13,
    /// Dominant 9.
    Dom9,
    /// Dominant 7#9 (the funky one).
    Dom7s9,
    /// Altered dominant (b9 #9 #11 b13).
    Dom7alt,
    /// Dominant 7b9 (half-whole diminished), minor-key V.
    Dom7b9,
    /// 9sus4 (Herbie's favourite).
    Sus9,
    /// Lydian dominant 13#11: tritone substitutions.
    Dom13s11,
    /// Half-diminished (locrian #2).
    HalfDim,
    /// Diminished 7th (whole-half).
    Dim7,
}

use Quality::*;

impl Quality {
    /// The four tones of a rootless left-hand voicing (intervals above the
    /// root; the bass plays the root). Classic Bill Evans A/B shapes: 3-5-7-9
    /// or 7-9-3-5, with the 13 or #11 swapped in where it colours the chord.
    pub fn voicing(self) -> [u8; 4] {
        match self {
            Maj9 => [4, 7, 11, 14],
            Maj7s11 => [4, 11, 14, 18],
            Maj69 => [4, 9, 14, 19],
            Min9 => [3, 7, 10, 14],
            Min11 => [3, 10, 14, 17],
            Min6 => [3, 7, 9, 14],
            MinMaj9 => [3, 7, 11, 14],
            Dom13 => [4, 10, 14, 21],
            Dom9 => [4, 7, 10, 14],
            Dom7s9 => [4, 10, 15, 19],
            Dom7alt => [4, 10, 15, 20],
            Dom7b9 => [4, 7, 10, 13],
            Sus9 => [5, 10, 14, 19],
            Dom13s11 => [4, 10, 18, 21],
            HalfDim => [3, 6, 10, 14],
            Dim7 => [3, 6, 9, 12],
        }
    }

    /// The chord-scale a soloist plays over this chord (intervals in one
    /// octave, ascending, starting at 0).
    pub fn scale(self) -> &'static [u8] {
        const IONIAN: &[u8] = &[0, 2, 4, 5, 7, 9, 11];
        const LYDIAN: &[u8] = &[0, 2, 4, 6, 7, 9, 11];
        const DORIAN: &[u8] = &[0, 2, 3, 5, 7, 9, 10];
        const MEL_MINOR: &[u8] = &[0, 2, 3, 5, 7, 9, 11];
        const MIXO: &[u8] = &[0, 2, 4, 5, 7, 9, 10];
        const ALTERED: &[u8] = &[0, 1, 3, 4, 6, 8, 10];
        const HW_DIM: &[u8] = &[0, 1, 3, 4, 6, 7, 9, 10];
        const LYD_DOM: &[u8] = &[0, 2, 4, 6, 7, 9, 10];
        const LOCRIAN2: &[u8] = &[0, 2, 3, 5, 6, 8, 10];
        const WH_DIM: &[u8] = &[0, 2, 3, 5, 6, 8, 9, 11];
        match self {
            Maj9 | Maj69 => IONIAN,
            Maj7s11 => LYDIAN,
            Min9 | Min11 => DORIAN,
            Min6 | MinMaj9 => MEL_MINOR,
            Dom13 | Dom9 | Sus9 => MIXO,
            Dom7s9 | Dom7alt => ALTERED,
            Dom7b9 => HW_DIM,
            Dom13s11 => LYD_DOM,
            HalfDim => LOCRIAN2,
            Dim7 => WH_DIM,
        }
    }

    /// Tones a melody may land on for a strong beat: the root plus every
    /// voicing tone (so 9ths, 11ths and 13ths count, as they do in fusion).
    pub fn strong(self) -> [u8; 5] {
        let v = self.voicing();
        [0, v[0] % 12, v[1] % 12, v[2] % 12, v[3] % 12]
    }

    /// The "guide" tones that define the chord: third and seventh.
    pub fn guide(self) -> [u8; 2] {
        let v = self.voicing();
        match self {
            Sus9 => [5, 10],
            Maj69 => [4, 9],
            Min6 => [3, 9],
            Dim7 => [3, 9],
            _ => [
                v[0] % 12,
                if v[1] % 12 >= 9 { v[1] % 12 } else { v[2] % 12 },
            ],
        }
    }

    pub fn is_dominant(self) -> bool {
        matches!(
            self,
            Dom13 | Dom9 | Dom7s9 | Dom7alt | Dom7b9 | Sus9 | Dom13s11
        )
    }

    pub fn is_minor(self) -> bool {
        matches!(self, Min9 | Min11 | Min6 | MinMaj9 | HalfDim | Dim7)
    }

    /// The fifth above the root (diminished on half-dim/dim).
    pub fn fifth(self) -> u8 {
        match self {
            HalfDim | Dim7 => 6,
            _ => 7,
        }
    }

    /// The seventh above the root (6th on the sixth chords).
    pub fn seventh(self) -> u8 {
        match self {
            Maj9 | Maj7s11 | MinMaj9 => 11,
            Maj69 | Min6 | Dim7 => 9,
            _ => 10,
        }
    }

    pub fn third(self) -> u8 {
        match self {
            Sus9 => 5,
            q if q.is_minor() => 3,
            _ => 4,
        }
    }
}

/// A chord: root pitch class and quality.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Chord {
    pub root: u8,
    pub q: Quality,
}

impl Chord {
    pub const fn new(root: u8, q: Quality) -> Self {
        Self { root: root % 12, q }
    }

    /// Is `note` a tone of this chord's scale?
    pub fn in_scale(self, note: u8) -> bool {
        let pc = (note + 12 - self.root) % 12;
        self.q.scale().contains(&pc)
    }

    /// Is `note` a tone a strong beat may land on?
    pub fn is_strong(self, note: u8) -> bool {
        let pc = (note + 12 - self.root) % 12;
        self.q.strong().contains(&pc)
    }

    /// The nearest strong (chord/colour) tone to `note`; ties go down.
    pub fn nearest_strong(self, note: u8) -> u8 {
        nearest(note, |n| self.is_strong(n))
    }

    /// The nearest scale tone to `note`.
    pub fn nearest_scale(self, note: u8) -> u8 {
        nearest(note, |n| self.in_scale(n))
    }

    /// Move `steps` scale steps from `note` (snapped into the scale first).
    pub fn scale_step(self, note: u8, steps: i32) -> u8 {
        let mut n = self.nearest_scale(note) as i32;
        let dir = steps.signum();
        for _ in 0..steps.abs() {
            n += dir;
            while !(0..128).contains(&n) || !self.in_scale(n as u8) {
                n += dir;
                if !(0..128).contains(&n) {
                    return note;
                }
            }
        }
        n.clamp(0, 127) as u8
    }

    /// The root as a note in the octave starting at `lo`.
    pub fn root_from(self, lo: u8) -> u8 {
        lo + (self.root + 12 - lo % 12) % 12
    }
}

fn nearest(note: u8, ok: impl Fn(u8) -> bool) -> u8 {
    for d in 0..12u8 {
        if note >= d && ok(note - d) {
            return note - d;
        }
        if note + d < 128 && ok(note + d) {
            return note + d;
        }
    }
    note
}

/// A four-note keyboard voicing, sorted low to high.
pub type Voicing = [u8; 4];

/// Total semitones every voice moves between two voicings.
pub fn movement(a: &Voicing, b: &Voicing) -> u32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as i32 - *y as i32).unsigned_abs())
        .sum()
}

/// Pick the voicing of `chord` (its rootless tones, in any inversion, close
/// or drop-2, with its lowest note in `lo..=hi`) that moves least from
/// `prev`, gently pulled towards the middle of the range so a long
/// progression doesn't drift off the keyboard.
pub fn voice_lead(chord: Chord, prev: &Voicing, lo: u8, hi: u8) -> Voicing {
    let tones = chord.q.voicing();
    // Sorted, so each rotation stacks into a close (within-octave) voicing.
    let mut pcs: [u8; 4] = tones.map(|t| (chord.root + t) % 12);
    pcs.sort_unstable();
    let centre = (lo as i32 + hi as i32) / 2 + 6;
    let mut best = *prev;
    let mut best_cost = i32::MAX;
    for rot in 0..4 {
        for drop2 in [false, true] {
            // Stack the pitch classes upward from the rotation's bottom note.
            let mut v = [0u8; 4];
            let mut n = pcs[rot] as i32;
            v[0] = n as u8;
            for (k, slot) in v.iter_mut().enumerate().skip(1) {
                let pc = pcs[(rot + k) % 4] as i32;
                let mut m = n + 1;
                while m.rem_euclid(12) != pc {
                    m += 1;
                }
                n = m;
                *slot = m as u8;
            }
            let mut v: [i32; 4] = v.map(i32::from);
            if drop2 {
                v[2] -= 12;
                v.sort_unstable();
            }
            // Try every octave placing the bottom note in range.
            for oct in -1..10 {
                let shift = oct * 12;
                let low = v[0] + shift;
                if low < lo as i32 || low > hi as i32 {
                    continue;
                }
                let cand = v.map(|x| (x + shift).clamp(0, 127) as u8);
                let mean = cand.iter().map(|&x| x as i32).sum::<i32>() / 4;
                let cost = movement(&cand, prev) as i32 * 4 + (mean - centre).abs();
                if cost < best_cost {
                    best_cost = cost;
                    best = cand;
                }
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voicings_are_chord_tones_in_range() {
        let qs = [
            Maj9, Maj7s11, Maj69, Min9, Min11, Min6, MinMaj9, Dom13, Dom9, Dom7s9, Dom7alt, Dom7b9,
            Sus9, Dom13s11, HalfDim, Dim7,
        ];
        let mut prev = [55, 59, 62, 65];
        for (i, q) in qs.iter().enumerate() {
            let c = Chord::new(i as u8 * 5, *q);
            let v = voice_lead(c, &prev, 50, 64);
            assert!(v.windows(2).all(|w| w[0] <= w[1]));
            assert!((50..=64).contains(&v[0]), "{v:?}");
            for n in v {
                assert!(c.is_strong(n), "{q:?} {n}");
            }
            prev = v;
        }
    }

    /// Smooth voice leading: through ii-V-I chains, tritone subs and
    /// modal-interchange moves, the four voices move little in total.
    #[test]
    fn voice_leading_cost_is_bounded() {
        let prog = [
            Chord::new(2, Min9),
            Chord::new(7, Dom13),
            Chord::new(0, Maj9),
            Chord::new(9, Dom7alt),
            Chord::new(2, Min11),
            Chord::new(1, Dom13s11),
            Chord::new(0, Maj7s11),
            Chord::new(5, Min6),
            Chord::new(4, Min9),
            Chord::new(3, Dom7s9),
            Chord::new(8, Maj7s11),
            Chord::new(10, Sus9),
        ];
        let mut prev = voice_lead(prog[0], &[52, 57, 60, 64], 50, 64);
        let mut total = 0;
        for round in 0..4 {
            for (i, c) in prog.iter().enumerate() {
                let c = Chord::new(c.root + round * 2, c.q);
                let v = voice_lead(c, &prev, 50, 64);
                let m = movement(&prev, &v);
                assert!(m <= 12, "step {i}: {prev:?} -> {v:?} moved {m}");
                total += m;
                prev = v;
            }
        }
        let mean = total as f32 / (4 * prog.len()) as f32;
        assert!(mean <= 6.5, "mean movement {mean}");
    }

    #[test]
    fn scale_steps_stay_in_scale() {
        let c = Chord::new(7, Dom7alt);
        let mut n = 67;
        for s in [1, 2, -3, 4, -1] {
            n = c.scale_step(n, s);
            assert!(c.in_scale(n));
        }
    }
}
