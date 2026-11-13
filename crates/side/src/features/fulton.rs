//! Fulton St station. From the user: "create a fulton street station wfc
//! station u can enter in nyc. it is an impossible number of levels with
//! stairs between platforms. you try to find the J train towards brooklyn.
//! the signage directs you in a specific path, but you dont reach it for a
//! very long time. when you do reach it you get a reward."
//!
//! A subway entrance on the city's sidewalk ([`Tuning::entrance`]): walk into
//! it and the server takes you down to the station's top level. The station
//! is a stack of levels ([`Tuning::levels`], 40 by default), each a small grid
//! of 6 m cells filled by seeded wave function collapse from a modeler tile
//! kit (`fs-tile-*`, listed in `data/side/fulton_tiles.ron`), the way the
//! warehouse's floor is. Every level has one stairway down, and a level's
//! stairs land on the next level at the same spot. Hanging "J Brooklyn" signs
//! mark the one way through: across the level by the shortest way (at least
//! [`Tuning::min_path`] cells), down the stairs, again, forty times. The last
//! level has the J platform instead of stairs. The colliders come from the
//! plan, not the models, so they never wait on a file.
//!
//! The reward (server): reaching the J platform raises your Pelf (the stats'
//! wealth, `stats.rs`) by [`Tuning::reward_pelf`], once per player per game.
//! A post on the platform rides the J back up to the street, and one on the
//! top level climbs back out.
//!
//! The layout is pure ([`generate`]) and seeded by the data file, so every peer
//! builds the same station.

use std::collections::HashSet;

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_wfc as wfc;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::{WalkInput, Walker};
use crate::features::interact::{Clickable, Clicked, SERVER_REACH, deny, within};
use crate::features::stats::Stats;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::rules::nyc::{CITY_Y, STATION_LEVEL_H, STATION_TOP_Y, in_station};

/// Metres per cell (the tile kit's size).
pub const CELL: f32 = 6.0;
/// Floor to ceiling inside a level.
const ROOM_H: f32 = 4.0;

/// Tuning and writing: `assets/data/side/fulton.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The subway entrance on the sidewalk (x, z).
    pub entrance: (f32, f32),
    /// Which way the entrance's open end faces, degrees about +y.
    pub entrance_yaw: f32,
    /// How close (m) to the entrance's middle takes you down.
    pub entrance_reach: f32,
    /// The middle of the station's grid under the city (x, z).
    pub station_at: (f32, f32),
    /// How many levels deep it goes (the J is on the last).
    pub levels: usize,
    /// Cells across (x) on every level.
    pub width: usize,
    /// Cells deep (z) on every level.
    pub depth: usize,
    /// The signed way across one level is at least this many cells long.
    pub min_path: u32,
    /// The layout's seed.
    pub seed: u32,
    /// Pelf for reaching the J.
    pub reward_pelf: u8,
    /// Toast on entering.
    pub enter_line: String,
    /// Toast on reaching a new level (`{n}`, `{of}`).
    pub level_line: String,
    /// Toast on reaching the J (`{pelf}`).
    pub reward_line: String,
    /// Toast on reaching the J again.
    pub again_line: String,
    /// The way-out post's label on the top level.
    pub exit_label: String,
    /// The J's post label.
    pub ride_label: String,
    /// Andrew's calls and the one he sends (`fulton_hunt.rs`).
    pub hunt: Hunt,
    /// Down here every sound echoes: this many times the open air's reverb send...
    pub echo_wet: f32,
    /// ...ringing this many seconds (RT60), and no music.
    pub echo_secs: f32,
}

/// Andrew's calls and the thing he sends after you (`fulton_hunt.rs`).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Hunt {
    /// Andrew calls when you first reach each of these levels (0 = the top).
    pub call_levels: Vec<usize>,
    /// The phone rings this long before it answers itself.
    pub ring_secs: f32,
    /// Seconds between lines of a call.
    pub line_gap: f32,
    /// Andrew's voice per call: pitch (Hz) and speed, angrier each time.
    pub voice: Vec<(f32, f32)>,
    /// Seconds after the last call before the thing he sent arrives.
    pub spawn_delay: f32,
    /// Its speed wandering and searching, and chasing (m/s; you walk 5, run 10.5).
    pub walk_speed: f32,
    pub chase_speed: f32,
    /// What it sees: how far (m) and how wide a cone (degrees).
    pub view_range: f32,
    pub view_degrees: f32,
    /// It gets you this close (m).
    pub catch_range: f32,
    /// The director: every so often it hears roughly where you are and
    /// wanders to a cell within this many cells of you.
    pub director_secs: f32,
    pub director_radius: u32,
    /// Lose sight of you and it keeps coming to where it saw you for this long.
    pub lose_secs: f32,
    /// Then it searches hiding spots within this many cells for this long...
    pub search_radius: u32,
    pub search_secs: f32,
    /// ...and opens a locker you're in with this chance.
    pub find_chance: f32,
    /// Then it backs off at least this many cells away for this long.
    pub retreat_cells: u32,
    pub retreat_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/fulton.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// A side of a tile piece, as the tile set names it.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    N,
    E,
    S,
    W,
}

impl Side {
    fn wfc(self) -> wfc::Side {
        match self {
            Side::N => wfc::Side::North,
            Side::E => wfc::Side::East,
            Side::S => wfc::Side::South,
            Side::W => wfc::Side::West,
        }
    }
}

/// One corridor piece of the kit.
#[derive(Deserialize, Clone, Debug)]
pub struct Piece {
    /// Under `assets/models/features/`.
    pub model: String,
    pub open: Vec<Side>,
    pub weight: f32,
    /// A hiding spot in it (the lockers), at [`HIDE_AT`] in the piece's own frame.
    #[serde(default)]
    pub hide: bool,
}

/// Where a hiding piece's hiding spot is, in its own frame (inside the middle locker).
pub const HIDE_AT: Vec3 = Vec3::new(0.0, 0.0, 2.48);

/// The tile kit (`assets/data/side/fulton_tiles.ron`).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct TileSet {
    pub pieces: Vec<Piece>,
    /// Stairs down (entered from N, lands facing S one level down).
    pub stairs: String,
    /// The J platform (entered from N).
    pub platform: String,
    /// Sign: arrow right from the front (left from the back).
    pub sign_turn: String,
    /// Sign: straight on.
    pub sign_ahead: String,
    /// Sign: down the stairs.
    pub sign_down: String,
}

pub static TILES: friendslop_data::Data<TileSet> =
    friendslop_data::Data::ron("data/side/fulton_tiles.ron");

// ------------------------------------------------------------------ layout

fn bit(s: wfc::Side) -> u8 {
    1 << s.index()
}

/// A mask's open sides after `turns` quarter turns clockwise (N goes E).
pub fn turned(mask: u8, turns: u8) -> u8 {
    (0..turns % 4).fold(mask & 15, |m, _| ((m << 1) | (m >> 3)) & 15)
}

/// What fills a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A corridor piece (index into the tile set's pieces), or `None` in the fallback hall.
    Corridor(Option<u8>),
    /// Where the stairs from the level above land (drawn by the stairs above).
    Landing,
    /// The stairs down.
    Stairs,
    /// The J.
    Platform,
}

/// One filled cell: its open sides, what it is and how far it's turned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fill {
    pub mask: u8,
    pub kind: Kind,
    /// Quarter turns clockwise (seen from above). Stairs and the platform are
    /// entered from their turned N; a landing opens to its turned S.
    pub turns: u8,
}

impl Fill {
    pub fn rotation(self) -> Quat {
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2 * self.turns as f32)
    }
}

/// One level of the station.
#[derive(Clone, Debug)]
pub struct Level {
    pub width: usize,
    pub depth: usize,
    /// Row-major (`z * width + x`).
    pub cells: Vec<Fill>,
    /// Where you arrive (the landing, or the start on the top level).
    pub arrive: wfc::Cell,
    /// The stairs down (or the J).
    pub goal: wfc::Cell,
    /// The side of the goal you walk in from.
    pub goal_entry: wfc::Side,
    /// The signed way: `arrive` to `goal`, both included.
    pub path: Vec<wfc::Cell>,
}

impl Level {
    pub fn fill(&self, c: wfc::Cell) -> Fill {
        self.cells[c.1 * self.width + c.0]
    }

    /// Can you walk from `c` across `s`? (Both cells open on the shared side.)
    pub fn open(&self, c: wfc::Cell, s: wfc::Side) -> bool {
        wfc::neighbour(self.width, self.depth, c, s).is_some_and(|n| {
            self.fill(c).mask & bit(s) != 0 && self.fill(n).mask & bit(s.opposite()) != 0
        })
    }

    /// Steps from `arrive` to every cell.
    pub fn distances(&self) -> Vec<Option<u32>> {
        wfc::distances(self.width, self.depth, self.arrive, |c, s| self.open(c, s))
    }
}

/// A WFC tile: its mask and fill.
type Var = (u8, Fill, f32);

fn variants(set: &TileSet) -> Vec<Var> {
    let mut out = Vec::new();
    for (i, p) in set.pieces.iter().enumerate() {
        let mask = p.open.iter().fold(0, |m, s| m | bit(s.wfc()));
        let mut seen = Vec::new();
        for turns in 0..4u8 {
            let m = turned(mask, turns);
            if seen.contains(&m) {
                continue;
            }
            seen.push(m);
            let kind = Kind::Corridor(Some(i as u8));
            out.push((
                m,
                Fill {
                    mask: m,
                    kind,
                    turns,
                },
                p.weight.max(0.01),
            ));
        }
    }
    out
}

/// The special pieces' tile: `kind` entered (or for a landing, left) by `side`.
fn special(kind: Kind, side: wfc::Side) -> Var {
    let turns = match kind {
        // A landing's opening is the stairs' S: two turns from its entry.
        Kind::Landing => (side.index() as u8 + 2) % 4,
        _ => side.index() as u8,
    };
    (
        bit(side),
        Fill {
            mask: bit(side),
            kind,
            turns,
        },
        1.0,
    )
}

/// The sides a goal at `c` may be entered from: the way in must be on the
/// grid, and (stairs) so must the way out of the landing below.
fn entries(w: usize, h: usize, c: wfc::Cell, last: bool) -> Vec<wfc::Side> {
    wfc::Side::ALL
        .into_iter()
        .filter(|&s| {
            wfc::neighbour(w, h, c, s).is_some()
                && (last || wfc::neighbour(w, h, c, s.opposite()).is_some())
        })
        .collect()
}

fn path_to(level: &Level, d: &[Option<u32>]) -> Vec<wfc::Cell> {
    let w = level.width;
    let mut path = vec![level.goal];
    let mut at = level.goal;
    while at != level.arrive {
        let Some(here) = d[at.1 * w + at.0] else {
            return Vec::new();
        };
        let back = wfc::Side::ALL.into_iter().find_map(|s| {
            let n = wfc::neighbour(w, level.depth, at, s)?;
            (level.open(n, s.opposite()) && d[n.1 * w + n.0] == Some(here.wrapping_sub(1)))
                .then_some(n)
        });
        let Some(n) = back else {
            return Vec::new();
        };
        path.push(n);
        at = n;
    }
    path.reverse();
    path
}

/// One level: WFC with the arrival and the goal pinned, rolled again until the
/// way between them is long enough.
fn plan_level(
    rng: &mut wfc::Rng,
    (w, h): (usize, usize),
    arrive: wfc::Cell,
    landing: Option<wfc::Side>,
    last: bool,
    min_path: u32,
    vars: &[Var],
) -> Level {
    let goal_kind = if last { Kind::Platform } else { Kind::Stairs };
    let corridors: Vec<usize> = (0..vars.len()).collect();
    let open_corridors: Vec<usize> = corridors
        .iter()
        .copied()
        .filter(|&i| vars[i].0 != 0)
        .collect();
    for attempt in 0..80 {
        let goal = (rng.below(w), rng.below(h));
        if goal == arrive {
            continue;
        }
        let sides = entries(w, h, goal, last);
        if sides.is_empty() {
            continue;
        }
        let entry = sides[rng.below(sides.len())];
        // Ask for the full length first, then settle for less.
        let need = if attempt < 60 { min_path } else { 1 };
        let manhattan = arrive.0.abs_diff(goal.0) + arrive.1.abs_diff(goal.1);
        if attempt < 60 && (manhattan as u32) * 2 < need {
            continue;
        }
        let mut tiles: Vec<Var> = vars.to_vec();
        let arrive_tile = landing.map(|s| {
            tiles.push(special(Kind::Landing, s));
            tiles.len() - 1
        });
        tiles.push(special(goal_kind, entry));
        let goal_tile = tiles.len() - 1;
        let mut problem = wfc::Wfc::new(
            w,
            h,
            tiles
                .iter()
                .map(|&(m, _, weight)| wfc::Tile {
                    sockets: wfc::Side::ALL.map(|s| m & bit(s) != 0),
                    weight,
                })
                .collect(),
        );
        for z in 0..h {
            for x in 0..w {
                let c = (x, z);
                let allowed = if c == goal {
                    vec![goal_tile]
                } else if c == arrive {
                    arrive_tile.map_or_else(|| open_corridors.clone(), |t| vec![t])
                } else {
                    corridors.clone()
                };
                problem = problem.pin(c, allowed);
            }
        }
        let build = |s: &wfc::Solution| Level {
            width: w,
            depth: h,
            cells: s.tiles.iter().map(|&t| tiles[t].1).collect(),
            arrive,
            goal,
            goal_entry: entry,
            path: Vec::new(),
        };
        let solved = problem.solve_until(
            rng,
            6,
            |a, b| a == b,
            |_, _, open| !*open,
            |s| {
                let l = build(s);
                l.distances()[goal.1 * w + goal.0].is_some_and(|d| d >= need)
            },
        );
        if let Some(s) = solved {
            let mut level = build(&s);
            let d = level.distances();
            level.path = path_to(&level, &d);
            return level;
        }
    }
    fallback(w, h, arrive, landing, last)
}

/// One open hall (never needed in practice; keeps generation total).
fn fallback(
    w: usize,
    h: usize,
    arrive: wfc::Cell,
    landing: Option<wfc::Side>,
    last: bool,
) -> Level {
    // The goal: the farthest cell with a way in that isn't the arrival, and
    // that the arrival's landing doesn't open straight onto.
    let exit_to = landing.and_then(|l| wfc::neighbour(w, h, arrive, l));
    let (goal, entry) = (0..w * h)
        .map(|i| (i % w, i / w))
        .filter(|&c| c != arrive && Some(c) != exit_to)
        .filter_map(|c| {
            let s = entries(w, h, c, last)
                .into_iter()
                .find(|&s| wfc::neighbour(w, h, c, s) != Some(arrive))?;
            Some((c, s))
        })
        .max_by_key(|(c, _)| c.0.abs_diff(arrive.0) + c.1.abs_diff(arrive.1))
        .unwrap_or(((0, 0), wfc::Side::South));
    let allow = |c: wfc::Cell, s: wfc::Side| {
        wfc::neighbour(w, h, c, s).is_some()
            && if c == goal {
                s == entry
            } else if c == arrive {
                landing.is_none_or(|l| s == l)
            } else {
                true
            }
    };
    let mut cells = Vec::with_capacity(w * h);
    for z in 0..h {
        for x in 0..w {
            let c = (x, z);
            let mut mask = 0;
            for s in wfc::Side::ALL {
                if let Some(n) = wfc::neighbour(w, h, c, s)
                    && allow(c, s)
                    && allow(n, s.opposite())
                {
                    mask |= bit(s);
                }
            }
            let fill = if c == goal {
                special(if last { Kind::Platform } else { Kind::Stairs }, entry).1
            } else if c == arrive
                && let Some(l) = landing
            {
                special(Kind::Landing, l).1
            } else {
                Fill {
                    mask,
                    kind: Kind::Corridor(None),
                    turns: 0,
                }
            };
            cells.push(fill);
        }
    }
    let mut level = Level {
        width: w,
        depth: h,
        cells,
        arrive,
        goal,
        goal_entry: entry,
        path: Vec::new(),
    };
    let d = level.distances();
    level.path = path_to(&level, &d);
    level
}

/// The whole station: every level, top first. Pure and seeded.
pub fn generate(t: &Tuning, set: &TileSet) -> Vec<Level> {
    let (w, h) = (t.width.max(3), t.depth.max(3));
    let mut rng = wfc::Rng::new(t.seed);
    let vars = variants(set);
    let mut levels = Vec::with_capacity(t.levels);
    let mut arrive = (w / 2, 0);
    let mut landing = None;
    for l in 0..t.levels.max(1) {
        let last = l + 1 == t.levels.max(1);
        let level = if vars.is_empty() {
            fallback(w, h, arrive, landing, last)
        } else {
            plan_level(&mut rng, (w, h), arrive, landing, last, t.min_path, &vars)
        };
        arrive = level.goal;
        landing = Some(level.goal_entry.opposite());
        levels.push(level);
    }
    levels
}

/// The middle of a cell's floor in the world.
pub fn cell_world(t: &Tuning, level: usize, c: wfc::Cell) -> Vec3 {
    let (w, h) = (t.width.max(3), t.depth.max(3));
    Vec3::new(
        t.station_at.0 + (c.0 as f32 - (w as f32 - 1.0) / 2.0) * CELL,
        level_floor(level),
        t.station_at.1 + (c.1 as f32 - (h as f32 - 1.0) / 2.0) * CELL,
    )
}

/// A level's floor height.
pub fn level_floor(level: usize) -> f32 {
    STATION_TOP_Y - level as f32 * STATION_LEVEL_H
}

/// The level a body with feet at `y` is on.
pub fn level_at(y: f32) -> usize {
    ((STATION_TOP_Y - y) / STATION_LEVEL_H).round().max(0.0) as usize
}

/// A unit step across a side, in the world (x, z).
fn dir(s: wfc::Side) -> Vec3 {
    let (dx, dz) = s.offset();
    Vec3::new(dx as f32, 0.0, dz as f32)
}

/// One static collider box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Block {
    pub centre: Vec3,
    pub size: Vec3,
    pub rotation: Quat,
}

/// The colliders of one level, from its plan.
pub fn colliders(t: &Tuning, li: usize, level: &Level) -> Vec<Block> {
    let mut out = Vec::new();
    let half = CELL / 2.0;
    for z in 0..level.depth {
        for x in 0..level.width {
            let c = (x, z);
            let f = level.fill(c);
            let at = cell_world(t, li, c);
            let r = f.rotation();
            let flat = |centre: Vec3, size: Vec3| Block {
                centre,
                size,
                rotation: Quat::IDENTITY,
            };
            if f.mask == 0 {
                out.push(flat(
                    at + Vec3::Y * (ROOM_H / 2.0 - 0.1),
                    Vec3::new(CELL, ROOM_H + 0.2, CELL),
                ));
                continue;
            }
            if f.kind != Kind::Stairs {
                out.push(flat(at - Vec3::Y * 0.1, Vec3::new(CELL, 0.2, CELL)));
            }
            for s in wfc::Side::ALL {
                if f.mask & bit(s) != 0 {
                    continue;
                }
                let d = dir(s);
                let size = if d.x != 0.0 {
                    Vec3::new(0.2, ROOM_H, CELL)
                } else {
                    Vec3::new(CELL, ROOM_H, 0.2)
                };
                out.push(flat(at + d * (half - 0.1) + Vec3::Y * (ROOM_H / 2.0), size));
            }
            match f.kind {
                Kind::Stairs => {
                    // A ramp from the entry edge (this floor) down to the far
                    // edge one level below; in the piece's own frame it runs
                    // from -z (high) to +z (low).
                    let drop = STATION_LEVEL_H;
                    let tilt = (drop / CELL).atan();
                    let ramp = Quat::from_rotation_x(tilt);
                    let normal = ramp * Vec3::Y;
                    let local = Vec3::new(0.0, -drop / 2.0, 0.0) - normal * 0.15;
                    out.push(Block {
                        centre: at + r * local,
                        size: Vec3::new(CELL - 0.4, 0.3, (CELL * CELL + drop * drop).sqrt()),
                        rotation: r * ramp,
                    });
                }
                Kind::Platform => {
                    // The track and the waiting train: keep off.
                    out.push(Block {
                        centre: at + r * Vec3::new(0.0, ROOM_H / 2.0 - 1.2, 1.775),
                        size: Vec3::new(CELL, ROOM_H + 2.4, 2.45),
                        rotation: r,
                    });
                }
                _ => {}
            }
        }
    }
    out
}

/// Which sign hangs where.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignKind {
    Right,
    Left,
    Ahead,
    Down,
}

/// A sign on the way: where it hangs (the cell floor below it), which way
/// its front faces (yaw) and what it says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sign {
    pub at: Vec3,
    pub yaw: f32,
    pub kind: SignKind,
}

/// The signs along a level's way: one in every cell you walk through (none
/// over the landing, where the stairs from above are the ceiling), facing
/// you as you come, pointing on; and "down" at the top of the stairs.
pub fn signs(t: &Tuning, li: usize, level: &Level) -> Vec<Sign> {
    let mut out = Vec::new();
    let p = &level.path;
    let landing = level.fill(level.arrive).kind == Kind::Landing;
    for (i, &c) in p.iter().enumerate() {
        if i == 0 && landing {
            continue;
        }
        let at = cell_world(t, li, c);
        let step = |a: wfc::Cell, b: wfc::Cell| {
            Vec3::new(b.0 as f32 - a.0 as f32, 0.0, b.1 as f32 - a.1 as f32)
        };
        let next = p.get(i + 1).map(|&n| step(c, n));
        let came = if i > 0 { Some(step(p[i - 1], c)) } else { next };
        let Some(came) = came else { continue };
        // Its front faces back along the way you came.
        let yaw = (-came.x).atan2(-came.z);
        if c == level.goal {
            if level.fill(c).kind == Kind::Stairs {
                let entry = dir(level.goal_entry);
                out.push(Sign {
                    at: at + entry * (CELL / 2.0 - 1.0),
                    yaw,
                    kind: SignKind::Down,
                });
            }
            continue;
        }
        let Some(next) = next else { continue };
        // Your right, walking along `came`.
        let right = Vec3::new(-came.z, 0.0, came.x);
        let kind = if next.dot(came) > 0.5 {
            SignKind::Ahead
        } else if next.dot(right) > 0.5 {
            SignKind::Right
        } else {
            SignKind::Left
        };
        let yaw = if kind == SignKind::Left {
            yaw + std::f32::consts::PI
        } else {
            yaw
        };
        out.push(Sign { at, yaw, kind });
    }
    out
}

/// Where you arrive at the top of the station.
pub fn start_at(t: &Tuning, levels: &[Level]) -> Vec3 {
    levels
        .first()
        .map_or(Vec3::ZERO, |l| cell_world(t, 0, l.arrive))
}

/// The middle of the J platform (on the platform side of its cell).
pub fn platform_at(t: &Tuning, levels: &[Level]) -> Option<Vec3> {
    let li = levels.len().checked_sub(1)?;
    let l = &levels[li];
    let f = l.fill(l.goal);
    (f.kind == Kind::Platform)
        .then(|| cell_world(t, li, l.goal) + f.rotation() * Vec3::new(0.0, 0.0, -1.2))
}

/// Is a body with feet at `feet` on the J platform?
pub fn on_platform(t: &Tuning, levels: &[Level], feet: Vec3) -> bool {
    platform_at(t, levels)
        .is_some_and(|p| (feet.y - p.y).abs() < 1.5 && feet.xz().distance(p.xz()) < CELL / 2.0)
}

fn entrance_dir(t: &Tuning) -> Vec3 {
    let a = t.entrance_yaw.to_radians();
    Vec3::new(a.sin(), 0.0, a.cos())
}

/// The entrance's middle on the street.
pub fn entrance_at(t: &Tuning) -> Vec3 {
    Vec3::new(t.entrance.0, CITY_Y, t.entrance.1)
}

/// Where you come back up: on the sidewalk in front of the entrance.
pub fn street_at(t: &Tuning) -> Vec3 {
    entrance_at(t) + entrance_dir(t) * 4.5
}

/// Does a body with feet at `feet` walk into the entrance?
pub fn entering(t: &Tuning, feet: Vec3) -> bool {
    (feet.y - CITY_Y).abs() < 1.5 && feet.xz().distance(entrance_at(t).xz()) < t.entrance_reach
}

/// Your Pelf after the reward (stats stay within 1..=10).
pub fn rewarded_pelf(pelf: u8, reward: u8) -> u8 {
    pelf.saturating_add(reward).min(10)
}

// ------------------------------------------------------------------ server

/// The station everyone builds from the data files.
#[derive(Resource)]
pub struct Station {
    pub levels: Vec<Level>,
}

/// Server: players already rewarded this game.
#[derive(Resource, Default)]
struct Rewarded(HashSet<u64>);

/// Client → server: leave the station (the way-out post, or ride the J).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct LeaveStation {
    pub ride: bool,
}

pub fn protocol(app: &mut App) {
    app.add_client_event::<LeaveStation>(Channel::Ordered)
        .add_observer(leave)
        .init_resource::<Rewarded>()
        .add_systems(
            Update,
            (go_down, reward)
                .run_if(in_state(ServerState::Running))
                .run_if(resource_exists::<Station>),
        );
}

type Body<'a> = (
    &'a PlayerOwner,
    &'a mut Walker,
    Option<&'a mut WalkInput>,
    Option<&'a mut crate::kcc::Kcc>,
);

fn put(
    to: Vec3,
    walker: &mut Walker,
    input: Option<Mut<WalkInput>>,
    kcc: Option<Mut<crate::kcc::Kcc>>,
) {
    walker.pos = to + Vec3::Y * 0.05;
    if let Some(mut input) = input {
        input.stick = Vec2::ZERO;
        input.jump = false;
    }
    if let Some(mut kcc) = kcc {
        kcc.velocity = Vec3::ZERO;
    }
}

/// Server: walk into the entrance and you're on the station's top level.
fn go_down(station: Res<Station>, mut bodies: Query<Body>) {
    let t = tuning();
    for (_, mut walker, input, kcc) in &mut bodies {
        if entering(&t, walker.pos) {
            put(start_at(&t, &station.levels), &mut walker, input, kcc);
        }
    }
}

/// Server: reach the J and your Pelf goes up, once a game.
fn reward(
    station: Res<Station>,
    mut done: ResMut<Rewarded>,
    mut players: Query<(&Player, &Walker, &mut Stats)>,
) {
    let t = tuning();
    for (p, w, mut stats) in &mut players {
        if on_platform(&t, &station.levels, w.pos) && done.0.insert(p.id) {
            stats.pelf = rewarded_pelf(stats.pelf, t.reward_pelf);
        }
    }
}

fn leave(
    ev: On<FromClient<LeaveStation>>,
    mut commands: Commands,
    station: Option<Res<Station>>,
    mut bodies: Query<Body>,
) {
    let Some(station) = station else { return };
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((_, mut walker, input, kcc)) = bodies.iter_mut().find(|b| b.0.0 == owner) else {
        return;
    };
    let t = tuning();
    let post = if ev.ride {
        platform_at(&t, &station.levels)
    } else {
        Some(start_at(&t, &station.levels))
    };
    if !post.is_some_and(|p| within(walker.pos, p, SERVER_REACH + 3.0)) {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    put(street_at(&t), &mut walker, input, kcc);
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_observer(click)
        .add_systems(OnEnter(AppState::InGame), build_station)
        .add_systems(
            Update,
            (stream_levels, follow_light, toasts, echo)
                .run_if(in_state(AppState::InGame))
                .run_if(resource_exists::<Station>),
        );
}

#[derive(Component)]
struct Post {
    ride: bool,
}

fn click(ev: On<Clicked>, mut commands: Commands, posts: Query<&Post>) {
    if let Ok(p) = posts.get(ev.entity) {
        commands.client_trigger(LeaveStation { ride: p.ride });
    }
}

/// The models of one level, spawned while the camera is near it.
#[derive(Component)]
struct LevelModels(usize);

#[derive(Component)]
struct StationLight;

fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

fn build_station(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    assets: Res<AssetServer>,
) {
    let t = tuning();
    let levels = generate(&t, &TILES.get());
    // Colliders never wait on a model: boxes from the plan, every level.
    for (li, level) in levels.iter().enumerate() {
        for b in colliders(&t, li, level) {
            commands.spawn((
                Transform::from_translation(b.centre).with_rotation(b.rotation),
                avian3d::prelude::RigidBody::Static,
                avian3d::prelude::Collider::cuboid(b.size.x, b.size.y, b.size.z),
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    // The entrance on the sidewalk: the city's subway entrance model, with
    // its low walls (offsets inside the model) as colliders.
    let at = entrance_at(&t);
    let turn = Quat::from_rotation_y(t.entrance_yaw.to_radians());
    commands.spawn((
        WorldAssetRoot(assets.load(
            GltfAssetLabel::Scene(0).from_asset("models/prefabs/nyc-kit-subway-entrance.gltf"),
        )),
        Transform::from_translation(at).with_rotation(turn),
        DespawnOnExit(AppState::InGame),
    ));
    for (offset, size) in [
        (Vec3::new(1.2, 0.65, 0.0), Vec3::new(0.25, 1.3, 6.2)),
        (Vec3::new(-1.2, 0.65, 0.0), Vec3::new(0.25, 1.3, 6.2)),
        (Vec3::new(0.0, 0.65, -3.1), Vec3::new(2.6, 1.3, 0.25)),
    ] {
        commands.spawn((
            Transform::from_translation(at + turn * offset).with_rotation(turn),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(size.x, size.y, size.z),
            DespawnOnExit(AppState::InGame),
        ));
    }
    // The way-out posts: top level, and on the J platform.
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let pole = materials.add(Color::srgb(0.2, 0.3, 0.22));
    let sign = materials.add(Color::srgb(0.6, 0.4, 0.2));
    let posts = [
        (start_at(&t, &levels), false, t.exit_label.clone()),
        (
            platform_at(&t, &levels).unwrap_or_else(|| start_at(&t, &levels)),
            true,
            t.ride_label.clone(),
        ),
    ];
    for (at, ride, label) in posts {
        commands
            .spawn((
                Post { ride },
                Transform::from_translation(at + Vec3::new(1.8, 0.0, 0.0)),
                Visibility::default(),
                // Labels are read once a game from the data file.
                Clickable::at_height(1.6)
                    .radius(60.0)
                    .labelled(label.leak()),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(pole.clone()),
                    Transform::from_xyz(0.0, 1.1, 0.0).with_scale(Vec3::new(0.12, 2.2, 0.12)),
                ));
                p.spawn((
                    Mesh3d(cube.clone()),
                    MeshMaterial3d(sign.clone()),
                    Transform::from_xyz(0.0, 1.9, 0.0).with_scale(Vec3::new(1.2, 0.35, 0.06)),
                ));
            });
    }
    commands.spawn((
        StationLight,
        PointLight {
            color: Color::srgb(0.92, 0.96, 1.0),
            intensity: 600_000.0,
            range: 26.0,
            ..default()
        },
        Transform::default(),
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
    commands.insert_resource(Station { levels });
}

fn camera_at(camera: &Query<&GlobalTransform, With<crate::world::MainCamera>>) -> Option<Vec3> {
    camera.iter().next().map(|g| g.translation())
}

/// Local: draw only the levels around the camera (forty levels of tiles is a lot).
fn stream_levels(
    mut commands: Commands,
    station: Res<Station>,
    assets: Res<AssetServer>,
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    spawned: Query<(Entity, &LevelModels)>,
) {
    let t = tuning();
    let want: Vec<usize> = match camera_at(&camera) {
        Some(at) if in_station(at - Vec3::Y * 1.6) => {
            let l = level_at(at.y - 1.6);
            (l.saturating_sub(1)..=l + 1)
                .filter(|&l| l < station.levels.len())
                .collect()
        }
        _ => Vec::new(),
    };
    let mut have = Vec::new();
    for (e, m) in &spawned {
        if want.contains(&m.0) {
            have.push(m.0);
        } else {
            commands.entity(e).despawn();
        }
    }
    let set = TILES.get();
    for &li in want.iter().filter(|l| !have.contains(l)) {
        let level = &station.levels[li];
        let root = commands
            .spawn((
                LevelModels(li),
                Transform::default(),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .id();
        for z in 0..level.depth {
            for x in 0..level.width {
                let f = level.fill((x, z));
                let file = match f.kind {
                    Kind::Corridor(Some(i)) => set.pieces.get(i as usize).map(|p| p.model.as_str()),
                    Kind::Stairs => Some(set.stairs.as_str()),
                    Kind::Platform => Some(set.platform.as_str()),
                    Kind::Corridor(None) | Kind::Landing => None,
                };
                let Some(file) = file else { continue };
                commands.spawn((
                    model(&assets, file),
                    Transform::from_translation(cell_world(&t, li, (x, z)))
                        .with_rotation(f.rotation()),
                    ChildOf(root),
                ));
            }
        }
        for s in signs(&t, li, level) {
            let file = match s.kind {
                SignKind::Right | SignKind::Left => &set.sign_turn,
                SignKind::Ahead => &set.sign_ahead,
                SignKind::Down => &set.sign_down,
            };
            commands.spawn((
                model(&assets, file),
                Transform::from_translation(s.at).with_rotation(Quat::from_rotation_y(s.yaw)),
                ChildOf(root),
            ));
        }
    }
}

/// Local: a light that goes with you down there (the tubes are only paint).
fn follow_light(
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    mut light: Query<(&mut Transform, &mut Visibility), With<StationLight>>,
) {
    let Some(at) = camera_at(&camera) else { return };
    let here = in_station(at - Vec3::Y * 1.6);
    for (mut tf, mut vis) in &mut light {
        tf.translation = at + Vec3::Y * 0.8;
        vis.set_if_neq(if here {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// Local: the station's tiled halls ring (every world sound gets a long
/// tail) and the music stops (the `fulton` signal hushes it).
fn echo(
    ears: Res<crate::spatial::Ears>,
    mut room: ResMut<crate::spatial::Room>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let t = tuning();
    let inside = in_station(ears.pos - Vec3::Y * 1.6);
    if let Some(mut signals) = signals {
        signals.flag("fulton", inside);
    }
    let want = if inside {
        crate::spatial::Room {
            wet: t.echo_wet,
            rt60: t.echo_secs,
        }
    } else {
        crate::spatial::Room::OPEN
    };
    if *room != want {
        *room = want;
    }
}

/// Local: say where you are as you go down, and when you find the J.
fn toasts(
    station: Res<Station>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut was: Local<Option<usize>>,
    mut found: Local<(bool, bool)>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let t = tuning();
    let here = in_station(w.pos).then(|| level_at(w.pos.y));
    let on = on_platform(&t, &station.levels, w.pos);
    if on && !found.1 {
        toast.show(if found.0 {
            t.again_line.clone()
        } else {
            t.reward_line.replace("{pelf}", &t.reward_pelf.to_string())
        });
        found.0 = true;
    } else if here != *was
        && let Some(l) = here
    {
        if was.is_none() {
            toast.show(t.enter_line.clone());
        } else if l > was.unwrap_or(0) {
            toast.show(
                t.level_line
                    .replace("{n}", &(l + 1).to_string())
                    .replace("{of}", &station.levels.len().to_string()),
            );
        }
    }
    found.1 = on;
    *was = here;
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

    fn data() -> (Tuning, TileSet) {
        load(&TUNING);
        load(&TILES);
        ((*TUNING.get()).clone(), (*TILES.get()).clone())
    }

    #[test]
    fn the_data_files_parse_and_have_everything() {
        let (t, set) = data();
        assert!(t.levels >= 10, "an impossible number of levels");
        assert!(t.min_path >= 3 && t.reward_pelf > 0);
        assert!(!t.enter_line.is_empty() && t.reward_line.contains("{pelf}"));
        assert!(t.level_line.contains("{n}") && !t.exit_label.is_empty());
        assert!(!set.pieces.is_empty() && !set.stairs.is_empty() && !set.platform.is_empty());
        assert!(
            !set.sign_turn.is_empty() && !set.sign_ahead.is_empty() && !set.sign_down.is_empty()
        );
    }

    #[test]
    fn the_signs_lead_all_the_way_down_to_the_j() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        assert_eq!(levels.len(), t.levels);
        let mut total = 0;
        for (li, l) in levels.iter().enumerate() {
            let last = li + 1 == levels.len();
            // The way exists, starts at the arrival, ends at the goal, one step at a time.
            assert_eq!(l.path.first(), Some(&l.arrive), "level {li}");
            assert_eq!(l.path.last(), Some(&l.goal), "level {li}");
            for pair in l.path.windows(2) {
                let s = wfc::Side::ALL
                    .into_iter()
                    .find(|&s| wfc::neighbour(l.width, l.depth, pair[0], s) == Some(pair[1]));
                assert!(
                    s.is_some_and(|s| l.open(pair[0], s)),
                    "level {li}: {pair:?}"
                );
            }
            // ...and it's long.
            assert!(
                l.path.len() as u32 > t.min_path,
                "level {li}: {}",
                l.path.len()
            );
            total += l.path.len();
            // The goal: stairs down, or the J on the last level.
            let goal = l.fill(l.goal);
            assert_eq!(goal.kind, if last { Kind::Platform } else { Kind::Stairs });
            assert!(l.open(l.goal, l.goal_entry));
            // The stairs land on the next level, at the same spot, open the far way.
            if let Some(next) = levels.get(li + 1) {
                assert_eq!(next.arrive, l.goal);
                let landing = next.fill(next.arrive);
                assert_eq!(landing.kind, Kind::Landing);
                assert!(next.open(next.arrive, l.goal_entry.opposite()));
            }
            // A sign at every step of the way you walk (bar the landing).
            assert!(signs(&t, li, l).len() + 2 >= l.path.len());
        }
        assert!(
            total as u32 >= t.levels as u32 * t.min_path,
            "a very long time"
        );
        // The J is at the bottom.
        let Some(j) = platform_at(&t, &levels) else {
            panic!("no J platform");
        };
        assert!(on_platform(&t, &levels, j));
        assert!(!on_platform(&t, &levels, start_at(&t, &levels)));
        assert!(j.y < start_at(&t, &levels).y - 100.0);
    }

    #[test]
    fn the_same_seed_builds_the_same_station() {
        let (t, set) = data();
        let a = generate(&t, &set);
        let b = generate(&t, &set);
        assert!(
            a.iter()
                .zip(&b)
                .all(|(a, b)| a.cells == b.cells && a.path == b.path)
        );
    }

    #[test]
    fn the_fallback_hall_still_has_a_way() {
        let (t, _) = data();
        let levels = generate(&t, &TileSet::default());
        for l in &levels {
            assert_eq!(l.path.last(), Some(&l.goal));
        }
    }

    #[test]
    fn the_station_is_under_the_city_clear_of_the_street() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        for (li, l) in levels.iter().enumerate() {
            for z in 0..l.depth {
                for x in 0..l.width {
                    let p = cell_world(&t, li, (x, z));
                    assert!(in_station(p), "{p}");
                    assert_eq!(level_at(p.y), li);
                    assert_eq!(crate::rules::depths::ground_at(p.x, p.z, p.y + 0.5), p.y);
                }
            }
        }
        // The entrance is on the street, and coming back up doesn't take you down again.
        assert!(entering(&t, entrance_at(&t)));
        assert!(!entering(&t, street_at(&t)));
        assert!(crate::rules::nyc::in_city(street_at(&t).x, street_at(&t).z));
    }

    #[test]
    fn the_stairs_ramp_meets_both_floors() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let l = &levels[0];
        let ramp = colliders(&t, 0, l)
            .into_iter()
            .find(|b| b.rotation != Quat::IDENTITY && b.size.z > CELL);
        let Some(ramp) = ramp else { panic!("no ramp") };
        let top = ramp.centre + ramp.rotation * Vec3::new(0.0, 0.15, -ramp.size.z / 2.0);
        let bottom = ramp.centre + ramp.rotation * Vec3::new(0.0, 0.15, ramp.size.z / 2.0);
        assert!((top.y - level_floor(0)).abs() < 0.01, "{top}");
        assert!((bottom.y - level_floor(1)).abs() < 0.01, "{bottom}");
        // The top is at the entry edge.
        let entry = cell_world(&t, 0, l.goal) + dir(l.goal_entry) * (CELL / 2.0);
        assert!(top.xz().distance(entry.xz()) < 0.01);
    }

    #[test]
    fn the_reward_caps_at_ten() {
        assert_eq!(rewarded_pelf(4, 3), 7);
        assert_eq!(rewarded_pelf(9, 3), 10);
    }

    #[test]
    fn the_signs_point_the_way() {
        // Straight on along +x, then a turn to +z (right, walking +x).
        let l = Level {
            width: 3,
            depth: 3,
            cells: vec![
                Fill {
                    mask: 15,
                    kind: Kind::Corridor(None),
                    turns: 0
                };
                9
            ],
            arrive: (0, 0),
            goal: (2, 2),
            goal_entry: wfc::Side::North,
            path: vec![(0, 0), (1, 0), (2, 0), (2, 1), (2, 2)],
        };
        let t = Tuning {
            width: 3,
            depth: 3,
            ..default()
        };
        let kinds: Vec<_> = signs(&t, 0, &l).iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [
                SignKind::Ahead,
                SignKind::Ahead,
                SignKind::Right,
                SignKind::Ahead
            ]
        );
        // The right-turn sign faces back along -x, its arrow (+x local) pointing to +z.
        let s = signs(&t, 0, &l)[2];
        let r = Quat::from_rotation_y(s.yaw);
        assert!((r * Vec3::Z).distance(Vec3::NEG_X) < 1e-4);
        assert!((r * Vec3::X).distance(Vec3::Z) < 1e-4);
    }
}
