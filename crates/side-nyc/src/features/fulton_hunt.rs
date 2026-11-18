//! Andrew calls, and sends someone. From the user, for Fulton St station:
//! "once sufficiently deep in the subway station, your phone will get a call
//! from andrew. you answer it, and he asks where you are. you say youre
//! coming. he sounds a bit upset but is ok. he calls 2 more times with
//! increasing anger. that last time, he says hes sending someone to go get
//! you. it is a monster that will persue you around the subway. you can evade
//! him by hiding (he has a view cone but has like the alien isolation director
//! ai)", and "andrews lines should be spoken with a SAM".
//!
//! **Calls** (server decides, the phone is yours): the first time you reach
//! each of [`Hunt::call_levels`] your phone rings. Press E (or let it ring
//! out) and the call plays: subtitles, with Andrew's lines spoken by a retro
//! speech synth (`samiam`, pure Rust), higher and faster as he gets angrier.
//! Lines in `data/text/fulton_calls.ron`.
//!
//! **The hunter** (server, replicated): a while after the third call it
//! arrives on your level, as far from you as it can. Its brain is pure
//! ([`think`]) and works like Alien: Isolation's two-layer AI: a *director*
//! that always roughly knows where you are and every few seconds sends it to
//! a cell near you (never exactly yours), and the creature's own senses, a
//! view cone with line of sight down the corridors, and ears (`noise`):
//! running or jumping carries, and it comes; stand still and you're silent.
//! Seen or heard, it chases you; lose
//! it and it goes where it last saw you, then searches the lockers nearby
//! (opening yours with [`Hunt::find_chance`]), then backs off for a while
//! before the director brings it drifting back. It walks the station's own
//! plan, stairs included.
//!
//! **Hiding**: the lockers piece has a locker you can climb into (click it).
//! Inside, its eyes can't find you; walk out to leave.
//!
//! **Caught**: it drags you all the way back up to the street, and Andrew
//! starts calling again from the top. (The game's other punishments are
//! deaths and floats; this one costs what the station is about: the walk.)

use std::collections::HashMap;

use crate::sound::AudioSample;
use crate::speech;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_wfc as wfc;
use serde::{Deserialize, Serialize};

use super::fulton::Hunt;
use super::fulton::{
    CELL, HIDE_AT, Kind, LOCKER_DOORS_X, Level, LevelModels, Station, TILES, TileSet, Tuning,
    cell_of, cell_world, floor_at, level_at, level_of, map_cell, street_at, tuning,
};
use crate::AppState;
use crate::features::fps::{Hiding, WalkInput, Walker, teleport};
use crate::features::interact::{Clickable, Clicked};
use crate::features::noise::{Level as Loud, Noise};
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::rules::nyc::in_station;

/// Writing: `assets/data/text/fulton_calls.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Writing {
    /// The ringing banner.
    pub ringing: String,
    /// Each call: (who, line).
    pub calls: Vec<Vec<(String, String)>>,
    /// Toast when the hunter arrives.
    pub sent: String,
    /// Toast when it starts chasing you.
    pub seen: String,
    /// Toast when it gets you.
    pub caught: String,
    /// Toasts climbing into and out of a locker.
    pub hidden: String,
    pub unhidden: String,
    /// The lockers' click label.
    pub hide_label: String,
}

pub static WRITING: friendslop_data::Data<Writing> =
    friendslop_data::Data::ron("data/text/fulton_calls.ron");

// ------------------------------------------------------------------ the map as a graph

/// A place in the station: (level, x, z).
pub type Node = (usize, usize, usize);

/// Where you can step from `n`: through open sides, and up or down a stairway.
pub fn neighbours(levels: &[Level], n: Node) -> Vec<Node> {
    let (li, x, z) = n;
    let Some(l) = levels.get(li) else {
        return Vec::new();
    };
    let mut out: Vec<Node> = wfc::Side::ALL
        .into_iter()
        .filter(|&s| l.open((x, z), s))
        .filter_map(|s| wfc::neighbour(l.width, l.depth, (x, z), s))
        .map(|c| (li, c.0, c.1))
        .collect();
    // (Levels differ in size: the stairs land on the same spot in the world.)
    if (x, z) == l.goal
        && l.fill(l.goal).kind == Kind::Stairs
        && let Some(below) = levels.get(li + 1)
        && let Some(c) = map_cell(l, below, (x, z))
    {
        out.push((li + 1, c.0, c.1));
    }
    if li > 0
        && (x, z) == l.arrive
        && l.fill(l.arrive).kind == Kind::Landing
        && let Some(c) = map_cell(l, &levels[li - 1], (x, z))
    {
        out.push((li - 1, c.0, c.1));
    }
    out
}

/// Steps from `from` to every node within `radius` (breadth first).
pub fn around(levels: &[Level], from: Node, radius: u32) -> Vec<(Node, u32)> {
    let mut seen = vec![(from, 0)];
    let mut i = 0;
    while i < seen.len() {
        let (n, d) = seen[i];
        i += 1;
        if d >= radius {
            continue;
        }
        for m in neighbours(levels, n) {
            if !seen.iter().any(|s| s.0 == m) {
                seen.push((m, d + 1));
            }
        }
    }
    seen
}

/// The shortest way from `from` to `to` (without `from`), or `None`.
pub fn route(levels: &[Level], from: Node, to: Node) -> Option<Vec<Node>> {
    if from == to {
        return Some(Vec::new());
    }
    let mut back: HashMap<Node, Node> = HashMap::new();
    let mut queue = std::collections::VecDeque::from([from]);
    while let Some(n) = queue.pop_front() {
        for m in neighbours(levels, n) {
            if m == from || back.contains_key(&m) {
                continue;
            }
            back.insert(m, n);
            if m == to {
                let mut out = vec![to];
                let mut at = to;
                while let Some(&p) = back.get(&at) {
                    if p == from {
                        break;
                    }
                    out.push(p);
                    at = p;
                }
                out.reverse();
                return Some(out);
            }
            queue.push_back(m);
        }
    }
    None
}

/// The node a body with feet at `p` is in.
pub fn node_at(t: &Tuning, levels: &[Level], p: Vec3) -> Option<Node> {
    if !in_station(p) {
        return None;
    }
    // (Down in an atrium is still the atrium's level.)
    let li = level_of(t, levels, p);
    let l = levels.get(li)?;
    let c = cell_of(t, l, p)?;
    Some((li, c.0, c.1))
}

/// The floor in the middle of a node.
pub fn node_pos(t: &Tuning, levels: &[Level], n: Node) -> Vec3 {
    let Some(l) = levels.get(n.0) else {
        return cell_world(t, n.0, &Level::core(t), (n.1, n.2));
    };
    cell_world(t, n.0, l, (n.1, n.2))
}

/// Every hiding spot: its node and where you stand inside it.
pub fn hiding_spots(t: &Tuning, set: &TileSet, levels: &[Level]) -> Vec<(Node, Vec3)> {
    let mut out = Vec::new();
    for (li, l) in levels.iter().enumerate() {
        for z in 0..l.depth {
            for x in 0..l.width {
                let f = l.fill((x, z));
                let Kind::Corridor(Some(i)) = f.kind else {
                    continue;
                };
                if set.pieces.get(i as usize).is_some_and(|p| p.hide) {
                    let at = cell_world(t, li, l, (x, z)) + f.rotation() * HIDE_AT;
                    out.push(((li, x, z), at));
                }
            }
        }
    }
    out
}

/// Can something standing at `from` facing `yaw` see someone standing at `to`
/// (feet, both)? Same level, close enough,
/// inside the cone, and nothing but open corridor between.
pub fn sees(t: &Tuning, levels: &[Level], from: Vec3, yaw: f32, to: Vec3) -> bool {
    let h = &t.hunt;
    if level_of(t, levels, from) != level_of(t, levels, to) {
        return false;
    }
    let d = (to - from).xz();
    let len = d.length();
    if len > h.view_range {
        return false;
    }
    if len > 0.5 {
        let facing = Vec2::new(yaw.sin(), yaw.cos());
        if facing.angle_to(d).abs().to_degrees() > h.view_degrees / 2.0 {
            return false;
        }
    }
    let steps = (len / 0.4).ceil().max(1.0) as usize;
    let mut prev = node_at(t, levels, from);
    for i in 1..=steps {
        let p = from.lerp(to, i as f32 / steps as f32);
        let here = node_at(t, levels, p);
        if here != prev {
            let (Some(a), Some(b)) = (prev, here) else {
                return false;
            };
            if a.0 != b.0 || !neighbours(levels, a).contains(&b) {
                return false;
            }
        }
        prev = here;
    }
    true
}

// ------------------------------------------------------------------ the brain

/// What it's doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Mode {
    /// The director sends it near you.
    #[default]
    Drift,
    /// It sees you.
    Chase,
    /// Checking lockers where it lost you.
    Search,
    /// It has had eyes on you long enough: curled into a ball and rolling
    /// at you, faster than you run. Breaks back into a run at the end.
    Roll,
    /// Backing off for a while.
    Retreat,
}

/// The hunter as everyone sees it (replicated).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Hunter {
    pub pos: Vec3,
    pub yaw: f32,
    pub mode: Mode,
}

/// The hunter's mind (server only).
#[derive(Component, Debug)]
pub struct Brain {
    /// The player Andrew sent it after.
    pub target: u64,
    pub route: Vec<Node>,
    pub timer: f32,
    pub last_seen: Option<Node>,
    pub searched: Vec<Node>,
    pub rng: wfc::Rng,
    /// Seconds it has had you in clear sight without blinking.
    pub sighted: f32,
    /// Seconds left of the roll it is in.
    pub rolling: f32,
    /// Seconds before it may roll again.
    pub roll_cd: f32,
    /// Seconds left of the director's calm after it gives up on you: it
    /// wanders away from you instead of being sent back.
    pub calm: f32,
}

impl Brain {
    pub fn new(target: u64, seed: u32) -> Self {
        Self {
            target,
            route: Vec::new(),
            timer: 0.0,
            last_seen: None,
            searched: Vec::new(),
            rng: wfc::Rng::new(seed | 1),
            sighted: 0.0,
            rolling: 0.0,
            roll_cd: 0.0,
            calm: 0.0,
        }
    }
}

/// A player as the brain knows them.
#[derive(Clone, Copy, Debug)]
pub struct Prey {
    pub id: u64,
    pub feet: Vec3,
    /// In a locker (at this spot).
    pub hidden: Option<Vec3>,
}

/// What a tick decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    /// It started chasing this player.
    Spotted(u64),
    /// It got this player.
    Caught(u64),
    /// Its target left the station: it goes too.
    Gone,
}

/// One tick of the hunter. Pure: the map, the players and a seeded generator in,
/// a new position and a decision out.
pub fn think(
    t: &Tuning,
    levels: &[Level],
    spots: &[(Node, Vec3)],
    hunter: &mut Hunter,
    brain: &mut Brain,
    prey: &[Prey],
    noises: &[crate::features::noise::Noise],
    dt: f32,
) -> Outcome {
    let h = &t.hunt;
    let Some(me) = node_at(t, levels, hunter.pos) else {
        return Outcome::Gone;
    };
    let Some(target) = prey.iter().find(|p| p.id == brain.target) else {
        return Outcome::Gone;
    };
    let Some(target_node) = node_at(t, levels, target.feet) else {
        return Outcome::Gone;
    };
    let mut out = Outcome::Nothing;
    // Close enough to grab someone out in the open.
    // (Never while the profiling bot drives.)
    for p in prey
        .iter()
        .filter(|p| p.hidden.is_none() && !super::fulton::bot_run())
    {
        if level_at(p.feet.y) == level_at(hunter.pos.y)
            && p.feet.xz().distance(hunter.pos.xz()) < h.catch_range
        {
            hunter.mode = Mode::Retreat;
            brain.route.clear();
            brain.timer = 0.0;
            return Outcome::Caught(p.id);
        }
    }
    brain.roll_cd = (brain.roll_cd - dt).max(0.0);
    brain.calm = (brain.calm - dt).max(0.0);
    // Its eyes.
    let seen = prey
        .iter()
        .filter(|p| p.hidden.is_none())
        .find(|p| sees(t, levels, hunter.pos, hunter.yaw, p.feet));
    // Clear sight for long enough and it stops pretending to be a person.
    if seen.is_some() && hunter.mode != Mode::Roll {
        brain.sighted += dt;
    } else if seen.is_none() {
        brain.sighted = 0.0;
    }
    if seen.is_some()
        && hunter.mode != Mode::Roll
        && brain.sighted >= h.roll_sight_secs
        && brain.roll_cd <= 0.0
    {
        hunter.mode = Mode::Roll;
        brain.rolling = h.roll_secs;
        brain.roll_cd = h.roll_cooldown_secs;
        brain.route.clear();
    }
    if let Some(p) = seen {
        let at = node_at(t, levels, p.feet);
        if hunter.mode != Mode::Chase {
            out = Outcome::Spotted(p.id);
        }
        if hunter.mode != Mode::Chase || brain.last_seen != at {
            brain.route = at.and_then(|a| route(levels, me, a)).unwrap_or_default();
        }
        if hunter.mode != Mode::Roll {
            hunter.mode = Mode::Chase;
        }
        brain.last_seen = at;
        brain.timer = h.lose_secs;
    }
    // Its ears. Running and jumping carry (`noise`); you heard, it comes —
    // without seeing you, so no toast, and standing still stays silent.
    let was_last = brain.last_seen;
    if seen.is_none()
        && let Some(n) = crate::features::noise::heard(noises, hunter.pos)
        && let Some(at) = node_at(t, levels, n.at)
    {
        if hunter.mode != Mode::Chase {
            hunter.mode = Mode::Chase;
            brain.searched.clear();
        }
        brain.last_seen = Some(at);
        brain.timer = h.lose_secs;
        if was_last != Some(at) || brain.route.is_empty() {
            brain.route = route(levels, me, at).unwrap_or_default();
        }
    }
    brain.timer -= dt;
    match hunter.mode {
        Mode::Chase if seen.is_none() => {
            // Lost sight of you, but not yet given up: it keeps coming, still
            // headed your way, until `lose_secs` runs out. You in a locker it
            // can't track: it goes where it last had eyes on you.
            if brain.route.is_empty() && brain.timer > 0.0 {
                let to = match target.hidden {
                    None => Some(target_node),
                    // You in a locker it can't track: it goes where it last
                    // had eyes on you.
                    Some(_) => brain.last_seen,
                };
                if let Some(to) = to.filter(|&n| n != me) {
                    brain.route = route(levels, me, to).unwrap_or_default();
                }
            }
            if brain.timer <= 0.0 {
                hunter.mode = Mode::Search;
                brain.timer = h.search_secs;
                brain.searched.clear();
            }
        }
        Mode::Chase => {}
        Mode::Roll => {
            brain.rolling -= dt;
            // Out of the roll and straight back into the run at you.
            if brain.rolling <= 0.0 {
                hunter.mode = if seen.is_some() {
                    Mode::Chase
                } else {
                    Mode::Search
                };
                brain.timer = if seen.is_some() {
                    h.lose_secs
                } else {
                    h.search_secs
                };
                brain.sighted = 0.0;
            }
        }
        Mode::Search => {
            if brain.route.is_empty() {
                // At a locker? Try it.
                if let Some(&(_, spot)) = spots.iter().find(|s| s.0 == me) {
                    brain.searched.push(me);
                    let inside = prey
                        .iter()
                        .find(|p| p.hidden.is_some_and(|s| s.distance(spot) < 0.5));
                    if let Some(p) = inside
                        && brain.rng.roll() < h.find_chance
                    {
                        hunter.mode = Mode::Retreat;
                        brain.timer = 0.0;
                        return Outcome::Caught(p.id);
                    }
                }
                let from = brain.last_seen.unwrap_or(me);
                let near = around(levels, from, h.search_radius);
                let next = near
                    .iter()
                    .filter(|(n, _)| spots.iter().any(|s| s.0 == *n) && !brain.searched.contains(n))
                    .map(|(n, _)| *n)
                    .next()
                    .unwrap_or_else(|| near[brain.rng.below(near.len())].0);
                brain.route = route(levels, me, next).unwrap_or_default();
                if next == me {
                    brain.searched.push(me);
                }
            }
            if brain.timer <= 0.0 {
                // Back off, away from where the director says you are, and
                // leave you be for a while after.
                hunter.mode = Mode::Retreat;
                brain.timer = h.retreat_secs;
                brain.calm = h.retreat_secs + h.calm_secs;
                let far = around(levels, me, h.retreat_cells + 4)
                    .into_iter()
                    .filter(|(n, d)| {
                        *d >= h.retreat_cells.min(3)
                            && around_dist(levels, *n, target_node, h.retreat_cells)
                    })
                    .map(|(n, _)| n)
                    .collect::<Vec<_>>();
                let to = if far.is_empty() {
                    me
                } else {
                    far[brain.rng.below(far.len())]
                };
                brain.route = route(levels, me, to).unwrap_or_default();
            }
        }
        Mode::Retreat => {
            if brain.timer <= 0.0 {
                hunter.mode = Mode::Drift;
                brain.timer = 0.0;
                brain.route.clear();
            }
        }
        Mode::Drift => {
            if brain.timer <= 0.0 || brain.route.is_empty() {
                let pick = drift_to(t, levels, me, target_node, target.hidden.is_some(), brain);
                brain.route = route(levels, me, pick).unwrap_or_default();
                brain.timer = h.director_secs;
            }
        }
    }
    // The ball keeps to the corridors too: a route to you (or where it last
    // had you), cell to cell, only faster.
    if hunter.mode == Mode::Roll {
        let goal = seen
            .and_then(|p| node_at(t, levels, p.feet))
            .or(brain.last_seen);
        if let Some(g) = goal.filter(|&g| g != me)
            && brain.route.last() != Some(&g)
        {
            brain.route = route(levels, me, g).unwrap_or_default();
        }
    }
    walk(t, levels, hunter, brain, dt);
    // In the same cell as whoever it's chasing: straight at them.
    if let Some(p) = seen
        && brain.route.is_empty()
    {
        let d = (p.feet - hunter.pos).xz();
        if d.length() > 0.01 {
            let step = d.normalize() * (mode_speed(t, hunter.mode) * dt).min(d.length());
            hunter.pos += Vec3::new(step.x, 0.0, step.y);
            hunter.yaw = d.x.atan2(d.y);
        }
    }
    out
}

/// Where the director sends it next while it drifts. Normally roughly where
/// you are (within `director_radius` cells, never exactly). But while you're
/// hidden it has nothing to go on, and for a while after it gives up on you
/// it's calm: either way it wanders somewhere at least `calm_cells` from you,
/// so a locker is a real way out.
pub fn drift_to(
    t: &Tuning,
    levels: &[Level],
    me: Node,
    you: Node,
    you_hidden: bool,
    brain: &mut Brain,
) -> Node {
    let h = &t.hunt;
    if you_hidden || brain.calm > 0.0 {
        let away = around(levels, me, h.wander_cells)
            .into_iter()
            .filter(|(n, _)| around_dist(levels, *n, you, h.calm_cells))
            .map(|(n, _)| n)
            .collect::<Vec<_>>();
        if !away.is_empty() {
            return away[brain.rng.below(away.len())];
        }
    }
    let near = around(levels, you, h.director_radius);
    near[brain.rng.below(near.len())].0
}

/// Is `n` at least `cells` steps from `from` (or out of reach)?
fn around_dist(levels: &[Level], n: Node, from: Node, cells: u32) -> bool {
    !around(levels, from, cells.saturating_sub(1))
        .iter()
        .any(|(m, _)| *m == n)
}

/// Where it stands in a node: the middle, on its floor (down in a tall
/// room's sunken cells).
pub fn node_floor(t: &Tuning, levels: &[Level], n: Node) -> Vec3 {
    let p = node_pos(t, levels, n);
    p.with_y(floor_at(t, levels, p))
}

/// How fast it moves in a mode (m/s): the walk and run clips are played to match.
fn mode_speed(t: &Tuning, mode: Mode) -> f32 {
    match mode {
        Mode::Roll => t.hunt.roll_speed,
        Mode::Chase => t.hunt.chase_speed,
        _ => t.hunt.walk_speed,
    }
}

/// Move along the route (on the floor: up and down a tall room's stairs).
fn walk(t: &Tuning, levels: &[Level], hunter: &mut Hunter, brain: &mut Brain, dt: f32) {
    let speed = mode_speed(t, hunter.mode);
    let mut left = speed * dt;
    while left > 0.0 {
        let Some(&next) = brain.route.first() else {
            break;
        };
        let to = node_floor(t, levels, next);
        let same_level = level_at(to.y) == level_at(hunter.pos.y);
        let d = to - hunter.pos;
        let len = d.length();
        if d.xz().length() > 0.01 {
            hunter.yaw = d.x.atan2(d.z);
        }
        if len <= left {
            hunter.pos = to;
            left -= len;
            brain.route.remove(0);
        } else {
            hunter.pos += d / len * left;
            if same_level {
                hunter.pos.y = floor_at(t, levels, hunter.pos);
            }
            left = 0.0;
        }
    }
}

/// Where it turns up: on your level, as far from you as it can get.
pub fn arrival(levels: &[Level], you: Node) -> Node {
    around(levels, you, 1000)
        .into_iter()
        .filter(|(n, _)| n.0 == you.0)
        .max_by_key(|(_, d)| *d)
        .map_or(you, |(n, _)| n)
}

/// Which call (0-based) a player who has had `had` calls gets at `level`, if any.
pub fn call_due(t: &Tuning, had: usize, level: usize) -> Option<usize> {
    t.hunt
        .call_levels
        .get(had)
        .is_some_and(|&l| level >= l)
        .then_some(had)
}

// ------------------------------------------------------------------ server

/// Server → one client: something about your phone or your pursuer.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phone {
    /// Andrew is calling (which call, 0-based).
    Call(u8),
    /// The one he sent is here.
    Sent,
    /// It saw you.
    Seen,
    /// It got you.
    Caught,
    /// You're in a locker (or out of it).
    Hidden(bool),
}

/// Client → server: climb into (or out of) the nearest locker.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Hide;

/// Server: who's had which calls, who's sent for, who's hiding.
#[derive(Resource, Default)]
struct Hunted {
    calls: HashMap<u64, usize>,
    /// Seconds until the hunter arrives for this player.
    coming: HashMap<u64, f32>,
    hiding: HashMap<u64, Vec3>,
    ticks: u32,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Hunter>()
        .add_server_event::<Phone>(Channel::Ordered)
        .add_client_event::<Hide>(Channel::Ordered)
        .add_observer(hide)
        .init_resource::<Hunted>()
        .add_systems(
            Update,
            (calls, hunt)
                .chain()
                // The ears follow the sounds the noise system gathered.
                .after(crate::features::noise::NoiseSet)
                .run_if(in_state(ServerState::Running))
                // The phones and the hunters are under New York (#412).
                .run_if(crate::sites::awake("nyc"))
                .run_if(resource_exists::<Station>),
        );
}

fn client_of(owner: &PlayerOwner) -> ClientId {
    owner.0.map_or(ClientId::Server, ClientId::Client)
}

fn tell(commands: &mut Commands, owner: &PlayerOwner, what: Phone) {
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(client_of(owner)),
        message: what,
    });
}

/// Server: ring phones; after the third call, send it.
fn calls(
    mut commands: Commands,
    time: Res<Time>,
    station: Res<Station>,
    mut hunted: ResMut<Hunted>,
    players: Query<(&Player, &PlayerOwner, &Walker)>,
    hunters: Query<&Brain>,
) {
    let t = tuning();
    let dt = time.delta_secs();
    for (p, owner, w) in &players {
        if !in_station(w.pos) {
            continue;
        }
        let had = hunted.calls.get(&p.id).copied().unwrap_or(0);
        if let Some(n) = call_due(&t, had, level_at(w.pos.y)) {
            hunted.calls.insert(p.id, n + 1);
            tell(&mut commands, owner, Phone::Call(n as u8));
            if n + 1 == t.hunt.call_levels.len() {
                hunted.coming.insert(p.id, t.hunt.spawn_delay);
            }
        }
        let Some(left) = hunted.coming.get_mut(&p.id) else {
            continue;
        };
        *left -= dt;
        if *left > 0.0 || hunters.iter().any(|b| b.target == p.id) {
            continue;
        }
        hunted.coming.remove(&p.id);
        let Some(you) = node_at(&t, station.levels(), w.pos) else {
            continue;
        };
        let at = node_floor(&t, station.levels(), arrival(station.levels(), you));
        hunted.ticks = hunted.ticks.wrapping_add(1);
        commands.spawn((
            Replicated,
            Hunter {
                pos: at,
                yaw: 0.0,
                mode: Mode::Drift,
            },
            Brain::new(
                p.id,
                t.seed ^ (p.id as u32) ^ hunted.ticks.wrapping_mul(0x9e37),
            ),
            DespawnOnExit(AppState::InGame),
        ));
        tell(&mut commands, owner, Phone::Sent);
    }
}

/// Server: the hunters think, move, and grab.
#[allow(clippy::type_complexity)]
fn hunt(
    mut commands: Commands,
    time: Res<Time>,
    station: Res<Station>,
    noises: Res<crate::features::noise::Noises>,
    mut hunted: ResMut<Hunted>,
    mut hunters: Query<(Entity, &mut Hunter, &mut Brain)>,
    mut players: Query<(
        Entity,
        &Player,
        &PlayerOwner,
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    if hunters.is_empty() && hunted.hiding.is_empty() {
        return;
    }
    let t = tuning();
    let set = TILES.get();
    let spots = hiding_spots(&t, &set, station.levels());
    // Walk out of a locker and you're out (headless and fallback walking;
    // with the controller the walker can't move while `Hiding`). And whoever
    // isn't in the map any more isn't hidden: drop their pin.
    let mut left_locker = Vec::new();
    for (e, p, owner, w, _, _) in &players {
        match hunted.hiding.get(&p.id) {
            Some(spot) if w.pos.xz().distance(spot.xz()) > 0.9 => {
                left_locker.push(p.id);
                commands.entity(e).remove::<Hiding>();
                tell(&mut commands, owner, Phone::Hidden(false));
            }
            Some(_) => {}
            None => {
                commands.entity(e).remove::<Hiding>();
            }
        }
    }
    for id in left_locker {
        hunted.hiding.remove(&id);
    }
    let prey: Vec<Prey> = players
        .iter()
        .filter(|(_, _, _, w, _, _)| in_station(w.pos))
        .map(|(_, p, _, w, _, _)| Prey {
            id: p.id,
            feet: w.pos,
            hidden: hunted.hiding.get(&p.id).copied(),
        })
        .collect();
    let dt = time.delta_secs().min(0.1);
    for (e, mut hunter, mut brain) in &mut hunters {
        let mut h = *hunter;
        let outcome = think(
            &t,
            station.levels(),
            &spots,
            &mut h,
            &mut brain,
            &prey,
            &noises.0,
            dt,
        );
        if h != *hunter {
            *hunter = h;
        }
        match outcome {
            Outcome::Nothing => {}
            Outcome::Gone => commands.entity(e).despawn(),
            Outcome::Spotted(id) => {
                if let Some((_, _, owner, ..)) = players.iter().find(|p| p.1.id == id) {
                    tell(&mut commands, owner, Phone::Seen);
                }
            }
            Outcome::Caught(id) => {
                hunted.hiding.remove(&id);
                // Andrew starts over from the top.
                hunted.calls.remove(&id);
                if let Some((pe, _, owner, mut w, mut input, mut kcc)) =
                    players.iter_mut().find(|p| p.1.id == id)
                {
                    let yaw = w.yaw;
                    teleport(
                        &mut w,
                        input.as_deref_mut(),
                        kcc.as_deref_mut(),
                        street_at(&t) + Vec3::Y * 0.05,
                        yaw,
                    );
                    commands.entity(pe).remove::<Hiding>();
                    tell(&mut commands, owner, Phone::Caught);
                }
                if id == brain.target {
                    commands.entity(e).despawn();
                }
            }
        }
    }
}

/// Server: climb into the nearest locker (within reach), or back out.
fn hide(
    ev: On<FromClient<Hide>>,
    mut commands: Commands,
    station: Option<Res<Station>>,
    mut hunted: ResMut<Hunted>,
    mut players: Query<(Entity, &Player, &PlayerOwner, &mut Walker)>,
) {
    let Some(station) = station else { return };
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((e, p, o, mut w)) = players.iter_mut().find(|b| b.2.0 == owner) else {
        return;
    };
    let t = tuning();
    if let Some(spot) = hunted.hiding.remove(&p.id) {
        // Out the locker's door, a step towards the cell's middle.
        let out = node_at(&t, station.levels(), spot).map_or(Vec2::ZERO, |n| {
            (node_pos(&t, station.levels(), n) - spot).xz().normalize()
        });
        let yaw = w.yaw;
        teleport(&mut w, None, None, spot + out.extend(0.0) * 1.2, yaw);
        commands.entity(e).remove::<Hiding>();
        tell(&mut commands, o, Phone::Hidden(false));
        return;
    }
    let spots = hiding_spots(&t, &TILES.get(), station.levels());
    let Some(&(n, spot)) = spots
        .iter()
        .filter(|(_, s)| (s.y - w.pos.y).abs() < 1.5)
        .min_by(|a, b| a.1.distance(w.pos).total_cmp(&b.1.distance(w.pos)))
    else {
        return;
    };
    // In reach is measured to the bank's nearest door (what you clicked),
    // not to the middle locker you end up in.
    let cell = node_pos(&t, station.levels(), n);
    let turn = station.levels()[n.0].fill((n.1, n.2)).rotation();
    let in_reach = LOCKER_DOORS_X.iter().any(|&x| {
        let door = cell + turn * Vec3::new(x, 0.0, HIDE_AT.z);
        door.xz().distance(w.pos.xz()) <= crate::features::interact::REACH
    });
    if !in_reach {
        crate::features::interact::deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    let yaw = w.yaw;
    teleport(&mut w, None, None, spot + Vec3::Y * 0.05, yaw);
    hunted.hiding.insert(p.id, spot);
    // The bank is solid now: the controller pins the body here until they
    // climb back out (`Hiding` in `fps`).
    commands.entity(e).insert(Hiding);
    tell(&mut commands, o, Phone::Hidden(true));
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_observer(on_phone)
        .add_observer(click_locker)
        .init_resource::<Call>()
        .add_systems(OnExit(AppState::InGame), |mut call: ResMut<Call>| {
            *call = Call::default()
        })
        .add_systems(Update, place_lockers.run_if(in_state(AppState::InGame)))
        .add_systems(
            Update,
            music_signals
                .in_set(crate::music::MusicSignalSet)
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            build_ui
                .run_if(in_state(AppState::InGame))
                .run_if(not(any_with_component::<PhoneBanner>)),
        )
        .add_systems(
            Update,
            (
                ring,
                dress_hunters,
                hunt_clips,
                hunter_gaze,
                move_hunters,
                hunt_sounds,
            )
                .run_if(in_state(AppState::InGame))
                .run_if(crate::sites::here("nyc")),
        );
}

// ------------------------------------------------------------------ its music

/// What the hunter's music has made of the hunt so far (local to each peer).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Dread {
    /// A hunter is after you: the chase music.
    pub chasing: bool,
    /// Seconds of tension left since the chase ended.
    pub tense: f32,
}

/// One tick of the music's view of the hunt. Pure: the state, whether a hunter
/// is after you (and you're down in the station), the time and how long the
/// tension lasts in; the new state and whether the chase *just* started (the
/// stinger's moment) out. Chase beats tension beats ambience; the tension
/// starts the moment the chase ends and counts down; leaving the station
/// resets it all.
pub fn dread(d: Dread, inside: bool, after_you: bool, dt: f32, tense_secs: f32) -> (Dread, bool) {
    if !inside {
        return (Dread::default(), false);
    }
    if after_you {
        return (
            Dread {
                chasing: true,
                tense: 0.0,
            },
            !d.chasing,
        );
    }
    let tense = if d.chasing {
        tense_secs
    } else {
        (d.tense - dt).max(0.0)
    };
    (
        Dread {
            chasing: false,
            tense,
        },
        false,
    )
}

/// Is `hunter` after someone near `ears`? It is chasing or rolling and close
/// (the server keeps who it's after to itself).
pub fn after_you(hunter: &Hunter, ears: Vec3, range: f32) -> bool {
    matches!(hunter.mode, Mode::Chase | Mode::Roll) && hunter.pos.distance(ears) < range
}

/// Local: the hunter's music. Publishes `fulton_chase` (1 while a hunter is
/// after you), `fulton_tense` (seconds of tension left once it's over) and the
/// cue `fulton_spotted` when a chase starts (data/music/moods.ron plays the
/// stinger and the moods from them).
fn music_signals(
    time: Res<Time>,
    ears: Res<crate::spatial::Ears>,
    hunters: Query<&Hunter>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
    mut state: Local<Dread>,
) {
    let Some(mut signals) = signals else { return };
    let t = tuning();
    let inside = in_station(ears.pos - Vec3::Y * 1.6);
    let chased = hunters
        .iter()
        .any(|h| after_you(h, ears.pos, t.hunt.music_chase_range));
    let (next, spotted) = dread(
        *state,
        inside,
        chased,
        time.delta_secs(),
        t.hunt.music_tense_secs,
    );
    *state = next;
    signals.flag("fulton_chase", next.chasing);
    signals.set("fulton_tense", next.tense);
    if spotted {
        signals.cue("fulton_spotted");
    }
}

/// The call on this screen.
#[derive(Resource, Default)]
struct Call {
    /// Which call, while it's going.
    call: Option<usize>,
    /// Ringing (not answered yet).
    ringing: bool,
    /// The next line, and seconds until it.
    line: usize,
    wait: f32,
    /// Seconds until the next ring tone.
    tone: f32,
}

#[derive(Component)]
struct PhoneBanner;

#[derive(Component)]
struct PhoneSubtitle;

fn build_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(30.0),
        TextColor(Color::srgb(0.55, 1.0, 0.6)),
        PhoneBanner,
        crate::hud::slot(crate::hud::HudSlot::CenterBanner),
        crate::sites::AtSite("nyc"),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        PhoneSubtitle,
        crate::hud::slot(crate::hud::HudSlot::Subtitle),
        crate::sites::AtSite("nyc"),
        DespawnOnExit(AppState::InGame),
    ));
}

fn on_phone(ev: On<Phone>, mut call: ResMut<Call>, toast: Option<ResMut<crate::hud::Toast>>) {
    let w = WRITING.get();
    let line = match *ev {
        Phone::Call(n) => {
            *call = Call {
                call: Some(n as usize),
                ringing: true,
                line: 0,
                wait: tuning().hunt.ring_secs,
                tone: 0.0,
            };
            None
        }
        Phone::Sent => Some(w.sent.clone()),
        Phone::Seen => Some(w.seen.clone()),
        Phone::Caught => Some(w.caught.clone()),
        Phone::Hidden(true) => Some(w.hidden.clone()),
        Phone::Hidden(false) => Some(w.unhidden.clone()),
    };
    if let (Some(line), Some(mut toast)) = (line, toast) {
        toast.show(line);
    }
}

/// A phone's ring: two tones together, on and off.
fn ring_tone(rate: u32) -> Vec<f32> {
    let n = (rate as f32 * 0.9) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let on = if ((t * 10.0) as u32).is_multiple_of(2) {
                1.0
            } else {
                0.4
            };
            let s = (t * 440.0 * std::f32::consts::TAU).sin()
                + (t * 480.0 * std::f32::consts::TAU).sin();
            s * 0.25 * on * (1.0 - t / 0.9).min(1.0)
        })
        .collect()
}

/// Local: ring, answer (E, or it answers itself), then play the call line by line.
#[allow(clippy::too_many_arguments)]
fn ring(
    mut commands: Commands,
    mut ctx: ResMut<crate::input::InputContexts>,
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut call: ResMut<Call>,
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    mut banner: Query<&mut Text, (With<PhoneBanner>, Without<PhoneSubtitle>)>,
    mut subtitle: Query<&mut Text, (With<PhoneSubtitle>, Without<PhoneBanner>)>,
) {
    let Some(n) = call.call else {
        crate::input::switch(&mut ctx, "phone", false);
        return;
    };
    crate::input::switch(&mut ctx, "phone", call.ringing);
    let dt = time.delta_secs();
    let at = camera.iter().next().map_or(Vec3::ZERO, |g| g.translation());
    let w = WRITING.get();
    let t = tuning();
    if call.ringing {
        call.wait -= dt;
        call.tone -= dt;
        if call.tone <= 0.0 {
            call.tone = 2.0;
            crate::spatial::play_wav_at(
                &mut commands,
                at,
                0.8,
                crate::spatial::Carry::NEARBY,
                || crate::features::bear::wav_bytes(&ring_tone(22_050), 22_050),
            );
        }
        set_text(&mut banner, &crate::input::fill(&w.ringing));
        if keys.just_pressed(KeyCode::KeyE) || call.wait <= 0.0 {
            call.ringing = false;
            call.wait = 0.3;
            set_text(&mut banner, "");
        }
        return;
    }
    call.wait -= dt;
    if call.wait > 0.0 {
        return;
    }
    let Some((who, line)) = w.calls.get(n).and_then(|c| c.get(call.line)) else {
        // Hung up.
        set_text(&mut subtitle, "");
        *call = Call::default();
        return;
    };
    set_text(
        &mut subtitle,
        &format!("{who}: {}", speech::split_mood(line).0),
    );
    let secs = if who == "Andrew" {
        // His voice for this call; the line may ask for another mood with a trailing [mood].
        let voice = t.hunt.voice.get(n).cloned().unwrap_or_default();
        let secs = speech::duration(line, &voice);
        let line = line.clone();
        // Made off the main thread: the call carries on when it's over.
        crate::spatial::play_samples_at(
            &mut commands,
            at,
            1.0,
            crate::spatial::Carry::NEARBY,
            move || speech::say(&line, &voice),
        );
        secs
    } else {
        line.split_whitespace().count() as f32 * 0.32
    };
    call.line += 1;
    call.wait = secs + t.hunt.line_gap;
}

fn set_text<F: bevy::ecs::query::QueryFilter>(q: &mut Query<&mut Text, F>, s: &str) {
    for mut text in q.iter_mut() {
        if text.0 != s {
            text.0 = s.to_string();
        }
    }
}

/// A locker you can click to climb into.
#[derive(Component)]
struct Locker;

/// Local: the lockers of each level the station draws (children of its
/// models, so they go with them).
fn place_lockers(
    mut commands: Commands,
    station: Option<Res<Station>>,
    levels: Query<(Entity, &LevelModels), Added<LevelModels>>,
) {
    let Some(station) = station else { return };
    if levels.is_empty() {
        return;
    }
    let t = tuning();
    let w = WRITING.get();
    let label: &'static str = w.hide_label.clone().leak();
    let set = TILES.get();
    let spots = hiding_spots(&t, &set, station.levels());
    for (root, l) in &levels {
        // One clickable per door, so any locker of the bank takes the click
        // (they all climb into the same hiding spot: the middle locker).
        for (n, at) in spots.iter().filter(|(n, _)| n.0 == l.0) {
            let level = &station.levels()[n.0];
            let turn = level.fill((n.1, n.2)).rotation();
            for &x in &LOCKER_DOORS_X {
                commands.spawn((
                    Locker,
                    Transform::from_translation(*at + turn * Vec3::new(x, 0.0, 0.0)),
                    Visibility::default(),
                    Clickable::at_height(1.2).radius(60.0).labelled(label),
                    ChildOf(root),
                ));
            }
        }
    }
}

fn click_locker(ev: On<Clicked>, mut commands: Commands, lockers: Query<(), With<Locker>>) {
    if lockers.contains(ev.entity) {
        commands.client_trigger(Hide);
    }
}

/// Local: draw each hunter (a child in New York's visuals; the hunter is replicated).
/// Only once: a hunter that already has its rig keeps the clip `hunt_clips` picked
/// (the rig's scene child isn't a `Dress`, so without this it was reset to the
/// walk every frame and never ran or rolled).
fn dress_hunters(
    mut commands: Commands,
    hunters: Query<(Entity, Option<&Children>), (With<Hunter>, Without<crate::rigged::Rigged>)>,
    dresses: Query<(), With<crate::area::Dress>>,
) {
    for (e, children) in &hunters {
        if crate::area::dressed(children, &dresses) {
            continue;
        }
        commands
            .entity(e)
            .insert_if_new((Transform::default(), Visibility::default()));
        commands.entity(e).insert((
            crate::rigged::Rigged {
                path: "models/features/fs_hunter.gltf",
            },
            crate::rigged::RigClip::looping("walk"),
            crate::rigged::Overlay::new("fs_hunter"),
        ));
    }
}

/// Local: while it chases, the hunter's head follows you (this peer's camera);
/// otherwise it looks where it is heading.
fn hunter_gaze(
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    mut hunters: Query<(&Hunter, &mut crate::rigged::Overlay)>,
) {
    let eye = camera.single().ok().map(GlobalTransform::translation);
    for (h, mut o) in &mut hunters {
        let want = if h.mode == Mode::Chase { eye } else { None };
        if o.look_at != want {
            o.look_at = want;
        }
    }
}

/// Seconds the grab lunge plays (the clip's length in the model).
const CATCH_SECS: f32 = 1.4;

/// Local: the grab lunge is playing; seconds left before the walk returns.
#[derive(Component)]
struct Catching(f32);

/// Local: pick the clip from what the hunter is doing: a lope while it
/// drifts and searches, a run on the chase, and the grab once when it
/// catches you (it flips from the chase straight to backing off).
fn hunt_clips(
    time: Res<Time>,
    mut commands: Commands,
    mut hunters: Query<(
        Entity,
        &Hunter,
        Option<&Gait>,
        Option<&mut crate::rigged::RigClip>,
        Option<&mut Catching>,
    )>,
) {
    let dt = time.delta_secs();
    for (e, hunter, gait, clip, catching) in &mut hunters {
        if let Some(mut catch) = catching {
            catch.0 -= dt;
            if catch.0 <= 0.0 {
                commands.entity(e).remove::<Catching>();
                if let Some(mut clip) = clip {
                    *clip = crate::rigged::RigClip::looping("walk")
                        .moving(pace_of(gait, mode_speed(&tuning(), hunter.mode)));
                }
            }
            continue;
        }
        let playing = clip.as_ref().map(|c| c.name);
        let grab = hunter.mode == Mode::Retreat && playing == Some("run");
        if grab {
            commands
                .entity(e)
                .insert((Catching(CATCH_SECS), crate::rigged::RigClip::once("catch")));
            continue;
        }
        let want = match hunter.mode {
            Mode::Roll => "roll",
            Mode::Chase => "run",
            _ => "walk",
        };
        // Strides keep up with the ground: play the clip at the pace it moves.
        let next = crate::rigged::RigClip::looping(want)
            .moving(pace_of(gait, mode_speed(&tuning(), hunter.mode)));
        if let Some(mut clip) = clip {
            if *clip != next {
                *clip = next;
            }
        } else {
            commands.entity(e).insert(next);
        }
    }
}

/// Local: how the hunter's body is really moving (the server's `Hunter` is
/// where it is meant to be; this is the body easing after it).
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Gait {
    /// Ground speed now (m/s), eased: the stride clips play to match it.
    pub speed: f32,
    /// Which way the body faces and travels (radians about up).
    pub heading: f32,
}

/// Shortest signed angle (radians) from `a` to `b`.
pub fn angle_diff(a: f32, b: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (b - a + PI).rem_euclid(TAU) - PI
}

/// How far along from walking (0) to chasing (1) a speed is.
fn pace(h: &Hunt, speed: f32) -> f32 {
    let span = (h.chase_speed - h.walk_speed).max(0.01);
    ((speed - h.walk_speed) / span).clamp(0.0, 1.0)
}

/// Turn rate (radians a second) at a speed: `turn_speed` walking, falling to
/// `run_turn_speed` at chase speed.
pub fn turn_rate(h: &Hunt, speed: f32) -> f32 {
    let deg = h.turn_speed + (h.run_turn_speed - h.turn_speed) * pace(h, speed);
    deg.max(1.0).to_radians()
}

/// What fraction of its speed it keeps for a turn of `angle` radians: all of
/// it up to `slow_turn_degrees`, easing to `turn_slow_min` at a full
/// about-turn, and only when it is fast (a walker turns on the spot).
pub fn turn_slowdown(h: &Hunt, speed: f32, angle: f32) -> f32 {
    let from = h.slow_turn_degrees.to_radians();
    let over = ((angle.abs() - from) / (std::f32::consts::PI - from).max(0.01)).clamp(0.0, 1.0);
    let at_pace = 1.0 - (1.0 - h.turn_slow_min.clamp(0.0, 1.0)) * over;
    1.0 - (1.0 - at_pace) * pace(h, speed)
}

/// Ease `speed` toward `target` by at most `accel` / `decel` m/s² over `dt`.
pub fn approach_speed(speed: f32, target: f32, accel: f32, decel: f32, dt: f32) -> f32 {
    if target > speed {
        (speed + accel.max(0.0) * dt).min(target)
    } else {
        (speed - decel.max(0.0) * dt).max(target)
    }
}

/// One frame of the body following the server: `to` is the ground offset
/// (x, z) to where the server has it, `rest_yaw` the way it should face when
/// it gets there, `top` the speed of its mode. Returns the new gait and how
/// far it moves. It moves straight at the server's spot, which walks the
/// station's corridors cell to cell, so it never cuts a corner through a wall;
/// only its facing swings round at a speed-dependent rate (and the bone
/// overlay leans it into the turn).
pub fn steer(h: &Hunt, g: Gait, to: Vec2, rest_yaw: f32, top: f32, dt: f32) -> (Gait, Vec2) {
    let dist = to.length();
    // Ease to a stop on arrival, hurry a little when behind, never much faster
    // than its mode.
    let arrive = (dist / (top * h.follow_secs).max(0.01)).min(1.0);
    let hurry = h.catch_up_gain * (dist - h.catch_up_slack).max(0.0);
    let target = (top * arrive + hurry).min(top * h.max_hurry);
    let speed = approach_speed(g.speed, target, h.accel, h.decel, dt);
    let step = (speed * dt).min(dist);
    let go = if dist > 1e-4 {
        to / dist * step
    } else {
        Vec2::ZERO
    };
    // Face where it's going (or the server's way, standing still).
    let want = if step > 1e-4 && dist > 0.05 {
        to.x.atan2(to.y)
    } else {
        rest_yaw
    };
    let diff = angle_diff(g.heading, want);
    let rate = turn_rate(h, g.speed) * dt;
    let heading = g.heading + diff.clamp(-rate, rate);
    (Gait { speed, heading }, go)
}

/// The stride clips play to the speed it really moves, in 0.25 m/s steps so
/// the clip isn't rewritten every frame.
fn pace_of(gait: Option<&Gait>, fallback: f32) -> f32 {
    gait.map_or(fallback, |g| (g.speed * 4.0).round() / 4.0)
}

/// Local: put the hunter where the server says, with a loping sway: the body
/// speeds up, slows down and swings round at a speed-dependent rate.
fn move_hunters(
    time: Res<Time>,
    mut commands: Commands,
    mut hunters: Query<(Entity, &Hunter, &mut Transform, Option<&mut Gait>)>,
) {
    let dt = time.delta_secs();
    let k = (dt * 12.0).min(1.0);
    let tun = tuning();
    let t = time.elapsed_secs();
    for (e, h, mut tf, gait) in &mut hunters {
        let top = mode_speed(&tun, h.mode);
        let start = Gait {
            speed: 0.0,
            heading: h.yaw,
        };
        let Some(mut gait) = gait else {
            tf.translation = h.pos;
            commands.entity(e).insert(start);
            continue;
        };
        let rolling = h.mode == Mode::Roll;
        let fast = if h.mode == Mode::Chase || rolling {
            2.0
        } else {
            1.0
        };
        let bob = if rolling {
            0.0
        } else {
            (t * 5.0 * fast).sin().abs() * 0.08
        };
        let target = h.pos + Vec3::Y * bob;
        let to = (h.pos - tf.translation).xz();
        if tf.translation.distance(target) > 6.0 {
            tf.translation = target;
            *gait = Gait {
                speed: top,
                heading: h.yaw,
            };
        } else {
            let (next, go) = steer(&tun.hunt, *gait, to, h.yaw, top, dt);
            *gait = next;
            tf.translation.x += go.x;
            tf.translation.z += go.y;
            tf.translation.y += (target.y - tf.translation.y) * k;
        }
        let lean = match h.mode {
            Mode::Roll => 0.0,
            Mode::Chase => 0.25,
            _ => 0.05,
        };
        tf.rotation = Quat::from_rotation_y(gait.heading)
            * Quat::from_rotation_x(lean)
            * Quat::from_rotation_z((t * 2.5 * fast).sin() * 0.06);
    }
}

// ------------------------------------------------------------------ its voice

/// Which sound goes with a change of mind (`from` to `to`), if any: a screech
/// when it spots you, a screech as it winds up into the ball, a screech when it
/// has you, and a deep groan when it gives up the chase. Pure, so it's tested.
pub fn cue(from: Mode, to: Mode) -> Option<Cue> {
    match (from, to) {
        (a, b) if a == b => None,
        (_, Mode::Roll) => Some(Cue::Charge),
        (Mode::Chase | Mode::Roll, Mode::Retreat) => Some(Cue::Catch),
        (Mode::Chase, Mode::Search) => Some(Cue::Lost),
        (Mode::Drift | Mode::Search | Mode::Retreat, Mode::Chase) => Some(Cue::Spot),
        _ => None,
    }
}

/// The next take to play: they go round in order.
pub fn round_robin(played: usize, takes: usize) -> usize {
    played % takes.max(1)
}

/// A one-off sound the hunter makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cue {
    Spot,
    Charge,
    Catch,
    Lost,
}

/// Metres between footfalls: half a stride of the clip it's playing, so a
/// step sounds as each foot lands at any speed. None while it rolls.
pub fn step_gap(hunt: &Hunt, mode: Mode) -> Option<f32> {
    match mode {
        Mode::Roll => None,
        Mode::Chase => Some(hunt.run_step_m),
        _ => Some(hunt.walk_step_m),
    }
}

/// Seconds the charge-up screech runs before the roll's slapping starts.
const WIND_UP: f32 = 0.9;
/// Metres per second it has to be moving for its feet to sound.
const MOVING: f32 = 0.4;

/// Local: what this peer has heard of a hunter so far.
#[derive(Component)]
struct HuntEars {
    mode: Mode,
    last: Vec3,
    /// Metres until the next footfall.
    step: f32,
    /// Footfalls so far (picks the variation).
    n: usize,
    /// Each cue's takes so far, in `Cue` order (round-robin).
    takes: [usize; 4],
    /// Seconds until the roll's slapping starts, while it's winding up.
    wind_up: f32,
    /// The looping slaps, while it rolls.
    slaps: Option<Entity>,
}

/// Local: its sounds (made in the sounder, `assets/audio/sfx/hunter/`). A file
/// that's missing just means silence.
#[derive(Resource)]
struct HuntSounds {
    steps: [Handle<AudioSample>; 3],
    runs: [Handle<AudioSample>; 3],
    spot: [Handle<AudioSample>; 3],
    charge: [Handle<AudioSample>; 3],
    slaps: Handle<AudioSample>,
    catch: [Handle<AudioSample>; 3],
    lost: [Handle<AudioSample>; 3],
}

impl HuntSounds {
    fn load(assets: &AssetServer) -> Self {
        let at = |name: &str| assets.load(format!("audio/sfx/hunter/{name}.wav"));
        Self {
            steps: [1, 2, 3].map(|i| at(&format!("hunter_step_{i}"))),
            runs: [1, 2, 3].map(|i| at(&format!("hunter_run_{i}"))),
            spot: [1, 2, 3].map(|i| at(&format!("hunter_spot_{i}"))),
            charge: [1, 2, 3].map(|i| at(&format!("hunter_charge_{i}"))),
            slaps: at("hunter_roll"),
            catch: [1, 2, 3].map(|i| at(&format!("hunter_catch_{i}"))),
            lost: [1, 2, 3].map(|i| at(&format!("hunter_lost_{i}"))),
        }
    }
}

/// Local: heavy steps, a heavier run, and its screeches, slaps and groan,
/// from what the server says it's doing.
fn hunt_sounds(
    time: Res<Time>,
    assets: Option<Res<AssetServer>>,
    sounds: Option<Res<HuntSounds>>,
    mut commands: Commands,
    mut hunters: Query<(Entity, &Hunter, &Transform, Option<&mut HuntEars>)>,
    mut slap_at: Query<&mut Transform, Without<Hunter>>,
) {
    use crate::spatial::{Carry, loop_at, play_at};
    let Some(sounds) = sounds else {
        if let Some(assets) = assets {
            commands.insert_resource(HuntSounds::load(&assets));
        }
        return;
    };
    let dt = time.delta_secs().max(1e-3);
    for (e, h, tf, ears) in &mut hunters {
        let here = tf.translation;
        let Some(mut ears) = ears else {
            commands.entity(e).insert(HuntEars {
                mode: h.mode,
                last: here,
                step: 0.0,
                n: 0,
                takes: [0; 4],
                wind_up: 0.0,
                slaps: None,
            });
            continue;
        };
        let voice = here + Vec3::Y * 1.6;
        if let Some(cue) = cue(ears.mode, h.mode) {
            let (takes, volume, carry) = match cue {
                Cue::Spot => (&sounds.spot, 0.7, Carry::LOUD),
                Cue::Charge => (&sounds.charge, 0.7, Carry::LOUD),
                Cue::Catch => (&sounds.catch, 0.8, Carry::LOUD),
                Cue::Lost => (&sounds.lost, 0.9, Carry::CALL),
            };
            // Round-robin: each cue goes through its takes in turn.
            let take = &mut ears.takes[cue as usize];
            let clip = &takes[round_robin(*take, takes.len())];
            *take += 1;
            play_at(&mut commands, clip, voice, volume, carry);
            if cue == Cue::Charge {
                ears.wind_up = WIND_UP;
            }
        }
        ears.mode = h.mode;
        // The ball's wet slapping, from the end of the wind-up until it uncurls.
        if h.mode == Mode::Roll {
            ears.wind_up -= dt;
            if ears.wind_up <= 0.0 && ears.slaps.is_none() {
                let slaps = loop_at(&mut commands, &sounds.slaps, here, 0.9, Carry::NEARBY);
                ears.slaps = Some(slaps);
            }
        } else if let Some(slaps) = ears.slaps.take() {
            commands.entity(slaps).despawn();
        }
        if let Some(slaps) = ears.slaps
            && let Ok(mut at) = slap_at.get_mut(slaps)
        {
            at.translation = here + Vec3::Y * 0.6;
        }
        // Feet.
        let moved = here.distance(ears.last);
        let speed = moved / dt;
        ears.last = here;
        let Some(gap) = step_gap(&tuning().hunt, h.mode) else {
            continue;
        };
        if speed < MOVING {
            ears.step = 0.0;
            continue;
        }
        ears.step -= moved;
        if ears.step > 0.0 {
            continue;
        }
        ears.step = gap;
        let set = if h.mode == Mode::Chase {
            &sounds.runs
        } else {
            &sounds.steps
        };
        let clip = &set[round_robin(ears.n, set.len())];
        ears.n += 1;
        play_at(&mut commands, clip, here, 1.0, Carry::NEARBY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load<T: Send + Sync + 'static>(d: &friendslop_data::Data<T>) {
        let path = format!("{}/../game/assets/{}", env!("CARGO_MANIFEST_DIR"), d.path());
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(d.set_bytes(&bytes), "{path} doesn't parse");
    }

    fn data() -> (Tuning, TileSet, Vec<Level>) {
        load(&super::super::fulton::TUNING);
        load(&TILES);
        load(&WRITING);
        let t = (*tuning()).clone();
        let set = (*TILES.get()).clone();
        let levels = super::super::fulton::generate(&t, &set);
        (t, set, levels)
    }

    #[test]
    fn the_music_follows_the_hunt() {
        let tense = 25.0;
        let step = |d: Dread, inside, chased, dt| dread(d, inside, chased, dt, tense);
        let calm = Dread::default();
        // Spotted: the chase starts, once, with the stinger.
        let (d, spotted) = step(calm, true, true, 0.1);
        assert!(d.chasing && spotted);
        let (d, spotted) = step(d, true, true, 0.1);
        assert!(d.chasing && !spotted, "the stinger fires once per chase");
        // It loses you: tension, counting down...
        let (d, spotted) = step(d, true, false, 0.1);
        assert!(!d.chasing && !spotted);
        assert_eq!(d.tense, tense);
        let (d, _) = step(d, true, false, 10.0);
        assert_eq!(d.tense, 15.0);
        // ...spotted again during it: a new stinger, no tension.
        let (again, spotted) = step(d, true, true, 0.1);
        assert!(again.chasing && spotted && again.tense == 0.0);
        // ...or all the way back to the ambience.
        let (d, _) = step(d, true, false, 20.0);
        assert_eq!(d, calm);
        // Leaving the station ends all of it.
        let (d, spotted) = step(again, false, true, 0.1);
        assert_eq!((d, spotted), (calm, false));
        // Which hunters count: chasing or rolling, and near.
        let at = |mode, x| Hunter {
            pos: Vec3::new(x, 0.0, 0.0),
            yaw: 0.0,
            mode,
        };
        let me = Vec3::ZERO;
        assert!(after_you(&at(Mode::Chase, 10.0), me, 45.0));
        assert!(after_you(&at(Mode::Roll, 10.0), me, 45.0));
        assert!(!after_you(&at(Mode::Chase, 60.0), me, 45.0));
        for mode in [Mode::Drift, Mode::Search, Mode::Retreat] {
            assert!(!after_you(&at(mode, 10.0), me, 45.0), "{mode:?}");
        }
    }

    #[test]
    fn a_runner_turns_slower_and_slows_for_tight_turns() {
        let (t, _, _) = data();
        let h = &t.hunt;
        assert!(h.run_turn_speed > 0.0 && h.run_turn_speed < h.turn_speed);
        assert!(h.accel > 0.0 && h.decel > 0.0 && h.follow_secs > 0.0);
        assert!((turn_rate(h, h.walk_speed).to_degrees() - h.turn_speed).abs() < 0.01);
        assert!((turn_rate(h, h.chase_speed).to_degrees() - h.run_turn_speed).abs() < 0.01);
        assert!(turn_rate(h, h.chase_speed * 2.0) <= turn_rate(h, h.walk_speed));
        let wide = std::f32::consts::PI;
        assert_eq!(turn_slowdown(h, h.chase_speed, 0.1), 1.0);
        assert_eq!(turn_slowdown(h, h.walk_speed, wide), 1.0);
        let tight = turn_slowdown(h, h.chase_speed, wide);
        assert!((tight - h.turn_slow_min).abs() < 0.01);
        let mid = turn_slowdown(h, h.chase_speed, 2.0);
        assert!(mid < 1.0 && mid > tight);
    }

    #[test]
    fn speed_eases_toward_the_target() {
        assert_eq!(approach_speed(0.0, 6.0, 10.0, 20.0, 0.1), 1.0);
        assert_eq!(approach_speed(5.5, 6.0, 10.0, 20.0, 0.1), 6.0);
        assert_eq!(approach_speed(6.0, 0.0, 10.0, 20.0, 0.1), 4.0);
        assert_eq!(approach_speed(1.0, 0.0, 10.0, 20.0, 0.1), 0.0);
    }

    #[test]
    fn the_body_keeps_to_the_servers_line_and_its_speed() {
        let (t, _, _) = data();
        let h = &t.hunt;
        let dt = 1.0 / 60.0;
        // Running along +z, the server's spot turns a corner to the right (+x):
        // the body must go straight at it (never swing wide into the wall)
        // and never run far faster than its mode.
        let mut g = Gait {
            speed: h.chase_speed,
            heading: 0.0,
        };
        let mut pos = Vec2::ZERO;
        let goal = Vec2::new(4.0, 0.0);
        for _ in 0..600 {
            let (n, go) = steer(h, g, goal - pos, 0.0, h.chase_speed, dt);
            g = n;
            pos += go;
            assert!(pos.y.abs() < 1e-3, "it left the line: {pos}");
            assert!(g.speed <= h.chase_speed * h.max_hurry + 1e-3);
        }
        assert!(pos.distance(goal) < 0.05, "it got there: {pos}");
    }

    #[test]
    fn it_leaves_you_be_while_you_hide_and_after_it_gives_up() {
        let (t, set, levels) = data();
        let h = &t.hunt;
        // A locker in the open station (a few sit in small cut-off pockets).
        let Some(&(you, _)) = hiding_spots(&t, &set, &levels)
            .iter()
            .find(|s| around(&levels, s.0, 1000).len() > 50)
        else {
            panic!("the station has lockers");
        };
        let me = arrival(&levels, you);
        let mut brain = Brain::new(1, 7);
        for _ in 0..50 {
            // Out in the open: the director sends it near you.
            let n = drift_to(&t, &levels, me, you, false, &mut brain);
            assert!(!around_dist(&levels, n, you, h.director_radius + 1));
            // Hidden, or while it's calm: somewhere well away from you.
            let n = drift_to(&t, &levels, me, you, true, &mut brain);
            assert!(around_dist(&levels, n, you, h.calm_cells), "{n:?}");
        }
        brain.calm = 1.0;
        let n = drift_to(&t, &levels, me, you, false, &mut brain);
        assert!(around_dist(&levels, n, you, h.calm_cells));
    }

    #[test]
    fn a_hunter_reaching_its_spot_stops_without_overshooting() {
        let (t, _, _) = data();
        let h = &t.hunt;
        let mut g = Gait {
            speed: 0.0,
            heading: 0.0,
        };
        let mut pos = Vec2::ZERO;
        let goal = Vec2::new(0.0, 3.0);
        for _ in 0..600 {
            let (n, go) = steer(h, g, goal - pos, 0.0, h.walk_speed, 1.0 / 60.0);
            g = n;
            pos += go;
            assert!(pos.y <= 3.0 + 1e-3, "overshot to {pos}");
        }
        assert!(pos.distance(goal) < 0.05 && g.speed < 0.1);
    }

    #[test]
    fn three_calls_angrier_each_time_and_lockers_to_hide_in() {
        let (t, set, levels) = data();
        let w = WRITING.get();
        assert_eq!(w.calls.len(), 3);
        assert_eq!(t.hunt.call_levels.len(), 3);
        assert_eq!(t.hunt.voice.len(), 3);
        assert!(t.hunt.call_levels.windows(2).all(|p| p[0] < p[1]));
        assert!(
            t.hunt.call_levels[2] < t.levels,
            "the last call comes before the J"
        );
        assert!(
            t.hunt.voice.windows(2).all(|v| v[1].pitch > v[0].pitch),
            "angrier, higher"
        );
        assert!(
            w.calls
                .iter()
                .all(|c| c.iter().any(|(who, _)| who == "Andrew"))
        );
        assert!(!w.ringing.is_empty() && !w.caught.is_empty() && !w.hide_label.is_empty());
        assert!(
            t.hunt.chase_speed > 5.0 && t.hunt.chase_speed < 10.5,
            "outrun it running"
        );
        // Plenty of lockers to hide in on the way down.
        let spots = hiding_spots(&t, &set, &levels);
        assert!(spots.len() >= levels.len() / 2, "{}", spots.len());
    }

    #[test]
    fn andrew_calls_once_per_depth() {
        let (t, ..) = data();
        let first = t.hunt.call_levels[0];
        assert_eq!(call_due(&t, 0, 0), None);
        assert_eq!(call_due(&t, 0, first), Some(0));
        assert_eq!(call_due(&t, 1, first), None);
        assert_eq!(call_due(&t, 3, t.levels), None);
    }

    #[test]
    fn his_voice_speaks() {
        let (t, ..) = data();
        let (slow, rate) = speech::say("Where are you?", &t.hunt.voice[0]);
        let (fast, _) = speech::say("Where are you?", &t.hunt.voice[2]);
        assert!(rate > 8000 && slow.len() > rate as usize / 4);
        assert!(fast.len() < slow.len(), "angrier is faster");
        let said = speech::duration("Where are you?", &t.hunt.voice[0]);
        assert!((said - slow.len() as f32 / rate as f32).abs() < 0.01);
        let (calm, _) = speech::say("Where are you? [calm]", &t.hunt.voice[0]);
        assert_ne!(calm, slow, "a line can ask for its own mood");
    }

    #[test]
    fn the_station_graph_goes_down_the_stairs() {
        let (t, _, levels) = data();
        let top = (0, levels[0].arrive.0, levels[0].arrive.1);
        let low = (3, levels[3].goal.0, levels[3].goal.1);
        let Some(way) = route(&levels, top, low) else {
            panic!("no way down");
        };
        assert_eq!(way.last(), Some(&low));
        // ...and every step is a real step.
        let mut at = top;
        for n in way {
            assert!(neighbours(&levels, at).contains(&n));
            at = n;
        }
        assert_eq!(node_at(&t, &levels, node_pos(&t, &levels, low)), Some(low));
    }

    fn hunter_at(t: &Tuning, levels: &[Level], n: Node) -> Hunter {
        Hunter {
            pos: node_pos(t, levels, n),
            yaw: 0.0,
            mode: Mode::Drift,
        }
    }

    /// Two cells on one level with a straight open corridor between them.
    fn corridor(levels: &[Level]) -> Option<(Node, Node)> {
        for (li, l) in levels.iter().enumerate() {
            for z in 0..l.depth {
                for x in 0..l.width {
                    let a = (li, x, z);
                    for s in wfc::Side::ALL {
                        if !l.open((x, z), s) {
                            continue;
                        }
                        let Some(b) = wfc::neighbour(l.width, l.depth, (x, z), s) else {
                            continue;
                        };
                        return Some((a, (li, b.0, b.1)));
                    }
                }
            }
        }
        None
    }

    #[test]
    fn held_in_sight_it_rolls_then_backs_off_the_roll_for_a_while() {
        let (t, set, levels) = data();
        let spots = hiding_spots(&t, &set, &levels);
        let Some((a, b)) = corridor(&levels) else {
            panic!("no corridor")
        };
        let mut h = hunter_at(&t, &levels, a);
        let start = node_pos(&t, &levels, a);
        let d = node_pos(&t, &levels, b) - start;
        let yaw = d.x.atan2(d.z);
        h.yaw = yaw;
        let you = Prey {
            id: 7,
            feet: node_pos(&t, &levels, b),
            hidden: None,
        };
        let mut brain = Brain::new(7, 1);
        let h_t = t.hunt.clone();

        // a first tick starts the chase; after `roll_sight_secs` of clear sight
        // it stops walking and rolls
        let mut rolled_at = None;
        for i in 0..400 {
            let out = think(&t, &levels, &spots, &mut h, &mut brain, &[you], &[], 0.05);
            if out == Outcome::Caught(7) {
                break;
            }
            h.pos = start;
            h.yaw = yaw;
            if h.mode == Mode::Roll {
                rolled_at = Some(i);
                break;
            }
        }
        let rolled_at = rolled_at.expect("it should roll when it keeps seeing us");
        let ticks = rolled_at as f32 * 0.05;
        assert!(
            (ticks - h_t.roll_sight_secs).abs() < 0.25,
            "rolled after {ticks}s, wanted about {}",
            h_t.roll_sight_secs
        );
        assert!(brain.rolling > h_t.roll_secs - 0.1, "a full roll is queued");
        assert!(brain.roll_cd > 0.0, "the roll goes on cooldown");

        // it comes out of the roll into a run/sweep, not straight back into one
        let mut left_roll_after = 0.0;
        for _ in 0..(h_t.roll_secs / 0.05).ceil() as usize + 2 {
            think(&t, &levels, &spots, &mut h, &mut brain, &[you], &[], 0.05);
            h.pos = start;
            left_roll_after += 0.05;
            if h.mode != Mode::Roll {
                break;
            }
        }
        assert_ne!(h.mode, Mode::Roll, "the roll has to end");
        assert!(
            (left_roll_after - h_t.roll_secs).abs() < 0.2,
            "rolled for {left_roll_after}s, wanted {}",
            h_t.roll_secs
        );

        // and it can't roll again until the cooldown has run down
        assert!(brain.roll_cd > h_t.roll_cooldown_secs - h_t.roll_secs - 0.5);
        for _ in 0..10 {
            think(&t, &levels, &spots, &mut h, &mut brain, &[you], &[], 0.05);
            h.pos = start;
            assert_ne!(h.mode, Mode::Roll, "on cooldown");
        }
    }

    #[test]
    fn it_chases_what_it_sees_and_catches_it() {
        let (t, set, levels) = data();
        let spots = hiding_spots(&t, &set, &levels);
        let Some((a, b)) = corridor(&levels) else {
            panic!("no corridor")
        };
        let mut h = hunter_at(&t, &levels, a);
        let d = node_pos(&t, &levels, b) - node_pos(&t, &levels, a);
        h.yaw = d.x.atan2(d.z);
        let you = Prey {
            id: 7,
            feet: node_pos(&t, &levels, b),
            hidden: None,
        };
        let mut brain = Brain::new(7, 1);
        let first = think(&t, &levels, &spots, &mut h, &mut brain, &[you], &[], 0.05);
        assert_eq!(first, Outcome::Spotted(7));
        assert_eq!(h.mode, Mode::Chase);
        let mut caught = false;
        for _ in 0..200 {
            if think(&t, &levels, &spots, &mut h, &mut brain, &[you], &[], 0.05)
                == Outcome::Caught(7)
            {
                caught = true;
                break;
            }
        }
        assert!(caught);
    }

    #[test]
    fn it_cannot_see_you_hiding_or_through_walls_or_behind_it() {
        let (t, _, levels) = data();
        let Some((a, b)) = corridor(&levels) else {
            panic!("no corridor")
        };
        let (pa, pb) = (node_pos(&t, &levels, a), node_pos(&t, &levels, b));
        let d = pb - pa;
        let ahead = d.x.atan2(d.z);
        let eye = pa;
        assert!(sees(&t, &levels, eye, ahead, pb));
        assert!(!sees(&t, &levels, eye, ahead + std::f32::consts::PI, pb));
        // Another level: never.
        assert!(!sees(&t, &levels, eye, ahead, pb - Vec3::Y * 4.0));
        // A wall: find a closed side between neighbours.
        let l = &levels[a.0];
        'walls: for z in 0..l.depth {
            for x in 0..l.width {
                for s in wfc::Side::ALL {
                    if let Some(n) = wfc::neighbour(l.width, l.depth, (x, z), s)
                        && !l.open((x, z), s)
                    {
                        let from = node_pos(&t, &levels, (a.0, x, z));
                        let to = node_pos(&t, &levels, (a.0, n.0, n.1));
                        let dd = to - from;
                        assert!(!sees(&t, &levels, from, dd.x.atan2(dd.z), to));
                        break 'walls;
                    }
                }
            }
        }
        // Hidden in a locker in plain view: it walks past.
        let spots = Vec::new();
        let mut h = hunter_at(&t, &levels, a);
        h.yaw = ahead;
        let mut brain = Brain::new(7, 1);
        let you = Prey {
            id: 7,
            feet: pb,
            hidden: Some(pb),
        };
        assert_ne!(
            think(&t, &levels, &spots, &mut h, &mut brain, &[you], &[], 0.05),
            Outcome::Spotted(7)
        );
        assert_ne!(h.mode, Mode::Chase);
    }

    #[test]
    fn while_you_hide_the_director_keeps_it_off_you() {
        let (t, set, levels) = data();
        let spots = hiding_spots(&t, &set, &levels);
        let li = 10;
        let you_node = (li, levels[li].goal.0, levels[li].goal.1);
        let start = arrival(&levels, you_node);
        assert_eq!(start.0, li);
        // You're hiding, so it has nothing to go on.
        let you = Prey {
            id: 3,
            feet: node_pos(&t, &levels, you_node),
            hidden: Some(node_pos(&t, &levels, you_node)),
        };
        let mut h = hunter_at(&t, &levels, start);
        let mut brain = Brain::new(3, 9);
        let far = route(&levels, start, you_node).map_or(0, |r| r.len());
        let mut closest = far;
        for _ in 0..(60.0 / 0.05) as usize {
            let out = think(&t, &levels, &spots, &mut h, &mut brain, &[you], &[], 0.05);
            assert_ne!(out, Outcome::Gone);
            if let Some(n) = node_at(&t, &levels, h.pos) {
                closest = closest.min(route(&levels, n, you_node).map_or(far, |r| r.len()));
            }
        }
        // A locker is a way out: with nothing to go on it never gets sent onto you.
        assert!(
            closest > t.hunt.director_radius as usize,
            "{closest} of {far}"
        );
    }

    #[test]
    fn losing_sight_it_keeps_chasing_for_a_while_then_searches() {
        let (t, set, levels) = data();
        let spots = hiding_spots(&t, &set, &levels);
        let Some((a, b)) = corridor(&levels) else {
            panic!("no corridor")
        };
        let mut h = hunter_at(&t, &levels, a);
        let mut brain = Brain::new(7, 1);
        let you = Prey {
            id: 7,
            feet: node_pos(&t, &levels, b),
            hidden: None,
        };
        // It saw you once, then you slipped out of its cone.
        brain.last_seen = Some(b);
        brain.timer = t.hunt.lose_secs;
        h.mode = Mode::Chase;
        let start = node_pos(&t, &levels, a);
        let mut kept = Vec::new();
        for _ in 0..20 {
            let out = think(&t, &levels, &spots, &mut h, &mut brain, &[you], &[], 0.05);
            // Getting you within the grace is the chase working.
            if out == Outcome::Caught(7) {
                return;
            }
            assert_eq!(h.mode, Mode::Chase, "still chasing while blind");
            kept.push(h.pos);
        }
        assert!(
            kept.iter().any(|p| p.xz().distance(start.xz()) > 0.1),
            "it keeps moving while blind"
        );
        assert!(
            kept.iter()
                .any(|p| p.xz().distance(node_pos(&t, &levels, b).xz())
                    < p.xz().distance(kept[0].xz())),
            "it closes in on you"
        );
        // After the grace runs out it stops hunting and starts searching.
        for _ in 0..(t.hunt.lose_secs / 0.05) as usize {
            think(&t, &levels, &spots, &mut h, &mut brain, &[you], &[], 0.05);
        }
        assert_eq!(h.mode, Mode::Search);
    }

    #[test]
    fn hiding_blinds_the_chase_to_you() {
        let (t, set, levels) = data();
        let spots = hiding_spots(&t, &set, &levels);
        let Some((a, b)) = corridor(&levels) else {
            panic!("no corridor")
        };
        // It chased you to `a`, you slipped into a locker at `b`; it only saw
        // you at `a`, so it must not keep walking on to `b`.
        let mut h = hunter_at(&t, &levels, a);
        let mut brain = Brain::new(7, 1);
        brain.last_seen = Some(a);
        brain.timer = t.hunt.lose_secs;
        h.mode = Mode::Chase;
        let locker = node_pos(&t, &levels, b);
        let you = Prey {
            id: 7,
            feet: locker,
            hidden: Some(locker),
        };
        for _ in 0..(t.hunt.lose_secs / 0.05) as usize {
            think(&t, &levels, &spots, &mut h, &mut brain, &[you], &[], 0.05);
        }
        assert_ne!(
            node_at(&t, &levels, h.pos),
            Some(b),
            "it walked up to the locker it never saw you enter"
        );
        assert_eq!(
            h.mode,
            Mode::Search,
            "and then it searches where it lost you"
        );
    }

    #[test]
    fn a_run_nearby_wakes_it_even_blind() {
        let (t, set, levels) = data();
        let spots = hiding_spots(&t, &set, &levels);
        let Some((a, b)) = corridor(&levels) else {
            panic!("no corridor")
        };
        let mut h = hunter_at(&t, &levels, a);
        // Facing away, so only its ears can find you.
        let d = node_pos(&t, &levels, b) - node_pos(&t, &levels, a);
        h.yaw = d.x.atan2(d.z) + std::f32::consts::PI;
        let mut brain = Brain::new(7, 1);
        let you = Prey {
            id: 7,
            feet: node_pos(&t, &levels, b),
            hidden: None,
        };
        // You run four cells away, out of its cone: it hears you coming.
        let far = route(&levels, a, b).map_or(4, |r| r.len().min(4));
        let over_there = route(&levels, a, b)
            .and_then(|r| r.get(far - 1).copied())
            .unwrap_or(b);
        let steps = Noise {
            at: node_pos(&t, &levels, over_there),
            level: Loud::Run,
            radius: 12.0,
        };
        let out = think(
            &t,
            &levels,
            &spots,
            &mut h,
            &mut brain,
            &[you],
            &[steps],
            0.05,
        );
        assert_eq!(out, Outcome::Nothing, "hearing is silent");
        assert_eq!(h.mode, Mode::Chase);
        assert_eq!(brain.last_seen, Some(over_there));
        // Its route heads for the sound, and the grace started fresh.
        assert!(
            brain.route.last() == Some(&over_there) || brain.route.is_empty(),
            "headed for where the run came from"
        );
        assert!(
            (brain.timer - (t.hunt.lose_secs - 0.05)).abs() < 1e-3,
            "the grace started fresh (this tick already spent 0.05)"
        );
    }

    #[test]
    fn it_leaves_when_you_do() {
        let (t, set, levels) = data();
        let spots = hiding_spots(&t, &set, &levels);
        let mut h = hunter_at(&t, &levels, (5, 0, 0));
        let mut brain = Brain::new(1, 1);
        let gone = Prey {
            id: 1,
            feet: street_at(&t),
            hidden: None,
        };
        assert_eq!(
            think(&t, &levels, &spots, &mut h, &mut brain, &[gone], &[], 0.05),
            Outcome::Gone
        );
    }

    #[test]
    fn it_screeches_on_spotting_and_winding_up_and_groans_on_losing_you() {
        assert_eq!(cue(Mode::Drift, Mode::Chase), Some(Cue::Spot));
        assert_eq!(cue(Mode::Search, Mode::Chase), Some(Cue::Spot));
        assert_eq!(cue(Mode::Chase, Mode::Roll), Some(Cue::Charge));
        assert_eq!(cue(Mode::Chase, Mode::Retreat), Some(Cue::Catch));
        assert_eq!(cue(Mode::Roll, Mode::Retreat), Some(Cue::Catch));
        assert_eq!(cue(Mode::Chase, Mode::Search), Some(Cue::Lost));
        assert_eq!(cue(Mode::Roll, Mode::Chase), None, "uncurling is quiet");
        assert_eq!(cue(Mode::Chase, Mode::Chase), None);
    }

    #[test]
    fn its_takes_go_round_in_order() {
        let order: Vec<usize> = (0..7).map(|n| round_robin(n, 3)).collect();
        assert_eq!(order, [0, 1, 2, 0, 1, 2, 0]);
        assert_eq!(round_robin(5, 0), 0, "no takes is still safe");
    }

    #[test]
    fn it_steps_once_a_half_stride_and_makes_no_steps_rolling() {
        let hunt = Hunt {
            walk_step_m: 1.16,
            run_step_m: 1.99,
            ..Default::default()
        };
        assert_eq!(step_gap(&hunt, Mode::Drift), Some(1.16));
        assert_eq!(step_gap(&hunt, Mode::Chase), Some(1.99));
        assert!(step_gap(&hunt, Mode::Roll).is_none());
    }
}
