//! Voice lines for NPCs and musical objects. From issue #19 by Kori.exe.
//!
//! The issue asks for audio from a specific 1993 game. We can't ship that
//! (it's someone else's copyrighted audio), so this is the closest original
//! take: every so often an NPC (the bear or a crow) or one of two little
//! "musical objects" (glowing music boxes) delivers a dramatic, slightly
//! janky, over-acted voice line. The line shows as a subtitle and is "spoken"
//! by a procedurally generated babble voice; music boxes play a little
//! wobbly jingle first. Timing comes from the clock, so every peer agrees
//! without network traffic.

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::random_dialogue;
use crate::features::synth_audio::play_synth;

/// A musical object on the floor at (x, z). `tune` picks its jingle.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct MusicBox {
    pub at: Vec2,
    pub tune: u8,
}

/// Tuning numbers: `assets/data/base/voice_lines.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    pub music_box_count: u8,
    /// Seconds between voice lines.
    pub line_period: f32,
    /// How long a line stays on screen.
    pub line_shown: f32,
    pub syllable: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/base/voice_lines.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

pub fn protocol(app: &mut App) {
    app.replicate::<MusicBox>()
        .add_systems(OnEnter(ServerState::Running), spawn_music_boxes);
}

fn spawn_music_boxes(mut commands: Commands) {
    commands.spawn((
        MusicBox {
            at: Vec2::new(5.0, -5.0),
            tune: 0,
        },
        Replicated,
    ));
    commands.spawn((
        MusicBox {
            at: Vec2::new(-3.0, 7.0),
            tune: 1,
        },
        Replicated,
    ));
}

/// Who says a line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speaker {
    Bear,
    Crow,
    MusicBox,
}

impl Speaker {
    fn name(self) -> &'static str {
        match self {
            Speaker::Bear => "the bear",
            Speaker::Crow => "a crow",
            Speaker::MusicBox => "a music box",
        }
    }

    /// Base pitch of the babble voice (Hz).
    fn pitch(self) -> f32 {
        match self {
            Speaker::Bear => 110.0,
            Speaker::Crow => 420.0,
            Speaker::MusicBox => 260.0,
        }
    }
}

/// Original, over-acted lines.
pub const LINES: [&str; 10] = [
    "I have waited ONE THOUSAND years for this moment!",
    "Gaze upon me, traveller. Gaze. Upon. Me.",
    "You must find the... the thing. Before it is too late!",
    "My power grows with every passing... second.",
    "Thank you for visiting. Now LEAVE.",
    "Hah! You call that heroism?",
    "Squad! Assemble! (there is no squad)",
    "I am so hungry I could eat a whole... a whole one.",
    "The prophecy was right about you. Mostly.",
    "Welcome, friend! Please don't touch anything!",
];

/// The `n`th line of the session: who says it and what.
pub fn line(n: u32) -> (Speaker, &'static str) {
    let speaker = match n % 3 {
        0 => Speaker::MusicBox,
        1 => Speaker::Bear,
        _ => Speaker::Crow,
    };
    let pick = (n.wrapping_mul(7).wrapping_add(3)) as usize % LINES.len();
    (speaker, LINES[pick])
}

/// Which line (if any) is showing at time `t`.
pub fn showing(t: f32) -> Option<u32> {
    let t = t - 6.0;
    if t < 0.0 || t % tuning().line_period >= tuning().line_shown {
        return None;
    }
    Some((t / tuning().line_period) as u32)
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_subtitle)
        .add_systems(
            Update,
            (dress_music_boxes, bob_music_boxes, speak).run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct Subtitle;

fn spawn_subtitle(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        Subtitle,
        crate::hud::slot(crate::hud::HudSlot::Subtitle),
        DespawnOnExit(AppState::InGame),
    ));
}

/// The singing music box: a modeler-made model
/// (`models/features/music_box.gltf`), lid open and orb glowing.
fn dress_music_boxes(
    mut commands: Commands,
    new: Query<(Entity, &MusicBox), Added<MusicBox>>,
    assets: Res<AssetServer>,
) {
    for (entity, b) in &new {
        commands.entity(entity).insert((
            WorldAssetRoot(
                assets.load(GltfAssetLabel::Scene(0).from_asset("models/features/music_box.gltf")),
            ),
            Transform::from_xyz(b.at.x, 0.0, b.at.y),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

fn bob_music_boxes(
    time: Res<Time>,
    offset: Res<super::stillness::SessionOffset>,
    mut boxes: Query<(&MusicBox, &mut Transform)>,
) {
    // The server's clock, not our process uptime: peers must agree on lines.
    let t = super::stillness::session_secs(&time, &offset);
    let singing = showing(t).is_some_and(|n| line(n).0 == Speaker::MusicBox);
    for (b, mut transform) in &mut boxes {
        let hop = if singing {
            ((t * 8.0 + b.tune as f32).sin() * 0.08).abs()
        } else {
            0.0
        };
        transform.translation = Vec3::new(b.at.x, hop, b.at.y);
    }
}

fn speak(
    time: Res<Time>,
    offset: Res<super::stillness::SessionOffset>,
    mut commands: Commands,
    audio: Option<Res<Assets<AudioSource>>>,
    bank: Option<Res<super::babble::Bank>>,
    mut last: Local<Option<u32>>,
    mut text: Query<&mut Text, With<Subtitle>>,
) {
    let now = showing(super::stillness::session_secs(&time, &offset));
    let want = now.map_or(String::new(), |n| {
        let (who, said) = line(n);
        format!("{}: \"{}\"", who.name(), random_dialogue::say(said))
    });
    if let Some(n) = now
        && *last != Some(n)
        && audio.is_some()
    {
        let (who, said) = line(n);
        let said = random_dialogue::say(said);
        // The recorded-grain bank once loaded; the synth blips without it.
        let bytes = bank
            .as_ref()
            .zip(audio.as_ref())
            .and_then(|(b, a)| a.get(&b.0))
            .map(|src| src.bytes.clone());
        play_synth(&mut commands, move || {
            wav_bytes(&voice(who, &said, n, bytes.as_deref()), RATE)
        });
    }
    *last = now;
    for mut text in &mut text {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

use crate::features::synth_audio::RATE;

/// Each speaker's gibberish voice (see [`super::babble`]).
pub fn babble_voice(who: Speaker) -> super::babble::Voice {
    use super::babble::{Timbre, Voice};
    match who {
        Speaker::Bear => Voice {
            timbre: Timbre::Low,
            pitch: 0.95,
            letter: 0.07,
        },
        Speaker::Crow => Voice {
            timbre: Timbre::Tonal,
            pitch: 2.1,
            letter: 0.045,
        },
        Speaker::MusicBox => Voice {
            timbre: Timbre::Singer,
            pitch: 1.4,
            letter: 0.06,
        },
    }
}

/// The line as sound: recorded-grain gibberish when the bank's `bytes` are
/// here (music boxes still play their jingle first), else [`voice_samples`].
pub fn voice(who: Speaker, said: &str, seed: u32, bytes: Option<&[u8]>) -> Vec<f32> {
    let Some(bank) = bytes
        .and_then(super::babble::parse_wav)
        .filter(|b| b.len() >= super::babble::bank_len())
    else {
        return voice_samples(who, said, seed, RATE);
    };
    let mut out = if who == Speaker::MusicBox {
        // Just the jingle: an empty line has no syllables to blip.
        let mut j = voice_samples(who, "", seed, RATE);
        j.truncate((0.16 * 5.0 * RATE as f32) as usize);
        j
    } else {
        Vec::new()
    };
    out.extend(super::babble::speak(
        &bank,
        babble_voice(who),
        said,
        seed,
        RATE,
    ));
    crate::features::synth_audio::master(&mut out, RATE, 9000.0, 0.6);
    out
}

/// A babble voice: one short vowel-ish blip per syllable of `said`, pitch
/// wandering around the speaker's voice. Music boxes play a jingle first.
pub fn voice_samples(who: Speaker, said: &str, seed: u32, rate: u32) -> Vec<f32> {
    let mut out = Vec::new();
    let mut rng = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 17;
        rng ^= rng << 5;
        rng as f32 / u32::MAX as f32
    };
    if who == Speaker::MusicBox {
        let scale = [523.0, 587.0, 659.0, 784.0, 880.0];
        for _ in 0..5 {
            let f = scale[(next() * 5.0) as usize % 5];
            push_tone(&mut out, rate, f, 0.16, |ph| {
                (ph * std::f32::consts::TAU).sin()
            });
        }
    }
    let syllables = said.split_whitespace().count().clamp(1, 16) * 2;
    for _ in 0..syllables {
        let f = who.pitch() * (0.8 + next() * 0.5);
        let formant = 2.0 + next() * 2.0;
        push_tone(&mut out, rate, f, tuning().syllable, move |ph| {
            let s = (ph * std::f32::consts::TAU).sin();
            (s + 0.5 * (ph * std::f32::consts::TAU * formant).sin()) * 0.6
        });
    }
    crate::features::synth_audio::master(&mut out, rate, 6000.0, 0.55);
    out
}

fn push_tone(out: &mut Vec<f32>, rate: u32, freq: f32, secs: f32, wave: impl Fn(f32) -> f32) {
    let n = (secs * rate as f32) as usize;
    let mut phase = 0.0f32;
    for i in 0..n {
        let p = i as f32 / n as f32;
        phase = (phase + freq / rate as f32).fract();
        let env = (p * 10.0).min(1.0) * ((1.0 - p) * 5.0).min(1.0);
        out.push(wave(phase) * env);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_come_and_go() {
        let shown = (0..10_000)
            .filter(|i| showing(*i as f32 * 0.01).is_some())
            .count();
        assert!(shown > 0 && shown < 5_000, "{shown}");
    }

    #[test]
    fn everyone_gets_a_turn() {
        let speakers: Vec<Speaker> = (0..3).map(|n| line(n).0).collect();
        for s in [Speaker::Bear, Speaker::Crow, Speaker::MusicBox] {
            assert!(speakers.contains(&s));
        }
    }

    #[test]
    fn voice_is_audible_and_safe() {
        let s = voice_samples(Speaker::MusicBox, LINES[0], 3, RATE);
        assert!(!s.is_empty());
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.1 && peak <= 1.0, "peak {peak}");
    }
}
