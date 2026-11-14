//! Boba shops in New York. From issue #356 by catherine: "have them be fully
//! staffed and modeled and even have music and allow prime to buy boba which
//! gives temporary status effects that can stack. have the names and visuals
//! parody real life boba places in nyc".
//!
//! Three shops on the avenue's east side ([`Tuning::shops`]): Gong Chaw,
//! Kung Fu Tee and Tiger Sugah. Each is a modeler model (`nyc-boba`, one
//! export per shop with its own sign and paint) with two staff behind the
//! counter and its own song (the `boba` music signal, mood "Boba"). Click a
//! cup on the counter to buy that drink with your dollars ([`super::wallet`]):
//! it adds a timed stack to your [`Buzz`] (walk faster, the screen tints).
//! Stacks add up, up to `max_stacks`. Read "prime" as "players".

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::wallet::Cash;
use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::player::{LocalPlayer, Player, PlayerOwner};
pub use crate::rules::boba::*;

/// One drink on a shop's menu.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Drink {
    pub name: String,
    /// Dollars.
    pub price: u32,
    /// Seconds a stack lasts.
    pub secs: f32,
    /// Walking speed it adds per stack (0.1 = +10%).
    pub speed: f32,
    /// Screen tint (r, g, b).
    pub tint: (f32, f32, f32),
}

/// One boba shop.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Shop {
    pub name: String,
    /// Model file under assets/.
    pub model: String,
    /// Where it stands (x, z) on the city floor.
    pub at: (f32, f32),
    /// Its turn about +y, degrees (0: the door faces +z).
    pub yaw: f32,
    /// Shown when you step in.
    pub line: String,
    /// The three drinks at the counter, left to right.
    pub drinks: Vec<Drink>,
}

/// Tuning and writing: `assets/data/side/boba.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Most stacks at once.
    pub max_stacks: usize,
    /// Can't pay ({price}, {cash}).
    pub broke_line: String,
    /// Bought ({drink}, {price}, {stacks}).
    pub bought_line: String,
    pub shops: Vec<Shop>,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/boba.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

// ------------------------------------------------------------------ layout

/// The shop model's floor (local x, z): 9 x 12 m, door at +z.
const HALF: Vec2 = Vec2::new(4.5, 6.0);
/// Where the cups stand on the counter (local), left to right facing it.
const CUPS: [Vec3; 3] = [
    Vec3::new(-1.0, 1.24, -2.0),
    Vec3::new(-0.2, 1.24, -2.0),
    Vec3::new(0.6, 1.24, -2.0),
];
/// The staff: at the register and at the tea urns (local feet).
const STAFF: [Vec3; 2] = [Vec3::new(2.0, 0.1, -3.2), Vec3::new(-1.6, 0.1, -4.4)];

/// The model's collider boxes (local centre, full size): walls, the
/// shopfront either side of the door and over it, the counter, the roof.
const BOXES: [(Vec3, Vec3); 9] = [
    (Vec3::new(-4.35, 2.1, 0.0), Vec3::new(0.3, 4.2, 12.0)),
    (Vec3::new(4.35, 2.1, 0.0), Vec3::new(0.3, 4.2, 12.0)),
    (Vec3::new(0.0, 2.1, -5.85), Vec3::new(9.0, 4.2, 0.3)),
    (Vec3::new(-2.85, 2.1, 5.85), Vec3::new(3.3, 4.2, 0.3)),
    (Vec3::new(2.85, 2.1, 5.85), Vec3::new(3.3, 4.2, 0.3)),
    (Vec3::new(0.0, 3.5, 5.85), Vec3::new(2.4, 1.4, 0.3)),
    (Vec3::new(0.0, 0.575, -2.2), Vec3::new(6.2, 1.15, 0.95)),
    (Vec3::new(0.0, 0.525, -5.3), Vec3::new(7.6, 1.05, 0.7)),
    (Vec3::new(0.0, 4.35, 0.0), Vec3::new(9.4, 0.3, 12.4)),
];

/// A shop's placement in the world.
pub fn frame(shop: &Shop) -> Transform {
    Transform::from_xyz(shop.at.0, crate::rules::nyc::CITY_Y, shop.at.1)
        .with_rotation(Quat::from_rotation_y(shop.yaw.to_radians()))
}

/// A point in a shop's own coordinates, in the world.
pub fn world(shop: &Shop, local: Vec3) -> Vec3 {
    frame(shop).transform_point(local)
}

/// Is (x, z) inside this shop's walls?
pub fn inside(shop: &Shop, p: Vec3) -> bool {
    let l = frame(shop).compute_affine().inverse().transform_point3(p);
    l.x.abs() < HALF.x - 0.3 && l.z.abs() < HALF.y - 0.3 && l.y > -1.0 && l.y < 4.2
}

/// The shop (index) someone at `p` is in.
pub fn shop_at(t: &Tuning, p: Vec3) -> Option<usize> {
    t.shops.iter().position(|s| inside(s, p))
}

/// Pure: fill a line's {drink}, {price}, {cash}, {stacks}.
pub fn fill(line: &str, drink: &str, price: u32, cash: u32, stacks: usize) -> String {
    line.replace("{drink}", drink)
        .replace("{price}", &price.to_string())
        .replace("{cash}", &cash.to_string())
        .replace("{stacks}", &stacks.to_string())
}

// ------------------------------------------------------------------ server

/// Client → server: buy drink `drink` at shop `shop`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct BuyBoba {
    pub shop: u8,
    pub drink: u8,
}

/// Server → clients: a toast for player `to`.
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct BobaNews {
    pub to: u64,
    pub text: String,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Buzz>()
        .add_client_event::<BuyBoba>(Channel::Ordered)
        .add_server_event::<BobaNews>(Channel::Ordered)
        .add_observer(give_buzz)
        .add_observer(buy)
        .add_systems(Update, wear_off.run_if(in_state(ServerState::Running)));
    for s in &tuning().shops {
        friendslop_world::footprints::claim(Vec2::new(s.at.0, s.at.1), 7.0);
    }
}

fn give_buzz(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() == ServerState::Running {
        commands.entity(add.entity).insert(Buzz::default());
    }
}

fn buy(
    ev: On<FromClient<BuyBoba>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    mut buyers: Query<(&Player, &PlayerOwner, &mut Cash, &mut Buzz)>,
) {
    let t = tuning();
    let Some(shop) = t.shops.get(ev.shop as usize) else {
        return;
    };
    let (Some(drink), Some(cup)) = (
        shop.drinks.get(ev.drink as usize),
        CUPS.get(ev.drink as usize),
    ) else {
        return;
    };
    if !in_reach(&mut commands, ev.client_id, &walkers, world(shop, *cup)) {
        return;
    }
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((player, _, mut cash, mut buzz)) = buyers.iter_mut().find(|b| b.1.0 == owner) else {
        return;
    };
    if cash.dollars < drink.price {
        let text = fill(&t.broke_line, &drink.name, drink.price, cash.dollars, 0);
        deny(&mut commands, ev.client_id, text);
        return;
    }
    cash.dollars -= drink.price;
    buzz.add(
        Stack {
            drink: drink.name.clone(),
            speed: drink.speed,
            secs: drink.secs,
            tint: drink.tint,
        },
        t.max_stacks,
    );
    let text = fill(
        &t.bought_line,
        &drink.name,
        drink.price,
        cash.dollars,
        buzz.stacks.len(),
    );
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: BobaNews {
            to: player.id,
            text,
        },
    });
}

/// Server: stacks wear down a second at a time (so replication stays quiet).
fn wear_off(time: Res<Time>, mut acc: Local<f32>, mut buzzes: Query<&mut Buzz>) {
    *acc += time.delta_secs();
    if *acc < 1.0 {
        return;
    }
    let dt = std::mem::take(&mut *acc);
    for mut b in &mut buzzes {
        if !b.stacks.is_empty() {
            b.tick(dt);
        }
    }
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_observer(show_news)
        .add_observer(click_cup)
        .add_systems(OnEnter(AppState::InGame), build)
        .add_systems(
            Update,
            (show_buzz, step_in).run_if(in_state(AppState::InGame)),
        )
        .add_systems(Update, music_signals.in_set(crate::music::MusicSignalSet));
}

/// A cup you can click to buy its drink.
#[derive(Component)]
struct Cup {
    shop: u8,
    drink: u8,
}

#[derive(Component)]
struct BuzzText;

#[derive(Component)]
struct BuzzTint;

fn build(mut commands: Commands, assets: Res<AssetServer>) {
    let t = tuning();
    for (i, shop) in t.shops.iter().enumerate() {
        let f = frame(shop);
        commands.spawn((
            WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(shop.model.clone()))),
            f,
            DespawnOnExit(AppState::InGame),
        ));
        for (c, s) in BOXES {
            commands.spawn((
                Transform::from_translation(f.transform_point(c)).with_rotation(f.rotation),
                avian3d::prelude::RigidBody::Static,
                avian3d::prelude::Collider::cuboid(s.x, s.y, s.z),
                DespawnOnExit(AppState::InGame),
            ));
        }
        for (d, cup) in CUPS.iter().enumerate().take(shop.drinks.len()) {
            commands.spawn((
                Cup {
                    shop: i as u8,
                    drink: d as u8,
                },
                Transform::from_translation(f.transform_point(*cup)),
                Visibility::default(),
                Clickable::at_height(0.0).labelled("buy this boba"),
                DespawnOnExit(AppState::InGame),
            ));
        }
        // Fully staffed: one at the register, one at the tea urns.
        for (k, at) in STAFF.iter().enumerate() {
            commands.spawn((
                crate::people::Gait::default(),
                Transform::from_translation(f.transform_point(*at)).with_rotation(f.rotation),
                Visibility::default(),
                crate::people::CharacterLook::random(0x0356_0000 + (i * 2 + k) as u64 * 13),
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    commands.spawn((
        Text::new(""),
        BuzzText,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        Pickable::IGNORE,
        GlobalZIndex(-10),
        BuzzTint,
        DespawnOnExit(AppState::InGame),
    ));
}

fn click_cup(click: On<Clicked>, mut commands: Commands, cups: Query<&Cup>) {
    if let Ok(c) = cups.get(click.entity) {
        commands.client_trigger(BuyBoba {
            shop: c.shop,
            drink: c.drink,
        });
    }
}

fn show_news(
    ev: On<BobaNews>,
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

/// Pure: the status line for your stacks.
pub fn buzz_line(b: &Buzz) -> String {
    if b.stacks.is_empty() {
        return String::new();
    }
    let mut out = format!(
        "boba x{}: +{:.0}% speed",
        b.stacks.len(),
        (b.speed() - 1.0) * 100.0
    );
    for s in &b.stacks {
        out += &format!("\n  {} {:.0}s", s.drink, s.secs.max(0.0));
    }
    out
}

/// Pure: the screen tint for your stacks (their colours mixed, stronger per stack).
pub fn tint(b: &Buzz) -> Color {
    if b.stacks.is_empty() {
        return Color::NONE;
    }
    let n = b.stacks.len() as f32;
    let (r, g, bl) = b.stacks.iter().fold((0.0, 0.0, 0.0), |a, s| {
        (a.0 + s.tint.0 / n, a.1 + s.tint.1 / n, a.2 + s.tint.2 / n)
    });
    Color::srgba(r, g, bl, (0.05 * n).min(0.25))
}

fn show_buzz(
    me: Option<Res<LocalPlayer>>,
    buzzes: Query<(&Player, &Buzz)>,
    mut text: Query<&mut Text, With<BuzzText>>,
    mut overlay: Query<&mut BackgroundColor, With<BuzzTint>>,
) {
    let Some(me) = me else { return };
    let mine = buzzes.iter().find(|(p, _)| p.id == me.0).map(|(_, b)| b);
    let empty = Buzz::default();
    let b = mine.unwrap_or(&empty);
    let want = buzz_line(b);
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
    let c = tint(b);
    for mut bg in &mut overlay {
        if bg.0 != c {
            bg.0 = c;
        }
    }
}

/// Step into a shop and its line shows.
fn step_in(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut was: Local<Option<usize>>,
) {
    let t = tuning();
    let now = crate::music::camera_at(&camera).and_then(|p| shop_at(&t, p));
    if now != *was
        && let (Some(i), Some(mut toast)) = (now, toast)
        && let Some(s) = t.shops.get(i)
    {
        toast.show(s.line.clone());
    }
    *was = now;
}

/// For the music: the camera's in a boba shop (`boba`).
fn music_signals(
    camera: Query<&GlobalTransform, With<Camera3d>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let t = tuning();
    let at = crate::music::camera_at(&camera);
    signals.flag("boba", at.is_some_and(|p| shop_at(&t, p).is_some()));
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
        assert!(t.max_stacks > 1);
        assert!(!t.broke_line.is_empty() && !t.bought_line.is_empty());
        assert_eq!(t.shops.len(), 3);
        for s in &t.shops {
            assert!(!s.name.is_empty() && !s.model.is_empty() && !s.line.is_empty());
            assert_eq!(s.drinks.len(), CUPS.len());
            for d in &s.drinks {
                assert!(!d.name.is_empty() && d.price > 0 && d.secs > 0.0 && d.speed > 0.0);
            }
        }
    }

    #[test]
    fn the_shops_are_in_the_city_and_you_can_walk_in() {
        let t = t();
        for s in &t.shops {
            assert!(crate::rules::nyc::in_city(s.at.0, s.at.1));
            assert!(inside(s, world(s, Vec3::new(0.0, 0.1, 2.0))));
            assert!(!inside(s, world(s, Vec3::new(0.0, 0.1, 8.0))));
            // The door faces the avenue (west, toward x = 0).
            assert!(world(s, Vec3::new(0.0, 0.0, 6.0)).x < s.at.0);
        }
    }

    #[test]
    fn the_status_line_and_tint_follow_the_stacks() {
        let mut b = Buzz::default();
        assert_eq!(buzz_line(&b), "");
        assert_eq!(tint(&b), Color::NONE);
        b.add(
            Stack {
                drink: "Taro Slush".into(),
                speed: 0.2,
                secs: 30.0,
                tint: (1.0, 0.0, 0.0),
            },
            5,
        );
        assert!(buzz_line(&b).starts_with("boba x1: +20% speed"));
        assert_eq!(
            fill("{drink} ${price} x{stacks}", "Taro", 7, 0, 2),
            "Taro $7 x2"
        );
    }
}
