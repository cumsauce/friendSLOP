//! Cute crows that caw occasionally. From issue #2 by corvy.
//!
//! The server spawns a few replicated `Crow`s. Their hopping and cawing is
//! pure decoration, so every peer animates them locally from the crow's id
//! (no per-frame network traffic). Each crow is a rigged modeler model
//! (`models/creatures/crow.gltf`) whose `hop` and `caw` clips are kept in
//! step with the session clock. A "caw" is a puff-up and open beak, a line
//! of text at the bottom of the screen and a caw sound (from `assets/audio`).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::assets::Sfx;
use crate::spatial::{Carry, play_on};

/// How many crows the server spawns.
pub const CROW_COUNT: u32 = 3;

/// A crow. `home` is where it hangs around (x, z on the floor).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Crow {
    pub id: u32,
    pub home: Vec2,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Crow>()
        .add_systems(OnEnter(ServerState::Running), spawn_crows);
}

/// Server only: scatter the crows in a ring round their site.
fn spawn_crows(mut commands: Commands) {
    let middle = crate::sites::at("crows");
    for id in 0..CROW_COUNT {
        let angle = id as f32 / CROW_COUNT as f32 * std::f32::consts::TAU;
        let home = middle + Vec2::from_angle(angle) * 4.0;
        commands.spawn((Crow { id, home }, Replicated));
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_caw_text)
        .add_systems(
            Update,
            (
                dress_crows,
                animate_crows,
                crow_clips,
                update_caw_text,
                caw_sound,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// Seconds between one crow's caws.
fn caw_period(id: u32) -> f32 {
    7.0 + id as f32 * 3.5
}

/// How long a caw lasts.
const CAW_LENGTH: f32 = 0.6;

/// Is this crow mid-caw at time `t`?
pub fn is_cawing(id: u32, t: f32) -> bool {
    (t + id as f32 * 2.3) % caw_period(id) < CAW_LENGTH
}

/// Where a crow is at time `t`: wandering in a small loop around home, hopping.
pub fn crow_position(crow: &Crow, t: f32) -> Vec3 {
    let phase = crow.id as f32 * 1.7;
    let wander = Vec2::new((t * 0.3 + phase).sin(), (t * 0.23 + phase).cos()) * 1.5;
    let hop = ((t * 3.0 + phase).sin().max(0.0)) * 0.25;
    Vec3::new(crow.home.x + wander.x, 0.3 + hop, crow.home.y + wander.y)
}

/// The crow model (#2): knitted body, hop and caw clips. Its feet are at the
/// model's origin, which sits `FEET` below the crow's own origin.
const CROW_MODEL: &str = "models/creatures/crow.gltf";
pub const FEET: f32 = 0.3;

/// On a crow: the child entity drawing its rigged model.
#[derive(Component)]
struct CrowRig(Entity);

/// Give every new crow (on every peer) its rigged model.
fn dress_crows(mut commands: Commands, new: Query<(Entity, &Crow), Added<Crow>>) {
    for (entity, crow) in &new {
        let rig = commands
            .spawn((
                crate::rigged::Rigged { path: CROW_MODEL },
                crate::rigged::RigClip::looping("hop"),
                Transform::from_xyz(0.0, -FEET, 0.0),
                Visibility::default(),
                ChildOf(entity),
            ))
            .id();
        commands.entity(entity).insert((
            Transform::from_translation(crow_position(crow, 0.0)),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
            CrowRig(rig),
        ));
    }
}

/// The clip a crow plays at time `t` and how far into it it is, so every
/// peer shows the same hop and caw in step with `crow_position`.
pub fn crow_clip(id: u32, t: f32) -> (&'static str, f32) {
    if is_cawing(id, t) {
        return ("caw", (t + id as f32 * 2.3) % caw_period(id));
    }
    // The hop lifts off where `crow_position`'s sine turns positive.
    let phase = id as f32 * 1.7;
    let cycle = (t * 3.0 + phase).rem_euclid(std::f32::consts::TAU);
    ("hop", cycle / 3.0)
}

/// Hop around and face where we're going.
fn animate_crows(
    time: Res<Time>,
    offset: Res<super::stillness::SessionOffset>,
    mut crows: Query<(&Crow, &mut Transform)>,
) {
    let t = super::stillness::session_secs(&time, &offset);
    for (crow, mut transform) in &mut crows {
        let here = crow_position(crow, t);
        let ahead = crow_position(crow, t + 0.1);
        let look = Vec3::new(ahead.x - here.x, 0.0, ahead.z - here.z);
        if look.length_squared() > 1e-6 {
            transform.look_to(look, Vec3::Y);
        }
        transform.translation = here;
    }
}

/// Pick the hop or caw clip and keep it in step with the session clock.
fn crow_clips(
    time: Res<Time>,
    offset: Res<super::stillness::SessionOffset>,
    crows: Query<(&Crow, &CrowRig)>,
    mut rigs: Query<(
        &mut crate::rigged::RigClip,
        Option<&crate::rigged::RigReady>,
    )>,
    mut players: Query<(&mut AnimationPlayer, &AnimationTransitions)>,
) {
    let t = super::stillness::session_secs(&time, &offset);
    for (crow, rig) in &crows {
        let Ok((mut clip, ready)) = rigs.get_mut(rig.0) else {
            continue;
        };
        let (name, at) = crow_clip(crow.id, t);
        if clip.name != name {
            *clip = if name == "caw" {
                crate::rigged::RigClip::once("caw").fade(0.1)
            } else {
                crate::rigged::RigClip::looping(name)
            };
            continue;
        }
        let Some(ready) = ready else { continue };
        let Ok((mut player, transitions)) = players.get_mut(ready.0) else {
            continue;
        };
        if let Some(main) = transitions.get_main_animation()
            && let Some(active) = player.animation_mut(main)
        {
            active.seek_to(at);
        }
    }
}

#[derive(Component)]
struct CawText;

fn spawn_caw_text(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        CawText,
        crate::hud::slot(crate::hud::HudSlot::Subtitle),
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_caw_text(
    time: Res<Time>,
    offset: Res<super::stillness::SessionOffset>,
    crows: Query<&Crow>,
    mut text: Query<&mut Text, With<CawText>>,
) {
    let t = super::stillness::session_secs(&time, &offset);
    let cawing = crows.iter().any(|c| is_cawing(c.id, t));
    let want = if cawing { "caw!" } else { "" };
    for mut text in &mut text {
        if text.0 != want {
            text.0 = want.to_string();
        }
    }
}

/// Play the caw sound once at the start of each crow's caw.
fn caw_sound(
    mut commands: Commands,
    time: Res<Time>,
    offset: Res<super::stillness::SessionOffset>,
    crows: Query<(Entity, &Crow)>,
    sfx: Option<Res<Sfx>>,
    mut last_caw: Local<[u64; CROW_COUNT as usize]>,
) {
    let Some(sfx) = sfx else { return };
    let t = super::stillness::session_secs(&time, &offset);
    for (entity, crow) in &crows {
        // Key the edge on which caw window this is, not on a bool that goes
        // stale when we leave and re-enter the game mid-caw.
        let slot = crow.id as usize % last_caw.len();
        let window = ((t + crow.id as f32 * 2.3) / caw_period(crow.id)) as u64 + 1;
        if is_cawing(crow.id, t) && last_caw[slot] != window {
            // From the crow's beak, wherever it flies.
            let beak = Vec3::new(0.0, 0.3, -0.3);
            play_on(&mut commands, &sfx.crow_caw, entity, beak, 0.8, Carry::CALL);
            last_caw[slot] = window;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crows_caw_only_occasionally() {
        for id in 0..CROW_COUNT {
            let samples = 10_000;
            let cawing = (0..samples)
                .filter(|i| is_cawing(id, *i as f32 * 0.01))
                .count();
            assert!(cawing > 0, "crow {id} never caws");
            assert!(cawing < samples / 5, "crow {id} caws too much");
        }
    }

    #[test]
    fn crow_clips_follow_the_hop_and_the_caw() {
        for id in 0..CROW_COUNT {
            for i in 0..2000 {
                let t = i as f32 * 0.037;
                let (name, at) = crow_clip(id, t);
                assert_eq!(name == "caw", is_cawing(id, t));
                let length = if name == "caw" { CAW_LENGTH } else { 2.1 };
                assert!((0.0..length).contains(&at), "{name} at {at}");
            }
        }
        // Airborne (hop clip's first half) exactly when crow_position lifts.
        let crow = Crow {
            id: 0,
            home: Vec2::ZERO,
        };
        let t = 0.4;
        let (_, at) = crow_clip(0, t);
        assert_eq!(
            at < std::f32::consts::FRAC_PI_3,
            crow_position(&crow, t).y > 0.3
        );
    }

    #[test]
    fn crows_stay_near_home_and_on_the_ground() {
        let crow = Crow {
            id: 1,
            home: Vec2::new(4.0, 0.0),
        };
        for i in 0..1000 {
            let p = crow_position(&crow, i as f32 * 0.1);
            assert!(p.y >= 0.3 && p.y <= 0.6);
            assert!(Vec2::new(p.x, p.z).distance(crow.home) < 2.2);
        }
    }
}
