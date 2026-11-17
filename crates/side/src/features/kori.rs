//! KoriCoins™ and the claw machine (#382, catherine): "a currency in the game
//! named kori coins that u earn from interacting with farts and poop and other
//! things of that nature that let u play a claw machine for little collectible
//! figures and stuff. spot in main menu for tracking what kori coin claw
//! machine prizes u have and to see them and inspect them. you can also equip
//! them in your hand and walk around with them like csgo knives and they speed
//! u up a lil".
//!
//! Later (#459, catherine): "a fully built out claw machine game like yakuza 0
//! with working physics and stuff ... also add more claw machines to the city,
//! one in every boba store and one in every station along the subway".
//!
//! - **Earning** (server): every fart pays [`KoriCoins`] (more if an NPC was
//!   shamed by it), and eating a toilet's feces pays more (`toilets.rs`).
//!   Amounts: `data/base/kori.ron`. Your coins show in the money bubble (wallet.rs).
//! - **The machines** ([`machine_spots`]): one by the gambling table, one in
//!   every boba shop and one on the platform of every subway station (every
//!   stop of a train line). Each is a replicated [`ClawMachine`].
//! - **A go** ([`Tuning::price`] coins, paid to the server): your eyes go to
//!   the glass and your hands to the stick. Steer the claw with the walk keys
//!   (or stick), drop it with `claw_drop` (or wait out the clock), and it goes
//!   down, closes, lifts and carries what it grabbed back to the prize bin in
//!   the front right corner. The prizes are real physics bodies (your own,
//!   local): they tumble, the claw shoves them, and its grip slips
//!   ([`ClawTuning::slip_per_sec`]) like every claw machine ever. Whatever ends
//!   up in the bin is yours (one a go), sent to the server, which shows
//!   everyone.
//! - **Your shelf** ([`Shelf`]): what you've won is kept on your own machine
//!   (`kori_prizes.txt`), across games. The main menu's "Kori prizes" page lists
//!   them and spins one up to inspect (`crates/game/src/kori_shelf.rs`).
//! - **In hand** (.): cycles through the figures you own and empty hands.
//!   Everyone sees it in your hand, and it adds [`Tuning::held_speed`] to your
//!   walk (`rules::kori::HeldPrize`).

use std::collections::HashMap;

use avian3d::prelude::{
    AngularVelocity, Collider, CollisionLayers, LayerMask, LinearVelocity, RigidBody,
};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_base::areas::{AreaApp as _, Shown};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{HandsBusy, Walker};
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::player::{LocalPlayer, Player, PlayerOwner};
pub use crate::rules::kori::{HeldPrize, KoriCoins};

/// One figure the claw can win.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Prize {
    pub name: String,
    /// What the shelf says about it.
    pub blurb: String,
    /// Its model, under `assets/`.
    pub model: String,
    /// Relative odds of a figure in the pit being this one.
    pub weight: f32,
}

/// How the claw itself plays (#459).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct ClawTuning {
    /// Seconds to steer before the claw drops by itself.
    pub steer_secs: f32,
    /// How fast the claw moves across the pit (m/s).
    pub move_speed: f32,
    /// How fast it goes down and comes back up (m/s).
    pub drop_speed: f32,
    pub lift_speed: f32,
    /// How close (m) a figure's middle must be to the closing prongs to be grabbed.
    pub grab_radius: f32,
    /// Chance per second the grip slips while it carries something.
    pub slip_per_sec: f32,
    /// Figures in the pit.
    pub pit_count: usize,
    /// A figure's ball (m) and how big its model is drawn in the pit.
    pub prize_radius: f32,
    pub prize_scale: f32,
    /// Seconds after the claw opens over the bin before the go ends.
    pub settle_secs: f32,
    /// Shown when your go starts.
    pub steer_line: String,
}

/// Tuning and writing: `assets/data/side/kori.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Coins a go costs.
    pub price: u32,
    /// Walking speed a figure in hand adds (0.1 = +10%).
    pub held_speed: f32,
    /// The commune's machine stands this far (x, z metres) from the gambling table...
    pub offset: (f32, f32),
    /// ...turned this much (degrees).
    pub yaw_deg: f32,
    /// Where a boba shop's machine stands in the shop (x, z m, the shop's own
    /// frame: door at +z) and its turn (degrees).
    pub boba_spot: (f32, f32, f32),
    /// Where a subway station's stands (x, z m along and across the stop's
    /// frame: the platform is on +z) and its turn (degrees).
    pub station_spot: (f32, f32, f32),
    /// The claw.
    pub claw: ClawTuning,
    /// Every figure.
    pub prizes: Vec<Prize>,
    /// Shown to everyone when someone wins: `{who}`, `{prize}`.
    pub win_line: String,
    /// Shown to you when you come away with nothing.
    pub miss_line: String,
    /// Shown when you can't pay: `{price}`, `{coins}`.
    pub broke_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/kori.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

pub const MACHINE_MODEL: &str = "models/props/claw_machine.gltf";
const GANTRY_MODEL: &str = "models/props/claw_gantry.gltf";
const TROLLEY_MODEL: &str = "models/props/claw_trolley.gltf";
const HUB_MODEL: &str = "models/props/claw_hub.gltf";
const PRONG_MODEL: &str = "models/props/claw_prong.gltf";
/// Collider half sizes (the cabinet is ~0.9 x 1.9 x 0.9 m).
const HALF: Vec3 = Vec3::new(0.45, 0.95, 0.45);
/// Where a held figure sits in front of your own camera.
const VIEW_HAND: Vec3 = Vec3::new(0.22, -0.2, -0.45);
/// Where it sits on someone else's body (feet-relative, before their yaw).
const BODY_HAND: Vec3 = Vec3::new(0.3, 1.0, -0.3);
/// Figures are shown this much bigger in hand than on the shelf.
const HAND_SCALE: f32 = 1.4;

// The pit inside the cabinet model (machine's own frame, metres).
/// The pit's floor and how far the claw roams either way (x and z).
const PIT_FLOOR: f32 = 1.05;
const PIT_HALF: f32 = 0.43;
const CLAW_REACH: f32 = 0.36;
/// The claw's hub when it's all the way up, and the gantry above it.
const CLAW_TOP: f32 = 1.62;
const GANTRY_Y: f32 = 1.72;
const TROLLEY_Y: f32 = 1.69;
/// The prize bin: its inner corner (x and z both above this) and wall height.
const BIN_FROM: f32 = 0.17;
const BIN_WALL: f32 = 0.14;
/// Where the claw rests: over the bin.
const HOME: Vec2 = Vec2::new(0.29, 0.29);
/// Prongs' hinge under the hub and how far below the hub they grab.
const PRONG_AT: f32 = 0.025;
const GRAB_BELOW: f32 = 0.1;
/// Prong tilt (radians) open and shut.
const OPEN: f32 = 0.55;
const SHUT: f32 = -0.12;
/// Where your eyes go while you play (machine frame), and what they look at.
const EYE: Vec3 = Vec3::new(0.0, 1.62, 1.05);
const LOOK: Vec3 = Vec3::new(0.0, 1.22, 0.0);
/// Pit physics only meets pit physics.
const PIT: LayerMask = LayerMask(1 << 21);

/// Pure: where a machine stands in the world for a spot `(x, z, yaw°)` in a frame.
fn in_frame(frame: Transform, (x, z, yaw): (f32, f32, f32), floor: f32) -> (Vec3, f32) {
    let at = frame.transform_point(Vec3::new(x, floor, z));
    let (fy, _, _) = frame.rotation.to_euler(EulerRot::YXZ);
    (at, fy + yaw.to_radians())
}

/// Every machine (feet and yaw): the commune's by the gambling table, one in
/// each boba shop, one on each subway platform.
pub fn machine_spots(t: &Tuning) -> Vec<(Vec3, f32)> {
    let p = friendslop_world::places::gamble_at() + Vec2::new(t.offset.0, t.offset.1);
    let mut out = vec![(
        Vec3::new(p.x, crate::terrain::height_at(p.x, p.y), p.y),
        t.yaw_deg.to_radians(),
    )];
    for shop in &crate::features::boba::tuning().shops {
        out.push(in_frame(
            crate::features::boba::frame(shop),
            t.boba_spot,
            0.0,
        ));
    }
    use crate::features::trains;
    for line in &trains::tuning().lines {
        if line.vehicle != trains::Vehicle::Train {
            continue;
        }
        for s in &line.stops {
            out.push(in_frame(
                trains::stop_frame(s),
                t.station_spot,
                trains::PLATFORM_TOP,
            ));
        }
    }
    out
}

/// A claw machine. Replicated; the server spawns them.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ClawMachine {
    pub id: u16,
    pub at: Vec3,
    pub yaw: f32,
}

/// Client → server: pay for a go on machine `0`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PlayClaw(pub u16);

/// Server → everyone: player `to` paid for a go on machine `machine`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ClawGo {
    pub to: u64,
    pub machine: u16,
}

/// Client → server: my go is over; this prize dropped into the bin (or none).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct ClawEnd(pub Option<u8>);

/// Client → server: hold this prize (an index into the list), or nothing.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct HoldPrize(pub Option<u8>);

/// Server → everyone: player `to`'s go ended; `prize` if they won one.
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct ClawResult {
    pub to: u64,
    pub prize: Option<u8>,
}

/// Server: who has a paid go running (player id → machine).
#[derive(Resource, Default, Debug)]
struct Goes(HashMap<u64, u16>);

pub fn protocol(app: &mut App) {
    app.replicate::<KoriCoins>()
        .replicate::<HeldPrize>()
        .replicate::<ClawMachine>()
        .add_client_event::<PlayClaw>(Channel::Ordered)
        .add_client_event::<HoldPrize>(Channel::Ordered)
        .add_server_event::<ClawResult>(Channel::Ordered)
        .add_server_event::<ClawGo>(Channel::Ordered)
        .add_client_event::<ClawEnd>(Channel::Ordered)
        .init_resource::<Goes>()
        .add_observer(play)
        .add_observer(end)
        .add_observer(hold)
        .add_observer(fart_pays)
        .add_systems(
            Update,
            (give_purses, place_machines).run_if(in_state(ServerState::Running)),
        );
    for (at, _) in machine_spots(&tuning()) {
        friendslop_world::footprints::claim(Vec2::new(at.x, at.z), 1.5);
    }
}

/// Pure: a cheap hash to a number in 0..1.
fn unit(seed: u64) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// Pure: which figure a roll in 0..1 picks, by weight.
pub fn pick(t: &Tuning, which: f32) -> Option<u8> {
    let total: f32 = t.prizes.iter().map(|p| p.weight.max(0.0)).sum();
    if total <= 0.0 {
        return None;
    }
    let mut left = which * total;
    for (i, p) in t.prizes.iter().enumerate() {
        left -= p.weight.max(0.0);
        if left < 0.0 {
            return Some(i as u8);
        }
    }
    Some((t.prizes.len() - 1) as u8)
}

/// Pure: fill a line's `{name}` holes.
pub fn fill(line: &str, pairs: &[(&str, String)]) -> String {
    let mut out = line.to_string();
    for (k, v) in pairs {
        out = out.replace(&["{", k, "}"].concat(), v);
    }
    out
}

fn give_purses(
    mut commands: Commands,
    new: Query<Entity, (With<Player>, With<Walker>, Without<KoriCoins>)>,
) {
    for e in &new {
        commands
            .entity(e)
            .insert((KoriCoins::default(), HeldPrize::default()));
    }
}

fn place_machines(mut commands: Commands, machines: Query<(), With<ClawMachine>>) {
    if !machines.is_empty() {
        return;
    }
    for (id, (at, yaw)) in machine_spots(&tuning()).into_iter().enumerate() {
        commands.spawn((
            ClawMachine {
                id: id as u16,
                at,
                yaw,
            },
            Replicated,
        ));
    }
}

/// Server: every fart pays (it's a server event the host hears too).
fn fart_pays(
    ev: On<crate::features::fart::Farted>,
    time: Res<Time>,
    mut purses: Query<(&Player, &mut KoriCoins)>,
) {
    let e = crate::rules::kori::earnings();
    let Some((_, coins)) = purses.iter_mut().find(|(p, _)| p.id == ev.player) else {
        return;
    };
    let seed =
        time.elapsed().as_nanos() as u64 ^ ev.player.rotate_left(32) ^ ev.at.x.to_bits() as u64;
    let n = crate::rules::kori::fart_coins(&e, ev.shamed, unit(seed));
    crate::rules::kori::pay(Some(coins), n);
}

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    }
}

fn play(
    ev: On<FromClient<PlayClaw>>,
    mut commands: Commands,
    mut goes: ResMut<Goes>,
    walkers: Query<(&PlayerOwner, &Walker)>,
    machines: Query<&ClawMachine>,
    mut purses: Query<(&Player, &PlayerOwner, &mut KoriCoins)>,
) {
    let t = tuning();
    let Some(machine) = machines.iter().find(|m| m.id == ev.0) else {
        return;
    };
    if !in_reach(&mut commands, ev.client_id, &walkers, machine.at) {
        return;
    }
    let owner = owner_of(ev.client_id);
    let Some((player, _, mut coins)) = purses.iter_mut().find(|p| p.1.0 == owner) else {
        return;
    };
    if goes.0.contains_key(&player.id) {
        return;
    }
    if coins.0 < t.price {
        let line = fill(
            &t.broke_line,
            &[
                ("price", t.price.to_string()),
                ("coins", coins.0.to_string()),
            ],
        );
        deny(&mut commands, ev.client_id, line);
        return;
    }
    coins.0 -= t.price;
    goes.0.insert(player.id, machine.id);
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: ClawGo {
            to: player.id,
            machine: machine.id,
        },
    });
}

/// Server: a go ends; a prize only counts if that player had paid for a go.
fn end(
    ev: On<FromClient<ClawEnd>>,
    mut commands: Commands,
    mut goes: ResMut<Goes>,
    players: Query<(&Player, &PlayerOwner)>,
) {
    let owner = owner_of(ev.client_id);
    let Some((player, _)) = players.iter().find(|p| p.1.0 == owner) else {
        return;
    };
    if goes.0.remove(&player.id).is_none() {
        return;
    }
    let prize = ev.0.filter(|i| (*i as usize) < tuning().prizes.len());
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: ClawResult {
            to: player.id,
            prize,
        },
    });
}

fn hold(ev: On<FromClient<HoldPrize>>, mut players: Query<(&PlayerOwner, &mut HeldPrize)>) {
    let t = tuning();
    let owner = owner_of(ev.client_id);
    let Some((_, mut held)) = players.iter_mut().find(|p| p.0.0 == owner) else {
        return;
    };
    let prize = ev.0.filter(|i| (*i as usize) < t.prizes.len());
    *held = HeldPrize {
        prize,
        speed: t.held_speed,
    };
}

// ------------------------------------------------------------------ the shelf

/// Your prizes: (name, how many), kept on this machine across games.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct Shelf {
    pub counts: Vec<(String, u32)>,
}

const SHELF_FILE: &str = "kori_prizes.txt";

impl Shelf {
    /// Pure: read `count<TAB>name` lines.
    pub fn parse(text: &str) -> Self {
        let counts = text
            .lines()
            .filter_map(|l| {
                let (n, name) = l.split_once('\t')?;
                Some((name.to_string(), n.trim().parse().ok()?))
            })
            .collect();
        Self { counts }
    }

    pub fn write(&self) -> String {
        self.counts
            .iter()
            .map(|(name, n)| format!("{n}\t{name}\n"))
            .collect()
    }

    pub fn load() -> Self {
        crate::settings::load_local(SHELF_FILE)
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }

    pub fn count(&self, name: &str) -> u32 {
        self.counts
            .iter()
            .find(|(n, _)| n == name)
            .map_or(0, |c| c.1)
    }

    pub fn add(&mut self, name: &str) {
        match self.counts.iter_mut().find(|(n, _)| n == name) {
            Some(c) => c.1 += 1,
            None => self.counts.push((name.to_string(), 1)),
        }
    }

    /// Pure: the next thing to hold after `now` (owned prizes in list order,
    /// then empty hands).
    pub fn next_held(&self, prizes: &[Prize], now: Option<u8>) -> Option<u8> {
        let owned: Vec<u8> = (0..prizes.len().min(255) as u8)
            .filter(|i| self.count(&prizes[*i as usize].name) > 0)
            .collect();
        match now.and_then(|n| owned.iter().position(|o| *o == n)) {
            Some(i) => owned.get(i + 1).copied(),
            None => owned.first().copied(),
        }
    }
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.dress_in_area::<ClawMachine>();
    app.insert_resource(Shelf::load())
        .init_resource::<MyGo>()
        .add_observer(click)
        .add_observer(go)
        .add_observer(result)
        .add_hud(spawn_ui)
        .add_systems(
            Update,
            (
                body,
                dress,
                (steer, run_claw, catch_in_bin, pose_rig, view, claw_context).chain(),
                show_coins,
                cycle_held,
                draw_held,
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnExit(AppState::InGame), stop_go);
}

/// The cabinet's collider, as soon as a machine turns up (kept through
/// area loads: it's the machine's body).
fn body(mut commands: Commands, new: Query<(Entity, &ClawMachine), Added<ClawMachine>>) {
    for (e, m) in &new {
        commands.entity(e).insert((
            Transform::from_translation(m.at).with_rotation(Quat::from_rotation_y(m.yaw)),
            Visibility::default(),
            Clickable::at_height(1.0).labelled("play the Kori claw"),
            ClawPose::default(),
            DespawnOnExit(AppState::InGame),
            children![(
                Transform::from_translation(Vec3::Y * HALF.y),
                RigidBody::Static,
                Collider::cuboid(HALF.x * 2.0, HALF.y * 2.0, HALF.z * 2.0),
            )],
        ));
    }
}

/// Where a machine's claw is (machine frame): over (x, z), the hub's height,
/// and how shut the prongs are (0 open .. 1 shut).
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct ClawPose {
    pub at: Vec2,
    pub y: f32,
    pub shut: f32,
}

impl Default for ClawPose {
    fn default() -> Self {
        Self {
            at: HOME,
            y: CLAW_TOP,
            shut: 0.3,
        }
    }
}

/// The drawn claw of one machine.
#[derive(Component)]
struct ClawRig {
    gantry: Entity,
    trolley: Entity,
    cable: Entity,
    hub: Entity,
}

#[derive(Component)]
struct Prong;

fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(file.to_string())))
}

/// The cabinet's looks and its claw, each time its area shows.
fn dress(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    new: Query<Entity, (With<ClawMachine>, Added<Shown>)>,
) {
    for e in &new {
        commands.spawn((
            Transform::default(),
            Visibility::default(),
            model(&assets, MACHINE_MODEL),
            ChildOf(e),
        ));
        let gantry = commands
            .spawn((
                Transform::default(),
                model(&assets, GANTRY_MODEL),
                ChildOf(e),
            ))
            .id();
        let trolley = commands
            .spawn((
                Transform::default(),
                model(&assets, TROLLEY_MODEL),
                ChildOf(e),
            ))
            .id();
        // The cable: a thread too plain to model, stretched each frame.
        let cable = commands
            .spawn((
                Transform::default(),
                Mesh3d(meshes.add(Cylinder::new(0.003, 1.0))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: Color::srgb(0.25, 0.25, 0.28),
                    metallic: 0.8,
                    perceptual_roughness: 0.4,
                    ..default()
                })),
                ChildOf(e),
            ))
            .id();
        let hub = commands
            .spawn((Transform::default(), model(&assets, HUB_MODEL), ChildOf(e)))
            .id();
        for _ in 0..3 {
            commands.spawn((
                Prong,
                Transform::default(),
                model(&assets, PRONG_MODEL),
                ChildOf(hub),
            ));
        }
        commands.entity(e).insert(ClawRig {
            gantry,
            trolley,
            cable,
            hub,
        });
    }
}

/// Pure: a prong's place under the hub (prong `i` of three, `shut` 0..1).
pub fn prong_transform(i: usize, shut: f32) -> Transform {
    let turn = Quat::from_rotation_y(i as f32 * std::f32::consts::TAU / 3.0);
    let tilt = OPEN + (SHUT - OPEN) * shut.clamp(0.0, 1.0);
    Transform::from_translation(turn * Vec3::new(0.03, -PRONG_AT, 0.0))
        .with_rotation(turn * Quat::from_rotation_z(tilt))
}

fn pose_rig(
    machines: Query<(&ClawPose, &ClawRig)>,
    children: Query<&Children>,
    mut parts: Query<&mut Transform, Without<ClawMachine>>,
    prongs: Query<(), With<Prong>>,
) {
    for (pose, rig) in &machines {
        let mut put = |e: Entity, t: Transform| {
            if let Ok(mut tf) = parts.get_mut(e)
                && *tf != t
            {
                *tf = t;
            }
        };
        put(rig.gantry, Transform::from_xyz(0.0, GANTRY_Y, pose.at.y));
        put(
            rig.trolley,
            Transform::from_xyz(pose.at.x, TROLLEY_Y, pose.at.y),
        );
        let len = (TROLLEY_Y - pose.y).max(0.01);
        put(
            rig.cable,
            Transform::from_xyz(pose.at.x, pose.y + len / 2.0, pose.at.y)
                .with_scale(Vec3::new(1.0, len, 1.0)),
        );
        put(rig.hub, Transform::from_xyz(pose.at.x, pose.y, pose.at.y));
        let Ok(kids) = children.get(rig.hub) else {
            continue;
        };
        for (i, k) in kids.iter().filter(|k| prongs.contains(*k)).enumerate() {
            put(k, prong_transform(i, pose.shut));
        }
    }
}

fn click(c: On<Clicked>, mut commands: Commands, mine: Res<MyGo>, machines: Query<&ClawMachine>) {
    if let Ok(m) = machines.get(c.entity)
        && mine.0.is_none()
    {
        commands.client_trigger(PlayClaw(m.id));
    }
}

/// What the claw is doing in your go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    /// Steering; seconds left.
    Steer(f32),
    Drop,
    /// Closing; seconds so far.
    Close(f32),
    Lift,
    Return,
    /// Open over the bin; seconds so far.
    Release(f32),
}

/// Your go, on your own screen: the machine and how far the claw has got.
#[derive(Resource, Default)]
pub struct MyGo(Option<Go>);

pub struct Go {
    machine: Entity,
    phase: Phase,
    /// The figure the claw holds.
    held: Option<Entity>,
    /// What fell in the bin.
    won: Option<u8>,
    seed: u64,
}

/// One figure in a pit: a physics ball under the machine.
#[derive(Component)]
struct PitPrize(u8);

/// The pit's walls and floor (and the claw's push), under the machine.
#[derive(Component)]
struct PitPart;

/// The claw's own ball, which shoves figures about.
#[derive(Component)]
struct ClawBall;

fn pit_layers() -> CollisionLayers {
    CollisionLayers::new(PIT, PIT)
}

/// Pure: the pit's static boxes (centre, full size, machine frame): floor,
/// glass walls, the bin's two walls.
pub fn pit_boxes() -> Vec<(Vec3, Vec3)> {
    let h = PIT_HALF;
    let (t, tall) = (0.2, 0.7);
    let mid = PIT_FLOOR + tall / 2.0;
    let bin = (h - BIN_FROM) / 2.0 + BIN_FROM;
    vec![
        (
            Vec3::new(0.0, PIT_FLOOR - t / 2.0, 0.0),
            Vec3::new(2.0 * h, t, 2.0 * h),
        ),
        (
            Vec3::new(h + t / 2.0, mid, 0.0),
            Vec3::new(t, tall, 2.0 * h),
        ),
        (
            Vec3::new(-h - t / 2.0, mid, 0.0),
            Vec3::new(t, tall, 2.0 * h),
        ),
        (
            Vec3::new(0.0, mid, h + t / 2.0),
            Vec3::new(2.0 * h, tall, t),
        ),
        (
            Vec3::new(0.0, mid, -h - t / 2.0),
            Vec3::new(2.0 * h, tall, t),
        ),
        (
            Vec3::new(BIN_FROM - 0.01, PIT_FLOOR + BIN_WALL / 2.0, bin),
            Vec3::new(0.02, BIN_WALL, h - BIN_FROM),
        ),
        (
            Vec3::new(bin, PIT_FLOOR + BIN_WALL / 2.0, BIN_FROM - 0.01),
            Vec3::new(h - BIN_FROM, BIN_WALL, 0.02),
        ),
    ]
}

/// Pure: is a figure at `p` (machine frame) down in the bin?
pub fn in_bin(p: Vec3) -> bool {
    p.x > BIN_FROM && p.z > BIN_FROM && p.y < PIT_FLOOR + BIN_WALL
}

/// Pure: where figure `n` of a fresh pit starts (machine frame), from a seed:
/// somewhere over the pit, never over the bin.
pub fn drop_spot(seed: u64, n: usize) -> Vec3 {
    let r = |k: u64| unit(seed ^ (n as u64 * 7919 + k)) * 2.0 - 1.0;
    let mut at = Vec2::new(r(1), r(2)) * (PIT_HALF - 0.07);
    if at.x > BIN_FROM - 0.06 && at.y > BIN_FROM - 0.06 {
        at = -at;
    }
    Vec3::new(at.x, PIT_FLOOR + 0.15 + 0.08 * (n % 5) as f32, at.y)
}

/// A go starts: take the controls, and fill the pit back up.
fn go(
    ev: On<ClawGo>,
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    assets: Res<AssetServer>,
    time: Res<Time>,
    mut mine: ResMut<MyGo>,
    mut busy: ResMut<HandsBusy>,
    toast: Option<ResMut<crate::hud::Toast>>,
    machines: Query<(Entity, &ClawMachine, Option<&Children>)>,
    prizes: Query<(), With<PitPrize>>,
    parts: Query<(), With<PitPart>>,
) {
    if me.is_none_or(|m| m.0 != ev.to) {
        return;
    }
    let Some((e, _, kids)) = machines.iter().find(|m| m.1.id == ev.machine) else {
        commands.client_trigger(ClawEnd(None));
        return;
    };
    let t = tuning();
    let seed = (time.elapsed_secs_f64() * 1000.0) as u64 ^ ((ev.machine as u64) << 40);
    let kids: Vec<Entity> = kids.map(|k| k.iter().collect()).unwrap_or_default();
    if !kids.iter().any(|k| parts.contains(*k)) {
        for (c, size) in pit_boxes() {
            commands.spawn((
                PitPart,
                Transform::from_translation(c),
                RigidBody::Static,
                Collider::cuboid(size.x, size.y, size.z),
                pit_layers(),
                ChildOf(e),
            ));
        }
        commands.spawn((
            PitPart,
            ClawBall,
            Transform::from_xyz(HOME.x, CLAW_TOP, HOME.y),
            RigidBody::Kinematic,
            Collider::sphere(0.035),
            pit_layers(),
            ChildOf(e),
        ));
    }
    let have = kids.iter().filter(|k| prizes.contains(**k)).count();
    for n in have..t.claw.pit_count {
        let Some(i) = pick(&t, unit(seed ^ (n as u64 * 131 + 17))) else {
            break;
        };
        let Some(p) = t.prizes.get(i as usize) else {
            continue;
        };
        commands.spawn((
            PitPrize(i),
            Transform::from_translation(drop_spot(seed, n)),
            Visibility::default(),
            RigidBody::Dynamic,
            Collider::sphere(t.claw.prize_radius),
            pit_layers(),
            ChildOf(e),
            children![(
                Transform::from_xyz(0.0, -t.claw.prize_radius, 0.0)
                    .with_scale(Vec3::splat(t.claw.prize_scale)),
                model(&assets, &p.model),
            )],
        ));
    }
    *mine = MyGo(Some(Go {
        machine: e,
        phase: Phase::Steer(t.claw.steer_secs),
        held: None,
        won: None,
        seed,
    }));
    busy.0 = true;
    if let Some(mut toast) = toast {
        toast.show(t.claw.steer_line.clone());
    }
}

/// Steering: walk keys or stick move the claw, `claw_drop` drops it,
/// `claw_quit` walks away from the go.
fn steer(
    mut commands: Commands,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    actions: crate::input::Actions,
    virt: Option<Res<crate::controls::VirtualInput>>,
    mut mine: ResMut<MyGo>,
    mut busy: ResMut<HandsBusy>,
    mut poses: Query<&mut ClawPose>,
) {
    let Some(g) = mine.0.as_mut() else { return };
    if actions.just_pressed(crate::input::Act("claw_quit")) {
        commands.client_trigger(ClawEnd(g.won));
        mine.0 = None;
        busy.0 = false;
        return;
    }
    let Phase::Steer(left) = g.phase else { return };
    let Ok(mut pose) = poses.get_mut(g.machine) else {
        return;
    };
    let t = tuning();
    let virt = virt.map(|v| *v).unwrap_or_default();
    let mut stick = Vec2::ZERO;
    for (a, b, dir) in [
        (KeyCode::KeyW, KeyCode::ArrowUp, Vec2::NEG_Y),
        (KeyCode::KeyS, KeyCode::ArrowDown, Vec2::Y),
        (KeyCode::KeyA, KeyCode::ArrowLeft, Vec2::NEG_X),
        (KeyCode::KeyD, KeyCode::ArrowRight, Vec2::X),
    ] {
        if keys.pressed(a) || keys.pressed(b) {
            stick += dir;
        }
    }
    if stick == Vec2::ZERO {
        stick = Vec2::new(virt.stick.x, -virt.stick.y);
    }
    let dt = time.delta_secs();
    pose.at = (pose.at + stick.clamp_length_max(1.0) * t.claw.move_speed * dt)
        .clamp(Vec2::splat(-CLAW_REACH), Vec2::splat(CLAW_REACH));
    pose.shut = 0.0;
    let left = left - dt;
    let drop = actions.just_pressed(crate::input::Act("claw_drop")) || virt.jump;
    g.phase = if drop || left <= 0.0 {
        Phase::Drop
    } else {
        Phase::Steer(left)
    };
}

/// Pure: how low the claw may go over `at` before it sits on something:
/// the floor, or the top of the highest figure under it.
pub fn drop_floor(at: Vec2, figures: &[Vec3], radius: f32) -> f32 {
    let mut low = PIT_FLOOR + GRAB_BELOW - 0.02;
    for f in figures {
        if Vec2::new(f.x, f.z).distance(at) < radius * 1.2 {
            low = low.max(f.y + radius + 0.02);
        }
    }
    low
}

/// The claw's drop, grab, lift and carry back; and the end of the go.
fn run_claw(
    mut commands: Commands,
    time: Res<Time>,
    mut mine: ResMut<MyGo>,
    mut busy: ResMut<HandsBusy>,
    mut poses: Query<&mut ClawPose>,
    mut prizes: Query<
        (
            Entity,
            &mut Transform,
            &mut LinearVelocity,
            &mut AngularVelocity,
            &ChildOf,
        ),
        (With<PitPrize>, Without<ClawBall>),
    >,
    mut balls: Query<(&mut Transform, &ChildOf), With<ClawBall>>,
) {
    let Some(g) = mine.0.as_mut() else { return };
    let Ok(mut pose) = poses.get_mut(g.machine) else {
        commands.client_trigger(ClawEnd(None));
        mine.0 = None;
        busy.0 = false;
        return;
    };
    let t = tuning();
    let c = &t.claw;
    let dt = time.delta_secs();
    let hold_at = |pose: &ClawPose| Vec3::new(pose.at.x, pose.y - GRAB_BELOW, pose.at.y);
    match g.phase {
        Phase::Steer(_) => {}
        Phase::Drop => {
            let figures: Vec<Vec3> = prizes
                .iter()
                .filter(|p| p.4.parent() == g.machine)
                .map(|p| p.1.translation)
                .collect();
            let low = drop_floor(pose.at, &figures, c.prize_radius) + GRAB_BELOW * 0.5;
            pose.y -= c.drop_speed * dt;
            if pose.y <= low {
                pose.y = low.min(CLAW_TOP);
                g.phase = Phase::Close(0.0);
            }
        }
        Phase::Close(s) => {
            let s = s + dt;
            pose.shut = (s / 0.5).min(1.0);
            if s >= 0.5 {
                let grab = hold_at(&pose);
                let nearest = prizes
                    .iter()
                    .filter(|p| p.4.parent() == g.machine)
                    .map(|p| (p.0, p.1.translation.distance(grab)))
                    .filter(|(_, d)| *d < c.grab_radius)
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                if let Some((e, _)) = nearest {
                    commands.entity(e).insert(RigidBody::Kinematic);
                    g.held = Some(e);
                }
                g.phase = Phase::Lift;
            } else {
                g.phase = Phase::Close(s);
            }
        }
        Phase::Lift | Phase::Return => {
            if g.phase == Phase::Lift {
                pose.y = (pose.y + c.lift_speed * dt).min(CLAW_TOP);
                if pose.y >= CLAW_TOP {
                    g.phase = Phase::Return;
                }
            } else {
                let to = HOME - pose.at;
                let step = c.move_speed * dt;
                if to.length() <= step {
                    pose.at = HOME;
                    g.phase = Phase::Release(0.0);
                } else {
                    pose.at += to.normalize() * step;
                }
            }
            // The grip slips, as they do.
            g.seed = g.seed.wrapping_add(1);
            let slips = unit(g.seed.wrapping_mul(31)) < c.slip_per_sec * dt;
            if slips && let Some(e) = g.held.take() {
                commands.entity(e).insert(RigidBody::Dynamic);
                pose.shut = 0.6;
            }
        }
        Phase::Release(s) => {
            if s == 0.0
                && let Some(e) = g.held.take()
            {
                commands.entity(e).insert(RigidBody::Dynamic);
            }
            pose.shut = 0.0;
            let s = s + dt;
            if s >= c.settle_secs {
                commands.client_trigger(ClawEnd(g.won));
                mine.0 = None;
                busy.0 = false;
                return;
            }
            g.phase = Phase::Release(s);
        }
    }
    // What the claw holds rides under it.
    if let Some(held) = g.held
        && let Ok((_, mut tf, mut v, mut w, _)) = prizes.get_mut(held)
    {
        tf.translation = hold_at(&pose);
        v.0 = Vec3::ZERO;
        w.0 = Vec3::ZERO;
    }
    for (mut tf, parent) in &mut balls {
        if parent.parent() == g.machine {
            tf.translation = Vec3::new(pose.at.x, pose.y - 0.03, pose.at.y);
        }
    }
}

/// A figure that lands in the bin is won (the first one a go).
fn catch_in_bin(
    mut commands: Commands,
    mut mine: ResMut<MyGo>,
    prizes: Query<(Entity, &PitPrize, &Transform, &ChildOf)>,
) {
    let Some(g) = mine.0.as_mut() else { return };
    for (e, p, tf, parent) in &prizes {
        if parent.parent() == g.machine && Some(e) != g.held && in_bin(tf.translation) {
            commands.entity(e).despawn();
            if g.won.is_none() {
                g.won = Some(p.0);
            }
        }
    }
}

/// Your eyes on the glass while you play.
fn view(
    mine: Res<MyGo>,
    machines: Query<&GlobalTransform, With<ClawMachine>>,
    mut camera: Query<&mut Transform, With<crate::world::MainCamera>>,
) {
    let Some(g) = mine.0.as_ref() else { return };
    let (Ok(m), Ok(mut cam)) = (machines.get(g.machine), camera.single_mut()) else {
        return;
    };
    let eye = m.transform_point(EYE);
    *cam = Transform::from_translation(eye).looking_at(m.transform_point(LOOK), Vec3::Y);
}

/// The claw's buttons and prompt bar while you play.
fn claw_context(mine: Res<MyGo>, mut ctx: ResMut<crate::input::InputContexts>) {
    crate::input::switch(&mut ctx, "claw", mine.0.is_some());
}

fn stop_go(mut mine: ResMut<MyGo>, mut busy: ResMut<HandsBusy>) {
    mine.0 = None;
    busy.0 = false;
}

fn result(
    ev: On<ClawResult>,
    me: Option<Res<LocalPlayer>>,
    mut shelf: ResMut<Shelf>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    let t = tuning();
    let prize = ev.prize.and_then(|i| t.prizes.get(i as usize));
    match prize {
        Some(p) => {
            if ev.to == me.0 {
                shelf.add(&p.name);
                crate::settings::save_local(SHELF_FILE, &shelf.write());
            }
            let who = if ev.to == me.0 {
                "You".to_string()
            } else {
                format!("Player {}", ev.to)
            };
            toast.show(fill(
                &t.win_line,
                &[("who", who), ("prize", p.name.clone())],
            ));
        }
        None if ev.to == me.0 => toast.show(t.miss_line.clone()),
        None => {}
    }
}

#[derive(Component)]
struct CoinsText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        CoinsText,
        crate::hud::slot(crate::hud::HudSlot::BottomLeft),
        DespawnOnExit(AppState::InGame),
    ));
}

fn show_coins(
    me: Option<Res<LocalPlayer>>,
    purses: Query<(&Player, &HeldPrize)>,
    mut text: Query<&mut Text, With<CoinsText>>,
) {
    let Some(me) = me else { return };
    let Some((_, held)) = purses.iter().find(|p| p.0.id == me.0) else {
        return;
    };
    let t = tuning();
    // The coins themselves show in the money bubble (wallet.rs); this just
    // names the figure in your hand.
    let mut want = String::new();
    if let Some(p) = held.prize.and_then(|i| t.prizes.get(i as usize)) {
        want = format!("holding {}", p.name);
    }
    for mut line in &mut text {
        if line.0 != want {
            line.0.clone_from(&want);
        }
    }
}

fn cycle_held(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    me: Option<Res<LocalPlayer>>,
    shelf: Res<Shelf>,
    held: Query<(&Player, &HeldPrize)>,
) {
    if !keys.just_pressed(KeyCode::Period) {
        return;
    }
    let Some(me) = me else { return };
    let now = held.iter().find(|h| h.0.id == me.0).and_then(|h| h.1.prize);
    commands.client_trigger(HoldPrize(shelf.next_held(&tuning().prizes, now)));
}

/// A figure drawn in someone's hand.
#[derive(Component)]
struct HeldFigure {
    player: u64,
    prize: u8,
}

fn draw_held(
    mut commands: Commands,
    assets: Res<AssetServer>,
    me: Option<Res<LocalPlayer>>,
    holders: Query<(&Player, &HeldPrize, Option<&Walker>)>,
    mut figures: Query<(Entity, &HeldFigure, &mut Transform)>,
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
) {
    let t = tuning();
    let me = me.map(|m| m.0);
    // Drop figures whose holder let go or swapped.
    for (e, f, _) in &figures {
        let still = holders
            .iter()
            .any(|(p, h, _)| p.id == f.player && h.prize == Some(f.prize));
        if !still {
            commands.entity(e).despawn();
        }
    }
    for (p, h, walker) in &holders {
        let Some(prize) = h.prize else { continue };
        let Some(def) = t.prizes.get(prize as usize) else {
            continue;
        };
        let place = if Some(p.id) == me {
            camera.iter().next().map(|c| {
                let c = c.compute_transform();
                Transform::from_translation(c.transform_point(VIEW_HAND))
                    .with_rotation(c.rotation * Quat::from_rotation_y(0.6))
            })
        } else {
            walker.map(|w| {
                let rot = Quat::from_rotation_y(w.yaw);
                Transform::from_translation(w.pos + rot * BODY_HAND).with_rotation(rot)
            })
        };
        let Some(place) = place.map(|t| t.with_scale(Vec3::splat(HAND_SCALE))) else {
            continue;
        };
        match figures
            .iter_mut()
            .find(|(_, f, _)| f.player == p.id && f.prize == prize)
        {
            Some((_, _, mut tf)) => *tf = place,
            None => {
                commands.spawn((
                    HeldFigure {
                        player: p.id,
                        prize,
                    },
                    place,
                    Visibility::default(),
                    WorldAssetRoot(
                        assets.load(GltfAssetLabel::Scene(0).from_asset(def.model.clone())),
                    ),
                    DespawnOnExit(AppState::InGame),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() -> std::sync::Arc<Tuning> {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        TUNING.get()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = load();
        assert!(t.price > 0);
        let c = &t.claw;
        assert!(c.steer_secs > 0.0 && c.move_speed > 0.0 && c.drop_speed > 0.0);
        assert!(c.lift_speed > 0.0 && c.grab_radius > 0.0 && c.settle_secs > 0.0);
        assert!((0.0..1.0).contains(&c.slip_per_sec) && c.pit_count > 0);
        assert!(c.prize_radius > 0.02 && c.prize_radius < 0.1 && c.prize_scale > 0.0);
        assert!(c.steer_line.contains("{claw_drop}"));
        assert!(t.held_speed > 0.0 && t.held_speed < 0.5, "only a lil");
        assert!(!t.prizes.is_empty() && t.prizes.len() < 255);
        for p in &t.prizes {
            assert!(!p.name.is_empty() && !p.blurb.is_empty() && p.weight > 0.0);
            assert!(p.model.ends_with(".gltf"));
        }
        assert!(!t.win_line.is_empty() && !t.miss_line.is_empty());
        assert!(!t.broke_line.is_empty());
    }

    fn two() -> Tuning {
        let p = |name: &str, weight| Prize {
            name: name.into(),
            blurb: "b".into(),
            model: "m.gltf".into(),
            weight,
        };
        Tuning {
            prizes: vec![p("a", 3.0), p("b", 1.0)],
            ..default()
        }
    }

    #[test]
    fn the_pit_picks_figures_by_weight() {
        let t = two();
        assert_eq!(pick(&t, 0.1), Some(0));
        assert_eq!(pick(&t, 0.74), Some(0));
        assert_eq!(pick(&t, 0.76), Some(1));
        assert_eq!(pick(&t, 1.0), Some(1));
    }

    #[test]
    fn figures_start_in_the_pit_and_never_in_the_bin() {
        for seed in 0..50u64 {
            for n in 0..12 {
                let p = drop_spot(seed * 977, n);
                assert!(p.x.abs() < PIT_HALF && p.z.abs() < PIT_HALF, "{p}");
                assert!(!(p.x > BIN_FROM && p.z > BIN_FROM), "{p}");
                assert!(p.y > PIT_FLOOR);
            }
        }
        assert!(in_bin(Vec3::new(0.3, PIT_FLOOR + 0.05, 0.3)));
        assert!(!in_bin(Vec3::new(0.3, PIT_FLOOR + 0.3, 0.3)));
    }

    #[test]
    fn the_claw_comes_to_rest_on_what_is_under_it() {
        let empty = drop_floor(Vec2::ZERO, &[], 0.05);
        assert!(empty > PIT_FLOOR && empty < PIT_FLOOR + GRAB_BELOW);
        let on = drop_floor(Vec2::ZERO, &[Vec3::new(0.01, PIT_FLOOR + 0.05, 0.0)], 0.05);
        assert!(on > PIT_FLOOR + 0.1);
        let beside = drop_floor(Vec2::ZERO, &[Vec3::new(0.3, PIT_FLOOR + 0.05, 0.0)], 0.05);
        assert_eq!(beside, empty);
        // The home is over the bin and inside the claw's reach.
        assert!(in_bin(Vec3::new(HOME.x, PIT_FLOOR, HOME.y)));
        assert!(HOME.max_element() <= CLAW_REACH);
    }

    #[test]
    fn prongs_open_out_and_close_in() {
        let open = prong_transform(0, 0.0).transform_point(Vec3::new(0.0, -0.12, 0.0));
        let shut = prong_transform(0, 1.0).transform_point(Vec3::new(0.0, -0.12, 0.0));
        assert!(open.x > shut.x + 0.05);
    }

    fn read<T: serde::de::DeserializeOwned + Default + Send + Sync + 'static>(
        d: &friendslop_data::Data<T>,
    ) {
        let path = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), d.path());
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(d.set_bytes(&bytes), "{path}");
    }

    #[test]
    fn there_is_a_machine_in_every_boba_shop_and_subway_station() {
        let _ = load();
        read(&crate::features::boba::TUNING);
        read(&crate::features::trains::TUNING);
        let spots = machine_spots(&tuning());
        let shops = crate::features::boba::tuning().shops.clone();
        let stops = crate::features::trains::tuning()
            .lines
            .iter()
            .filter(|l| l.vehicle == crate::features::trains::Vehicle::Train)
            .count()
            * 2;
        assert!(shops.len() >= 3 && stops >= 4);
        assert_eq!(spots.len(), 1 + shops.len() + stops);
        for (i, shop) in shops.iter().enumerate() {
            let at = spots[1 + i].0 + Vec3::Y * 0.5;
            assert!(crate::features::boba::inside(shop, at), "{}", shop.name);
        }
    }

    #[test]
    fn the_shelf_round_trips_and_cycles_what_you_own() {
        let t = two();
        let mut s = Shelf::default();
        assert_eq!(s.next_held(&t.prizes, None), None);
        s.add("b");
        s.add("b");
        s.add("a");
        assert_eq!(Shelf::parse(&s.write()), s);
        assert_eq!(s.count("b"), 2);
        assert_eq!(s.next_held(&t.prizes, None), Some(0));
        assert_eq!(s.next_held(&t.prizes, Some(0)), Some(1));
        assert_eq!(s.next_held(&t.prizes, Some(1)), None);
    }
}
