//! A formant speech synthesizer with moods. No game code.
//!
//! Text goes through samiam's letter-to-sound rules (`text`), is cut into chunks by the writer's
//! markup (`markup`), laid out as sounds with durations and a hat-pattern pitch contour, and
//! rendered by a Klatt-style cascade fed by a glottal pulse with breath, jitter, shimmer and
//! drift (`synth`). A [`Delivery`] is one way of saying it (angry, bored, preaching), and
//! [`post`] gives the result a telephone or a loudspeaker.
//!
//! Markup in the text: `*word*` stresses a word, `...` is a hesitation, `--` a short break, and a
//! long ALL-CAPS word in a sentence with lower case is shouted.

mod markup;
pub mod post;
mod synth;
mod text;

pub use synth::{Delivery, Voice};
pub use text::phonemes;
