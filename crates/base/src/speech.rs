//! How the game's robot voices speak: a speaker's [`Preset`] (pitch, speed, mood, what the place
//! does to the sound) and the moods in `data/base/speech.ron`, on top of `friendslop-speech`.
//!
//! A line of text may end in `[mood]` to say it differently from its speaker's usual mood
//! (`"Pick up the phone. [angry]"`); use [`split_mood`] before showing a line. The markup `*stress*`,
//! `...` and `--` is the synth's own (see the `friendslop-speech` docs).

use std::collections::HashMap;

use friendslop_data::Data;
use serde::Deserialize;

pub use friendslop_speech::{Delivery, Voice};

pub static MOODS: Data<Moods> = Data::ron("data/base/speech.ron");

/// Every named way of saying a line.
#[derive(Deserialize, Default)]
pub struct Moods {
    pub moods: HashMap<String, Delivery>,
}

/// What a place does to a voice after it's made.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
pub enum Effect {
    #[default]
    None,
    /// Heard down a telephone line.
    Phone,
    /// Heard over a loudspeaker.
    Pa,
}

/// A speaker: Andrew on the phone, the platform announcer, a preacher.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default)]
pub struct Preset {
    /// Hz at the start of a sentence.
    pub pitch: f32,
    /// 2.0 talks twice as fast.
    pub speed: f32,
    /// Name of a mood in `data/base/speech.ron`.
    pub mood: String,
    pub effect: Effect,
    /// Another seed is another throat.
    pub seed: u32,
}

impl Default for Preset {
    fn default() -> Self {
        Self {
            pitch: 95.0,
            speed: 1.0,
            mood: "neutral".into(),
            effect: Effect::None,
            seed: 1,
        }
    }
}

/// Splits a trailing `[mood]` off a line: the words to say or show, and the mood if it has one.
pub fn split_mood(line: &str) -> (&str, Option<&str>) {
    let line = line.trim_end();
    if let Some(body) = line.strip_suffix(']')
        && let Some(open) = body.rfind('[')
    {
        let mood = body[open + 1..].trim();
        if !mood.is_empty() && mood.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
            return (body[..open].trim_end(), Some(mood));
        }
    }
    (line, None)
}

/// The delivery for a mood by name; the neutral voice if the file has no such mood.
pub fn delivery(mood: &str) -> Delivery {
    MOODS.get().moods.get(mood).cloned().unwrap_or_default()
}

fn voice(preset: &Preset, mood: Option<&str>) -> Voice {
    Voice {
        pitch: preset.pitch,
        speed: preset.speed,
        seed: preset.seed,
        delivery: delivery(mood.unwrap_or(&preset.mood)),
        ..Voice::new()
    }
}

/// `line` said by `preset`: mono samples and their sample rate.
pub fn say(line: &str, preset: &Preset) -> (Vec<f32>, u32) {
    let (text, mood) = split_mood(line);
    let voice = voice(preset, mood);
    let rate = voice.sample_rate;
    let mut samples = voice.speak(text);
    match preset.effect {
        Effect::None => {}
        Effect::Phone => friendslop_speech::post::phone(&mut samples, rate as f32),
        Effect::Pa => friendslop_speech::post::pa(&mut samples, rate as f32),
    }
    (samples, rate)
}

/// How many seconds `say` will take, without making the sound.
pub fn duration(line: &str, preset: &Preset) -> f32 {
    let (text, mood) = split_mood(line);
    voice(preset, mood).duration(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped moods, loaded the way the game loads them.
    fn load() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/base/speech.ron"
        );
        let bytes = std::fs::read(path).expect("speech.ron");
        assert!(MOODS.set_bytes(&bytes), "speech.ron doesn't parse");
    }

    #[test]
    fn splits_a_trailing_mood() {
        assert_eq!(split_mood("Pick up. [angry]"), ("Pick up.", Some("angry")));
        assert_eq!(split_mood("Hello there."), ("Hello there.", None));
        assert_eq!(split_mood("a [b c]"), ("a [b c]", None));
        assert_eq!(split_mood("see [1]"), ("see [1]", None));
    }

    #[test]
    fn the_mood_file_parses_and_has_the_moods_the_game_asks_for() {
        load();
        let moods = MOODS.get();
        for name in [
            "neutral", "calm", "annoyed", "angry", "furious", "sad", "excited", "afraid", "bored",
            "preacher", "frantic",
        ] {
            assert!(moods.moods.contains_key(name), "{name}");
        }
        assert_eq!(moods.moods["neutral"], Delivery::default());
    }

    #[test]
    fn moods_change_the_sound() {
        load();
        let line = "I told you not to call me again.";
        let said = |mood: &str| {
            let preset = Preset {
                mood: mood.into(),
                ..Preset::default()
            };
            (say(line, &preset).0.len(), duration(line, &preset))
        };
        assert!(said("angry").0 < said("neutral").0);
        assert!(said("sad").0 > said("neutral").0);
        assert!((said("sad").1 * 22_050.0 - said("sad").0 as f32).abs() < 22_050.0 * 0.01);
    }

    #[test]
    fn a_line_mood_overrides_the_speakers() {
        load();
        let preset = Preset::default();
        let plain = say("Come here.", &preset).0.len();
        let sad = say("Come here. [sad]", &preset).0.len();
        assert!(sad > plain);
    }

    #[test]
    fn the_phone_and_the_loudspeaker_bound_the_sound() {
        load();
        for effect in [Effect::Phone, Effect::Pa] {
            let preset = Preset {
                effect,
                ..Preset::default()
            };
            let (samples, _) = say("Stand clear of the closing doors.", &preset);
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
    }
}
