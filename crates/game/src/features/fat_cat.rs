//! Everyone gets a fat cat that follows them around, meows horrifically and
//! grows whenever it isn't in view. From issue #145 by hadalade.
//!
//! The server gives every player a [`FatCat`] (replicated) that waddles after
//! their body. Each client watches its *own* cat: whenever none of it is on
//! that player's screen, the client tells the server ([`CatInView`]) and the
//! server grows the cat. Looking at it stops the growth (it never shrinks).
//! The cat is built from primitives; its meows are synthesized locally on
//! every peer (lower and more horrible the bigger it gets), heard in 3D.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::world::MainCamera;

/// Player `owner`'s fat cat. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct FatCat {
    pub owner: u64,
    /// Feet position.
    pub pos: Vec3,
    pub yaw: f32,
    /// Scale: 1.0 is a merely very fat cat.
    pub size: f32,
}

/// Client -> server: "my cat is (or isn't) on my screen".
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct CatInView {
    pub seen: bool,
}

/// Server only: whether the owner can currently see their cat.
#[derive(Component, Default, Debug)]
pub struct Watched(pub bool);

/// How much bigger (in scale units) an unwatched cat gets per second.
pub const GROW_PER_SEC: f32 = 0.08;
/// It stops at this size, so the world still fits around it.
pub const MAX_SIZE: f32 = 25.0;
/// How fast the cat waddles after you (faster than you run, it's motivated).
const SPEED: f32 = 7.0;
/// It stays this far from your feet (times its size).
const HEEL: f32 = 1.3;
/// Further away than this (a teleport, the warehouse) it just appears behind you.
const LOST: f32 = 30.0;

pub fn protocol(app: &mut App) {
    app.replicate::<FatCat>()
        .add_client_event::<CatInView>(Channel::Ordered)
        .add_observer(on_in_view)
        .add_systems(
            Update,
            (give_cats, follow_and_grow)
                .chain()
                .run_if(in_state(ServerState::Running)),
        );
}

/// Server: one cat per player (spawned at their feet), and none for leavers.
fn give_cats(
    mut commands: Commands,
    players: Query<(&Player, &Walker)>,
    cats: Query<(Entity, &FatCat)>,
) {
    for (entity, cat) in &cats {
        if !players.iter().any(|(p, _)| p.id == cat.owner) {
            commands.entity(entity).despawn();
        }
    }
    for (player, walker) in &players {
        if cats.iter().any(|(_, c)| c.owner == player.id) {
            continue;
        }
        commands.spawn((
            FatCat {
                owner: player.id,
                pos: walker.pos + Vec3::new(1.0, 0.0, 1.0),
                yaw: 0.0,
                size: 1.0,
            },
            Watched::default(),
            Replicated,
        ));
    }
}

fn on_in_view(
    ev: On<FromClient<CatInView>>,
    players: Query<(&Player, &PlayerOwner)>,
    mut cats: Query<(&FatCat, &mut Watched)>,
) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((me, _)) = players.iter().find(|(_, o)| o.0 == owner) else {
        return;
    };
    for (cat, mut watched) in &mut cats {
        if cat.owner == me.id {
            watched.0 = ev.seen;
        }
    }
}

/// Where a cat at `from` ends up after `dt` seconds of waddling towards
/// `target`, keeping `heel` metres away. Pure so it can be tested.
pub fn waddle(from: Vec2, target: Vec2, heel: f32, dt: f32) -> Vec2 {
    let gap = target - from;
    let len = gap.length();
    if len <= heel {
        return from;
    }
    from + gap / len * (SPEED * dt).min(len - heel)
}

/// The size after `dt` seconds, growing only while nobody's watching.
pub fn grown(size: f32, watched: bool, dt: f32) -> f32 {
    if watched {
        size
    } else {
        (size + GROW_PER_SEC * dt).min(MAX_SIZE)
    }
}

fn follow_and_grow(
    time: Res<Time>,
    players: Query<(&Player, &Walker)>,
    mut cats: Query<(&mut FatCat, &Watched)>,
    still: Query<&super::stillness::Stillness>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let dt = time.delta_secs();
    for (mut cat, watched) in &mut cats {
        let Some((_, walker)) = players.iter().find(|(p, _)| p.id == cat.owner) else {
            continue;
        };
        let mut next = *cat;
        next.size = grown(cat.size, watched.0, dt);
        let owner = walker.pos;
        let here = cat.pos.xz();
        let flat = if here.distance(owner.xz()) > LOST + HEEL * next.size {
            // Lost you: pop up behind you.
            let behind = Vec2::new(walker.yaw.sin(), walker.yaw.cos()) * HEEL * next.size;
            owner.xz() + behind
        } else {
            waddle(here, owner.xz(), HEEL * next.size, dt)
        };
        let gap = owner.xz() - flat;
        if gap.length() > 0.05 {
            next.yaw = (-gap.x).atan2(-gap.y);
        }
        // On the meadow it sits on the ground; indoors or underground it
        // stands on your floor instead.
        let ground = crate::terrain::height_at(flat.x, flat.y);
        let y = if (owner.y - ground).abs() > 2.0 {
            owner.y
        } else {
            ground
        };
        next.pos = Vec3::new(flat.x, y, flat.y);
        if next != *cat {
            *cat = next;
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_cat_hud)
        .add_systems(
            Update,
            (
                dress_cats,
                move_cats,
                report_in_view,
                meow,
                update_cat_hud.run_if(crate::world::hud_tick()),
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// Local: the time until this cat's next meow.
#[derive(Component)]
struct MeowIn {
    secs: f32,
    count: u32,
}

const FUR: Color = Color::srgb(0.95, 0.55, 0.2);
const STRIPE: Color = Color::srgb(0.7, 0.35, 0.1);
const BELLY: Color = Color::srgb(1.0, 0.92, 0.8);

fn dress_cats(
    mut commands: Commands,
    new: Query<(Entity, &FatCat), Added<FatCat>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, cat) in &new {
        let fur = crate::art::soft(&mut materials, FUR);
        let stripe = crate::art::soft(&mut materials, STRIPE);
        let belly = crate::art::soft(&mut materials, BELLY);
        let pink = crate::art::soft(&mut materials, Color::srgb(1.0, 0.6, 0.65));
        let whisker = crate::art::soft(&mut materials, Color::WHITE);
        commands
            .entity(entity)
            .insert((
                Transform::from_translation(cat.pos)
                    .with_rotation(Quat::from_rotation_y(cat.yaw))
                    .with_scale(Vec3::splat(cat.size)),
                Visibility::default(),
                MeowIn {
                    secs: 2.0 + (cat.owner % 5) as f32,
                    count: 0,
                },
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                // The enormous body: wider than it is long.
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.4))),
                    MeshMaterial3d(fur.clone()),
                    Transform::from_xyz(0.0, 0.38, 0.0).with_scale(Vec3::new(1.35, 0.95, 1.4)),
                ));
                // Belly sagging underneath.
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.3))),
                    MeshMaterial3d(belly.clone()),
                    Transform::from_xyz(0.0, 0.24, -0.05).with_scale(Vec3::new(1.4, 0.7, 1.5)),
                ));
                // Tabby stripes across the back.
                let band = meshes.add(Capsule3d::new(0.05, 0.6));
                for z in [-0.2, 0.0, 0.2] {
                    p.spawn((
                        Mesh3d(band.clone()),
                        MeshMaterial3d(stripe.clone()),
                        Transform::from_xyz(0.0, 0.73, z)
                            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                            .with_scale(Vec3::new(1.0, 1.0, 0.5)),
                    ));
                }
                // Stubby legs, barely visible under all that cat.
                let leg = meshes.add(Capsule3d::new(0.07, 0.08));
                for (x, z) in [(-0.3, -0.3), (0.3, -0.3), (-0.3, 0.3), (0.3, 0.3)] {
                    p.spawn((
                        Mesh3d(leg.clone()),
                        MeshMaterial3d(fur.clone()),
                        Transform::from_xyz(x, 0.08, z),
                    ));
                }
                // Tail curling up behind.
                p.spawn((
                    Mesh3d(meshes.add(Capsule3d::new(0.06, 0.4))),
                    MeshMaterial3d(fur.clone()),
                    Transform::from_xyz(0.0, 0.6, 0.6).with_rotation(Quat::from_rotation_x(0.7)),
                ));
                // Head with ears, eyes, nose and whiskers (facing -Z).
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.24))),
                    MeshMaterial3d(fur.clone()),
                    Transform::from_xyz(0.0, 0.7, -0.5).with_scale(Vec3::new(1.15, 0.95, 1.0)),
                ))
                .with_children(|h| {
                    crate::art::eyes(
                        h,
                        &mut meshes,
                        &mut materials,
                        Vec3::new(0.0, 0.05, -0.2),
                        0.18,
                        0.05,
                    );
                    let ear = meshes.add(Cone::new(0.08, 0.16).mesh().resolution(4));
                    for x in [-0.13, 0.13] {
                        h.spawn((
                            Mesh3d(ear.clone()),
                            MeshMaterial3d(fur.clone()),
                            Transform::from_xyz(x, 0.21, 0.0)
                                .with_rotation(Quat::from_rotation_z(-x * 2.5)),
                        ));
                    }
                    h.spawn((
                        Mesh3d(meshes.add(Sphere::new(0.03))),
                        MeshMaterial3d(pink.clone()),
                        Transform::from_xyz(0.0, -0.03, -0.235),
                    ));
                    let hair = meshes.add(Cylinder::new(0.004, 0.22));
                    for side in [-1.0f32, 1.0] {
                        for tilt in [-0.15f32, 0.15] {
                            h.spawn((
                                Mesh3d(hair.clone()),
                                MeshMaterial3d(whisker.clone()),
                                Transform::from_xyz(side * 0.15, -0.05, -0.19).with_rotation(
                                    Quat::from_rotation_z(std::f32::consts::FRAC_PI_2 + tilt),
                                ),
                            ));
                        }
                    }
                });
            });
    }
}

/// Glide to the replicated spot and size, rolling side to side as it waddles.
fn move_cats(time: Res<Time>, mut cats: Query<(&FatCat, &mut Transform)>) {
    let t = time.elapsed_secs();
    let k = (time.delta_secs() * 8.0).min(1.0);
    for (cat, mut tf) in &mut cats {
        let gap = cat.pos - tf.translation;
        let moving = gap.xz().length() > 0.02 * cat.size;
        tf.translation += gap * k;
        let size = tf.scale.x + (cat.size - tf.scale.x) * k;
        tf.scale = Vec3::splat(size);
        let roll = if moving {
            (t * 9.0 + cat.owner as f32).sin() * 0.12
        } else {
            0.0
        };
        tf.rotation = Quat::from_rotation_y(cat.yaw) * Quat::from_rotation_z(roll);
    }
}

/// Points spread over a cat of `size` at `pos`: if any is on screen, it's in view.
fn sample_points(pos: Vec3, size: f32) -> [Vec3; 7] {
    let r = 0.5 * size;
    let c = pos + Vec3::Y * 0.4 * size;
    [
        c,
        c + Vec3::Y * r,
        c - Vec3::Y * 0.3 * size,
        c + Vec3::X * r,
        c - Vec3::X * r,
        c + Vec3::Z * r,
        c - Vec3::Z * r,
    ]
}

/// True if `p` lands inside the camera's picture (in front of it).
fn on_screen(camera: &Camera, cam_tf: &GlobalTransform, p: Vec3) -> bool {
    let view = cam_tf.affine().inverse().transform_point3(p);
    if view.z >= 0.0 {
        return false; // behind the camera
    }
    camera
        .world_to_ndc(cam_tf, p)
        .is_some_and(|n| n.x.abs() <= 1.0 && n.y.abs() <= 1.0)
}

/// Tell the server whether my cat is on my screen: whenever it changes, and
/// twice a second anyway.
fn report_in_view(
    mut commands: Commands,
    time: Res<Time>,
    mut last: Local<Option<bool>>,
    mut every: Local<f32>,
    me: Option<Res<LocalPlayer>>,
    cats: Query<(&FatCat, &Transform)>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    let Some(me) = me else { return };
    let Some((cat, tf)) = cats.iter().find(|(c, _)| c.owner == me.0) else {
        return;
    };
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };
    let seen = sample_points(tf.translation, cat.size.max(tf.scale.x))
        .iter()
        .any(|p| on_screen(camera, cam_tf, *p));
    *every -= time.delta_secs();
    if *last != Some(seen) || *every <= 0.0 {
        *last = Some(seen);
        *every = 0.5;
        commands.client_trigger(CatInView { seen });
    }
}

/// Every few seconds each cat lets out a horrible meow, deeper the bigger it is.
fn meow(mut commands: Commands, time: Res<Time>, mut cats: Query<(&FatCat, &mut MeowIn)>) {
    for (cat, mut next) in &mut cats {
        next.secs -= time.delta_secs();
        if next.secs > 0.0 {
            continue;
        }
        next.count += 1;
        let seed = (cat.owner as u32).wrapping_mul(2_654_435_761) ^ next.count;
        next.secs = 5.0 + crate::art::hash(seed as i32, next.count as i32, 145) * 9.0;
        let size = cat.size;
        let mouth = cat.pos + Vec3::Y * 0.7 * size;
        let carry = if size > 3.0 {
            crate::spatial::Carry::LOUD
        } else {
            crate::spatial::Carry::CALL
        };
        crate::spatial::play_wav_at(&mut commands, mouth, 1.0, carry, move || {
            let s = meow_samples(super::synth_audio::RATE, size, seed);
            super::bear::wav_bytes(&s, super::synth_audio::RATE)
        });
    }
}

/// A horrific meow: a detuned, ring-modulated, distorted "mrrrEEEOOOWWW"
/// whose vowel sweeps from m to ee to ow. Bigger cats meow lower and longer.
pub fn meow_samples(rate: u32, size: f32, seed: u32) -> Vec<f32> {
    use std::f32::consts::{PI, TAU};
    let size = size.max(0.1);
    let length = (1.4 + 0.25 * size.sqrt()).min(3.5);
    let pitch_k = (1.0 / size.sqrt()).clamp(0.3, 1.0);
    let wobble = 0.9 + (seed % 7) as f32 * 0.04;
    let n = (length * rate as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let detune = [1.0f32, 1.017, 0.983, 1.49];
    let mut phases = [0.0f32; 4];
    // Two resonant band-pass filters for the vowel (state-variable).
    let mut f1 = (0.0f32, 0.0f32);
    let mut f2 = (0.0f32, 0.0f32);
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let p = t / length;
        // "mrr" low, then up into the scream, then a long sag down.
        let contour = if p < 0.25 {
            380.0 + 600.0 * (p / 0.25)
        } else {
            980.0 - 700.0 * ((p - 0.25) / 0.75).powf(0.7)
        };
        // Vibrato that gets wilder as the meow goes on.
        let vib = 1.0 + (0.01 + 0.06 * p) * (TAU * 6.5 * wobble * t).sin();
        let f0 = contour * vib * pitch_k * wobble;
        let mut voice = 0.0;
        for (k, ph) in phases.iter_mut().enumerate() {
            let inc = f0 * detune[k] / rate as f32;
            *ph = (*ph + inc).fract();
            let gain = if k == 3 { 0.4 } else { 1.0 };
            voice += super::synth_audio::saw(*ph, inc) * gain;
        }
        // Ring modulation: a sickly metallic growl under it.
        voice *= 0.6 + 0.4 * (TAU * 47.0 * pitch_k * t).sin();
        // Vowel: m (300 Hz) -> ee (2400 Hz) -> ow (700 Hz).
        let vowel = (p * PI).sin();
        let form1 = 300.0 + 500.0 * vowel;
        let form2 = 700.0 + 1700.0 * vowel * (1.0 - p * 0.6);
        let band = |state: &mut (f32, f32), freq: f32, x: f32| {
            let f = 2.0 * (PI * freq / rate as f32).sin();
            let q = 0.25;
            let high = x - state.0 - q * state.1;
            state.1 += f * high;
            state.0 += f * state.1;
            state.1
        };
        let shaped = band(&mut f1, form1, voice) + 0.7 * band(&mut f2, form2, voice);
        // Clip it hard: horrific, as requested.
        let crushed = (shaped * 3.0).tanh();
        let env = (p * 12.0).min(1.0) * ((1.0 - p) * 3.0).min(1.0);
        out.push(crushed * env);
    }
    super::synth_audio::master(&mut out, rate, 6000.0, 0.75);
    out
}

#[derive(Component)]
struct CatHud;

fn spawn_cat_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        CatHud,
        crate::hud::slot(crate::hud::HudSlot::BottomLeft),
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_cat_hud(
    me: Option<Res<LocalPlayer>>,
    cats: Query<&FatCat>,
    mut hud: Query<&mut Text, With<CatHud>>,
) {
    let want = me
        .and_then(|me| cats.iter().find(|c| c.owner == me.0))
        .map(|c| {
            let warning = if c.size >= MAX_SIZE {
                "it can't get any bigger"
            } else {
                "it grows when you don't look at it"
            };
            format!("your fat cat: {:.1}x ({warning})", c.size)
        })
        .unwrap_or_default();
    for mut text in &mut hud {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cat_waddles_to_heel() {
        let p = waddle(Vec2::ZERO, Vec2::new(10.0, 0.0), 1.0, 0.5);
        assert!((p.x - SPEED * 0.5).abs() < 1e-4);
        let p = waddle(Vec2::ZERO, Vec2::new(3.0, 0.0), 1.0, 5.0);
        assert!((p.x - 2.0).abs() < 1e-4, "stops at heel: {p}");
        assert_eq!(
            waddle(Vec2::ZERO, Vec2::new(0.5, 0.0), 1.0, 1.0),
            Vec2::ZERO
        );
    }

    #[test]
    fn grows_only_when_unwatched_and_stops_at_max() {
        assert_eq!(grown(2.0, true, 10.0), 2.0);
        assert!(grown(2.0, false, 1.0) > 2.0);
        assert_eq!(grown(MAX_SIZE - 0.01, false, 100.0), MAX_SIZE);
    }

    #[test]
    fn meow_is_loud_but_safe_and_bigger_cats_meow_longer() {
        let rate = crate::features::synth_audio::RATE;
        let small = meow_samples(rate, 1.0, 3);
        let big = meow_samples(rate, 16.0, 3);
        assert!(big.len() > small.len());
        for s in [&small, &big] {
            let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak > 0.5 && peak <= 0.76, "peak {peak}");
            assert!(s.iter().all(|x| x.is_finite()));
        }
    }
}
