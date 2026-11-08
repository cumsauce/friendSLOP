//! Cute crows that caw occasionally. From issue #2 by corvy.
//!
//! The server spawns a few replicated `Crow`s. Their hopping and cawing is
//! pure decoration, so every peer animates them locally from the crow's id
//! (no per-frame network traffic). A "caw" is a little puff-up, a line of
//! text at the bottom of the screen and a caw sound (from `assets/audio`).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::assets::{Sfx, play_sfx};

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

/// Server only: scatter the crows around the middle of the floor.
fn spawn_crows(mut commands: Commands) {
    for id in 0..CROW_COUNT {
        let angle = id as f32 / CROW_COUNT as f32 * std::f32::consts::TAU;
        let home = Vec2::from_angle(angle) * 4.0;
        commands.spawn((Crow { id, home }, Replicated));
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_caw_text)
        .add_systems(
            Update,
            (dress_crows, animate_crows, update_caw_text, caw_sound)
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

/// Give every new crow (on every peer) a body, head and beak.
fn dress_crows(
    mut commands: Commands,
    new: Query<(Entity, &Crow), Added<Crow>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, crow) in &new {
        let feathers = materials.add(Color::srgb(0.08, 0.08, 0.12));
        let beak = materials.add(Color::srgb(0.95, 0.7, 0.2));
        let body_mesh = meshes.add(Sphere::new(0.3));
        let head_mesh = meshes.add(Sphere::new(0.2));
        let beak_mesh = meshes.add(Cone::new(0.06, 0.2).mesh().resolution(6));
        commands
            .entity(entity)
            .insert((
                Transform::from_translation(crow_position(crow, 0.0)),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(body_mesh),
                    MeshMaterial3d(feathers.clone()),
                    Transform::from_scale(Vec3::new(1.0, 0.9, 1.3)),
                ));
                p.spawn((
                    Mesh3d(head_mesh),
                    MeshMaterial3d(feathers.clone()),
                    Transform::from_xyz(0.0, 0.3, -0.3),
                ))
                .with_children(|h| {
                    h.spawn((
                        Mesh3d(beak_mesh),
                        MeshMaterial3d(beak.clone()),
                        Transform::from_xyz(0.0, -0.02, -0.22)
                            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
                    ));
                    crate::art::eyes(
                        h,
                        &mut meshes,
                        &mut materials,
                        Vec3::new(0.0, 0.06, -0.15),
                        0.16,
                        0.045,
                    );
                });
                // Folded wings, a fanned tail and two twiggy legs.
                let wing = meshes.add(Sphere::new(0.22));
                for x in [-0.24, 0.24] {
                    p.spawn((
                        Mesh3d(wing.clone()),
                        MeshMaterial3d(feathers.clone()),
                        Transform::from_xyz(x, 0.05, 0.05)
                            .with_rotation(Quat::from_rotation_z(x * 1.2))
                            .with_scale(Vec3::new(0.35, 0.8, 1.3)),
                    ));
                }
                p.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.22, 0.04, 0.3))),
                    MeshMaterial3d(feathers.clone()),
                    Transform::from_xyz(0.0, 0.08, 0.45).with_rotation(Quat::from_rotation_x(-0.4)),
                ));
                let leg = meshes.add(Cylinder::new(0.018, 0.22));
                for x in [-0.08, 0.08] {
                    p.spawn((
                        Mesh3d(leg.clone()),
                        MeshMaterial3d(beak.clone()),
                        Transform::from_xyz(x, -0.3, 0.0),
                    ));
                }
            });
    }
}

/// Hop around, face where we're going, and puff up when cawing.
fn animate_crows(time: Res<Time>, mut crows: Query<(&Crow, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (crow, mut transform) in &mut crows {
        let here = crow_position(crow, t);
        let ahead = crow_position(crow, t + 0.1);
        let look = Vec3::new(ahead.x - here.x, 0.0, ahead.z - here.z);
        if look.length_squared() > 1e-6 {
            transform.look_to(look, Vec3::Y);
        }
        transform.translation = here;
        transform.scale = Vec3::splat(if is_cawing(crow.id, t) { 1.3 } else { 1.0 });
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
    crows: Query<&Crow>,
    mut text: Query<&mut Text, With<CawText>>,
) {
    let t = time.elapsed_secs();
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
    crows: Query<&Crow>,
    sfx: Option<Res<Sfx>>,
    mut was_cawing: Local<[bool; CROW_COUNT as usize]>,
) {
    let Some(sfx) = sfx else { return };
    let t = time.elapsed_secs();
    for crow in &crows {
        let cawing = is_cawing(crow.id, t);
        let slot = crow.id as usize % was_cawing.len();
        if cawing && !was_cawing[slot] {
            play_sfx(&mut commands, &sfx.crow_caw, 0.7);
        }
        was_cawing[slot] = cawing;
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
