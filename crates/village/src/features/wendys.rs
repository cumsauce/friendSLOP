//! Wendy's: a roadside burger restaurant with human meat burgers to pick up.
//! From issue #117 by Kori.exe.
//!
//! The restaurant stands in the meadow beside the road to the east village
//! (local scenery, same on every peer). Walk in through the front door: a
//! dining room, an order counter across the middle, a kitchen behind it, a
//! butcher's backroom behind the kitchen and stairs up to an attic under the
//! roof. The server keeps up to [`SLOTS`] replicated [`Burger`]s on the
//! counter. Click one to pick it up:
//! the server checks you're in reach and adds it to your replicated
//! [`BurgerBag`]; a fresh burger appears in that spot [`RESTOCK_SECS`] later.
//! Carrying burgers doesn't do anything else yet.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, in_reach};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Where the shack stands (x, z). Its counter faces north (-z), towards the road.
pub use friendslop_world::places::wendys_at;
/// Burgers on the counter at once.
pub const SLOTS: u8 = 3;
/// Tuning numbers: `assets/data/village/wendys.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Seconds until a taken burger is replaced.
    pub restock_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/village/wendys.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The building's footprint around its spot (x, z): half width, and how far
/// it reaches north (the front door, towards the road) and south (the
/// butcher's backroom). Matches the modeler model `sp-wendys`.
const HALF_W: f32 = 6.15;
const FRONT: f32 = 4.65;
const BACK: f32 = 8.85;
/// The order counter: top height above the floor centre and where it runs (z).
const COUNTER_H: f32 = 1.06;
const COUNTER_Z: f32 = 0.8;

/// A human meat burger waiting on the counter, in spot `slot`.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Burger {
    pub slot: u8,
}

/// How many human meat burgers this player has picked up.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BurgerBag(pub u32);

/// Client -> server: "I'm taking the burger in this spot."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TakeBurger {
    pub slot: u8,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Burger>()
        .replicate::<BurgerBag>()
        .add_client_event::<TakeBurger>(Channel::Ordered)
        .init_resource::<Kitchen>()
        .add_observer(give_bag)
        .add_observer(take_burger)
        .add_systems(OnEnter(ServerState::Running), open_up)
        .add_systems(OnExit(ServerState::Running), close_up)
        .add_systems(Update, restock.run_if(in_state(ServerState::Running)));
}

/// Server only: which spots are empty and how long until they're refilled.
#[derive(Resource, Default)]
struct Kitchen {
    restock: [Option<f32>; SLOTS as usize],
}

/// The floor level of the shack: the highest ground under its footprint, so
/// it never sinks into a slope.
pub fn floor_y() -> f32 {
    let w = HALF_W + 0.5;
    let (f, b) = (-FRONT - 0.5, BACK + 0.5);
    [
        (-w, f),
        (w, f),
        (-w, b),
        (w, b),
        (0.0, f),
        (0.0, b),
        (0.0, 0.0),
    ]
    .iter()
    .map(|(x, z)| crate::terrain::height_at(wendys_at().x + x, wendys_at().y + z))
    .fold(0.0, f32::max)
}

/// Where the burger in `slot` sits (on top of the order counter inside).
pub fn burger_spot(slot: u8) -> Vec3 {
    let x = (slot as f32 - (SLOTS as f32 - 1.0) / 2.0) * 0.9;
    Vec3::new(
        wendys_at().x + x,
        floor_y() + COUNTER_H,
        wendys_at().y + COUNTER_Z,
    )
}

/// Is `p` (x, z) on the building's ground, with `margin` to spare? Scenery
/// keeps its trees and rocks off it.
pub fn covers(p: Vec2, margin: f32) -> bool {
    let d = p - wendys_at();
    d.x.abs() < HALF_W + 0.4 + margin && d.y > -FRONT - 1.2 - margin && d.y < BACK + 0.3 + margin
}

fn give_bag(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() == ServerState::Running {
        commands.entity(add.entity).insert(BurgerBag::default());
    }
}

fn open_up(mut commands: Commands, mut kitchen: ResMut<Kitchen>) {
    *kitchen = Kitchen::default();
    for slot in 0..SLOTS {
        commands.spawn((Burger { slot }, Replicated));
    }
}

fn close_up(
    mut commands: Commands,
    mut kitchen: ResMut<Kitchen>,
    burgers: Query<Entity, With<Burger>>,
) {
    for e in &burgers {
        commands.entity(e).despawn();
    }
    *kitchen = Kitchen::default();
}

/// Server: refill empty spots (the kitchen stops in the stillness too).
fn restock(
    time: Res<Time>,
    mut commands: Commands,
    mut kitchen: ResMut<Kitchen>,
    still: Query<&super::stillness::Stillness>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    for (slot, timer) in kitchen.restock.iter_mut().enumerate() {
        let Some(left) = timer.as_mut() else { continue };
        *left -= time.delta_secs();
        if *left <= 0.0 {
            *timer = None;
            commands.spawn((Burger { slot: slot as u8 }, Replicated));
        }
    }
}

/// Server: whoever asks first gets the burger.
#[allow(clippy::too_many_arguments)]
fn take_burger(
    take: On<FromClient<TakeBurger>>,
    mut commands: Commands,
    mut kitchen: ResMut<Kitchen>,
    burgers: Query<(Entity, &Burger)>,
    mut bags: Query<(&PlayerOwner, &mut BurgerBag)>,
    walkers: Query<(&PlayerOwner, &Walker)>,
    still: Query<&super::stillness::Stillness>,
    mut fate: super::fate::Fate,
    odds: Query<&crate::rules::house_odds::HouseOdds>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let slot = take.slot;
    // A pending restock means this spot was already emptied (maybe earlier
    // this tick, with the despawn still queued).
    let Some(timer) = kitchen.restock.get_mut(slot as usize) else {
        return;
    };
    if timer.is_some() {
        return;
    }
    let Some((burger, _)) = burgers.iter().find(|(_, b)| b.slot == slot) else {
        return;
    };
    if !in_reach(&mut commands, take.client_id, &walkers, burger_spot(slot)) {
        return;
    }
    if fate.fails(take.client_id) {
        return;
    }
    let owner = match take.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((_, mut bag)) = bags.iter_mut().find(|(o, _)| o.0 == owner) else {
        return;
    };
    bag.0 += 1;
    commands.entity(burger).despawn();
    *timer = Some(crate::rules::house_odds::odds(&odds).restock_secs());
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, music_signals.in_set(crate::music::MusicSignalSet));
    app.add_observer(click_burger)
        .add_systems(OnEnter(AppState::InGame), (build_restaurant, spawn_ui))
        .add_systems(
            Update,
            (dress_burgers, show_bag).run_if(in_state(AppState::InGame)),
        );
}

/// Local: the unified click landed on a burger.
fn click_burger(click: On<Clicked>, mut commands: Commands, burgers: Query<&Burger>) {
    if let Ok(b) = burgers.get(click.entity) {
        commands.client_trigger(TakeBurger { slot: b.slot });
    }
}

#[derive(Component)]
struct BagText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        BagText,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(
            crate::input::fill("Wendy's (by the road to the village): walk in and {use} a human meat burger on the counter to pick it up"),
        ),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Show your own burger count (and say so when it goes up).
fn show_bag(
    me: Option<Res<LocalPlayer>>,
    bags: Query<(&Player, &BurgerBag)>,
    mut text: Query<&mut Text, With<BagText>>,
    toast: Option<ResMut<Toast>>,
    mut last: Local<Option<u32>>,
) {
    let Some(me) = me else { return };
    let n = bags
        .iter()
        .find(|(p, _)| p.id == me.0)
        .map_or(0, |(_, b)| b.0);
    if *last == Some(n) {
        return;
    }
    if last.is_some_and(|l| n > l)
        && let Some(mut toast) = toast
    {
        toast.show("you picked up a human meat burger");
    }
    *last = Some(n);
    let want = if n == 0 {
        String::new()
    } else {
        format!("human meat burgers: {n}")
    };
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

/// Bun, lettuce, a pinkish patty and cheese, sat on the counter.
fn dress_burgers(
    mut commands: Commands,
    new: Query<(Entity, &Burger), Added<Burger>>,
    assets: Res<AssetServer>,
) {
    for (entity, burger) in &new {
        commands
            .entity(entity)
            .insert((
                Transform::from_translation(burger_spot(burger.slot)),
                Clickable::at_height(0.1)
                    .radius(30.0)
                    .labelled("take the burger"),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                // Modeler-made (sources in `.modeler/`, prefab `sp-burger`).
                p.spawn((model(&assets, "burger.gltf"), Transform::default()));
            });
    }
}

/// A modeler-made feature model in `assets/models/features`.
fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

/// Invisible blockers inside the model (centre and full size in metres, from
/// the floor centre; x across, z towards the back). Same boxes as the parts
/// of `sp-wendys`.
const SOLIDS: &[([f32; 3], [f32; 3])] = &[
    // Front wall either side of the door (the windows are solid), and the
    // wall above the door.
    ([-3.55, 1.7, -4.5], [5.1, 3.4, 0.2]),
    ([3.55, 1.7, -4.5], [5.1, 3.4, 0.2]),
    ([0.0, 2.9, -4.5], [2.0, 1.0, 0.2]),
    // Side walls and the back wall of the backroom.
    ([-6.0, 1.7, 2.1], [0.2, 3.4, 13.0]),
    ([6.0, 1.7, 2.1], [0.2, 3.4, 13.0]),
    ([0.0, 1.7, 8.7], [12.2, 3.4, 0.2]),
    // Kitchen back wall with the doorway into the butcher's backroom.
    ([-1.65, 1.7, 4.5], [8.9, 3.4, 0.2]),
    ([5.25, 1.7, 4.5], [1.7, 3.4, 0.2]),
    ([3.6, 2.9, 4.5], [1.6, 1.0, 0.2]),
    // The order counter (the gap at its +x end leads into the kitchen).
    ([-1.25, 0.53, 0.8], [9.3, 1.06, 0.72]),
    // Kitchen: grill, fryer, prep counter, sink, fridge, prep island.
    ([-3.6, 0.475, 4.0], [2.4, 0.95, 0.8]),
    ([-1.7, 0.475, 4.0], [1.0, 0.95, 0.8]),
    ([0.3, 0.45, 4.05], [1.8, 0.9, 0.7]),
    ([2.0, 0.45, 4.05], [1.2, 0.9, 0.7]),
    ([5.2, 1.05, 3.9], [1.1, 2.1, 0.8]),
    ([-1.5, 0.465, 2.4], [3.0, 0.93, 0.8]),
    // Dining tables.
    ([-3.6, 0.39, -3.0], [0.9, 0.78, 0.9]),
    ([-3.6, 0.39, -1.2], [0.9, 0.78, 0.9]),
    ([3.6, 0.39, -3.0], [0.9, 0.78, 0.9]),
    ([3.6, 0.39, -1.2], [0.9, 0.78, 0.9]),
    // Butcher's backroom: chopping block and butcher table.
    ([-3.0, 0.475, 6.0], [1.0, 0.95, 0.8]),
    ([-5.45, 0.5, 6.2], [0.8, 1.0, 2.4]),
    // Attic floor (the ceiling), around the stairwell.
    ([0.0, 3.5, 1.375], [12.2, 0.2, 11.95]),
    ([-3.075, 3.5, 8.075], [6.05, 0.2, 1.45]),
    ([4.7, 3.5, 8.075], [2.8, 0.2, 1.45]),
    // Railing round the stairwell up in the attic.
    ([1.625, 4.1, 7.3], [3.4, 1.0, 0.05]),
    ([3.32, 4.1, 7.95], [0.05, 1.0, 1.3]),
    // Gable ends, stepped under the roof slope.
    ([0.0, 4.2, -4.5], [12.2, 1.2, 0.2]),
    ([0.0, 5.4, -4.5], [8.3, 1.2, 0.2]),
    ([0.0, 6.6, -4.5], [4.2, 1.2, 0.2]),
    ([0.0, 4.2, 8.7], [12.2, 1.2, 0.2]),
    ([0.0, 5.4, 8.7], [8.3, 1.2, 0.2]),
    ([0.0, 6.6, 8.7], [4.2, 1.2, 0.2]),
    // Dining: condiment station and the potted plants by the door.
    ([5.62, 0.56, -2.1], [0.56, 1.12, 1.06]),
    ([-5.5, 0.45, -3.95], [0.45, 0.9, 0.45]),
    ([5.5, 0.45, -3.95], [0.45, 0.9, 0.45]),
    // Backroom: chest freezer and the scrap bins.
    ([1.05, 0.475, 5.0], [1.4, 0.95, 0.7]),
    ([-5.2, 0.45, 8.15], [1.2, 0.8, 0.7]),
    // Attic: crates, the chest and the dust-sheeted furniture.
    ([1.6, 4.15, -2.8], [0.7, 1.1, 0.7]),
    ([-1.8, 3.9, 1.0], [0.7, 0.6, 0.7]),
    ([-1.9, 3.9, -3.5], [0.6, 0.6, 0.6]),
    ([2.2, 4.05, 2.0], [0.5, 0.9, 0.5]),
    ([-2.2, 3.85, -0.2], [0.5, 0.5, 0.5]),
    ([-2.0, 3.8, 3.0], [0.4, 0.4, 0.4]),
    ([2.0, 3.85, 0.5], [0.8, 0.5, 0.5]),
    ([1.8, 4.05, 4.0], [0.8, 0.9, 0.8]),
];

/// Sloped blockers: centre, full size and tilt about z (degrees). The stairs
/// are one ramp from the backroom floor (east) up to the attic (west); the
/// roof is two slopes meeting at the ridge.
const SLOPES: &[([f32; 3], [f32; 3], f32)] = &[
    ([2.747, 1.765, 8.0], [6.6, 0.2, 1.2], -32.0),
    ([3.2, 5.42, 2.1], [7.39, 0.15, 13.9], -30.0),
    ([-3.2, 5.42, 2.1], [7.39, 0.15, 13.9], 30.0),
];

/// Ceiling lights (from the floor centre), colour and range: dining room,
/// kitchen, butcher's backroom, attic.
const LIGHTS: &[([f32; 3], [f32; 3], f32)] = &[
    ([0.0, 3.0, -2.0], [1.0, 0.85, 0.65], 9.0),
    ([0.0, 3.0, 2.8], [1.0, 0.8, 0.55], 7.0),
    ([-2.0, 3.0, 6.6], [1.0, 0.6, 0.5], 7.0),
    ([0.0, 6.5, 2.1], [1.0, 0.75, 0.5], 8.0),
];

/// Depth of each stone step down from the door.
const STEP_RUN: f32 = 0.4;

/// The stone steps from the door down to ground `rise` metres below the
/// floor: each step's top (relative to the floor centre's base) and its
/// centre z, from the door outward, none higher than a walker steps up.
fn door_steps(rise: f32) -> Vec<(f32, f32)> {
    if rise < 0.15 {
        return Vec::new();
    }
    let n = (rise / 0.3).ceil() as usize - 1;
    let r = rise / (n as f32 + 1.0);
    (0..n)
        .map(|k| {
            let top = 0.1 - (k as f32 + 1.0) * r;
            (top, -FRONT - STEP_RUN * (k as f32 + 0.5))
        })
        .collect()
}

/// The restaurant: a modeler-made building (prefab `sp-wendys`) on a stone
/// base that follows the slope, its lights, and invisible blockers where its
/// walls, counters, stairs and roof are. Local scenery, the same on every peer.
fn build_restaurant(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    use avian3d::prelude::{Collider, RigidBody};
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let stone = crate::art::soft(&mut materials, Color::srgb(0.58, 0.56, 0.54));

    let base = floor_y();
    let c = Vec3::new(wendys_at().x, base, wendys_at().y);
    let low = [(-1.0, -FRONT), (1.0, -FRONT), (-1.0, BACK), (1.0, BACK)]
        .iter()
        .map(|(x, z)| crate::terrain::height_at(wendys_at().x + x * HALF_W, wendys_at().y + z))
        .fold(base, f32::min);
    commands.spawn((
        model(&assets, "wendys.gltf"),
        Transform::from_translation(c),
        DespawnOnExit(AppState::InGame),
    ));
    // The front door (walk in southwards, +z) and the foot of the attic
    // stairs in the backroom (they climb westwards, -x).
    {
        use crate::features::doorway::{Doorway, Way, spawn, yaw_towards};
        spawn(
            &mut commands,
            c + Vec3::new(0.0, 0.1, -FRONT - 0.15),
            yaw_towards(Vec2::Y),
            Doorway::way_in("Wendy's").size(1.6, 2.4),
        );
        spawn(
            &mut commands,
            c + Vec3::new(5.4, 0.1, 8.0),
            yaw_towards(-Vec2::X),
            Doorway::new(Way::Up, "the attic").size(1.2, 2.4),
        );
    }
    for &(at, color, range) in LIGHTS {
        commands.spawn((
            PointLight {
                color: Color::srgb(color[0], color[1], color[2]),
                intensity: 60_000.0,
                range,
                ..default()
            },
            Transform::from_translation(c + Vec3::from(at)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // The stone base under the floor follows the slope, so it stays built
    // here; it's what you walk on, its top a hair under the model's tiled
    // floor so the two don't flicker.
    let depth = base - low + 0.3;
    let mid_z = (BACK - FRONT) / 2.0;
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(stone.clone()),
        Transform::from_translation(c + Vec3::new(0.0, -depth / 2.0 + 0.04, mid_z))
            .with_scale(Vec3::new(HALF_W * 2.0, depth + 0.1, FRONT + BACK)),
        RigidBody::Static,
        Collider::cuboid(1.0, 1.0, 1.0),
        DespawnOnExit(AppState::InGame),
    ));
    // Stone steps down from the door when the ground there is lower.
    let door_ground = crate::terrain::height_at(wendys_at().x, wendys_at().y - FRONT - 0.6);
    for (top, z) in door_steps(base + 0.1 - door_ground) {
        let h = top + 0.3;
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(stone.clone()),
            Transform::from_translation(c + Vec3::new(0.0, top - h / 2.0, z))
                .with_scale(Vec3::new(2.4, h, STEP_RUN)),
            RigidBody::Static,
            Collider::cuboid(1.0, 1.0, 1.0),
            DespawnOnExit(AppState::InGame),
        ));
    }
    for &(at, size) in SOLIDS {
        commands.spawn((
            Transform::from_translation(c + Vec3::from(at)).with_scale(Vec3::from(size)),
            RigidBody::Static,
            Collider::cuboid(1.0, 1.0, 1.0),
            DespawnOnExit(AppState::InGame),
        ));
    }
    for &(at, size, tilt) in SLOPES {
        commands.spawn((
            Transform::from_translation(c + Vec3::from(at))
                .with_rotation(Quat::from_rotation_z(tilt.to_radians())),
            RigidBody::Static,
            Collider::cuboid(size[0], size[1], size[2]),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// For the music: the camera's inside Wendy's, and up in its attic (whose
/// floor is 3.6 m up: the eye is past 4.2 m there).
fn music_signals(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let inside = crate::music::camera_at(&camera).and_then(music_place);
    signals.flag("wendys", inside.is_some());
    signals.flag("wendys_attic", inside == Some(true));
}

/// Is `at` inside Wendy's (`Some`), and up in the attic (`Some(true)`)?
/// (The `wendys` and `wendys_attic` music signals.)
pub fn music_place(at: Vec3) -> Option<bool> {
    (covers(at.xz(), 0.0) && at.y < floor_y() + 9.0).then_some(at.y > floor_y() + 4.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burgers_sit_on_the_counter_inside() {
        for slot in 0..SLOTS {
            let p = burger_spot(slot);
            let d = Vec2::new(p.x, p.z) - wendys_at();
            assert!(d.x.abs() < HALF_W);
            assert!(d.y > -FRONT && d.y < BACK, "inside the building");
            assert!(p.y >= floor_y() + 1.0);
            // Every burger is over the counter's blocker.
            let (at, size) = SOLIDS[9];
            assert!((d.x - at[0]).abs() < size[0] / 2.0);
            assert!((d.y - at[2]).abs() < size[2] / 2.0);
        }
    }

    #[test]
    fn the_restaurant_is_in_the_open_meadow_off_the_road() {
        use crate::zones::{Zone, zone_of};
        assert_eq!(zone_of(wendys_at()), Zone::Meadow);
        for dx in [-HALF_W, 0.0, HALF_W] {
            for dz in [-FRONT, 0.0, BACK / 2.0, BACK] {
                let (x, z) = (wendys_at().x + dx, wendys_at().y + dz);
                assert!(
                    !crate::terrain::is_path(x, z),
                    "restaurant on the road at {x},{z}"
                );
                assert!(!crate::terrain::is_pond(x, z));
                assert!(covers(Vec2::new(x, z), 0.0));
            }
        }
    }

    #[test]
    fn door_steps_are_low_enough_to_walk_up() {
        assert!(door_steps(0.1).is_empty());
        for rise in [0.2, 0.45, 0.73, 1.5] {
            let steps = door_steps(rise);
            let mut last = 0.1;
            for (top, _) in &steps {
                assert!(last - top <= 0.31 && last > *top);
                last = *top;
            }
            // The last step is a step above the ground.
            assert!(last - (0.1 - rise) <= 0.31, "{rise}: {steps:?}");
        }
    }

    #[test]
    fn the_stair_ramp_runs_from_the_backroom_floor_to_the_attic() {
        let (at, size, tilt) = SLOPES[0];
        let (s, c) = tilt.to_radians().sin_cos();
        // The top face's two ends along the ramp.
        let half = size[0] / 2.0;
        let up = Vec2::new(-s, c) * size[1] / 2.0;
        let a = Vec2::new(at[0], at[1]) + Vec2::new(c, s) * half + up;
        let b = Vec2::new(at[0], at[1]) - Vec2::new(c, s) * half + up;
        assert!((a.y - 0.1).abs() < 0.05, "bottom on the floor: {a}");
        assert!((b.y - 3.6).abs() < 0.05, "top at the attic floor: {b}");
        assert!(a.x > 5.0 && b.x.abs() < 0.2);
    }
}
