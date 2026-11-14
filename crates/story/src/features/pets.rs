//! Animal pets that follow you, and connecting them into one pack.
//! From issue #15 by doomy ("surprise me").
//!
//! A few stray animals sit around the floor. Click one to adopt it (one pet
//! each); it then trots after your mouse pointer on the floor, since players
//! have no bodies. Pets standing close together are connected by a glowing
//! leash-line, and when every adopted pet is linked into one pack the whole
//! group has "connected the pets", shown on everyone's screen. Each animal
//! is a rigged knitted modeler model (`models/creatures/<animal>.gltf`,
//! the duck `pet_duck.gltf`)
//! playing `idle`, or `walk` while it trots.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, deny, in_reach, refuse};
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::rigged::{RigClip, Rigged};
use crate::terrain::PLAY_HALF;
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

    /// The rigged model (#15): knitted plush, `idle` and `walk` clips.
    pub fn model(self) -> &'static str {
        match self {
            Animal::Dog => "models/creatures/dog.gltf",
            Animal::Cat => "models/creatures/cat.gltf",
            Animal::Bunny => "models/creatures/bunny.gltf",
            Animal::Duck => "models/creatures/pet_duck.gltf",
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

/// Tuning numbers: `assets/data/story/pets.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Pets closer than this are connected.
    pub link_dist: f32,
    /// How fast a pet trots after its owner.
    pub speed: f32,
    /// Pets stop this short of the pointer.
    pub heel: f32,
    /// Where the dog, cat, bunny and duck wait, from the middle of their site (x, z).
    pub strays: Vec<(f32, f32)>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/story/pets.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

pub fn protocol(app: &mut App) {
    app.replicate::<Pet>()
        .add_client_event::<AdoptPet>(Channel::Ordered)
        .add_client_event::<PetLure>(Channel::Ordered)
        .init_resource::<Lures>()
        .add_observer(adopt)
        .add_observer(lure)
        .add_observer(release_on_leave)
        .add_observer(release_all)
        .add_systems(OnEnter(ServerState::Running), place_strays)
        .add_systems(Update, follow.run_if(in_state(ServerState::Running)));
}

/// Server only: where each player's pointer is.
#[derive(Resource, Default)]
struct Lures(HashMap<u64, Vec2>);

/// The strays, in the order `strays` in the data file places them.
const STRAYS: [Animal; 4] = [Animal::Dog, Animal::Cat, Animal::Bunny, Animal::Duck];

/// Where each stray waits: round the pets' site, as the data file spreads them.
pub fn stray_spots(middle: Vec2, t: &Tuning) -> Vec<(u32, Animal, Vec2)> {
    STRAYS
        .into_iter()
        .zip(&t.strays)
        .zip(1..)
        .map(|((animal, &(x, z)), id)| (id, animal, middle + Vec2::new(x, z)))
        .collect()
}

fn place_strays(mut commands: Commands) {
    for (id, animal, at) in stray_spots(crate::sites::at("pets"), &tuning()) {
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
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
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
    let Some(mut pet) = pets.iter_mut().find(|p| p.id == ev.id) else {
        return;
    };
    if pet.owner.is_some() {
        deny(&mut commands, ev.client_id, "that pet already has an owner");
        return;
    }
    let at = Vec3::new(pet.at.x, 0.0, pet.at.y);
    if in_reach(&mut commands, ev.client_id, &walkers, at) {
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
        lures.0.insert(
            me,
            ev.at.clamp(Vec2::splat(-PLAY_HALF), Vec2::splat(PLAY_HALF)),
        );
    }
}

/// A new round: every pet is a stray again (where it stands).
fn release_all(
    _reset: On<crate::rules::campaign::RoundReset>,
    mut lures: ResMut<Lures>,
    mut pets: Query<&mut Pet>,
) {
    lures.0.clear();
    for mut pet in &mut pets {
        pet.owner = None;
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
    if len <= tuning().heel {
        return from;
    }
    from + gap / len * (tuning().speed * dt).min(len - tuning().heel)
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
                if !seen[j] && points[i].distance(points[j]) < tuning().link_dist {
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
    app.add_observer(click_pet)
        .add_systems(OnEnter(AppState::InGame), spawn_pet_hud)
        .add_systems(
            Update,
            (
                dress_pets,
                move_pets,
                send_lure,
                draw_links,
                update_pet_hud.run_if(crate::world::hud_tick()),
            )
                .run_if(in_state(AppState::InGame)),
        );
}

fn dress_pets(mut commands: Commands, new: Query<(Entity, &Pet), Added<Pet>>) {
    for (entity, pet) in &new {
        commands.entity(entity).insert((
            Transform::from_translation(crate::terrain::on_ground(pet.at)),
            Clickable::at_height(0.3).labelled("adopt"),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
            Rigged {
                path: pet.animal.model(),
            },
            RigClip::looping("idle"),
        ));
    }
}

/// Glide to the replicated spot, trotting (the `walk` clip) while moving.
fn move_pets(time: Res<Time>, mut pets: Query<(&Pet, &mut Transform, &mut RigClip)>) {
    for (pet, mut tf, mut clip) in &mut pets {
        let here = Vec2::new(tf.translation.x, tf.translation.z);
        let gap = pet.at - here;
        let next = here + gap * (time.delta_secs() * 10.0).min(1.0);
        let moving = gap.length() > 0.02;
        tf.translation = crate::terrain::on_ground(next);
        if moving {
            tf.rotation = Quat::from_rotation_y((-gap.x).atan2(-gap.y));
        }
        let want = if moving { "walk" } else { "idle" };
        if clip.name != want {
            *clip = RigClip::looping(want);
        }
    }
}

fn floor_point(window: &Window, camera: &Camera, cam_tf: &GlobalTransform) -> Option<Vec2> {
    let cursor = window.cursor_position()?;
    // The camera may draw into a smaller picture (desktop "Fast" resolution).
    let cursor = cursor * camera.logical_viewport_size()? / window.size();
    let ray = camera.viewport_to_world(cam_tf, cursor).ok()?;
    let dist = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))?;
    let p = ray.get_point(dist);
    Some(Vec2::new(p.x, p.z))
}

/// Tell the server where our pointer is, a few times a second.
#[allow(clippy::too_many_arguments)]
fn send_lure(
    mut commands: Commands,
    time: Res<Time>,
    mut every: Local<f32>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &super::fps::Walker)>,
    duelists: Query<&super::duel::Duelist>,
) {
    *every -= time.delta_secs();
    if *every > 0.0 {
        return;
    }
    *every = 0.1;
    // With a body (#53), your pet follows you.
    if let Some(w) = super::fps::my_walker(me.as_deref(), &walkers, &duelists) {
        commands.client_trigger(PetLure { at: w.pos.xz() });
        return;
    }
    let Ok(window) = window.single() else { return };
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };
    if let Some(at) = floor_point(window, camera, cam_tf) {
        commands.client_trigger(PetLure { at });
    }
}

/// The unified click landed on a pet: adopt it if it's a stray and you have none.
fn click_pet(
    click: On<Clicked>,
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    pets: Query<&Pet>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let Ok(pet) = pets.get(click.entity) else {
        return;
    };
    let mine = me.is_some_and(|me| pets.iter().any(|p| p.owner == Some(me.0)));
    if pet.owner.is_some() {
        refuse(toast, "that pet already has an owner");
    } else if mine {
        refuse(toast, "you already have a pet");
    } else {
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
            if a.distance(*b) < tuning().link_dist {
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
        crate::hud::slot(crate::hud::HudSlot::Scoreboard),
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
    let mut lines = vec![crate::input::fill(
        "pets ({use} a stray to adopt; it follows your pointer)",
    )];
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
    fn each_stray_waits_at_its_spot_round_the_site() {
        let t = Tuning {
            strays: vec![(-4.0, 4.0), (3.0, 8.0), (-9.0, 2.0), (7.0, -2.0)],
            ..Default::default()
        };
        let spots = stray_spots(Vec2::new(10.0, 20.0), &t);
        assert_eq!(spots.len(), 4);
        assert_eq!(spots[0], (1, Animal::Dog, Vec2::new(6.0, 24.0)));
        assert_eq!(spots[3], (4, Animal::Duck, Vec2::new(17.0, 18.0)));
    }

    #[test]
    fn pets_trot_towards_the_pointer_and_stop_at_heel() {
        let p = trot(Vec2::ZERO, Vec2::new(10.0, 0.0), 0.5);
        assert!((p.x - tuning().speed * 0.5).abs() < 1e-4);
        let p = trot(Vec2::ZERO, Vec2::new(1.0, 0.0), 5.0);
        assert!(
            (p.x - (1.0 - tuning().heel)).abs() < 1e-4,
            "stops short: {p}"
        );
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
