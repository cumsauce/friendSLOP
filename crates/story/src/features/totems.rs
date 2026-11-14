//! Magical totems and the locked doors they open. From issue #5.
//!
//! Totems are the world's magic: while you hold one you can cast its spell
//! (R). Magic doors only open while a totem of their kind is inserted, and an
//! inserted totem can't be cast. A door can be opened from either side, but
//! the totem can only be taken back out from the side it went in.
//!
//! Players have no bodies, so "which side you're on" is the side of the door
//! you click on, and "getting into" the vault means being allowed to pick up
//! the totem inside it, which only works while one of its doors is open.

use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::player::{Player, PlayerOwner};

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
pub use friendslop_world::places::vault_at;
/// Half the vault's width (its spot's box on the map).
fn vault_half() -> f32 {
    friendslop_world::places::sizes("vault").size.x / 2.0
}

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
        .add_observer(reset_totems)
        .add_systems(OnEnter(ServerState::Running), place_totems_and_doors);
}

fn place_totems_and_doors(mut commands: Commands) {
    spawn_totems_and_doors(&mut commands);
}

/// A new round: every totem back where it started, every door empty.
fn reset_totems(
    _reset: On<crate::rules::campaign::RoundReset>,
    mut commands: Commands,
    old: Query<Entity, Or<(With<Totem>, With<Door>)>>,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    spawn_totems_and_doors(&mut commands);
}

fn spawn_totems_and_doors(commands: &mut Commands) {
    let totem = |id, magic, at, in_vault| Totem {
        id,
        magic,
        at,
        in_vault,
        holder: None,
        slotted: None,
    };
    commands.spawn((
        totem(1, Magic::Wind, crate::sites::at("totem_wind"), false),
        Replicated,
    ));
    commands.spawn((
        totem(2, Magic::Frost, crate::sites::at("totem_frost"), false),
        Replicated,
    ));
    commands.spawn((totem(3, Magic::Fire, vault_at(), true), Replicated));
    for (id, magic, dx) in [
        (1, Magic::Wind, -vault_half()),
        (2, Magic::Frost, vault_half()),
    ] {
        commands.spawn((
            Door {
                id,
                magic,
                at: vault_at() + Vec2::new(dx, 0.0),
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
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    still: Query<&crate::features::stillness::Stillness>,
    players: Query<(&Player, &PlayerOwner)>,
    doors: Query<&Door>,
    mut totems: Query<&mut Totem>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Some(me) = player_id(ev.client_id, &players) else {
        return;
    };
    let vault_open = doors.iter().any(Door::is_open);
    let Some(mut totem) = totems.iter_mut().find(|t| t.id == ev.id) else {
        return;
    };
    if totem.holder.is_some() || totem.slotted.is_some() {
        deny(
            &mut commands,
            ev.client_id,
            "someone already has that totem",
        );
        return;
    }
    if totem.in_vault && !vault_open {
        deny(&mut commands, ev.client_id, "the vault is locked");
        return;
    }
    let at = Vec3::new(totem.at.x, 0.0, totem.at.y);
    if !in_reach(&mut commands, ev.client_id, &walkers, at) {
        return;
    }
    totem.holder = Some(me);
}

fn use_door(
    ev: On<FromClient<UseDoor>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    still: Query<&crate::features::stillness::Stillness>,
    players: Query<(&Player, &PlayerOwner)>,
    mut doors: Query<&mut Door>,
    mut totems: Query<&mut Totem>,
) {
    if crate::features::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Some(me) = player_id(ev.client_id, &players) else {
        return;
    };
    let Some(mut door) = doors.iter_mut().find(|d| d.id == ev.id) else {
        return;
    };
    let at = Vec3::new(door.at.x, 0.0, door.at.y);
    if !in_reach(&mut commands, ev.client_id, &walkers, at) {
        return;
    }
    // Your side is where your body stands, whatever the client says.
    let side: i8 = match crate::features::interact::body_of(ev.client_id, &walkers) {
        Some(here) if here.x < door.at.x => -1,
        Some(_) => 1,
        None => ev.side.signum(),
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
        Some(_) => deny(
            &mut commands,
            ev.client_id,
            "that totem went in from the other side",
        ),
        None => {
            if let Some(mut t) = totems
                .iter_mut()
                .find(|t| t.holder == Some(me) && t.magic == door.magic)
            {
                t.holder = None;
                t.slotted = Some(door.id);
                door.key_side = Some(side);
            } else {
                deny(&mut commands, ev.client_id, "you aren't holding that totem");
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
        .add_observer(click_totem_or_door)
        .add_systems(OnEnter(AppState::InGame), (build_vault, spawn_totem_hud))
        .add_systems(
            Update,
            (
                dress_totems,
                dress_doors,
                place_totems,
                swing_doors,
                cast_key,
                update_totem_hud.run_if(crate::world::hud_tick()),
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// The vault's walls (local scenery; the doors are replicated). One
/// modeler-made model draws them (`models/features/vault.gltf`, sources in
/// `.modeler/`); each wall keeps its own collider.
fn build_vault(mut commands: Commands, assets: Res<AssetServer>) {
    let h = vault_half();
    // North and south walls, then the short stubs either side of each door.
    let mut pieces = vec![
        (Vec2::new(2.0 * h + 0.3, 0.3), Vec2::new(0.0, -h)),
        (Vec2::new(2.0 * h + 0.3, 0.3), Vec2::new(0.0, h)),
    ];
    for x in [-h, h] {
        for z in [-1.0, 1.0] {
            pieces.push((Vec2::new(0.3, h - 0.6), Vec2::new(x, z * (h + 0.6) / 2.0)));
        }
    }
    for (size, off) in pieces {
        commands.spawn((
            // Walls are solid: the vault is more than a picture of one.
            RigidBody::Static,
            Collider::cuboid(size.x, 1.6, size.y),
            Transform::from_xyz(vault_at().x + off.x, 0.8, vault_at().y + off.y),
            DespawnOnExit(AppState::InGame),
        ));
    }
    commands.spawn((
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset("models/features/vault.gltf")),
        ),
        Transform::from_xyz(vault_at().x, 0.0, vault_at().y),
        DespawnOnExit(AppState::InGame),
    ));
}

fn dress_totems(
    mut commands: Commands,
    new: Query<(Entity, &Totem), Added<Totem>>,
    assets: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, totem) in &new {
        // The carved post and orb glow in the magic's colour (a modeler mesh
        // in a code material); the stone plinth and gold rings are a model.
        let glow = materials.add(StandardMaterial {
            base_color: totem.magic.color(),
            emissive: LinearRgba::from(totem.magic.color()) * 0.6,
            ..default()
        });
        commands
            .entity(entity)
            .insert((
                Transform::from_xyz(totem.at.x, 0.0, totem.at.y),
                Clickable::at_height(0.5).labelled("take the totem"),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                // The carved post and orb (prefab `sp-totem-crystal`): its
                // mesh, worn in the glow material.
                p.spawn((
                    Mesh3d(
                        assets.load(
                            GltfAssetLabel::Primitive {
                                mesh: 0,
                                primitive: 0,
                            }
                            .from_asset("models/features/totem_crystal.gltf"),
                        ),
                    ),
                    MeshMaterial3d(glow.clone()),
                    Transform::default(),
                ));
                p.spawn((
                    WorldAssetRoot(assets.load(
                        GltfAssetLabel::Scene(0).from_asset("models/features/totem_base.gltf"),
                    )),
                    Transform::default(),
                ));
            });
    }
}

fn dress_doors(
    mut commands: Commands,
    new: Query<(Entity, &Door), Added<Door>>,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, door) in &new {
        // The way into the vault, marked from outside (it walks towards the
        // vault's middle). Local, like the rest of the door's look.
        let inwards = Vec2::new((vault_at().x - door.at.x).signum(), 0.0);
        crate::features::doorway::spawn(
            &mut commands,
            Vec3::new(door.at.x, 0.0, door.at.y) - inwards.extend(0.0).xzy() * 0.35,
            crate::features::doorway::yaw_towards(inwards),
            crate::features::doorway::Doorway::way_in("the vault").size(1.2, 1.4),
        );
        // The rune glows in the door's magic colour; the slab itself is a
        // modeler-made model (timber, iron bands, a gold ring round the rune).
        let mat = materials.add(StandardMaterial {
            base_color: door.magic.color(),
            emissive: LinearRgba::from(door.magic.color()) * 0.6,
            ..default()
        });
        commands
            .entity(entity)
            .insert((
                // The collider rides the transform, so the door sinking open
                // clears the gap and a closed one blocks it.
                RigidBody::Static,
                Collider::cuboid(0.15, 1.4, 1.2),
                Transform::from_xyz(door.at.x, 0.7, door.at.y),
                Clickable::at_height(0.0).labelled("open the door"),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|d| {
                d.spawn((
                    WorldAssetRoot(assets.load(
                        GltfAssetLabel::Scene(0).from_asset("models/features/magic_door.gltf"),
                    )),
                    Transform::default(),
                ));
                // A diamond rune through the slab, showing on both faces.
                d.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.14, 0.26, 0.26))),
                    MeshMaterial3d(mat),
                    Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_4)),
                ));
            });
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
        let at = Vec3::new(pos.x, y, pos.y);
        if tf.translation != at {
            tf.translation = at;
        }
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
        if y == target {
            continue; // at rest: don't re-sync the door's static collider (#151)
        }
        let next = y + (target - y) * (time.delta_secs() * 4.0).min(1.0);
        tf.translation.y = if (target - next).abs() < 0.002 {
            target
        } else {
            next
        };
    }
}

/// The unified click landed on a totem or a door. A totem stuck in a door
/// counts as using that door; the server works out which side you're on.
fn click_totem_or_door(
    click: On<Clicked>,
    mut commands: Commands,
    totems: Query<&Totem>,
    doors: Query<&Door>,
) {
    if let Ok(t) = totems.get(click.entity) {
        match t.slotted {
            Some(id) => commands.client_trigger(UseDoor { id, side: 0 }),
            None => commands.client_trigger(GrabTotem { id: t.id }),
        }
    } else if let Ok(door) = doors.get(click.entity) {
        commands.client_trigger(UseDoor {
            id: door.id,
            side: 0,
        });
    }
}

fn cast_key(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::KeyR) {
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
        crate::hud::slot(crate::hud::HudSlot::Scoreboard),
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_totem_hud(
    time: Res<Time>,
    mut log: ResMut<SpellLog>,
    totems: Query<&Totem>,
    doors: Query<&Door>,
    mut hud: Query<&mut Text, With<TotemHud>>,
    mut last_run: Local<Option<f32>>,
) {
    // Throttled: count down by the time since this system last ran.
    let now = time.elapsed_secs();
    let dt = now - last_run.replace(now).unwrap_or(now);
    for entry in &mut log.0 {
        entry.1 -= dt;
    }
    log.0.retain(|e| e.1 > 0.0);
    let mut lines = vec![crate::input::fill(
        "totems ({use} to take, {use} a door's side to insert/remove, {totem} casts)",
    )];
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
