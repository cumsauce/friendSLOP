//! Gambling with the game's balance. From issue #173 by catherine: "allow the
//! user to gamble to adjust the settings for certain features ... things that
//! make the game more unfair or more fair ... random ... the interactions of
//! different features with each other ... have [a cat] dressed as a wizard
//! host the gambling minigame and commentate on how the player is doing".
//!
//! - **Marmalade the Magnificent**, a round orange tabby in a starry wizard
//!   hat and robe, keeps a wheel of fortune at the edge of the commune
//!   ([`GAMBLE_AT`]). Click him (or the wheel) to spin. It's free: the stake
//!   is the game's balance.
//! - The wheel has one wedge per [`Knob`] (a setting another feature reads)
//!   plus one blank wedge. A knob wedge nudges that setting one step fairer
//!   or one step crueller, at random ([`spin`]); the blank resets every
//!   setting to how it was. Each knob has five steps, the middle one is the
//!   normal game. Several knobs are where two features meet (farting and
//!   romance, harming the landlord and the helicopter, being shamed and the
//!   villagers' attack mode).
//! - The settings are shared by everyone and last the whole campaign (like
//!   morality); a new campaign starts with a fair wheel again.
//! - Marmalade commentates every spin, for everyone, on what changed and on
//!   how the spinner is doing: winning and losing streaks, up or down
//!   overall ([`commentary`]). Each player's record is a replicated
//!   [`Gambler`], shown with the house rules on the Tab scoreboard.
//!
//! The server owns [`HouseOdds`] (one replicated entity). Other features read
//! it through [`odds`], which falls back to the normal game when there is no
//! wheel (tests, menus).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fate::Fate;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::features::landlord::{RAISE_BY, Tenancy};
use crate::features::synth_audio::{RATE, master, play_synth_at};
use crate::people::{CharacterLook, Gait, HeadAnchor};
use crate::player::{Player, PlayerOwner};

/// Where Marmalade keeps his wheel (x, z): south of the bonfire, off the
/// spawn rows and the paths.
pub const GAMBLE_AT: Vec2 = Vec2::new(-10.0, 9.0);
/// Seconds between one player's spins.
pub const COOLDOWN_SECS: f32 = 4.0;
/// Chance the wheel lands on the blank wedge (everything back to normal).
pub const BLANK_CHANCE: f32 = 0.08;
/// Of the rest, the chance a spin is kind.
pub const FAIR_CHANCE: f32 = 0.5;
/// Steps per knob: 0 is the fairest, `STEPS - 1` the cruellest.
pub const STEPS: usize = 5;
/// The normal game.
pub const NORMAL: u8 = 2;
/// How long the wheel spins before it lands.
pub const SPIN_SECS: f32 = 2.6;

/// A setting the wheel can move. Append only: the replicated levels are
/// indexed by this order.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Knob {
    /// Action failing (#8): chance any action fails.
    FailChance,
    /// Farting (#18): how close an NPC has to be to see you.
    ShameRadius,
    /// Farting x romance (#97): one shameful fart in N makes the NPC fall for you.
    Romance,
    /// Shame x attack mode (#123): catches before the villagers attack.
    Strikes,
    /// Morality x helicopter (#134): how low morality gets before it hunts you.
    Helicopter,
    /// Landlord x morality (#21/#43): how much harming him raises your morality.
    LandlordBonus,
    /// Landlord (#11): how much each rent raise adds.
    Rent,
    /// Warehouse (#130): the monster's hunting speed.
    Monster,
    /// Wendy's (#117): how fast burgers restock.
    Burgers,
}

impl Knob {
    pub const ALL: [Knob; 9] = [
        Knob::FailChance,
        Knob::ShameRadius,
        Knob::Romance,
        Knob::Strikes,
        Knob::Helicopter,
        Knob::LandlordBonus,
        Knob::Rent,
        Knob::Monster,
        Knob::Burgers,
    ];

    fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }

    /// Short name on the wheel and the scoreboard.
    pub fn name(self) -> &'static str {
        match self {
            Knob::FailChance => "fate",
            Knob::ShameRadius => "nosy NPCs",
            Knob::Romance => "fart romance",
            Knob::Strikes => "the commune's patience",
            Knob::Helicopter => "the helicopter's conscience",
            Knob::LandlordBonus => "landlord karma",
            Knob::Rent => "rent",
            Knob::Monster => "the warehouse monster",
            Knob::Burgers => "Wendy's",
        }
    }

    /// The rule at `level`, in plain words.
    pub fn rule(self, level: u8) -> String {
        let o = HouseOdds::default().with(self, level);
        match self {
            Knob::FailChance => {
                format!("{:.0}% of actions fail", o.fail_chance() * 100.0)
            }
            Knob::ShameRadius => format!("NPCs see farts within {} m", o.shame_radius()),
            Knob::Romance => format!(
                "1 shameful fart in {} makes the NPC fall for you",
                o.romance_one_in()
            ),
            Knob::Strikes => format!(
                "{} fart catches set the villagers on you",
                o.strikes_to_attack()
            ),
            Knob::Helicopter => format!(
                "the helicopter comes at morality {} or lower",
                o.hunted_at()
            ),
            Knob::LandlordBonus => format!(
                "harming the landlord gives {:.0}% of the usual morality",
                o.landlord_bonus() * 100.0
            ),
            Knob::Rent => format!("each rent raise adds {}", o.rent_raise()),
            Knob::Monster => format!("the warehouse monster hunts at {} m/s", o.hunt_speed()),
            Knob::Burgers => format!("Wendy's restocks a burger in {} s", o.restock_secs()),
        }
    }
}

/// Values per step, fairest first. The middle is the normal game.
const FAIL: [f32; STEPS] = [0.01, 0.03, 0.05, 0.10, 0.20];
const SHAME: [f32; STEPS] = [2.0, 3.0, 4.0, 5.5, 7.5];
const ROMANCE: [u32; STEPS] = [1, 2, 3, 5, 9];
const STRIKES: [u8; STEPS] = [7, 5, 4, 3, 2];
const HUNTED: [i32; STEPS] = [-15, -10, -6, -4, -2];
const BONUS: [f32; STEPS] = [2.0, 1.5, 1.0, 0.6, 0.3];
const RENT: [u32; STEPS] = [5, 10, 15, 22, 33];
const HUNT: [f32; STEPS] = [4.8, 5.5, 6.2, 7.2, 8.4];
const RESTOCK: [f32; STEPS] = [5.0, 10.0, 15.0, 30.0, 60.0];

/// Replicated singleton: the current step of every knob.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct HouseOdds {
    pub levels: [u8; 9],
}

impl Default for HouseOdds {
    fn default() -> Self {
        Self {
            levels: [NORMAL; 9],
        }
    }
}

impl HouseOdds {
    pub fn level(&self, knob: Knob) -> u8 {
        self.levels[knob.index()].min(STEPS as u8 - 1)
    }

    fn step<T: Copy>(&self, knob: Knob, table: &[T; STEPS]) -> T {
        table[self.level(knob) as usize]
    }

    pub fn with(mut self, knob: Knob, level: u8) -> Self {
        self.levels[knob.index()] = level.min(STEPS as u8 - 1);
        self
    }

    pub fn fail_chance(&self) -> f32 {
        self.step(Knob::FailChance, &FAIL)
    }
    pub fn shame_radius(&self) -> f32 {
        self.step(Knob::ShameRadius, &SHAME)
    }
    pub fn romance_one_in(&self) -> u32 {
        self.step(Knob::Romance, &ROMANCE)
    }
    pub fn strikes_to_attack(&self) -> u8 {
        self.step(Knob::Strikes, &STRIKES)
    }
    pub fn hunted_at(&self) -> i32 {
        self.step(Knob::Helicopter, &HUNTED)
    }
    pub fn landlord_bonus(&self) -> f32 {
        self.step(Knob::LandlordBonus, &BONUS)
    }
    pub fn rent_raise(&self) -> u32 {
        self.step(Knob::Rent, &RENT)
    }
    pub fn hunt_speed(&self) -> f32 {
        self.step(Knob::Monster, &HUNT)
    }
    pub fn restock_secs(&self) -> f32 {
        self.step(Knob::Burgers, &RESTOCK)
    }

    /// Knobs that aren't at the normal step: (knob, fairer?).
    pub fn changed(&self) -> Vec<(Knob, bool)> {
        Knob::ALL
            .iter()
            .filter(|k| self.level(**k) != NORMAL)
            .map(|k| (*k, self.level(*k) < NORMAL))
            .collect()
    }
}

/// The house odds in play (the normal game if there's no wheel).
pub fn odds(q: &Query<&HouseOdds>) -> HouseOdds {
    q.iter().next().copied().unwrap_or_default()
}

/// Each player's record at the wheel.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Gambler {
    pub spins: u32,
    pub fair: u32,
    pub cruel: u32,
    /// Positive: kind spins in a row. Negative: cruel ones.
    pub streak: i32,
}

impl Gambler {
    /// Kind spins minus cruel ones.
    pub fn net(&self) -> i32 {
        self.fair as i32 - self.cruel as i32
    }
}

/// Client -> server: spin the wheel.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SpinWheel;

/// Server -> everyone: where the wheel landed and what Marmalade says.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Spun {
    pub player: u64,
    /// Wedge: a knob's index, or `Knob::ALL.len()` for the blank.
    pub wedge: u8,
    /// Was it good for everyone?
    pub kind: bool,
    pub text: String,
}

/// What one spin did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The knob moved one step fairer (or crueller).
    Moved { knob: Knob, fairer: bool },
    /// It was already at the end that way: nothing changed.
    Stuck { knob: Knob, fairer: bool },
    /// The blank: everything back to normal.
    Blank,
}

impl Outcome {
    pub fn wedge(self) -> u8 {
        match self {
            Outcome::Moved { knob, .. } | Outcome::Stuck { knob, .. } => knob.index() as u8,
            Outcome::Blank => Knob::ALL.len() as u8,
        }
    }

    /// Kind to the players? (The blank is neither, and counts as a push.)
    pub fn kind(self) -> Option<bool> {
        match self {
            Outcome::Moved { fairer, .. } => Some(fairer),
            _ => None,
        }
    }
}

/// Spin: `wedge_roll` picks the wedge, `luck_roll` whether it's kind. Both in
/// `0.0..1.0`. Pure, so it's tested.
pub fn spin(odds: &mut HouseOdds, wedge_roll: f32, luck_roll: f32) -> Outcome {
    if wedge_roll < BLANK_CHANCE {
        *odds = HouseOdds::default();
        return Outcome::Blank;
    }
    let k = (wedge_roll - BLANK_CHANCE) / (1.0 - BLANK_CHANCE);
    let i = ((k.clamp(0.0, 0.999_999)) * Knob::ALL.len() as f32) as usize;
    let knob = Knob::ALL[i.min(Knob::ALL.len() - 1)];
    let fairer = luck_roll < FAIR_CHANCE;
    let level = odds.level(knob);
    let next = if fairer {
        level.checked_sub(1)
    } else {
        (level + 1 < STEPS as u8).then_some(level + 1)
    };
    match next {
        Some(n) => {
            *odds = odds.with(knob, n);
            Outcome::Moved { knob, fairer }
        }
        None => Outcome::Stuck { knob, fairer },
    }
}

/// Count a spin into a player's record.
pub fn record(g: &mut Gambler, outcome: Outcome) {
    g.spins += 1;
    match outcome.kind() {
        Some(true) => {
            g.fair += 1;
            g.streak = g.streak.max(0) + 1;
        }
        Some(false) => {
            g.cruel += 1;
            g.streak = g.streak.min(0) - 1;
        }
        None => g.streak = 0,
    }
}

const KIND_QUIPS: [&str; 5] = [
    "the stars align, and so do my whiskers",
    "a kind spin! don't get used to it",
    "the wheel purrs for you today",
    "lovely. I'd clap, but paws",
    "mrrraow! fortune smiles, showing several teeth",
];

const CRUEL_QUIPS: [&str; 5] = [
    "oof. the wheel has no mercy, and neither do I",
    "a cruel spin. I'll knock something off a table in your honour",
    "the house always wins, and I am the house",
    "ah, bad luck. have you considered being luckier?",
    "the omens were bad. I read them in my food bowl",
];

/// What Marmalade says about a spin by `player` with record `g` (already
/// counted), after `outcome` left the odds at `odds`.
pub fn commentary(player: u64, g: &Gambler, outcome: Outcome, odds: &HouseOdds) -> String {
    let what = match outcome {
        Outcome::Moved { knob, fairer } => format!(
            "{}! {} is {}: {}",
            if fairer { "FAIRER" } else { "CRUELLER" },
            knob.name(),
            if fairer { "kinder" } else { "nastier" },
            knob.rule(odds.level(knob))
        ),
        Outcome::Stuck { knob, fairer } => format!(
            "{}, but {} can't get any {}. the wheel shrugs",
            knob.name(),
            knob.name(),
            if fairer { "kinder" } else { "worse" }
        ),
        Outcome::Blank => "the BLANK! every rule snaps back to normal".to_string(),
    };
    let n = g.spins as usize;
    let remark = if g.spins == 1 {
        "a first spin! beginner's luck is a myth I invented".to_string()
    } else if g.streak >= 3 {
        format!(
            "{} kind spins in a row! the house is getting nervous",
            g.streak
        )
    } else if g.streak <= -3 {
        format!(
            "{} bad spins in a row. I'd stop, but I'm a cat, I never stop",
            -g.streak
        )
    } else if matches!(outcome, Outcome::Blank) {
        "a clean slate. how dull. spin again".to_string()
    } else if outcome.kind() == Some(true) {
        KIND_QUIPS[n % KIND_QUIPS.len()].to_string()
    } else {
        CRUEL_QUIPS[n % CRUEL_QUIPS.len()].to_string()
    };
    let tally = match g.net() {
        0 => "dead even".to_string(),
        x if x > 0 => format!("up {x}"),
        x => format!("down {}", -x),
    };
    format!(
        "player {player} spins: {what}. Marmalade: \"{remark}\" (player {player} is {tally} after {} spins)",
        g.spins
    )
}

/// Server: the wheel's dice and who spun when.
#[derive(Resource)]
struct Wheel {
    seed: u32,
    last: Vec<(u64, f32)>,
}

impl Default for Wheel {
    fn default() -> Self {
        Self {
            seed: 0x6a3b_1e73,
            last: Vec::new(),
        }
    }
}

impl Wheel {
    fn roll(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<HouseOdds>()
        .replicate::<Gambler>()
        .add_client_event::<SpinWheel>(Channel::Ordered)
        .add_server_event::<Spun>(Channel::Ordered)
        .init_resource::<Wheel>()
        .add_observer(give_gambler)
        .add_observer(on_spin)
        .add_observer(new_campaign)
        .add_systems(OnEnter(ServerState::Running), spawn_odds)
        .add_systems(Update, scale_rent.run_if(in_state(ServerState::Running)));
}

fn give_gambler(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() == ServerState::Running {
        commands.entity(add.entity).insert(Gambler::default());
    }
}

fn spawn_odds(mut commands: Commands) {
    commands.spawn((HouseOdds::default(), Replicated));
}

/// A new campaign: a fair wheel and clean records.
fn new_campaign(
    _reset: On<crate::features::campaign::NewCampaign>,
    mut odds: Query<&mut HouseOdds>,
    mut gamblers: Query<&mut Gambler>,
) {
    for mut o in &mut odds {
        o.set_if_neq(HouseOdds::default());
    }
    for mut g in &mut gamblers {
        g.set_if_neq(Gambler::default());
    }
}

#[allow(clippy::too_many_arguments)]
fn on_spin(
    ev: On<FromClient<SpinWheel>>,
    mut commands: Commands,
    time: Res<Time<Real>>,
    still: Query<&super::stillness::Stillness>,
    walkers: Query<(&PlayerOwner, &Walker)>,
    mut wheel: ResMut<Wheel>,
    // `Fate` reads the odds too, so they take turns.
    mut fate_and_odds: ParamSet<(Fate, Query<&mut HouseOdds>)>,
    mut players: Query<(&Player, &PlayerOwner, &mut Gambler)>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let target = Vec3::new(GAMBLE_AT.x, 0.0, GAMBLE_AT.y);
    if !in_reach(&mut commands, ev.client_id, &walkers, target) {
        return;
    }
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((player, _, mut gambler)) = players.iter_mut().find(|(_, o, _)| o.0 == owner) else {
        return;
    };
    let now = time.elapsed_secs();
    let id = player.id;
    if let Some((_, at)) = wheel.last.iter().find(|(p, _)| *p == id)
        && now - at < COOLDOWN_SECS
    {
        deny(
            &mut commands,
            ev.client_id,
            "Marmalade: \"patience! the wheel is still dizzy\"",
        );
        return;
    }
    wheel.last.retain(|(p, _)| *p != id);
    wheel.last.push((id, now));
    if fate_and_odds.p0().fails(ev.client_id) {
        return;
    }
    let mut odds_q = fate_and_odds.p1();
    let Ok(mut odds) = odds_q.single_mut() else {
        return;
    };
    let (a, b) = (wheel.roll(), wheel.roll());
    let mut next = *odds;
    let outcome = spin(&mut next, a, b);
    odds.set_if_neq(next);
    record(&mut gambler, outcome);
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: Spun {
            player: id,
            wedge: outcome.wedge(),
            kind: !matches!(outcome, Outcome::Stuck { fairer: false, .. })
                && outcome.kind() != Some(false),
            text: commentary(id, &gambler, outcome, &next),
        },
    });
}

/// Server: each rent raise adds the gambled amount instead of the usual
/// [`RAISE_BY`] (the landlord does his own raise, this tops it up or takes
/// some back).
fn scale_rent(odds_q: Query<&HouseOdds>, mut tenancy: Query<&mut Tenancy>, mut seen: Local<u32>) {
    let Ok(mut t) = tenancy.single_mut() else {
        return;
    };
    if t.raises < *seen {
        *seen = t.raises; // a new chapter
    }
    if t.raises == *seen {
        return;
    }
    let raises = t.raises - *seen;
    *seen = t.raises;
    let want = odds(&odds_q).rent_raise();
    if want == RAISE_BY {
        return;
    }
    let delta = (want as i64 - RAISE_BY as i64) * raises as i64;
    t.rent = (t.rent as i64 + delta).max(1) as u32;
}

// ---------------------------------------------------------------- local --

pub fn plugin(app: &mut App) {
    app.add_observer(click)
        .add_observer(on_spun)
        .add_systems(
            OnEnter(AppState::InGame),
            (spawn_booth, spawn_hud, spawn_help),
        )
        .add_systems(
            Update,
            (dress_wizard, turn_wheel, update_hud).run_if(in_state(AppState::InGame)),
        );
}

/// Marmalade's body.
#[derive(Component)]
struct Wizard;

/// The wheel (spins about its local Z).
#[derive(Component, Default)]
struct WheelFace {
    angle: f32,
    from: f32,
    to: f32,
    t: f32,
    /// Shown when it lands.
    pending: Option<String>,
}

/// The words box.
#[derive(Component)]
struct WordsText;

/// House rules + records on the scoreboard.
#[derive(Component)]
struct RulesHud;

fn wizard_look() -> CharacterLook {
    CharacterLook {
        body: 1, // round critter
        height: 0.8,
        girth: 0.85,
        head: 0.75,
        limbs: 0.3,
        skin: 7,    // orange fur
        clothes: 8, // a blue-violet robe
        accent: 3,
        eyes: 2,  // sleepy, unimpressed
        ears: 2,  // pointy
        horns: 0, // none
        tail: 2,  // long
        snout: 1, // button nose
        accessory: 8,
    }
}

fn booth_at() -> Vec3 {
    let y = crate::terrain::height_at(GAMBLE_AT.x, GAMBLE_AT.y);
    Vec3::new(GAMBLE_AT.x, y, GAMBLE_AT.y)
}

/// Facing towards the commune centre.
fn booth_facing() -> Quat {
    let face = -GAMBLE_AT;
    Quat::from_rotation_y(f32::atan2(-face.x, -face.y))
}

/// Angle (about the wheel's axis) that puts `wedge` under the pointer.
fn wedge_angle(wedge: u8) -> f32 {
    let n = (Knob::ALL.len() + 1) as f32;
    -(wedge as f32) * std::f32::consts::TAU / n
}

fn spawn_booth(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let at = booth_at();
    let rot = booth_facing();
    // Marmalade, behind his table.
    commands.spawn((
        Wizard,
        Gait::default(),
        Transform::from_translation(at + rot * Vec3::new(0.0, 0.0, 0.75)).with_rotation(rot),
        Visibility::default(),
        Clickable::at_height(0.9).radius(70.0),
        wizard_look(),
        DespawnOnExit(AppState::InGame),
    ));
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let cloth = crate::art::soft(&mut materials, Color::srgb(0.35, 0.2, 0.55));
    let wood = crate::art::soft(&mut materials, Color::srgb(0.45, 0.3, 0.18));
    let gold = crate::art::glossy(&mut materials, Color::srgb(0.95, 0.78, 0.3));
    let disc = meshes.add(Cylinder::new(1.0, 1.0));
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    // A round table with a purple cloth.
    commands.spawn((
        Mesh3d(disc.clone()),
        MeshMaterial3d(cloth.clone()),
        Transform::from_translation(at + Vec3::Y * 0.25).with_scale(Vec3::new(0.55, 0.5, 0.55)),
        DespawnOnExit(AppState::InGame),
    ));
    // The wheel on a post beside him.
    let side = at + rot * Vec3::new(1.5, 0.0, 0.4);
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(wood.clone()),
        Transform::from_translation(side + Vec3::Y * 0.8).with_scale(Vec3::new(0.12, 1.6, 0.12)),
        DespawnOnExit(AppState::InGame),
    ));
    let hub = side + Vec3::Y * 1.75 + rot * Vec3::new(0.0, 0.0, -0.1);
    // Pointer at the top.
    commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(gold.clone()),
        Transform::from_translation(hub + Vec3::Y * 0.78 + rot * Vec3::new(0.0, 0.0, -0.06))
            .with_rotation(rot * Quat::from_rotation_z(std::f32::consts::FRAC_PI_4))
            .with_scale(Vec3::splat(0.12)),
        DespawnOnExit(AppState::InGame),
    ));
    let wedges = Knob::ALL.len() + 1;
    let face = commands
        .spawn((
            WheelFace::default(),
            Transform::from_translation(hub).with_rotation(rot),
            Visibility::default(),
            Clickable::at_height(0.0).radius(70.0),
            DespawnOnExit(AppState::InGame),
        ))
        .id();
    let rim = meshes.add(Torus::new(0.68, 0.74));
    let back = crate::art::soft(&mut materials, Color::srgb(0.15, 0.1, 0.25));
    commands.entity(face).with_children(|w| {
        w.spawn((
            Mesh3d(disc.clone()),
            MeshMaterial3d(back),
            Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2))
                .with_scale(Vec3::new(0.72, 0.04, 0.72)),
        ));
        w.spawn((
            Mesh3d(rim),
            MeshMaterial3d(gold.clone()),
            Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        ));
        for i in 0..wedges {
            let a = i as f32 * std::f32::consts::TAU / wedges as f32;
            let color = if i == wedges - 1 {
                Color::srgb(0.95, 0.95, 0.95) // the blank
            } else {
                Color::hsl(i as f32 * 360.0 / wedges as f32, 0.7, 0.55)
            };
            let mat = crate::art::soft(&mut materials, color);
            // Each wedge is a spoke from the hub to the rim; wedge 0 points up.
            w.spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(mat),
                Transform::from_rotation(Quat::from_rotation_z(a))
                    * Transform::from_xyz(0.0, 0.36, -0.03).with_scale(Vec3::new(0.32, 0.62, 0.03)),
            ));
        }
    });
}

/// A starry wizard hat on Marmalade's head, and a little staff.
fn dress_wizard(
    mut commands: Commands,
    new: Query<(Entity, &HeadAnchor), Added<HeadAnchor>>,
    wizards: Query<(), With<Wizard>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    for (anchor, head) in &new {
        if wizards.get(head.root).is_err() {
            continue;
        }
        let r = head.radius;
        let hat = crate::art::soft(&mut materials, Color::srgb(0.2, 0.2, 0.55));
        let star = materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.9, 0.4),
            emissive: LinearRgba::rgb(1.2, 0.9, 0.2),
            unlit: true,
            ..default()
        });
        let stripe = crate::art::soft(&mut materials, Color::srgb(0.8, 0.42, 0.15));
        let cone = meshes.add(Cone::new(1.0, 1.0));
        let disc = meshes.add(Cylinder::new(1.0, 1.0));
        let ball = meshes.add(Sphere::new(1.0));
        commands.entity(anchor).with_children(|h| {
            // Brim and a tall, slightly bent cone.
            h.spawn((
                Mesh3d(disc.clone()),
                MeshMaterial3d(hat.clone()),
                Transform::from_xyz(0.0, r * 0.75, 0.0).with_scale(Vec3::new(
                    r * 1.5,
                    r * 0.06,
                    r * 1.5,
                )),
            ));
            h.spawn((
                Mesh3d(cone.clone()),
                MeshMaterial3d(hat.clone()),
                Transform::from_xyz(0.0, r * 1.6, r * 0.1)
                    .with_rotation(Quat::from_rotation_x(-0.18))
                    .with_scale(Vec3::new(r * 0.85, r * 1.7, r * 0.85)),
            ));
            for (x, y, z) in [(0.3, 1.2, -0.55), (-0.35, 1.5, -0.4), (0.05, 1.95, -0.3)] {
                h.spawn((
                    Mesh3d(ball.clone()),
                    MeshMaterial3d(star.clone()),
                    Transform::from_xyz(r * x, r * y, r * z).with_scale(Vec3::splat(r * 0.11)),
                ));
            }
            // Tabby stripes on the forehead.
            for x in [-0.25, 0.0, 0.25] {
                h.spawn((
                    Mesh3d(ball.clone()),
                    MeshMaterial3d(stripe.clone()),
                    Transform::from_xyz(r * x, r * 0.55, -r * 0.8).with_scale(Vec3::new(
                        r * 0.07,
                        r * 0.25,
                        r * 0.07,
                    )),
                ));
            }
        });
    }
}

fn click(
    ev: On<Clicked>,
    mut commands: Commands,
    targets: Query<(), Or<(With<Wizard>, With<WheelFace>)>>,
) {
    if targets.contains(ev.entity) {
        commands.client_trigger(SpinWheel);
    }
}

/// The wheel lands where the server says, then Marmalade speaks.
fn on_spun(ev: On<Spun>, mut commands: Commands, mut wheels: Query<&mut WheelFace>) {
    for mut w in &mut wheels {
        let target = wedge_angle(ev.wedge);
        let tau = std::f32::consts::TAU;
        // A few whole turns, then the wedge.
        let base = w.angle - w.angle.rem_euclid(tau);
        w.from = w.angle;
        w.to = base - 3.0 * tau + target.rem_euclid(tau) - tau;
        w.t = 0.0;
        w.pending = Some(ev.text.clone());
    }
    let kind = ev.kind;
    play_synth_at(
        &mut commands,
        booth_at() + Vec3::Y * 1.7,
        crate::spatial::Carry::NEARBY,
        0.5,
        move || crate::features::bear::wav_bytes(&ticks(kind, RATE), RATE),
    );
}

fn turn_wheel(
    time: Res<Time>,
    mut wheels: Query<(&mut WheelFace, &mut Transform)>,
    mut words: Query<&mut Text, With<WordsText>>,
) {
    let rot = booth_facing();
    for (mut w, mut tf) in &mut wheels {
        if w.pending.is_none() {
            continue;
        }
        w.t = (w.t + time.delta_secs() / SPIN_SECS).min(1.0);
        // Ease out: fast, then clicking to a stop.
        let e = 1.0 - (1.0 - w.t).powi(3);
        w.angle = w.from + (w.to - w.from) * e;
        tf.rotation = rot * Quat::from_rotation_z(w.angle);
        if w.t >= 1.0
            && let Some(text) = w.pending.take()
        {
            for mut t in &mut words {
                t.0.clone_from(&text);
            }
        }
    }
}

/// The wheel's clicks slowing down, then a chime (bright if kind, a sour
/// buzz if cruel).
fn ticks(kind: bool, rate: u32) -> Vec<f32> {
    let total = SPIN_SECS + 0.9;
    let n = (total * rate as f32) as usize;
    let mut out = vec![0.0f32; n];
    // Tick times follow the same ease-out as the wheel.
    let ticks_total = 3.0 * (Knob::ALL.len() + 1) as f32;
    let mut last = 0usize;
    for i in 0..n {
        let t = i as f32 / rate as f32;
        if t < SPIN_SECS {
            let k = t / SPIN_SECS;
            let passed = ((1.0 - (1.0 - k).powi(3)) * ticks_total) as usize;
            if passed > last {
                last = passed;
                for j in 0..(0.012 * rate as f32) as usize {
                    if let Some(s) = out.get_mut(i + j) {
                        let tt = j as f32 / rate as f32;
                        *s += (tt * 2400.0 * std::f32::consts::TAU).sin() * (-tt * 400.0).exp();
                    }
                }
            }
        } else {
            let tt = t - SPIN_SECS;
            let env = (-tt * 4.0).exp();
            let v = if kind {
                [880.0, 1108.7, 1318.5]
                    .iter()
                    .map(|f| (tt * f * std::f32::consts::TAU).sin())
                    .sum::<f32>()
            } else {
                let p = (tt * 110.0).fract();
                (p * 2.0 - 1.0) + 0.5 * ((tt * 116.0).fract() * 2.0 - 1.0)
            };
            out[i] += v * env * 0.35;
        }
    }
    master(&mut out, rate, 9000.0, 0.7);
    out
}

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        WordsText,
        TextFont::from_font_size(17.0),
        TextColor(Color::srgb(1.0, 0.82, 0.45)),
        TextLayout::justify(Justify::Center),
        crate::hud::slot(crate::hud::HudSlot::Subtitle),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(13.0),
        RulesHud,
        crate::hud::slot(crate::hud::HudSlot::Scoreboard),
        DespawnOnExit(AppState::InGame),
    ));
}

/// The house rules, as the scoreboard shows them.
pub fn rules_table(odds: &HouseOdds, records: &[(u64, Gambler)]) -> String {
    let mut lines = vec!["house rules (Marmalade's wheel)".to_string()];
    let changed = odds.changed();
    if changed.is_empty() {
        lines.push("all normal: spin the wheel to gamble with them".into());
    }
    for (knob, fairer) in changed {
        lines.push(format!(
            "{} {}: {}",
            if fairer { "+" } else { "-" },
            knob.name(),
            knob.rule(odds.level(knob))
        ));
    }
    for (id, g) in records.iter().filter(|(_, g)| g.spins > 0) {
        lines.push(format!(
            "player {id}: {} spins, {} kind, {} cruel",
            g.spins, g.fair, g.cruel
        ));
    }
    lines.join("\n")
}

fn update_hud(
    odds_q: Query<&HouseOdds>,
    gamblers: Query<(&Player, &Gambler)>,
    mut hud: Query<&mut Text, With<RulesHud>>,
) {
    let mut records: Vec<(u64, Gambler)> = gamblers.iter().map(|(p, g)| (p.id, *g)).collect();
    records.sort_by_key(|(id, _)| *id);
    let want = rules_table(&odds(&odds_q), &records);
    for mut text in &mut hud {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

fn spawn_help(mut commands: Commands) {
    commands.spawn((
        Text::new(
            "Marmalade's wheel (south of the bonfire): spin to make the game fairer or crueller for everyone. Tab shows the house rules",
        ),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_middle_step_is_the_normal_game() {
        let o = HouseOdds::default();
        assert_eq!(o.fail_chance(), crate::features::fate::FAIL_CHANCE);
        assert_eq!(o.shame_radius(), crate::features::fart::SHAME_RADIUS);
        assert_eq!(o.romance_one_in(), crate::features::romance::SMITTEN_ONE_IN);
        assert_eq!(
            o.strikes_to_attack(),
            crate::features::punishment::STRIKES_TO_ATTACK
        );
        assert_eq!(o.hunted_at(), crate::features::helicopter::HUNTED_AT);
        assert_eq!(o.landlord_bonus(), 1.0);
        assert_eq!(o.rent_raise(), RAISE_BY);
        assert_eq!(o.hunt_speed(), crate::features::warehouse::HUNT_SPEED);
        assert_eq!(o.restock_secs(), crate::features::wendys::RESTOCK_SECS);
        assert!(o.changed().is_empty());
    }

    #[test]
    fn spins_move_one_step_and_stop_at_the_ends() {
        let mut o = HouseOdds::default();
        // Just past the blank: the first knob. Kind roll.
        let first = BLANK_CHANCE + 0.001;
        assert_eq!(
            spin(&mut o, first, 0.0),
            Outcome::Moved {
                knob: Knob::FailChance,
                fairer: true
            }
        );
        assert_eq!(o.level(Knob::FailChance), 1);
        spin(&mut o, first, 0.0);
        assert_eq!(
            spin(&mut o, first, 0.0),
            Outcome::Stuck {
                knob: Knob::FailChance,
                fairer: true
            }
        );
        for _ in 0..6 {
            spin(&mut o, first, 0.99);
        }
        assert_eq!(o.level(Knob::FailChance), STEPS as u8 - 1);
        assert_eq!(o.changed(), vec![(Knob::FailChance, false)]);
        assert_eq!(spin(&mut o, 0.0, 0.5), Outcome::Blank);
        assert_eq!(o, HouseOdds::default());
    }

    #[test]
    fn every_knob_can_come_up() {
        let mut seen = std::collections::HashSet::new();
        for i in 0..1000 {
            let mut o = HouseOdds::default();
            seen.insert(spin(&mut o, i as f32 / 1000.0, 0.9).wedge());
        }
        assert_eq!(seen.len(), Knob::ALL.len() + 1);
    }

    #[test]
    fn fair_is_kinder_for_every_knob() {
        let fair = HouseOdds { levels: [0; 9] };
        let cruel = HouseOdds { levels: [4; 9] };
        assert!(fair.fail_chance() < cruel.fail_chance());
        assert!(fair.shame_radius() < cruel.shame_radius());
        assert!(fair.romance_one_in() < cruel.romance_one_in());
        assert!(fair.strikes_to_attack() > cruel.strikes_to_attack());
        assert!(fair.hunted_at() < cruel.hunted_at());
        assert!(fair.landlord_bonus() > cruel.landlord_bonus());
        assert!(fair.rent_raise() < cruel.rent_raise());
        assert!(fair.hunt_speed() < cruel.hunt_speed());
        assert!(fair.restock_secs() < cruel.restock_secs());
        // Harming the landlord is still always rewarded (#43), just less.
        assert!(cruel.landlord_bonus() > 0.0);
    }

    #[test]
    fn marmalade_follows_the_streaks() {
        let mut g = Gambler::default();
        let o = HouseOdds::default();
        let kind = Outcome::Moved {
            knob: Knob::Rent,
            fairer: true,
        };
        let cruel = Outcome::Moved {
            knob: Knob::Rent,
            fairer: false,
        };
        record(&mut g, kind);
        assert!(commentary(1, &g, kind, &o).contains("first spin"));
        record(&mut g, kind);
        record(&mut g, kind);
        assert_eq!(g.streak, 3);
        assert!(commentary(1, &g, kind, &o).contains("3 kind spins in a row"));
        assert!(commentary(1, &g, kind, &o).contains("up 3"));
        for _ in 0..6 {
            record(&mut g, cruel);
        }
        assert_eq!(g.streak, -6);
        let line = commentary(1, &g, cruel, &o);
        assert!(line.contains("6 bad spins in a row"), "{line}");
        assert!(line.contains("down 3"), "{line}");
    }

    #[test]
    fn the_booth_is_somewhere_open() {
        let p = GAMBLE_AT;
        assert!(!crate::terrain::is_pond(p.x, p.y));
        assert!(!crate::terrain::is_path(p.x, p.y));
        assert_eq!(super::super::villagers::outside_arena(p), p);
        assert!(p.distance(crate::scenery::BONFIRE_AT.xz()) > 5.0);
        assert!(
            p.distance(crate::features::dave_commune::PARLIAMENT_AT) > 12.0,
            "clear of the parliament ruin"
        );
    }

    #[test]
    fn the_tick_sound_is_finite() {
        let s = ticks(true, 8000);
        assert!(s.iter().all(|x| x.is_finite()));
        assert!(s.iter().any(|x| x.abs() > 0.1));
    }
}
