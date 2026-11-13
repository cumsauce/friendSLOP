//! friendSLOP's generated, adaptive 90s jazz-fusion soundtrack.
//!
//! - [`theory`]: chords, chord-scales, voice-led rootless voicings.
//! - [`compose`]: the music director and composer, one bar at a time.
//! - [`inst`]: the instruments, by name, from `instruments.ron`.
//! - [`synth`]: FM voices (DX7-style EP, slap and fretless bass, leads,
//!   bell pad, kalimba), subtractive voices (CS-80 style horn section,
//!   string machine, Minimoog lead), a sample bank (the synthesized kit
//!   from [`drums`] plus `foley-synth` note-block instruments) and the mixer.
//! - [`dsp`]: EQ, filters, oscillators, chorus, echo and reverb.
//! - [`direct`]: the score's rules (which mood plays when), over signals.
//! - [`engine`]: runs the composer at bar lines and mixes the band.
//! - [`render`]: offline rendering and analysis, used by the
//!   `fusion-render` tool to audition instruments without the game.
//!
//! The game (`friendslop::features::fusion`) only wraps [`engine::Engine`]
//! in a Bevy audio source and feeds it a mood.

pub mod compose;
pub mod direct;
pub mod drums;
pub mod dsp;
pub mod engine;
pub mod inst;
pub mod render;
pub mod synth;
pub mod theory;
