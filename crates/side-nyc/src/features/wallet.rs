//! Money and the wallet you lose on the subway (#322, doomy: "money mechanic.
//! random chance of losing your wallet on the subway. you need to find it in
//! fulton station if that happens. if you don't recover it you lose ur money
//! temporarily but get it back through chinese insurance via a dji drone that
//! flies to you however if you choose this they take a 70% cut. fulton can let
//! you double your payout or more by finding other scattered wallets. money is
//! personal to each user unless they brag with brag button. bragging
//! exponentially leads to more chance of coins falling out that can be picked
//! up by others.")
//!
//! Every player carries [`Cash`] (dollars, replicated, starts at
//! [`Tuning::start_cash`]). Only you see yours, bottom right.
//!
//! - **Losing it** (server): each time you go down into Fulton St there's a
//!   [`Tuning::lose_chance`] that your wallet (all your cash) falls out
//!   somewhere in the station's top [`Tuning::lose_levels`] levels.
//! - **Finding it**: walk over a wallet to pick it up. Yours gives you your
//!   money back; strangers' wallets ([`Tuning::strangers`] of them, scattered
//!   through the station at the start) are extra cash for whoever finds them.
//!   Someone else can pick up your wallet too, and keep it.
//! - **Insurance** (I): while your wallet is lost, call the insurer. A drone
//!   flies to you, wherever you are, and hands you
//!   [`Tuning::insurance_share`] of what was in it; your wallet is written off
//!   and vanishes.
//! - **Bragging** (B): shows everyone your cash and makes it more likely to
//!   fall out. Each brag doubles the chance a second that a coin drops at your
//!   feet; anyone else walking over a coin takes it. Bragging lasts the game.
//!
//! The rules are pure functions ([`lost_on_entering`], [`drop_chance`],
//! [`payout`]); the systems are wiring.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_wfc as wfc;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::fulton::{self, Station};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::rules::nyc::in_station;

/// Tuning and writing: `assets/data/side/wallet.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Dollars everyone starts with.
    pub start_cash: u32,
    /// Chance (0..1) your wallet falls out each time you go into the subway.
    pub lose_chance: f32,
    /// It lands somewhere on the station's levels 0..this.
    pub lose_levels: usize,
    /// Strangers' wallets scattered through the station at the start.
    pub strangers: usize,
    /// ...each holding between these many dollars.
    pub stranger_cash: (u32, u32),
    /// ...on levels 0..this.
    pub stranger_levels: usize,
    /// Walk this close (m) to a wallet or coin to pick it up.
    pub pickup_reach: f32,
    /// Share (0..1) of your lost cash the insurance drone brings (they take the rest).
    pub insurance_share: f32,
    /// The drone starts this far (m) above and behind you and flies this fast (m/s).
    pub drone_distance: f32,
    pub drone_speed: f32,
    /// A coin's chance a second of falling out after one brag; each brag doubles it.
    pub brag_chance: f32,
    /// Bragging stops counting after this many brags.
    pub max_brags: u8,
    /// Dollars in one dropped coin.
    pub coin_value: u32,
    /// Toasts (`{cash}`, `{who}`, `{share}`).
    pub lost_line: String,
    pub found_line: String,
    pub found_other_line: String,
    pub insure_line: String,
    pub insured_line: String,
    pub no_claim_line: String,
    pub brag_line: String,
    pub coin_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/wallet.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// A player's money. `lost` is what's in their wallet while it's lost.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Cash {
    pub dollars: u32,
    pub lost: u32,
    /// Times they've bragged (0 = their cash is private).
    pub brags: u8,
    /// An insurance drone is on its way.
    pub claimed: bool,
}

/// A wallet lying in the station. `owner` is `None` for a stranger's.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Wallet {
    pub owner: Option<u64>,
    pub dollars: u32,
    pub at: Vec3,
}

/// A coin that fell out of a bragger's pocket (`from` can't pick it back up).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Coin {
    pub from: u64,
    pub dollars: u32,
    pub at: Vec3,
}

/// The insurance drone flying to player `to`.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Drone {
    pub to: u64,
    pub at: Vec3,
}

/// Client → server: call the insurance.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Insure;

/// Client → server: brag.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Brag;

/// Server → clients: a toast for one player (`to`), or everyone (`None`).
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct WalletNews {
    pub to: Option<u64>,
    pub text: String,
}

// ------------------------------------------------------------------ rules

/// Does your wallet fall out going into the subway (`roll` in 0..1)?
pub fn lost_on_entering(roll: f32, chance: f32, dollars: u32) -> bool {
    dollars > 0 && roll < chance
}

/// A bragger's chance a second of dropping a coin: doubles with every brag.
pub fn drop_chance(brags: u8, base: f32, max_brags: u8) -> f32 {
    if brags == 0 {
        return 0.0;
    }
    let n = brags.min(max_brags.max(1)) - 1;
    (base * 2f32.powi(n as i32)).min(1.0)
}

/// What the insurance drone brings back.
pub fn payout(lost: u32, share: f32) -> u32 {
    (lost as f32 * share.clamp(0.0, 1.0)).floor() as u32
}

/// A random walkable spot in the station on levels `0..levels`.
pub fn spot_in_station(
    t: &fulton::Tuning,
    station: &[fulton::Level],
    levels: usize,
    rng: &mut wfc::Rng,
) -> Option<Vec3> {
    let n = levels.min(station.len());
    if n == 0 {
        return None;
    }
    let li = rng.below(n);
    let l = &station[li];
    // Only cells you can actually walk to from where you arrive.
    let reach: Vec<usize> = l
        .distances()
        .iter()
        .enumerate()
        .filter_map(|(i, d)| d.map(|_| i))
        .collect();
    let i = *reach.get(rng.below(reach.len().max(1)))?;
    let c = (i % l.width, i / l.width);
    let jitter = Vec3::new(rng.roll() - 0.5, 0.0, rng.roll() - 0.5) * 3.0;
    Some(fulton::cell_world(t, li, c) + jitter + Vec3::Y * 0.02)
}

fn fill(line: &str, cash: u32, who: u64, share: f32) -> String {
    line.replace("{cash}", &cash.to_string())
        .replace("{who}", &format!("player {who}"))
        .replace("{share}", &format!("{:.0}", share * 100.0))
}

// ------------------------------------------------------------------ server

#[derive(Resource)]
struct Dice(wfc::Rng);

impl Default for Dice {
    fn default() -> Self {
        Dice(wfc::Rng::new(
            crate::settings::since_epoch().as_millis() as u32 | 1,
        ))
    }
}

/// Server: players who were in the station last frame, and whether the strangers' wallets are out.
#[derive(Resource, Default)]
struct Below {
    in_station: std::collections::HashSet<u64>,
    scattered: bool,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Cash>()
        .replicate::<Wallet>()
        .replicate::<Coin>()
        .replicate::<Drone>()
        .add_client_event::<Insure>(Channel::Ordered)
        .add_client_event::<Brag>(Channel::Ordered)
        .add_server_event::<WalletNews>(Channel::Ordered)
        .init_resource::<Dice>()
        .init_resource::<Below>()
        .add_observer(give_cash)
        .add_observer(insure)
        .add_observer(brag)
        .add_systems(OnEnter(AppState::InGame), |mut b: ResMut<Below>| {
            *b = Below::default()
        })
        .add_systems(
            Update,
            (scatter, lose, pick_up, drop_coins, fly_drones)
                .run_if(in_state(ServerState::Running))
                .run_if(resource_exists::<Station>),
        );
}

fn news(commands: &mut Commands, to: Option<u64>, text: String) {
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: WalletNews { to, text },
    });
}

fn give_cash(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() == ServerState::Running {
        commands.entity(add.entity).insert(Cash {
            dollars: tuning().start_cash,
            ..default()
        });
    }
}

fn scatter(
    mut commands: Commands,
    station: Res<Station>,
    mut below: ResMut<Below>,
    mut dice: ResMut<Dice>,
) {
    if below.scattered {
        return;
    }
    below.scattered = true;
    let (t, ft) = (tuning(), fulton::tuning());
    let (lo, hi) = (
        t.stranger_cash.0.min(t.stranger_cash.1),
        t.stranger_cash.0.max(t.stranger_cash.1),
    );
    for _ in 0..t.strangers {
        let Some(at) = spot_in_station(&ft, &station.levels, t.stranger_levels, &mut dice.0) else {
            return;
        };
        let dollars = lo + dice.0.below((hi - lo + 1) as usize) as u32;
        commands.spawn((
            Replicated,
            Wallet {
                owner: None,
                dollars,
                at,
            },
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Server: going into the subway, your wallet may fall out somewhere below.
fn lose(
    mut commands: Commands,
    station: Res<Station>,
    mut below: ResMut<Below>,
    mut dice: ResMut<Dice>,
    mut players: Query<(&Player, &Walker, &mut Cash)>,
) {
    let (t, ft) = (tuning(), fulton::tuning());
    for (p, w, mut cash) in &mut players {
        let now = in_station(w.pos);
        let arrived = now && below.in_station.insert(p.id);
        if !now {
            below.in_station.remove(&p.id);
        }
        if !arrived
            || cash.lost > 0
            || !lost_on_entering(dice.0.roll(), t.lose_chance, cash.dollars)
        {
            continue;
        }
        let Some(at) = spot_in_station(&ft, &station.levels, t.lose_levels, &mut dice.0) else {
            continue;
        };
        let dollars = cash.dollars;
        cash.dollars = 0;
        cash.lost = dollars;
        commands.spawn((
            Replicated,
            Wallet {
                owner: Some(p.id),
                dollars,
                at,
            },
            DespawnOnExit(AppState::InGame),
        ));
        news(
            &mut commands,
            Some(p.id),
            fill(&t.lost_line, dollars, p.id, t.insurance_share),
        );
    }
}

/// Server: walk over a wallet or someone else's coin and it's yours.
fn pick_up(
    mut commands: Commands,
    wallets: Query<(Entity, &Wallet)>,
    coins: Query<(Entity, &Coin)>,
    mut players: Query<(&Player, &Walker, &mut Cash)>,
) {
    let t = tuning();
    let near =
        |a: Vec3, b: Vec3| (a.y - b.y).abs() < 2.0 && a.xz().distance(b.xz()) < t.pickup_reach;
    for (e, wallet) in &wallets {
        let Some((p, _, _)) = players.iter().find(|(_, w, _)| near(w.pos, wallet.at)) else {
            continue;
        };
        let finder = p.id;
        commands.entity(e).despawn();
        for (q, _, mut cash) in &mut players {
            if q.id == finder {
                cash.dollars += wallet.dollars;
            }
            if Some(q.id) == wallet.owner && q.id == finder {
                cash.lost = 0;
            }
        }
        let line = if wallet.owner == Some(finder) {
            &t.found_line
        } else {
            &t.found_other_line
        };
        news(
            &mut commands,
            Some(finder),
            fill(
                line,
                wallet.dollars,
                wallet.owner.unwrap_or(0),
                t.insurance_share,
            ),
        );
    }
    for (e, coin) in &coins {
        let Some((_, _, mut cash)) = players
            .iter_mut()
            .find(|(p, w, _)| p.id != coin.from && near(w.pos, coin.at))
        else {
            continue;
        };
        cash.dollars += coin.dollars;
        commands.entity(e).despawn();
    }
}

/// Server: braggers' coins fall out.
fn drop_coins(
    mut commands: Commands,
    time: Res<Time>,
    mut dice: ResMut<Dice>,
    mut players: Query<(&Player, &Walker, &mut Cash)>,
) {
    let t = tuning();
    for (p, w, mut cash) in &mut players {
        let chance = drop_chance(cash.brags, t.brag_chance, t.max_brags) * time.delta_secs();
        if cash.dollars == 0 || dice.0.roll() >= chance {
            continue;
        }
        let dollars = t.coin_value.max(1).min(cash.dollars);
        cash.dollars -= dollars;
        let a = dice.0.roll() * std::f32::consts::TAU;
        let at = w.pos + Vec3::new(a.cos(), 0.02, a.sin()) * 1.5;
        commands.spawn((
            Replicated,
            Coin {
                from: p.id,
                dollars,
                at,
            },
            DespawnOnExit(AppState::InGame),
        ));
        news(&mut commands, Some(p.id), t.coin_line.clone());
    }
}

fn sender(
    client: ClientId,
    players: &Query<(&Player, &PlayerOwner, &mut Cash, &Walker)>,
) -> Option<u64> {
    let owner = match client {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    players
        .iter()
        .find(|(_, o, _, _)| o.0 == owner)
        .map(|(p, ..)| p.id)
}

fn insure(
    ev: On<FromClient<Insure>>,
    mut commands: Commands,
    mut players: Query<(&Player, &PlayerOwner, &mut Cash, &Walker)>,
) {
    let Some(id) = sender(ev.client_id, &players) else {
        return;
    };
    let t = tuning();
    let Some((_, _, mut cash, w)) = players.iter_mut().find(|(p, ..)| p.id == id) else {
        return;
    };
    if cash.lost == 0 || cash.claimed {
        news(&mut commands, Some(id), t.no_claim_line.clone());
        return;
    }
    cash.claimed = true;
    let at = w.pos + Vec3::new(-1.0, 1.0, -1.0).normalize() * t.drone_distance;
    commands.spawn((
        Replicated,
        Drone { to: id, at },
        DespawnOnExit(AppState::InGame),
    ));
    news(
        &mut commands,
        Some(id),
        fill(
            &t.insure_line,
            payout(cash.lost, t.insurance_share),
            id,
            t.insurance_share,
        ),
    );
}

fn brag(
    ev: On<FromClient<Brag>>,
    mut commands: Commands,
    mut players: Query<(&Player, &PlayerOwner, &mut Cash, &Walker)>,
) {
    let Some(id) = sender(ev.client_id, &players) else {
        return;
    };
    let t = tuning();
    let Some((_, _, mut cash, _)) = players.iter_mut().find(|(p, ..)| p.id == id) else {
        return;
    };
    cash.brags = cash.brags.saturating_add(1).min(t.max_brags.max(1));
    news(
        &mut commands,
        None,
        fill(&t.brag_line, cash.dollars, id, t.insurance_share),
    );
}

/// Server: the drone flies straight to you (through walls: it's very good) and pays out.
fn fly_drones(
    mut commands: Commands,
    time: Res<Time>,
    mut drones: Query<(Entity, &mut Drone)>,
    wallets: Query<(Entity, &Wallet)>,
    mut players: Query<(&Player, &Walker, &mut Cash)>,
) {
    let t = tuning();
    for (e, mut d) in &mut drones {
        let Some((_, w, mut cash)) = players.iter_mut().find(|(p, ..)| p.id == d.to) else {
            commands.entity(e).despawn();
            continue;
        };
        let target = w.pos + Vec3::Y * 1.8;
        let step = t.drone_speed * time.delta_secs();
        if d.at.distance(target) > step.max(0.8) {
            d.at = d.at.move_towards(target, step);
            continue;
        }
        let paid = payout(cash.lost, t.insurance_share);
        cash.dollars += paid;
        cash.lost = 0;
        cash.claimed = false;
        commands.entity(e).despawn();
        for (we, wallet) in &wallets {
            if wallet.owner == Some(d.to) {
                commands.entity(we).despawn();
            }
        }
        news(
            &mut commands,
            Some(d.to),
            fill(&t.insured_line, paid, d.to, t.insurance_share),
        );
    }
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_observer(show_news)
        .add_systems(OnEnter(AppState::InGame), spawn_ui)
        .add_systems(
            Update,
            (keys, show_cash, dress, place_drones).run_if(in_state(AppState::InGame)),
        );
}

fn show_news(ev: On<WalletNews>, me: Option<Res<LocalPlayer>>, toast: Option<ResMut<Toast>>) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    if ev.to.is_none_or(|to| to == me.0) {
        toast.show(ev.text.clone());
    }
}

fn keys(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::KeyB) {
        commands.client_trigger(Brag);
    }
    if keys.just_pressed(KeyCode::KeyI) {
        commands.client_trigger(Insure);
    }
}

#[derive(Component)]
struct CashText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        CashText,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Pure: the money line. Yours, then everyone who's bragging.
pub fn cash_line(me: u64, all: &[(u64, Cash)]) -> String {
    let mut out = String::new();
    if let Some((_, c)) = all.iter().find(|(id, _)| *id == me) {
        out = format!("${}", c.dollars);
        if c.lost > 0 {
            out += &format!(
                "  (wallet lost in Fulton St: ${}{})",
                c.lost,
                if c.claimed {
                    ", drone coming"
                } else {
                    ", I: insurance"
                }
            );
        }
    }
    for (id, c) in all {
        if *id != me && c.brags > 0 {
            out += &format!("\nplayer {id} brags: ${}", c.dollars);
        }
    }
    out
}

fn show_cash(
    me: Option<Res<LocalPlayer>>,
    purses: Query<(&Player, &Cash)>,
    mut text: Query<&mut Text, With<CashText>>,
) {
    let Some(me) = me else { return };
    let all: Vec<(u64, Cash)> = purses.iter().map(|(p, c)| (p.id, *c)).collect();
    let want = cash_line(me.0, &all);
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

#[derive(Component)]
struct Dressed;

fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

/// Wallets and coins get their models (drawn bigger than life so you can spot them).
fn dress(
    mut commands: Commands,
    assets: Res<AssetServer>,
    wallets: Query<(Entity, &Wallet), Without<Dressed>>,
    coins: Query<(Entity, &Coin), Without<Dressed>>,
    drones: Query<(Entity, &Drone), Without<Dressed>>,
) {
    for (e, w) in &wallets {
        commands.entity(e).insert((
            Dressed,
            model(&assets, "fs-wallet.gltf"),
            Transform::from_translation(w.at)
                .with_rotation(Quat::from_rotation_y(w.at.x * 7.0))
                .with_scale(Vec3::splat(2.5)),
            Visibility::default(),
        ));
    }
    for (e, c) in &coins {
        commands.entity(e).insert((
            Dressed,
            model(&assets, "fs-coin.gltf"),
            Transform::from_translation(c.at).with_scale(Vec3::splat(4.0)),
            Visibility::default(),
        ));
    }
    for (e, d) in &drones {
        commands.entity(e).insert((
            Dressed,
            model(&assets, "fs-insurance-drone.gltf"),
            Transform::from_translation(d.at),
            Visibility::default(),
        ));
    }
}

fn place_drones(mut drones: Query<(&Drone, &mut Transform), Changed<Drone>>) {
    for (d, mut tf) in &mut drones {
        tf.translation = d.at;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        let t = TUNING.get();
        assert!(t.start_cash > 0 && t.lose_chance > 0.0 && t.strangers > 0);
        assert!(t.insurance_share > 0.0 && t.insurance_share < 1.0);
        assert!(t.drone_speed > 0.0 && t.pickup_reach > 0.0 && t.coin_value > 0);
        for l in [
            &t.lost_line,
            &t.found_line,
            &t.found_other_line,
            &t.insure_line,
            &t.insured_line,
            &t.no_claim_line,
            &t.brag_line,
            &t.coin_line,
        ] {
            assert!(!l.is_empty());
        }
    }

    #[test]
    fn bragging_doubles_the_drop_chance() {
        assert_eq!(drop_chance(0, 0.02, 8), 0.0);
        assert_eq!(drop_chance(1, 0.02, 8), 0.02);
        assert_eq!(drop_chance(3, 0.02, 8), 0.08);
        assert_eq!(drop_chance(20, 0.02, 4), drop_chance(4, 0.02, 4));
        assert_eq!(drop_chance(20, 0.5, 20), 1.0);
    }

    #[test]
    fn insurance_takes_seventy_percent() {
        assert_eq!(payout(100, 0.3), 30);
        assert_eq!(payout(0, 0.3), 0);
    }

    #[test]
    fn an_empty_wallet_cant_be_lost() {
        assert!(lost_on_entering(0.1, 0.25, 50));
        assert!(!lost_on_entering(0.1, 0.25, 0));
        assert!(!lost_on_entering(0.9, 0.25, 50));
    }

    #[test]
    fn only_braggers_show_their_cash() {
        let quiet = Cash {
            dollars: 70,
            ..default()
        };
        let loud = Cash {
            dollars: 90,
            brags: 2,
            ..default()
        };
        let line = cash_line(0, &[(0, quiet), (1, quiet), (2, loud)]);
        assert!(line.starts_with("$70"));
        assert!(!line.contains("player 1") && line.contains("player 2 brags: $90"));
    }
}
