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
//! Subway platforms ("fulton street needs subway platforms and empty tracks
//! too"): every [`Tuning::platform_every`] levels the kit's track pieces join
//! in: a platform deck with its yellow edge, columns, benches and line signs,
//! and 1.2 m below it an empty track bed (ballast, ties, rails, the third rail
//! under its board) running between tunnel mouths, with steps down at each
//! end. Track sides only meet track sides, with the deck on the same side, so
//! a run hangs together. The bed sinks into the level below, so the cells
//! under it there are solid ([`Kind::Under`]) and the generator keeps that
//! level winding round them.
//!
//! Big rooms ("large rooms that are bigger than the grid ... so its not just
//! all hallways"): before WFC runs, a level may get a room or two several
//! cells across ([`Tuning::rooms`], placed by `friendslop_wfc::rooms`). Each
//! room cell is pinned to a tile open on its inside sides and its doors, so
//! the corridors are run up to the doors. They're drawn from the kit's
//! `room` pieces: floor, a wall on each outside side, a column wherever four
//! room cells meet.
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
use crate::features::fps::{WalkInput, Walker, teleport};
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
    /// The hole in the street the entrance's stairs go down through
    /// (see `friendslop_world::holes`), in the entrance's own frame.
    pub entrance_hole: EntranceHole,
    /// Walk this far (m) down the stairs and you're in the station.
    pub entrance_reach: f32,
    /// The middle of the station's grid under the city (x, z).
    pub station_at: (f32, f32),
    /// How many levels deep it goes (the J is on the last).
    pub levels: usize,
    /// Levels are planned and built as people go down: this many below the
    /// deepest body in the station (and the top this many when New York loads).
    #[serde(default)]
    pub plan_ahead: usize,
    /// Cells across (x) on every level.
    pub width: usize,
    /// Cells deep (z) on every level.
    pub depth: usize,
    /// The signed way across one level is at least this many cells long.
    pub min_path: u32,
    /// The layout's seed.
    pub seed: u32,
    /// Every this many levels (the 1st, the n+1th, ...; never the J's) the
    /// subway platforms come in: track pieces may fill cells and at least
    /// [`Tuning::platform_cells`] of them must. 0 turns them off.
    #[serde(default)]
    pub platform_every: usize,
    /// How many track cells a platform level has at least.
    #[serde(default)]
    pub platform_cells: usize,
    /// ...and at most (the level below has to wind round them).
    #[serde(default)]
    pub platform_max: usize,
    /// The station's lights.
    #[serde(default)]
    pub light: Lights,
    /// Big rooms several cells across, among the corridors.
    #[serde(default)]
    pub rooms: Rooms,
    /// Caved-in corridors: rubble you can't pass, or a squeeze.
    #[serde(default)]
    pub collapse: Collapse,
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

/// The entrance's hole in the street, in the entrance model's own frame
/// (+z is its open end, where the stairs start).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct EntranceHole {
    /// Its middle along the entrance (m, +z towards the open end).
    pub offset: f32,
    /// Its size across (x) and along (z).
    pub size: (f32, f32),
    /// How far down its lining goes, and how far above the street it starts.
    pub depth: f32,
    pub lip: f32,
}

/// The station's lights: one under every cell's ceiling tubes on the levels
/// being drawn, and a dim one that goes with you.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Lights {
    /// Each cell's ceiling light (lumens), how far it reaches (m) and its height.
    pub cell_lumens: f32,
    pub cell_range: f32,
    pub cell_height: f32,
    /// The light that goes with you (lumens) and its reach (m).
    pub follow_lumens: f32,
    pub follow_range: f32,
    /// The station's reflections (its baked probe, "fulton"): brightness.
    pub probe: f32,
    /// The nearest this many cell lights cast shadows (0: none; off when the
    /// video settings turn shadows off).
    pub shadow_lights: usize,
}

/// Big rooms: rectangles of cells with no walls between them, stamped on a
/// level before WFC runs the corridors up to their doors.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Rooms {
    /// The chance a level gets rooms at all.
    pub chance: f32,
    /// At most this many on one level.
    pub max: usize,
    /// Their sizes in cells (each also tried turned round).
    pub sizes: Vec<(usize, usize)>,
    /// How many doors each has, at least and at most.
    pub doors: (usize, usize),
    /// What the rooms are for (a waiting room, storage, a boiler room...):
    /// each room is one of these, picked by weight among those that fit
    /// its size. None: every room is a plain hall with columns.
    #[serde(default)]
    pub kinds: Vec<RoomKind>,
    /// The chance a room (2 by 2 or bigger) loses a corner block and is
    /// L-shaped (the corner goes back to corridor).
    #[serde(default)]
    pub l_chance: f32,
    /// Tall rooms with mezzanines.
    #[serde(default)]
    pub tall: Tall,
    /// Props for the plain corridor pieces (not the platform, stairs or
    /// lockers), placed the same way as a room kind's.
    #[serde(default)]
    pub corridors: Vec<RoomProp>,
}

/// Tall rooms: the room's floor away from its doors sinks into the level
/// below (whose cells there are solid), so its ceiling is that much higher,
/// and the cells with doors stay up as railed mezzanine decks with stairs
/// down.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tall {
    /// The chance a room is tall (when it has a cell with no door).
    pub chance: f32,
    /// How far its floor sinks (m; at most 1.9).
    pub sink: f32,
}

/// Caved-in corridors, rolled for each open side between two corridor cells.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Collapse {
    /// The chance it's blocked solid (only where there's another way round:
    /// a blocked side never cuts anything off; elsewhere it's a squeeze).
    pub blocked: f32,
    /// The chance it's a squeeze: heaps either side and a gap between.
    pub squeeze: f32,
    /// A squeeze's gap (m).
    pub gap: f32,
}

/// What a big room is for, and what's in it.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct RoomKind {
    pub name: String,
    pub weight: f32,
    /// The sizes it comes in (either way round); empty: any.
    #[serde(default)]
    pub sizes: Vec<(usize, usize)>,
    /// Columns where four of its cells meet.
    #[serde(default)]
    pub columns: bool,
    /// Its dressing.
    #[serde(default)]
    pub props: Vec<RoomProp>,
}

/// Where a room prop goes.
#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropPlace {
    /// Against a walled side of a room cell (never a door), facing in.
    #[default]
    Wall,
    /// Around the middle of a room cell (never solid: you walk over or past it).
    Middle,
    /// Hung from the ceiling somewhere within `slots[0]` m of a cell's
    /// middle (never solid: it hangs well over your head).
    Ceiling,
}

/// One kind of prop in a room.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct RoomProp {
    /// Under `assets/models/`.
    pub model: String,
    #[serde(default)]
    pub place: PropPlace,
    /// The chance of one at each slot.
    pub chance: f32,
    /// Wall: where along the wall (m from its middle) it may stand.
    /// Middle: how far off the cell's middle (m, each way at most).
    #[serde(default)]
    pub slots: Vec<f32>,
    /// Raised this far (m) so it stands on the floor.
    #[serde(default)]
    pub lift: f32,
    /// Its collider (across the wall, up, out from it; m). Zero: none.
    #[serde(default)]
    pub size: (f32, f32, f32),
    /// Scaled this much (0 counts as 1).
    #[serde(default)]
    pub scale: f32,
    /// Wall: modelled with its back on the wall at z = 0 (posters, pipes,
    /// panels), so it goes right on the wall's face instead of standing off it.
    #[serde(default)]
    pub flush: bool,
    /// Turned this much more about up (degrees) where it stands.
    #[serde(default)]
    pub yaw: f32,
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
    /// The open sides that carry the subway's track on (a platform with its
    /// track bed): they only meet other track sides. The platform runs along
    /// these, its deck on the piece's N half, the track bed on its S half.
    #[serde(default)]
    pub track: Vec<Side>,
}

/// How far the track bed lies below the platform (m).
pub const TRACK_DROP: f32 = 1.2;
/// The platform's edge (its own frame, +z): the deck is N of it, the bed S.
pub const PLATFORM_EDGE_Z: f32 = 0.45;
/// The steps down to the track at a platform's tunnel end (its own frame):
/// across x, and from the deck (-z, high) down to the bed at the edge.
pub const TRACK_STEPS_X: (f32, f32) = (0.6, 2.4);
pub const TRACK_STEPS_Z: f32 = -1.2;
/// Where a tunnel end's platform stops (own frame, x): the tunnel is W of it.
pub const TUNNEL_PORTAL_X: f32 = -1.2;
/// The columns on a platform (own frame, x, z).
pub const COLUMNS: [(f32, f32); 2] = [(-1.5, -0.4), (1.5, -0.4)];

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
    /// The big rooms' pieces.
    #[serde(default)]
    pub room: RoomKit,
}

/// The pieces a big room is drawn with (each a model one cell across).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct RoomKit {
    /// Floor and ceiling of one room cell, no walls.
    pub floor: String,
    /// One cell's wall, modelled along its N edge; turned onto each walled side.
    pub wall: String,
    /// A column, modelled at the cell's middle; stood where four room cells meet.
    pub column: String,
    /// A tall room's sunken floor (no ceiling).
    #[serde(default)]
    pub pit: String,
    /// A ceiling alone (over a sunken floor).
    #[serde(default)]
    pub ceiling: String,
    /// A plain 1 m wall strip along the N edge, stretched up.
    #[serde(default)]
    pub wall_plain: String,
    /// Mezzanine stairs: from a deck's edge (origin) down towards +z.
    #[serde(default)]
    pub stairs: String,
    /// 1 m of railing along x, stretched.
    #[serde(default)]
    pub rail: String,
    /// A heap of rubble filling a corridor across (x), centred on the side.
    #[serde(default)]
    pub rubble: String,
}

impl RoomKit {
    /// The model a shape is drawn with.
    pub fn file(&self, piece: ShapePiece) -> &str {
        match piece {
            ShapePiece::Pit => &self.pit,
            ShapePiece::Ceiling => &self.ceiling,
            ShapePiece::Wall => &self.wall,
            ShapePiece::WallPlain => &self.wall_plain,
            ShapePiece::Stairs => &self.stairs,
            ShapePiece::Rail => &self.rail,
            ShapePiece::Rubble => &self.rubble,
        }
    }
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
    /// Solid ground under a track bed from the level above (no model).
    Under,
    /// A cell of a big room (index into the level's rooms).
    Room(u8),
}

/// One filled cell: its open sides, what it is and how far it's turned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fill {
    pub mask: u8,
    /// The open sides that are track (a subway platform piece), already turned.
    pub track: u8,
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
    /// The big rooms on it (their bounding rectangles: an L-shaped room's
    /// missing corner is corridor; a cell is in room `i` when its kind is
    /// `Kind::Room(i)`).
    pub rooms: Vec<wfc::Room>,
    /// Per cell: the sides blocked by rubble (bits by side; empty: none).
    pub rubble: Vec<u8>,
    /// Per cell: the sides with a squeeze (rubble either side of a gap).
    pub squeeze: Vec<u8>,
    /// Per cell: a tall room's sunken floor, [`Level::sink`] down.
    pub sunk: Vec<bool>,
    /// Per cell: a mezzanine deck's sides with stairs down into the sunken
    /// cell next door.
    pub stairs: Vec<u8>,
    /// How far sunken cells are sunk (m).
    pub sink: f32,
}

impl Level {
    /// No cells yet (a base for the plan's `..`).
    fn empty(w: usize, h: usize) -> Level {
        Level {
            width: w,
            depth: h,
            cells: Vec::new(),
            arrive: (0, 0),
            goal: (0, 0),
            goal_entry: wfc::Side::North,
            path: Vec::new(),
            rooms: Vec::new(),
            rubble: Vec::new(),
            squeeze: Vec::new(),
            sunk: Vec::new(),
            stairs: Vec::new(),
            sink: 0.0,
        }
    }

    pub fn fill(&self, c: wfc::Cell) -> Fill {
        self.cells[c.1 * self.width + c.0]
    }

    fn side_bit(v: &[u8], i: usize, s: wfc::Side) -> bool {
        v.get(i).is_some_and(|b| b & bit(s) != 0)
    }

    /// Is side `s` of `c` blocked by rubble?
    pub fn rubble(&self, c: wfc::Cell, s: wfc::Side) -> bool {
        Self::side_bit(&self.rubble, c.1 * self.width + c.0, s)
    }

    /// Is side `s` of `c` a squeeze?
    pub fn squeeze(&self, c: wfc::Cell, s: wfc::Side) -> bool {
        Self::side_bit(&self.squeeze, c.1 * self.width + c.0, s)
    }

    /// Is `c` a tall room's sunken floor?
    pub fn sunk(&self, c: wfc::Cell) -> bool {
        self.sunk
            .get(c.1 * self.width + c.0)
            .copied()
            .unwrap_or(false)
    }

    /// Do stairs go down from deck `c` across `s`?
    pub fn stairs_down(&self, c: wfc::Cell, s: wfc::Side) -> bool {
        Self::side_bit(&self.stairs, c.1 * self.width + c.0, s)
    }

    /// How far below the level's floor `c`'s floor is.
    pub fn drop(&self, c: wfc::Cell) -> f32 {
        if self.sunk(c) { self.sink } else { 0.0 }
    }

    /// Can you walk from `c` across `s`? (Both cells open on the shared
    /// side, no rubble, and between a deck and a sunken floor only by stairs.)
    pub fn open(&self, c: wfc::Cell, s: wfc::Side) -> bool {
        wfc::neighbour(self.width, self.depth, c, s).is_some_and(|n| {
            self.fill(c).mask & bit(s) != 0
                && self.fill(n).mask & bit(s.opposite()) != 0
                && !self.rubble(c, s)
                && (self.sunk(c) == self.sunk(n)
                    || self.stairs_down(c, s)
                    || self.stairs_down(n, s.opposite()))
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
        let track = p.track.iter().fold(0, |m, s| m | bit(s.wfc())) & mask;
        let mut seen = Vec::new();
        for turns in 0..4u8 {
            let m = turned(mask, turns);
            let tr = turned(track, turns);
            // A track piece's turns all differ: which side its deck is on.
            let key = (m, tr, if tr == 0 { 0 } else { turns });
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let kind = Kind::Corridor(Some(i as u8));
            out.push((
                m,
                Fill {
                    mask: m,
                    track: tr,
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
    let mask = if kind == Kind::Under { 0 } else { bit(side) };
    (
        mask,
        Fill {
            mask,
            track: 0,
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

/// A tile side's socket: closed, corridor or track (only like meets like).
/// A track socket also says which way its deck lies (the piece's turned N),
/// so a run's platform stays on one side of its track bed.
fn socket(f: &Fill, s: wfc::Side) -> u8 {
    // (A room's open sides are plain corridor sockets: its inside sides
    // only ever meet its own pinned cells.)
    if f.mask & bit(s) == 0 {
        0
    } else if f.track & bit(s) != 0 {
        2 + f.turns % 4
    } else {
        1
    }
}

/// Does a level's track leave the level below room to wind through? The
/// cells off the track (which stay open below) must hang together, and no
/// row or column may be track from end to end.
fn leaves_room_below(l: &Level) -> bool {
    hangs_together(l, |c| l.fill(c).track != 0)
}

/// Does the level below hang together with the cells `solid` says sink
/// into it (track beds, sunken floors) solid there?
fn hangs_together(l: &Level, track: impl Fn(wfc::Cell) -> bool) -> bool {
    let (w, h) = (l.width, l.depth);
    // The landing below (under the stairs) opens one way only: count it as
    // blocked too.
    let blocked = |c: wfc::Cell| track(c) || c == l.goal;
    let full_row = (0..h).any(|z| (0..w).all(|x| track((x, z))));
    let full_col = (0..w).any(|x| (0..h).all(|z| track((x, z))));
    if full_row || full_col {
        return false;
    }
    let free = (0..w * h).filter(|&i| !blocked((i % w, i / w))).count();
    let Some(start) = (0..w * h).map(|i| (i % w, i / w)).find(|&c| !blocked(c)) else {
        return false;
    };
    let reach = wfc::distances(w, h, start, |c, s| {
        wfc::neighbour(w, h, c, s).is_some_and(|n| !blocked(c) && !blocked(n))
    });
    reach.iter().filter(|d| d.is_some()).count() == free
}

/// What one level asks of the generator besides its way through.
struct LevelAsk<'a> {
    arrive: wfc::Cell,
    landing: Option<wfc::Side>,
    last: bool,
    min_path: u32,
    /// Cells under the level above's track beds: solid, no way through.
    under: &'a [bool],
    /// This many track cells at least (0: no platforms on this level)...
    platform_cells: usize,
    /// ...and at most.
    platform_max: usize,
    rooms: &'a Rooms,
}

/// A level's big rooms and their doors: up to `ask.max` rooms on cells
/// `free` allows, a cell apart, each with doors onto cells that aren't solid.
fn place_rooms(
    rng: &mut wfc::Rng,
    (w, h): (usize, usize),
    ask: &Rooms,
    free: impl Fn(wfc::Cell) -> bool,
    solid: impl Fn(wfc::Cell) -> bool,
) -> Vec<(wfc::Room, Vec<(wfc::Cell, wfc::Side)>)> {
    let count = 1 + rng.below(ask.max.max(1));
    let rooms = wfc::rooms::place(rng, (w, h), count.min(ask.max), &ask.sizes, 1, free);
    let (lo, hi) = (ask.doors.0.max(1), ask.doors.1.max(ask.doors.0.max(1)));
    rooms
        .into_iter()
        .map(|r| {
            let n = lo + rng.below(hi - lo + 1);
            let doors = wfc::rooms::doors(rng, &r, (w, h), n, |c, s| {
                wfc::neighbour(w, h, c, s).is_some_and(|n| !solid(n))
            });
            (r, doors)
        })
        .collect()
}

/// One level: WFC with the arrival and the goal pinned, rolled again until the
/// way between them is long enough (and, on a platform level, there's track).
fn plan_level(rng: &mut wfc::Rng, (w, h): (usize, usize), ask: &LevelAsk, vars: &[Var]) -> Level {
    let LevelAsk {
        arrive,
        landing,
        last,
        min_path,
        under,
        platform_cells,
        platform_max,
        rooms: room_ask,
    } = *ask;
    let goal_kind = if last { Kind::Platform } else { Kind::Stairs };
    let platforms = platform_cells > 0;
    let corridors: Vec<usize> = (0..vars.len())
        .filter(|&i| platforms || vars[i].1.track == 0)
        .collect();
    let open_corridors: Vec<usize> = corridors
        .iter()
        .copied()
        .filter(|&i| vars[i].0 != 0 && vars[i].1.track == 0)
        .collect();
    let is_under = |c: wfc::Cell| under.get(c.1 * w + c.0).copied().unwrap_or(false);
    for attempt in 0..160 {
        let goal = (rng.below(w), rng.below(h));
        if goal == arrive || is_under(goal) {
            continue;
        }
        let sides = entries(w, h, goal, last);
        if sides.is_empty() {
            continue;
        }
        let entry = sides[rng.below(sides.len())];
        // Ask for the full length first, then settle for less.
        let need = if attempt < 140 { min_path } else { 1 };
        let manhattan = arrive.0.abs_diff(goal.0) + arrive.1.abs_diff(goal.1);
        if attempt < 140 && (manhattan as u32) * 2 < need {
            continue;
        }
        let mut tiles: Vec<Var> = vars.to_vec();
        let arrive_tile = landing.map(|s| {
            tiles.push(special(Kind::Landing, s));
            tiles.len() - 1
        });
        tiles.push(special(goal_kind, entry));
        let goal_tile = tiles.len() - 1;
        tiles.push(special(Kind::Under, entry));
        let under_tile = tiles.len() - 1;
        // Big rooms (not in the last tries, which settle for anything).
        let rooms = if attempt < 120 && rng.roll() < room_ask.chance {
            place_rooms(
                rng,
                (w, h),
                room_ask,
                |c| {
                    let near = |a: wfc::Cell| a.0.abs_diff(c.0) + a.1.abs_diff(c.1) <= 1;
                    !is_under(c) && !near(arrive) && !near(goal)
                },
                is_under,
            )
        } else {
            Vec::new()
        };
        // L-shaped rooms: a corner block goes back to corridor.
        let mut rooms = rooms;
        let cuts: Vec<Vec<wfc::Cell>> = rooms
            .iter_mut()
            .map(|(room, doors)| {
                let (a, b) = room.size;
                if a < 2 || b < 2 || rng.roll() >= room_ask.l_chance {
                    return Vec::new();
                }
                let corner = rng.below(4);
                let (ca, cb) = (a / 2, b / 2);
                let x0 = if corner.is_multiple_of(2) {
                    room.at.0
                } else {
                    room.at.0 + a - ca
                };
                let z0 = if corner < 2 {
                    room.at.1
                } else {
                    room.at.1 + b - cb
                };
                let cut: Vec<wfc::Cell> = (z0..z0 + cb)
                    .flat_map(|z| (x0..x0 + ca).map(move |x| (x, z)))
                    .collect();
                let kept: Vec<_> = doors
                    .iter()
                    .copied()
                    .filter(|(c, _)| !cut.contains(c))
                    .collect();
                if kept.is_empty() {
                    return Vec::new();
                }
                *doors = kept;
                cut
            })
            .collect();
        let mut room_tile = vec![None; w * h];
        for (ri, (room, doors)) in rooms.iter().enumerate() {
            let cut = &cuts[ri];
            for c in room.cells().filter(|c| !cut.contains(c)) {
                let edges = wfc::Side::ALL.map(|s| {
                    let into_cut = wfc::neighbour(w, h, c, s).is_some_and(|n| cut.contains(&n));
                    if into_cut {
                        wfc::Edge::Wall
                    } else {
                        room.edge(c, s, doors)
                    }
                });
                let mask = wfc::Side::ALL
                    .into_iter()
                    .filter(|s| edges[s.index()] != wfc::Edge::Wall)
                    .fold(0, |m, s| m | bit(s));
                tiles.push((
                    mask,
                    Fill {
                        mask,
                        track: 0,
                        kind: Kind::Room(ri as u8),
                        turns: 0,
                    },
                    1.0,
                ));
                room_tile[c.1 * w + c.0] = Some(tiles.len() - 1);
            }
        }
        let mut problem = wfc::Wfc::new(
            w,
            h,
            tiles
                .iter()
                .map(|(_, f, weight)| wfc::Tile {
                    sockets: wfc::Side::ALL.map(|s| socket(f, s)),
                    weight: *weight,
                })
                .collect(),
        );
        for z in 0..h {
            for x in 0..w {
                let c = (x, z);
                let allowed = if let Some(t) = room_tile[z * w + x] {
                    vec![t]
                } else if c == goal {
                    vec![goal_tile]
                } else if is_under(c) {
                    vec![under_tile]
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
            rooms: rooms.iter().map(|r| r.0).collect(),
            ..Level::empty(w, h)
        };
        let solved = problem.solve_until(
            rng,
            if platforms { 24 } else { 6 },
            |a, b| a == b,
            |_, _, socket| *socket == 0,
            |s| {
                let l = build(s);
                let tracks = l.cells.iter().filter(|f| f.track != 0).count();
                // Every platform and room can be walked into from the way in.
                let d = l.distances();
                let reached = l.cells.iter().zip(&d).all(|(f, d)| {
                    (f.track == 0 && !matches!(f.kind, Kind::Room(_))) || d.is_some()
                });
                // The landing below opens away from the stairs' entry: that
                // cell can't be under a track bed.
                let below_exit = wfc::neighbour(w, h, goal, entry.opposite())
                    .is_some_and(|c| l.fill(c).track != 0);
                tracks >= platform_cells
                    && tracks <= platform_max
                    && reached
                    && !below_exit
                    && (tracks == 0 || leaves_room_below(&l))
                    && d[goal.1 * w + goal.0].is_some_and(|d| d >= need)
            },
        );
        if let Some(s) = solved {
            let mut level = build(&s);
            let d = level.distances();
            level.path = path_to(&level, &d);
            return level;
        }
    }
    fallback(w, h, ask)
}

/// One open hall (never needed in practice; keeps generation total).
fn fallback(w: usize, h: usize, ask: &LevelAsk) -> Level {
    let LevelAsk {
        arrive,
        landing,
        last,
        under,
        ..
    } = *ask;
    let is_under = |c: wfc::Cell| under.get(c.1 * w + c.0).copied().unwrap_or(false);
    // The goal: the farthest cell with a way in that isn't the arrival, and
    // that the arrival's landing doesn't open straight onto.
    let exit_to = landing.and_then(|l| wfc::neighbour(w, h, arrive, l));
    let (goal, entry) = (0..w * h)
        .map(|i| (i % w, i / w))
        .filter(|&c| c != arrive && Some(c) != exit_to && !is_under(c))
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
            && !is_under(c)
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
            } else if is_under(c) {
                special(Kind::Under, wfc::Side::North).1
            } else {
                Fill {
                    mask,
                    track: 0,
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
        ..Level::empty(w, h)
    };
    let d = level.distances();
    level.path = path_to(&level, &d);
    level
}

/// After the plan: tall rooms (sunken floors, mezzanine decks and their
/// stairs) and caved-in corridors, rolled per station, level, room and side
/// (so they don't touch the plan's random numbers). Never cuts anything off:
/// every cell you could reach before you still can.
fn shape_level(t: &Tuning, li: usize, l: &mut Level) {
    let n = l.width * l.depth;
    l.rubble = vec![0; n];
    l.squeeze = vec![0; n];
    l.sunk = vec![false; n];
    l.stairs = vec![0; n];
    l.sink = t.rooms.tall.sink.clamp(0.0, 1.9);
    let reach = |l: &Level| -> Vec<bool> { l.distances().iter().map(|d| d.is_some()).collect() };
    let before = reach(l);
    if l.sink > 0.2 {
        for ri in 0..l.rooms.len() {
            if dress_roll(t, [li, ri, usize::MAX - 1, 0]) < t.rooms.tall.chance {
                make_tall(l, ri, |k| dress_roll(t, [li, ri, usize::MAX - 1, k + 1]));
            }
        }
    }
    let c = &t.collapse;
    let w = l.width;
    for i in 0..n {
        let a = (i % w, i / w);
        for s in [wfc::Side::East, wfc::Side::South] {
            let Some(b) = wfc::neighbour(w, l.depth, a, s) else {
                continue;
            };
            let corridor = |c: wfc::Cell| {
                let f = l.fill(c);
                matches!(f.kind, Kind::Corridor(_)) && f.track == 0
            };
            if !corridor(a) || !corridor(b) || !l.open(a, s) {
                continue;
            }
            let j = b.1 * w + b.0;
            let roll = dress_roll(t, [li, i, s.index(), 7]);
            let mut squeeze = roll >= c.blocked && roll < c.blocked + c.squeeze;
            if roll < c.blocked {
                l.rubble[i] |= bit(s);
                l.rubble[j] |= bit(s.opposite());
                let after = reach(l);
                if before.iter().zip(&after).any(|(b, a)| *b && !*a) {
                    // The only way through: it caves in partly.
                    l.rubble[i] &= !bit(s);
                    l.rubble[j] &= !bit(s.opposite());
                    squeeze = true;
                }
            }
            if squeeze {
                l.squeeze[i] |= bit(s);
                l.squeeze[j] |= bit(s.opposite());
            }
        }
    }
    let d = l.distances();
    l.path = path_to(l, &d);
}

/// Make room `ri` tall if it can be: the cells with a door stay up as
/// decks, the rest sink, and stairs go down from decks (one landing per
/// sunken cell) until the room hangs together. Leaves it alone when it has
/// no cell without a door, or the level below wouldn't hang together.
fn make_tall(l: &mut Level, ri: usize, roll: impl Fn(usize) -> f32) {
    let (w, h) = (l.width, l.depth);
    let mine = |c: wfc::Cell| l.fill(c).kind == Kind::Room(ri as u8);
    let cells: Vec<wfc::Cell> = (0..w * h)
        .map(|i| (i % w, i / w))
        .filter(|&c| mine(c))
        .collect();
    let deck = |c: wfc::Cell| {
        wfc::Side::ALL.into_iter().any(|s| {
            l.fill(c).mask & bit(s) != 0 && wfc::neighbour(w, h, c, s).is_some_and(|n| !mine(n))
        })
    };
    let pit: Vec<bool> = (0..w * h)
        .map(|i| mine((i % w, i / w)) && !deck((i % w, i / w)))
        .collect();
    let pits = pit.iter().filter(|p| **p).count();
    if pits == 0 || pits == cells.len() {
        return;
    }
    let solid = |c: wfc::Cell| {
        let i = c.1 * w + c.0;
        l.fill(c).track != 0 || l.sunk[i] || pit[i]
    };
    if !hangs_together(l, solid) {
        return;
    }
    // Join the room up: same-height neighbours always, a deck and a sunken
    // cell by stairs.
    let mut group: Vec<usize> = (0..w * h).collect();
    fn find(g: &mut [usize], mut i: usize) -> usize {
        while g[i] != i {
            g[i] = g[g[i]];
            i = g[i];
        }
        i
    }
    let mut stairs: Vec<(wfc::Cell, wfc::Side)> = Vec::new();
    let mut ways = Vec::new();
    for &c in &cells {
        for s in [
            wfc::Side::East,
            wfc::Side::South,
            wfc::Side::North,
            wfc::Side::West,
        ] {
            let Some(n) = wfc::neighbour(w, h, c, s) else {
                continue;
            };
            if !mine(n) || l.fill(c).mask & bit(s) == 0 {
                continue;
            }
            let (ic, inn) = (c.1 * w + c.0, n.1 * w + n.0);
            if pit[ic] == pit[inn] {
                let (a, b) = (find(&mut group, ic), find(&mut group, inn));
                group[a] = b;
            } else if !pit[ic] {
                ways.push((roll(ic * 4 + s.index()), c, s));
            }
        }
    }
    ways.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut landed = Vec::new();
    for (_, c, s) in ways {
        let Some(n) = wfc::neighbour(w, h, c, s) else {
            continue;
        };
        let (ic, inn) = (c.1 * w + c.0, n.1 * w + n.0);
        let (a, b) = (find(&mut group, ic), find(&mut group, inn));
        if a != b && !landed.contains(&n) {
            group[a] = b;
            landed.push(n);
            stairs.push((c, s));
        }
    }
    let root = find(&mut group, cells[0].1 * w + cells[0].0);
    if cells
        .iter()
        .any(|c| find(&mut group, c.1 * w + c.0) != root)
    {
        return;
    }
    for (i, p) in pit.iter().enumerate() {
        if *p {
            l.sunk[i] = true;
        }
    }
    for (c, s) in stairs {
        l.stairs[c.1 * w + c.0] |= bit(s);
    }
}

/// The station's plan so far, planned a level at a time (each takes a WFC
/// solve): the same levels [`generate`] makes, in the same order, from the
/// same seed, however many are planned at once. Every peer plans its own copy.
#[derive(Clone)]
pub struct StationPlan {
    rng: wfc::Rng,
    vars: Vec<Var>,
    size: (usize, usize),
    total: usize,
    arrive: wfc::Cell,
    landing: Option<wfc::Side>,
    /// Cells under the last planned level's track beds.
    under: Vec<bool>,
    /// The levels planned so far, top first.
    pub levels: Vec<Level>,
}

impl StationPlan {
    /// Nothing planned yet.
    pub fn new(t: &Tuning, set: &TileSet) -> Self {
        let (w, h) = (t.width.max(3), t.depth.max(3));
        StationPlan {
            rng: wfc::Rng::new(t.seed),
            vars: variants(set),
            size: (w, h),
            total: t.levels.max(1),
            arrive: (w / 2, 0),
            landing: None,
            under: vec![false; w * h],
            levels: Vec::with_capacity(t.levels.max(1)),
        }
    }

    /// Every level is planned.
    pub fn done(&self) -> bool {
        self.levels.len() >= self.total
    }

    /// Plan the next level down (`None` when the station is all planned).
    pub fn plan_next(&mut self, t: &Tuning) -> Option<&Level> {
        if self.done() {
            return None;
        }
        let l = self.levels.len();
        let ask = LevelAsk {
            arrive: self.arrive,
            landing: self.landing,
            last: l + 1 == self.total,
            min_path: t.min_path,
            under: &self.under,
            platform_cells: if platform_level(t, l) {
                t.platform_cells.max(2)
            } else {
                0
            },
            platform_max: t.platform_max.max(t.platform_cells.max(2)),
            rooms: &t.rooms,
        };
        let level = if self.vars.is_empty() {
            fallback(self.size.0, self.size.1, &ask)
        } else {
            plan_level(&mut self.rng, self.size, &ask, &self.vars)
        };
        let mut level = level;
        shape_level(t, l, &mut level);
        self.arrive = level.goal;
        self.landing = Some(level.goal_entry.opposite());
        self.under = (0..level.cells.len())
            .map(|i| level.cells[i].track != 0 || level.sunk[i])
            .collect();
        self.levels.push(level);
        self.levels.last()
    }

    /// Plan until at least `n` levels (or all of them) are planned.
    pub fn plan_to(&mut self, t: &Tuning, n: usize) {
        while self.levels.len() < n && self.plan_next(t).is_some() {}
    }
}

/// The whole station: every level, top first. Pure and seeded.
pub fn generate(t: &Tuning, set: &TileSet) -> Vec<Level> {
    let mut plan = StationPlan::new(t, set);
    plan.plan_to(t, usize::MAX);
    plan.levels
}

/// Does level `l` get subway platforms? Every [`Tuning::platform_every`]
/// levels from the first, never the J's.
pub fn platform_level(t: &Tuning, l: usize) -> bool {
    t.platform_every > 0 && l.is_multiple_of(t.platform_every) && l + 1 < t.levels.max(1)
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

/// The floor's height under `p` (on the level `p` is on): lower in a tall
/// room's sunken cells, on the way down their stairs.
pub fn floor_at(t: &Tuning, levels: &[Level], p: Vec3) -> f32 {
    let li = level_at(p.y);
    let top = level_floor(li);
    let Some(l) = levels.get(li) else {
        return top;
    };
    let o = cell_world(t, li, (0, 0));
    let (x, z) = (((p.x - o.x) / CELL).round(), ((p.z - o.z) / CELL).round());
    if x < 0.0 || z < 0.0 || x as usize >= l.width || z as usize >= l.depth {
        return top;
    }
    let c = (x as usize, z as usize);
    if !l.sunk(c) {
        return top;
    }
    let off = p - cell_world(t, li, c);
    let mut y = top - l.sink;
    for s in wfc::Side::ALL {
        let Some(n) = wfc::neighbour(l.width, l.depth, c, s) else {
            continue;
        };
        if !l.stairs_down(n, s.opposite()) {
            continue;
        }
        let d = dir(s);
        let into = CELL / 2.0 - off.dot(d);
        let across = (off - d * off.dot(d)).with_y(0.0).length();
        if across <= STAIRS_W / 2.0 && into < STAIRS_RUN {
            y = y.max(top - l.sink * (into.max(0.0) / STAIRS_RUN));
        }
    }
    y
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
                // Solid; under a track bed only up to the bed's underside.
                let top = if f.kind == Kind::Under {
                    ROOM_H - TRACK_DROP.max(t.rooms.tall.sink.clamp(0.0, 1.9)) - 0.2
                } else {
                    ROOM_H
                };
                out.push(flat(
                    at + Vec3::Y * ((top - 0.2) / 2.0),
                    Vec3::new(CELL, top + 0.2, CELL),
                ));
                continue;
            }
            if f.track != 0 {
                for b in track_blocks(f) {
                    out.push(Block {
                        centre: at + r * b.centre,
                        size: b.size,
                        rotation: r * b.rotation,
                    });
                }
            } else if f.kind != Kind::Stairs {
                let drop = level.drop(c);
                out.push(flat(
                    at - Vec3::Y * (0.1 + drop),
                    Vec3::new(CELL, 0.2, CELL),
                ));
            }
            // A track cell's walls go on down past the bed, a sunken cell's
            // down to its floor.
            let low = if f.track != 0 {
                TRACK_DROP + 0.2
            } else {
                level.drop(c)
            };
            for s in wfc::Side::ALL {
                if f.mask & bit(s) != 0 {
                    continue;
                }
                let d = dir(s);
                let tall = ROOM_H + low;
                let size = if d.x != 0.0 {
                    Vec3::new(0.2, tall, CELL)
                } else {
                    Vec3::new(CELL, tall, 0.2)
                };
                out.push(flat(
                    at + d * (half - 0.1) + Vec3::Y * (tall / 2.0 - low),
                    size,
                ));
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
    out.extend(
        room_dressing(t, li, level)
            .into_iter()
            .chain(corridor_dressing(t, &TILES.get(), li, level))
            .filter_map(|d| d.block),
    );
    out.extend(shapes(t, li, level).into_iter().filter_map(|s| s.block));
    for at in room_columns(t, li, level) {
        out.push(Block {
            centre: at + Vec3::Y * (ROOM_H / 2.0),
            size: Vec3::new(COLUMN_SIZE, ROOM_H, COLUMN_SIZE),
            rotation: Quat::IDENTITY,
        });
    }
    out
}

/// Where the big rooms' columns stand (on the floor): every inside corner,
/// where four of a room's cells meet.
pub fn room_columns(t: &Tuning, li: usize, level: &Level) -> Vec<Vec3> {
    level
        .rooms
        .iter()
        .enumerate()
        .filter(|&(ri, r)| room_kind(t, li, ri, r).is_none_or(|k| k.columns))
        // Not in a tall room (its floor isn't all one height).
        .filter(|&(ri, r)| {
            !r.cells()
                .any(|c| level.sunk(c) && level.fill(c).kind == Kind::Room(ri as u8))
        })
        .flat_map(|(ri, r)| {
            r.inner_corners().filter(move |c| {
                // All four cells in the room (not an L-shaped room's missing corner).
                [(0, 0), (1, 0), (0, 1), (1, 1)]
                    .iter()
                    .all(|(dx, dz)| level.fill((c.0 + dx, c.1 + dz)).kind == Kind::Room(ri as u8))
            })
        })
        .map(|c| cell_world(t, li, c) + Vec3::new(CELL / 2.0, 0.0, CELL / 2.0))
        .collect()
}

/// A number from 0 to 1 for this station, level, room, cell and slot (the
/// rooms' dressing doesn't touch the plan's random numbers).
fn dress_roll(t: &Tuning, parts: [usize; 4]) -> f32 {
    let mut h = t.seed as u64 ^ 0x9e37_79b9_7f4a_7c15;
    for p in parts {
        h = (h ^ p as u64).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        h ^= h >> 31;
        h = h.wrapping_mul(0x94d0_49bb_1331_11eb);
        h ^= h >> 29;
    }
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// What big room `ri` on level `li` is for: one of [`Rooms::kinds`] that
/// fits its size, by weight (`None` when none does: a plain hall).
pub fn room_kind<'a>(t: &'a Tuning, li: usize, ri: usize, r: &wfc::Room) -> Option<&'a RoomKind> {
    let (a, b) = r.size;
    let fits: Vec<&RoomKind> = t
        .rooms
        .kinds
        .iter()
        .filter(|k| {
            k.weight > 0.0
                && (k.sizes.is_empty() || k.sizes.iter().any(|&s| s == (a, b) || s == (b, a)))
        })
        .collect();
    let total: f32 = fits.iter().map(|k| k.weight).sum();
    let mut pick = dress_roll(t, [li, ri, usize::MAX, 0]) * total;
    for k in &fits {
        if pick < k.weight {
            return Some(k);
        }
        pick -= k.weight;
    }
    fits.last().copied()
}

/// One prop dressing a big room.
#[derive(Clone, Debug, PartialEq)]
pub struct Dressing {
    /// Under `assets/models/`.
    pub model: String,
    pub transform: Transform,
    /// Its collider, if it has one.
    pub block: Option<Block>,
}

/// How far a wall prop stands off the wall (m, the wall's thickness and a gap).
const PROP_INSET: f32 = 0.3;
/// How far a wall's inner face is in from the cell's edge (m): flush props' backs.
const WALL_FACE: f32 = 0.2;
/// The ceiling's underside over a cell's floor (m): ceiling props hang from it.
const CEILING_Y: f32 = 3.8;

/// The props dressing level `li`'s big rooms, by their kinds. Wall props
/// stand against walled sides only (never in a door), and their colliders
/// keep well clear of the cell's middle, so the way through and every
/// walking spot stay open.
pub fn room_dressing(t: &Tuning, li: usize, level: &Level) -> Vec<Dressing> {
    let mut out = Vec::new();
    for (ri, r) in level.rooms.iter().enumerate() {
        let Some(kind) = room_kind(t, li, ri, r) else {
            continue;
        };
        for (ci, c) in r.cells().enumerate() {
            let f = level.fill(c);
            if f.kind != Kind::Room(ri as u8) {
                continue;
            }
            let at = cell_world(t, li, c) - Vec3::Y * level.drop(c);
            // A sunken cell's ceiling stays at the level's.
            let ceiling = CEILING_Y + level.drop(c);
            dress_cell(
                t,
                [li, ri * 64 + ci],
                at,
                ceiling,
                f.mask,
                &kind.props,
                &mut out,
            );
        }
    }
    out
}

/// The props dressing level `li`'s plain corridor pieces (`rooms.corridors`):
/// not the platform, the stairs, the landings or a piece with a hiding spot.
pub fn corridor_dressing(t: &Tuning, set: &TileSet, li: usize, level: &Level) -> Vec<Dressing> {
    let mut out = Vec::new();
    if t.rooms.corridors.is_empty() {
        return out;
    }
    for z in 0..level.depth {
        for x in 0..level.width {
            let c = (x, z);
            let f = level.fill(c);
            let Kind::Corridor(Some(pi)) = f.kind else {
                continue;
            };
            if f.track != 0 || set.pieces.get(pi as usize).is_none_or(|p| p.hide) {
                continue;
            }
            let at = cell_world(t, li, c);
            let key = 1 << 20 | (z * level.width + x);
            dress_cell(
                t,
                [li, key],
                at,
                CEILING_Y,
                f.mask,
                &t.rooms.corridors,
                &mut out,
            );
        }
    }
    out
}

/// Every prop of `props` that lands in one cell (its floor's middle at `at`,
/// its ceiling `ceiling` m above that, open on the sides in `mask`), rolled
/// from `key` so every peer agrees.
fn dress_cell(
    t: &Tuning,
    key: [usize; 2],
    at: Vec3,
    ceiling: f32,
    mask: u8,
    props: &[RoomProp],
    out: &mut Vec<Dressing>,
) {
    for (pi, p) in props.iter().enumerate() {
        let scale = if p.scale > 0.0 { p.scale } else { 1.0 };
        let roll = |side: usize, slot: usize, k: usize| {
            dress_roll(t, [key[0], key[1], pi * 16 + side, slot * 4 + k])
        };
        let yaw = Quat::from_rotation_y(p.yaw.to_radians());
        match p.place {
            PropPlace::Wall => {
                for s in wfc::Side::ALL {
                    if mask & bit(s) != 0 {
                        continue;
                    }
                    let turn =
                        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2 * s.index() as f32);
                    let slots: &[f32] = if p.slots.is_empty() { &[0.0] } else { &p.slots };
                    for (si, &along) in slots.iter().enumerate() {
                        if roll(s.index(), si, 0) >= p.chance {
                            continue;
                        }
                        // Modelled facing +z: on the N wall it faces S, into the room.
                        let depth = p.size.2.max(0.0);
                        let (origin, middle) = if p.flush {
                            let face = CELL / 2.0 - WALL_FACE;
                            (face, face - depth / 2.0)
                        } else {
                            let off = CELL / 2.0 - PROP_INSET - depth / 2.0;
                            (off, off)
                        };
                        let centre = at + turn * Vec3::new(along, p.lift, -origin);
                        let block =
                            (p.size.0 > 0.0 && p.size.1 > 0.0 && depth > 0.0).then(|| Block {
                                centre: at + turn * Vec3::new(along, p.size.1 / 2.0, -middle),
                                size: Vec3::new(p.size.0, p.size.1, depth),
                                rotation: turn,
                            });
                        out.push(Dressing {
                            model: p.model.clone(),
                            transform: Transform::from_translation(centre)
                                .with_rotation(turn * yaw)
                                .with_scale(Vec3::splat(scale)),
                            block,
                        });
                    }
                }
            }
            PropPlace::Middle | PropPlace::Ceiling => {
                if roll(4, 0, 0) >= p.chance {
                    continue;
                }
                let off = p.slots.first().copied().unwrap_or(0.0);
                let jitter = Vec3::new(roll(4, 0, 1) - 0.5, 0.0, roll(4, 0, 2) - 0.5) * (2.0 * off);
                let y = if p.place == PropPlace::Ceiling {
                    ceiling - p.lift
                } else {
                    p.lift
                };
                // Ceiling props turn by quarters (ducts run along the corridor).
                let spin = if p.place == PropPlace::Ceiling {
                    (roll(4, 0, 3) * 4.0).floor() * std::f32::consts::FRAC_PI_2
                } else {
                    roll(4, 0, 3) * std::f32::consts::TAU
                };
                out.push(Dressing {
                    model: p.model.clone(),
                    transform: Transform::from_translation(at + jitter + Vec3::Y * y)
                        .with_rotation(Quat::from_rotation_y(spin) * yaw)
                        .with_scale(Vec3::splat(scale)),
                    block: None,
                });
            }
        }
    }
}

/// What a [`Shape`] is drawn with (the room kit's tall-room and rubble pieces).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapePiece {
    /// A sunken cell's floor.
    Pit,
    /// A sunken cell's ceiling (at the level's usual ceiling).
    Ceiling,
    /// The usual room wall.
    Wall,
    /// A plain wall strip, stretched up.
    WallPlain,
    /// Stairs from a deck down into a sunken cell.
    Stairs,
    /// A railing, stretched along.
    Rail,
    /// A heap of rubble.
    Rubble,
}

/// One piece of a tall room or a caved-in corridor, and its collider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub piece: ShapePiece,
    pub transform: Transform,
    pub block: Option<Block>,
}

/// The mezzanine stairs' run (m) and width, as modelled (`fs-room-stairs`,
/// 1.6 m down: scaled to the level's sink).
pub const STAIRS_RUN: f32 = 2.6;
pub const STAIRS_W: f32 = 2.4;
const STAIRS_MODEL_DROP: f32 = 1.6;
/// How tall a deck's railing's collider is.
const RAIL_H: f32 = 1.1;
/// How far a wall's top stays under the floor of the level above (m).
const FLUSH_GAP: f32 = 0.02;
/// How deep (m, across the side) a rubble heap's collider is.
const RUBBLE_DEPTH: f32 = 2.4;

/// A wall piece (modelled along the N edge) turned onto side `s`.
fn side_turn(s: wfc::Side) -> Quat {
    Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2 * s.index() as f32)
}

/// The tall rooms' and caved-in corridors' pieces on level `li`, from the
/// plan: sunken floors with their tall walls, the faces under the decks,
/// stairs, railings (open where the stairs go down), rubble heaps.
pub fn shapes(t: &Tuning, li: usize, level: &Level) -> Vec<Shape> {
    let mut out = Vec::new();
    let half = CELL / 2.0;
    let drop = level.sink;
    let (w, h) = (level.width, level.depth);
    let put = |piece, at: Vec3, rotation: Quat, scale: Vec3, block: Option<Block>| Shape {
        piece,
        transform: Transform::from_translation(at)
            .with_rotation(rotation)
            .with_scale(scale),
        block,
    };
    for z in 0..h {
        for x in 0..w {
            let c = (x, z);
            let f = level.fill(c);
            let at = cell_world(t, li, c);
            let in_room = |n: wfc::Cell| {
                matches!(level.fill(n).kind, Kind::Room(_)) && level.fill(n).kind == f.kind
            };
            if level.sunk(c) {
                let floor = at - Vec3::Y * drop;
                out.push(put(ShapePiece::Pit, floor, Quat::IDENTITY, Vec3::ONE, None));
                out.push(put(
                    ShapePiece::Ceiling,
                    at,
                    Quat::IDENTITY,
                    Vec3::ONE,
                    None,
                ));
                for s in wfc::Side::ALL {
                    let turn = side_turn(s);
                    let stretch = Vec3::new(1.0, drop, 1.0);
                    if f.mask & bit(s) == 0 {
                        out.push(put(ShapePiece::Wall, floor, turn, Vec3::ONE, None));
                        // The wall's top up to the ceiling, stopping short of
                        // it: its tiled top would lie flush in the floor of
                        // the level above and flicker there.
                        out.push(put(
                            ShapePiece::WallPlain,
                            floor + Vec3::Y * (ROOM_H - FLUSH_GAP),
                            turn,
                            Vec3::new(1.0, drop - FLUSH_GAP, 1.0),
                            None,
                        ));
                    } else if wfc::neighbour(w, h, c, s)
                        .is_some_and(|n| in_room(n) && !level.sunk(n))
                    {
                        // The face under a deck.
                        out.push(put(
                            ShapePiece::WallPlain,
                            floor,
                            turn,
                            stretch,
                            Some(Block {
                                centre: at + dir(s) * (half - 0.1) - Vec3::Y * (drop / 2.0),
                                size: Vec3::new(CELL, drop, 0.2),
                                rotation: turn,
                            }),
                        ));
                    }
                }
            } else if matches!(f.kind, Kind::Room(_)) {
                for s in wfc::Side::ALL {
                    if f.mask & bit(s) == 0
                        || !wfc::neighbour(w, h, c, s).is_some_and(|n| in_room(n) && level.sunk(n))
                    {
                        continue;
                    }
                    let turn = side_turn(s);
                    let edge = at + dir(s) * half;
                    let segments: &[(f32, f32)] = if level.stairs_down(c, s) {
                        let down = Quat::from_rotation_y(
                            std::f32::consts::PI - std::f32::consts::FRAC_PI_2 * s.index() as f32,
                        );
                        let tilt = (drop / STAIRS_RUN).atan();
                        let ramp = Quat::from_rotation_x(tilt);
                        let normal = ramp * Vec3::Y;
                        let local = Vec3::new(0.0, -drop / 2.0, STAIRS_RUN / 2.0) - normal * 0.15;
                        out.push(put(
                            ShapePiece::Stairs,
                            edge,
                            down,
                            Vec3::new(1.0, drop / STAIRS_MODEL_DROP, 1.0),
                            Some(Block {
                                centre: edge + down * local,
                                size: Vec3::new(
                                    STAIRS_W,
                                    0.3,
                                    (STAIRS_RUN * STAIRS_RUN + drop * drop).sqrt(),
                                ),
                                rotation: down * ramp,
                            }),
                        ));
                        &[(-half, -STAIRS_W / 2.0), (STAIRS_W / 2.0, half)]
                    } else {
                        &[(-half, half)]
                    };
                    for &(a, b) in segments {
                        let local = Vec3::new((a + b) / 2.0, 0.0, -(half - 0.08));
                        out.push(put(
                            ShapePiece::Rail,
                            at + turn * local,
                            turn,
                            Vec3::new(b - a, 1.0, 1.0),
                            Some(Block {
                                centre: at + turn * (local + Vec3::Y * (RAIL_H / 2.0)),
                                size: Vec3::new(b - a, RAIL_H, 0.1),
                                rotation: turn,
                            }),
                        ));
                    }
                }
            }
            // Caved-in sides, once per side (from its W / N cell).
            for s in [wfc::Side::East, wfc::Side::South] {
                let turn = side_turn(s);
                let edge = at + dir(s) * half;
                if level.rubble(c, s) {
                    out.push(put(
                        ShapePiece::Rubble,
                        edge,
                        turn,
                        Vec3::ONE,
                        Some(Block {
                            centre: edge + Vec3::Y * (ROOM_H / 2.0),
                            size: Vec3::new(CELL, ROOM_H, RUBBLE_DEPTH),
                            rotation: turn,
                        }),
                    ));
                } else if level.squeeze(c, s) {
                    let gap = t.collapse.gap.clamp(0.8, 4.0);
                    let heap = half - gap / 2.0;
                    for sign in [-1.0, 1.0] {
                        let along = sign * (gap / 2.0 + heap / 2.0);
                        let centre = edge + turn * Vec3::new(along, 0.0, 0.0);
                        out.push(put(
                            ShapePiece::Rubble,
                            centre,
                            turn,
                            Vec3::new(heap / CELL * 1.05, 0.75, 0.7),
                            Some(Block {
                                centre: centre + Vec3::Y * 1.5,
                                size: Vec3::new(heap, 3.0, RUBBLE_DEPTH * 0.7),
                                rotation: turn,
                            }),
                        ));
                    }
                }
            }
        }
    }
    out
}

/// A room column's collider (it stands on the floor at its point).
pub const COLUMN_SIZE: f32 = 0.6;

/// A subway platform piece's own colliders, in its own frame (track along x,
/// deck on -z, bed on +z): the deck, solid down to the bed; the bed; the
/// columns; and at a tunnel end (track on one side only) the steps down and
/// the portal beside the tunnel mouth.
pub fn track_blocks(f: Fill) -> Vec<Block> {
    let h = CELL / 2.0;
    let bed = -TRACK_DROP;
    let edge = PLATFORM_EDGE_Z;
    // A tunnel end is modelled with its track going E (tunnel W); the one
    // with its track going W is its mirror image.
    let own = turned(f.track, (4 - f.turns % 4) % 4);
    let flip = if own == bit(wfc::Side::West) {
        -1.0
    } else {
        1.0
    };
    let solid = |x0: f32, x1: f32, z0: f32, z1: f32, y0: f32, y1: f32| Block {
        centre: Vec3::new(flip * (x0 + x1) / 2.0, (y0 + y1) / 2.0, (z0 + z1) / 2.0),
        size: Vec3::new(x1 - x0, y1 - y0, z1 - z0),
        rotation: Quat::IDENTITY,
    };
    let mut out = vec![solid(-h, h, edge, h, bed - 0.2, bed)];
    // Track on one side only: a tunnel end, with the steps.
    if f.track.count_ones() == 1 {
        let (sx0, sx1) = TRACK_STEPS_X;
        let sz = TRACK_STEPS_Z;
        out.push(solid(-h, h, -h, sz, bed - 0.2, 0.0));
        out.push(solid(-h, sx0, sz, edge, bed - 0.2, 0.0));
        out.push(solid(sx1, h, sz, edge, bed - 0.2, 0.0));
        // The steps as a ramp: deck edge at sz (high) to the bed at the edge.
        let (run, rise) = (edge - sz, TRACK_DROP);
        let tilt = (rise / run).atan();
        let ramp = Quat::from_rotation_x(tilt);
        let normal = ramp * Vec3::Y;
        out.push(Block {
            centre: Vec3::new(flip * (sx0 + sx1) / 2.0, -rise / 2.0, (sz + edge) / 2.0)
                - normal * 0.15,
            size: Vec3::new(sx1 - sx0, 0.3, (run * run + rise * rise).sqrt()),
            rotation: ramp,
        });
        // The tunnel portal's mass beside the mouth, where the platform ends.
        out.push(solid(-h, TUNNEL_PORTAL_X, -h, edge, 0.0, ROOM_H));
    } else {
        out.push(solid(-h, h, -h, edge, bed - 0.2, 0.0));
        for (x, z) in COLUMNS {
            out.push(solid(x - 0.15, x + 0.15, z - 0.15, z + 0.15, 0.0, ROOM_H));
        }
    }
    out
}

/// The GTA VI ad board (#359: "add gta 6 ads to all subway stations").
pub const AD_MODEL: &str = "gta6-ad.gltf";

/// Where a platform piece's ad stands, in its own frame (it faces +z, the
/// track): at the back of the deck of a through platform (track on both
/// ends), when the back is a wall, not a way on.
pub fn ad_spot(f: Fill) -> Option<Vec3> {
    let own_mask = turned(f.mask, (4 - f.turns % 4) % 4);
    (f.track.count_ones() == 2 && own_mask & bit(wfc::Side::North) == 0).then_some(Vec3::new(
        0.0,
        0.0,
        -CELL / 2.0 + 0.35,
    ))
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
/// `None` until the last level is planned.
pub fn platform_at(t: &Tuning, levels: &[Level]) -> Option<Vec3> {
    if levels.len() < t.levels.max(1) {
        return None;
    }
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

/// The hole in the street the entrance's stairs go down through.
pub fn entrance_hole(t: &Tuning) -> friendslop_world::holes::Hole {
    let h = &t.entrance_hole;
    let yaw = t.entrance_yaw.to_radians();
    let at = entrance_at(t) + Quat::from_rotation_y(yaw) * Vec3::new(0.0, 0.0, h.offset);
    friendslop_world::holes::Hole::rect(at.xz(), Vec2::new(h.size.0, h.size.1), yaw)
        .on_floor(CITY_Y)
        .lined(h.depth, h.lip)
}

/// Every hole the station cuts in the street (`friendslop_world::holes::provide`).
fn holes() -> Vec<friendslop_world::holes::Hole> {
    vec![entrance_hole(&tuning())]
}

/// Does a body with feet at `feet` walk down into the entrance? Inside its
/// hole and [`Tuning::entrance_reach`] down the stairs.
pub fn entering(t: &Tuning, feet: Vec3) -> bool {
    feet.y < CITY_Y - t.entrance_reach
        && feet.y > CITY_Y - 6.0
        && entrance_hole(t).contains(feet.xz())
}

/// The entrance's stairs and walls (offsets inside its model, in its own
/// frame): a ramp from the street at the open end down to a landing at the
/// back, side and back walls from the landing up past the railings.
pub fn entrance_colliders(t: &Tuning) -> Vec<Block> {
    let at = entrance_at(t);
    let turn = Quat::from_rotation_y(t.entrance_yaw.to_radians());
    // The steps' noses: the top one at the open end, the bottom one at the landing.
    let (top, bottom) = (Vec3::new(0.0, 0.0, 3.05), Vec3::new(0.0, -3.06, -1.4));
    let along = top - bottom;
    let tilt = Quat::from_rotation_arc(Vec3::Z, along.normalize());
    let ramp_mid = (top + bottom) / 2.0 + tilt * Vec3::new(0.0, -0.15, 0.0);
    let mut out = vec![
        (ramp_mid, Vec3::new(2.2, 0.3, along.length()), tilt),
        // The landing at the foot of the stairs.
        (
            Vec3::new(0.0, -3.21, -2.3),
            Vec3::new(2.2, 0.3, 1.8),
            Quat::IDENTITY,
        ),
    ];
    for (offset, size) in [
        (Vec3::new(1.2, -1.15, 0.0), Vec3::new(0.25, 4.9, 6.2)),
        (Vec3::new(-1.2, -1.15, 0.0), Vec3::new(0.25, 4.9, 6.2)),
        (Vec3::new(0.0, -1.15, -3.1), Vec3::new(2.6, 4.9, 0.25)),
    ] {
        out.push((offset, size, Quat::IDENTITY));
    }
    out.into_iter()
        .map(|(offset, size, rotation)| Block {
            centre: at + turn * offset,
            size,
            rotation: turn * rotation,
        })
        .collect()
}

// ------------------------------------------------------------------ server

/// The station everyone builds from the data files, planned a level at a
/// time as people go down ([`plan_station`]): a level that isn't in
/// [`Station::levels`] yet isn't planned (or built) yet.
#[derive(Resource)]
pub struct Station {
    plan: StationPlan,
    /// Plan at least this many levels (features that need levels nobody
    /// has reached yet ask for them with [`Station::want`]).
    want: usize,
}

impl Station {
    /// The top [`Tuning::plan_ahead`] levels planned.
    pub fn new(t: &Tuning, set: &TileSet) -> Self {
        let mut plan = StationPlan::new(t, set);
        plan.plan_to(t, t.plan_ahead.max(1));
        Station { plan, want: 0 }
    }

    /// The levels planned so far, top first.
    pub fn levels(&self) -> &[Level] {
        &self.plan.levels
    }

    /// Ask for at least `n` levels to be planned (a level a frame).
    pub fn want(&mut self, n: usize) {
        self.want = self.want.max(n);
    }

    /// Are the top `n` levels (or all there are) planned?
    pub fn has(&self, t: &Tuning, n: usize) -> bool {
        self.plan.levels.len() >= n.min(t.levels.max(1))
    }
}

/// Pure: how many levels should be planned with bodies on these levels
/// (`None`: not in the station): [`Tuning::plan_ahead`] below the deepest,
/// and never fewer than `want`.
pub fn levels_wanted(t: &Tuning, deepest: Option<usize>, want: usize) -> usize {
    let ahead = t.plan_ahead.max(1);
    let by_body = deepest.map_or(ahead, |l| l + 1 + ahead);
    by_body.max(want).min(t.levels.max(1))
}

/// Pure: which levels' colliders should stand with bodies on these levels:
/// the top [`Tuning::plan_ahead`], and around each body the level above to
/// [`Tuning::plan_ahead`] below (the level under you is always solid before
/// you can drop into it).
pub fn levels_built(t: &Tuning, bodies: &[usize], planned: usize) -> Vec<usize> {
    let ahead = t.plan_ahead.max(1);
    let mut out: Vec<usize> = (0..ahead).collect();
    for &l in bodies {
        out.extend(l.saturating_sub(1)..=l + ahead);
    }
    out.sort_unstable();
    out.dedup();
    out.retain(|&l| l < planned);
    out
}

/// The levels of the bodies in the station.
fn body_levels<'a>(walkers: impl Iterator<Item = &'a Walker>) -> Vec<usize> {
    walkers
        .filter(|w| in_station(w.pos))
        .map(|w| level_at(w.pos.y))
        .collect()
}

/// Every peer: plan the next level when someone is getting near the bottom
/// of the plan (or a feature asked for more), one a frame.
pub fn plan_station(mut station: ResMut<Station>, walkers: Query<&Walker>) {
    let t = tuning();
    let deepest = body_levels(walkers.iter()).into_iter().max();
    let n = levels_wanted(&t, deepest, station.want);
    if station.plan.levels.len() < n {
        station.plan.plan_next(&t);
    }
}

/// One level's colliders (a parent in New York's colliders layer).
#[derive(Component)]
pub struct LevelSim(pub usize);

/// Every peer: the colliders of the levels near someone ([`levels_built`]),
/// spawned as they're planned, gone when nobody is near.
fn build_levels(
    mut commands: Commands,
    station: Res<Station>,
    walkers: Query<&Walker>,
    built: Query<(Entity, &LevelSim)>,
) {
    let t = tuning();
    let want = levels_built(&t, &body_levels(walkers.iter()), station.levels().len());
    let mut have = Vec::new();
    for (e, l) in &built {
        if want.contains(&l.0) {
            have.push(l.0);
        } else {
            commands.entity(e).despawn();
        }
    }
    for &li in want.iter().filter(|l| !have.contains(l)) {
        let level = &station.levels()[li];
        let root = commands
            .spawn((LevelSim(li), Transform::default(), crate::area::sim()))
            .id();
        // Colliders never wait on a model: boxes from the plan.
        for b in colliders(&t, li, level) {
            commands.spawn((
                Transform::from_translation(b.centre).with_rotation(b.rotation),
                avian3d::prelude::RigidBody::Static,
                avian3d::prelude::Collider::cuboid(b.size.x, b.size.y, b.size.z),
                ChildOf(root),
            ));
        }
    }
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
    // The entrance's stairs go down through a real hole in the street.
    friendslop_world::holes::provide(holes);
    app.add_client_event::<LeaveStation>(Channel::Ordered)
        .add_observer(leave)
        .init_resource::<Rewarded>()
        .add_systems(
            Update,
            (go_down, reward)
                .run_if(in_state(ServerState::Running))
                // The entrance and the J are in New York (#412).
                .run_if(crate::sites::awake("nyc"))
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
    mut input: Option<Mut<WalkInput>>,
    mut kcc: Option<Mut<crate::kcc::Kcc>>,
) {
    let yaw = walker.yaw;
    teleport(
        walker,
        input.as_deref_mut(),
        kcc.as_deref_mut(),
        to + Vec3::Y * 0.05,
        yaw,
    );
}

/// Server: walk into the entrance and you're on the station's top level.
fn go_down(station: Res<Station>, mut bodies: Query<Body>) {
    let t = tuning();
    for (_, mut walker, input, kcc) in &mut bodies {
        if entering(&t, walker.pos) {
            put(start_at(&t, station.levels()), &mut walker, input, kcc);
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
        if on_platform(&t, station.levels(), w.pos) && done.0.insert(p.id) {
            stats.earn_pelf(t.reward_pelf);
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
        platform_at(&t, station.levels())
    } else {
        Some(start_at(&t, station.levels()))
    };
    if !post.is_some_and(|p| within(walker.pos, p, SERVER_REACH + 3.0)) {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    put(street_at(&t), &mut walker, input, kcc);
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    crate::area::on_sim(app, build_sim);
    crate::area::on_show(app, build_station.after(build_sim));
    app.add_systems(
        PreUpdate,
        (plan_station, build_levels)
            .chain()
            .after(build_sim)
            .run_if(in_state(AppState::InGame))
            .run_if(resource_exists::<Station>)
            .run_if(friendslop_base::areas::live(crate::area::NYC)),
    );
    crate::features::start::provide(starts);
    app.add_observer(click)
        .add_observer(merge_ready)
        .add_systems(OnExit(AppState::InGame), |mut commands: Commands| {
            commands.remove_resource::<Station>()
        })
        .add_systems(
            Update,
            (
                stream_levels,
                merge_levels.after(stream_levels),
                hide_surface,
                follow_light,
                light_shadows,
                station_look.in_set(crate::place::PlaceSet),
                toasts,
                echo,
            )
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

/// The models of one level, spawned while the camera is near it (other
/// features dress it too: the hunt's lockers).
#[derive(Component)]
pub struct LevelModels(pub usize);

/// The way-out posts' meshes, made once.
struct PostLook {
    cube: Handle<Mesh>,
    pole: Handle<StandardMaterial>,
    sign: Handle<StandardMaterial>,
}

impl PostLook {
    fn new(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Self {
        PostLook {
            cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
            pole: materials.add(Color::srgb(0.2, 0.3, 0.22)),
            sign: materials.add(Color::srgb(0.6, 0.4, 0.2)),
        }
    }
}

/// A level's ways on and off, children of its `root`: its stairs down
/// glow where you step onto them, the top has the way-out post and the
/// J's level the ride post.
fn level_ways(
    commands: &mut Commands,
    t: &Tuning,
    levels: &[Level],
    li: usize,
    root: Entity,
    look: &mut PostLook,
) {
    use crate::features::doorway::Doorway;
    let level = &levels[li];
    if level.fill(level.goal).kind == Kind::Stairs {
        let side = dir(level.goal_entry);
        let e = crate::area::doorway(
            commands,
            cell_world(t, li, level.goal) + side * (CELL / 2.0 - 0.4),
            crate::features::doorway::yaw_towards(-side.xz()),
            Doorway::new(
                crate::features::doorway::Way::Down,
                format!("level {}", li + 2),
            )
            .size(2.6, 2.6),
        );
        commands.entity(e).insert(ChildOf(root));
    }
    let mut posts = Vec::new();
    if li == 0 {
        posts.push((start_at(t, levels), false, t.exit_label.clone()));
    }
    if let Some(at) = platform_at(t, levels) {
        posts.push((at, true, t.ride_label.clone()));
    }
    for (at, ride, label) in posts {
        let way_out = Doorway::way_out(label.clone()).size(1.6, 2.4);
        commands
            .spawn((
                Post { ride },
                way_out,
                Transform::from_translation(at + Vec3::new(1.8, 0.0, 0.0)),
                Visibility::default(),
                Clickable::at_height(1.6)
                    .radius(60.0)
                    .labelled(label.leak()),
                ChildOf(root),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(look.cube.clone()),
                    MeshMaterial3d(look.pole.clone()),
                    Transform::from_xyz(0.0, 1.1, 0.0).with_scale(Vec3::new(0.12, 2.2, 0.12)),
                ));
                p.spawn((
                    Mesh3d(look.cube.clone()),
                    MeshMaterial3d(look.sign.clone()),
                    Transform::from_xyz(0.0, 1.9, 0.0).with_scale(Vec3::new(1.2, 0.35, 0.06)),
                ));
            });
    }
}

#[derive(Component)]
struct StationLight;

fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

fn build_station(mut commands: Commands, assets: Res<AssetServer>, station: Option<Res<Station>>) {
    let t = tuning();
    let Some(station) = station else { return };
    let levels = station.levels();
    // The entrance on the sidewalk: the city's subway entrance model over
    // its hole in the street, with its stairs and walls as colliders.
    let at = entrance_at(&t);
    let turn = Quat::from_rotation_y(t.entrance_yaw.to_radians());
    commands.spawn((
        WorldAssetRoot(assets.load(
            GltfAssetLabel::Scene(0).from_asset("models/prefabs/nyc-kit-subway-entrance.gltf"),
        )),
        Transform::from_translation(at).with_rotation(turn),
        crate::area::show(),
    ));
    // The way in: a glowing threshold where the stairs start, at the open end.
    {
        use crate::area::doorway as spawn;
        use crate::features::doorway::Doorway;
        let h = &t.entrance_hole;
        spawn(
            &mut commands,
            at + turn * Vec3::new(0.0, 0.0, h.offset + h.size.1 / 2.0),
            t.entrance_yaw.to_radians(),
            Doorway::way_in("Fulton St").size(h.size.0, 2.6),
        );
    }
    commands.spawn((
        StationLight,
        PointLight {
            color: Color::srgb(0.92, 0.96, 1.0),
            intensity: t.light.follow_lumens,
            range: t.light.follow_range,
            ..default()
        },
        Transform::default(),
        Visibility::Hidden,
        crate::area::show(),
    ));
    // Its reflections: one probe baked in a hall of the top levels (or the
    // arrival), used on every level (they all look alike).
    let bake_at = levels
        .iter()
        .enumerate()
        .find_map(|(li, l)| {
            let r = l.rooms.first()?;
            let a = cell_world(&t, li, r.at);
            let b = cell_world(&t, li, (r.at.0 + r.size.0 - 1, r.at.1 + r.size.1 - 1));
            Some((a + b) / 2.0)
        })
        .unwrap_or_else(|| start_at(&t, levels))
        + Vec3::Y * 1.7;
    commands.queue(move |world: &mut World| {
        if let Some(mut b) = world.get_resource_mut::<friendslop_base::probes::ProbeBakes>() {
            b.add("fulton", bake_at);
        }
    });
}

/// The station's plan (once a game, every peer: replicated state and the
/// server's systems read it; only its top levels now, the rest as people go
/// down) and the entrance's colliders, in New York's colliders layer. The
/// levels' colliders come from [`build_levels`].
pub fn build_sim(mut commands: Commands, station: Option<Res<Station>>) {
    let t = tuning();
    if station.is_none() {
        commands.insert_resource(Station::new(&t, &TILES.get()));
    }
    for b in entrance_colliders(&t) {
        commands.spawn((
            Transform::from_translation(b.centre).with_rotation(b.rotation),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(b.size.x, b.size.y, b.size.z),
            crate::area::sim(),
        ));
    }
}

/// `FRIENDSLOP_START=fulton` starts you on the top level. Every lookup of
/// any start asks this, so the answer (the top level, planned alone) is
/// worked out once and kept.
fn starts() -> Vec<(String, Vec3)> {
    static AT: std::sync::Mutex<Option<Vec3>> = std::sync::Mutex::new(None);
    if let Ok(at) = AT.lock()
        && let Some(at) = *at
    {
        return vec![("fulton".into(), at)];
    }
    let t = tuning();
    let mut plan = StationPlan::new(&t, &TILES.get());
    plan.plan_to(&t, 1);
    let levels = plan.levels;
    let at = start_at(&t, &levels);
    // Only keep a real plan (not one made before the data files loaded).
    if !levels.is_empty()
        && let Ok(mut kept) = AT.lock()
    {
        *kept = Some(at);
    }
    vec![("fulton".into(), at)]
}

fn camera_at(camera: &Query<&GlobalTransform, With<crate::world::MainCamera>>) -> Option<Vec3> {
    camera.iter().next().map(|g| g.translation())
}

/// Hidden by the station (`place::HiddenBy`).
const HIDES: &str = "fulton";

/// Local: underground, nothing on the surface can be seen, but it was all
/// still drawn (ten thousand meshes, a few fps). While you're in the
/// station, hide every top-level thing standing above it (the city, the
/// commune, their lights), checking now and then for new ones; let it all
/// go on the way up.
#[allow(clippy::type_complexity)]
fn hide_surface(
    mut commands: Commands,
    owned: Query<
        (),
        Or<(
            With<friendslop_base::areas::AreaRoot>,
            With<friendslop_base::areas::InArea>,
            With<Replicated>,
        )>,
    >,
    time: Res<Time>,
    mut rescan: Local<Option<f32>>,
    here: Res<crate::place::Here>,
    mut roots: Query<
        (
            Entity,
            &GlobalTransform,
            Option<&mut crate::place::HiddenBy>,
        ),
        (
            With<Visibility>,
            Without<ChildOf>,
            Without<LevelModels>,
            Without<Camera>,
            Without<Node>,
            Without<DirectionalLight>,
        ),
    >,
) {
    if !in_station(here.0) {
        // Just came up: let go of everything.
        if rescan.take().is_some() {
            for (_, _, h) in &mut roots {
                if let Some(mut h) = h
                    && h.has(HIDES)
                {
                    h.remove(HIDES);
                }
            }
        }
        return;
    }
    let left = rescan.get_or_insert(0.0);
    *left -= time.delta_secs();
    if *left > 0.0 {
        return;
    }
    *left = 0.5;
    for (e, g, h) in &mut roots {
        if !out_of_sight(g.translation().y) {
            continue;
        }
        // Only what belongs to an area or is replicated (cabs, cats, other
        // players): globals (the sky, the camera's helpers) stay, or nothing
        // at all is drawn down here.
        if !owned.contains(e) {
            continue;
        }
        match h {
            Some(mut h) => {
                if !h.has(HIDES) {
                    h.add(HIDES);
                }
            }
            None => {
                commands
                    .entity(e)
                    .try_insert(crate::place::HiddenBy::new(HIDES));
            }
        }
    }
}

/// Pure: a top-level thing at this height is on the surface (out of sight from
/// the station).
pub fn surface(y: f32) -> bool {
    y > STATION_TOP_Y + STATION_LEVEL_H
}

/// Pure: a top-level thing at this height can't be seen from the station: on
/// the surface, or under its bottom (the trains' ride cars hang there).
pub fn out_of_sight(y: f32) -> bool {
    surface(y) || y < crate::rules::nyc::STATION_BOTTOM_Y
}

/// Local: draw only the levels around the camera (forty levels of tiles is a lot).
fn stream_levels(
    mut commands: Commands,
    station: Res<Station>,
    assets: Res<AssetServer>,
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    spawned: Query<(Entity, &LevelModels)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = tuning();
    let mut posts = PostLook::new(&mut meshes, &mut materials);
    let want: Vec<usize> = match camera_at(&camera) {
        Some(at) if in_station(at - Vec3::Y * 1.6) => {
            let l = level_at(at.y - 1.6);
            (l.saturating_sub(1)..=l + 1)
                .filter(|&l| l < station.levels().len())
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
        let level = &station.levels()[li];
        let root = commands
            .spawn((
                LevelModels(li),
                Transform::default(),
                Visibility::default(),
                crate::area::show(),
            ))
            .id();
        level_ways(&mut commands, &t, station.levels(), li, root, &mut posts);
        // The level's reflections.
        if !friendslop_base::probes::baking() {
            let (w, d) = (level.width as f32 * CELL, level.depth as f32 * CELL);
            let mid = cell_world(&t, li, (0, 0))
                .lerp(cell_world(&t, li, (level.width - 1, level.depth - 1)), 0.5);
            commands.spawn((
                friendslop_base::probes::probe(
                    &assets,
                    "fulton",
                    mid + Vec3::Y * (ROOM_H / 2.0),
                    Vec3::new(w, ROOM_H + 0.4, d),
                    t.light.probe,
                ),
                ChildOf(root),
            ));
        }
        for z in 0..level.depth {
            for x in 0..level.width {
                let f = level.fill((x, z));
                let file = match f.kind {
                    Kind::Corridor(Some(i)) => set.pieces.get(i as usize).map(|p| p.model.as_str()),
                    Kind::Stairs => Some(set.stairs.as_str()),
                    Kind::Platform => Some(set.platform.as_str()),
                    Kind::Room(_) => Some(set.room.floor.as_str()),
                    Kind::Corridor(None) | Kind::Landing | Kind::Under => None,
                };
                let Some(file) = file else { continue };
                let at = cell_world(&t, li, (x, z));
                let sunk = level.sunk((x, z));
                // Its ceiling tubes light it (no shadows: there are a lot of them).
                if t.light.cell_lumens > 0.0 {
                    commands.spawn((
                        CellLight,
                        PointLight {
                            color: Color::srgb(0.9, 0.95, 1.0),
                            intensity: t.light.cell_lumens,
                            range: t.light.cell_range,
                            shadow_maps_enabled: false,
                            ..default()
                        },
                        Transform::from_translation(at + Vec3::Y * t.light.cell_height),
                        ChildOf(root),
                    ));
                }
                // A sunken cell is drawn by its shapes (below).
                if sunk {
                    continue;
                }
                commands.spawn((
                    model(&assets, file),
                    Merge,
                    Transform::from_translation(at).with_rotation(f.rotation()),
                    ChildOf(root),
                ));
                // A subway platform's ad (#359): against its back wall,
                // facing the track, where the back of the deck is shut.
                if let Some(local) = ad_spot(f) {
                    commands.spawn((
                        model(&assets, AD_MODEL),
                        Transform::from_translation(at + f.rotation() * local)
                            .with_rotation(f.rotation()),
                        ChildOf(root),
                    ));
                }
                // A room cell's walls: the wall piece turned onto each walled side.
                if matches!(f.kind, Kind::Room(_)) && !set.room.wall.is_empty() {
                    for side in wfc::Side::ALL {
                        if f.mask & bit(side) != 0 {
                            continue;
                        }
                        let turn = Fill {
                            turns: side.index() as u8,
                            ..f
                        };
                        commands.spawn((
                            model(&assets, &set.room.wall),
                            Merge,
                            Transform::from_translation(at).with_rotation(turn.rotation()),
                            ChildOf(root),
                        ));
                    }
                }
            }
        }
        // Tall rooms (sunken floors, decks, stairs, railings) and rubble.
        for sh in shapes(&t, li, level) {
            let file = set.room.file(sh.piece);
            if file.is_empty() {
                continue;
            }
            commands.spawn((model(&assets, file), Merge, sh.transform, ChildOf(root)));
        }
        // The big rooms' and the corridors' dressing (benches, crates, trash...).
        for d in room_dressing(&t, li, level)
            .into_iter()
            .chain(corridor_dressing(&t, &set, li, level))
        {
            commands.spawn((
                WorldAssetRoot(
                    assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/{}", d.model))),
                ),
                d.transform,
                ChildOf(root),
            ));
        }
        if !set.room.column.is_empty() {
            for at in room_columns(&t, li, level) {
                commands.spawn((
                    model(&assets, &set.room.column),
                    Merge,
                    Transform::from_translation(at),
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
                Merge,
                Transform::from_translation(s.at).with_rotation(Quat::from_rotation_y(s.yaw)),
                ChildOf(root),
            ));
        }
    }
}

/// Local: a level's static model pieces (tiles, room walls and floors, tall
/// rooms' pieces, columns, signs), merged into one mesh per material once
/// they've all loaded, so a level is a few draws instead of hundreds.
#[derive(Component)]
struct Merge;

/// A [`Merge`] piece whose model has spawned.
#[derive(Component)]
struct MergeReady;

/// A level whose pieces have been merged.
#[derive(Component)]
struct LevelMerged;

fn merge_ready(
    ready: On<bevy::world_serialization::WorldInstanceReady>,
    mut commands: Commands,
    marked: Query<(), With<Merge>>,
) {
    if marked.contains(ready.entity) {
        commands.entity(ready.entity).try_insert(MergeReady);
    }
}

/// What makes two of the pieces' materials look the same: the files' own
/// copies of a shared material (same textures, same numbers) merge together.
fn material_key(m: &StandardMaterial) -> String {
    let id = |h: &Option<Handle<Image>>| h.as_ref().map(|h| h.id());
    format!(
        "{:?}",
        (
            (
                m.base_color,
                id(&m.base_color_texture),
                m.base_color_channel.clone()
            ),
            (
                m.emissive,
                id(&m.emissive_texture),
                m.emissive_exposure_weight
            ),
            (
                m.perceptual_roughness,
                m.metallic,
                id(&m.metallic_roughness_texture)
            ),
            (
                m.reflectance,
                id(&m.normal_map_texture),
                m.flip_normal_map_y
            ),
            (
                id(&m.occlusion_texture),
                m.double_sided,
                m.cull_mode,
                m.unlit
            ),
            (m.alpha_mode, m.depth_bias, m.uv_transform, m.fog_enabled),
            (m.diffuse_transmission, m.specular_transmission, m.clearcoat),
        )
    )
}

/// `e`'s transform into `root`'s frame (the root's own left out).
fn to_root(
    e: Entity,
    root: Entity,
    nodes: &Query<(&Transform, Option<&ChildOf>)>,
) -> bevy::math::Affine3A {
    let mut acc = bevy::math::Affine3A::IDENTITY;
    let mut at = e;
    while at != root {
        let Ok((tf, parent)) = nodes.get(at) else {
            break;
        };
        acc = tf.compute_affine() * acc;
        let Some(parent) = parent else { break };
        at = parent.parent();
    }
    acc
}

/// Local: once every [`Merge`] piece of a level has spawned, their meshes are
/// baked into one mesh per material (children of the level, so they go with
/// it) and the pieces are despawned. A mesh that can't be merged stays put.
#[allow(clippy::type_complexity)]
fn merge_levels(
    mut commands: Commands,
    levels: Query<(Entity, &Children), (With<LevelModels>, Without<LevelMerged>)>,
    pieces: Query<Has<MergeReady>, With<Merge>>,
    children: Query<&Children>,
    nodes: Query<(&Transform, Option<&ChildOf>)>,
    drawn: Query<
        (
            Option<&MeshMaterial3d<StandardMaterial>>,
            Has<bevy::mesh::skinning::SkinnedMesh>,
        ),
        With<Mesh3d>,
    >,
    mesh_of: Query<&Mesh3d>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<Assets<StandardMaterial>>,
) {
    use friendslop_structures::batch::{MeshBatch, layout};
    for (root, kids) in &levels {
        let roots: Vec<Entity> = kids.iter().filter(|&e| pieces.contains(e)).collect();
        if roots.iter().any(|&e| !matches!(pieces.get(e), Ok(true))) {
            continue;
        }
        let mut groups: Vec<(String, Handle<StandardMaterial>, MeshBatch)> = Vec::new();
        for &r in &roots {
            let mut all = true;
            let mut merged = Vec::new();
            for e in children.iter_descendants(r) {
                let Ok((mat, skinned)) = drawn.get(e) else {
                    continue;
                };
                let (Some(mat), false, Ok(m)) = (mat, skinned, mesh_of.get(e)) else {
                    all = false;
                    continue;
                };
                let (Some(mesh), Some(material)) = (meshes.get(&m.0), materials.get(&mat.0)) else {
                    all = false;
                    continue;
                };
                let Some(lay) = layout(mesh) else {
                    all = false;
                    continue;
                };
                let key = format!("{}{lay:?}", material_key(material));
                let i = match groups.iter().position(|g| g.0 == key) {
                    Some(i) => i,
                    None => {
                        groups.push((key, mat.0.clone(), MeshBatch::default()));
                        groups.len() - 1
                    }
                };
                if groups[i].2.push(mesh, to_root(e, root, &nodes)) {
                    merged.push(e);
                } else {
                    all = false;
                }
            }
            if all {
                commands.entity(r).despawn();
            } else {
                for e in merged {
                    commands.entity(e).remove::<Mesh3d>();
                }
            }
        }
        for (_, material, batch) in groups {
            if batch.is_empty() {
                continue;
            }
            commands.spawn((
                Mesh3d(meshes.add(batch.build())),
                MeshMaterial3d(material),
                Transform::default(),
                ChildOf(root),
            ));
        }
        commands.entity(root).insert(LevelMerged);
    }
}

/// A cell's ceiling light (the nearest few cast shadows).
#[derive(Component)]
struct CellLight;

/// Local: the nearest [`Lights::shadow_lights`] cell lights cast shadows.
fn light_shadows(
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    quality: Option<Res<crate::graphics::GraphicsQuality>>,
    mut lights: Query<(&GlobalTransform, &mut PointLight), With<CellLight>>,
) {
    let Some(at) = camera_at(&camera) else { return };
    let t = tuning();
    let n = if quality.is_none_or(|q| q.shadows) {
        t.light.shadow_lights
    } else {
        0
    };
    let mut all: Vec<_> = lights
        .iter_mut()
        .map(|(g, l)| (g.translation().distance_squared(at), l))
        .collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (i, (_, mut l)) in all.into_iter().enumerate() {
        let want = i < n;
        if l.shadow_maps_enabled != want {
            l.shadow_maps_enabled = want;
        }
    }
}

/// The station's look (`data/base/place_looks.ron`): no sky, little light, a dim haze.
pub const LOOK: &str = "fulton";

/// Local: inside the station the sky is out (`place.rs`).
fn station_look(here: Res<crate::place::Here>, mut looks: ResMut<crate::place::PlaceLooks>) {
    if in_station(here.0) {
        looks.push(LOOK);
    }
}

/// Local: a light that goes with you down there.
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
    let on = on_platform(&t, station.levels(), w.pos);
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
                    .replace("{of}", &t.levels.max(1).to_string()),
            );
        }
    }
    found.1 = on;
    *was = here;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_station_hides_the_surface_but_not_itself() {
        assert!(surface(CITY_Y) && surface(0.0));
        assert!(!surface(STATION_TOP_Y) && !surface(level_floor(10)));
        assert!(out_of_sight(CITY_Y) && out_of_sight(-300.0));
        assert!(!out_of_sight(level_floor(10)));
    }

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
    fn the_station_fits_its_depth_limit() {
        let (t, _) = data();
        assert!(t.levels <= crate::rules::nyc::STATION_MAX_LEVELS);
        let bottom = STATION_TOP_Y - (t.levels - 1) as f32 * STATION_LEVEL_H;
        assert!(in_station(Vec3::new(0.0, bottom, 200.0)));
    }

    #[test]
    fn the_data_files_parse_and_have_everything() {
        let (t, set) = data();
        assert!(t.levels >= 10, "an impossible number of levels");
        assert!(t.min_path >= 3 && t.reward_pelf > 0);
        assert!(
            t.light.probe > 0.0
                && t.light.cell_lumens > 0.0
                && t.light.cell_range > 0.0
                && t.light.follow_range > 0.0
        );
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
    fn platform_levels_have_track_that_joins_up_over_solid_ground() {
        let (t, set) = data();
        assert!(t.platform_every > 0 && set.pieces.iter().any(|p| !p.track.is_empty()));
        let levels = generate(&t, &set);
        let mut platform_levels = 0;
        for (li, l) in levels.iter().enumerate() {
            let tracks: Vec<wfc::Cell> = (0..l.width * l.depth)
                .map(|i| (i % l.width, i / l.width))
                .filter(|&c| l.fill(c).track != 0)
                .collect();
            if platform_level(&t, li) {
                assert!(tracks.len() >= t.platform_cells, "level {li}: {tracks:?}");
                platform_levels += 1;
            } else {
                assert!(tracks.is_empty(), "level {li}");
            }
            let d = l.distances();
            for &c in &tracks {
                assert!(
                    d[c.1 * l.width + c.0].is_some(),
                    "level {li}: {c:?} unreachable"
                );
                let f = l.fill(c);
                // Track sides meet track sides, never the grid's edge or a corridor.
                for s in wfc::Side::ALL {
                    if f.track & bit(s) == 0 {
                        continue;
                    }
                    let Some(n) = wfc::neighbour(l.width, l.depth, c, s) else {
                        panic!("level {li}: track off the grid at {c:?}");
                    };
                    assert!(l.fill(n).track & bit(s.opposite()) != 0, "level {li}");
                }
                // Under it, on the level below, solid ground (no model, no way).
                if let Some(below) = levels.get(li + 1) {
                    assert_eq!(below.fill(c).kind, Kind::Under, "level {li} {c:?}");
                    assert_eq!(below.fill(c).mask, 0);
                }
            }
            // Corridor sides never open onto track sides.
            for z in 0..l.depth {
                for x in 0..l.width {
                    for s in wfc::Side::ALL {
                        if let Some(n) = wfc::neighbour(l.width, l.depth, (x, z), s) {
                            assert_eq!(
                                socket(&l.fill((x, z)), s),
                                socket(&l.fill(n), s.opposite()),
                                "level {li} {x},{z}"
                            );
                        }
                    }
                }
            }
        }
        assert!(platform_levels >= t.levels / t.platform_every - 1);
    }

    #[test]
    fn the_track_bed_has_steps_back_up_at_a_tunnel_end() {
        let end = Fill {
            mask: bit(wfc::Side::East),
            track: bit(wfc::Side::East),
            kind: Kind::Corridor(Some(0)),
            turns: 0,
        };
        let blocks = track_blocks(end);
        // A tilted block: the steps' ramp, from the deck (y 0) to the bed.
        let ramp = blocks
            .iter()
            .find(|b| b.rotation != Quat::IDENTITY)
            .copied();
        let Some(ramp) = ramp else {
            panic!("no steps");
        };
        let top = ramp.centre + ramp.rotation * Vec3::new(0.0, 0.15, -ramp.size.z / 2.0);
        let low = ramp.centre + ramp.rotation * Vec3::new(0.0, 0.15, ramp.size.z / 2.0);
        assert!(
            top.y.abs() < 0.05 && (low.y + TRACK_DROP).abs() < 0.05,
            "{top} {low}"
        );
        assert!((low.z - PLATFORM_EDGE_Z).abs() < 0.05);
        // Not too steep to walk.
        let (rise, run) = (top.y - low.y, low.z - top.z);
        assert!(rise / run < 1.0, "{rise} over {run}");
        // A run piece has no steps.
        let run = Fill {
            mask: 0b1010,
            track: 0b1010,
            ..end
        };
        assert!(
            track_blocks(run)
                .iter()
                .all(|b| b.rotation == Quat::IDENTITY)
        );
    }

    #[test]
    fn through_platforms_get_a_gta6_ad_against_a_shut_back() {
        let run = Fill {
            mask: 0b1010,
            track: 0b1010,
            kind: Kind::Corridor(Some(0)),
            turns: 0,
        };
        let at = ad_spot(run).expect("a through platform gets an ad");
        assert!(at.z < -PLATFORM_EDGE_Z, "on the deck, not the track: {at}");
        // An open back (a way on) or a tunnel end gets none.
        assert!(
            ad_spot(Fill {
                mask: 0b1011,
                ..run
            })
            .is_none()
        );
        assert!(
            ad_spot(Fill {
                track: 0b0010,
                ..run
            })
            .is_none()
        );
    }

    #[test]
    fn big_rooms_break_up_the_corridors_and_join_the_way() {
        let (t, set) = data();
        assert!(t.rooms.chance > 0.0 && !t.rooms.sizes.is_empty());
        assert!(!set.room.floor.is_empty() && !set.room.wall.is_empty());
        let levels = generate(&t, &set);
        let mut with_rooms = 0;
        for (li, l) in levels.iter().enumerate() {
            if !l.rooms.is_empty() {
                with_rooms += 1;
            }
            let d = l.distances();
            for (ri, r) in l.rooms.iter().enumerate() {
                assert!(r.size.0 * r.size.1 >= 4, "level {li}");
                // (An L-shaped room's missing corner is corridor.)
                let mine = |c: wfc::Cell| l.fill(c).kind == Kind::Room(ri as u8);
                assert!(r.cells().filter(|&c| mine(c)).count() >= 3, "level {li}");
                for c in r.cells().filter(|&c| mine(c)) {
                    // No walls inside a room (but a tall room's decks and
                    // sunken floor meet only at stairs), and you can walk into it.
                    for s in wfc::Side::ALL {
                        if let Some(n) = wfc::neighbour(l.width, l.depth, c, s)
                            && mine(n)
                            && l.sunk(c) == l.sunk(n)
                        {
                            assert!(l.open(c, s), "level {li} {c:?} {s:?}");
                        }
                    }
                    assert!(d[c.1 * l.width + c.0].is_some(), "level {li} {c:?}");
                }
                // Not on the arrival or the stairs.
                assert!(!r.contains(l.arrive) && !r.contains(l.goal));
            }
        }
        assert!(
            with_rooms * 3 >= levels.len(),
            "{with_rooms} of {}",
            levels.len()
        );
    }

    #[test]
    fn rooms_have_kinds_and_their_dressing_keeps_the_way_clear() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let mut seen = std::collections::BTreeMap::<String, usize>::new();
        for (li, l) in levels.iter().enumerate() {
            let mut line = format!("level {li:2}:");
            for (ri, r) in l.rooms.iter().enumerate() {
                let name = room_kind(&t, li, ri, r).map_or("hall", |k| k.name.as_str());
                *seen.entry(name.to_string()).or_default() += 1;
                let mid = cell_world(&t, li, r.at).lerp(
                    cell_world(&t, li, (r.at.0 + r.size.0 - 1, r.at.1 + r.size.1 - 1)),
                    0.5,
                );
                line += &format!(" {name} {}x{} at {mid:.0}", r.size.0, r.size.1);
            }
            println!("{line}");
            let d = room_dressing(&t, li, l);
            // The same every time (every peer).
            assert_eq!(d, room_dressing(&t, li, l));
            for p in &d {
                assert!(!p.model.is_empty());
                let Some(b) = p.block else { continue };
                // Solid props stay well off a cell's middle (the walking line
                // and the wallet's spots) and inside the room.
                let o = cell_world(&t, li, (0, 0));
                let c = (
                    ((b.centre.x - o.x) / CELL).round() as usize,
                    ((b.centre.z - o.z) / CELL).round() as usize,
                );
                let mid = cell_world(&t, li, c);
                let off = (b.centre - mid).with_y(0.0);
                let reach = off.length() - b.size.x.max(b.size.z) / 2.0;
                let inward = off.x.abs().max(off.z.abs()) - b.size.z / 2.0;
                assert!(inward >= 1.5 || reach >= 1.5, "level {li} {p:?}");
                assert!(matches!(l.fill(c).kind, Kind::Room(_)), "level {li} {p:?}");
            }
        }
        println!("{seen:?}");
        for k in &t.rooms.kinds {
            assert!(seen.contains_key(&k.name), "no {} in {seen:?}", k.name);
        }
    }

    #[test]
    fn corridor_props_keep_the_way_clear() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let mut n = 0;
        for (li, l) in levels.iter().enumerate() {
            let d = corridor_dressing(&t, &set, li, l);
            assert_eq!(d, corridor_dressing(&t, &set, li, l));
            n += d.len();
            for p in &d {
                let Some(b) = p.block else { continue };
                let o = cell_world(&t, li, (0, 0));
                let c = (
                    ((b.centre.x - o.x) / CELL).round() as usize,
                    ((b.centre.z - o.z) / CELL).round() as usize,
                );
                let off = (b.centre - cell_world(&t, li, c)).with_y(0.0);
                let inward = off.x.abs().max(off.z.abs()) - b.size.z / 2.0;
                assert!(inward >= 1.5, "level {li} {p:?}");
                assert!(
                    matches!(l.fill(c).kind, Kind::Corridor(Some(_))),
                    "level {li} {p:?}"
                );
            }
        }
        assert!(n > 0, "no corridor props at all");
    }

    #[test]
    fn planning_a_level_at_a_time_makes_the_same_station() {
        let (t, set) = data();
        let all = generate(&t, &set);
        let mut plan = StationPlan::new(&t, &set);
        plan.plan_to(&t, t.plan_ahead);
        assert_eq!(plan.levels.len(), t.plan_ahead);
        // Some now, more later, one at a time, as people go down.
        while plan.plan_next(&t).is_some() {
            if plan.levels.len() == 7 {
                plan.plan_to(&t, 12);
            }
        }
        assert!(plan.done());
        assert_eq!(plan.levels.len(), all.len());
        for (a, b) in plan.levels.iter().zip(&all) {
            assert!(a.cells == b.cells && a.path == b.path);
            assert_eq!(
                (a.arrive, a.goal, a.goal_entry),
                (b.arrive, b.goal, b.goal_entry)
            );
        }
        // The J only shows up once its level is planned.
        assert!(platform_at(&t, &all[..all.len() - 1]).is_none());
        assert!(platform_at(&t, &all).is_some());
    }

    #[test]
    fn levels_are_planned_and_built_ahead_of_the_deepest() {
        let (t, _) = data();
        assert!(
            t.plan_ahead >= 2,
            "the level under you is built before you drop"
        );
        assert_eq!(levels_wanted(&t, None, 0), t.plan_ahead);
        assert_eq!(levels_wanted(&t, Some(5), 0), 6 + t.plan_ahead);
        assert_eq!(levels_wanted(&t, Some(1), 12), 12);
        assert_eq!(levels_wanted(&t, Some(t.levels - 1), 0), t.levels);
        let built = levels_built(&t, &[10], 40);
        for l in 9..=10 + t.plan_ahead {
            assert!(built.contains(&l), "{l}");
        }
        assert!(built.contains(&0) && !built.contains(&20));
        assert!(!levels_built(&t, &[10], 11).contains(&11));
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
        // And the same tall rooms, stairs and rubble.
        for (li, (a, b)) in a.iter().zip(&b).enumerate() {
            assert_eq!(
                (&a.sunk, &a.stairs, &a.rubble, &a.squeeze),
                (&b.sunk, &b.stairs, &b.rubble, &b.squeeze)
            );
            assert_eq!(shapes(&t, li, a), shapes(&t, li, b));
        }
    }

    #[test]
    fn l_shaped_rooms_appear_and_join_the_way() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let mut l_rooms = 0;
        for l in &levels {
            let d = l.distances();
            for (ri, r) in l.rooms.iter().enumerate() {
                let cells: Vec<_> = r
                    .cells()
                    .filter(|&c| l.fill(c).kind == Kind::Room(ri as u8))
                    .collect();
                if cells.len() < r.size.0 * r.size.1 {
                    l_rooms += 1;
                    // An L: the rectangle less one corner block.
                    assert!(cells.len() >= 3);
                }
                for c in cells {
                    assert!(d[c.1 * l.width + c.0].is_some(), "room cell {c:?} cut off");
                }
            }
        }
        println!("L-shaped rooms: {l_rooms}");
        assert!(l_rooms >= 3, "only {l_rooms} L-shaped rooms");
    }

    #[test]
    fn collapses_never_cut_the_way_or_anything_off() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let (mut blocked, mut squeezes) = (0, 0);
        for (li, l) in levels.iter().enumerate() {
            blocked += l.rubble.iter().map(|b| b.count_ones()).sum::<u32>() / 2;
            squeezes += l.squeeze.iter().map(|b| b.count_ones()).sum::<u32>() / 2;
            // The way down is still there, and signed.
            assert_eq!(l.path.first(), Some(&l.arrive), "level {li}");
            assert_eq!(l.path.last(), Some(&l.goal), "level {li}");
            // Everything you could reach without the rubble you still can.
            let clear = Level {
                rubble: Vec::new(),
                ..l.clone()
            };
            let (with, without) = (l.distances(), clear.distances());
            for (a, b) in with.iter().zip(&without) {
                assert_eq!(a.is_some(), b.is_some(), "level {li} cut off");
            }
            // Rubble only across corridors, and nothing walks through it.
            for i in 0..l.width * l.depth {
                let c = (i % l.width, i / l.width);
                for s in wfc::Side::ALL {
                    if l.rubble(c, s) {
                        assert!(!l.open(c, s));
                        assert!(matches!(l.fill(c).kind, Kind::Corridor(_)));
                        assert!(
                            !crate::features::fulton_hunt::neighbours(&levels, (li, c.0, c.1))
                                .iter()
                                .any(|n| wfc::neighbour(l.width, l.depth, c, s)
                                    == Some((n.1, n.2))
                                    && n.0 == li)
                        );
                    }
                }
            }
        }
        println!("blocked {blocked}, squeezes {squeezes}");
        assert!(blocked > 0 && squeezes > 0);
    }

    #[test]
    fn mezzanines_are_reachable_by_stairs_you_can_walk() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let (mut tall, mut stairs) = (0, 0);
        for (li, l) in levels.iter().enumerate() {
            let d = l.distances();
            let sunk: Vec<usize> = (0..l.sunk.len()).filter(|&i| l.sunk[i]).collect();
            if sunk.is_empty() {
                continue;
            }
            tall += 1;
            for &i in &sunk {
                assert!(d[i].is_some(), "level {li}: sunken cell {i} cut off");
                assert!(matches!(l.cells[i].kind, Kind::Room(_)));
            }
            // The level below is solid under them.
            if let Some(below) = levels.get(li + 1) {
                for &i in &sunk {
                    assert_eq!(below.cells[i].kind, Kind::Under, "level {}", li + 1);
                }
            }
            for sh in shapes(&t, li, l) {
                if sh.piece != ShapePiece::Stairs {
                    continue;
                }
                stairs += 1;
                // Eight steps no taller than the controller climbs, a slope it walks.
                assert!(l.sink / 8.0 <= 0.3);
                assert!((l.sink / STAIRS_RUN).atan().to_degrees() <= 40.0);
                // The ramp's top meets the deck and its foot the sunken floor.
                let Some(b) = sh.block else {
                    panic!("stairs with no collider")
                };
                let top_at = |z: f32| {
                    let p = b.centre + b.rotation * Vec3::new(0.0, 0.15, z);
                    p.y - level_floor(li)
                };
                let half = b.size.z / 2.0;
                assert!(top_at(-half).abs() < 0.1, "{}", top_at(-half));
                assert!((top_at(half) + l.sink).abs() < 0.1, "{}", top_at(half));
                // And the floor heights agree.
                let foot =
                    sh.transform.translation + sh.transform.rotation * Vec3::Z * (STAIRS_RUN - 0.3);
                assert!((floor_at(&t, &levels, foot) - level_floor(li) + l.sink).abs() < 0.3);
            }
        }
        println!("tall rooms on {tall} levels, {stairs} stairs");
        assert!(tall >= 3 && stairs >= tall);
    }

    /// How tall each piece is as modelled (its top cap, before scaling).
    fn piece_top(p: ShapePiece) -> Option<f32> {
        match p {
            ShapePiece::Wall | ShapePiece::Ceiling => Some(3.99),
            ShapePiece::WallPlain => Some(1.0),
            ShapePiece::Rail => Some(1.025),
            _ => None,
        }
    }

    #[test]
    fn no_piece_lies_flush_in_the_floor_above_or_is_flattened() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        for (li, l) in levels.iter().enumerate() {
            let above = level_floor(li) + STATION_LEVEL_H;
            for sh in shapes(&t, li, l) {
                let s = sh.transform.scale;
                assert!(
                    s.min_element() > 0.05,
                    "level {li}: {:?} squashed to {s}",
                    sh.piece
                );
                let Some(top) = piece_top(sh.piece) else {
                    continue;
                };
                let top = sh.transform.translation.y + top * s.y;
                assert!(
                    top < above - 0.005,
                    "level {li}: {:?} tops out at {top}, flush with the floor above ({above}) at {}",
                    sh.piece,
                    sh.transform.translation
                );
            }
        }
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
        // You go down the stairs, not on the street above them.
        assert!(!entering(&t, entrance_at(&t)));
        assert!(entering(&t, entrance_at(&t) - Vec3::Y * 2.5));
        assert!(!entering(&t, street_at(&t)));
        assert!(!entering(&t, street_at(&t) - Vec3::Y * 2.5));
        // The hole is under the stairs, not where you come back up.
        let hole = entrance_hole(&t);
        assert!(hole.contains(entrance_at(&t).xz()));
        assert!(!hole.contains(street_at(&t).xz()));
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
        let mut s = Stats::roll([0.0; 6]);
        s.pelf = 4;
        s.earn_pelf(3);
        assert_eq!(s.pelf, 7);
        s.earn_pelf(3);
        assert_eq!(s.pelf, 10);
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
                    track: 0,
                    kind: Kind::Corridor(None),
                    turns: 0
                };
                9
            ],
            arrive: (0, 0),
            goal: (2, 2),
            goal_entry: wfc::Side::North,
            path: vec![(0, 0), (1, 0), (2, 0), (2, 1), (2, 2)],
            ..Level::empty(3, 3)
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
