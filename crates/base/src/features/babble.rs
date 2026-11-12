//! Animal Crossing style gibberish voices (UI/audio polish, asked for in
//! chat): every letter of a line is a tiny slice of a real voice, sped up
//! and pitched per speaker. The slices come from `assets/audio/voice/
//! grains.wav`, cut by `scripts/voice_grains.py` from five CC0 Wikimedia
//! Commons voice recordings (see CREDITS.md): `VOICES` voices x `GRAINS`
//! short voiced syllables each, every one `GRAIN_MS` long and sorted dark to
//! bright, so vowels pick a grain that sounds roughly like them.
//!
//! Pure functions here; a missing bank file just means callers keep their
//! old synth voice.

use bevy::audio::AudioSource;
use bevy::prelude::*;

/// Sample rate of the bank file.
pub const BANK_RATE: u32 = 22_050;
/// Voices in the bank, in file order.
pub const VOICES: usize = 5;
/// Grains per voice.
pub const GRAINS: usize = 10;
/// Length of every grain.
pub const GRAIN_MS: u32 = 90;

/// Which recorded voice a speaker borrows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Timbre {
    /// A speaking woman's voice.
    Bright = 0,
    /// A singer holding notes with vibrato.
    Singer = 1,
    /// A man speaking Cantonese (lots of pitch movement).
    Tonal = 2,
    /// A lower man's voice.
    Low = 3,
    /// A man speaking Danish.
    Soft = 4,
}

/// How someone babbles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Voice {
    pub timbre: Timbre,
    /// Playback speed of each grain: 1 is the recording, 2 an octave up.
    pub pitch: f32,
    /// Seconds per letter.
    pub letter: f32,
}

/// The bank's handle; [`samples`] reads it once loaded.
#[derive(Resource, Clone, Default)]
pub struct Bank(pub Handle<AudioSource>);

pub fn plugin(app: &mut App) {
    app.init_resource::<Bank>().add_systems(
        Startup,
        |mut commands: Commands, assets: Option<Res<AssetServer>>| {
            if let Some(assets) = assets {
                commands.insert_resource(Bank(assets.load("audio/voice/grains.wav")));
            }
        },
    );
}

/// The bank as samples, if it has loaded and looks right.
pub fn samples(bank: &Bank, audio: &Assets<AudioSource>) -> Option<Vec<f32>> {
    let src = audio.get(&bank.0)?;
    let s = parse_wav(&src.bytes)?;
    (s.len() >= bank_len()).then_some(s)
}

/// Samples a complete bank has.
pub fn bank_len() -> usize {
    grain_len() * VOICES * GRAINS
}

fn grain_len() -> usize {
    (BANK_RATE * GRAIN_MS / 1000) as usize
}

/// 16-bit mono PCM from a WAV file's `data` chunk.
pub fn parse_wav(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let len = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().ok()?) as usize;
        let body = bytes.get(at + 8..(at + 8 + len).min(bytes.len()))?;
        if id == b"data" {
            return Some(
                body.as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| i16::from_le_bytes(*c) as f32 / 32768.0)
                    .collect(),
            );
        }
        at += 8 + len + (len & 1);
    }
    None
}

/// Which grain a letter uses: vowels by how bright they sound, consonants
/// spread over the rest. `None` for spaces and punctuation.
pub fn grain_for(c: char) -> Option<usize> {
    let c = c.to_ascii_lowercase();
    Some(match c {
        'u' => 0,
        'o' => 2,
        'a' => 4,
        'e' => 7,
        'i' | 'y' => 9,
        c if c.is_ascii_alphabetic() => (c as usize * 7) % GRAINS,
        c if c.is_ascii_digit() => (c as usize * 3) % GRAINS,
        _ => return None,
    })
}

/// Babble `said` in `voice`, at `rate`. Each letter plays the start of its
/// grain, sped up to `voice.pitch` (with a little wobble from `seed`);
/// spaces are short gaps and sentence ends longer ones, and a line ending in
/// "?" goes up at the end.
pub fn speak(bank: &[f32], voice: Voice, said: &str, seed: u32, rate: u32) -> Vec<f32> {
    let glen = grain_len();
    let base = voice.timbre as usize * GRAINS * glen;
    let mut rng = seed.wrapping_mul(2_654_435_761) | 1;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng as f32 / u32::MAX as f32
    };
    let letter = (voice.letter * rate as f32) as usize;
    let fade = (0.006 * rate as f32) as usize;
    let question = said.trim_end().ends_with('?');
    let chars: Vec<char> = said.chars().collect();
    let mut out: Vec<f32> = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        let Some(g) = grain_for(c) else {
            let gap = match c {
                '.' | '!' | '?' => 0.16,
                ',' | ';' | ':' => 0.09,
                _ => 0.035,
            };
            out.extend(std::iter::repeat_n(0.0, (gap * rate as f32) as usize));
            continue;
        };
        let near_end = i + 6 >= chars.len();
        let lift = if question && near_end { 1.25 } else { 1.0 };
        let pitch = voice.pitch * (0.94 + next() * 0.12) * lift;
        // Read the grain at `pitch` x speed, resampled from the bank rate.
        let step = pitch * BANK_RATE as f32 / rate as f32;
        let start = base + g * glen;
        let at = out.len().saturating_sub(fade);
        out.resize(at + letter, 0.0);
        for k in 0..letter {
            let pos = k as f32 * step;
            let j = pos as usize;
            if j + 1 >= glen {
                break;
            }
            let frac = pos - j as f32;
            let s = bank[start + j] * (1.0 - frac) + bank[start + j + 1] * frac;
            let env = (k as f32 / fade as f32)
                .min((letter - k) as f32 / fade as f32)
                .min(1.0);
            out[at + k] += s * env * 0.8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `BABBLE_DEMO=out.wav cargo test -p friendslop babble_demo -- --ignored`
    /// renders every speaker from the real bank, to listen to.
    #[test]
    #[ignore]
    fn babble_demo() {
        let (Ok(out), Ok(bytes)) = (
            std::env::var("BABBLE_DEMO"),
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../game/assets/audio/voice/grains.wav"
            )),
        ) else {
            return;
        };
        use crate::features::voice_lines::{LINES, Speaker, voice};
        let mut all = Vec::new();
        for (i, who) in [Speaker::Bear, Speaker::Crow, Speaker::MusicBox]
            .into_iter()
            .enumerate()
        {
            all.extend(voice(who, LINES[i], i as u32, Some(&bytes)));
            all.extend(std::iter::repeat_n(0.0, 20_000));
        }
        let rate = crate::features::synth_audio::RATE;
        let _ = std::fs::write(out, crate::features::bear::wav_bytes(&all, rate));
    }

    fn fake_bank() -> Vec<f32> {
        (0..grain_len() * VOICES * GRAINS)
            .map(|i| (i as f32 * 0.07).sin() * 0.9)
            .collect()
    }

    #[test]
    fn letters_pick_grains_and_gaps() {
        assert_eq!(grain_for('U'), Some(0));
        assert_eq!(grain_for('i'), Some(9));
        assert!(grain_for('k').is_some_and(|g| g < GRAINS));
        assert_eq!(grain_for(' '), None);
        assert_eq!(grain_for('!'), None);
    }

    #[test]
    fn speaking_is_audible_bounded_and_scales_with_length() {
        let bank = fake_bank();
        let v = Voice {
            timbre: Timbre::Low,
            pitch: 1.6,
            letter: 0.055,
        };
        let short = speak(&bank, v, "hi", 1, 44_100);
        let long = speak(&bank, v, "hello there, friend!", 1, 44_100);
        assert!(long.len() > short.len() * 4);
        let peak = long.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.1 && peak < 1.5, "{peak}");
    }

    #[test]
    fn reads_a_wav() {
        let wav = crate::features::bear::wav_bytes(&[0.0, 0.5, -0.5], 22_050);
        let s = parse_wav(&wav).unwrap_or_default();
        assert_eq!(s.len(), 3);
        assert!((s[1] - 0.5).abs() < 0.01);
        assert_eq!(parse_wav(b"nope"), None);
    }
}
