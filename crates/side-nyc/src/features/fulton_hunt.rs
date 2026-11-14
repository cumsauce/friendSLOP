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
//! view cone with line of sight down the corridors. Seen, it chases you; lose
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

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_wfc as wfc;
use serde::{Deserialize, Serialize};

use super::fulton::{
    CELL, HIDE_AT, Kind, Level, Station, TILES, TileSet, Tuning, cell_world, level_at, street_at,
    tuning,
};
use crate::AppState;
use crate::features::fps::{WalkInput, Walker, teleport};
use crate::features::interact::{Clickable, Clicked};
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
    if (x, z) == l.goal && l.fill(l.goal).kind == Kind::Stairs && li + 1 < levels.len() {
        out.push((li + 1, x, z));
    }
    if li > 0 && (x, z) == l.arrive && l.fill(l.arrive).kind == Kind::Landing {
        out.push((li - 1, x, z));
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
    let li = level_at(p.y);
    let l = levels.get(li)?;
    let origin = cell_world(t, li, (0, 0));
    let x = ((p.x - origin.x) / CELL).round();
    let z = ((p.z - origin.z) / CELL).round();
    (x >= 0.0 && z >= 0.0 && (x as usize) < l.width && (z as usize) < l.depth)
        .then_some((li, x as usize, z as usize))
}

/// The floor in the middle of a node.
pub fn node_pos(t: &Tuning, n: Node) -> Vec3 {
    cell_world(t, n.0, (n.1, n.2))
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
                    let at = cell_world(t, li, (x, z)) + f.rotation() * HIDE_AT;
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
    if level_at(from.y) != level_at(to.y) {
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
    for p in prey.iter().filter(|p| p.hidden.is_none()) {
        if level_at(p.feet.y) == level_at(hunter.pos.y)
            && p.feet.xz().distance(hunter.pos.xz()) < h.catch_range
        {
            hunter.mode = Mode::Retreat;
            brain.route.clear();
            brain.timer = 0.0;
            return Outcome::Caught(p.id);
        }
    }
    // Its eyes.
    let seen = prey
        .iter()
        .filter(|p| p.hidden.is_none())
        .find(|p| sees(t, levels, hunter.pos, hunter.yaw, p.feet));
    if let Some(p) = seen {
        let at = node_at(t, levels, p.feet);
        if hunter.mode != Mode::Chase {
            out = Outcome::Spotted(p.id);
        }
        if hunter.mode != Mode::Chase || brain.last_seen != at {
            brain.route = at.and_then(|a| route(levels, me, a)).unwrap_or_default();
        }
        hunter.mode = Mode::Chase;
        brain.last_seen = at;
        brain.timer = h.lose_secs;
    }
    brain.timer -= dt;
    match hunter.mode {
        Mode::Chase if seen.is_none() => {
            if brain.route.is_empty() || brain.timer <= 0.0 {
                hunter.mode = Mode::Search;
                brain.timer = h.search_secs;
                brain.searched.clear();
            }
        }
        Mode::Chase => {}
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
                // Back off, away from where the director says you are.
                hunter.mode = Mode::Retreat;
                brain.timer = h.retreat_secs;
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
                // The director: roughly where you are, never exactly.
                let near = around(levels, target_node, h.director_radius);
                let pick = near[brain.rng.below(near.len())].0;
                brain.route = route(levels, me, pick).unwrap_or_default();
                brain.timer = h.director_secs;
            }
        }
    }
    walk(t, hunter, brain, dt);
    // In the same cell as whoever it's chasing: straight at them.
    if let Some(p) = seen
        && brain.route.is_empty()
    {
        let d = (p.feet - hunter.pos).xz();
        if d.length() > 0.01 {
            let step = d.normalize() * (h.chase_speed * dt).min(d.length());
            hunter.pos += Vec3::new(step.x, 0.0, step.y);
            hunter.yaw = d.x.atan2(d.y);
        }
    }
    out
}

/// Is `n` at least `cells` steps from `from` (or out of reach)?
fn around_dist(levels: &[Level], n: Node, from: Node, cells: u32) -> bool {
    !around(levels, from, cells.saturating_sub(1))
        .iter()
        .any(|(m, _)| *m == n)
}

/// Move along the route.
fn walk(t: &Tuning, hunter: &mut Hunter, brain: &mut Brain, dt: f32) {
    let speed = if hunter.mode == Mode::Chase {
        t.hunt.chase_speed
    } else {
        t.hunt.walk_speed
    };
    let mut left = speed * dt;
    while left > 0.0 {
        let Some(&next) = brain.route.first() else {
            break;
        };
        let to = node_pos(t, next);
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
                .run_if(in_state(ServerState::Running))
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
        let Some(you) = node_at(&t, &station.levels, w.pos) else {
            continue;
        };
        let at = node_pos(&t, arrival(&station.levels, you));
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
    mut hunted: ResMut<Hunted>,
    mut hunters: Query<(Entity, &mut Hunter, &mut Brain)>,
    mut players: Query<(
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
    let spots = hiding_spots(&t, &set, &station.levels);
    // Walk out of a locker and you're out.
    let mut left_locker = Vec::new();
    for (p, owner, w, _, _) in &players {
        if let Some(spot) = hunted.hiding.get(&p.id)
            && w.pos.xz().distance(spot.xz()) > 0.9
        {
            left_locker.push(p.id);
            tell(&mut commands, owner, Phone::Hidden(false));
        }
    }
    for id in left_locker {
        hunted.hiding.remove(&id);
    }
    let prey: Vec<Prey> = players
        .iter()
        .filter(|(_, _, w, _, _)| in_station(w.pos))
        .map(|(p, _, w, _, _)| Prey {
            id: p.id,
            feet: w.pos,
            hidden: hunted.hiding.get(&p.id).copied(),
        })
        .collect();
    let dt = time.delta_secs().min(0.1);
    for (e, mut hunter, mut brain) in &mut hunters {
        let mut h = *hunter;
        let outcome = think(&t, &station.levels, &spots, &mut h, &mut brain, &prey, dt);
        if h != *hunter {
            *hunter = h;
        }
        match outcome {
            Outcome::Nothing => {}
            Outcome::Gone => commands.entity(e).despawn(),
            Outcome::Spotted(id) => {
                if let Some((_, owner, ..)) = players.iter().find(|p| p.0.id == id) {
                    tell(&mut commands, owner, Phone::Seen);
                }
            }
            Outcome::Caught(id) => {
                hunted.hiding.remove(&id);
                // Andrew starts over from the top.
                hunted.calls.remove(&id);
                if let Some((_, owner, mut w, mut input, mut kcc)) =
                    players.iter_mut().find(|p| p.0.id == id)
                {
                    let yaw = w.yaw;
                    teleport(
                        &mut w,
                        input.as_deref_mut(),
                        kcc.as_deref_mut(),
                        street_at(&t) + Vec3::Y * 0.05,
                        yaw,
                    );
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
    mut players: Query<(&Player, &PlayerOwner, &mut Walker)>,
) {
    let Some(station) = station else { return };
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((p, o, mut w)) = players.iter_mut().find(|b| b.1.0 == owner) else {
        return;
    };
    let t = tuning();
    if let Some(spot) = hunted.hiding.remove(&p.id) {
        // Out the locker's door, a step forward.
        let out = (w.pos - spot).xz();
        let yaw = w.yaw;
        teleport(
            &mut w,
            None,
            None,
            spot + Vec3::new(out.x, 0.0, out.y).normalize_or(Vec3::Z) * 1.2,
            yaw,
        );
        tell(&mut commands, o, Phone::Hidden(false));
        return;
    }
    let spots = hiding_spots(&t, &TILES.get(), &station.levels);
    let Some(&(_, spot)) = spots
        .iter()
        .filter(|(_, s)| (s.y - w.pos.y).abs() < 1.5)
        .min_by(|a, b| a.1.distance(w.pos).total_cmp(&b.1.distance(w.pos)))
    else {
        return;
    };
    if spot.xz().distance(w.pos.xz()) > 3.5 {
        crate::features::interact::deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    let yaw = w.yaw;
    teleport(&mut w, None, None, spot + Vec3::Y * 0.05, yaw);
    hunted.hiding.insert(p.id, spot);
    tell(&mut commands, o, Phone::Hidden(true));
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_observer(on_phone)
        .add_observer(dress_hunter)
        .add_observer(click_locker)
        .init_resource::<Call>()
        .add_systems(OnEnter(AppState::InGame), (build_ui, place_lockers))
        .add_systems(
            Update,
            (ring, move_hunters).run_if(in_state(AppState::InGame)),
        );
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

fn build_ui(mut commands: Commands, mut call: ResMut<Call>) {
    *call = Call::default();
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(30.0),
        TextColor(Color::srgb(0.55, 1.0, 0.6)),
        PhoneBanner,
        crate::hud::slot(crate::hud::HudSlot::CenterBanner),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        PhoneSubtitle,
        crate::hud::slot(crate::hud::HudSlot::Subtitle),
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

/// Andrew's voice: the retro synth at his pitch and speed for this call.
pub fn andrew_says(text: &str, (pitch, speed): (f32, f32)) -> (Vec<f32>, u32) {
    let mut voice = samiam::Voice::new();
    voice.pitch = pitch;
    voice.speed = speed;
    (voice.speak(text), voice.sample_rate)
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
    set_text(&mut subtitle, &format!("{who}: {line}"));
    let secs = if who == "Andrew" {
        let voice = t.hunt.voice.get(n).copied().unwrap_or((95.0, 1.0));
        let (samples, rate) = andrew_says(line, voice);
        let secs = samples.len() as f32 / rate.max(1) as f32;
        crate::spatial::play_wav_at(
            &mut commands,
            at,
            1.0,
            crate::spatial::Carry::NEARBY,
            move || crate::features::bear::wav_bytes(&samples, rate),
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

fn place_lockers(mut commands: Commands) {
    let t = tuning();
    let w = WRITING.get();
    // The station's plan, the same one `fulton` builds.
    let levels = super::fulton::generate(&t, &TILES.get());
    let label: &'static str = w.hide_label.clone().leak();
    for (_, at) in hiding_spots(&t, &TILES.get(), &levels) {
        commands.spawn((
            Locker,
            Transform::from_translation(at),
            Visibility::default(),
            Clickable::at_height(1.2).radius(60.0).labelled(label),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

fn click_locker(ev: On<Clicked>, mut commands: Commands, lockers: Query<(), With<Locker>>) {
    if lockers.contains(ev.entity) {
        commands.client_trigger(Hide);
    }
}

fn dress_hunter(add: On<Add<Hunter>>, mut commands: Commands, assets: Res<AssetServer>) {
    commands.entity(add.entity).insert((
        Transform::default(),
        Visibility::default(),
        children![(
            WorldAssetRoot(
                assets.load(GltfAssetLabel::Scene(0).from_asset("models/features/fs_hunter.gltf")),
            ),
            Transform::default(),
        )],
    ));
}

/// Local: put the hunter where the server says, smoothly, with a loping sway.
fn move_hunters(time: Res<Time>, mut hunters: Query<(&Hunter, &mut Transform)>) {
    let k = (time.delta_secs() * 12.0).min(1.0);
    let t = time.elapsed_secs();
    for (h, mut tf) in &mut hunters {
        let fast = if h.mode == Mode::Chase { 2.0 } else { 1.0 };
        let target = h.pos + Vec3::Y * ((t * 5.0 * fast).sin().abs() * 0.08);
        if tf.translation.distance(target) > 6.0 {
            tf.translation = target;
        } else {
            tf.translation = tf.translation.lerp(target, k);
        }
        let lean = if h.mode == Mode::Chase { 0.25 } else { 0.05 };
        let want = Quat::from_rotation_y(h.yaw)
            * Quat::from_rotation_x(lean)
            * Quat::from_rotation_z((t * 2.5 * fast).sin() * 0.06);
        tf.rotation = tf.rotation.slerp(want, k);
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
            t.hunt.voice.windows(2).all(|v| v[1].0 > v[0].0),
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
        let (slow, rate) = andrew_says("Where are you?", (92.0, 1.0));
        let (fast, _) = andrew_says("Where are you?", (150.0, 1.35));
        assert!(rate > 8000 && slow.len() > rate as usize / 4);
        assert!(fast.len() < slow.len(), "angrier is faster");
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
        assert_eq!(node_at(&t, &levels, node_pos(&t, low)), Some(low));
    }

    fn hunter_at(t: &Tuning, n: Node) -> Hunter {
        Hunter {
            pos: node_pos(t, n),
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
    fn it_chases_what_it_sees_and_catches_it() {
        let (t, set, levels) = data();
        let spots = hiding_spots(&t, &set, &levels);
        let Some((a, b)) = corridor(&levels) else {
            panic!("no corridor")
        };
        let mut h = hunter_at(&t, a);
        let d = node_pos(&t, b) - node_pos(&t, a);
        h.yaw = d.x.atan2(d.z);
        let you = Prey {
            id: 7,
            feet: node_pos(&t, b),
            hidden: None,
        };
        let mut brain = Brain::new(7, 1);
        let first = think(&t, &levels, &spots, &mut h, &mut brain, &[you], 0.05);
        assert_eq!(first, Outcome::Spotted(7));
        assert_eq!(h.mode, Mode::Chase);
        let mut caught = false;
        for _ in 0..200 {
            if think(&t, &levels, &spots, &mut h, &mut brain, &[you], 0.05) == Outcome::Caught(7) {
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
        let (pa, pb) = (node_pos(&t, a), node_pos(&t, b));
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
                        let from = node_pos(&t, (a.0, x, z));
                        let to = node_pos(&t, (a.0, n.0, n.1));
                        let dd = to - from;
                        assert!(!sees(&t, &levels, from, dd.x.atan2(dd.z), to));
                        break 'walls;
                    }
                }
            }
        }
        // Hidden in a locker in plain view: it walks past.
        let spots = Vec::new();
        let mut h = hunter_at(&t, a);
        h.yaw = ahead;
        let mut brain = Brain::new(7, 1);
        let you = Prey {
            id: 7,
            feet: pb,
            hidden: Some(pb),
        };
        assert_ne!(
            think(&t, &levels, &spots, &mut h, &mut brain, &[you], 0.05),
            Outcome::Spotted(7)
        );
        assert_ne!(h.mode, Mode::Chase);
    }

    #[test]
    fn the_director_brings_it_near_you_but_not_onto_you() {
        let (t, set, levels) = data();
        let spots = hiding_spots(&t, &set, &levels);
        let li = 10;
        let you_node = (li, levels[li].goal.0, levels[li].goal.1);
        let start = arrival(&levels, you_node);
        assert_eq!(start.0, li);
        // You're hiding, so it only has the director to go on.
        let you = Prey {
            id: 3,
            feet: node_pos(&t, you_node),
            hidden: Some(node_pos(&t, you_node)),
        };
        let mut h = hunter_at(&t, start);
        let mut brain = Brain::new(3, 9);
        let far = route(&levels, start, you_node).map_or(0, |r| r.len());
        let mut closest = far;
        for _ in 0..(60.0 / 0.05) as usize {
            let out = think(&t, &levels, &spots, &mut h, &mut brain, &[you], 0.05);
            assert_ne!(out, Outcome::Gone);
            if let Some(n) = node_at(&t, &levels, h.pos) {
                closest = closest.min(route(&levels, n, you_node).map_or(far, |r| r.len()));
            }
        }
        assert!(
            closest <= t.hunt.director_radius as usize,
            "{closest} of {far}"
        );
    }

    #[test]
    fn it_leaves_when_you_do() {
        let (t, set, levels) = data();
        let spots = hiding_spots(&t, &set, &levels);
        let mut h = hunter_at(&t, (5, 0, 0));
        let mut brain = Brain::new(1, 1);
        let gone = Prey {
            id: 1,
            feet: street_at(&t),
            hidden: None,
        };
        assert_eq!(
            think(&t, &levels, &spots, &mut h, &mut brain, &[gone], 0.05),
            Outcome::Gone
        );
    }
}
