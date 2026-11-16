//! Live controls: numbers the game sets every frame (is the player in the
//! air, how fast are they going) that a song part can follow, its level
//! fading with them in real time rather than bar by bar.
//!
//! A part writes `follow: (signal: "airborne", fade: 0.4)`: its instrument
//! plays at the signal's level (0..1), eased over `fade` seconds. The game
//! publishes the signals [`names`] lists into the engine's slots, in order.

use crate::compose::score::SONGS;

/// How many signals the songs may follow at once.
pub const LIVE: usize = 8;

fn fade() -> f32 {
    0.4
}

/// A part's `follow:` (see the module docs).
#[derive(serde::Deserialize, Clone, Debug, PartialEq)]
pub struct Follow {
    /// The signal's name (what the game's features publish).
    pub signal: String,
    /// Seconds to ease towards a new level.
    #[serde(default = "fade")]
    pub fade: f32,
    /// Play when the signal is 0, fade out as it rises.
    #[serde(default)]
    pub invert: bool,
}

/// Every signal any song follows, in slot order (sorted, at most [`LIVE`]).
pub fn names() -> Vec<String> {
    let songs = SONGS.get();
    let mut all: Vec<String> = songs
        .iter()
        .flat_map(|s| s.parts.iter().flatten())
        .filter_map(|p| p.follow.as_ref().map(|f| f.signal.clone()))
        .collect();
    all.sort();
    all.dedup();
    all.truncate(LIVE);
    all
}

/// The slot `name` is published in.
pub fn slot(name: &str) -> Option<usize> {
    names().iter().position(|n| n == name)
}

/// One part following a slot this bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Followed {
    pub inst: crate::inst::Inst,
    pub slot: usize,
    pub fade: f32,
    pub invert: bool,
}

/// Ease `level` towards `target` over `n` samples with time constant `fade` s.
pub fn ease(level: f32, target: f32, fade: f32, n: usize) -> f32 {
    let k = 1.0 - (-(n as f32) * crate::synth::INV_RATE / fade.max(0.005)).exp();
    level + (target.clamp(0.0, 1.0) - level) * k
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_follow_parses_and_eases() {
        let f: Follow = ron::from_str("(signal: \"airborne\")").unwrap_or_else(|e| panic!("{e}"));
        assert_eq!((f.fade, f.invert), (0.4, false));
        let mut l = 0.0;
        for _ in 0..(crate::synth::RATE as usize / 256) {
            l = ease(l, 1.0, 0.4, 256);
        }
        assert!(l > 0.9 && l <= 1.0);
    }
}
