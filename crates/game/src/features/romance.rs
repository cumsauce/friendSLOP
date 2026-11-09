//! Romance: sometimes an NPC who catches you farting falls for you. From
//! issue #97 by Kori.exe.
//!
//! When the server announces a shameful fart (`Farted { shamed: true }`, see
//! `fart.rs`), it rolls the dice: one time in `SMITTEN_ONE_IN`, the NPC that
//! saw it (the nearest one within `SHAME_RADIUS`) is smitten. Everyone then
//! sees pink hearts float up above that NPC and hears a soft, generated
//! chorus of "aww". You are still shamed: being caught is being caught.

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::{Bear, wav_bytes};
use crate::features::crows::Crow;
use crate::features::fart::{Farted, SHAME_RADIUS, npc_spots};
use crate::features::synth_audio::play_synth;

/// One shameful fart in this many makes the witness fall for you.
pub const SMITTEN_ONE_IN: u32 = 3;
/// How long the hearts float.
pub const HEART_SECS: f32 = 4.0;
/// How many hearts float up.
const HEARTS: usize = 6;

/// Server -> everyone: the NPC at `at` (x, z) is smitten with `player`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Smitten {
    pub player: u64,
    pub at: Vec2,
}

pub fn protocol(app: &mut App) {
    app.init_resource::<LoveDice>()
        .add_server_event::<Smitten>(Channel::Ordered)
        .add_observer(maybe_fall_in_love);
}

/// Server only: tiny deterministic random source.
#[derive(Resource)]
struct LoveDice(u32);

impl Default for LoveDice {
    fn default() -> Self {
        Self(0x10e_b0b5)
    }
}

impl LoveDice {
    fn roll(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0 >> 8
    }
}

/// The NPC (of `npcs`) that saw a fart at `at`: the closest within range.
pub fn witness(at: Vec2, npcs: &[Vec2]) -> Option<Vec2> {
    npcs.iter()
        .copied()
        .filter(|n| n.distance(at) < SHAME_RADIUS)
        .min_by(|a, b| a.distance(at).total_cmp(&b.distance(at)))
}

/// Server: a shameful fart might be a romantic one.
fn maybe_fall_in_love(
    farted: On<Farted>,
    server: Res<State<ServerState>>,
    mut dice: ResMut<LoveDice>,
    mut commands: Commands,
    time: Res<Time>,
    bears: Query<&Bear>,
    crows: Query<&Crow>,
) {
    if *server.get() != ServerState::Running || !farted.shamed {
        return;
    }
    if !dice.roll().is_multiple_of(SMITTEN_ONE_IN) {
        return;
    }
    let npcs = npc_spots(time.elapsed_secs(), bears, crows);
    let Some(at) = witness(farted.at, &npcs) else {
        return;
    };
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: Smitten {
            player: farted.player,
            at,
        },
    });
}

pub fn plugin(app: &mut App) {
    app.add_observer(swoon)
        .add_systems(Update, float_hearts.run_if(in_state(AppState::InGame)));
}

/// A floating heart: seconds alive, and its sideways wobble phase.
#[derive(Component)]
struct Heart {
    age: f32,
    phase: f32,
    base: Vec3,
}

/// Hearts above the smitten NPC, and a soft "aww".
fn swoon(
    smitten: On<Smitten>,
    mut commands: Commands,
    state: Option<Res<State<AppState>>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    audio: Option<Res<Assets<AudioSource>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    if audio.is_some() {
        play_synth(&mut commands, || wav_bytes(&aww(AWW_RATE), AWW_RATE));
    }
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let lobe = meshes.add(Sphere::new(0.1));
    let point = meshes.add(Cuboid::new(0.2, 0.2, 0.08));
    let pink = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.35, 0.6),
        emissive: LinearRgba::rgb(1.2, 0.2, 0.5),
        unlit: true,
        ..default()
    });
    let ground = crate::terrain::height_at(smitten.at.x, smitten.at.y);
    for i in 0..HEARTS {
        let f = i as f32;
        let base = Vec3::new(
            smitten.at.x + (f * 2.4).sin() * 0.4,
            ground + 1.8 + f * 0.15,
            smitten.at.y + (f * 1.7).cos() * 0.4,
        );
        commands
            .spawn((
                Heart {
                    age: -f * 0.3, // they rise one after another
                    phase: f * 1.3,
                    base,
                },
                Transform::from_translation(base).with_scale(Vec3::ZERO),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|heart| {
                // A heart: a square on its corner with two round lobes on top.
                heart.spawn((
                    Mesh3d(point.clone()),
                    MeshMaterial3d(pink.clone()),
                    Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)),
                ));
                for side in [-1.0, 1.0] {
                    heart.spawn((
                        Mesh3d(lobe.clone()),
                        MeshMaterial3d(pink.clone()),
                        Transform::from_xyz(side * 0.07, 0.07, 0.0),
                    ));
                }
            });
    }
}

/// Hearts drift up, sway and spin gently, then shrink away.
fn float_hearts(
    mut commands: Commands,
    time: Res<Time>,
    mut hearts: Query<(Entity, &mut Heart, &mut Transform)>,
) {
    for (entity, mut heart, mut tf) in &mut hearts {
        heart.age += time.delta_secs();
        if heart.age >= HEART_SECS {
            commands.entity(entity).despawn();
            continue;
        }
        let t = heart.age.max(0.0);
        let size = if heart.age < 0.0 {
            0.0
        } else {
            (t * 4.0).min(1.0) * ((HEART_SECS - t) * 1.5).min(1.0)
        };
        let sway = (t * 2.5 + heart.phase).sin() * 0.25;
        tf.translation = heart.base + Vec3::new(sway, t * 0.6, 0.0);
        tf.rotation = Quat::from_rotation_y(t * 1.5 + heart.phase);
        tf.scale = Vec3::splat(size * (1.0 + 0.1 * (t * 9.0).sin()));
    }
}

const AWW_RATE: u32 = 22_050;
/// Length of the "aww".
pub const AWW_SECS: f32 = 1.6;

/// A soft, breathy chorus going "awww": a few voices, each a stack of
/// harmonics shaped by "aw" vowel formants, sliding gently down in pitch with
/// a little vibrato, faded in and out.
pub fn aww(rate: u32) -> Vec<f32> {
    // (start pitch Hz, vibrato rate, delay s): a small, slightly ragged crowd.
    const VOICES: [(f32, f32, f32); 3] =
        [(330.0, 5.1, 0.0), (262.0, 4.6, 0.06), (392.0, 5.7, 0.11)];
    // "aw" formants: (centre Hz, width Hz).
    const FORMANTS: [(f32, f32); 2] = [(650.0, 180.0), (1000.0, 220.0)];
    let n = (AWW_SECS * rate as f32) as usize;
    let mut out = vec![0.0f32; n];
    for (pitch, vib, delay) in VOICES {
        let mut phase = 0.0f32;
        let len = AWW_SECS - delay;
        for (i, s) in out.iter_mut().enumerate() {
            let t = i as f32 / rate as f32 - delay;
            if t < 0.0 {
                continue;
            }
            let u = t / len;
            let f0 =
                pitch * (1.0 - 0.18 * u) * (1.0 + 0.012 * (t * vib * std::f32::consts::TAU).sin());
            phase = (phase + f0 / rate as f32).fract();
            let mut v = 0.0;
            for k in 1..=12 {
                let fk = f0 * k as f32;
                let amp: f32 = FORMANTS
                    .iter()
                    .map(|(c, w)| (-((fk - c) / w).powi(2)).exp())
                    .sum::<f32>()
                    + 0.3 / k as f32;
                v += amp * (phase * k as f32 * std::f32::consts::TAU).sin();
            }
            let env = (t / 0.25).min(1.0) * ((len - t) / 0.6).clamp(0.0, 1.0);
            *s += v * env * 0.08;
        }
    }
    for s in &mut out {
        *s = s.tanh() * 0.6;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_closest_onlooker_is_smitten() {
        let npcs = [
            Vec2::new(3.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(30.0, 0.0),
        ];
        assert_eq!(witness(Vec2::ZERO, &npcs), Some(Vec2::new(1.0, 0.0)));
        assert_eq!(witness(Vec2::new(20.0, 20.0), &npcs), None);
    }

    #[test]
    fn romance_happens_sometimes() {
        let mut dice = LoveDice::default();
        let hits = (0..3000)
            .filter(|_| dice.roll().is_multiple_of(SMITTEN_ONE_IN))
            .count();
        assert!((700..1300).contains(&hits), "{hits}");
    }

    #[test]
    fn aww_is_soft_and_sane() {
        let s = aww(AWW_RATE);
        assert_eq!(s.len(), (AWW_SECS * AWW_RATE as f32) as usize);
        assert!(s.iter().all(|x| x.is_finite()));
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.05 && peak <= 0.6, "peak {peak}");
    }
}
