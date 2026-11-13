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
use crate::features::fart::{Farted, npc_spots};

pub const AWW_RATE: u32 = 22050;

/// Tuning numbers: `assets/data/village/romance.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// One shameful fart in this many makes the witness fall for you.
    pub smitten_one_in: u32,
    /// How long the hearts float.
    pub heart_secs: f32,
    /// How many hearts float up.
    pub hearts: usize,
    /// Length of the "aww".
    pub aww_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/village/romance.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

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
    witness_within(at, npcs, crate::features::fart::tuning().shame_radius)
}

/// Like [`witness`], with NPCs that see `radius` metres (#173).
pub fn witness_within(at: Vec2, npcs: &[Vec2], radius: f32) -> Option<Vec2> {
    npcs.iter()
        .copied()
        .filter(|n| n.distance(at) < radius)
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
    villagers: Query<&crate::rules::villagers::Villager>,
    odds: Query<&crate::rules::house_odds::HouseOdds>,
) {
    if *server.get() != ServerState::Running || !farted.shamed {
        return;
    }
    // Marmalade's wheel (#173) makes love likelier or rarer.
    let odds = crate::rules::house_odds::odds(&odds);
    if !dice.roll().is_multiple_of(odds.romance_one_in().max(1)) {
        return;
    }
    let npcs = npc_spots(time.elapsed_secs(), bears, crows, villagers);
    let Some(at) = witness_within(farted.at, &npcs, odds.shame_radius()) else {
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
    app.add_observer(swoon).add_systems(
        Update,
        (shape_hearts, float_hearts).run_if(in_state(AppState::InGame)),
    );
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
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    audio: Option<Res<Assets<AudioSource>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    if audio.is_some() {
        // From the smitten one.
        let at = Vec3::new(
            smitten.at.x,
            crate::terrain::height_at(smitten.at.x, smitten.at.y) + 1.5,
            smitten.at.y,
        );
        let carry = crate::spatial::Carry::CALL;
        crate::spatial::play_wav_at(&mut commands, at, 1.0, carry, || {
            wav_bytes(&aww(AWW_RATE), AWW_RATE)
        });
    }
    let Some(mut materials) = materials else {
        return;
    };
    let ground = crate::terrain::height_at(smitten.at.x, smitten.at.y);
    spawn_hearts(
        &mut commands,
        &mut materials,
        Vec3::new(smitten.at.x, ground + 1.8, smitten.at.y),
    );
}

/// Pink hearts that rise one after another from `above` (also used by
/// Abuela, see abuela.rs).
pub fn spawn_hearts(
    commands: &mut Commands,
    materials: &mut Assets<StandardMaterial>,
    above: Vec3,
) {
    let pink = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.35, 0.6),
        emissive: LinearRgba::rgb(1.2, 0.2, 0.5),
        unlit: true,
        ..default()
    });
    for i in 0..tuning().hearts {
        let f = i as f32;
        let base = Vec3::new(
            above.x + (f * 2.4).sin() * 0.4,
            above.y + f * 0.15,
            above.z + (f * 1.7).cos() * 0.4,
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
                heart.spawn((HeartShape, MeshMaterial3d(pink.clone())));
            });
    }
}

/// A heart's look: the modeler heart (prefab `sp-heart`, 0.35 m across,
/// centred) put on as the entity's mesh, worn in whatever glowing material
/// it was spawned with. Sesbian Lex and the affair use it too.
#[derive(Component)]
pub struct HeartShape;

/// The heart model's mesh (the asset server loads the file once).
pub fn heart_mesh(assets: &AssetServer) -> Handle<Mesh> {
    assets.load(
        GltfAssetLabel::Primitive {
            mesh: 0,
            primitive: 0,
        }
        .from_asset("models/features/heart.gltf"),
    )
}

fn shape_hearts(
    mut commands: Commands,
    new: Query<Entity, Added<HeartShape>>,
    assets: Option<Res<AssetServer>>,
) {
    let Some(assets) = assets else {
        return;
    };
    for e in &new {
        commands.entity(e).insert(Mesh3d(heart_mesh(&assets)));
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
        if heart.age >= tuning().heart_secs {
            commands.entity(entity).despawn();
            continue;
        }
        let t = heart.age.max(0.0);
        let size = if heart.age < 0.0 {
            0.0
        } else {
            (t * 4.0).min(1.0) * ((tuning().heart_secs - t) * 1.5).min(1.0)
        };
        let sway = (t * 2.5 + heart.phase).sin() * 0.25;
        tf.translation = heart.base + Vec3::new(sway, t * 0.6, 0.0);
        tf.rotation = Quat::from_rotation_y(t * 1.5 + heart.phase);
        tf.scale = Vec3::splat(size * (1.0 + 0.1 * (t * 9.0).sin()));
    }
}

/// A soft, breathy chorus going "awww": a few voices, each a stack of
/// harmonics shaped by "aw" vowel formants, sliding gently down in pitch with
/// a little vibrato, faded in and out.
pub fn aww(rate: u32) -> Vec<f32> {
    // (start pitch Hz, vibrato rate, delay s): a small, slightly ragged crowd.
    const VOICES: [(f32, f32, f32); 3] =
        [(330.0, 5.1, 0.0), (262.0, 4.6, 0.06), (392.0, 5.7, 0.11)];
    // "aw" formants: (centre Hz, width Hz).
    const FORMANTS: [(f32, f32); 2] = [(650.0, 180.0), (1000.0, 220.0)];
    let n = (tuning().aww_secs * rate as f32) as usize;
    let mut out = vec![0.0f32; n];
    for (pitch, vib, delay) in VOICES {
        let mut phase = 0.0f32;
        let len = tuning().aww_secs - delay;
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
            .filter(|_| dice.roll().is_multiple_of(tuning().smitten_one_in))
            .count();
        assert!((700..1300).contains(&hits), "{hits}");
    }

    #[test]
    fn aww_is_soft_and_sane() {
        let s = aww(AWW_RATE);
        assert_eq!(s.len(), (tuning().aww_secs * AWW_RATE as f32) as usize);
        assert!(s.iter().all(|x| x.is_finite()));
        let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        assert!(peak > 0.05 && peak <= 0.6, "peak {peak}");
    }
}
