//! Animal pets that follow you, and connecting them into one pack.
//! From issue #15 by doomy ("surprise me").
//!
//! A few stray animals sit around the floor. Click one to adopt it (one pet
//! each); it then trots after your mouse pointer on the floor, since players
//! have no bodies. Pets standing close together are connected by a glowing
//! leash-line, and when every adopted pet is linked into one pack the whole
//! group has "connected the pets", shown on everyone's screen.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::player::{Player, PlayerOwner};
use crate::world::MainCamera;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Animal {
    Dog,
    Cat,
    Bunny,
    Duck,
}

impl Animal {
    pub fn name(self) -> &'static str {
        match self {
            Animal::Dog => "dog",
            Animal::Cat => "cat",
            Animal::Bunny => "bunny",
            Animal::Duck => "duck",
        }
    }

    fn color(self) -> Color {
        match self {
            Animal::Dog => Color::srgb(0.7, 0.5, 0.3),
            Animal::Cat => Color::srgb(0.95, 0.6, 0.25),
            Animal::Bunny => Color::srgb(0.95, 0.95, 0.95),
            Animal::Duck => Color::srgb(1.0, 0.9, 0.2),
        }
    }
}

/// A pet at (x, z). `owner` is the `Player::id` it follows, if adopted.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Pet {
    pub id: u32,
    pub animal: Animal,
    pub at: Vec2,
    pub owner: Option<u64>,
}

/// Client -> server: "I adopt pet `id`."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct AdoptPet {
    pub id: u32,
}

/// Client -> server: "my pointer is at (x, z) on the floor; come here."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PetLure {
    pub at: Vec2,
}

/// Pets closer than this are connected.
pub const LINK_DIST: f32 = 2.5;
/// How fast a pet trots after its owner.
const SPEED: f32 = 4.0;
/// Pets stop this short of the pointer.
const HEEL: f32 = 0.8;

pub fn protocol(app: &mut App) {
    app.replicate::<Pet>()
        .add_client_event::<AdoptPet>(Channel::Ordered)
        .add_client_event::<PetLure>(Channel::Unreliable)
        .init_resource::<Lures>()
        .add_observer(adopt)
        .add_observer(lure)
        .add_observer(release_on_leave)
        .add_systems(OnEnter(ServerState::Running), place_strays)
        .add_systems(Update, follow.run_if(in_state(ServerState::Running)));
}

/// Server only: where each player's pointer is.
#[derive(Resource, Default)]
struct Lures(HashMap<u64, Vec2>);

fn place_strays(mut commands: Commands) {
    for (id, animal, at) in [
        (1, Animal::Dog, Vec2::new(-4.0, 4.0)),
        (2, Animal::Cat, Vec2::new(3.0, 8.0)),
        (3, Animal::Bunny, Vec2::new(-9.0, 2.0)),
        (4, Animal::Duck, Vec2::new(7.0, -2.0)),
    ] {
        commands.spawn((
            Pet {
                id,
                animal,
                at,
                owner: None,
            },
            Replicated,
        ));
    }
}

fn player_id(client: ClientId, players: &Query<(&Player, &PlayerOwner)>) -> Option<u64> {
    let owner = match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    players
        .iter()
        .find(|(_, o)| o.0 == owner)
        .map(|(p, _)| p.id)
}

/// Server: a stray goes to the first player without a pet who asks.
fn adopt(
    ev: On<FromClient<AdoptPet>>,
    still: Query<&crate::features::stillness::Stillness>,
    players: Query<(&Player, &PlayerOwner)>,
    mut pets: Query<&mut Pet>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Some(me) = player_id(ev.client_id, &players) else {
        return;
    };
    if pets.iter().any(|p| p.owner == Some(me)) {
        return;
    }
    if let Some(mut pet) = pets.iter_mut().find(|p| p.id == ev.id && p.owner.is_none()) {
        pet.owner = Some(me);
    }
}

fn lure(
    ev: On<FromClient<PetLure>>,
    still: Query<&crate::features::stillness::Stillness>,
    players: Query<(&Player, &PlayerOwner)>,
    mut lures: ResMut<Lures>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Some(me) = player_id(ev.client_id, &players) else {
        return;
    };
    if ev.at.is_finite() {
        lures
            .0
            .insert(me, ev.at.clamp(Vec2::splat(-24.0), Vec2::splat(24.0)));
    }
}

/// Server: a leaving player's pet becomes a stray again.
fn release_on_leave(
    remove: On<Remove<Player>>,
    server: Res<State<ServerState>>,
    players: Query<&Player>,
    mut lures: ResMut<Lures>,
    mut pets: Query<&mut Pet>,
) {
    if *server.get() != ServerState::Running {
        return;
    }
    let Ok(player) = players.get(remove.entity) else {
        return;
    };
    lures.0.remove(&player.id);
    for mut pet in &mut pets {
        if pet.owner == Some(player.id) {
            pet.owner = None;
        }
    }
}

/// Where a pet at `from` ends up after `dt` seconds of trotting to `target`.
pub fn trot(from: Vec2, target: Vec2, dt: f32) -> Vec2 {
    let gap = target - from;
    let len = gap.length();
    if len <= HEEL {
        return from;
    }
    from + gap / len * (SPEED * dt).min(len - HEEL)
}

fn follow(
    time: Res<Time>,
    lures: Res<Lures>,
    mut pets: Query<&mut Pet>,
    still: Query<&super::stillness::Stillness>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    for mut pet in &mut pets {
        let Some(target) = pet.owner.and_then(|o| lures.0.get(&o)) else {
            continue;
        };
        let next = trot(pet.at, *target, time.delta_secs());
        if next != pet.at {
            pet.at = next;
        }
    }
}

/// Size of the biggest group of points linked (directly or through others)
/// by gaps shorter than `LINK_DIST`.
pub fn biggest_pack(points: &[Vec2]) -> usize {
    let mut seen = vec![false; points.len()];
    let mut best = 0;
    for start in 0..points.len() {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![start];
        let mut size = 0;
        while let Some(i) = stack.pop() {
            size += 1;
            for j in 0..points.len() {
                if !seen[j] && points[i].distance(points[j]) < LINK_DIST {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        best = best.max(size);
    }
    best
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_pet_hud)
        .add_systems(
            Update,
            (
                dress_pets,
                move_pets,
                click_pet,
                send_lure,
                draw_links,
                update_pet_hud,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

fn dress_pets(
    mut commands: Commands,
    new: Query<(Entity, &Pet), Added<Pet>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, pet) in &new {
        let fur = crate::art::soft(&mut materials, pet.animal.color());
        let eye = crate::art::glossy(&mut materials, Color::srgb(0.1, 0.1, 0.1));
        let beak = crate::art::soft(&mut materials, Color::srgb(1.0, 0.55, 0.15));
        let eye_mesh = meshes.add(Sphere::new(0.04));
        commands
            .entity(entity)
            .insert((
                Transform::from_xyz(pet.at.x, 0.0, pet.at.y),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.25))),
                    MeshMaterial3d(fur.clone()),
                    Transform::from_xyz(0.0, 0.25, 0.0).with_scale(Vec3::new(1.0, 0.85, 1.3)),
                ));
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.17))),
                    MeshMaterial3d(fur.clone()),
                    Transform::from_xyz(0.0, 0.45, -0.3),
                ))
                .with_children(|h| {
                    crate::art::eyes(
                        h,
                        &mut meshes,
                        &mut materials,
                        Vec3::new(0.0, 0.04, -0.14),
                        0.14,
                        0.04,
                    );
                    // Each animal gets its own ears, nose or beak.
                    match pet.animal {
                        Animal::Bunny => {
                            let ear = meshes.add(Capsule3d::new(0.035, 0.22));
                            for x in [-0.06, 0.06] {
                                h.spawn((
                                    Mesh3d(ear.clone()),
                                    MeshMaterial3d(fur.clone()),
                                    Transform::from_xyz(x, 0.25, 0.02)
                                        .with_rotation(Quat::from_rotation_z(x * 2.5)),
                                ));
                            }
                        }
                        Animal::Cat => {
                            let ear = meshes.add(Cone::new(0.06, 0.12).mesh().resolution(4));
                            for x in [-0.09, 0.09] {
                                h.spawn((
                                    Mesh3d(ear.clone()),
                                    MeshMaterial3d(fur.clone()),
                                    Transform::from_xyz(x, 0.15, 0.0)
                                        .with_rotation(Quat::from_rotation_z(-x * 2.0)),
                                ));
                            }
                        }
                        Animal::Dog => {
                            let ear = meshes.add(Sphere::new(0.07));
                            for x in [-0.15, 0.15] {
                                h.spawn((
                                    Mesh3d(ear.clone()),
                                    MeshMaterial3d(eye.clone()),
                                    Transform::from_xyz(x, 0.02, 0.0)
                                        .with_scale(Vec3::new(0.5, 1.4, 0.9)),
                                ));
                            }
                            h.spawn((
                                Mesh3d(meshes.add(Sphere::new(0.08))),
                                MeshMaterial3d(fur.clone()),
                                Transform::from_xyz(0.0, -0.05, -0.15),
                            ));
                            h.spawn((
                                Mesh3d(eye_mesh.clone()),
                                MeshMaterial3d(eye.clone()),
                                Transform::from_xyz(0.0, -0.02, -0.23),
                            ));
                        }
                        Animal::Duck => {
                            h.spawn((
                                Mesh3d(meshes.add(Sphere::new(0.08))),
                                MeshMaterial3d(beak.clone()),
                                Transform::from_xyz(0.0, -0.03, -0.17)
                                    .with_scale(Vec3::new(1.2, 0.4, 1.4)),
                            ));
                        }
                    }
                });
                // A tail: a fluffy puff for the bunny, a tail feather for the duck.
                let (tail, tail_tf) = match pet.animal {
                    Animal::Bunny => (
                        meshes.add(Sphere::new(0.08)),
                        Transform::from_xyz(0.0, 0.3, 0.32),
                    ),
                    Animal::Duck => (
                        meshes.add(Cone::new(0.08, 0.14)),
                        Transform::from_xyz(0.0, 0.35, 0.33)
                            .with_rotation(Quat::from_rotation_x(0.9)),
                    ),
                    Animal::Dog | Animal::Cat => (
                        meshes.add(Capsule3d::new(0.03, 0.2)),
                        Transform::from_xyz(0.0, 0.4, 0.35)
                            .with_rotation(Quat::from_rotation_x(0.6)),
                    ),
                };
                p.spawn((Mesh3d(tail), MeshMaterial3d(fur.clone()), tail_tf));
                // Little feet (orange webbed ones for the duck).
                let foot = meshes.add(Sphere::new(0.06));
                let foot_mat = if pet.animal == Animal::Duck {
                    beak.clone()
                } else {
                    fur.clone()
                };
                for (x, z) in [(-0.1, -0.12), (0.1, -0.12), (-0.1, 0.15), (0.1, 0.15)] {
                    p.spawn((
                        Mesh3d(foot.clone()),
                        MeshMaterial3d(foot_mat.clone()),
                        Transform::from_xyz(x, 0.05, z).with_scale(Vec3::new(1.0, 0.6, 1.3)),
                    ));
                }
            });
    }
}

/// Glide to the replicated spot with a little hop while moving.
fn move_pets(time: Res<Time>, mut pets: Query<(&Pet, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (pet, mut tf) in &mut pets {
        let here = Vec2::new(tf.translation.x, tf.translation.z);
        let gap = pet.at - here;
        let next = here + gap * (time.delta_secs() * 10.0).min(1.0);
        let moving = gap.length() > 0.02;
        let hop = if moving {
            (t * 12.0 + pet.id as f32).sin().abs() * 0.12
        } else {
            0.0
        };
        tf.translation = Vec3::new(next.x, hop, next.y);
        if moving {
            tf.rotation = Quat::from_rotation_y((-gap.x).atan2(-gap.y));
        }
    }
}

fn floor_point(window: &Window, camera: &Camera, cam_tf: &GlobalTransform) -> Option<Vec2> {
    let cursor = window.cursor_position()?;
    let ray = camera.viewport_to_world(cam_tf, cursor).ok()?;
    let dist = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    let p = ray.get_point(dist);
    Some(Vec2::new(p.x, p.z))
}

/// Tell the server where our pointer is, a few times a second.
fn send_lure(
    mut commands: Commands,
    time: Res<Time>,
    mut every: Local<f32>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    *every -= time.delta_secs();
    if *every > 0.0 {
        return;
    }
    *every = 0.1;
    let Ok(window) = window.single() else { return };
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };
    if let Some(at) = floor_point(window, camera, cam_tf) {
        commands.client_trigger(PetLure { at });
    }
}

const CLICK_RADIUS: f32 = 40.0;

fn click_pet(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    pets: Query<(&Pet, &Transform)>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = window.single() else { return };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };
    let hit = pets.iter().find(|(p, tf)| {
        p.owner.is_none()
            && camera
                .world_to_viewport(cam_tf, tf.translation + Vec3::Y * 0.3)
                .is_ok_and(|s| s.distance(cursor) < CLICK_RADIUS)
    });
    if let Some((pet, _)) = hit {
        commands.client_trigger(AdoptPet { id: pet.id });
    }
}

/// Glowing leash-lines between adopted pets standing close together.
fn draw_links(mut gizmos: Gizmos, pets: Query<(&Pet, &Transform)>) {
    let owned: Vec<Vec3> = pets
        .iter()
        .filter(|(p, _)| p.owner.is_some())
        .map(|(_, tf)| tf.translation + Vec3::Y * 0.3)
        .collect();
    for (i, a) in owned.iter().enumerate() {
        for b in &owned[i + 1..] {
            if a.distance(*b) < LINK_DIST {
                gizmos.line(*a, *b, Color::srgb(1.0, 0.5, 0.9));
            }
        }
    }
}

#[derive(Component)]
struct PetHud;

fn spawn_pet_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        PetHud,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(48),
            right: px(12),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_pet_hud(pets: Query<&Pet>, mut hud: Query<&mut Text, With<PetHud>>) {
    let mut all: Vec<&Pet> = pets.iter().collect();
    all.sort_by_key(|p| p.id);
    let owned: Vec<Vec2> = all
        .iter()
        .filter(|p| p.owner.is_some())
        .map(|p| p.at)
        .collect();
    let mut lines = vec!["pets (click a stray to adopt; it follows your pointer)".to_string()];
    for p in &all {
        lines.push(match p.owner {
            Some(o) => format!("{}: player {o}'s pet", p.animal.name()),
            None => format!("{}: stray", p.animal.name()),
        });
    }
    let pack = biggest_pack(&owned);
    lines.push(if owned.len() >= 2 && pack == owned.len() {
        format!("ALL {pack} PETS CONNECTED! what a pack")
    } else {
        format!(
            "pets connected: {pack} of {} (bring them together!)",
            owned.len()
        )
    });
    let want = lines.join("\n");
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
    fn pets_trot_towards_the_pointer_and_stop_at_heel() {
        let p = trot(Vec2::ZERO, Vec2::new(10.0, 0.0), 0.5);
        assert!((p.x - SPEED * 0.5).abs() < 1e-4);
        let p = trot(Vec2::ZERO, Vec2::new(1.0, 0.0), 5.0);
        assert!((p.x - (1.0 - HEEL)).abs() < 1e-4, "stops short: {p}");
        assert_eq!(trot(Vec2::ZERO, Vec2::new(0.1, 0.0), 1.0), Vec2::ZERO);
    }

    #[test]
    fn packs_link_through_each_other() {
        assert_eq!(biggest_pack(&[]), 0);
        let chain = [Vec2::ZERO, Vec2::new(2.0, 0.0), Vec2::new(4.0, 0.0)];
        assert_eq!(biggest_pack(&chain), 3, "linked through the middle one");
        let apart = [Vec2::ZERO, Vec2::new(2.0, 0.0), Vec2::new(10.0, 0.0)];
        assert_eq!(biggest_pack(&apart), 2);
    }
}
