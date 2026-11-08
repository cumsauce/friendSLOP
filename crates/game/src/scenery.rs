//! Dresses the shared floor with low-poly nature (trees, grass, flowers,
//! rocks), lights a campfire beside the burnt parliament and loops a little
//! tune. Pure scenery from `assets/models/nature` (see CREDITS.md): nothing
//! here is replicated — every peer scatters the same props the same way from
//! a fixed seed, and missing files simply mean fewer props.

use bevy::audio::Volume;
use bevy::gltf::GltfAssetLabel;
use bevy::prelude::*;

use crate::AppState;
use crate::assets::Sfx;
use crate::features::dave_commune::PARLIAMENT_AT;
use crate::features::duel::{ARENA_CENTER, ARENA_HALF};

pub fn plugin(app: &mut App) {
    app.add_systems(
        OnEnter(AppState::InGame),
        (dress_floor, light_bonfire, play_music),
    )
    .add_systems(Update, flicker_bonfire.run_if(in_state(AppState::InGame)));
}

/// Props live in a ring around the action: outside this radius…
const RING_INNER: f32 = 13.0;
/// …and inside this one (the floor itself is 50×50).
const RING_OUTER: f32 = 24.0;
/// How many props to scatter.
const PROP_COUNT: u32 = 150;

/// One model from the nature kit: file, how common it is, size jitter.
struct Prop {
    file: &'static str,
    weight: f32,
    /// Range of uniform random scales, roughly around the model's native size.
    scale: [f32; 2],
}

const PROPS: &[Prop] = &[
    Prop {
        file: "CommonTree_1.gltf",
        weight: 3.0,
        scale: [0.7, 1.2],
    },
    Prop {
        file: "CommonTree_2.gltf",
        weight: 3.0,
        scale: [0.7, 1.2],
    },
    Prop {
        file: "CommonTree_4.gltf",
        weight: 2.0,
        scale: [0.7, 1.2],
    },
    Prop {
        file: "Pine_1.gltf",
        weight: 3.0,
        scale: [0.6, 1.1],
    },
    Prop {
        file: "Pine_3.gltf",
        weight: 2.0,
        scale: [0.6, 1.1],
    },
    // Dead and twisted trees lean towards the burnt parliament.
    Prop {
        file: "DeadTree_1.gltf",
        weight: 1.0,
        scale: [0.4, 0.8],
    },
    Prop {
        file: "DeadTree_3.gltf",
        weight: 1.0,
        scale: [0.3, 0.6],
    },
    Prop {
        file: "TwistedTree_2.gltf",
        weight: 1.0,
        scale: [0.3, 0.6],
    },
    Prop {
        file: "Bush_Common.gltf",
        weight: 4.0,
        scale: [0.7, 1.3],
    },
    Prop {
        file: "Bush_Common_Flowers.gltf",
        weight: 2.0,
        scale: [0.7, 1.3],
    },
    Prop {
        file: "Fern_1.gltf",
        weight: 4.0,
        scale: [0.8, 1.5],
    },
    Prop {
        file: "Grass_Common_Tall.gltf",
        weight: 5.0,
        scale: [0.8, 1.5],
    },
    Prop {
        file: "Grass_Wispy_Tall.gltf",
        weight: 5.0,
        scale: [0.8, 1.5],
    },
    Prop {
        file: "Clover_1.gltf",
        weight: 4.0,
        scale: [0.8, 1.5],
    },
    Prop {
        file: "Flower_3_Single.gltf",
        weight: 4.0,
        scale: [0.8, 1.5],
    },
    Prop {
        file: "Flower_3_Group.gltf",
        weight: 3.0,
        scale: [0.8, 1.5],
    },
    Prop {
        file: "Flower_4_Single.gltf",
        weight: 3.0,
        scale: [0.8, 1.5],
    },
    Prop {
        file: "Plant_1.gltf",
        weight: 3.0,
        scale: [0.8, 1.4],
    },
    Prop {
        file: "Plant_7.gltf",
        weight: 3.0,
        scale: [0.8, 1.4],
    },
    Prop {
        file: "Mushroom_Common.gltf",
        weight: 2.0,
        scale: [0.8, 1.6],
    },
    Prop {
        file: "Pebble_Round_1.gltf",
        weight: 3.0,
        scale: [0.6, 1.4],
    },
    Prop {
        file: "Pebble_Round_2.gltf",
        weight: 3.0,
        scale: [0.6, 1.4],
    },
    Prop {
        file: "Pebble_Square_2.gltf",
        weight: 2.0,
        scale: [0.6, 1.4],
    },
    Prop {
        file: "Rock_Medium_1.gltf",
        weight: 3.0,
        scale: [0.4, 1.0],
    },
    Prop {
        file: "Rock_Medium_2.gltf",
        weight: 2.0,
        scale: [0.4, 1.0],
    },
    Prop {
        file: "Rock_Medium_3.gltf",
        weight: 2.0,
        scale: [0.4, 1.0],
    },
];

/// Is `p` clear of the duel arena and the parliament ruins?
fn is_clear(p: Vec2) -> bool {
    let in_rect = |c: Vec2, half: f32| (p.x - c.x).abs() < half && (p.y - c.y).abs() < half;
    // The arena keeps a wide margin so walls sit in open grass.
    if in_rect(ARENA_CENTER.xz(), ARENA_HALF + 4.0) {
        return false;
    }
    // The parliament ruin is bigger than it looks (fallen tower and all).
    if in_rect(PARLIAMENT_AT, 9.0) {
        return false;
    }
    true
}

/// A tiny deterministic random source, same on every peer (like the mushroom
/// patch's), so everyone scatters an identical forest.
fn next_v01(seed: &mut u32) -> f32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*seed >> 8) as f32 / (1u32 << 24) as f32
}

fn dress_floor(mut commands: Commands, assets: Res<AssetServer>) {
    let total = PROPS.iter().map(|p| p.weight).sum::<f32>();
    let mut seed = 0x5EED_5EED;
    for _ in 0..PROP_COUNT {
        let mut pos = Vec2::ZERO;
        for _ in 0..20 {
            // Area-uniform ring sample between the inner and outer radius.
            let r2min = RING_INNER * RING_INNER;
            let r2max = RING_OUTER * RING_OUTER;
            let r = (r2min + next_v01(&mut seed) * (r2max - r2min)).sqrt();
            let angle = next_v01(&mut seed) * std::f32::consts::TAU;
            pos = Vec2::from_angle(angle) * r;
            if is_clear(pos) {
                break;
            }
        }
        if !is_clear(pos) {
            continue;
        }
        let mut pick = next_v01(&mut seed) * total;
        let prop = PROPS
            .iter()
            .find(|p| {
                pick -= p.weight;
                pick <= 0.0
            })
            .unwrap_or(&PROPS[0]);
        let scale = prop.scale[0] + next_v01(&mut seed) * (prop.scale[1] - prop.scale[0]);
        let yaw = next_v01(&mut seed) * std::f32::consts::TAU;
        commands.spawn((
            WorldAssetRoot(
                assets.load(
                    GltfAssetLabel::Scene(0).from_asset(format!("models/nature/{}", prop.file)),
                ),
            ),
            Transform::from_xyz(pos.x, 0.0, pos.y)
                .with_rotation(Quat::from_rotation_y(yaw))
                .with_scale(Vec3::splat(scale)),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Where the commune keeps its campfire, beside the ruins and Dave.
const BONFIRE_AT: Vec3 = Vec3::new(PARLIAMENT_AT.x + 3.5, 0.0, PARLIAMENT_AT.y + 8.5);

#[derive(Component)]
struct BonfireFlame;

fn light_bonfire(mut commands: Commands, assets: Res<AssetServer>) {
    let scene = |file: &str| {
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/props/{file}")))
    };
    commands.spawn((
        WorldAssetRoot(scene("bonfire.glb")),
        Transform::from_translation(BONFIRE_AT).with_scale(Vec3::splat(1.3)),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        WorldAssetRoot(scene("bonfire_fire.glb")),
        BonfireFlame,
        Transform::from_translation(BONFIRE_AT + Vec3::Y * 0.1).with_scale(Vec3::splat(1.3)),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.55, 0.2),
            intensity: 1800.0,
            range: 14.0,
            ..default()
        },
        BonfireFlame,
        Transform::from_translation(BONFIRE_AT + Vec3::Y * 1.2),
        DespawnOnExit(AppState::InGame),
    ));
}

/// The fire breathes: gentle flicker on the campfire light.
fn flicker_bonfire(time: Res<Time>, mut flames: Query<&mut PointLight, With<BonfireFlame>>) {
    let t = time.elapsed_secs();
    for mut light in &mut flames {
        let flicker = (t * 7.0).sin() * 200.0 + (t * 13.7).sin() * 150.0;
        light.intensity = 1800.0 + flicker;
    }
}

fn play_music(mut commands: Commands, sfx: Option<Res<Sfx>>) {
    let Some(sfx) = sfx else { return };
    commands.spawn((
        AudioPlayer(sfx.music.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.35)),
        DespawnOnExit(AppState::InGame),
    ));
}
