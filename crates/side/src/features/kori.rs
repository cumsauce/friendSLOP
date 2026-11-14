//! Kori coins and the claw machine (#382, catherine): "a currency in the game
//! named kori coins that u earn from interacting with farts and poop and other
//! things of that nature that let u play a claw machine for little collectible
//! figures and stuff. spot in main menu for tracking what kori coin claw
//! machine prizes u have and to see them and inspect them. you can also equip
//! them in your hand and walk around with them like csgo knives and they speed
//! u up a lil".
//!
//! - **Earning** (server): every fart pays [`KoriCoins`] (more if an NPC was
//!   shamed by it), and eating a toilet's feces pays more (`toilets.rs`).
//!   Amounts: `data/base/kori.ron`. Your coins show bottom left.
//! - **The claw machine** stands by the gambling table. Click it to pay
//!   [`Tuning::price`] coins for a go; it wins [`Tuning::win_chance`] of the
//!   time, a figure picked by weight from [`Tuning::prizes`].
//! - **Your shelf** ([`Shelf`]): what you've won is kept on your own machine
//!   (`kori_prizes.txt`), across games. The main menu's "Kori prizes" page lists
//!   them and spins one up to inspect (`crates/game/src/kori_shelf.rs`).
//! - **In hand** (.): cycles through the figures you own and empty hands.
//!   Everyone sees it in your hand, and it adds [`Tuning::held_speed`] to your
//!   walk (`rules::kori::HeldPrize`).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
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
    /// Relative odds of the claw grabbing this one.
    pub weight: f32,
}

/// Tuning and writing: `assets/data/side/kori.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Coins a go costs.
    pub price: u32,
    /// Chance (0..1) a go wins something.
    pub win_chance: f32,
    /// Walking speed a figure in hand adds (0.1 = +10%).
    pub held_speed: f32,
    /// The machine stands this far (x, z metres) from the gambling table...
    pub offset: (f32, f32),
    /// ...turned this much (degrees).
    pub yaw_deg: f32,
    /// Every figure.
    pub prizes: Vec<Prize>,
    /// Shown to everyone when someone wins: `{who}`, `{prize}`.
    pub win_line: String,
    /// Shown to you when the claw drops it.
    pub miss_line: String,
    /// Shown when you can't pay: `{price}`, `{coins}`.
    pub broke_line: String,
    /// The coins line, bottom left: `{coins}`.
    pub coins_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/kori.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

pub const MACHINE_MODEL: &str = "models/props/claw_machine.gltf";
/// Collider half sizes (the cabinet is ~0.9 x 1.9 x 0.9 m).
const HALF: Vec3 = Vec3::new(0.45, 0.95, 0.45);
/// Where a held figure sits in front of your own camera.
const VIEW_HAND: Vec3 = Vec3::new(0.22, -0.2, -0.45);
/// Where it sits on someone else's body (feet-relative, before their yaw).
const BODY_HAND: Vec3 = Vec3::new(0.3, 1.0, -0.3);
/// Figures are shown this much bigger in hand than on the shelf.
const HAND_SCALE: f32 = 1.4;

/// Pure: where the machine stands (x, z) and its yaw.
pub fn machine_spot(t: &Tuning) -> (Vec2, f32) {
    (
        friendslop_world::places::gamble_at() + Vec2::new(t.offset.0, t.offset.1),
        t.yaw_deg.to_radians(),
    )
}

/// The claw machine. Replicated; the server spawns it.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct ClawMachine {
    pub at: Vec3,
    pub yaw: f32,
}

/// Client → server: one go on the claw.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PlayClaw;

/// Client → server: hold this prize (an index into the list), or nothing.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct HoldPrize(pub Option<u8>);

/// Server → everyone: player `to` had a go; `prize` if they won one.
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct ClawResult {
    pub to: u64,
    pub prize: Option<u8>,
}

pub fn protocol(app: &mut App) {
    app.replicate::<KoriCoins>()
        .replicate::<HeldPrize>()
        .replicate::<ClawMachine>()
        .add_client_event::<PlayClaw>(Channel::Ordered)
        .add_client_event::<HoldPrize>(Channel::Ordered)
        .add_server_event::<ClawResult>(Channel::Ordered)
        .add_observer(play)
        .add_observer(hold)
        .add_observer(fart_pays)
        .add_systems(
            Update,
            (give_purses, place_machine).run_if(in_state(ServerState::Running)),
        );
    let (at, _) = machine_spot(&tuning());
    friendslop_world::footprints::claim(at, 1.5);
}

/// Pure: a cheap hash to a number in 0..1.
fn unit(seed: u64) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// Pure: what one go gives, from two rolls in 0..1 (win?, which?).
pub fn roll(t: &Tuning, win: f32, which: f32) -> Option<u8> {
    if win >= t.win_chance {
        return None;
    }
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
        out = out.replace(&format!("{{{k}}}"), v);
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

fn place_machine(mut commands: Commands, machines: Query<(), With<ClawMachine>>) {
    if !machines.is_empty() {
        return;
    }
    let (p, yaw) = machine_spot(&tuning());
    commands.spawn((
        ClawMachine {
            at: Vec3::new(p.x, crate::terrain::height_at(p.x, p.y), p.y),
            yaw,
        },
        Replicated,
    ));
}

/// Server: every fart pays (it's a server event the host hears too).
fn fart_pays(ev: On<crate::features::fart::Farted>, mut purses: Query<(&Player, &mut KoriCoins)>) {
    let e = crate::rules::kori::earnings();
    let Some((_, coins)) = purses.iter_mut().find(|(p, _)| p.id == ev.player) else {
        return;
    };
    let n = e.fart + if ev.shamed { e.shameful_fart } else { 0 };
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
    time: Res<Time>,
    mut goes: Local<u64>,
    walkers: Query<(&PlayerOwner, &Walker)>,
    machines: Query<&ClawMachine>,
    mut purses: Query<(&Player, &PlayerOwner, &mut KoriCoins)>,
) {
    let t = tuning();
    let Some(machine) = machines.iter().next() else {
        return;
    };
    if !in_reach(&mut commands, ev.client_id, &walkers, machine.at) {
        return;
    }
    let owner = owner_of(ev.client_id);
    let Some((player, _, mut coins)) = purses.iter_mut().find(|p| p.1.0 == owner) else {
        return;
    };
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
    *goes += 1;
    let seed = *goes ^ (time.elapsed_secs_f64() * 1000.0) as u64 ^ (player.id << 32);
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: ClawResult {
            to: player.id,
            prize: roll(&t, unit(seed), unit(seed ^ 0xABCD)),
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
    app.insert_resource(Shelf::load())
        .add_observer(click)
        .add_observer(result)
        .add_systems(OnEnter(AppState::InGame), spawn_ui)
        .add_systems(
            Update,
            (dress, show_coins, cycle_held, draw_held).run_if(in_state(AppState::InGame)),
        );
}

fn dress(
    mut commands: Commands,
    assets: Res<AssetServer>,
    new: Query<(Entity, &ClawMachine), Added<ClawMachine>>,
) {
    for (e, m) in &new {
        commands.entity(e).insert((
            Transform::from_translation(m.at).with_rotation(Quat::from_rotation_y(m.yaw)),
            Visibility::default(),
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(MACHINE_MODEL))),
            Clickable::at_height(1.0).labelled("play the Kori claw"),
            DespawnOnExit(AppState::InGame),
            children![(
                Transform::from_translation(Vec3::Y * HALF.y),
                avian3d::prelude::RigidBody::Static,
                avian3d::prelude::Collider::cuboid(HALF.x * 2.0, HALF.y * 2.0, HALF.z * 2.0),
            )],
        ));
    }
}

fn click(c: On<Clicked>, mut commands: Commands, machines: Query<(), With<ClawMachine>>) {
    if machines.contains(c.entity) {
        commands.client_trigger(PlayClaw);
    }
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
    purses: Query<(&Player, &KoriCoins, &HeldPrize)>,
    mut text: Query<&mut Text, With<CoinsText>>,
) {
    let Some(me) = me else { return };
    let Some((_, coins, held)) = purses.iter().find(|p| p.0.id == me.0) else {
        return;
    };
    let t = tuning();
    let mut want = fill(&t.coins_line, &[("coins", coins.0.to_string())]);
    if let Some(p) = held.prize.and_then(|i| t.prizes.get(i as usize)) {
        want += &format!("  ·  holding {}", p.name);
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
        assert!(t.price > 0 && t.win_chance > 0.0 && t.win_chance <= 1.0);
        assert!(t.held_speed > 0.0 && t.held_speed < 0.5, "only a lil");
        assert!(!t.prizes.is_empty() && t.prizes.len() < 255);
        for p in &t.prizes {
            assert!(!p.name.is_empty() && !p.blurb.is_empty() && p.weight > 0.0);
            assert!(p.model.ends_with(".gltf"));
        }
        assert!(!t.win_line.is_empty() && !t.miss_line.is_empty());
        assert!(!t.broke_line.is_empty() && t.coins_line.contains("{coins}"));
    }

    fn two() -> Tuning {
        let p = |name: &str, weight| Prize {
            name: name.into(),
            blurb: "b".into(),
            model: "m.gltf".into(),
            weight,
        };
        Tuning {
            win_chance: 0.5,
            prizes: vec![p("a", 3.0), p("b", 1.0)],
            ..default()
        }
    }

    #[test]
    fn the_claw_wins_sometimes_and_picks_by_weight() {
        let t = two();
        assert_eq!(roll(&t, 0.6, 0.1), None);
        assert_eq!(roll(&t, 0.1, 0.1), Some(0));
        assert_eq!(roll(&t, 0.1, 0.74), Some(0));
        assert_eq!(roll(&t, 0.1, 0.76), Some(1));
        assert_eq!(roll(&t, 0.1, 1.0), Some(1));
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
