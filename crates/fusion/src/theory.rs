//! Jazz harmony maths for the fusion score: chords, scale steps, nearest
//! chord tones and voice-led keyboard voicings. What each chord quality is
//! (its voicing, scales and colour tones) is data (`library.ron`).
//!
//! Notes are MIDI numbers (60 = middle C); pitch classes are `0..12`.

use std::sync::RwLock;

use crate::compose::library::{QualityDef, lib};

/// A chord quality, by name (`Maj9`, `Dom7alt`...): what each one is (its
/// voicing, scales and colour tones) is data, in `library.ron`'s
/// `qualities`. Interned like [`crate::inst::Inst`]: cheap to copy and
/// compare, and `{:?}` prints its name. A name the library doesn't list
/// plays as a plain major chord.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Quality(u16);

/// Every quality name ever asked for, in order: a [`Quality`] is an index.
static NAMES: RwLock<Vec<&'static str>> = RwLock::new(Vec::new());

impl Quality {
    /// No quality (plays as the fallback).
    pub const NONE: Quality = Quality(u16::MAX);

    /// The quality called `name` (the same name is always the same one).
    pub fn named(name: &str) -> Quality {
        if let Ok(names) = NAMES.read()
            && let Some(i) = names.iter().position(|n| *n == name)
        {
            return Quality(i as u16);
        }
        let Ok(mut names) = NAMES.write() else {
            return Quality::NONE;
        };
        if let Some(i) = names.iter().position(|n| *n == name) {
            return Quality(i as u16);
        }
        if names.len() >= u16::MAX as usize {
            return Quality::NONE;
        }
        names.push(Box::leak(name.to_owned().into_boxed_str()));
        Quality(names.len() as u16 - 1)
    }

    /// Its name ("?" for none).
    pub fn name(self) -> &'static str {
        NAMES
            .read()
            .ok()
            .and_then(|n| n.get(self.0 as usize).copied())
            .unwrap_or("?")
    }

    pub(crate) fn index(self) -> usize {
        self.0 as usize
    }

    fn def(self) -> &'static QualityDef {
        lib().qualities.get(self)
    }

    /// The four tones of a rootless left-hand voicing (intervals above the
    /// root; the bass plays the root).
    pub fn voicing(self) -> [u8; 4] {
        self.def().voicing
    }

    /// The chord-scale a soloist plays over this chord (intervals in one
    /// octave, ascending, starting at 0).
    pub fn scale(self) -> &'static [u8] {
        &self.def().scale
    }

    /// The five-note scale fast runs use over it.
    pub fn pent(self) -> &'static [u8] {
        &self.def().pent
    }

    /// Tones a melody may land on for a strong beat (pitch classes above the
    /// root: 9ths, 11ths and 13ths count, as they do in fusion).
    pub fn strong(self) -> &'static [u8] {
        &self.def().strong
    }

    /// The "guide" tones that define the chord: third and seventh.
    pub fn guide(self) -> [u8; 2] {
        self.def().guide
    }

    pub fn is_dominant(self) -> bool {
        self.def().dominant
    }

    pub fn is_minor(self) -> bool {
        self.def().minor
    }

    /// Its fifth is bent (an altered dominant).
    pub fn is_altered(self) -> bool {
        self.def().altered
    }

    /// The fifth above the root.
    pub fn fifth(self) -> u8 {
        self.def().fifth
    }

    /// The seventh above the root (6th on the sixth chords).
    pub fn seventh(self) -> u8 {
        self.def().seventh
    }

    pub fn third(self) -> u8 {
        self.def().third
    }
}

impl std::fmt::Debug for Quality {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Reads a bare name (`Min6`, as progressions write them).
impl<'de> serde::Deserialize<'de> for Quality {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Name;
        impl serde::de::Visitor<'_> for Name {
            type Value = Quality;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a chord quality name")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Quality, E> {
                Ok(Quality::named(v))
            }
        }
        d.deserialize_identifier(Name)
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

    fn q(name: &str) -> Quality {
        Quality::named(name)
    }

    #[test]
    fn voicings_are_chord_tones_in_range() {
        let qs = [
            "Maj9", "Maj7s11", "Maj69", "Min9", "Min11", "Min6", "MinMaj9", "Dom13", "Dom9",
            "Dom7s9", "Dom7alt", "Dom7b9", "Sus9", "Dom13s11", "HalfDim", "Dim7",
        ]
        .map(Quality::named);
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
            Chord::new(2, q("Min9")),
            Chord::new(7, q("Dom13")),
            Chord::new(0, q("Maj9")),
            Chord::new(9, q("Dom7alt")),
            Chord::new(2, q("Min11")),
            Chord::new(1, q("Dom13s11")),
            Chord::new(0, q("Maj7s11")),
            Chord::new(5, q("Min6")),
            Chord::new(4, q("Min9")),
            Chord::new(3, q("Dom7s9")),
            Chord::new(8, q("Maj7s11")),
            Chord::new(10, q("Sus9")),
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
        let c = Chord::new(7, q("Dom7alt"));
        let mut n = 67;
        for s in [1, 2, -3, 4, -1] {
            n = c.scale_step(n, s);
            assert!(c.in_scale(n));
        }
    }
}
