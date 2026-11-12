//! Random dialogue. From issue #10.
//!
//! All the game's written dialogue (the goat's remarks and the NPC voice
//! lines) is treated as one long script. Pick a random number between 1 and
//! 1,000,000; after that many words of dialogue, the next word is replaced with
//! a completely random word, then a new number is picked, and so on. If fewer
//! than 50 words got replaced, start over with half the upper bound, halving
//! until at least 50 are (or the bound reaches 1).
//!
//! The rewrite uses a fixed seed, so every peer sees the same dialogue.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::features::voice_lines;

pub const START_BOUND: u64 = 1_000_000;
pub const MIN_REPLACED: usize = 50;
const SEED: u64 = 0x5EED_F00D_D1A1_0600;

/// Completely random words.
pub static RANDOM_WORDS: friendslop_data::Data<Vec<String>> =
    friendslop_data::Data::ron("data/text/random_words.ron");

/// Small deterministic xorshift so peers agree.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number in 1..=bound.
    fn upto(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1) + 1
    }
}

/// Run the replacement over `lines` (one script, words counted across lines)
/// with the given upper bound. Returns the rewritten lines and how many words
/// were replaced.
pub fn rewrite_once(lines: &[&str], bound: u64, seed: u64) -> (Vec<String>, usize) {
    let words = RANDOM_WORDS.get();
    let mut rng = Rng(seed | 1);
    let mut until = rng.upto(bound);
    let mut replaced = 0;
    let out = lines
        .iter()
        .map(|line| {
            line.split_whitespace()
                .map(|word| {
                    if until == 0 {
                        replaced += 1;
                        until = rng.upto(bound);
                        words
                            .get(rng.next() as usize % words.len().max(1))
                            .cloned()
                            .unwrap_or_else(|| word.to_string())
                    } else {
                        until -= 1;
                        word.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    (out, replaced)
}

/// The full rule: start at 1,000,000 and halve until at least 50 words are
/// replaced. Returns the lines, how many were replaced and the bound used.
pub fn rewrite(lines: &[&str], seed: u64) -> (Vec<String>, usize, u64) {
    let mut bound = START_BOUND;
    loop {
        let (out, n) = rewrite_once(lines, bound, seed);
        if n >= MIN_REPLACED || bound <= 1 {
            return (out, n, bound);
        }
        bound /= 2;
    }
}

// The goat's remarks (goat.rs re-exports these).

/// A choice the goat has noticed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Joined,
    Left,
    AteMushroom,
    Muted,
    Unmuted,
    DidNothing,
}

impl Choice {
    pub const ALL: [Choice; 6] = [
        Choice::Joined,
        Choice::Left,
        Choice::AteMushroom,
        Choice::Muted,
        Choice::Unmuted,
        Choice::DidNothing,
    ];
}

/// The goat's opinion of `choice`. `n` picks among its remarks so it doesn't
/// repeat itself every time.
pub fn remark(choice: Choice, n: usize) -> &'static str {
    let lines: &[&str] = match choice {
        Choice::Joined => &[
            "Oh, you came. Interesting choice.",
            "Another one. Wonderful. Truly.",
            "Nice of you to finally show up.",
        ],
        Choice::Left => &[
            "They left. No, it's fine. It's fine.",
            "Well. Someone had somewhere better to be.",
        ],
        Choice::AteMushroom => &[
            "You ate a floor mushroom. Bold. I'd never.",
            "Sure, eat the random mushroom. What could go wrong.",
            "I'm not saying it was a mistake. I'm just thinking it.",
        ],
        Choice::Muted => &[
            "Muting yourself. Probably for the best.",
            "Ah, the silent treatment. Very mature.",
        ],
        Choice::Unmuted => &[
            "Oh good, you're talking again.",
            "Unmuted. Brave, given last time.",
        ],
        Choice::DidNothing => &[
            "Standing around. Love that for you.",
            "No, take your time. I have all day. I'm a goat.",
            "Doing nothing is also a choice. A choice you made.",
        ],
    };
    lines[n % lines.len()]
}

/// Every line of dialogue in the game, in a fixed order.
pub fn script() -> Vec<&'static str> {
    let mut lines: Vec<&'static str> = Vec::new();
    for choice in Choice::ALL {
        for n in 0..8 {
            let l = remark(choice, n);
            if !lines.contains(&l) {
                lines.push(l);
            }
        }
    }
    lines.extend(voice_lines::LINES);
    lines
}

fn table() -> &'static HashMap<&'static str, String> {
    static TABLE: OnceLock<HashMap<&'static str, String>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let lines = script();
        let (out, _, _) = rewrite(&lines, SEED);
        lines.into_iter().zip(out).collect()
    })
}

/// The randomised version of a line of dialogue (unchanged if unknown).
pub fn say(line: &str) -> String {
    table()
        .get(line)
        .cloned()
        .unwrap_or_else(|| line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn at_least_fifty_words_replaced() {
        let lines = script();
        let (out, n, bound) = rewrite(&lines, SEED);
        assert!(n >= MIN_REPLACED, "only {n} replaced at bound {bound}");
        assert!(bound < START_BOUND);
        assert_eq!(out.len(), lines.len());
        assert!(lines.iter().any(|l| say(l) != *l));
    }

    #[test]
    fn small_bound_replaces_often() {
        let (_, n) = rewrite_once(&["a b c d e f g h i j"], 1, 7);
        assert!(n >= 3);
    }

    #[test]
    fn same_on_every_peer() {
        let lines = script();
        assert_eq!(rewrite(&lines, SEED).0, rewrite(&lines, SEED).0);
    }
}
