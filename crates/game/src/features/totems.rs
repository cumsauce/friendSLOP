//! Magical totems and the locked doors they open. From issue #5.
//!
//! Totems are the world's magic: while you hold one you can cast its spell
//! (F). Magic doors only open while a totem of their kind is inserted, and an
//! inserted totem can't be cast. A door can be opened from either side, but
//! the totem can only be taken back out from the side it went in.
//!
//! Players have no bodies, so "which side you're on" is the side of the door
//! you click on, and "getting into" the vault means being allowed to pick up
//! the totem inside it, which only works while one of its doors is open.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::player::{Player, PlayerOwner};
use crate::world::MainCamera;

/// The kinds of magic. Each totem and door has one.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Magic {
    Wind,
    Frost,
    Fire,
}

impl Magic {
    pub fn name(self) -> &'static str {
        match self {
            Magic::Wind => "Wind",
            Magic::Frost => "Frost",
            Magic::Fire => "Fire",
        }
    }

    pub fn spell(self) -> &'static str {
        match self {
            Magic::Wind => "a howling gust",
            Magic::Frost => "a flurry of frost",
            Magic::Fire => "a burst of flame",
        }
    }

    fn color(self) -> Color {
        match self {
            Magic::Wind => Color::srgb(0.6, 0.95, 0.75),
            Magic::Frost => Color::srgb(0.55, 0.8, 1.0),
            Magic::Fire => Color::srgb(1.0, 0.45, 0.15),
        }
    }
}

/// Centre of the walled vault. Its west door is Wind, its east door Frost.
pub const VAULT_AT: Vec2 = Vec2::new(13.0, -2.0);
const VAULT_HALF: f32 = 2.0;

/// A totem. Lying at `at` when `holder` and `slotted` are both `None`.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Totem {
    pub id: u32,
    pub magic: Magic,
    pub at: Vec2,
    /// Inside the vault: can only be picked up while a vault door is open.
    pub in_vault: bool,
    /// The `Player::id` carrying it.
    pub holder: Option<u64>,
    /// The `Door::id` it's inserted into.
    pub slotted: Option<u32>,
}

/// A magic door in a wall running north-south (its sides are west/east).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Door {
    pub id: u32,
    pub magic: Magic,
    pub at: Vec2,
    /// Which side (-1 west, +1 east) the totem was inserted from. Open iff set.
    pub key_side: Option<i8>,
}

impl Door {
    pub fn is_open(&self) -> bool {
        self.key_side.is_some()
    }
}

/// Client -> server: "I pick up totem `id`."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct GrabTotem {
    pub id: u32,
}

/// Client -> server: "I use door `id` from `side` (-1 west, +1 east)":
/// insert a matching totem, or take the inserted one back out.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct UseDoor {
    pub id: u32,
    pub side: i8,
}

/// Client -> server: "I cast the spells of every totem I hold."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct CastSpells;

/// Server -> everyone: `player` cast `magic`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SpellCast {
    pub player: u64,
    pub magic: Magic,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Totem>()
        .replicate::<Door>()
        .add_client_event::<GrabTotem>(Channel::Ordered)
        .add_client_event::<UseDoor>(Channel::Ordered)
        .add_client_event::<CastSpells>(Channel::Ordered)
        .add_server_event::<SpellCast>(Channel::Ordered)
        .add_observer(grab_totem)
        .add_observer(use_door)
        .add_observer(cast_spells)
        .add_observer(drop_on_leave)
        .add_systems(OnEnter(ServerState::Running), place_totems_and_doors);
}

fn place_totems_and_doors(mut commands: Commands) {
    let totem = |id, magic, at, in_vault| Totem {
        id,
        magic,
        at,
        in_vault,
        holder: None,
        slotted: None,
    };
    commands.spawn((
        totem(1, Magic::Wind, Vec2::new(8.0, 3.0), false),
        Replicated,
    ));
    commands.spawn((
        totem(2, Magic::Frost, Vec2::new(9.0, -8.0), false),
        Replicated,
    ));
    commands.spawn((totem(3, Magic::Fire, VAULT_AT, true), Replicated));
    for (id, magic, dx) in [(1, Magic::Wind, -VAULT_HALF), (2, Magic::Frost, VAULT_HALF)] {
        commands.spawn((
            Door {
                id,
                magic,
                at: VAULT_AT + Vec2::new(dx, 0.0),
                key_side: None,
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

fn grab_totem(
    ev: On<FromClient<GrabTotem>>,
    players: Query<(&Player, &PlayerOwner)>,
    doors: Query<&Door>,
    mut totems: Query<&mut Totem>,
) {
    let Some(me) = player_id(ev.client_id, &players) else {
        return;
    };
    let vault_open = doors.iter().any(Door::is_open);
    let Some(mut totem) = totems.iter_mut().find(|t| t.id == ev.id) else {
        return;
    };
    if totem.holder.is_some() || totem.slotted.is_some() || (totem.in_vault && !vault_open) {
        return;
    }
    totem.holder = Some(me);
}

fn use_door(
    ev: On<FromClient<UseDoor>>,
    players: Query<(&Player, &PlayerOwner)>,
    mut doors: Query<&mut Door>,
    mut totems: Query<&mut Totem>,
) {
    let Some(me) = player_id(ev.client_id, &players) else {
        return;
    };
    let side = ev.side.signum();
    let Some(mut door) = doors.iter_mut().find(|d| d.id == ev.id) else {
        return;
    };
    match door.key_side {
        // Only the side it went in from can take it back out.
        Some(k) if k == side => {
            if let Some(mut t) = totems.iter_mut().find(|t| t.slotted == Some(door.id)) {
                t.slotted = None;
                t.holder = Some(me);
            }
            door.key_side = None;
        }
        Some(_) => {}
        None => {
            if let Some(mut t) = totems
                .iter_mut()
                .find(|t| t.holder == Some(me) && t.magic == door.magic)
            {
                t.holder = None;
                t.slotted = Some(door.id);
                door.key_side = Some(side);
            }
        }
    }
}

fn cast_spells(
    ev: On<FromClient<CastSpells>>,
    mut commands: Commands,
    players: Query<(&Player, &PlayerOwner)>,
    totems: Query<&Totem>,
    still: Query<&super::stillness::Stillness>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let Some(me) = player_id(ev.client_id, &players) else {
        return;
    };
    for t in totems.iter().filter(|t| t.holder == Some(me)) {
        commands.server_trigger(ToClients {
            targets: SendTargets::All,
            message: SpellCast {
                player: me,
                magic: t.magic,
            },
        });
    }
}

/// Server: a leaving player drops what they carry where they picked it up.
fn drop_on_leave(
    remove: On<Remove<Player>>,
    server: Res<State<ServerState>>,
    players: Query<&Player>,
    mut totems: Query<&mut Totem>,
) {
    if *server.get() != ServerState::Running {
        return;
    }
    let Ok(player) = players.get(remove.entity) else {
        return;
    };
    for mut t in &mut totems {
        if t.holder == Some(player.id) {
            t.holder = None;
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<SpellLog>()
        .add_observer(hear_spell)
        .add_systems(OnEnter(AppState::InGame), (build_vault, spawn_totem_hud))
        .add_systems(
            Update,
            (
                dress_totems,
                dress_doors,
                place_totems,
                swing_doors,
                click_totems_and_doors,
                cast_key,
                update_totem_hud,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// The vault's walls (local scenery; the doors are replicated).
fn build_vault(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let stone = materials.add(Color::srgb(0.55, 0.5, 0.6));
    let h = VAULT_HALF;
    let wall = |len_x: f32, len_z: f32| Cuboid::new(len_x, 1.6, len_z);
    // North and south walls, then the short stubs either side of each door.
    let mut pieces = vec![
        (wall(2.0 * h + 0.3, 0.3), Vec2::new(0.0, -h)),
        (wall(2.0 * h + 0.3, 0.3), Vec2::new(0.0, h)),
    ];
    for x in [-h, h] {
        for z in [-1.0, 1.0] {
            pieces.push((wall(0.3, h - 0.6), Vec2::new(x, z * (h + 0.6) / 2.0)));
        }
    }
    for (shape, off) in pieces {
        commands.spawn((
            Mesh3d(meshes.add(shape)),
            MeshMaterial3d(stone.clone()),
            Transform::from_xyz(VAULT_AT.x + off.x, 0.8, VAULT_AT.y + off.y),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

fn dress_totems(
    mut commands: Commands,
    new: Query<(Entity, &Totem), Added<Totem>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, totem) in &new {
        let glow = materials.add(StandardMaterial {
            base_color: totem.magic.color(),
            emissive: LinearRgba::from(totem.magic.color()) * 0.6,
            ..default()
        });
        commands
            .entity(entity)
            .insert((
                Transform::from_xyz(totem.at.x, 0.0, totem.at.y),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(meshes.add(Cylinder::new(0.15, 0.6))),
                    MeshMaterial3d(glow.clone()),
                    Transform::from_xyz(0.0, 0.3, 0.0),
                ));
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.18))),
                    MeshMaterial3d(glow),
                    Transform::from_xyz(0.0, 0.75, 0.0),
                ));
            });
    }
}

fn dress_doors(
    mut commands: Commands,
    new: Query<(Entity, &Door), Added<Door>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, door) in &new {
        let mat = materials.add(StandardMaterial {
            base_color: door.magic.color(),
            emissive: LinearRgba::from(door.magic.color()) * 0.2,
            ..default()
        });
        commands.entity(entity).insert((
            Mesh3d(meshes.add(Cuboid::new(0.15, 1.4, 1.2))),
            MeshMaterial3d(mat),
            Transform::from_xyz(door.at.x, 0.7, door.at.y),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Free totems stand on the floor, inserted ones glow above their door,
/// carried ones are with their (bodiless) holder and so not shown.
fn place_totems(doors: Query<&Door>, mut totems: Query<(&Totem, &mut Transform, &mut Visibility)>) {
    for (t, mut tf, mut vis) in &mut totems {
        let pos = t
            .slotted
            .and_then(|d| doors.iter().find(|door| door.id == d))
            .map_or(t.at, |door| door.at);
        let y = if t.slotted.is_some() { 1.5 } else { 0.0 };
        tf.translation = Vec3::new(pos.x, y, pos.y);
        let want = if t.holder.is_some() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if *vis != want {
            *vis = want;
        }
    }
}

/// Open doors sink into the floor; locked ones stand in the gap.
fn swing_doors(time: Res<Time>, mut doors: Query<(&Door, &mut Transform)>) {
    for (door, mut tf) in &mut doors {
        let target = if door.is_open() { -0.75 } else { 0.7 };
        let y = tf.translation.y;
        tf.translation.y = y + (target - y) * (time.delta_secs() * 4.0).min(1.0);
    }
}

const CLICK_RADIUS: f32 = 40.0;

fn click_totems_and_doors(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    totems: Query<&Totem>,
    doors: Query<&Door>,
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
    let near = |p: Vec3| {
        camera
            .world_to_viewport(cam_tf, p)
            .is_ok_and(|s| s.distance(cursor) < CLICK_RADIUS)
    };
    if let Some(t) = totems
        .iter()
        .find(|t| t.holder.is_none() && t.slotted.is_none() && near(Vec3::new(t.at.x, 0.5, t.at.y)))
    {
        commands.client_trigger(GrabTotem { id: t.id });
        return;
    }
    // Which side of the door you click on is the side you're standing on.
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else {
        return;
    };
    let Some(dist) = ray.intersect_plane(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y)) else {
        return;
    };
    let floor = ray.get_point(dist);
    let floor = Vec2::new(floor.x, floor.z);
    if let Some(door) = doors.iter().find(|d| floor.distance(d.at) < 1.3) {
        let side = if floor.x < door.at.x { -1 } else { 1 };
        commands.client_trigger(UseDoor { id: door.id, side });
    }
}

fn cast_key(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::KeyF) {
        commands.client_trigger(CastSpells);
    }
}

/// Recent spells, newest last, with seconds left on screen.
#[derive(Resource, Default)]
struct SpellLog(Vec<(String, f32)>);

fn hear_spell(cast: On<SpellCast>, mut log: ResMut<SpellLog>) {
    log.0.push((
        format!(
            "player {} casts {} ({})!",
            cast.player,
            cast.magic.name(),
            cast.magic.spell()
        ),
        4.0,
    ));
    if log.0.len() > 4 {
        log.0.remove(0);
    }
}

#[derive(Component)]
struct TotemHud;

fn spawn_totem_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TotemHud,
        Node {
            position_type: PositionType::Absolute,
            top: percent(60),
            right: px(12),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_totem_hud(
    time: Res<Time>,
    mut log: ResMut<SpellLog>,
    totems: Query<&Totem>,
    doors: Query<&Door>,
    mut hud: Query<&mut Text, With<TotemHud>>,
) {
    for entry in &mut log.0 {
        entry.1 -= time.delta_secs();
    }
    log.0.retain(|e| e.1 > 0.0);
    let mut lines =
        vec!["totems (click to take, click a door's side to insert/remove, F casts)".to_string()];
    let mut ts: Vec<_> = totems.iter().collect();
    ts.sort_by_key(|t| t.id);
    for t in ts {
        let place = match (t.holder, t.slotted) {
            (Some(p), _) => format!("held by player {p}"),
            (None, Some(_)) => "in its door".to_string(),
            (None, None) if t.in_vault => "in the vault".to_string(),
            (None, None) => "on the floor".to_string(),
        };
        lines.push(format!("{}: {place}", t.magic.name()));
    }
    let mut ds: Vec<_> = doors.iter().collect();
    ds.sort_by_key(|d| d.id);
    for d in ds {
        let state = match d.key_side {
            None => "locked",
            Some(s) if s < 0 => "open (key in from the west)",
            Some(_) => "open (key in from the east)",
        };
        lines.push(format!("{} door: {state}", d.magic.name()));
    }
    lines.extend(log.0.iter().map(|e| e.0.clone()));
    let want = lines.join("\n");
    for mut text in &mut hud {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}
