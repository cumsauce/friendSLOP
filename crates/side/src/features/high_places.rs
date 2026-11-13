//! High Places, the rooftop dispensary. From issue #349 by doomy: "there
//! should be a dispensary in new york called high places and to get there you
//! go into WTC One and take the elevator to the top floor where theres a
//! rooftop smoke shop and place to light up. smoking mechanic is like a
//! minigame, you have to draw enough by clicking left button so that it doesnt
//! go out, but if you draw too much hit in one go youll cough. goal is to
//! smoke 100% of the joint with no waste."
//!
//! The elevator door is on One World Trade Center's avenue side; walk in and
//! you come out of the elevator hut on a deck on the roof, 428 m up (the
//! `high-places` model, on WTC's top). The HIGH PLACES kiosk sells joints for
//! dollars ([`super::wallet`]); they go in your pocket like Sewer Paul's
//! ([`crate::features::smoking::Stash`]). At the lounge (LIGHT UP HERE) you
//! spark one: everyone sees you smoke, and you play the joint ([`Joint`]):
//!
//! - the ember cools all the time; let it go cold and it goes out (the burnt
//!   end is wasted, and your next click relights it);
//! - left click draws: it heats the ember and smokes a bit of the joint, but
//!   fills your lungs; draw too much in one go (lungs over full) and you cough
//!   (and blow some away);
//! - draw it red hot (ember over `hot`) and it burns away on its own (wasted),
//!   so keep a steady rhythm.
//!
//! It's done when it's all gone: the score is how much you smoked; 100% with
//! no waste is a perfect smoke. Prices, timings, the minigame and the writing
//! are `data/side/high_places.ron`. Colliders are boxes, so the deck works
//! without the model.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::wallet::Cash;
use crate::AppState;
use crate::features::fps::{WalkInput, Walker};
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::features::smoking::{Smoking, Stash};
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Tuning and writing: `assets/data/side/high_places.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// A joint's price (dollars).
    pub price: u32,
    /// Stand this close (m) to an elevator door and you ride it.
    pub door_reach: f32,
    /// How long everyone sees you smoke (s).
    pub smoke_secs: f32,
    /// The ember: how much a draw heats it, and how fast it cools (per s).
    pub draw_heat: f32,
    pub cool: f32,
    /// A draw's share of the joint, and how full it fills your lungs (1 = a cough).
    pub draw_amount: f32,
    pub draw_lung: f32,
    /// How fast you breathe out (lungs per s).
    pub exhale: f32,
    /// What's wasted: burning away on its own while the ember's hotter than
    /// `hot` (per s), a cough, going out.
    pub hot: f32,
    pub idle_burn: f32,
    pub cough_waste: f32,
    pub out_waste: f32,
    /// Lines: walking out of the elevator up top, buying, broke, no joint,
    /// sparking it, coughing, going out, the result (`{smoked}` and `{wasted}`
    /// in percent, `{coughs}`), and a perfect smoke.
    pub arrive_line: String,
    pub bought_line: String,
    pub broke_line: String,
    pub no_joint_line: String,
    pub lit_line: String,
    pub cough_line: String,
    pub out_line: String,
    pub result_line: String,
    pub perfect_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/high_places.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

// ------------------------------------------------------------------ the place
// Offsets from WTC's foot (its map instance) and, up top, from the deck's
// middle (the `high-places` model's origin).

/// The deck's top above WTC's foot (its top collider's top).
pub const ROOF_Y: f32 = 428.0;
/// The street door (on the avenue face) and where you come out down there.
pub const GROUND_DOOR: Vec3 = Vec3::new(-31.4, 0.0, 0.0);
pub const GROUND_OUT: Vec3 = Vec3::new(-33.6, 0.0, 0.0);
/// Up top: the elevator hut's door and where you step out.
pub const ROOF_DOOR: Vec3 = Vec3::new(-10.2, 0.0, 0.0);
pub const ROOF_OUT: Vec3 = Vec3::new(-7.4, 0.0, 0.0);
/// The kiosk's counter and the lounge's table.
pub const COUNTER: Vec3 = Vec3::new(8.0, 0.0, -8.8);
pub const LOUNGE: Vec3 = Vec3::new(5.0, 0.0, 8.0);
const HALF: f32 = 15.0;

/// Where WTC One stands (its map instance's foot).
pub fn wtc(map: &friendslop_world::places::MapFile) -> Option<Vec3> {
    let e = map
        .entities
        .iter()
        .find(|e| e.gltf.as_deref().is_some_and(|g| g.contains("nyc-wtc")))?;
    Some(Vec3::from_array(e.translation))
}

/// The deck's middle, from WTC's foot.
pub fn deck(foot: Vec3) -> Vec3 {
    foot + Vec3::Y * ROOF_Y
}

/// Riding the elevator: from the street door to the roof, or back. Where you
/// end up, if anywhere.
pub fn ride(pos: Vec3, foot: Vec3, reach: f32) -> Option<Vec3> {
    let near = |at: Vec3| {
        Vec2::new(pos.x - at.x, pos.z - at.z).length() < reach && (pos.y - at.y).abs() < 3.0
    };
    let top = deck(foot);
    if near(foot + GROUND_DOOR) {
        Some(top + ROOF_OUT)
    } else if near(top + ROOF_DOOR) {
        Some(foot + GROUND_OUT)
    } else {
        None
    }
}

/// Is a point on the deck?
pub fn on_deck(pos: Vec3, foot: Vec3) -> bool {
    let l = pos - deck(foot);
    l.x.abs() < HALF && l.z.abs() < HALF && (-1.0..6.0).contains(&l.y)
}

/// The deck's static boxes (centre, size), from its middle.
pub fn boxes() -> Vec<(Vec3, Vec3)> {
    let span = |a: Vec3, b: Vec3| ((a + b) / 2.0, (b - a).abs());
    let (h, r) = (HALF, 1.2);
    vec![
        // The glass railing round the edge.
        span(Vec3::new(-h, 0.0, h - 0.1), Vec3::new(h, r, h)),
        span(Vec3::new(-h, 0.0, -h), Vec3::new(h, r, -h + 0.1)),
        span(Vec3::new(h - 0.1, 0.0, -h), Vec3::new(h, r, h)),
        span(Vec3::new(-h, 0.0, -h), Vec3::new(-h + 0.1, r, h)),
        // The elevator hut, the kiosk and its counter.
        span(Vec3::new(-13.5, 0.0, -1.7), Vec3::new(-10.5, 3.0, 1.7)),
        span(Vec3::new(4.0, 0.0, -13.25), Vec3::new(12.0, 3.2, -9.75)),
        span(Vec3::new(5.0, 0.0, -9.7), Vec3::new(11.0, 1.05, -8.9)),
    ]
}

// ------------------------------------------------------------------ the joint (pure)

/// A joint being smoked: the ember's heat (0..1), your lungs (1 = a cough),
/// and the shares of it smoked and wasted.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Joint {
    pub heat: f32,
    pub lungs: f32,
    pub smoked: f32,
    pub wasted: f32,
    pub lit: bool,
    pub coughs: u32,
}

impl Default for Joint {
    fn default() -> Self {
        Self {
            heat: 0.6,
            lungs: 0.0,
            smoked: 0.0,
            wasted: 0.0,
            lit: true,
            coughs: 0,
        }
    }
}

/// What a step of the joint said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Puff {
    Cough,
    WentOut,
    Done,
}

impl Joint {
    pub fn left(&self) -> f32 {
        (1.0 - self.smoked - self.wasted).max(0.0)
    }
    pub fn done(&self) -> bool {
        self.left() <= 1e-4
    }
    fn waste(&mut self, amount: f32) {
        self.wasted += amount.min(self.left());
    }
}

/// One step: `dt` seconds pass, and `draw` is whether you clicked.
pub fn step(mut j: Joint, dt: f32, draw: bool, t: &Tuning) -> (Joint, Option<Puff>) {
    if j.done() {
        return (j, None);
    }
    let mut said = None;
    if draw {
        if !j.lit {
            j.lit = true;
            j.heat = t.draw_heat;
        } else {
            j.heat = (j.heat + t.draw_heat).min(1.0);
            let take = t.draw_amount.min(j.left());
            j.smoked += take;
            j.lungs += t.draw_lung;
            if j.lungs > 1.0 {
                j.lungs = 0.0;
                j.coughs += 1;
                j.waste(t.cough_waste);
                said = Some(Puff::Cough);
            }
        }
    }
    j.lungs = (j.lungs - t.exhale * dt).max(0.0);
    if j.lit {
        if j.heat > t.hot {
            j.waste(t.idle_burn * dt);
        }
        j.heat -= t.cool * dt;
        if j.heat <= 0.0 {
            j.heat = 0.0;
            j.lit = false;
            j.waste(t.out_waste);
            said = Some(Puff::WentOut);
        }
    }
    if j.done() {
        said = Some(Puff::Done);
    }
    (j, said)
}

/// Fill a result line: `{smoked}` and `{wasted}` (percent) and `{coughs}`.
pub fn result(line: &str, j: &Joint) -> String {
    line.replace("{smoked}", &format!("{:.0}", j.smoked * 100.0))
        .replace("{wasted}", &format!("{:.0}", j.wasted * 100.0))
        .replace("{coughs}", &j.coughs.to_string())
}

/// A perfect smoke: all of it, nothing wasted (to the nearest percent).
pub fn perfect(j: &Joint) -> bool {
    j.done() && j.wasted < 0.005
}

// ------------------------------------------------------------------ server

/// Client → server: buy a joint at the counter.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct BuyJoint;

/// Client → server: spark one up at the lounge.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct LightUp;

/// Server → clients: a toast for player `to`, and whether their joint's lit.
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct HighNews {
    pub to: u64,
    pub text: String,
    pub lit: bool,
}

pub fn protocol(app: &mut App) {
    app.add_client_event::<BuyJoint>(Channel::Ordered)
        .add_client_event::<LightUp>(Channel::Ordered)
        .add_server_event::<HighNews>(Channel::Ordered)
        .add_observer(buy)
        .add_observer(light_up)
        .add_systems(
            Update,
            elevator
                .run_if(in_state(AppState::InGame))
                .run_if(in_state(ServerState::Running)),
        );
}

fn foot() -> Option<Vec3> {
    wtc(&super::nyc::MAP.get())
}

fn owner(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    }
}

fn news(commands: &mut Commands, to: u64, text: String, lit: bool) {
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: HighNews { to, text, lit },
    });
}

fn buy(
    ev: On<FromClient<BuyJoint>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    mut buyers: Query<(&Player, &PlayerOwner, &mut Cash, &mut Stash)>,
) {
    let (t, Some(foot)) = (tuning(), foot()) else {
        return;
    };
    if !in_reach(&mut commands, ev.client_id, &walkers, deck(foot) + COUNTER) {
        return;
    }
    let me = owner(ev.client_id);
    let Some((player, _, mut cash, mut stash)) = buyers.iter_mut().find(|b| b.1.0 == me) else {
        return;
    };
    if cash.dollars < t.price {
        let text = t
            .broke_line
            .replace("{price}", &t.price.to_string())
            .replace("{cash}", &cash.dollars.to_string());
        deny(&mut commands, ev.client_id, text);
        return;
    }
    cash.dollars -= t.price;
    stash.joints += 1;
    let text = t
        .bought_line
        .replace("{price}", &t.price.to_string())
        .replace("{cash}", &cash.dollars.to_string());
    news(&mut commands, player.id, text, false);
}

fn light_up(
    ev: On<FromClient<LightUp>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    mut smokers: Query<(Entity, &Player, &PlayerOwner, &mut Stash, Has<Smoking>)>,
) {
    let (t, Some(foot)) = (tuning(), foot()) else {
        return;
    };
    if !in_reach(&mut commands, ev.client_id, &walkers, deck(foot) + LOUNGE) {
        return;
    }
    let me = owner(ev.client_id);
    let Some((e, player, _, mut stash, busy)) = smokers.iter_mut().find(|s| s.2.0 == me) else {
        return;
    };
    if busy {
        return;
    }
    if stash.joints == 0 {
        deny(&mut commands, ev.client_id, t.no_joint_line.clone());
        return;
    }
    stash.joints -= 1;
    commands.entity(e).insert(Smoking {
        left: t.smoke_secs,
        joint: true,
    });
    news(&mut commands, player.id, t.lit_line.clone(), true);
}

/// Server: the elevator, both ways.
fn elevator(
    mut bodies: Query<(
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    let Some(foot) = foot() else { return };
    let reach = tuning().door_reach;
    for (mut walker, input, kcc) in &mut bodies {
        let Some(to) = ride(walker.pos, foot, reach) else {
            continue;
        };
        walker.pos = to + Vec3::Y * 0.05;
        if let Some(mut input) = input {
            input.stick = Vec2::ZERO;
            input.jump = false;
        }
        if let Some(mut kcc) = kcc {
            kcc.velocity = Vec3::ZERO;
        }
    }
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.init_resource::<Smoke>()
        .add_observer(show_news)
        .add_observer(click)
        .add_systems(OnEnter(AppState::InGame), build)
        .add_systems(
            Update,
            (play, arrive_line).run_if(in_state(AppState::InGame)),
        );
}

/// Local: the joint you're smoking, if any.
#[derive(Resource, Default)]
struct Smoke(Option<Joint>);

#[derive(Component, Clone, Copy, PartialEq)]
enum Thing {
    Counter,
    Lounge,
}

#[derive(Component)]
struct Meter;

fn build(mut commands: Commands, assets: Res<AssetServer>) {
    let Some(foot) = foot() else { return };
    let top = deck(foot);
    commands.spawn((
        WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset("models/high-places.gltf"))),
        Transform::from_translation(top),
        DespawnOnExit(AppState::InGame),
    ));
    for (c, s) in boxes() {
        commands.spawn((
            Transform::from_translation(top + c),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(s.x, s.y, s.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
    let price = tuning().price;
    for (thing, at, label) in [
        (Thing::Counter, COUNTER, format!("buy a joint (${price})")),
        (Thing::Lounge, LOUNGE, "light up".to_string()),
    ] {
        commands.spawn((
            thing,
            Transform::from_translation(top + at),
            Clickable::at_height(1.0).labelled(label.leak()),
            DespawnOnExit(AppState::InGame),
        ));
    }
    use crate::features::doorway::{Doorway, spawn, yaw_towards};
    spawn(
        &mut commands,
        foot + GROUND_DOOR,
        yaw_towards(Vec2::X),
        Doorway::new(crate::features::doorway::Way::Up, "High Places").size(2.0, 2.6),
    );
    spawn(
        &mut commands,
        top + ROOF_DOOR,
        yaw_towards(Vec2::NEG_X),
        Doorway::new(crate::features::doorway::Way::Down, "the street").size(1.6, 2.3),
    );
    // The minigame's meter (hidden until you're smoking).
    commands.spawn((
        Meter,
        Text::new(""),
        TextFont {
            font_size: bevy::text::FontSize::Px(22.0),
            ..default()
        },
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Percent(18.0),
            left: Val::Percent(30.0),
            right: Val::Percent(30.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
    let (up, down) = (top + ROOF_OUT, foot + GROUND_OUT);
    commands.queue(move |world: &mut World| {
        if let Some(mut p) = world.get_resource_mut::<crate::features::start::StartPoints>() {
            p.add("high_places", up);
            p.add("wtc_door", down);
        }
    });
}

fn click(click: On<Clicked>, mut commands: Commands, things: Query<&Thing>, smoke: Res<Smoke>) {
    match things.get(click.entity) {
        Ok(Thing::Counter) => commands.client_trigger(BuyJoint),
        Ok(Thing::Lounge) if smoke.0.is_none() => commands.client_trigger(LightUp),
        _ => {}
    }
}

fn show_news(
    ev: On<HighNews>,
    me: Option<Res<LocalPlayer>>,
    mut smoke: ResMut<Smoke>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    if me.is_none_or(|me| me.0 != ev.to) {
        return;
    }
    if ev.lit {
        smoke.0 = Some(Joint::default());
    }
    if let Some(mut toast) = toast {
        toast.show(ev.text.clone());
    }
}

fn bar(v: f32) -> String {
    let n = (v.clamp(0.0, 1.0) * 10.0).round() as usize;
    format!("{}{}", "#".repeat(n), "-".repeat(10 - n))
}

/// Local: play the joint (left click draws) and show the meter.
fn play(
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut smoke: ResMut<Smoke>,
    mut meter: Query<(&mut Text, &mut Visibility), With<Meter>>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let Ok((mut text, mut vis)) = meter.single_mut() else {
        return;
    };
    let Some(j) = smoke.0 else {
        vis.set_if_neq(Visibility::Hidden);
        return;
    };
    let t = tuning();
    let (j, said) = step(
        j,
        time.delta_secs(),
        mouse.just_pressed(MouseButton::Left),
        &t,
    );
    vis.set_if_neq(Visibility::Inherited);
    let line = format!(
        "ember [{}]{}  lungs [{}]\nsmoked {:.0}%  wasted {:.0}%  (left click to draw)",
        bar(j.heat),
        if j.lit { "" } else { " OUT" },
        bar(j.lungs),
        j.smoked * 100.0,
        j.wasted * 100.0,
    );
    // Only touch the text when it reads differently (a change re-lays it out).
    if text.0 != line {
        text.0 = line;
    }
    smoke.0 = Some(j);
    let line = match said {
        Some(Puff::Cough) => t.cough_line.clone(),
        Some(Puff::WentOut) => t.out_line.clone(),
        Some(Puff::Done) => {
            smoke.0 = None;
            if perfect(&j) {
                t.perfect_line.clone()
            } else {
                result(&t.result_line, &j)
            }
        }
        None => return,
    };
    if let Some(mut toast) = toast {
        toast.show(line);
    }
}

/// Local: a line when you step out of the elevator up top.
fn arrive_line(
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut was: Local<bool>,
) {
    let Some(foot) = foot() else { return };
    let pos = me.as_ref().and_then(|me| {
        walkers
            .iter()
            .find(|(p, _)| p.id == me.0)
            .map(|(_, w)| w.pos)
    });
    let now = pos.is_some_and(|p| on_deck(p, foot));
    if now
        && !*was
        && let Some(mut toast) = toast
    {
        toast.show(tuning().arrive_line.clone());
    }
    *was = now;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.price > 0 && t.door_reach > 0.0 && t.smoke_secs > 0.0);
        assert!(t.draw_heat > 0.0 && t.cool > 0.0 && t.draw_amount > 0.0);
        assert!(t.draw_lung > 0.0 && t.draw_lung < 1.0 && t.exhale > 0.0);
        assert!(t.hot > 0.0 && t.hot < 1.0);
        assert!(t.idle_burn > 0.0 && t.cough_waste > 0.0 && t.out_waste > 0.0);
        for s in [
            &t.arrive_line,
            &t.bought_line,
            &t.broke_line,
            &t.no_joint_line,
            &t.lit_line,
            &t.cough_line,
            &t.out_line,
            &t.perfect_line,
        ] {
            assert!(!s.is_empty());
        }
        assert!(t.result_line.contains("{smoked}"));
    }

    #[test]
    fn the_elevator_goes_up_and_down_and_never_bounces() {
        let foot = Vec3::new(57.0, 0.0, 297.5);
        let up = ride(foot + GROUND_DOOR, foot, 1.6);
        assert_eq!(up, Some(deck(foot) + ROOF_OUT));
        assert!(on_deck(deck(foot) + ROOF_OUT, foot));
        assert_eq!(ride(deck(foot) + ROOF_OUT, foot, 1.6), None);
        let down = ride(deck(foot) + ROOF_DOOR, foot, 1.6);
        assert_eq!(down, Some(foot + GROUND_OUT));
        assert_eq!(ride(foot + GROUND_OUT, foot, 1.6), None);
    }

    #[test]
    fn the_wtc_is_on_the_map_and_the_street_door_is_outside_it() {
        let path = format!(
            "{}/../game/assets/maps/nyc.map.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(super::super::nyc::MAP.set_bytes(&bytes));
        let Some(foot) = wtc(&super::super::nyc::MAP.get()) else {
            panic!("no WTC on the map");
        };
        // WTC's base is 62 m across: the door is just outside its face, in the city.
        let door = foot + GROUND_DOOR;
        assert!(door.x < foot.x - 31.0);
        assert!(crate::rules::nyc::in_city(door.x, door.z));
        assert!(crate::rules::nyc::in_city(foot.x + GROUND_OUT.x, foot.z));
    }

    #[test]
    fn steady_drawing_smokes_it_all_and_spamming_coughs() {
        let t = t();
        let dt = 1.0 / 60.0;
        // A good rhythm: draw just often enough to keep it lit.
        let every = ((t.draw_heat / t.cool) * 0.98 / dt) as usize;
        let mut j = Joint::default();
        let mut coughs = 0;
        for i in 0..60 * 600 {
            let (n, said) = step(j, dt, (i + 1) % every.max(1) == 0, &t);
            j = n;
            if said == Some(Puff::Cough) {
                coughs += 1;
            }
            if j.done() {
                break;
            }
        }
        assert!(j.done());
        assert_eq!(coughs, 0, "a steady rhythm doesn't cough");
        assert!(perfect(&j), "smoked {} wasted {}", j.smoked, j.wasted);
        // Click every frame: lungs overflow.
        let (mut j, mut coughed) = (Joint::default(), false);
        for _ in 0..120 {
            let (n, said) = step(j, dt, true, &t);
            j = n;
            coughed |= said == Some(Puff::Cough);
        }
        assert!(coughed && j.wasted > 0.0);
        // Never draw: it goes out, and that's waste.
        let (mut j, mut out) = (Joint::default(), false);
        for _ in 0..60 * 30 {
            let (n, said) = step(j, dt, false, &t);
            j = n;
            out |= said == Some(Puff::WentOut);
        }
        assert!(out && !j.lit && j.wasted > 0.0);
        // Shares never pass the whole joint.
        assert!(j.smoked + j.wasted <= 1.0 + 1e-4);
    }

    #[test]
    fn a_perfect_smoke_is_all_smoked_nothing_wasted() {
        let mut j = Joint {
            smoked: 1.0,
            ..Joint::default()
        };
        assert!(perfect(&j));
        j.smoked = 0.97;
        j.wasted = 0.03;
        assert!(!perfect(&j));
        assert_eq!(result("{smoked}/{wasted}/{coughs}", &j), "97/3/0");
    }
}
