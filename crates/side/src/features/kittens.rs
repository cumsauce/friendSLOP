//! Kitten gangs. From issue #374 by catherine: "roaming gangs of baby kittens
//! that try to beat u up while meowing. they haunt the city streets".
//!
//! Gangs (`data/side/kittens.ron`) roam New York's streets, each kitten
//! wandering inside its gang's box. Walk near one and the whole gang turns
//! on you: they run at you, meowing, and swat at your ankles. Every swat is a
//! line on your screen; once a gang has landed enough of them you're knocked
//! flying (a ragdoll, #320) and they go back to strutting about. They're baby
//! kittens, so you can outrun them, and they never chase you far from their
//! turf. The kittens are the pet cat model (`models/creatures/cat.gltf`)
//! shrunk to kitten size. Meows are synthesised; nothing needs a sound file.

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::deny;
use crate::player::{Player, PlayerOwner};

pub const MEOW_RATE: u32 = 22050;
/// The kitten model: the rigged pet cat, shrunk.
pub const KITTEN_MODEL: &str = "models/creatures/cat.gltf";
/// Server moves (and replicated updates) per second.
const TICK_HZ: f32 = 10.0;

/// One gang: its turf (a box on the street) and how many kittens.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Gang {
    pub name: String,
    /// Turf middle (x, z) and half sizes (m).
    pub center: (f32, f32),
    pub half: (f32, f32),
    pub count: usize,
}

/// Tuning and writing: `assets/data/side/kittens.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    pub gangs: Vec<Gang>,
    /// Strutting and charging speeds (m/s). Keep charge under a player's run.
    pub strut: f32,
    pub charge: f32,
    /// A gang notices you this close to its turf (m beyond the box edge).
    pub notice: f32,
    /// A kitten swats you from this close (m), once every `swat_every` s.
    pub reach: f32,
    pub swat_every: f32,
    /// Swats a gang lands before you're knocked flying, and how hard (m/s).
    pub swats_to_knock: u32,
    pub knock: f32,
    pub knock_up: f32,
    /// After knocking you down, a gang leaves you alone this long (s).
    pub cooldown: f32,
    /// How big a kitten is next to the pet cat (scale).
    pub size: f32,
    /// Chance per second an angry kitten meows (a calm one meows a tenth as often).
    pub meow_chance: f32,
    /// What you're told when a swat lands, and when you're knocked over.
    pub swat_lines: Vec<String>,
    pub knock_lines: Vec<String>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/kittens.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Server -> everyone: one kitten. `at` is its feet (x, z); `angry` while
/// its gang is after someone.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Kitten {
    pub gang: u8,
    pub id: u8,
    pub at: Vec2,
    pub yaw: f32,
    pub angry: bool,
}

/// Server only: a kitten's wander target, dice and swat timer.
#[derive(Component, Debug)]
struct Prowl {
    to: Vec2,
    seed: u32,
    swat_ready: f32,
}

/// Server only: each gang's swat count on its current victim and cooldown.
#[derive(Resource, Default, Debug)]
struct Gangs {
    swats: Vec<u32>,
    calm_until: Vec<f32>,
    spawned: usize,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Kitten>().add_systems(
        Update,
        (spawn_kittens, prowl)
            .chain()
            .run_if(in_state(ServerState::Running)),
    );
}

fn roll(seed: &mut u32) -> u32 {
    *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    *seed >> 8
}

/// A random spot on gang `g`'s turf.
pub fn spot_on_turf(g: &Gang, r: u32) -> Vec2 {
    let u = (r & 0xfff) as f32 / 4095.0 * 2.0 - 1.0;
    let v = ((r >> 12) & 0xfff) as f32 / 4095.0 * 2.0 - 1.0;
    Vec2::new(g.center.0 + u * g.half.0, g.center.1 + v * g.half.1)
}

/// How far (m) `p` is outside gang `g`'s turf (0 inside).
pub fn off_turf(g: &Gang, p: Vec2) -> f32 {
    let d = (p - Vec2::new(g.center.0, g.center.1)).abs() - Vec2::new(g.half.0, g.half.1);
    d.max(Vec2::ZERO).length()
}

/// Who gang `g` goes for: the nearest body on the street near its turf.
pub fn victim(g: &Gang, notice: f32, bodies: &[(u64, Vec3)]) -> Option<(u64, Vec2)> {
    let mid = Vec2::new(g.center.0, g.center.1);
    bodies
        .iter()
        .filter(|(_, p)| (p.y - crate::rules::nyc::CITY_Y).abs() < 2.0)
        .filter(|(_, p)| off_turf(g, p.xz()) <= notice)
        .min_by(|a, b| a.1.xz().distance(mid).total_cmp(&b.1.xz().distance(mid)))
        .map(|(id, p)| (*id, p.xz()))
}

/// One step from `from` towards `to`, at most `step` m, stopping `short` of it.
pub fn step_towards(from: Vec2, to: Vec2, step: f32, short: f32) -> Vec2 {
    let gap = to - from;
    let len = gap.length();
    if len <= short {
        return from;
    }
    from + gap / len * step.min(len - short)
}

fn spawn_kittens(mut commands: Commands, mut gangs: Local<Option<usize>>) {
    if gangs.is_some() {
        return;
    }
    let t = tuning();
    if t.gangs.is_empty() {
        return; // data not loaded yet
    }
    let mut state = Gangs::default();
    for (gi, g) in t.gangs.iter().enumerate() {
        for id in 0..g.count.min(255) {
            let mut seed = 0x6a77_0000 ^ ((gi * 64 + id) as u32).wrapping_mul(2_654_435_761);
            let at = spot_on_turf(g, roll(&mut seed));
            commands.spawn((
                Kitten {
                    gang: gi as u8,
                    id: id as u8,
                    at,
                    yaw: id as f32 * 1.3,
                    angry: false,
                },
                Prowl {
                    to: at,
                    seed,
                    swat_ready: 0.0,
                },
                Replicated,
            ));
        }
    }
    state.spawned = t.gangs.len();
    state.swats = vec![0; t.gangs.len()];
    state.calm_until = vec![0.0; t.gangs.len()];
    commands.insert_resource(state);
    *gangs = Some(t.gangs.len());
}

/// Server: kittens strut about their turf; when someone comes near, the gang
/// charges and swats them, knocking them flying after enough swats.
#[allow(clippy::too_many_arguments)]
fn prowl(
    mut commands: Commands,
    time: Res<Time>,
    mut acc: Local<f32>,
    mut line: Local<usize>,
    gangs: Option<ResMut<Gangs>>,
    still: Query<&super::stillness::Stillness>,
    bodies: Query<(
        Entity,
        &Player,
        &PlayerOwner,
        &Walker,
        Option<&crate::features::ragdoll::Ragdoll>,
    )>,
    mut kittens: Query<(&mut Kitten, &mut Prowl)>,
) {
    let Some(mut gangs) = gangs else { return };
    *acc += time.delta_secs();
    let dt = 1.0 / TICK_HZ;
    if *acc < dt {
        return;
    }
    *acc = (*acc - dt).min(dt);
    if super::stillness::is_frozen(&still) {
        return;
    }
    let t = tuning();
    let now = time.elapsed_secs();
    let standing: Vec<(u64, Vec3)> = bodies
        .iter()
        .filter(|b| b.4.is_none())
        .map(|b| (b.1.id, b.3.pos))
        .collect();
    let targets: Vec<Option<(u64, Vec2)>> = t
        .gangs
        .iter()
        .enumerate()
        .map(|(gi, g)| {
            if gangs.calm_until.get(gi).is_some_and(|c| now < *c) {
                None
            } else {
                victim(g, t.notice, &standing)
            }
        })
        .collect();
    for (mut kitten, mut p) in &mut kittens {
        let gi = kitten.gang as usize;
        let Some(g) = t.gangs.get(gi) else { continue };
        let target = targets.get(gi).copied().flatten();
        let (to, speed, short) = match target {
            Some((_, at)) => (at, t.charge, t.reach * 0.6),
            None => {
                if kitten.at.distance(p.to) < 0.1 {
                    let r = roll(&mut p.seed);
                    if !r.is_multiple_of(3) {
                        p.to = spot_on_turf(g, roll(&mut p.seed));
                    }
                }
                (p.to, t.strut, 0.0)
            }
        };
        let next = step_towards(kitten.at, to, speed * dt, short);
        let d = to - kitten.at;
        let yaw = if d.length() > 1e-3 {
            (-d.x).atan2(-d.y)
        } else {
            kitten.yaw
        };
        let angry = target.is_some();
        if next != kitten.at || yaw != kitten.yaw || angry != kitten.angry {
            *kitten = Kitten {
                at: next,
                yaw,
                angry,
                ..*kitten
            };
        }
        let Some((who, at)) = target else { continue };
        if next.distance(at) > t.reach || now < p.swat_ready {
            continue;
        }
        p.swat_ready = now + t.swat_every;
        let Some((entity, _, owner, w, _)) = bodies.iter().find(|b| b.1.id == who) else {
            continue;
        };
        let client = owner.0.map_or(ClientId::Server, ClientId::Client);
        let Some(swats) = gangs.swats.get_mut(gi) else {
            continue;
        };
        *swats += 1;
        *line += 1;
        if *swats >= t.swats_to_knock {
            *swats = 0;
            if let Some(c) = gangs.calm_until.get_mut(gi) {
                *c = now + t.cooldown;
            }
            let away = (w.pos.xz() - next).normalize_or(Vec2::X) * t.knock;
            crate::features::ragdoll::knock(
                &mut commands,
                entity,
                Vec3::new(away.x, t.knock_up, away.y),
            );
            if !t.knock_lines.is_empty() {
                let l = &t.knock_lines[*line % t.knock_lines.len()];
                deny(&mut commands, client, format!("{}: {l}", g.name));
            }
        } else if !t.swat_lines.is_empty() {
            let l = &t.swat_lines[*line % t.swat_lines.len()];
            deny(&mut commands, client, l.clone());
        }
    }
    // A gang with nobody to fight forgets its swat count.
    for (gi, tg) in targets.iter().enumerate() {
        if tg.is_none()
            && let Some(s) = gangs.swats.get_mut(gi)
        {
            *s = 0;
        }
    }
}

// ---------------------------------------------------------------- local side

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (dress_kittens, glide_kittens, meow).run_if(in_state(AppState::InGame)),
    );
}

#[derive(Component)]
struct Dressed;

fn dress_kittens(mut commands: Commands, new: Query<(Entity, &Kitten), Without<Dressed>>) {
    let size = tuning().size.max(0.05);
    for (entity, k) in &new {
        commands.entity(entity).insert((
            Dressed,
            Transform::from_xyz(k.at.x, crate::rules::nyc::CITY_Y, k.at.y)
                .with_rotation(Quat::from_rotation_y(k.yaw))
                .with_scale(Vec3::splat(size)),
            Visibility::default(),
            crate::rigged::Rigged { path: KITTEN_MODEL },
            crate::rigged::RigClip::looping("idle"),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// The clip a kitten plays: a fast scamper when charging, a strut when
/// wandering, sitting about when still.
pub fn kitten_clip(moving: bool, angry: bool) -> crate::rigged::RigClip {
    use crate::rigged::RigClip;
    match (moving, angry) {
        (true, true) => RigClip::looping("walk").speed(2.2).fade(0.1),
        (true, false) => RigClip::looping("walk").fade(0.15),
        (false, _) => RigClip::looping("idle").fade(0.25),
    }
}

fn glide_kittens(
    time: Res<Time>,
    mut kittens: Query<(&Kitten, &mut Transform, &mut crate::rigged::RigClip), With<Dressed>>,
) {
    let k = (time.delta_secs() * 12.0).min(1.0);
    for (kitten, mut tf, mut clip) in &mut kittens {
        let here = tf.translation.xz();
        let gap = kitten.at - here;
        let moving = gap.length() > 0.02;
        let next = here + gap * k;
        let facing = Quat::from_rotation_y(kitten.yaw);
        if moving || tf.rotation != facing {
            tf.translation.x = next.x;
            tf.translation.z = next.y;
            tf.rotation = tf.rotation.slerp(facing, k);
        }
        let want = kitten_clip(moving, kitten.angry);
        if clip.name != want.name || clip.speed != want.speed {
            *clip = want;
        }
    }
}

/// Kittens meow: often when they're after you, now and then otherwise.
/// Only kittens near the camera bother.
fn meow(
    mut commands: Commands,
    time: Res<Time>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    audio: Option<Res<Assets<AudioSource>>>,
    kittens: Query<&Kitten>,
    mut seed: Local<u32>,
) {
    if audio.is_none() {
        return;
    }
    let Ok(cam) = camera.single() else { return };
    let t = tuning();
    let dt = time.delta_secs();
    for k in &kittens {
        let at = Vec3::new(k.at.x, crate::rules::nyc::CITY_Y + 0.2, k.at.y);
        if at.distance(cam.translation()) > 30.0 {
            continue;
        }
        let chance = t.meow_chance * if k.angry { 1.0 } else { 0.1 } * dt;
        let r = roll(&mut seed) as f32 / (1u32 << 24) as f32;
        if r >= chance {
            continue;
        }
        let pitch = 1.0 + ((k.id as u32 * 7 + k.gang as u32 * 3) % 9) as f32 * 0.06;
        let angry = k.angry;
        crate::spatial::play_wav_at(
            &mut commands,
            at,
            0.5,
            crate::spatial::Carry::CALL,
            move || super::bear::wav_bytes(&meow_wave(MEOW_RATE, pitch, angry), MEOW_RATE),
        );
    }
}

/// A tiny "mew": a pitch that rises then falls, with a nasal buzz. Angry
/// ones are shorter and higher.
pub fn meow_wave(rate: u32, pitch: f32, angry: bool) -> Vec<f32> {
    let len = if angry { 0.28 } else { 0.42 };
    let base = if angry { 900.0 } else { 700.0 } * pitch;
    let n = (len * rate as f32) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let u = i as f32 / n as f32;
            let f = base * (1.0 + 0.5 * (u * std::f32::consts::PI).sin());
            phase = (phase + f / rate as f32).fract();
            let s = (phase * std::f32::consts::TAU).sin();
            let buzz = (phase * 2.0 * std::f32::consts::TAU).sin() * 0.3;
            let env = (u * 10.0).min(1.0) * (1.0 - u).powf(1.5);
            (s + buzz) / 1.3 * env * 0.35
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gang() -> Gang {
        Gang {
            name: "t".into(),
            center: (0.0, 200.0),
            half: (5.0, 20.0),
            count: 3,
        }
    }

    #[test]
    fn kittens_stay_on_their_turf() {
        let g = gang();
        let mut s = 1u32;
        for _ in 0..200 {
            assert_eq!(off_turf(&g, spot_on_turf(&g, roll(&mut s))), 0.0);
        }
        assert!((off_turf(&g, Vec2::new(8.0, 200.0)) - 3.0).abs() < 1e-4);
    }

    #[test]
    fn gangs_go_for_people_on_the_street_nearby() {
        let g = gang();
        let near = (1, Vec3::new(6.0, 0.0, 200.0));
        let far = (2, Vec3::new(40.0, 0.0, 200.0));
        let up = (3, Vec3::new(0.0, 30.0, 200.0));
        assert_eq!(victim(&g, 4.0, &[far, up]), None);
        assert_eq!(victim(&g, 4.0, &[far, near, up]).map(|v| v.0), Some(1));
    }

    #[test]
    fn charging_stops_at_your_ankles() {
        let a = step_towards(Vec2::ZERO, Vec2::new(10.0, 0.0), 1.0, 0.5);
        assert_eq!(a, Vec2::new(1.0, 0.0));
        let b = step_towards(Vec2::new(9.6, 0.0), Vec2::new(10.0, 0.0), 1.0, 0.5);
        assert_eq!(b, Vec2::new(9.6, 0.0));
    }

    #[test]
    fn angry_kittens_scamper() {
        assert!(kitten_clip(true, true).speed > kitten_clip(true, false).speed);
        assert_eq!(kitten_clip(false, true).name, "idle");
    }

    #[test]
    fn meows_are_sane() {
        for angry in [false, true] {
            let s = meow_wave(MEOW_RATE, 1.2, angry);
            assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 0.36));
            assert!(s.iter().any(|x| x.abs() > 0.1));
        }
    }
}
