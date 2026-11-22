//! The arcade's FPS machine: a deathmatch against bots, on a level of its own.
//!
//! Click the red FRAG cabinet in Neverland (an `Arcade` marker `fps` on the
//! theatre map) and the host teleports your body into the arena, a small neon
//! map of its own (the modeler document `fps-arena`, exported to
//! `maps/arcade_fps.bsn`, hung far under the world like the theatre). There
//! you walk with the normal controls, with an AK-47 in your hands (the rigged
//! `fps-ak47` model: its `idle` / `fire` / `reload` / `draw` clips, plus
//! bob, sway and recoil in code) and fight bots to a kill limit.
//!
//! The match is **solo and local**, like Skee-Ball and the Kori claw: the
//! bots, your health, ammo and the score live on your own client, and
//! nothing about them is replicated. What the server does is only what the
//! host must: it moves your body in and out ([`ArcadeFps`]: `Enter` at the
//! cabinet, `Respawn(n)` at a spawn point, `Exit` back to where you stood),
//! checking you are in reach of the cabinet or in the arena first.
//!
//! - **Bots** are rigged cowboys that patrol a waypoint graph (the map's
//!   `dm_wp_*` spots, joined where one can see the next), spot the nearest
//!   enemy (you or another bot) in their view cone, close in or hold, strafe
//!   and shoot with a reaction delay and a spread that grows with distance,
//!   chase to where they lost you, die in a tumble and come back at the
//!   spawn farthest from everyone. Everyone is against everyone: it is a
//!   deathmatch, so bots shoot each other too.
//! - **Shooting** is hitscan: a ray (spread, recoil) tested against bodies
//!   as stacked spheres (legs, chest, head: [`ray_body`]) and against the
//!   level's colliders for walls.
//! - **Rules** ([`outcome`]): first to the kill limit, or most kills when the
//!   clock runs out. Pickups (the map's `dm_health` / `dm_ammo` markers) heal
//!   and refill, then come back after a while.
//!
//! The numbers (gun, bots, match, view) are in `data/side/arcade_fps.ron`.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::{Deserialize, Serialize};

use super::michael::{self, theater_at};
use crate::AppState;
use crate::features::fps::{FpsView, HandsBusy, WalkInput, Walker, teleport};
use crate::features::interact::{Clickable, Clicked, SERVER_REACH, deny, within};
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// The arena's own area (`areas.ron`).
pub const AREA: &str = "arcade_fps";

/// Eye height over the feet (m).
const EYE: f32 = 1.6;
const BOT_MODEL: &str = "models/characters/cowboy_male.gltf";
const BOT_MODEL_ALT: &str = "models/characters/cowboy_female.gltf";
const GUN_MODEL: &str = "models/prefabs/fps-ak47.gltf";

// ------------------------------------------------------------------ tuning

/// `data/side/arcade_fps.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Where the arena's map origin hangs in the world.
    pub arena_at: (f32, f32, f32),
    /// Half size (x, z) of the arena and how high it goes (m): what counts as inside.
    pub arena_half: (f32, f32),
    pub arena_height: f32,
    pub match_rules: Rules,
    pub gun: Gun,
    pub bots: Bots,
    pub view: View,
}

/// The deathmatch's rules.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Rules {
    /// First to this many kills wins.
    pub kill_limit: u32,
    /// Seconds before the most kills wins.
    pub time_limit: f32,
    pub player_health: f32,
    /// Seconds dead before you are back at a spawn point.
    pub respawn_secs: f32,
    /// Seconds before a taken pickup returns.
    pub pickup_respawn_secs: f32,
    /// How close (m) takes a pickup.
    pub pickup_radius: f32,
}

/// The rifle.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Gun {
    pub damage: f32,
    /// Damage multiplier for a head shot.
    pub head_mult: f32,
    pub rounds_per_min: f32,
    pub mag: u32,
    pub reserve: u32,
    pub reload_secs: f32,
    /// Spread (degrees) standing still, and added at full run speed.
    pub spread_deg: f32,
    pub spread_move_deg: f32,
    /// Degrees of pitch each shot kicks the view up, and how fast it settles (per second).
    pub kick_deg: f32,
    pub kick_settle: f32,
    pub range: f32,
    /// When (s into the reload) the mag drops out, the new one goes in and the bolt is charged: the
    /// `fps-ak47` reload clip's events, where the sounds play.
    pub reload_cues: (f32, f32, f32),
}

/// The bots.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Bots {
    pub count: u32,
    pub health: f32,
    pub walk_speed: f32,
    pub run_speed: f32,
    /// How far (m) and how wide (degrees, full cone) a bot sees.
    pub sight: f32,
    pub fov_deg: f32,
    /// Seconds from spotting someone to the first shot.
    pub reaction_secs: f32,
    /// Spread (degrees) per 10 m of range.
    pub spread_deg_per_10m: f32,
    pub damage: f32,
    /// Seconds between a bot's shots.
    pub fire_secs: f32,
    /// The distance (m) a bot likes to fight at, and how long (s) it remembers where it lost you.
    pub preferred_range: f32,
    pub memory_secs: f32,
    pub respawn_secs: f32,
    /// Seconds a bot's death tumble takes.
    pub death_secs: f32,
}

/// The view and viewmodel.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct View {
    /// The gun's grip in the model (m) and where it sits against the camera.
    pub grip: (f32, f32, f32),
    pub hand: (f32, f32, f32),
    pub scale: f32,
    /// Walk bob: metres and cycles per metre walked; sway from looking about (m per radian).
    pub bob: f32,
    pub bob_rate: f32,
    pub sway: f32,
    /// A wall this close (m) in front lowers the gun.
    pub wall_lower: f32,
    /// Seconds of the draw clip and the fire clip (the model's clip lengths).
    pub draw_secs: f32,
    pub fire_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/arcade_fps.ron");

/// The arena's map data (its spots and markers).
pub static MAP: friendslop_data::Data<friendslop_world::places::MapFile> =
    friendslop_data::Data::json("maps/arcade_fps.map.json");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Where the arena's map origin hangs in the world.
pub fn arena_at() -> Vec3 {
    let t = tuning().arena_at;
    Vec3::new(t.0, t.1, t.2)
}

/// Is a body at `pos` inside the arena?
pub fn in_arena(pos: Vec3) -> bool {
    let t = tuning();
    let d = pos - arena_at();
    d.x.abs() <= t.arena_half.0 + 2.0
        && d.z.abs() <= t.arena_half.1 + 2.0
        && (-5.0..=t.arena_height).contains(&d.y)
}

// -------------------------------------------------------------- the level

/// Everything the arena's markers say.
#[derive(Clone, Debug, Default)]
pub struct Level {
    /// Spawn points (world) and the way they face.
    pub spawns: Vec<(Vec3, f32)>,
    /// Bot waypoints (world).
    pub waypoints: Vec<Vec3>,
    pub pickups: Vec<PickupSpawn>,
}

/// A pickup on the map.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PickupSpawn {
    pub at: Vec3,
    pub kind: PickupKind,
    pub amount: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupKind {
    Health,
    Ammo,
}

/// Yaw that faces from `from` toward `to` (0 faces -Z, like the walkers).
pub fn yaw_toward(from: Vec3, to: Vec3) -> f32 {
    let d = to - from;
    (-d.x).atan2(-d.z)
}

/// Read the level from the map file.
pub fn level() -> Level {
    let map = MAP.get();
    let origin = arena_at();
    let mut out = Level::default();
    let mut spawn_ids: Vec<(u32, Vec3)> = Vec::new();
    let mut wp_ids: Vec<(u32, Vec3)> = Vec::new();
    for e in map.entities.iter() {
        let at = origin + Vec3::from_array(e.translation);
        let name = e.text("Spot", "name");
        if let Some(n) = name.strip_prefix("dm_spawn_") {
            spawn_ids.push((n.parse().unwrap_or(0), at));
        } else if let Some(n) = name.strip_prefix("dm_wp_") {
            wp_ids.push((n.parse().unwrap_or(0), at));
        } else if e.has("Arcade") {
            let kind = match e.text("Arcade", "game").as_str() {
                "dm_health" => PickupKind::Health,
                "dm_ammo" => PickupKind::Ammo,
                _ => continue,
            };
            out.pickups.push(PickupSpawn {
                at,
                kind,
                amount: e.num("Arcade", "points"),
            });
        }
    }
    spawn_ids.sort_by_key(|s| s.0);
    wp_ids.sort_by_key(|s| s.0);
    out.waypoints = wp_ids.into_iter().map(|s| s.1).collect();
    // Spawns face the middle of the arena.
    out.spawns = spawn_ids
        .into_iter()
        .map(|(_, p)| (p, yaw_toward(p, origin)))
        .collect();
    out
}

/// The FRAG cabinets on the theatre map (world position, id).
pub fn cabinets() -> Vec<(u32, Vec3)> {
    let map = michael::MAP.get();
    let origin = theater_at();
    map.entities
        .iter()
        .filter(|e| e.has("Arcade") && e.text("Arcade", "game") == "fps")
        .map(|e| {
            (
                e.num("Arcade", "id") as u32,
                origin + Vec3::from_array(e.translation),
            )
        })
        .collect()
}

// ------------------------------------------------------------ pure combat

/// A hit on a body: how far along the ray, and whether it was the head.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyHit {
    pub dist: f32,
    pub head: bool,
}

/// Where a ray from `origin` along unit `dir` first meets a body standing at
/// `feet`: legs, chest and head as spheres. Pure.
pub fn ray_body(origin: Vec3, dir: Vec3, feet: Vec3) -> Option<BodyHit> {
    // (centre height, radius, is head)
    const SPHERES: [(f32, f32, bool); 3] =
        [(0.5, 0.42, false), (1.2, 0.4, false), (1.68, 0.2, true)];
    SPHERES
        .iter()
        .filter_map(|&(h, r, head)| {
            let to = feet + Vec3::Y * h - origin;
            let along = to.dot(dir);
            if along <= 0.0 {
                return None;
            }
            let miss2 = (to - dir * along).length_squared();
            (miss2 < r * r).then(|| BodyHit {
                dist: along - (r * r - miss2).sqrt(),
                head,
            })
        })
        .min_by(|a, b| a.dist.total_cmp(&b.dist))
}

/// `dir` bent by up to `spread` radians in a random direction (`rnd`: two
/// numbers in 0..1). Pure.
pub fn spread_dir(dir: Vec3, spread: f32, rnd: (f32, f32)) -> Vec3 {
    let up = if dir.y.abs() > 0.99 { Vec3::X } else { Vec3::Y };
    let right = dir.cross(up).normalize_or_zero();
    let up = right.cross(dir).normalize_or_zero();
    let a = rnd.0 * std::f32::consts::TAU;
    let r = rnd.1.sqrt() * spread.tan();
    (dir + right * (a.cos() * r) + up * (a.sin() * r)).normalize_or_zero()
}

/// A rifle's spread (radians) at `speed` m/s of walking, `run` the full-run speed. Pure.
pub fn gun_spread(g: &Gun, speed: f32, run: f32) -> f32 {
    (g.spread_deg + g.spread_move_deg * (speed / run.max(0.1)).clamp(0.0, 1.0)).to_radians()
}

/// Damage of a shot: the base (more for a head shot). Pure.
pub fn shot_damage(base: f32, head_mult: f32, head: bool) -> f32 {
    if head { base * head_mult } else { base }
}

/// Who a shot hits first: the nearest of `targets` (id, feet) along the ray
/// that is not behind a wall at `wall` metres. Pure.
pub fn first_hit(
    origin: Vec3,
    dir: Vec3,
    targets: &[(u32, Vec3)],
    wall: Option<f32>,
    range: f32,
) -> Option<(u32, BodyHit)> {
    targets
        .iter()
        .filter_map(|&(id, feet)| ray_body(origin, dir, feet).map(|h| (id, h)))
        .filter(|(_, h)| h.dist <= range && wall.is_none_or(|w| h.dist < w))
        .min_by(|a, b| a.1.dist.total_cmp(&b.1.dist))
}

// ---------------------------------------------------------------- scoring

/// One fighter's tally.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Score {
    pub kills: u32,
    pub deaths: u32,
}

/// How a match stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Running,
    /// This fighter reached the kill limit.
    Won(usize),
    /// The clock ran out; this fighter had the most kills.
    TimeUp(usize),
    /// The clock ran out with the lead shared.
    Tie,
}

/// The rules over the scores (fighter 0 is you). Pure.
pub fn outcome(scores: &[Score], elapsed: f32, rules: &Rules) -> Outcome {
    if let Some(i) = scores.iter().position(|s| s.kills >= rules.kill_limit) {
        return Outcome::Won(i);
    }
    if elapsed < rules.time_limit {
        return Outcome::Running;
    }
    let best = scores.iter().map(|s| s.kills).max().unwrap_or(0);
    let leaders: Vec<usize> = scores
        .iter()
        .enumerate()
        .filter(|(_, s)| s.kills == best)
        .map(|(i, _)| i)
        .collect();
    match leaders.as_slice() {
        [one] => Outcome::TimeUp(*one),
        _ => Outcome::Tie,
    }
}

// ------------------------------------------------------------ bot thinking

/// What a bot is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    /// Walking the waypoint graph.
    Patrol,
    /// Has an enemy in view (the id) and for how long.
    Engage { target: u32, seen_for: f32 },
    /// Lost them: heading to where they were.
    Hunt { at: Vec3, left: f32 },
}

/// What a bot can see this tick: the nearest enemy it has line of sight to.
#[derive(Clone, Copy, Debug)]
pub struct Sighting {
    pub target: u32,
    pub at: Vec3,
    pub dist: f32,
}

/// Does a bot at `eye` facing `yaw` have `at` inside its view cone and sight range? Pure.
pub fn in_view(eye: Vec3, yaw: f32, at: Vec3, sight: f32, fov_deg: f32) -> bool {
    let d = at - eye;
    let flat = Vec2::new(d.x, d.z);
    if flat.length() > sight || flat.length() < 0.01 {
        return flat.length() < 0.01;
    }
    let facing = Vec2::new(-yaw.sin(), -yaw.cos());
    facing.dot(flat.normalize()) >= (fov_deg.to_radians() * 0.5).cos()
}

/// The next mode of a bot given what it sees, after `dt` seconds. Pure.
pub fn next_mode(mode: Mode, seen: Option<Sighting>, dt: f32, memory: f32) -> Mode {
    match (mode, seen) {
        (Mode::Engage { target, seen_for }, Some(s)) if s.target == target => Mode::Engage {
            target,
            seen_for: seen_for + dt,
        },
        (_, Some(s)) => Mode::Engage {
            target: s.target,
            seen_for: 0.0,
        },
        (Mode::Engage { .. }, None) => Mode::Hunt {
            at: Vec3::ZERO,
            left: memory,
        },
        (Mode::Hunt { at, left }, None) => {
            if left - dt <= 0.0 {
                Mode::Patrol
            } else {
                Mode::Hunt {
                    at,
                    left: left - dt,
                }
            }
        }
        (Mode::Patrol, None) => Mode::Patrol,
    }
}

/// May a bot that has had `seen_for` seconds of an enemy shoot, `cool` seconds
/// since its last shot? Pure.
pub fn may_fire(seen_for: f32, reaction: f32, cool: f32, interval: f32) -> bool {
    seen_for >= reaction && cool >= interval
}

/// Which way to strafe (-1, 0 or 1) at `time`, switching every few seconds
/// and differently per bot. Pure.
pub fn strafe_sign(time: f32, bot: u32) -> f32 {
    let phase = (time * 0.7 + bot as f32 * 1.9).sin();
    if phase.abs() < 0.2 {
        0.0
    } else {
        phase.signum()
    }
}

/// Waypoints joined where one sees the next (`clear(a, b)`), for pathing.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    pub points: Vec<Vec3>,
    pub edges: Vec<Vec<usize>>,
}

impl Graph {
    /// Join every pair closer than `reach` metres that `clear` says is open.
    pub fn build(points: Vec<Vec3>, reach: f32, clear: impl Fn(Vec3, Vec3) -> bool) -> Graph {
        let mut edges = vec![Vec::new(); points.len()];
        for i in 0..points.len() {
            for j in (i + 1)..points.len() {
                let climb = (points[i].y - points[j].y).abs();
                let run = points[i].xz().distance(points[j].xz()).max(0.1);
                // A step, or a slope the walkers can climb (about 38 degrees).
                let walkable = climb <= 0.5 || climb / run <= 0.8;
                if points[i].distance(points[j]) <= reach && walkable && clear(points[i], points[j])
                {
                    edges[i].push(j);
                    edges[j].push(i);
                }
            }
        }
        Graph { points, edges }
    }

    /// The waypoint closest to `p`.
    pub fn nearest(&self, p: Vec3) -> Option<usize> {
        (0..self.points.len()).min_by(|&a, &b| {
            self.points[a]
                .distance_squared(p)
                .total_cmp(&self.points[b].distance_squared(p))
        })
    }

    /// The shortest route of waypoint indices from `a` to `b` (A*), both included.
    pub fn path(&self, a: usize, b: usize) -> Option<Vec<usize>> {
        use std::cmp::Ordering;
        use std::collections::BinaryHeap;
        #[derive(PartialEq)]
        struct Node(f32, usize);
        impl Eq for Node {}
        impl Ord for Node {
            fn cmp(&self, o: &Self) -> Ordering {
                o.0.total_cmp(&self.0)
            }
        }
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        let n = self.points.len();
        if a >= n || b >= n {
            return None;
        }
        let mut best = vec![f32::INFINITY; n];
        let mut from = vec![usize::MAX; n];
        let mut open = BinaryHeap::new();
        best[a] = 0.0;
        open.push(Node(self.points[a].distance(self.points[b]), a));
        while let Some(Node(_, i)) = open.pop() {
            if i == b {
                let mut route = vec![b];
                let mut at = b;
                while at != a {
                    at = from[at];
                    route.push(at);
                }
                route.reverse();
                return Some(route);
            }
            for &j in &self.edges[i] {
                let g = best[i] + self.points[i].distance(self.points[j]);
                if g < best[j] {
                    best[j] = g;
                    from[j] = i;
                    open.push(Node(g + self.points[j].distance(self.points[b]), j));
                }
            }
        }
        None
    }
}

// ------------------------------------------------------------------ the match

/// A pickup on the floor (or waiting to come back).
struct Pickup {
    spawn: PickupSpawn,
    /// Seconds until it is back, 0 when it is there.
    cooldown: f32,
    entity: Option<Entity>,
}

/// Your tally and state in the match.
struct Me {
    health: f32,
    mag: u32,
    reserve: u32,
    /// Seconds left of a reload, if reloading, and how many of its sounds have played.
    reload: Option<f32>,
    cue: u8,
    /// Seconds until the next shot may leave.
    cool: f32,
    /// Seconds until you are back, while dead.
    dead: Option<f32>,
    /// Red flash after a hit (0..1) and the view's recoil kick (radians).
    flash: f32,
    kick: f32,
    /// A hit marker (0..1) after you land a shot.
    marker: f32,
}

/// The match you are playing. Local.
#[derive(Resource, Default)]
pub struct Match(Option<Running>);

struct Running {
    scores: Vec<Score>,
    elapsed: f32,
    me: Me,
    graph: Graph,
    level: Level,
    pickups: Vec<Pickup>,
    outcome: Outcome,
    rng: u32,
    /// Seconds the draw animation has left, so the first shot waits for it.
    draw: f32,
    /// The kill feed's last line.
    feed: String,
}

impl Running {
    fn rnd(&mut self) -> f32 {
        // xorshift32 mapped to 0..1
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// The spawn point farthest from every position in `others`. Pure.
pub fn farthest_spawn(spawns: &[(Vec3, f32)], others: &[Vec3]) -> usize {
    (0..spawns.len())
        .max_by(|&a, &b| {
            let score = |i: usize| {
                others
                    .iter()
                    .map(|o| spawns[i].0.distance_squared(*o))
                    .fold(f32::INFINITY, f32::min)
            };
            score(a).total_cmp(&score(b))
        })
        .unwrap_or(0)
}

// ------------------------------------------------------------------ protocol

/// Client -> server: move my body for the arcade FPS.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum ArcadeFps {
    /// Walk into the machine at a FRAG cabinet within reach.
    Enter,
    /// I died: put me at spawn point `n`.
    Respawn(u8),
    /// Back out to where I was standing at the cabinet.
    Exit,
}

/// Server: where each player stood when they went in (to put them back).
#[derive(Resource, Default, Debug)]
struct Returns(std::collections::HashMap<Option<Entity>, (Vec3, f32)>);

pub fn protocol(app: &mut App) {
    app.add_client_event::<ArcadeFps>(Channel::Ordered)
        .init_resource::<Returns>()
        .add_observer(on_request);
}

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    }
}

/// Server: move the sender's body in, between spawns, or back out.
fn on_request(
    ev: On<FromClient<ArcadeFps>>,
    mut commands: Commands,
    mut returns: ResMut<Returns>,
    mut bodies: Query<(
        &PlayerOwner,
        &mut Walker,
        Option<&mut WalkInput>,
        Option<&mut crate::kcc::Kcc>,
    )>,
    mut cover: MessageWriter<friendslop_base::areas::Cover>,
) {
    let owner = owner_of(ev.client_id);
    let Some((_, mut walker, input, kcc)) = bodies.iter_mut().find(|b| b.0.0 == owner) else {
        return;
    };
    let level = level();
    let req: ArcadeFps = **ev;
    match req {
        ArcadeFps::Enter => {
            let Some(&(_, cabinet)) = cabinets().iter().min_by(|a, b| {
                a.1.xz()
                    .distance(walker.pos.xz())
                    .total_cmp(&b.1.xz().distance(walker.pos.xz()))
            }) else {
                return;
            };
            if !within(walker.pos, cabinet, SERVER_REACH) || in_arena(walker.pos) {
                deny(&mut commands, ev.client_id, "too far away: walk closer");
                return;
            }
            let Some(&(to, yaw)) = level.spawns.first() else {
                return;
            };
            returns.0.insert(owner, (walker.pos, walker.yaw));
            cover.write(friendslop_base::areas::Cover);
            teleport(
                &mut walker,
                input.map(|i| i.into_inner()),
                kcc.map(|k| k.into_inner()),
                to,
                yaw,
            );
        }
        ArcadeFps::Respawn(n) => {
            if !in_arena(walker.pos) {
                return;
            }
            let Some(&(to, yaw)) = level.spawns.get(n as usize % level.spawns.len().max(1)) else {
                return;
            };
            teleport(
                &mut walker,
                input.map(|i| i.into_inner()),
                kcc.map(|k| k.into_inner()),
                to,
                yaw,
            );
        }
        ArcadeFps::Exit => {
            if !in_arena(walker.pos) {
                return;
            }
            let (to, yaw) = returns.0.remove(&owner).unwrap_or_else(|| {
                let at = cabinets().first().map_or(theater_at(), |c| c.1);
                (at + Vec3::new(0.0, -0.8, -1.0), 0.0)
            });
            cover.write(friendslop_base::areas::Cover);
            teleport(
                &mut walker,
                input.map(|i| i.into_inner()),
                kcc.map(|k| k.into_inner()),
                to,
                yaw,
            );
        }
    }
}

// -------------------------------------------------------------- the area

/// The arena's colliders (its `Block` markers).
fn build_colliders(mut commands: Commands) {
    use avian3d::prelude::{Collider, RigidBody};
    let origin = arena_at();
    for e in MAP.get().entities.iter().filter(|e| e.has("Block")) {
        let size = e.size.map_or(Vec3::ONE, Vec3::from_array);
        commands.spawn((
            Transform::from_translation(origin + Vec3::from_array(e.translation))
                .with_rotation(Quat::from_rotation_y(e.yaw.to_radians())),
            RigidBody::Static,
            Collider::cuboid(size.x, size.y, size.z),
            friendslop_base::areas::sim(AREA),
        ));
    }
}

/// The arena's looks: its map, on flat ground.
fn build_arena(mut commands: Commands, assets: Res<AssetServer>) {
    // SALT: not needing `Cargo.lock` for this `.bsn` either; x didn't set us
    // up for success, we need more SME's, we haven't considered the problem
    // space enough, BSN syntax needs more changes, and `Children` sugaring?
    // Ask Sam "Salt" Saltman.
    commands.spawn((
        friendslop_world::map::Map(assets.load("maps/arcade_fps.bsn")),
        friendslop_world::map::FlatGround,
        Transform::from_translation(arena_at()),
        Visibility::default(),
        friendslop_base::areas::show(AREA),
    ));
}

/// The FRAG cabinet's click spot.
#[derive(Component)]
struct Cabinet;

fn spawn_cabinets(mut commands: Commands) {
    for (_, at) in cabinets() {
        commands.spawn((
            Cabinet,
            Transform::from_translation(at),
            Visibility::default(),
            Clickable::at_height(0.0).labelled("play FRAG").radius(48.0),
            friendslop_base::areas::sim(michael::AREA),
        ));
    }
}

fn click(
    c: On<Clicked>,
    mut commands: Commands,
    cabinets: Query<(), With<Cabinet>>,
    m: Res<Match>,
) {
    if cabinets.get(c.entity).is_ok() && m.0.is_none() {
        commands.client_trigger(ArcadeFps::Enter);
    }
}

// ------------------------------------------------------------ bots (ECS)

/// A bot's body: fighter id 1.. (you are 0).
#[derive(Component)]
struct Bot {
    id: u32,
    health: f32,
    mode: Mode,
    /// Seconds since its last shot.
    cool: f32,
    /// Waypoint route being followed (indices) and the next index into it.
    route: Vec<usize>,
    step: usize,
    yaw: f32,
    /// Seconds into its death tumble, if dead, and then until it is back.
    dying: Option<f32>,
    respawn: f32,
    /// Position last tick (for the gait).
    last: Vec3,
}

/// A pickup's visual.
#[derive(Component)]
struct PickupVisual;

/// Everything a match spawned (bots, pickups, the viewmodel).
#[derive(Component)]
struct MatchProp;

/// The held rifle.
#[derive(Component)]
struct Viewmodel {
    /// Seconds into the walk bob (distance walked, really).
    walked: f32,
    /// Seconds the current one-shot clip has left (back to idle after).
    clip_left: f32,
    /// Yaw/pitch last frame, for sway.
    look: Vec2,
}

/// The red hit flash and hit marker on the screen.
#[derive(Component)]
struct Flash;

/// The HUD line and the scoreboard.
#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct ScoreText;

#[derive(Component)]
struct BannerText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        StatusText,
        crate::hud::slot(crate::hud::HudSlot::BottomLeft),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        ScoreText,
        crate::hud::slot(crate::hud::HudSlot::Scoreboard),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        BannerText,
        crate::hud::slot(crate::hud::HudSlot::CenterBanner),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Flash,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::srgba(0.8, 0.0, 0.0, 0.0)),
        DespawnOnExit(AppState::InGame),
    ));
}

fn bot_model(id: u32) -> &'static str {
    if id.is_multiple_of(2) {
        BOT_MODEL_ALT
    } else {
        BOT_MODEL
    }
}

/// The physics world for line-of-sight and walls. Absent in headless tests,
/// where nothing is in the way.
type Sight<'w, 's> = Option<avian3d::prelude::SpatialQuery<'w, 's>>;

/// Is the line from `a` to `b` open (no collider between)?
fn clear_line(sight: &Sight, a: Vec3, b: Vec3) -> bool {
    wall_dist(sight, a, b - a).is_none_or(|d| d >= (b - a).length() - 0.1)
}

/// How far (m) a ray from `origin` along `dir` goes before a collider.
fn wall_dist(sight: &Sight, origin: Vec3, dir: Vec3) -> Option<f32> {
    let spatial = sight.as_ref()?;
    let d = Dir3::new(dir).ok()?;
    spatial
        .cast_ray(
            origin,
            d,
            200.0,
            true,
            &avian3d::prelude::SpatialQueryFilter::default(),
        )
        .map(|h| h.distance)
}

/// The floor height under `p` (m), or `fallback`.
fn floor_y(sight: &Sight, p: Vec3, fallback: f32) -> f32 {
    wall_dist(sight, p + Vec3::Y * 1.0, Vec3::NEG_Y).map_or(fallback, |d| p.y + 1.0 - d)
}

#[allow(clippy::too_many_arguments)]
fn lifecycle(
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    bodies: Query<(&Player, &Walker)>,
    mut m: ResMut<Match>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    time: Res<Time>,
    assets: Option<Res<AssetServer>>,
    props: Query<Entity, With<MatchProp>>,
    sight: Sight,
    mut view: ResMut<FpsView>,
    mut busy: ResMut<HandsBusy>,
    camera: Query<Entity, With<crate::world::MainCamera>>,
) {
    let Some(me) = me else { return };
    let Some((_, walker)) = bodies.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let inside = in_arena(walker.pos);
    match (inside, m.0.is_some()) {
        (true, false) => {
            let t = tuning();
            let level = level();
            let graph = Graph::build(level.waypoints.clone(), 14.0, |a, b| {
                clear_line(&sight, a + Vec3::Y * 0.6, b + Vec3::Y * 0.6)
            });
            let n = t.bots.count as usize;
            let mut scores = vec![Score::default(); n + 1];
            scores.truncate(n + 1);
            let seed = (time.elapsed_secs() * 1000.0) as u32 | 1;
            let mut run = Running {
                scores,
                elapsed: 0.0,
                me: Me {
                    health: t.match_rules.player_health,
                    mag: t.gun.mag,
                    reserve: t.gun.reserve,
                    reload: None,
                    cue: 0,
                    cool: 0.0,
                    dead: None,
                    flash: 0.0,
                    kick: 0.0,
                    marker: 0.0,
                },
                graph,
                level: level.clone(),
                pickups: Vec::new(),
                outcome: Outcome::Running,
                rng: seed,
                draw: t.view.draw_secs,
                feed: String::new(),
            };
            // Bots start at the spawn points farthest from you.
            let mut taken = vec![walker.pos];
            for i in 0..n {
                let s = farthest_spawn(&level.spawns, &taken);
                let (at, yaw) = level.spawns.get(s).copied().unwrap_or((walker.pos, 0.0));
                taken.push(at);
                commands.spawn((
                    Bot {
                        id: i as u32 + 1,
                        health: t.bots.health,
                        mode: Mode::Patrol,
                        cool: 0.0,
                        route: Vec::new(),
                        step: 0,
                        yaw,
                        dying: None,
                        respawn: 0.0,
                        last: at,
                    },
                    MatchProp,
                    Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)),
                    Visibility::default(),
                    crate::rigged::Rigged {
                        path: bot_model(i as u32 + 1),
                    },
                    crate::rigged::RigClip::looping("idle"),
                    friendslop_base::areas::show(AREA),
                ));
            }
            for p in &level.pickups {
                let colour = match p.kind {
                    PickupKind::Health => Color::srgb(0.2, 1.0, 0.3),
                    PickupKind::Ammo => Color::srgb(1.0, 0.8, 0.2),
                };
                let entity = commands
                    .spawn((
                        PickupVisual,
                        MatchProp,
                        Mesh3d(meshes.add(Cuboid::new(0.35, 0.35, 0.35))),
                        MeshMaterial3d(materials.add(StandardMaterial {
                            base_color: colour,
                            emissive: colour.to_linear() * 3.0,
                            ..default()
                        })),
                        Transform::from_translation(p.at + Vec3::Y * 0.9),
                        friendslop_base::areas::show(AREA),
                    ))
                    .id();
                run.pickups.push(Pickup {
                    spawn: *p,
                    cooldown: 0.0,
                    entity: Some(entity),
                });
            }
            // The rifle, in your hands.
            if let Ok(camera) = camera.single() {
                let v = &t.view;
                let grip = Vec3::new(v.grip.0, v.grip.1, v.grip.2);
                let hand = Vec3::new(v.hand.0, v.hand.1, v.hand.2);
                // The model has its barrel along +z: turn it to face -z, with the grip at `hand`.
                let rot = Quat::from_rotation_y(std::f32::consts::PI);
                let at = hand - rot * grip * v.scale;
                let gun = commands
                    .spawn((
                        Viewmodel {
                            walked: 0.0,
                            clip_left: v.draw_secs,
                            look: Vec2::ZERO,
                        },
                        MatchProp,
                        Transform::from_translation(at)
                            .with_rotation(rot)
                            .with_scale(Vec3::splat(v.scale)),
                        Visibility::default(),
                        crate::rigged::Rigged { path: GUN_MODEL },
                        crate::rigged::RigClip::once("draw").fade(0.01),
                    ))
                    .id();
                commands.entity(camera).add_child(gun);
            }
            if let Some(assets) = &assets {
                let load = |n: &str| assets.load(format!("audio/sfx/{n}.ogg"));
                commands.insert_resource(FragSounds {
                    shot: load("fps_rifle_shot"),
                    mag_out: load("fps_mag_out"),
                    mag_in: load("fps_mag_in"),
                    bolt: load("fps_bolt"),
                });
            }
            view.first_person = true;
            busy.0 = false;
            m.0 = Some(run);
        }
        (false, true) => {
            m.0 = None;
            busy.0 = false;
            for e in &props {
                commands.entity(e).despawn();
            }
        }
        _ => {}
    }
}

/// Mouse and the rest, for the rifle.
#[allow(clippy::too_many_arguments)]
fn player_play(
    mut commands: Commands,
    time: Res<Time>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    actions: crate::input::Actions,
    me: Option<Res<LocalPlayer>>,
    bodies: Query<(&Player, &Walker)>,
    view: Res<FpsView>,
    mut m: ResMut<Match>,
    mut busy: ResMut<HandsBusy>,
    mut bots: Query<(&mut Bot, &Transform), Without<Viewmodel>>,
    mut gun: Query<&mut Viewmodel>,
    mut clips: Query<&mut crate::rigged::RigClip, With<Viewmodel>>,
    sight: Sight,
    sounds: Option<Res<FragSounds>>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let Some(me_id) = me else { return };
    let Some((_, walker)) = bodies.iter().find(|(p, _)| p.id == me_id.0) else {
        return;
    };
    let Some(run) = m.0.as_mut() else { return };
    let t = tuning();
    let dt = time.delta_secs().min(0.1);
    run.elapsed += dt;
    run.draw = (run.draw - dt).max(0.0);
    run.me.flash = (run.me.flash - dt * 2.0).max(0.0);
    run.me.marker = (run.me.marker - dt * 4.0).max(0.0);
    run.me.cool = (run.me.cool - dt).max(0.0);
    run.me.kick = (run.me.kick - t.gun.kick_settle.to_radians() * dt).max(0.0);
    for p in &mut run.pickups {
        p.cooldown = (p.cooldown - dt).max(0.0);
    }

    // The clock and the kill limit.
    if run.outcome == Outcome::Running {
        run.outcome = outcome(&run.scores, run.elapsed, &t.match_rules);
        if run.outcome != Outcome::Running {
            run.feed = match run.outcome {
                Outcome::Won(0) | Outcome::TimeUp(0) => "YOU WIN".into(),
                Outcome::Tie => "TIE".into(),
                _ => "YOU LOSE".into(),
            };
            if let Some(mut toast) = toast {
                toast.show(format!("FRAG: {}", run.feed));
            }
        }
    }
    if run.outcome != Outcome::Running {
        // Done: fire to play again; leaving is `arcade_quit`.
        if mouse.just_pressed(MouseButton::Left) {
            let n = run.scores.len();
            run.scores = vec![Score::default(); n];
            run.elapsed = 0.0;
            run.outcome = Outcome::Running;
            run.me.health = t.match_rules.player_health;
            run.me.mag = t.gun.mag;
            run.me.reserve = t.gun.reserve;
            run.feed.clear();
            for (mut b, _) in &mut bots {
                b.health = t.bots.health;
                b.dying = None;
            }
        }
        return;
    }

    // Dead: wait, then ask the host for a spawn point.
    if let Some(left) = run.me.dead.as_mut() {
        *left -= dt;
        if *left <= 0.0 {
            run.me.dead = None;
            run.me.health = t.match_rules.player_health;
            run.me.mag = t.gun.mag;
            run.me.reserve = t.gun.reserve;
            let enemies: Vec<Vec3> = bots.iter().map(|(_, tf)| tf.translation).collect();
            let s = farthest_spawn(&run.level.spawns, &enemies);
            commands.client_trigger(ArcadeFps::Respawn(s as u8));
            busy.0 = false;
            if let Ok(mut clip) = clips.single_mut() {
                clip.set_if_neq(crate::rigged::RigClip::once("draw").fade(0.01));
            }
            if let Ok(mut g) = gun.single_mut() {
                g.clip_left = t.view.draw_secs;
            }
            run.draw = t.view.draw_secs;
        }
        return;
    }

    // Pickups.
    for p in &mut run.pickups {
        if p.cooldown > 0.0 || walker.pos.distance(p.spawn.at) > t.match_rules.pickup_radius {
            continue;
        }
        let used = match p.spawn.kind {
            PickupKind::Health if run.me.health < t.match_rules.player_health => {
                run.me.health = (run.me.health + p.spawn.amount).min(t.match_rules.player_health);
                true
            }
            PickupKind::Ammo if run.me.reserve < t.gun.reserve => {
                run.me.reserve = (run.me.reserve + p.spawn.amount as u32).min(t.gun.reserve);
                true
            }
            _ => false,
        };
        if used {
            p.cooldown = t.match_rules.pickup_respawn_secs;
            if let Some(e) = p.entity {
                commands.entity(e).insert(Visibility::Hidden);
            }
        } else if let Some(e) = p.entity {
            let _ = e;
        }
    }
    for p in &run.pickups {
        if let Some(e) = p.entity {
            commands.entity(e).insert(if p.cooldown > 0.0 {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
        }
    }

    // Reload.
    if let Some(left) = run.me.reload.as_mut() {
        *left -= dt;
        let done = t.gun.reload_secs - *left;
        let cues = t.gun.reload_cues;
        if let Some(sounds) = &sounds {
            let (due, which) = match run.me.cue {
                0 => (cues.0, &sounds.mag_out),
                1 => (cues.1, &sounds.mag_in),
                2 => (cues.2, &sounds.bolt),
                _ => (f32::INFINITY, &sounds.bolt),
            };
            if done >= due {
                crate::assets::play_sfx(&mut commands, which, 0.7);
                run.me.cue += 1;
            }
        } else if done >= cues.2 {
            run.me.cue = 3;
        }
        if *left <= 0.0 {
            let need = t.gun.mag - run.me.mag;
            let take = need.min(run.me.reserve);
            run.me.mag += take;
            run.me.reserve -= take;
            run.me.reload = None;
        }
    } else if (actions.just_pressed(crate::input::Act("fps_reload"))
        || keys.just_pressed(KeyCode::KeyR))
        && run.me.mag < t.gun.mag
        && run.me.reserve > 0
    {
        run.me.reload = Some(t.gun.reload_secs);
        run.me.cue = 0;
        if let Ok(mut clip) = clips.single_mut() {
            clip.set_if_neq(crate::rigged::RigClip::once("reload").fade(0.05));
        }
        if let Ok(mut g) = gun.single_mut() {
            g.clip_left = t.gun.reload_secs;
        }
    }

    // Fire.
    let firing = mouse.pressed(MouseButton::Left)
        && run.me.reload.is_none()
        && run.me.cool <= 0.0
        && run.draw <= 0.0;
    if firing {
        if run.me.mag == 0 {
            run.me.cool = 0.3;
            if run.reserve_left() {
                run.me.reload = Some(t.gun.reload_secs);
                run.me.cue = 0;
                if let Ok(mut clip) = clips.single_mut() {
                    clip.set_if_neq(crate::rigged::RigClip::once("reload").fade(0.05));
                }
                if let Ok(mut g) = gun.single_mut() {
                    g.clip_left = t.gun.reload_secs;
                }
            }
        } else {
            run.me.mag -= 1;
            run.me.cool = 60.0 / t.gun.rounds_per_min.max(1.0);
            run.me.kick += t.gun.kick_deg.to_radians();
            let eye = walker.pos + Vec3::Y * EYE;
            let speed = 0.0f32;
            let spread = gun_spread(&t.gun, speed, 5.0);
            let aim = crate::features::duel::look_dir(view.yaw, view.pitch + run.me.kick * 0.5);
            let rnd = (run.rnd(), run.rnd());
            let dir = spread_dir(aim, spread, rnd);
            let targets: Vec<(u32, Vec3)> = bots
                .iter()
                .filter(|(b, _)| b.dying.is_none())
                .map(|(b, tf)| (b.id, tf.translation))
                .collect();
            let wall = wall_dist(&sight, eye, dir);
            if let Some((id, hit)) = first_hit(eye, dir, &targets, wall, t.gun.range) {
                let dmg = shot_damage(t.gun.damage, t.gun.head_mult, hit.head);
                run.me.marker = 1.0;
                if let Some((mut bot, _)) = bots.iter_mut().find(|(b, _)| b.id == id) {
                    bot.health -= dmg;
                    if bot.health <= 0.0 && bot.dying.is_none() {
                        bot.dying = Some(0.0);
                        bot.respawn = t.bots.respawn_secs;
                        run.scores[0].kills += 1;
                        if let Some(s) = run.scores.get_mut(id as usize) {
                            s.deaths += 1;
                        }
                        run.feed = format!(
                            "you fragged bot {id}{}",
                            if hit.head { " (headshot)" } else { "" }
                        );
                    }
                }
            }
            if let Ok(mut clip) = clips.single_mut() {
                clip.set_if_neq(crate::rigged::RigClip::once("fire").fade(0.01));
            }
            if let Ok(mut g) = gun.single_mut() {
                g.clip_left = t.view.fire_secs;
            }
            if let Some(sounds) = &sounds {
                crate::assets::play_sfx(&mut commands, &sounds.shot, 0.8);
            }
        }
    }
}

impl Running {
    fn reserve_left(&self) -> bool {
        self.me.reserve > 0
    }
}

/// Bots: think, move, shoot, die and return.
#[allow(clippy::too_many_arguments)]
fn bots_play(
    mut commands: Commands,
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    bodies: Query<(&Player, &Walker)>,
    mut m: ResMut<Match>,
    mut bots: Query<(
        Entity,
        &mut Bot,
        &mut Transform,
        &mut crate::rigged::RigClip,
    )>,
    sight: Sight,
    sfx: Option<Res<crate::assets::Sfx>>,
    sounds: Option<Res<FragSounds>>,
    mut busy: ResMut<HandsBusy>,
) {
    let Some(me_id) = me else { return };
    let Some((_, walker)) = bodies.iter().find(|(p, _)| p.id == me_id.0) else {
        return;
    };
    let Some(run) = m.0.as_mut() else { return };
    if run.outcome != Outcome::Running {
        return;
    }
    let t = tuning();
    let dt = time.delta_secs().min(0.1);
    let now = run.elapsed;
    let my_pos = walker.pos;
    let my_alive = run.me.dead.is_none();

    // Everyone's positions this tick (id 0 = you).
    let snapshot: Vec<(u32, Vec3, bool)> = std::iter::once((0, my_pos, my_alive))
        .chain(
            bots.iter()
                .map(|(_, b, tf, _)| (b.id, tf.translation, b.dying.is_none())),
        )
        .collect();
    // Damage dealt to you this tick, and kills by bots (killer, victim).
    let mut hurt_me = 0.0;
    let mut my_killer: Option<u32> = None;
    let mut kills: Vec<(u32, u32)> = Vec::new();
    let mut dmg_to_bots: Vec<(u32, f32, u32)> = Vec::new();

    for (entity, mut bot, mut tf, mut clip) in &mut bots {
        // Dying and coming back.
        if let Some(secs) = bot.dying.as_mut() {
            *secs += dt;
            let k = (*secs / t.bots.death_secs.max(0.1)).min(1.0);
            tf.rotation = Quat::from_rotation_y(bot.yaw)
                * Quat::from_rotation_x(-k * std::f32::consts::FRAC_PI_2);
            clip.set_if_neq(crate::rigged::RigClip::once("hit").fade(0.05));
            bot.respawn -= dt;
            if bot.respawn <= 0.0 {
                let others: Vec<Vec3> = snapshot.iter().filter(|s| s.2).map(|s| s.1).collect();
                let s = farthest_spawn(&run.level.spawns, &others);
                let (at, yaw) = run
                    .level
                    .spawns
                    .get(s)
                    .copied()
                    .unwrap_or((tf.translation, 0.0));
                tf.translation = at;
                bot.yaw = yaw;
                tf.rotation = Quat::from_rotation_y(yaw);
                bot.health = t.bots.health;
                bot.dying = None;
                bot.mode = Mode::Patrol;
                bot.route.clear();
                bot.last = at;
            }
            continue;
        }
        bot.cool += dt;
        let eye = tf.translation + Vec3::Y * EYE;

        // Who can this bot see? The nearest enemy in its cone with a clear line.
        let mut seen: Option<Sighting> = None;
        for &(id, pos, alive) in &snapshot {
            if id == bot.id || !alive {
                continue;
            }
            let target_eye = pos + Vec3::Y * 1.2;
            let dist = tf.translation.distance(pos);
            if dist > t.bots.sight
                || !in_view(eye, bot.yaw, target_eye, t.bots.sight, t.bots.fov_deg)
                || !clear_line(&sight, eye, target_eye)
            {
                continue;
            }
            if seen.is_none_or(|s| dist < s.dist) {
                seen = Some(Sighting {
                    target: id,
                    at: pos,
                    dist,
                });
            }
        }
        let before = bot.mode;
        bot.mode = next_mode(bot.mode, seen, dt, t.bots.memory_secs);
        if let (Mode::Engage { .. }, Mode::Hunt { .. }) = (before, bot.mode)
            && let Some(last) = snapshot
                .iter()
                .find(|s| matches!(before, Mode::Engage { target, .. } if target == s.0))
        {
            bot.mode = Mode::Hunt {
                at: last.1,
                left: t.bots.memory_secs,
            };
            bot.route.clear();
        }

        // Where to go and how fast.
        let mut goal: Option<Vec3> = None;
        let mut speed = t.bots.walk_speed;
        let mut strafe = 0.0;
        match bot.mode {
            Mode::Engage { target, .. } => {
                if let Some(s) = seen.filter(|s| s.target == target) {
                    let want = t.bots.preferred_range;
                    if s.dist > want * 1.3 {
                        goal = Some(s.at);
                        speed = t.bots.run_speed;
                    } else if s.dist < want * 0.6 {
                        goal = Some(tf.translation - (s.at - tf.translation));
                    }
                    strafe = strafe_sign(now, bot.id);
                    // Face the target.
                    let want_yaw = yaw_toward(tf.translation, s.at);
                    bot.yaw = turn_toward(bot.yaw, want_yaw, 6.0 * dt);
                }
            }
            Mode::Hunt { at, .. } => {
                goal = Some(at);
                speed = t.bots.run_speed;
            }
            Mode::Patrol => {
                // Follow the route; pick a new one at the end.
                if bot.step >= bot.route.len()
                    && let Some(from) = run.graph.nearest(tf.translation)
                    && !run.graph.points.is_empty()
                {
                    let to = (run.rnd() * run.graph.points.len() as f32) as usize
                        % run.graph.points.len();
                    bot.route = run.graph.path(from, to).unwrap_or_default();
                    bot.step = 0;
                }
                if let Some(&w) = bot.route.get(bot.step) {
                    let p = run.graph.points[w];
                    if tf.translation.xz().distance(p.xz()) < 0.7 {
                        bot.step += 1;
                    } else {
                        goal = Some(p);
                    }
                }
            }
        }

        // Steer (flat), with sideways strafing, and stay on the floor.
        let mut vel = Vec3::ZERO;
        if let Some(g) = goal {
            let d = Vec3::new(g.x - tf.translation.x, 0.0, g.z - tf.translation.z);
            if d.length() > 0.2 {
                vel += d.normalize() * speed;
                if matches!(bot.mode, Mode::Patrol | Mode::Hunt { .. }) {
                    let want_yaw = yaw_toward(tf.translation, tf.translation + d);
                    bot.yaw = turn_toward(bot.yaw, want_yaw, 8.0 * dt);
                }
            }
        }
        if strafe != 0.0 {
            let right = Vec3::new(bot.yaw.cos(), 0.0, -bot.yaw.sin());
            vel += right * strafe * speed * 0.6;
        }
        let step = vel * dt;
        let next = tf.translation + step;
        // Walls: don't step into one.
        let probe = tf.translation + Vec3::Y * 0.6;
        let blocked = step.length() > 0.0001
            && wall_dist(&sight, probe, step.normalize()).is_some_and(|d| d < step.length() + 0.4);
        if !blocked {
            tf.translation.x = next.x;
            tf.translation.z = next.z;
        }
        tf.translation.y = floor_y(&sight, tf.translation, tf.translation.y);
        tf.rotation = Quat::from_rotation_y(bot.yaw);
        // Gait.
        let moved = (tf.translation - bot.last).length() / dt.max(0.001);
        bot.last = tf.translation;
        let next_clip = if matches!(bot.mode, Mode::Engage { .. }) && moved < 0.5 {
            crate::rigged::RigClip::looping("shoot")
        } else if moved > 0.3 {
            crate::rigged::RigClip::looping("walk")
                .speed((moved / t.bots.walk_speed.max(0.1)).clamp(0.5, 2.0))
        } else {
            crate::rigged::RigClip::looping("idle")
        };
        clip.set_if_neq(next_clip);

        // Shoot.
        if let (Mode::Engage { target, seen_for }, Some(s)) = (bot.mode, seen)
            && s.target == target
            && may_fire(seen_for, t.bots.reaction_secs, bot.cool, t.bots.fire_secs)
        {
            bot.cool = 0.0;
            let origin = eye;
            let aim_at = s.at + Vec3::Y * 1.2;
            let dir = (aim_at - origin).normalize_or_zero();
            let spread = (t.bots.spread_deg_per_10m * (s.dist / 10.0).max(0.3)).to_radians();
            let rnd = (run.rnd(), run.rnd());
            let dir = spread_dir(dir, spread, rnd);
            // Does it land on the target (you or another bot)?
            let victims: Vec<(u32, Vec3)> = snapshot
                .iter()
                .filter(|s| s.0 != bot.id && s.2)
                .map(|s| (s.0, s.1))
                .collect();
            let wall = wall_dist(&sight, origin, dir);
            if let Some((id, hit)) = first_hit(origin, dir, &victims, wall, t.bots.sight) {
                let dmg = shot_damage(t.bots.damage, 1.5, hit.head);
                if id == 0 {
                    hurt_me += dmg;
                    my_killer = Some(bot.id);
                } else {
                    dmg_to_bots.push((id, dmg, bot.id));
                }
            }
            if let Some(sounds) = &sounds {
                crate::spatial::play_at(
                    &mut commands,
                    &sounds.shot,
                    tf.translation + Vec3::Y,
                    0.7,
                    crate::spatial::Carry::LOUD,
                );
            }
        }
        let _ = entity;
    }

    // Damage between bots.
    for (victim, dmg, killer) in dmg_to_bots {
        if let Some((_, mut b, _, _)) = bots.iter_mut().find(|(_, b, _, _)| b.id == victim) {
            if b.dying.is_some() {
                continue;
            }
            b.health -= dmg;
            if b.health <= 0.0 {
                b.dying = Some(0.0);
                b.respawn = t.bots.respawn_secs;
                kills.push((killer, victim));
            }
        }
    }
    for (killer, victim) in kills {
        if let Some(s) = run.scores.get_mut(killer as usize) {
            s.kills += 1;
        }
        if let Some(s) = run.scores.get_mut(victim as usize) {
            s.deaths += 1;
        }
        run.feed = format!("bot {killer} fragged bot {victim}");
    }

    // Damage to you.
    if hurt_me > 0.0 && run.me.dead.is_none() {
        run.me.health -= hurt_me;
        run.me.flash = 1.0;
        if let Some(sfx) = &sfx {
            crate::assets::play_sfx(&mut commands, &sfx.punch, 0.5);
        }
        if run.me.health <= 0.0 {
            run.me.dead = Some(t.match_rules.respawn_secs);
            run.scores[0].deaths += 1;
            if let Some(k) = my_killer
                && let Some(s) = run.scores.get_mut(k as usize)
            {
                s.kills += 1;
                run.feed = format!("bot {k} fragged you");
            }
            busy.0 = true;
        }
    }
}

/// The machine's sounds (made with the sounder: `fps_rifle_shot`,
/// `fps_mag_out`, `fps_mag_in`, `fps_bolt`).
#[derive(Resource, Clone)]
struct FragSounds {
    shot: Handle<crate::sound::AudioSample>,
    mag_out: Handle<crate::sound::AudioSample>,
    mag_in: Handle<crate::sound::AudioSample>,
    bolt: Handle<crate::sound::AudioSample>,
}

/// Turn `yaw` toward `want` by at most `step` radians.
pub fn turn_toward(yaw: f32, want: f32, step: f32) -> f32 {
    let mut d = (want - yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU)
        - std::f32::consts::PI;
    d = d.clamp(-step, step);
    yaw + d
}

/// The rifle's motion: bob, sway, kick, lowering at a wall, and back to
/// idle once a clip finishes.
#[allow(clippy::too_many_arguments)]
fn viewmodel_motion(
    time: Res<Time>,
    m: Res<Match>,
    view: Res<FpsView>,
    me: Option<Res<LocalPlayer>>,
    bodies: Query<(&Player, &Walker)>,
    mut gun: Query<(&mut Viewmodel, &mut Transform, &mut crate::rigged::RigClip)>,
    sight: Sight,
    mut last_pos: Local<Option<Vec3>>,
) {
    let Some(run) = m.0.as_ref() else { return };
    let Ok((mut vm, mut tf, mut clip)) = gun.single_mut() else {
        return;
    };
    let Some(me_id) = me else { return };
    let Some((_, walker)) = bodies.iter().find(|(p, _)| p.id == me_id.0) else {
        return;
    };
    let t = tuning();
    let v = &t.view;
    let dt = time.delta_secs().min(0.1);
    // Walked distance drives the bob.
    let moved = last_pos.map_or(0.0, |p| (walker.pos - p).xz().length());
    *last_pos = Some(walker.pos);
    vm.walked += moved;
    let bob = (vm.walked * v.bob_rate * std::f32::consts::TAU).sin();
    let bob_x = (vm.walked * v.bob_rate * std::f32::consts::PI).sin() * v.bob * 0.5;
    let amp = (moved / dt.max(0.001) / 5.0).clamp(0.0, 1.0);
    // Sway: the gun lags a little behind looking about.
    let look = Vec2::new(view.yaw, view.pitch);
    let delta = look - vm.look;
    vm.look = look;
    let sway = Vec3::new(-delta.x, delta.y, 0.0) * v.sway;
    let grip = Vec3::new(v.grip.0, v.grip.1, v.grip.2);
    let hand = Vec3::new(v.hand.0, v.hand.1, v.hand.2);
    let rot = Quat::from_rotation_y(std::f32::consts::PI);
    let base = hand - rot * grip * v.scale;
    // Lower the gun near a wall, and when dead.
    let eye = walker.pos + Vec3::Y * EYE;
    let ahead = crate::features::duel::look_dir(view.yaw, view.pitch);
    let near = wall_dist(&sight, eye, ahead).is_some_and(|d| d < v.wall_lower);
    let lower = if near || run.me.dead.is_some() {
        -0.25
    } else {
        0.0
    };
    let target = base
        + Vec3::new(bob_x * amp, bob.abs() * v.bob * amp, 0.0)
        + sway.clamp(Vec3::splat(-0.05), Vec3::splat(0.05))
        + Vec3::new(0.0, lower, 0.0)
        + Vec3::new(0.0, 0.0, run.me.kick * 0.4);
    let k = 1.0 - (-18.0 * dt).exp();
    tf.translation = tf.translation.lerp(target, k);
    tf.rotation = Quat::from_rotation_y(std::f32::consts::PI)
        * Quat::from_rotation_x(run.me.kick * 0.6 - if near { 0.5 } else { 0.0 });
    // Clips: a one-shot back to idle when it ends.
    if vm.clip_left > 0.0 {
        vm.clip_left -= dt;
        if vm.clip_left <= 0.0 {
            clip.set_if_neq(crate::rigged::RigClip::looping("idle"));
        }
    }
}

/// The HUD.
#[allow(clippy::too_many_arguments)]
fn show_hud(
    m: Res<Match>,
    mut status: Query<&mut Text, (With<StatusText>, Without<ScoreText>, Without<BannerText>)>,
    mut score: Query<&mut Text, (With<ScoreText>, Without<StatusText>, Without<BannerText>)>,
    mut banner: Query<&mut Text, (With<BannerText>, Without<StatusText>, Without<ScoreText>)>,
    mut flash: Query<&mut BackgroundColor, With<Flash>>,
) {
    let t = tuning();
    let (status_text, score_text, banner_text, flash_a) = match m.0.as_ref() {
        None => (String::new(), String::new(), String::new(), 0.0),
        Some(run) => {
            let left = (t.match_rules.time_limit - run.elapsed).max(0.0);
            let reload = run
                .me
                .reload
                .map(|r| format!("  reloading {:.1}", r.max(0.0)))
                .unwrap_or_default();
            let s = format!(
                "HP {:>3}   AMMO {}/{}{reload}   FRAGS {}/{}   {}:{:02}\n{}",
                run.me.health.max(0.0).round(),
                run.me.mag,
                run.me.reserve,
                run.scores[0].kills,
                t.match_rules.kill_limit,
                (left / 60.0) as u32,
                (left % 60.0) as u32,
                run.feed
            );
            let mut sc = String::from("FRAG\n");
            let mut order: Vec<usize> = (0..run.scores.len()).collect();
            order.sort_by_key(|&i| std::cmp::Reverse(run.scores[i].kills));
            for i in order {
                let name = if i == 0 {
                    "you".to_string()
                } else {
                    format!("bot {i}")
                };
                sc.push_str(&format!(
                    "{name:<8} {:>3} / {:<3}\n",
                    run.scores[i].kills, run.scores[i].deaths
                ));
            }
            let b = match (run.outcome, run.me.dead) {
                (Outcome::Running, Some(d)) => format!("YOU DIED  {:.0}", d.max(0.0).ceil()),
                (Outcome::Running, None) => String::new(),
                (_, _) => format!(
                    "{}\n{{arcade_quit}} to leave, click to play again",
                    run.feed
                ),
            };
            (s, sc, crate::input::fill(&b), run.me.flash * 0.45)
        }
    };
    for mut x in &mut status {
        if x.0 != status_text {
            x.0 = status_text.clone();
        }
    }
    for mut x in &mut score {
        if x.0 != score_text {
            x.0 = score_text.clone();
        }
    }
    for mut x in &mut banner {
        if x.0 != banner_text {
            x.0 = banner_text.clone();
        }
    }
    for mut f in &mut flash {
        f.0 = Color::srgba(0.8, 0.0, 0.0, flash_a);
    }
}

/// Leave with `arcade_quit`.
fn quit(
    mut commands: Commands,
    actions: crate::input::Actions,
    mut m: ResMut<Match>,
    mut busy: ResMut<HandsBusy>,
) {
    if m.0.is_some() && actions.just_pressed(crate::input::Act("arcade_quit")) {
        commands.client_trigger(ArcadeFps::Exit);
        // The match ends when the body is out (`lifecycle`); free the hands now.
        busy.0 = false;
        if let Some(run) = m.0.as_mut() {
            run.outcome = Outcome::Tie;
        }
    }
}

fn fps_context(m: Res<Match>, mut ctx: ResMut<crate::input::InputContexts>) {
    crate::input::switch(&mut ctx, "fps", m.0.is_some());
}

fn stop(mut m: ResMut<Match>, mut busy: ResMut<HandsBusy>) {
    m.0 = None;
    busy.0 = false;
}

/// The soundtrack's `arcade_fps` signal: on while the camera is in the arena
/// (moods.ron plays the FRAG mood for it).
fn music_signals(
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::world::MapCamera>)>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    let inside = crate::music::camera_at(&camera).is_some_and(in_arena);
    signals.flag("arcade_fps", inside);
}

/// `FRIENDSLOP_START=arcade_fps` (the first spawn point, in the match).
fn starts() -> Vec<(String, Vec3)> {
    level()
        .spawns
        .first()
        .map(|s| vec![("arcade_fps".to_string(), s.0)])
        .unwrap_or_default()
}

pub fn plugin(app: &mut App) {
    crate::features::start::provide(starts);
    friendslop_base::areas::on_sim(app, AREA, build_colliders);
    friendslop_base::areas::on_show(app, AREA, build_arena);
    friendslop_base::areas::on_sim(app, michael::AREA, spawn_cabinets);
    app.init_resource::<Match>()
        .add_observer(click)
        .add_hud(spawn_ui)
        .add_systems(
            Update,
            (
                lifecycle,
                player_play,
                bots_play,
                viewmodel_motion,
                show_hud,
                quit,
                fps_context,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(Update, music_signals.run_if(in_state(AppState::InGame)))
        .add_systems(OnExit(AppState::InGame), stop);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gun() -> Gun {
        tuning().gun.clone()
    }

    #[test]
    fn the_data_parses_and_is_sane() {
        let t = tuning();
        assert!(t.bots.count >= 2 && t.match_rules.kill_limit > 3);
        assert!(t.gun.mag > 0 && t.gun.damage > 0.0 && t.gun.rounds_per_min > 100.0);
        assert!(t.view.scale > 0.0);
    }

    #[test]
    fn a_ray_hits_a_body_and_the_head_counts() {
        let feet = Vec3::new(0.0, 0.0, -10.0);
        let origin = Vec3::new(0.0, 1.6, 0.0);
        // Straight at the head.
        let head = ray_body(origin, (feet + Vec3::Y * 1.68 - origin).normalize(), feet);
        assert!(head.is_some_and(|h| h.head), "{head:?}");
        // At the chest.
        let chest = ray_body(origin, (feet + Vec3::Y * 1.2 - origin).normalize(), feet);
        assert!(chest.is_some_and(|h| !h.head));
        // Wide of the body, and behind the shooter.
        assert!(ray_body(origin, Vec3::new(1.0, 0.0, -1.0).normalize(), feet).is_none());
        assert!(ray_body(origin, Vec3::Z, feet).is_none());
    }

    #[test]
    fn walls_and_range_stop_a_shot() {
        let origin = Vec3::new(0.0, 1.6, 0.0);
        let near = (1, Vec3::new(0.0, 0.0, -5.0));
        let far = (2, Vec3::new(0.0, 0.0, -20.0));
        let dir = (near.1 + Vec3::Y * 1.2 - origin).normalize();
        let hit = first_hit(origin, dir, &[far, near], None, 100.0);
        assert_eq!(hit.map(|h| h.0), Some(1), "the nearer one is hit");
        assert!(
            first_hit(origin, dir, &[near], Some(2.0), 100.0).is_none(),
            "a wall at 2 m blocks it"
        );
        assert!(
            first_hit(origin, dir, &[near], None, 3.0).is_none(),
            "out of range"
        );
    }

    #[test]
    fn spread_widens_with_movement_and_stays_unit() {
        let g = gun();
        assert!(gun_spread(&g, 5.0, 5.0) > gun_spread(&g, 0.0, 5.0));
        for i in 0..20 {
            let d = spread_dir(
                Vec3::NEG_Z,
                0.05,
                (i as f32 / 20.0, (i * 7 % 20) as f32 / 20.0),
            );
            assert!((d.length() - 1.0).abs() < 1e-4);
            assert!(d.dot(Vec3::NEG_Z) >= 0.05f32.cos() - 1e-4);
        }
        assert_eq!(shot_damage(20.0, 2.0, true), 40.0);
        assert_eq!(shot_damage(20.0, 2.0, false), 20.0);
    }

    #[test]
    fn the_rules_end_the_match() {
        let rules = Rules {
            kill_limit: 10,
            time_limit: 300.0,
            ..default()
        };
        let mut scores = vec![Score::default(); 3];
        assert_eq!(outcome(&scores, 10.0, &rules), Outcome::Running);
        scores[2].kills = 10;
        assert_eq!(outcome(&scores, 10.0, &rules), Outcome::Won(2));
        scores[2].kills = 4;
        scores[0].kills = 6;
        assert_eq!(outcome(&scores, 301.0, &rules), Outcome::TimeUp(0));
        scores[1].kills = 6;
        assert_eq!(outcome(&scores, 301.0, &rules), Outcome::Tie);
    }

    #[test]
    fn bots_spot_lose_and_forget() {
        let seen = Some(Sighting {
            target: 0,
            at: Vec3::ZERO,
            dist: 5.0,
        });
        let m = next_mode(Mode::Patrol, seen, 0.1, 4.0);
        assert!(matches!(m, Mode::Engage { target: 0, .. }));
        let m = next_mode(m, seen, 0.5, 4.0);
        assert!(matches!(m, Mode::Engage { seen_for, .. } if seen_for > 0.4));
        let m = next_mode(m, None, 0.1, 4.0);
        assert!(matches!(m, Mode::Hunt { .. }));
        let m = next_mode(m, None, 5.0, 4.0);
        assert_eq!(m, Mode::Patrol);
        assert!(
            !may_fire(0.1, 0.4, 5.0, 0.2),
            "the reaction delay holds fire"
        );
        assert!(may_fire(0.5, 0.4, 0.3, 0.2));
        assert!(!may_fire(0.5, 0.4, 0.1, 0.2), "so does the rate of fire");
    }

    #[test]
    fn a_bot_sees_only_what_is_in_front() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        // Yaw 0 faces -Z.
        assert!(in_view(eye, 0.0, Vec3::new(0.0, 1.0, -10.0), 30.0, 100.0));
        assert!(
            !in_view(eye, 0.0, Vec3::new(0.0, 1.0, 10.0), 30.0, 100.0),
            "behind"
        );
        assert!(
            !in_view(eye, 0.0, Vec3::new(0.0, 1.0, -40.0), 30.0, 100.0),
            "too far"
        );
        assert!((yaw_toward(Vec3::ZERO, Vec3::new(0.0, 0.0, -5.0))).abs() < 1e-5);
    }

    #[test]
    fn pathing_walks_the_graph() {
        // A line of four points with the middle two joined to both neighbours only.
        let pts = vec![
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
            Vec3::new(15.0, 0.0, 0.0),
        ];
        let g = Graph::build(pts, 6.0, |_, _| true);
        assert_eq!(g.path(0, 3), Some(vec![0, 1, 2, 3]));
        assert_eq!(g.nearest(Vec3::new(9.0, 0.0, 1.0)), Some(2));
        // A wall between the middle two splits the graph.
        let pts = vec![
            Vec3::ZERO,
            Vec3::new(5.0, 0.0, 0.0),
            Vec3::new(10.0, 0.0, 0.0),
        ];
        let g = Graph::build(pts, 6.0, |a, b| !(a.x < 6.0 && b.x > 6.0));
        assert_eq!(g.path(0, 2), None);
    }

    #[test]
    fn spawns_are_far_from_the_fighting() {
        let spawns = vec![
            (Vec3::new(-10.0, 0.0, 0.0), 0.0),
            (Vec3::new(10.0, 0.0, 0.0), 0.0),
        ];
        assert_eq!(farthest_spawn(&spawns, &[Vec3::new(9.0, 0.0, 0.0)]), 0);
        assert_eq!(farthest_spawn(&spawns, &[Vec3::new(-9.0, 0.0, 0.0)]), 1);
    }

    #[test]
    fn bots_turn_the_short_way() {
        let tau = std::f32::consts::TAU;
        assert!((turn_toward(0.1, tau - 0.1, 1.0) - (-0.1)).abs() < 1e-4);
        assert!((turn_toward(0.0, 1.0, 0.25) - 0.25).abs() < 1e-5);
    }

    #[test]
    fn the_map_has_what_the_code_names() {
        let l = level();
        let t = tuning();
        assert!(l.spawns.len() >= 6, "spawn points");
        assert!(l.waypoints.len() >= 12, "waypoints");
        assert!(l.pickups.iter().any(|p| p.kind == PickupKind::Health));
        assert!(l.pickups.iter().any(|p| p.kind == PickupKind::Ammo));
        // Everything sits inside the arena and its bounds check.
        for (p, _) in &l.spawns {
            assert!(in_arena(*p), "{p:?} is outside the arena");
        }
        assert!(t.arena_half.0 > 10.0);
        // The graph is connected enough to patrol: every waypoint reaches the first.
        let g = Graph::build(l.waypoints.clone(), 14.0, |_, _| true);
        for i in 1..g.points.len() {
            assert!(g.path(0, i).is_some(), "waypoint {i} is cut off");
        }
        // The theatre has the cabinet that starts it.
        assert!(!cabinets().is_empty(), "no FRAG cabinet on the theatre map");
    }

    /// A headless app with the player systems, you in the arena.
    fn arena_app() -> (App, Entity) {
        use bevy::time::TimeUpdateStrategy;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_asset::<bevy::world_serialization::WorldAsset>()
            .init_asset::<crate::sound::AudioSample>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(
                std::time::Duration::from_micros(16_667),
            ))
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<FpsView>()
            .init_resource::<HandsBusy>()
            .init_resource::<Match>()
            .insert_resource(LocalPlayer(1))
            .add_systems(Update, (lifecycle, player_play).chain());
        let spawn = level().spawns[0].0;
        let body = app
            .world_mut()
            .spawn((
                Player { id: 1 },
                Walker {
                    pos: spawn,
                    yaw: 0.0,
                    grounded: true,
                },
            ))
            .id();
        app.update();
        (app, body)
    }

    fn frames(app: &mut App, n: usize) {
        for _ in 0..n {
            app.update();
        }
    }

    #[test]
    fn walking_into_the_arena_starts_a_match_with_bots() {
        let (app, _) = arena_app();
        let m = app.world().resource::<Match>();
        let run = m.0.as_ref().unwrap_or_else(|| panic!("no match started"));
        assert_eq!(run.scores.len() as u32, tuning().bots.count + 1);
        assert_eq!(run.me.health, tuning().match_rules.player_health);
        let mut app = app;
        let bots = app.world_mut().query::<&Bot>().iter(app.world()).count();
        assert_eq!(bots as u32, tuning().bots.count);
    }

    #[test]
    fn shooting_a_bot_kills_it_and_scores() {
        let (mut app, body) = arena_app();
        let t = tuning();
        // Put a bot 8 m dead ahead of the player and aim at its chest.
        let feet = app
            .world()
            .get::<Walker>(body)
            .map_or(Vec3::ZERO, |w| w.pos);
        let bot_at = feet + Vec3::new(0.0, 0.0, -8.0);
        let mut q = app.world_mut().query::<(&mut Bot, &mut Transform)>();
        let mut first = true;
        for (_, mut tf) in q.iter_mut(app.world_mut()) {
            // The first goes in front; the rest far away.
            tf.translation = if first {
                bot_at
            } else {
                feet + Vec3::new(40.0, 0.0, 40.0)
            };
            first = false;
        }
        let eye = feet + Vec3::Y * EYE;
        let aim = (bot_at + Vec3::Y * 1.2 - eye).normalize();
        {
            let mut view = app.world_mut().resource_mut::<FpsView>();
            view.yaw = (-aim.x).atan2(-aim.z);
            view.pitch = aim.y.asin();
        }
        // Wait out the draw, then hold the trigger.
        frames(&mut app, (t.view.draw_secs * 60.0) as usize + 5);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        frames(&mut app, 90);
        let dead = app
            .world_mut()
            .query::<&Bot>()
            .iter(app.world())
            .filter(|b| b.dying.is_some())
            .count();
        assert!(dead >= 1, "a bot should be down");
        let run = app.world().resource::<Match>().0.as_ref();
        assert!(
            run.is_some_and(|r| r.scores[0].kills >= 1),
            "and you credited"
        );
        assert!(
            run.is_some_and(|r| r.me.mag < t.gun.mag),
            "the magazine went down"
        );
    }

    #[test]
    fn the_kill_limit_ends_the_match() {
        let (mut app, _) = arena_app();
        {
            let mut m = app.world_mut().resource_mut::<Match>();
            if let Some(run) = m.0.as_mut() {
                run.scores[0].kills = tuning().match_rules.kill_limit;
            }
        }
        frames(&mut app, 3);
        let m = app.world().resource::<Match>();
        assert_eq!(m.0.as_ref().map(|r| r.outcome), Some(Outcome::Won(0)));
    }

    #[test]
    fn reloading_refills_the_magazine() {
        let (mut app, _) = arena_app();
        let t = tuning();
        {
            let mut m = app.world_mut().resource_mut::<Match>();
            if let Some(run) = m.0.as_mut() {
                run.me.mag = 5;
            }
        }
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyR);
        frames(&mut app, 2);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .release(KeyCode::KeyR);
        frames(&mut app, (t.gun.reload_secs * 60.0) as usize + 10);
        let run = app.world().resource::<Match>().0.as_ref();
        assert_eq!(run.map(|r| r.me.mag), Some(t.gun.mag));
        assert_eq!(
            run.map(|r| r.me.reserve),
            Some(t.gun.reserve - (t.gun.mag - 5))
        );
    }
}
