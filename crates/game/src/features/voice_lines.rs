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

pub const MUSIC_BOX_COUNT: u8 = 2;

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

/// Seconds between voice lines.
pub const LINE_PERIOD: f32 = 17.0;
/// How long a line stays on screen.
pub const LINE_SHOWN: f32 = 4.5;

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
    if t < 0.0 || t % LINE_PERIOD >= LINE_SHOWN {
        return None;
    }
    Some((t / LINE_PERIOD) as u32)
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
        Node {
            position_type: PositionType::Absolute,
            bottom: px(92),
            left: percent(25),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
}

fn dress_music_boxes(
    mut commands: Commands,
    new: Query<(Entity, &MusicBox), Added<MusicBox>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, b) in &new {
        let wood = crate::art::glossy(&mut materials, Color::srgb(0.45, 0.2, 0.5));
        let gold = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.8, 0.3),
            metallic: 0.9,
            perceptual_roughness: 0.3,
            ..default()
        });
        let glow = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.85, 0.3),
            emissive: LinearRgba::rgb(2.0, 1.6, 0.4),
            ..default()
        });
        commands
            .entity(entity)
            .insert((
                Transform::from_xyz(b.at.x, 0.0, b.at.y),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.6, 0.35, 0.4))),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(0.0, 0.2, 0.0),
                ));
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.12))),
                    MeshMaterial3d(glow.clone()),
                    Transform::from_xyz(0.0, 0.55, 0.0),
                ));
                // Open lid tilted back, gold trim, a wind-up key and little feet.
                p.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.62, 0.05, 0.42))),
                    MeshMaterial3d(wood.clone()),
                    Transform::from_xyz(0.0, 0.55, 0.3).with_rotation(Quat::from_rotation_x(1.2)),
                ));
                p.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.64, 0.05, 0.44))),
                    MeshMaterial3d(gold.clone()),
                    Transform::from_xyz(0.0, 0.38, 0.0),
                ));
                p.spawn((
                    Mesh3d(meshes.add(Torus::new(0.03, 0.07))),
                    MeshMaterial3d(gold.clone()),
                    Transform::from_xyz(0.36, 0.22, 0.0)
                        .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                ));
                let foot = meshes.add(Sphere::new(0.05));
                for (x, z) in [(-0.25, -0.15), (0.25, -0.15), (-0.25, 0.15), (0.25, 0.15)] {
                    p.spawn((
                        Mesh3d(foot.clone()),
                        MeshMaterial3d(gold.clone()),
                        Transform::from_xyz(x, 0.02, z),
                    ));
                }
            });
    }
}

fn bob_music_boxes(time: Res<Time>, mut boxes: Query<(&MusicBox, &mut Transform)>) {
    let t = time.elapsed_secs();
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
    mut commands: Commands,
    audio: Option<Res<Assets<AudioSource>>>,
    mut last: Local<Option<u32>>,
    mut text: Query<&mut Text, With<Subtitle>>,
) {
    let now = showing(time.elapsed_secs());
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
        play_synth(&mut commands, move || {
            wav_bytes(&voice_samples(who, &said, n, RATE), RATE)
        });
    }
    *last = now;
    for mut text in &mut text {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

const RATE: u32 = 22_050;
const SYLLABLE: f32 = 0.11;

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
        push_tone(&mut out, rate, f, SYLLABLE, move |ph| {
            let s = (ph * std::f32::consts::TAU).sin();
            (s + 0.5 * (ph * std::f32::consts::TAU * formant).sin()) * 0.6
        });
    }
    out
}

fn push_tone(out: &mut Vec<f32>, rate: u32, freq: f32, secs: f32, wave: impl Fn(f32) -> f32) {
    let n = (secs * rate as f32) as usize;
    let mut phase = 0.0f32;
    for i in 0..n {
        let p = i as f32 / n as f32;
        phase = (phase + freq / rate as f32).fract();
        let env = (p * 10.0).min(1.0) * ((1.0 - p) * 5.0).min(1.0);
        out.push((wave(phase) * env * 0.45).clamp(-1.0, 1.0));
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
