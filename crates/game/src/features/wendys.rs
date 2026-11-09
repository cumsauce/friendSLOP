//! Wendy's: a roadside burger shack with human meat burgers to pick up.
//! From issue #117 by Kori.exe.
//!
//! The shack stands in the meadow beside the road to the east village
//! (local scenery, same on every peer). The server keeps up to
//! [`SLOTS`] replicated [`Burger`]s on its counter. Click one to pick it up:
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
pub const WENDYS_AT: Vec2 = Vec2::new(30.0, 9.0);
/// Burgers on the counter at once.
pub const SLOTS: u8 = 3;
/// Seconds until a taken burger is replaced.
pub const RESTOCK_SECS: f32 = 15.0;

/// Shack size: half width (x), half depth (z), wall height.
const HALF_W: f32 = 2.6;
const HALF_D: f32 = 1.8;
const WALL_H: f32 = 2.8;
/// The counter: top height and how far it sticks out in front.
const COUNTER_H: f32 = 1.05;
const COUNTER_Z: f32 = HALF_D + 0.35;

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
    let (w, d) = (HALF_W + 0.5, COUNTER_Z + 0.5);
    [(-w, -d), (w, -d), (-w, d), (w, d), (0.0, 0.0)]
        .iter()
        .map(|(x, z)| crate::terrain::height_at(WENDYS_AT.x + x, WENDYS_AT.y + z))
        .fold(0.0, f32::max)
}

/// Where the burger in `slot` sits (on top of the counter).
pub fn burger_spot(slot: u8) -> Vec3 {
    let x = (slot as f32 - (SLOTS as f32 - 1.0) / 2.0) * 0.9;
    // The counter is on the north (-z) side, facing the road.
    Vec3::new(
        WENDYS_AT.x + x,
        floor_y() + COUNTER_H,
        WENDYS_AT.y - COUNTER_Z,
    )
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
    odds: Query<&super::gamble::HouseOdds>,
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
    *timer = Some(crate::features::gamble::odds(&odds).restock_secs());
}

pub fn plugin(app: &mut App) {
    app.add_observer(click_burger)
        .add_systems(OnEnter(AppState::InGame), (build_shack, spawn_ui))
        .add_systems(
            Update,
            (dress_burgers, show_bag, place_sign).run_if(in_state(AppState::InGame)),
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

#[derive(Component)]
struct SignLabel;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        BagText,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(
            "Wendy's (by the road to the village): click a human meat burger on the counter to pick it up",
        ),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        SignLabel,
        Text::new("WENDY'S\nhuman meat burgers"),
        TextFont::from_font_size(15.0),
        TextColor(Color::srgb(1.0, 0.95, 0.85)),
        TextShadow::default(),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        Visibility::Hidden,
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

/// Float the shop's name over its sign when it's in view and not too far.
fn place_sign(
    camera: Query<(&Camera, &GlobalTransform), With<crate::world::MainCamera>>,
    mut label: Query<(&mut Node, &mut Visibility), With<SignLabel>>,
) {
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };
    let at = Vec3::new(WENDYS_AT.x, floor_y() + WALL_H + 1.2, WENDYS_AT.y);
    let near = cam_tf.translation().distance(at) < 45.0;
    let screen = camera.world_to_viewport(cam_tf, at).ok().filter(|_| near);
    for (mut node, mut vis) in &mut label {
        vis.set_if_neq(if screen.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if let Some(p) = screen
            && (node.left != Val::Px(p.x - 70.0) || node.top != Val::Px(p.y - 20.0))
        {
            node.left = Val::Px(p.x - 70.0);
            node.top = Val::Px(p.y - 20.0);
        }
    }
}

/// Bun, lettuce, a pinkish patty and cheese, sat on the counter.
fn dress_burgers(
    mut commands: Commands,
    new: Query<(Entity, &Burger), Added<Burger>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if new.is_empty() {
        return;
    }
    let bun = crate::art::soft(&mut materials, Color::srgb(0.85, 0.6, 0.3));
    let patty = crate::art::soft(&mut materials, Color::srgb(0.6, 0.28, 0.25));
    let lettuce = crate::art::soft(&mut materials, Color::srgb(0.4, 0.75, 0.3));
    let cheese = crate::art::soft(&mut materials, Color::srgb(0.98, 0.8, 0.2));
    let dome = meshes.add(Sphere::new(0.13));
    let disc = meshes.add(Cylinder::new(0.14, 0.04));
    let slice = meshes.add(Cuboid::new(0.24, 0.015, 0.24));
    for (entity, burger) in &new {
        commands
            .entity(entity)
            .insert((
                Transform::from_translation(burger_spot(burger.slot)),
                Clickable::at_height(0.1).radius(30.0),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                // Bottom bun.
                p.spawn((
                    Mesh3d(disc.clone()),
                    MeshMaterial3d(bun.clone()),
                    Transform::from_xyz(0.0, 0.02, 0.0),
                ));
                p.spawn((
                    Mesh3d(disc.clone()),
                    MeshMaterial3d(lettuce.clone()),
                    Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(1.12, 0.4, 1.12)),
                ));
                p.spawn((
                    Mesh3d(disc.clone()),
                    MeshMaterial3d(patty.clone()),
                    Transform::from_xyz(0.0, 0.08, 0.0).with_scale(Vec3::new(1.05, 1.1, 1.05)),
                ));
                p.spawn((
                    Mesh3d(slice.clone()),
                    MeshMaterial3d(cheese.clone()),
                    Transform::from_xyz(0.0, 0.105, 0.0).with_rotation(Quat::from_rotation_y(0.4)),
                ));
                // Top bun.
                p.spawn((
                    Mesh3d(dome.clone()),
                    MeshMaterial3d(bun.clone()),
                    Transform::from_xyz(0.0, 0.11, 0.0).with_scale(Vec3::new(1.05, 0.55, 1.05)),
                ));
            });
    }
}

/// The shack itself: a little cream building on a stone base, an open serving
/// window with a counter facing the road, a striped awning, a roof sign and a
/// grill glowing inside. Local scenery; walls and counter block bodies.
fn build_shack(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let soft =
        |m: &mut Assets<StandardMaterial>, r, g, b| crate::art::soft(m, Color::srgb(r, g, b));
    let cream = soft(&mut materials, 0.95, 0.9, 0.78);
    let red = soft(&mut materials, 0.8, 0.18, 0.15);
    let white = soft(&mut materials, 0.97, 0.96, 0.94);
    let stone = soft(&mut materials, 0.58, 0.56, 0.54);
    let wood = soft(&mut materials, 0.55, 0.38, 0.24);
    let steel = crate::art::glossy(&mut materials, Color::srgb(0.6, 0.62, 0.65));
    let glow = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.5, 0.2),
        emissive: LinearRgba::rgb(2.0, 0.7, 0.2),
        ..default()
    });

    let base = floor_y();
    let c = Vec3::new(WENDYS_AT.x, base, WENDYS_AT.y);
    let low = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .iter()
        .map(|(x, z)| {
            crate::terrain::height_at(
                WENDYS_AT.x + x * (HALF_W + 0.5),
                WENDYS_AT.y + z * (COUNTER_Z + 0.5),
            )
        })
        .fold(base, f32::min);
    let front = -HALF_D;
    let lintel = WALL_H - 0.5;
    // Striped awning over the window, sloping out and down.
    let stripes = 8;
    let sw = HALF_W * 2.0 / stripes as f32;
    for i in 0..stripes {
        let x = -HALF_W + sw * (i as f32 + 0.5);
        let mat = if i % 2 == 0 { &red } else { &white };
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(c + Vec3::new(x, lintel - 0.05, front - 0.45))
                .with_rotation(Quat::from_rotation_x(-0.45))
                .with_scale(Vec3::new(sw, 0.05, 1.0)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.7, 0.4),
            intensity: 40_000.0,
            range: 6.0,
            ..default()
        },
        Transform::from_translation(c + Vec3::new(0.0, 2.0, 0.0)),
        DespawnOnExit(AppState::InGame),
    ));
    let mut block = |at: Vec3, size: Vec3, mat: &Handle<StandardMaterial>, solid: bool| {
        let mut e = commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(c + at).with_scale(size),
            DespawnOnExit(AppState::InGame),
        ));
        if solid {
            e.insert((
                avian3d::prelude::RigidBody::Static,
                avian3d::prelude::Collider::cuboid(1.0, 1.0, 1.0),
            ));
        }
    };

    // Stone base, reaching down into the slope.
    let depth = base - low + 0.3;
    block(
        Vec3::new(0.0, -depth / 2.0 + 0.05, 0.0),
        Vec3::new(HALF_W * 2.0 + 0.3, depth + 0.1, HALF_D * 2.0 + 0.3),
        &stone,
        true,
    );
    // Back and side walls.
    let wy = WALL_H / 2.0;
    block(
        Vec3::new(0.0, wy, HALF_D),
        Vec3::new(HALF_W * 2.0, WALL_H, 0.2),
        &cream,
        true,
    );
    for s in [-1.0, 1.0] {
        block(
            Vec3::new(s * HALF_W, wy, 0.0),
            Vec3::new(0.2, WALL_H, HALF_D * 2.0),
            &cream,
            true,
        );
    }
    // Front: a low wall up to the counter, a serving window, a lintel above.
    block(
        Vec3::new(0.0, COUNTER_H / 2.0, front),
        Vec3::new(HALF_W * 2.0, COUNTER_H, 0.2),
        &red,
        true,
    );
    block(
        Vec3::new(0.0, COUNTER_H - 0.03, -COUNTER_Z + 0.15),
        Vec3::new(HALF_W * 1.6, 0.06, 0.8),
        &white,
        true,
    );
    block(
        Vec3::new(0.0, lintel + 0.25, front),
        Vec3::new(HALF_W * 2.0, 0.5, 0.2),
        &cream,
        true,
    );
    // Window posts.
    for s in [-1.0, 1.0] {
        block(
            Vec3::new(s * (HALF_W - 0.3), (COUNTER_H + lintel) / 2.0, front),
            Vec3::new(0.6, lintel - COUNTER_H, 0.2),
            &cream,
            true,
        );
    }
    // Roof slab.
    block(
        Vec3::new(0.0, WALL_H + 0.1, 0.0),
        Vec3::new(HALF_W * 2.0 + 0.4, 0.2, HALF_D * 2.0 + 0.4),
        &red,
        false,
    );
    // Roof sign: a board on two posts.
    for s in [-1.0, 1.0] {
        block(
            Vec3::new(s * 1.2, WALL_H + 0.55, 0.0),
            Vec3::new(0.12, 0.8, 0.12),
            &wood,
            false,
        );
    }
    block(
        Vec3::new(0.0, WALL_H + 1.2, 0.0),
        Vec3::new(3.4, 0.8, 0.15),
        &red,
        false,
    );
    block(
        Vec3::new(0.0, WALL_H + 1.2, -0.09),
        Vec3::new(3.0, 0.5, 0.04),
        &white,
        false,
    );
    // Inside: a grill along the back wall with a warm glow.
    block(
        Vec3::new(0.0, 0.45, HALF_D - 0.5),
        Vec3::new(2.6, 0.9, 0.7),
        &steel,
        false,
    );
    block(
        Vec3::new(0.0, 0.92, HALF_D - 0.5),
        Vec3::new(2.3, 0.05, 0.55),
        &glow,
        false,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burgers_sit_on_the_counter_in_front_of_the_shack() {
        for slot in 0..SLOTS {
            let p = burger_spot(slot);
            assert!(p.z < WENDYS_AT.y, "counter faces the road (north)");
            assert!((p.x - WENDYS_AT.x).abs() < HALF_W);
            assert!(p.y >= COUNTER_H);
        }
    }

    #[test]
    fn the_shack_is_in_the_open_meadow_off_the_road() {
        use crate::zones::{Zone, zone_of};
        assert_eq!(zone_of(WENDYS_AT), Zone::Meadow);
        for dx in [-HALF_W, 0.0, HALF_W] {
            for dz in [-COUNTER_Z, 0.0, HALF_D] {
                let (x, z) = (WENDYS_AT.x + dx, WENDYS_AT.y + dz);
                assert!(
                    !crate::terrain::is_path(x, z),
                    "shack on the road at {x},{z}"
                );
                assert!(!crate::terrain::is_pond(x, z));
            }
        }
    }
}
