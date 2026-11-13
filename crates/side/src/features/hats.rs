//! Hats from the bodega. From issue #323 by doomy: "hats you can buy with
//! dollars in the bodega. everyone can see them. theyre all making a
//! statement".
//!
//! A rack of hats stands along the bodega's back wall ([`Tuning::hats`], each
//! a modeler model under `models/hats/` with its statement on it). Click one
//! to buy it with your dollars ([`super::wallet`]): it goes on your head, and
//! [`Hat`] is replicated, so everyone sees it. Buying another swaps it; click
//! the one you're wearing to take it off (no refunds).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::wallet::Cash;
use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::people::HeadAnchor;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// One hat on the rack.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct HatKind {
    pub name: String,
    /// Model file under assets/ (modeled for a 0.28 m head, centre at the origin).
    pub model: String,
    /// Dollars.
    pub price: u32,
}

/// Tuning and writing: `assets/data/side/hats.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    pub hats: Vec<HatKind>,
    /// Bought ({hat}, {price}, {cash}).
    pub bought_line: String,
    /// Can't pay ({hat}, {price}, {cash}).
    pub broke_line: String,
    /// Took it off ({hat}).
    pub off_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/hats.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The head these hats are modeled on (people.rs's head size).
const HEAD_REF: f32 = 0.28;
/// The rack: along the bodega's back wall (the wall away from its door), this
/// far in from it (m), at this height, spaced this far apart.
const RACK_IN: f32 = 0.6;
const RACK_Y: f32 = 1.35;
const RACK_STEP: f32 = 1.1;

/// What a player wears: an index into [`Tuning::hats`], or nothing.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Hat(pub Option<u8>);

/// Pure: where hat `i` of `n` hangs, for the bodega's room (its door is on the
/// side facing the avenue at x = 0, so the back wall is the other one).
pub fn rack_spot(room: &super::nyc::Room, i: usize, n: usize) -> Vec3 {
    let mid = (room.min + room.max) / 2.0;
    let x = if mid.x < 0.0 {
        room.min.x + RACK_IN
    } else {
        room.max.x - RACK_IN
    };
    let z = mid.y + (i as f32 - (n as f32 - 1.0) / 2.0) * RACK_STEP;
    Vec3::new(x, crate::rules::nyc::CITY_Y + RACK_Y, z)
}

/// The bodega's room on the city map.
fn bodega() -> Option<super::nyc::Room> {
    let c = super::nyc::city(&super::nyc::MAP.get());
    c.rooms.first().copied().filter(|r| r.min.x < r.max.x)
}

/// What clicking hat `pick` does to someone wearing `now` with `cash` dollars.
#[derive(Debug, PartialEq, Eq)]
pub enum Deal {
    /// Take it off.
    Off,
    /// Too poor.
    Broke,
    /// Pay and wear it.
    Buy,
}

/// Pure: the deal.
pub fn deal(now: Hat, pick: u8, price: u32, cash: u32) -> Deal {
    if now.0 == Some(pick) {
        Deal::Off
    } else if cash < price {
        Deal::Broke
    } else {
        Deal::Buy
    }
}

/// Pure: fill a line's {hat}, {price}, {cash}.
pub fn fill(line: &str, hat: &str, price: u32, cash: u32) -> String {
    line.replace("{hat}", hat)
        .replace("{price}", &price.to_string())
        .replace("{cash}", &cash.to_string())
}

// ------------------------------------------------------------------ server

/// Client → server: click hat `0` on the rack.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PickHat(pub u8);

/// Server → clients: a toast for player `to`.
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct HatNews {
    pub to: u64,
    pub text: String,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Hat>()
        .add_client_event::<PickHat>(Channel::Ordered)
        .add_server_event::<HatNews>(Channel::Ordered)
        .add_observer(give_hat)
        .add_observer(pick);
}

fn give_hat(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() == ServerState::Running {
        commands.entity(add.entity).insert(Hat::default());
    }
}

fn pick(
    ev: On<FromClient<PickHat>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    mut buyers: Query<(&Player, &PlayerOwner, &mut Cash, &mut Hat)>,
) {
    let t = tuning();
    let Some(kind) = t.hats.get(ev.0 as usize) else {
        return;
    };
    let Some(room) = bodega() else { return };
    let at = rack_spot(&room, ev.0 as usize, t.hats.len());
    if !in_reach(&mut commands, ev.client_id, &walkers, at) {
        return;
    }
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((player, _, mut cash, mut hat)) = buyers.iter_mut().find(|b| b.1.0 == owner) else {
        return;
    };
    let text = match deal(*hat, ev.0, kind.price, cash.dollars) {
        Deal::Off => {
            *hat = Hat(None);
            fill(&t.off_line, &kind.name, kind.price, cash.dollars)
        }
        Deal::Broke => {
            let text = fill(&t.broke_line, &kind.name, kind.price, cash.dollars);
            deny(&mut commands, ev.client_id, text);
            return;
        }
        Deal::Buy => {
            cash.dollars -= kind.price;
            *hat = Hat(Some(ev.0));
            fill(&t.bought_line, &kind.name, kind.price, cash.dollars)
        }
    };
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: HatNews {
            to: player.id,
            text,
        },
    });
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_observer(show_news)
        .add_observer(click)
        .add_systems(OnEnter(AppState::InGame), build)
        .add_systems(Update, wear.run_if(in_state(AppState::InGame)));
}

/// A hat on the rack you can click.
#[derive(Component)]
struct OnRack(u8);

/// The hat model on someone's head: which hat it shows.
#[derive(Component)]
struct Worn(u8);

fn hat_scene(assets: &AssetServer, kind: &HatKind) -> WorldAssetRoot {
    WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(kind.model.clone())))
}

fn build(mut commands: Commands, assets: Res<AssetServer>) {
    let t = tuning();
    let Some(room) = bodega() else { return };
    let n = t.hats.len();
    // Facing into the room, so the statements read from the aisle.
    let face = if room.min.x + room.max.x < 0.0 {
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)
    } else {
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
    };
    for (i, kind) in t.hats.iter().enumerate() {
        let at = rack_spot(&room, i, n);
        // Hat models sit on a head's centre, so lift the peg's top under it.
        commands.spawn((
            OnRack(i as u8),
            Transform::from_translation(at - Vec3::Y * 0.2).with_rotation(face),
            Visibility::default(),
            Clickable::at_height(0.25).labelled(format!("{} (${})", kind.name, kind.price).leak()),
            DespawnOnExit(AppState::InGame),
            children![(hat_scene(&assets, kind), Transform::from_xyz(0.0, 0.0, 0.0))],
        ));
    }
}

fn click(click: On<Clicked>, mut commands: Commands, racks: Query<&OnRack>) {
    if let Ok(r) = racks.get(click.entity) {
        commands.client_trigger(PickHat(r.0));
    }
}

fn show_news(
    ev: On<HatNews>,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    if ev.to == me.0 {
        toast.show(ev.text.clone());
    }
}

/// Keep each head's hat model matching its player's [`Hat`]. Runs on every
/// peer, so everyone sees everyone's statement. Heads are rebuilt with their
/// rig, so a new anchor gets its hat again.
fn wear(
    mut commands: Commands,
    assets: Res<AssetServer>,
    heads: Query<(Entity, &HeadAnchor, Option<&Children>)>,
    hats: Query<&Hat>,
    worn: Query<&Worn>,
    changed: Query<(), Changed<Hat>>,
    new_heads: Query<(), Added<HeadAnchor>>,
) {
    // Nothing to do unless someone's hat changed or a head was (re)built.
    if changed.is_empty() && new_heads.is_empty() {
        return;
    }
    let t = tuning();
    for (anchor, head, children) in &heads {
        let Ok(hat) = hats.get(head.root) else {
            continue;
        };
        let current = children
            .into_iter()
            .flatten()
            .find_map(|c| worn.get(*c).ok().map(|w| (*c, w.0)));
        if current.map(|c| c.1) == hat.0 {
            continue;
        }
        if let Some((e, _)) = current {
            commands.entity(e).despawn();
        }
        if let Some(i) = hat.0
            && let Some(kind) = t.hats.get(i as usize)
        {
            commands.entity(anchor).with_child((
                Worn(i),
                hat_scene(&assets, kind),
                Transform::from_scale(Vec3::splat(head.radius / HEAD_REF)),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> std::sync::Arc<Tuning> {
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
        let t = t();
        assert!(t.hats.len() >= 3 && t.hats.len() < 256);
        for h in &t.hats {
            assert!(!h.name.is_empty() && h.model.starts_with("models/hats/") && h.price > 0);
        }
        assert!(!t.bought_line.is_empty() && !t.broke_line.is_empty() && !t.off_line.is_empty());
    }

    #[test]
    fn buying_swapping_and_taking_off() {
        assert_eq!(deal(Hat(None), 0, 10, 5), Deal::Broke);
        assert_eq!(deal(Hat(None), 0, 10, 10), Deal::Buy);
        assert_eq!(deal(Hat(Some(1)), 0, 10, 50), Deal::Buy);
        assert_eq!(deal(Hat(Some(0)), 0, 10, 0), Deal::Off);
        assert_eq!(
            fill("{hat} ${price} left ${cash}", "TAX ME", 30, 4),
            "TAX ME $30 left $4"
        );
    }

    #[test]
    fn the_rack_is_inside_the_bodega_on_its_back_wall() {
        let room = super::super::nyc::Room {
            min: Vec2::new(-31.7, 160.3),
            max: Vec2::new(-18.25, 169.7),
        };
        for i in 0..5 {
            let p = rack_spot(&room, i, 5);
            assert!(room.contains(p.xz()), "{p}");
            assert!(p.x < -30.0);
        }
    }
}
