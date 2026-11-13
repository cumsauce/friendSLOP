//! The kit: grooves, fills and hand percussion.

use super::*;

impl Composer {
    pub(super) fn write_drums(&mut self, bar: &mut Bar, fill: bool, crash: bool) {
        let s = self.style();
        let i = self.intensity;
        let kind = self.plan.kind;
        let odd_bar = self.plan.bar % 2 == 1;
        let (kicks, snare, hat, extra): ([&str; 2], &str, &str, &[(Inst, u8, [&str; 2])]) =
            match s.groove {
                Groove::City => (
                    ["x.....x...x.....", "x.....x.o.x..o.."],
                    "....x..g.g..x..g",
                    "xoxoxoxoxoxoxOxo",
                    &[],
                ),
                Groove::Laid => (
                    ["x.......x.x.....", "x.......x...o..."],
                    "....x.......x..g",
                    "x.o.x.o.x.o.x.oo",
                    &[],
                ),
                Groove::Brush => (
                    ["x.........x.....", "x.......o......."],
                    "....g.......g...",
                    "o.g.o.g.o.g.o.gg",
                    &[],
                ),
                Groove::Sparse => (
                    ["x.........x.....", "x..............."],
                    "................",
                    "..o...o...o...o.",
                    &[(pk().rim, 37, ["......o.......o.", "......o....o...."])],
                ),
                Groove::Dream => (
                    ["x...............", "................"],
                    "................",
                    "....g.......g...",
                    &[],
                ),
                Groove::Bolero => (
                    ["x.......x.......", "x.......x.....o."],
                    "................",
                    "ogggogggogggoggg",
                    &[
                        // 3-2 son clave across two bars, and bongos.
                        (pk().rim, 37, ["x..x..x.........", "....x...x......."]),
                        (pk().conga, 64, ["......og......og", "......og....o.og"]),
                    ],
                ),
                Groove::Tense => (
                    ["x..o............", "x..o............"],
                    "................",
                    "o.g.o.g.o.g.o.g.",
                    &[(pk().rim, 37, ["..........o.....", "..........o..o.."])],
                ),
                Groove::Drive => (
                    ["x...x...x...x.o.", "x...x...x..ox..."],
                    "....x..g....x..g",
                    "xoOoxoOoxoOoxoOo",
                    &[],
                ),
                Groove::Shred => (
                    ["x..x..x...x..x..", "x.xx..x.x.x..xx."],
                    "....x.g.g...x.gx",
                    "xoxoxoxoxoxoxoxo",
                    &[],
                ),
                Groove::March => (
                    ["x...x...x...x...", "x...x...x...x.x."],
                    "............x...",
                    "x.o.x.o.x.o.x.o.",
                    &[(pk().snare, 38, ["..g.......g.g.g.", "..g...g...g.ggg."])],
                ),
            };
        let soft = match s.groove {
            Groove::Brush | Groove::Dream => 0.6,
            Groove::Sparse | Groove::Tense => 0.75,
            _ => 1.0,
        };
        // Where the backbeat sits: laid-back grooves drag it a hair.
        let lay = match s.groove {
            Groove::Laid | Groove::Brush | Groove::Bolero => 110,
            Groove::Drive | Groove::Shred => 0,
            _ => 50,
        };
        let fill_from = if fill {
            if i > 0.6 { 8 } else { 12 }
        } else {
            99
        };
        let kick = kicks[usize::from(odd_bar && i > 0.3)];
        let steps = self.steps.min(16);
        // The ride takes over from the hats in solos and flat-out duels.
        let ride = s.kit.ride && (kind == Section::Solo || (s.groove == Groove::Shred && i > 0.85));
        let clap = s.kit.clap && (kind == Section::Hook || i > 0.75);
        let shaker = s.kit.shaker && i > 0.2 && kind != Section::Intro;
        let conga = s.kit.conga && i > 0.3 && kind != Section::Intro;
        // Tumbao: heel-toe, a slap on two, open tones on four and its "and".
        const TUMBAO: [&str; 2] = ["h.t.S.t.h.t.o.o.", "h.t.S.t.h.tSl.l."];
        for st in 0..steps {
            let k = st as usize;
            let at = self.at(st);
            if st >= fill_from {
                self.fill_step(bar, st, fill_from, steps);
                continue;
            }
            if let Some(v) = vel(kick, k) {
                Self::push(bar, hit(at, pk().kick, 36, v * soft));
            }
            match snare.as_bytes().get(k) {
                Some(b'g') if i > 0.4 => {
                    let v = 0.5 + 0.3 * self.rand();
                    Self::push(bar, hit(at + 40, pk().ghost, 38, v * soft));
                }
                Some(b'x') | Some(b'o') => {
                    let v = vel(snare, k).unwrap_or(0.8);
                    Self::push(bar, hit(at + lay, pk().snare, 38, v * soft));
                    if clap && st % 8 == 4 {
                        Self::push(bar, hit(at + lay + 25, pk().clap, 39, 0.75));
                    }
                }
                _ => {}
            }
            let jitter = (self.rand() * 80.0) as u32;
            match hat.as_bytes().get(k) {
                Some(_) if ride => {
                    // Ride: quarters and the "and"s, bell-ish accents on the
                    // beat, foot hat on two and four.
                    if st % 2 == 0 {
                        let v = if st % 4 == 0 { 0.9 } else { 0.62 } * (0.9 + 0.2 * self.rand());
                        Self::push(bar, hit(at + jitter, pk().ride, 51, v));
                    }
                    if st % 8 == 4 {
                        Self::push(bar, hit(at, pk().hat, 42, 0.35));
                    }
                }
                Some(b'O') if i > 0.5 => {
                    Self::push(bar, hit(at, pk().open_hat, 46, 0.7 * soft));
                }
                Some(c) if *c != b'.' => {
                    // Below mid intensity, 16th hats thin to 8ths.
                    let thin =
                        st % 2 == 1 && i < 0.45 && hat.len() == 16 && hat.as_bytes()[1] != b'.';
                    if !thin {
                        let v = vel(hat, k).unwrap_or(0.6) * (0.85 + 0.3 * self.rand());
                        Self::push(bar, hit(at + jitter, pk().hat, 42, v * soft));
                    }
                }
                _ => {}
            }
            for (inst, note, pats) in extra {
                if let Some(v) = vel(pats[usize::from(odd_bar)], k) {
                    let note = match *inst {
                        who if who == pk().conga && v < 0.5 => 71,
                        _ => *note,
                    };
                    Self::push(bar, hit(at, *inst, note, v * soft));
                }
            }
            if shaker && (i > 0.5 || st % 2 == 0) {
                // Accent the "and"s; the beads lean a touch ahead.
                let v = match st % 4 {
                    2 => 0.85,
                    0 => 0.55,
                    _ => 0.38,
                } * (0.85 + 0.3 * self.rand());
                Self::push(bar, hit(at.saturating_sub(60) + jitter, pk().shaker, 70, v));
            }
            if conga && let Some(c) = TUMBAO[usize::from(odd_bar)].as_bytes().get(k) {
                let (note, v) = match c {
                    b'h' => (58, 0.22),
                    b't' => (58, 0.3),
                    b'S' => (64, 0.85),
                    b'o' => (58, 0.75),
                    b'l' => (53, 0.8),
                    _ => (0, 0.0),
                };
                if note > 0 {
                    let (inst, note) = hand_note(s.hand, note);
                    Self::push(bar, hit(at + jitter / 2, inst, note, v * soft));
                }
            }
            for &inst in s.perc {
                if i < 0.3 || kind == Section::Intro {
                    continue;
                }
                if let Some(v) = perc_hit(inst, k, odd_bar) {
                    let v = v * (0.85 + 0.3 * self.rand());
                    let note = match inst {
                        // Woodblock: high on the beat, low off it.
                        who if who == pk().woodblock && !k.is_multiple_of(4) => 77,
                        who if who == pk().woodblock => 76,
                        who if who == pk().tambourine => 54,
                        who if who == pk().guiro => 73,
                        _ => 75,
                    };
                    Self::push(bar, hit(at + jitter, inst, note, v * soft));
                }
            }
        }
        if crash {
            Self::push(bar, hit(0, pk().crash, 49, 0.9));
            Self::push(bar, hit(0, pk().kick, 36, 1.0));
        }
    }
}

impl Composer {
    /// One step of a drum fill: snare 16ths swelling, or toms rolling down.
    pub(super) fn fill_step(&mut self, bar: &mut Bar, st: u32, from: u32, steps: u32) {
        let s = self.style();
        let span = (steps - from).max(1) as f32;
        let p = (st - from) as f32 / span;
        let at = self.at(st);
        match s.groove {
            Groove::Brush | Groove::Dream | Groove::Sparse | Groove::Tense => {
                if st.is_multiple_of(2) {
                    Self::push(bar, hit(at, pk().ghost, 38, 0.5 + 0.4 * p));
                }
            }
            _ => {
                let kind = (self.plan.bar / 4 + self.bars_played as u16 / 8) % 3;
                match kind {
                    0 => Self::push(bar, hit(at, pk().snare, 38, 0.45 + 0.55 * p)),
                    1 => {
                        let note = [55u8, 55, 50, 50, 45, 45, 45, 45][((p * 8.0) as usize).min(7)];
                        Self::push(bar, hit(at, pk().tom, note, 0.7 + 0.3 * p));
                    }
                    _ => {
                        if st.is_multiple_of(2) {
                            Self::push(bar, hit(at, pk().snare, 38, 0.8));
                            Self::push(bar, hit(at, pk().kick, 36, 0.8));
                        } else {
                            Self::push(bar, hit(at, pk().tom, 50, 0.6));
                        }
                    }
                }
                if st.is_multiple_of(4) {
                    Self::push(bar, hit(at, pk().kick, 36, 0.7));
                }
            }
        }
    }

    // --- Bass -----------------------------------------------------------
}

/// The conga part's (note, instrument) on another hand drum: the tumbao's
/// low "l", open "o"/slap "S" and heel-toe "h"/"t" land on that drum's
/// low and high heads.
pub(super) fn hand_note(hand: Inst, conga: u8) -> (Inst, u8) {
    let high = conga >= 58;
    let slap = conga >= 64;
    let note = match hand {
        who if who == pk().bongo => {
            if slap {
                72
            } else {
                67
            }
        }
        who if who == pk().timbales => {
            if high {
                65
            } else {
                60
            }
        }
        who if who == pk().cajon => {
            if slap {
                38
            } else {
                36
            }
        }
        who if who == pk().tabla => {
            if high {
                64
            } else {
                41
            }
        }
        _ => return (pk().conga, conga),
    };
    (hand, note)
}

/// Extra hand percussion: velocity for 16th step `st` (of a 4/4 bar), if it
/// plays there. Tambourine on the backbeat with 8th shakes, claves in a 3-2
/// son clave over two bars, guiro long-short scrapes, woodblock on the 8ths.
pub(super) fn perc_hit(inst: Inst, st: usize, odd_bar: bool) -> Option<f32> {
    let pat: &[u8] = match inst {
        who if who == pk().tambourine => b"..o.X.o...o.X.o.",
        who if who == pk().claves && odd_bar => b"....X...X.......",
        who if who == pk().claves => b"X..X...X........",
        who if who == pk().guiro => b"X.o.....X.o.....",
        who if who == pk().woodblock => b"o.o.o.o.o.o.o.oX",
        _ => return None,
    };
    match pat.get(st % 16) {
        Some(b'X') => Some(0.85),
        Some(b'o') => Some(0.45),
        _ => None,
    }
}
