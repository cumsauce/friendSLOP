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
//! Train tunnels ("create some new tiles for train tunnels ... much larger
//! tile size ... curves and straight sections ... forced to use them if a
//! tunnel is collapsed ... fatal to be chased with no stamina"): every
//! [`Tunnels::every`] levels the stairs drop you into a tunnel level instead
//! of a hall maze. It is a ring of 12 m double-track tiles (2x2 cells; straights
//! and curves, `fs-tunnel-*`) round the edge of the grid, with the stairs
//! landing on one side of the ring and the way down on the other: two arms
//! that each run at least 36 m. A cave-in ([`Tunnels::blocked`]) fills one arm;
//! then that arm is shut and the other is the only way. Nothing else is down
//! there, so a long run with no stamina is the whole level.
//!
//! The reward (server): reaching the J platform raises your Pelf (the stats'
//! wealth, `stats.rs`) by [`Tuning::reward_pelf`], once per player per game.
//! A post on the platform rides the J back up to the street, and one on the
//! top level climbs back out.
//!
//! Level types (set pieces, coarse first): each level is a type picked from
//! `data/side/fulton_layout.ron` (a warren of corridors, a big tall hall, an
//! atrium whose floor drops a whole level, or on a platform level a long
//! line of track) with a footprint of its own, centred under the station on
//! one shared lattice of cells ([`Level::origin`], [`map_cell`]). A small
//! WFC over macro cells ([`coarse`]) places the set piece, which is pinned
//! before the fine WFC runs the corridors up to it. The stairs always go in
//! the core every level shares, so they land on the next whatever its size.
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

mod graph;

/// How far in from a cell's edge its walls' faces are (the tile kit's walls
/// stand 2.8 m from a cell's middle).
const WALL_FACE_IN: f32 = 0.2;
/// Metres per cell (the tile kit's size).
pub const CELL: f32 = 6.0;
/// Floor to ceiling inside a level.
const ROOM_H: f32 = 4.0;

fn default_downs() -> (usize, usize) {
    (1, 1)
}

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
    /// A level's tiles are merged into one mesh per material for each block
    /// of this many cells square (so a light's shadow only draws the blocks
    /// near it, not the whole level).
    pub merge_cells: usize,
    /// Cells across (x) of the core every level shares (where the stairs go;
    /// the tunnel levels' size). Levels may be bigger: `fulton_layout.ron`.
    pub width: usize,
    /// Cells deep (z) of the core.
    pub depth: usize,
    /// The signed way across one level is at least this many cells long.
    pub min_path: u32,
    /// Ways down per level (min, max): one is the signed main stairs, the
    /// rest more stairs elsewhere in the core, each landing somewhere else
    /// on the level below. (Never above a tunnel level or on the J's.)
    #[serde(default = "default_downs")]
    pub downs: (usize, usize),
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
    /// Cars left standing on the platforms' tracks.
    #[serde(default)]
    pub trains: Trains,
    /// Train tunnel levels: a ring of big tiles with cave-ins.
    #[serde(default)]
    pub tunnels: Tunnels,
    /// Train tunnels running on from the platforms' tunnel mouths.
    #[serde(default)]
    pub rails: RailsAsk,
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
    /// ...ringing this many seconds (RT60).
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
    /// Only the nearest this many cell lights are on (the rest of a big,
    /// bright level's tubes still glow, they just light nothing): hundreds
    /// of lights overflowed the renderer's light lists.
    pub max_lights: usize,
    /// The shadow tubes' cone: its half-angle (degrees, under 89).
    pub tube_angle: f32,
    /// The share of cells whose ceiling light still works (the rest are broken).
    pub working: f32,
    /// From this level down (0 is the top), a level may be pitch dark: no
    /// working lights at all, no glow around you, only your flashlight.
    pub dark_from: usize,
    /// The chance a level from `dark_from` down is pitch dark.
    pub dark_chance: f32,
    /// Your flashlight in the station: lumens, reach (m), and its cone's
    /// outer and inner half-angles (degrees).
    pub flashlight_lumens: f32,
    pub flashlight_range: f32,
    pub flashlight_outer: f32,
    pub flashlight_inner: f32,
    /// A broken light's skin: its tubes stop glowing and their colour is
    /// scaled by this (dead, dusty glass).
    pub broken_tint: f32,
    /// The top levels, from the top: each one's own state of repair, before the
    /// station goes bad. Levels below the list use the settings above.
    pub stages: Vec<Stage>,
}

/// How far gone one of the top levels is.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Stage {
    /// The share of its ceiling lights that work.
    pub working: f32,
    /// How it looks (data/base/place_looks.ron).
    #[serde(default)]
    pub look: StageLook,
    /// How much of the usual rubble it has (0 none, 1 all).
    pub rubble: f32,
    /// The creepy station music plays here (else the city's music goes on).
    pub creepy: bool,
}

/// A top level's look: kept up (`fulton_kept`), run down (`fulton_worn`),
/// or gone bad like the rest (`fulton`).
#[derive(Deserialize, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageLook {
    Kept,
    Worn,
    #[default]
    Bad,
}

impl StageLook {
    /// Its id in data/base/place_looks.ron, over the station's own.
    pub fn id(self) -> Option<&'static str> {
        match self {
            StageLook::Kept => Some(LOOK_KEPT),
            StageLook::Worn => Some(LOOK_WORN),
            StageLook::Bad => None,
        }
    }
}

/// One level's state of repair: its stage if it's one of the top ones,
/// else the station's usual (gone bad) settings.
pub fn stage(t: &Tuning, level: usize) -> Stage {
    t.light.stages.get(level).cloned().unwrap_or(Stage {
        working: t.light.working,
        look: StageLook::Bad,
        rubble: 1.0,
        creepy: true,
    })
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

/// Train tunnels off the platforms (`fs-rails-*`, one cell across).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct RailsAsk {
    /// How many cells a train tunnel runs on from a platform's tunnel mouth
    /// (at least, at most; 0: none), winding through the rock...
    pub len: (usize, usize),
    /// ...turning at each cell with this chance.
    pub turn: f32,
}

/// Train tunnel levels (`fs-tunnel-*` tiles, [`TUNNEL_CELLS`] cells across).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tunnels {
    /// Every this many levels is a tunnel level (0: none)...
    pub every: usize,
    /// ...counting from this level (0 is the top). Pick one that isn't a
    /// platform level's neighbour below (their track beds sink into it).
    pub offset: usize,
    /// The chance each straight tile has caved in across (only where the
    /// other arm of the ring is still open; elsewhere it's a squeeze).
    pub blocked: f32,
    /// The chance each straight tile is a squeeze instead.
    pub squeeze: f32,
}

/// Cars left standing on the tracks (models 18 m long along x, 3.05 m
/// wide, wheels on the rails at y = 0, their +z side the platform side).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Trains {
    /// The chance of a car on each straight run of three platform cells.
    pub chance: f32,
    /// Which car, by weight.
    pub cars: Vec<TrainCar>,
}

/// One kind of standing car.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct TrainCar {
    /// Under `assets/models/`.
    pub model: String,
    pub weight: f32,
    /// Its doors on the platform side are open and you can walk through it
    /// (floor 1.1 m up, level with the platform); otherwise it's solid.
    #[serde(default)]
    pub open: bool,
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
    /// Only a level's set piece (a big hall or an atrium) is one of these,
    /// and a set piece is only one of these.
    #[serde(default)]
    pub set_piece: bool,
    /// Its dressing.
    #[serde(default)]
    pub props: Vec<RoomProp>,
    /// The first level (0 is the top) it turns up on: the deeper kinds.
    #[serde(default)]
    pub from: usize,
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
    pub voice: Vec<crate::speech::Preset>,
    /// Seconds after the last call before the thing he sent arrives.
    pub spawn_delay: f32,
    /// Its speed wandering and searching, and chasing (m/s; you walk 5, run 10.5).
    pub walk_speed: f32,
    pub chase_speed: f32,
    /// Rubber band: while you can't see it and it's after you (chasing,
    /// searching or drifting your way), it goes this many times its speed...
    pub rubber_base: f32,
    /// ...plus this much more for every second you've kept running (out of its
    /// sight and yours)...
    pub rubber_per_run_sec: f32,
    /// ...up to this many times its speed in all.
    pub rubber_max: f32,
    /// How wide your own view is (degrees) for "you can't see it".
    pub player_view_degrees: f32,
    /// What it sees: how far (m) and how wide a cone (degrees).
    pub view_range: f32,
    pub view_degrees: f32,
    /// It gets you this close (m).
    pub catch_range: f32,
    /// The director: every so often it hears roughly where you are and
    /// wanders to a cell within this many cells of you.
    pub director_secs: f32,
    pub director_radius: u32,
    /// Hold it in clear sight this long and it stops walking: it curls into
    /// a ball and rolls at you, faster than you can run.
    pub roll_sight_secs: f32,
    /// When it first sees you it stops and stares this long (head on one
    /// side, a sting) before it comes; break its sight and it comes to look.
    #[serde(default)]
    pub notice_secs: f32,
    /// How fast the ball rolls (m/s).
    pub roll_speed: f32,
    /// How long a roll lasts before it breaks back into a run.
    pub roll_secs: f32,
    /// Seconds before it may roll again.
    pub roll_cooldown_secs: f32,
    /// How fast the hunter turns to face where it's going (degrees a second).
    pub turn_speed: f32,
    /// The turn rate at chase speed (degrees a second): between walk and chase
    /// speed it blends from `turn_speed` down to this, so a runner curves.
    pub run_turn_speed: f32,
    /// Metres between footfalls walking: half the walk clip's stride (its
    /// ground speed in gaits.ron times half its cycle), so steps match the feet.
    pub walk_step_m: f32,
    /// Metres between footfalls running (half the run clip's stride).
    pub run_step_m: f32,
    /// A turn wider than this (degrees) slows a fast hunter down...
    pub slow_turn_degrees: f32,
    /// ...down to this fraction of its speed for a full about-turn (at chase
    /// speed; walking barely slows).
    pub turn_slow_min: f32,
    /// How fast it speeds up and slows down (m/s per second).
    pub accel: f32,
    pub decel: f32,
    /// It eases to a stop over this many seconds of travel as it reaches where
    /// the server has it.
    pub follow_secs: f32,
    /// Fallen behind the server by more than this (m), it hurries by this many
    /// extra m/s for each metre further back.
    pub catch_up_slack: f32,
    pub catch_up_gain: f32,
    /// The body never goes faster than this many times its mode's speed, however
    /// far behind the server it is (past 6 m it jumps there instead).
    pub max_hurry: f32,
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
    /// After backing off, the director leaves you be this many more seconds:
    /// it wanders at least `calm_cells` from you (and always while you hide).
    pub calm_secs: f32,
    pub calm_cells: u32,
    /// How far (cells) it looks for somewhere to wander to.
    pub wander_cells: u32,
    /// The music (the hunter's score): a hunter chasing or rolling within this many
    /// metres of you counts as after you (chase music, a stinger as it starts)...
    pub music_chase_range: f32,
    /// ...and for this many seconds after it stops, the music stays tense.
    pub music_tense_secs: f32,
    /// The hunter's rigged model (it needs the clips walk, run, roll, search
    /// and kill; one without search / kill just holds its pose for those).
    pub model: String,
    /// Seconds it spends prising a locker open and peering in (its `search`
    /// clip) before you're found or it moves on.
    pub check_secs: f32,
    /// Seconds its kill plays (the `kill` clip) before you're dragged back
    /// up to the street.
    pub kill_secs: f32,
    /// How far in front of it (m) you're held while it kills you.
    pub kill_reach: f32,
    /// Its service doors and the ducts behind them.
    #[serde(default)]
    pub ducts: Ducts,
}

/// The service doors on every level and the ducts that join them: a way
/// round only the hunter takes (the vents in the walls).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Ducts {
    /// Service doors per level (min, max), spread out over it.
    pub doors: (usize, usize),
    /// How fast it crawls through a duct (m/s).
    pub speed: f32,
    /// A duct counts as this many cells' walk when it picks a way round, so
    /// it takes one only to save a long way.
    pub cost: u32,
    /// Seconds between the bangs and scrapes you hear while it's in one.
    pub bang_secs: f32,
    /// The service door's model (`assets/models/`).
    pub model: String,
    /// While it drifts: the chance it takes a duct out of a door near you
    /// (within `show_cells` steps, on your level), so you can see it come out.
    #[serde(default)]
    pub show_chance: f32,
    #[serde(default)]
    pub show_cells: u32,
    /// Seconds it takes to squeeze out of a service door.
    #[serde(default)]
    pub emerge_secs: f32,
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
    /// A tunnel end's model with its tunnel mouth open, for when a train
    /// tunnel runs on from it ([`Kind::Rails`]).
    #[serde(default)]
    pub mouth_open: String,
}

/// How far the track bed lies below the platform (m): the rails' tops are
/// 1.1 m down, so a car's floor is level with the platform, as in New York.
pub const TRACK_DROP: f32 = 1.38;
/// The platform's edge (its own frame, +z): the deck is N of it, the bed S.
/// The bed (3.4 m) takes a 10 ft (3.05 m) B Division car like the J's, with
/// a 10 cm gap at the platform and room by the far wall.
pub const PLATFORM_EDGE_Z: f32 = -0.6;
/// Where a car on the track stands (own frame): its middle, across, and
/// the rails' tops (the car models' y = 0).
pub const CAR_Z: f32 = 1.025;
pub const RAIL_TOP: f32 = -1.1;
/// The steps down to the track at a platform's tunnel end (its own frame):
/// across x, and from the deck (-z, high) down to the bed at the edge.
pub const TRACK_STEPS_X: (f32, f32) = (0.6, 2.4);
pub const TRACK_STEPS_Z: f32 = -2.25;
/// Where a tunnel end's platform stops (own frame, x): the tunnel is W of it.
pub const TUNNEL_PORTAL_X: f32 = -1.2;
/// The columns on a platform (own frame, x, z).
pub const COLUMNS: [(f32, f32); 2] = [(-2.4, -2.55), (2.4, -2.55)];

/// Where a hiding piece's hiding spot is, in its own frame (inside the middle locker).
pub const HIDE_AT: Vec3 = Vec3::new(0.0, 0.0, 2.48);
/// The x of each of the lockers bank's seven doors, in the piece's own frame
/// (each 0.66 m wide, spaced 0.7 m, against the piece's S wall).
pub const LOCKER_DOORS_X: [f32; 7] = [-2.1, -1.4, -0.7, 0.0, 0.7, 1.4, 2.1];
/// The bank's collider, in the piece's own frame: its centre, then its full
/// size (the seven doors, their backs against the wall).
pub const LOCKERS_BLOCK: (Vec3, Vec3) = (Vec3::new(0.0, 1.05, 2.48), Vec3::new(4.9, 2.1, 0.64));

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
    /// The train tunnels' tiles.
    #[serde(default)]
    pub tunnel: TunnelKit,
    /// Looks that take over the corridors as you go down, weirder each time.
    #[serde(default)]
    pub themes: Vec<Theme>,
    /// The train tunnels running on from the platforms.
    #[serde(default)]
    pub rails: RailsKit,
    /// The skins: from a depth down, every corridor, room and gallery cell
    /// is drawn as the skin's floor plus one of its walls on each walled
    /// side, so the same plan takes any skin and walls always match.
    #[serde(default)]
    pub skins: Vec<Skin>,
}

/// A train tunnel's pieces ([`Kind::Rails`]), each with its origin on the
/// track bed: the straight (rails along x), the curve (rails from the N edge
/// to the E edge), the wall (along the N edge, bed to ceiling) and the buffer
/// stop at a dead end (facing +x, along its rails).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct RailsKit {
    pub straight: String,
    pub curve: String,
    pub wall: String,
    pub buffer: String,
}

/// A skin (see [`TileSet::skins`]): models one cell across, under
/// `assets/models/features/`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Skin {
    pub name: String,
    /// The first level (0 is the top) that has it.
    pub from: usize,
    /// The cell lights' colour (r, g, b), if not the default cold white.
    #[serde(default)]
    pub light: Option<(f32, f32, f32)>,
    /// A cell's floor and ceiling (with its light fitting), no walls.
    pub floor: String,
    /// Walls, each modelled along the cell's N edge facing in, and how often
    /// each is picked.
    pub walls: Vec<(String, f32)>,
    /// Its column (corners, rooms, galleries), modelled on the origin.
    #[serde(default)]
    pub column: String,
}

impl Skin {
    /// The wall on side `side` of cell `c` of level `li`: picked by weight,
    /// the same every time.
    pub fn wall(&self, t: &Tuning, li: usize, c: wfc::Cell, side: wfc::Side) -> &str {
        let total: f32 = self.walls.iter().map(|w| w.1.max(0.0)).sum();
        let mut pick = dress_roll(t, [li, c.0 * 131 + c.1, side.index(), 0x5c1]) * total;
        for (file, w) in &self.walls {
            if pick < *w {
                return file;
            }
            pick -= w.max(0.0);
        }
        self.walls.last().map_or("", |w| w.0.as_str())
    }
}

/// A depth look: from level `from` down, corridor pieces named in `swap` are
/// drawn with another model (colliders and layout come from the plan, so a
/// theme is only a skin), and the cell lights take on `light`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Theme {
    pub name: String,
    /// The first level (0 is the top) that has it.
    pub from: usize,
    /// (default model, this theme's model); pieces not listed keep theirs.
    #[serde(default)]
    pub swap: Vec<(String, String)>,
    /// The cell lights' colour (r, g, b), if not the default cold white.
    #[serde(default)]
    pub light: Option<(f32, f32, f32)>,
}

impl TileSet {
    /// The theme level `li` is in: the deepest one whose `from` it has reached.
    pub fn theme(&self, li: usize) -> Option<&Theme> {
        self.themes
            .iter()
            .filter(|t| t.from <= li)
            .max_by_key(|t| t.from)
    }

    /// The skin level `li` is drawn in: the deepest one whose `from` it has reached.
    pub fn skin(&self, li: usize) -> Option<&Skin> {
        self.skins
            .iter()
            .filter(|s| s.from <= li && !s.floor.is_empty() && !s.walls.is_empty())
            .max_by_key(|s| s.from)
    }

    /// The model a corridor piece is drawn with on level `li`.
    pub fn themed<'a>(&'a self, li: usize, model: &'a str) -> &'a str {
        self.theme(li)
            .and_then(|t| t.swap.iter().find(|(from, _)| from == model))
            .map_or(model, |(_, to)| to.as_str())
    }
}

/// The tunnel tiles: each a model [`TUNNEL_CELLS`] cells across (12 m), walls,
/// rails and columns only (the floor and ceiling are the room floor's, a
/// cell at a time), centred on the tile's middle with its floor at y = 0.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct TunnelKit {
    /// Open N and S as modelled; tracks at x = -3 and 3, columns down the middle.
    pub straight: String,
    /// Open N and E as modelled: tracks bend round its NE corner, the outer
    /// corner (SW) rounded off with a 6 m radius.
    pub curve: String,
    /// A straight with a 6 m doorway in its W wall's N half (onto the hall in
    /// the ring's middle, where the stairs are), and its mirror image (the S half).
    #[serde(default)]
    pub door: String,
    #[serde(default)]
    pub door_mirror: String,
}

impl TunnelKit {
    pub fn file(&self, piece: TunnelPiece) -> &str {
        match piece {
            TunnelPiece::Straight => &self.straight,
            TunnelPiece::Curve => &self.curve,
        }
    }
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
    /// A gallery curve's rounded outer wall, modelled round the 12 m tile's
    /// middle (origin) through its SW quadrant, as the tunnel curve's.
    #[serde(default)]
    pub arc: String,
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
    /// A cell of a train tunnel tile (see [`Level::tunnel`]); open on every
    /// side inside its tile.
    Tunnel,
    /// A cell of a gallery: a tunnel tile without the track (see
    /// [`Level::galleries`]), walled like a room.
    Gallery,
    /// A cell of a train tunnel running on from a platform's tunnel mouth:
    /// its floor is the track bed, [`TRACK_DROP`] down.
    Rails,
}

/// How many cells across a tunnel tile is.
pub const TUNNEL_CELLS: usize = 2;
/// The tunnel tile's size (m).
pub const TUNNEL_TILE: f32 = CELL * TUNNEL_CELLS as f32;

/// A tunnel tile's shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TunnelPiece {
    Straight,
    /// A quarter turn.
    Curve,
}

/// One tunnel tile on a level: its top-left cell, its shape and quarter turns
/// clockwise (like a [`Fill`]'s).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TunnelTile {
    pub at: wfc::Cell,
    pub piece: TunnelPiece,
    pub turns: u8,
}

impl TunnelTile {
    /// The sides it carries on through, as modelled (N-S; N-E) and turned.
    pub fn mask(self) -> u8 {
        let m = match self.piece {
            TunnelPiece::Straight => bit(wfc::Side::North) | bit(wfc::Side::South),
            TunnelPiece::Curve => bit(wfc::Side::North) | bit(wfc::Side::East),
        };
        turned(m, self.turns)
    }

    pub fn rotation(self) -> Quat {
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2 * self.turns as f32)
    }
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
    /// A tunnel level's tiles (the ring); empty on every other level.
    pub tunnel: Vec<TunnelTile>,
    /// A concourse level's galleries' tiles (no track; cells `Kind::Gallery`).
    pub galleries: Vec<TunnelTile>,
    /// Doorways into the floor of the level above's two-storey room (its
    /// sunken floor is this level's floor): the corridor cell here and its
    /// side that opens into the room (whose cells here are `Kind::Under`).
    pub from_above: Vec<(wfc::Cell, wfc::Side)>,
    /// This level's two-storey room's doorways down there: its sunken cell
    /// and the side with the doorway in its lower wall (from the level
    /// below's [`Level::from_above`], once that is planned).
    pub low_doors: Vec<(wfc::Cell, wfc::Side)>,
    /// What the level is (its type).
    pub kind: LevelKind,
    /// Its coarse layout (`None`: corridors and small rooms only).
    pub coarse: Option<Coarse>,
    /// Per room: what it is (Warren for the small rooms among the
    /// corridors, Hall or Atrium for a set piece).
    pub room_meta: Vec<Meta>,
    /// How far the level above's floor sinks into this one's solid cells (m).
    pub above_drop: f32,
    /// A tall room's stairs' run (m).
    pub run: f32,
    /// The footprint the level below will have: what sinks into it must
    /// leave it in one piece there too.
    pub below: Vec<(usize, usize)>,
    /// Its service doors (staff only, the hunter's way into the ducts): the
    /// cell and the walled side the door is in.
    pub service: Vec<(wfc::Cell, wfc::Side)>,
    /// Where its cell (0, 0) is on the station's lattice of cells (whose
    /// cell (0, 0) has its north-west corner on `station_at`): levels of
    /// different sizes, all centred there, line up cell for cell.
    pub origin: (i32, i32),
}

impl Level {
    /// No cells yet (a base for the plan's `..`).
    pub fn empty(w: usize, h: usize) -> Level {
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
            tunnel: Vec::new(),
            galleries: Vec::new(),
            from_above: Vec::new(),
            low_doors: Vec::new(),
            kind: LevelKind::Warren,
            coarse: None,
            room_meta: Vec::new(),
            above_drop: 0.0,
            run: STAIRS_RUN,
            origin: centred((w, h)),
            below: Vec::new(),
            service: Vec::new(),
        }
    }

    /// Every way down: the stairs' cells and the side each is walked into from
    /// (the main one, [`Level::goal`], among them).
    pub fn downs(&self) -> Vec<(wfc::Cell, wfc::Side)> {
        self.with_kind(Kind::Stairs)
    }

    /// Every way up: the landings' cells and the side each opens onto.
    pub fn ups(&self) -> Vec<(wfc::Cell, wfc::Side)> {
        self.with_kind(Kind::Landing)
    }

    fn with_kind(&self, kind: Kind) -> Vec<(wfc::Cell, wfc::Side)> {
        (0..self.cells.len())
            .filter(|&i| self.cells[i].kind == kind)
            .filter_map(|i| {
                let f = self.cells[i];
                let s = wfc::Side::ALL.into_iter().find(|&s| f.mask & bit(s) != 0)?;
                Some(((i % self.width, i / self.width), s))
            })
            .collect()
    }

    /// The core every level shares, empty (the tunnel levels' frame).
    pub fn core(t: &Tuning) -> Level {
        Level::empty(t.width.max(3), t.depth.max(3))
    }

    /// Cell `c` on the station's lattice.
    pub fn lattice(&self, c: wfc::Cell) -> (i32, i32) {
        (c.0 as i32 + self.origin.0, c.1 as i32 + self.origin.1)
    }

    /// The cell at lattice cell `p`, if it's on this level.
    pub fn local(&self, p: (i32, i32)) -> Option<wfc::Cell> {
        let (x, z) = (p.0 - self.origin.0, p.1 - self.origin.1);
        (x >= 0 && z >= 0 && (x as usize) < self.width && (z as usize) < self.depth)
            .then_some((x as usize, z as usize))
    }

    /// Is room `ri` the level's set piece (a big hall or an atrium)?
    pub fn set_piece(&self, ri: usize) -> bool {
        self.room_meta.get(ri).is_some_and(|m| *m != Meta::Warren)
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
        if self.sunk(c) {
            self.sink
        } else if self.fill(c).kind == Kind::Rails {
            TRACK_DROP
        } else {
            0.0
        }
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

/// The lattice cell of a `w × h` level's cell (0, 0), centred under the station.
pub fn centred((w, h): (usize, usize)) -> (i32, i32) {
    (-((w / 2) as i32), -((h / 2) as i32))
}

/// Cell `c` of level `from` on level `to` (the same place in the world),
/// if `to` reaches there. Levels differ in size but share one lattice.
pub fn map_cell(from: &Level, to: &Level, c: wfc::Cell) -> Option<wfc::Cell> {
    to.local(from.lattice(c))
}

/// A level type's footprint on level `li` (cells across, deep): one of its
/// sizes (maybe turned round), never smaller than the `core` every level
/// shares (where the stairs are, so they always land on the next) nor
/// wider than `reach` cells either side of the station's middle.
fn footprint(
    t: &Tuning,
    ty: &LevelType,
    li: usize,
    core: (usize, usize),
    reach: usize,
) -> (usize, usize) {
    let fits: Vec<(usize, usize)> = ty
        .sizes
        .iter()
        .flat_map(|&(a, b)| [(a, b), (b, a)])
        .filter(|&(a, b)| a >= core.0 && b >= core.1 && a <= 2 * reach && b <= 2 * reach)
        .collect();
    if fits.is_empty() {
        return core;
    }
    fits[((dress_roll(t, [li, 0xf007, 0, 0]) * fits.len() as f32) as usize).min(fits.len() - 1)]
}

/// How big level `li` is: its type's footprint, or the `core` over (and on)
/// a tunnel level. Pure: the level above checks what sinks into it.
fn level_size(t: &Tuning, layout: &Layout, li: usize, core: (usize, usize)) -> (usize, usize) {
    if tunnel_level(t, li) || tunnel_level(t, li + 1) || tunnel_level(t, li + 2) {
        return core;
    }
    let kind = level_kind(t, layout, li);
    layout
        .types
        .iter()
        .find(|ty| ty.kind == kind)
        .map_or(core, |ty| {
            footprint(t, ty, li, core, layout.reach.max(core.0.max(core.1) / 2))
        })
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
/// (On the level below's own footprint, [`Level::below`], lined up with
/// this one; on this level's grid when that isn't known. A bigger level
/// over a smaller one may leave a pocket or two off its far edge: most of
/// it, from where the stairs land, has to hang together.)
fn hangs_together(l: &Level, track: impl Fn(wfc::Cell) -> bool) -> bool {
    let Some(&(w, h)) = l.below.first() else {
        return grid_hangs(l.width, l.depth, l.goal, None, 1.0, &track);
    };
    let frame = Level::empty(w, h);
    let goal = map_cell(l, &frame, l.goal).unwrap_or((usize::MAX, usize::MAX));
    let exit = wfc::neighbour(l.width, l.depth, l.goal, l.goal_entry.opposite())
        .and_then(|c| map_cell(l, &frame, c));
    let share = if (w, h) == (l.width, l.depth) {
        1.0
    } else {
        0.8
    };
    grid_hangs(w, h, goal, exit, share, |c| {
        map_cell(&frame, l, c).is_some_and(&track)
    })
}

/// [`hangs_together`] on one `w × h` grid with the stairs landing at `goal`:
/// no row or column all track, and at least `share` of the open cells
/// joined up (with `from`, when given).
fn grid_hangs(
    w: usize,
    h: usize,
    goal: wfc::Cell,
    from: Option<wfc::Cell>,
    share: f32,
    track: impl Fn(wfc::Cell) -> bool,
) -> bool {
    // The landing below (under the stairs) opens one way only: count it as
    // blocked too.
    let blocked = |c: wfc::Cell| track(c) || c == goal;
    let full_row = (0..h).any(|z| (0..w).all(|x| track((x, z))));
    let full_col = (0..w).any(|x| (0..h).all(|z| track((x, z))));
    if full_row || full_col {
        return false;
    }
    let free = (0..w * h).filter(|&i| !blocked((i % w, i / w))).count();
    let first = (0..w * h).map(|i| (i % w, i / w)).find(|&c| !blocked(c));
    let Some(start) = from.or(first).filter(|&c| !blocked(c)) else {
        return false;
    };
    let reach = wfc::distances(w, h, start, |c, s| {
        wfc::neighbour(w, h, c, s).is_some_and(|n| !blocked(c) && !blocked(n))
    });
    let joined = reach.iter().filter(|d| d.is_some()).count();
    joined as f32 >= free as f32 * share - 1e-3
}

// ------------------------------------------------------------ coarse layout

/// How levels are laid out coarse first (`assets/data/side/fulton_layout.ron`):
/// a level type, then a WFC over macro cells whose same-kind neighbours merge
/// into set pieces (a line of track, a hall, an atrium), then the fine WFC
/// runs the corridors up to them.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Layout {
    /// Fine cells across one macro cell (each way).
    pub macro_cells: usize,
    /// The level types, picked per level by weight among those for its depth.
    pub types: Vec<LevelType>,
    /// Faces of different coarse tiles that may meet (a face always meets
    /// its own kind, except [`Face::TrackSide`]).
    pub next_to: Vec<(Face, Face)>,
    /// No level reaches more than this many cells from the station's middle.
    pub reach: usize,
    pub line: LineAsk,
    pub hall: HallAsk,
    #[serde(default)]
    pub atrium: AtriumAsk,
    #[serde(default)]
    pub concourse: graph::ConcourseAsk,
}

pub static LAYOUT: friendslop_data::Data<Layout> =
    friendslop_data::Data::ron("data/side/fulton_layout.ron");

/// What a level is, before its corridors.
#[derive(Deserialize, Serialize, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LevelKind {
    /// Corridors and small rooms (as it always was).
    #[default]
    Warren,
    /// A long straight line of track and its platform (the platform levels).
    Line,
    /// One big tall hall with columns.
    Hall,
    /// A hall whose floor drops a whole level.
    Atrium,
    /// Rooms and curved galleries joined up by corridors, with loops
    /// (planned as a graph: `fulton/graph.rs`).
    Concourse,
}

/// One level type.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct LevelType {
    pub kind: LevelKind,
    pub weight: f32,
    /// The first level (0 is the top) it may be, and the last.
    pub from: usize,
    pub until: usize,
    /// The coarse tiles it's made of, by weight.
    pub metas: Vec<(Meta, f32)>,
    /// Its footprints in cells (across, deep).
    #[serde(default)]
    pub sizes: Vec<(usize, usize)>,
}

/// A coarse (macro) tile.
#[derive(Deserialize, Serialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Meta {
    Warren,
    Hall,
    Atrium,
    Solid,
    /// A straight line of track (along x or z).
    Track,
}

/// A coarse tile's side.
#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Warren,
    Hall,
    Atrium,
    Solid,
    /// Along a track (where it carries on or ends).
    TrackEnd,
    /// Beside a track.
    TrackSide,
}

/// A line level's track.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct LineAsk {
    /// How many macros long (at least, at most).
    pub run: (usize, usize),
}

/// A big hall.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct HallAsk {
    /// At least this many macros (either way round).
    pub min: (usize, usize),
    /// Its doors (at least, at most).
    pub doors: (usize, usize),
}

/// An atrium.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct AtriumAsk {
    /// At least this many macros (either way round).
    pub min: (usize, usize),
    /// How far its floor drops (m: a whole level).
    pub drop: f32,
    /// Its stairs' run (m).
    pub stair_run: f32,
}

/// A set piece on the macro grid: a rectangle of macros.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub meta: Meta,
    /// Its top-left macro and size in macros.
    pub at: wfc::Cell,
    pub size: (usize, usize),
    /// A track's direction (along x, else along z).
    pub along_x: bool,
}

/// A solved macro grid: every macro's tile and the set pieces it merged into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Coarse {
    pub width: usize,
    pub depth: usize,
    /// Row-major; a set piece's leftovers (outside its rectangle) are Warren.
    pub metas: Vec<Meta>,
    pub regions: Vec<Region>,
}

impl Coarse {
    pub fn meta(&self, m: wfc::Cell) -> Meta {
        self.metas
            .get(m.1 * self.width + m.0)
            .copied()
            .unwrap_or(Meta::Warren)
    }
}

/// A coarse tile's faces (N, E, S, W).
fn meta_faces(m: Meta, along_x: bool) -> [Face; 4] {
    let all = |f| [f; 4];
    match m {
        Meta::Warren => all(Face::Warren),
        Meta::Hall => all(Face::Hall),
        Meta::Atrium => all(Face::Atrium),
        Meta::Solid => all(Face::Solid),
        Meta::Track if along_x => [
            Face::TrackSide,
            Face::TrackEnd,
            Face::TrackSide,
            Face::TrackEnd,
        ],
        Meta::Track => [
            Face::TrackEnd,
            Face::TrackSide,
            Face::TrackEnd,
            Face::TrackSide,
        ],
    }
}

fn faces_meet(next_to: &[(Face, Face)], a: Face, b: Face) -> bool {
    (a == b && a != Face::TrackSide) || next_to.iter().any(|&p| p == (a, b) || p == (b, a))
}

/// The biggest rectangle of macros all in `cells` (first found on a tie).
fn biggest_rect(cells: &[wfc::Cell]) -> (wfc::Cell, (usize, usize)) {
    let (x0, x1) = (
        cells.iter().map(|c| c.0).min().unwrap_or(0),
        cells.iter().map(|c| c.0).max().unwrap_or(0),
    );
    let (z0, z1) = (
        cells.iter().map(|c| c.1).min().unwrap_or(0),
        cells.iter().map(|c| c.1).max().unwrap_or(0),
    );
    let mut best = (cells.first().copied().unwrap_or((0, 0)), (1, 1));
    for az in z0..=z1 {
        for ax in x0..=x1 {
            for bz in az..=z1 {
                for bx in ax..=x1 {
                    let (w, h) = (bx - ax + 1, bz - az + 1);
                    if w * h <= best.1.0 * best.1.1 {
                        continue;
                    }
                    let all = (az..=bz).all(|z| (ax..=bx).all(|x| cells.contains(&(x, z))));
                    if all {
                        best = ((ax, az), (w, h));
                    }
                }
            }
        }
    }
    best
}

/// The coarse WFC: a `mw × mh` macro grid of `weights`' tiles (a track both
/// ways round) whose faces meet by `next_to`, with `allowed` narrowing a
/// macro's tiles (a track's along x or not), solved until `accept` likes the set pieces. Same-kind
/// neighbours merge: a hall's or atrium's macros into their biggest
/// rectangle (the rest goes back to warren), a track's into its straight
/// line. Pure and seeded.
pub fn coarse(
    rng: &mut wfc::Rng,
    (mw, mh): (usize, usize),
    weights: &[(Meta, f32)],
    next_to: &[(Face, Face)],
    allowed: impl Fn(wfc::Cell) -> Option<Vec<(Meta, bool)>>,
    accept: impl Fn(&Coarse) -> bool,
) -> Option<Coarse> {
    let mut tiles: Vec<(Meta, bool)> = Vec::new();
    let mut ws = Vec::new();
    for &(m, w) in weights.iter().filter(|(_, w)| *w > 0.0) {
        let turns: &[bool] = if m == Meta::Track {
            &[true, false]
        } else {
            &[true]
        };
        for &a in turns {
            if !tiles.contains(&(m, a)) {
                tiles.push((m, a));
                ws.push(w / turns.len() as f32);
            }
        }
    }
    if tiles.is_empty() || mw == 0 || mh == 0 {
        return None;
    }
    let mut problem = wfc::Wfc::new(
        mw,
        mh,
        tiles
            .iter()
            .zip(&ws)
            .map(|(&(m, a), &weight)| wfc::Tile {
                sockets: meta_faces(m, a),
                weight,
            })
            .collect(),
    );
    for z in 0..mh {
        for x in 0..mw {
            if let Some(ok) = allowed((x, z)) {
                let keep: Vec<usize> = (0..tiles.len())
                    .filter(|&i| {
                        let (m, a) = tiles[i];
                        ok.iter()
                            .any(|&(om, oa)| om == m && (m != Meta::Track || oa == a))
                    })
                    .collect();
                problem = problem.pin((x, z), keep);
            }
        }
    }
    let build = |s: &wfc::Solution| -> Coarse {
        let at = |c: wfc::Cell| tiles[s.get(c)];
        let mut metas: Vec<Meta> = s.tiles.iter().map(|&t| tiles[t].0).collect();
        let mut seen = vec![false; mw * mh];
        let mut regions = Vec::new();
        for i in 0..mw * mh {
            let c = (i % mw, i / mw);
            let (m, along) = at(c);
            if seen[i] || matches!(m, Meta::Warren | Meta::Solid) {
                continue;
            }
            // Its same-kind neighbours (a track's only along it).
            let mut group = vec![c];
            seen[i] = true;
            let mut k = 0;
            while k < group.len() {
                let g = group[k];
                k += 1;
                for side in wfc::Side::ALL {
                    if m == Meta::Track && meta_faces(m, along)[side.index()] != Face::TrackEnd {
                        continue;
                    }
                    let Some(n) = wfc::neighbour(mw, mh, g, side) else {
                        continue;
                    };
                    let j = n.1 * mw + n.0;
                    if !seen[j] && at(n) == (m, along) {
                        seen[j] = true;
                        group.push(n);
                    }
                }
            }
            let (rat, size) = biggest_rect(&group);
            let rect = wfc::Room { at: rat, size };
            for g in &group {
                if !rect.contains(*g) {
                    metas[g.1 * mw + g.0] = Meta::Warren;
                }
            }
            regions.push(Region {
                meta: m,
                at: rat,
                size,
                along_x: along,
            });
        }
        Coarse {
            width: mw,
            depth: mh,
            metas,
            regions,
        }
    };
    let sol = problem.solve_until(
        rng,
        40,
        |a, b| faces_meet(next_to, *a, *b),
        |_, _, _| true,
        |s| accept(&build(s)),
    )?;
    Some(build(&sol))
}

/// The fine cells of a region (on a level with macros `mc` cells across).
fn region_cells(r: &Region, mc: usize) -> wfc::Room {
    wfc::Room {
        at: (r.at.0 * mc, r.at.1 * mc),
        size: (r.size.0 * mc, r.size.1 * mc),
    }
}

/// The level type of level `li`: Line on a platform level, Warren next to a
/// tunnel level and on the J's, else by weight among the types for its depth.
pub fn level_kind(t: &Tuning, layout: &Layout, li: usize) -> LevelKind {
    let last = li + 1 >= t.levels.max(1);
    if last || tunnel_level(t, li) || tunnel_level(t, li + 1) || tunnel_level(t, li + 2) {
        return LevelKind::Warren;
    }
    let has = |k: LevelKind| layout.types.iter().any(|ty| ty.kind == k);
    if platform_level(t, li) {
        return if has(LevelKind::Line) {
            LevelKind::Line
        } else {
            LevelKind::Warren
        };
    }
    let fits: Vec<&LevelType> = layout
        .types
        .iter()
        .filter(|ty| {
            ty.kind != LevelKind::Line && ty.weight > 0.0 && (ty.from..=ty.until).contains(&li)
        })
        .collect();
    let total: f32 = fits.iter().map(|ty| ty.weight).sum();
    let mut pick = dress_roll(t, [li, 0x1e7e1, 0, 0]) * total;
    let mut kind = fits.last().map_or(LevelKind::Warren, |ty| ty.kind);
    for ty in &fits {
        if pick < ty.weight {
            kind = ty.kind;
            break;
        }
        pick -= ty.weight;
    }
    // An atrium's pit would take too much of a platform level under it.
    if kind == LevelKind::Atrium && platform_level(t, li + 1) && has(LevelKind::Hall) {
        kind = LevelKind::Hall;
    }
    kind
}

/// The coarse layout one level asks for.
#[derive(Clone, Copy)]
struct LevelPlan<'a> {
    kind: LevelKind,
    ty: &'a LevelType,
    layout: &'a Layout,
}

/// What one level asks of the generator besides its way through.
struct LevelAsk<'a> {
    arrive: wfc::Cell,
    landing: Option<wfc::Side>,
    /// More landings (from the level above's other stairs): cell, open side.
    more: &'a [(wfc::Cell, wfc::Side)],
    /// How many more stairs down besides the goal.
    extra_downs: usize,
    last: bool,
    min_path: u32,
    /// Cells under the level above's track beds: solid, no way through.
    under: &'a [bool],
    /// This many track cells at least (0: no platforms on this level)...
    platform_cells: usize,
    /// ...and at most.
    platform_max: usize,
    rooms: &'a Rooms,
    /// The level below is a tunnel level: the stairs go where its ring is.
    ring_below: bool,
    /// The level after next is one: no track beds over the ring's hall (they
    /// would sink into it, and the stairs must come down there).
    ring_two_below: bool,
    /// Its type and coarse layout (`None`: corridors only, as ever).
    plan: Option<LevelPlan<'a>>,
    /// The cells every level shares: the stairs (and the landing's way
    /// out below them) go here, so they land on the next level whatever
    /// its size.
    core: wfc::Room,
    /// The footprints the level below may have.
    below: &'a [(usize, usize)],
    /// It may have a two-storey room (whose floor is the level below's).
    two_storey: bool,
    /// Doorways the level above's two-storey room wants down here: the cell
    /// and its side towards the room.
    low: &'a [(wfc::Cell, wfc::Side)],
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

/// One level's coarse layout: its type's macro tiles, warren where the way
/// in is and over the level above's track beds, until there is the set piece
/// the type is for (one, of a size the data asks for).
fn plan_coarse(
    rng: &mut wfc::Rng,
    (w, h): (usize, usize),
    ask: &LevelAsk,
    p: &LevelPlan,
) -> Option<Coarse> {
    let mc = p.layout.macro_cells.max(1);
    let (mw, mh) = (w / mc, h / mc);
    let is_under = |c: wfc::Cell| ask.under.get(c.1 * w + c.0).copied().unwrap_or(false);
    let exit = ask
        .landing
        .and_then(|s| wfc::neighbour(w, h, ask.arrive, s));
    let keep_warren = |m: wfc::Cell| {
        (0..mc).any(|dz| {
            (0..mc).any(|dx| {
                let c = (m.0 * mc + dx, m.1 * mc + dz);
                c == ask.arrive || Some(c) == exit || is_under(c)
            })
        })
    };
    let want = match p.kind {
        LevelKind::Warren | LevelKind::Concourse => None,
        LevelKind::Line => Some(Meta::Track),
        LevelKind::Hall => Some(Meta::Hall),
        LevelKind::Atrium => Some(Meta::Atrium),
    };
    let fits = |r: &Region| -> bool {
        let (a, b) = r.size;
        match r.meta {
            Meta::Track => {
                let (len, across) = if r.along_x { (a, mw) } else { (b, mh) };
                let run = &p.layout.line.run;
                let top = run.1.min(across.saturating_sub(1));
                // (Along the level's long way, against its edge: the
                // platform faces in, the track bed is along the wall.)
                let edge = if r.along_x {
                    r.at.1 == 0 || r.at.1 + r.size.1 == mh
                } else {
                    r.at.0 == 0 || r.at.0 + r.size.0 == mw
                };
                r.along_x == (mw >= mh) && edge && len >= run.0.min(top).max(1) && len <= top
            }
            Meta::Hall | Meta::Atrium => {
                let min = if r.meta == Meta::Hall {
                    p.layout.hall.min
                } else {
                    p.layout.atrium.min
                };
                (a >= min.0 && b >= min.1) || (a >= min.1 && b >= min.0)
            }
            _ => true,
        }
    };
    // The set piece is picked first and pinned (a line along the long way
    // against an edge, a hall anywhere it fits), clear of the way in; the
    // rest is solved round it, with no other set piece.
    let others: Vec<(Meta, bool)> =
        p.ty.metas
            .iter()
            .filter(|(m, w)| matches!(m, Meta::Warren | Meta::Solid) && *w > 0.0)
            .map(|&(m, _)| (m, true))
            .collect();
    if want.is_some() && others.is_empty() {
        return None;
    }
    let clear = |r: &wfc::Room| r.cells().all(|m| !keep_warren(m));
    let piece: Option<(Meta, bool, wfc::Room)> = match want {
        None => None,
        Some(Meta::Track) => {
            let along_x = mw >= mh;
            let (extent, across) = if along_x { (mw, mh) } else { (mh, mw) };
            let run = &p.layout.line.run;
            let top = run.1.min(extent.saturating_sub(1));
            let low = run.0.min(top).max(1);
            if top == 0 {
                return None;
            }
            let picked = (0..12).find_map(|_| {
                let len = low + rng.below(top - low + 1);
                let start = rng.below(extent - len + 1);
                let edge = if rng.roll() < 0.5 { 0 } else { across - 1 };
                let r = if along_x {
                    wfc::Room {
                        at: (start, edge),
                        size: (len, 1),
                    }
                } else {
                    wfc::Room {
                        at: (edge, start),
                        size: (1, len),
                    }
                };
                clear(&r).then_some((Meta::Track, along_x, r))
            });
            Some(picked?)
        }
        Some(m) => {
            let min = if m == Meta::Hall {
                p.layout.hall.min
            } else {
                p.layout.atrium.min
            };
            // (Never the whole way across: the corridors go round it.)
            let (top_a, top_b) = (mw.saturating_sub(1), mh.saturating_sub(1));
            let picked = (0..16).find_map(|_| {
                let (a, b) = if rng.roll() < 0.5 {
                    min
                } else {
                    (min.1, min.0)
                };
                let a = (a + rng.below(2)).min(top_a);
                let b = (b + rng.below(2)).min(top_b);
                if a == 0 || b == 0 {
                    return None;
                }
                let r = wfc::Room {
                    at: (rng.below(mw - a + 1), rng.below(mh - b + 1)),
                    size: (a, b),
                };
                clear(&r).then_some((m, true, r))
            });
            Some(picked?)
        }
    };
    coarse(
        rng,
        (mw, mh),
        &p.ty.metas,
        &p.layout.next_to,
        |m| {
            if keep_warren(m) {
                return Some(vec![(Meta::Warren, true)]);
            }
            let (meta, along_x, r) = piece?;
            if r.contains(m) {
                Some(vec![(meta, along_x)])
            } else {
                Some(others.clone())
            }
        },
        |k| match want {
            None => k.regions.is_empty(),
            Some(m) => k.regions.len() == 1 && k.regions[0].meta == m && fits(&k.regions[0]),
        },
    )
}

/// What a fine cell is in a coarse layout: its region's kind (or warren).
fn cell_meta(co: Option<&Coarse>, mc: usize, c: wfc::Cell) -> Meta {
    let Some(co) = co else {
        return Meta::Warren;
    };
    let m = (c.0 / mc, c.1 / mc);
    if m.0 >= co.width || m.1 >= co.depth {
        return Meta::Warren;
    }
    co.meta(m)
}

/// One level: WFC with the arrival and the goal pinned, rolled again until the
/// way between them is long enough (and, on a platform level, there's track).
/// A level with a coarse layout has its set pieces stamped first: a line's
/// track pinned along one row of its region, a hall's (or atrium's) room
/// over its whole region, solid rock; the corridors fill the warren.
fn plan_level(
    rng: &mut wfc::Rng,
    (w, h): (usize, usize),
    ask: &LevelAsk,
    vars: &[Var],
) -> Option<Level> {
    let LevelAsk {
        arrive,
        landing,
        more,
        extra_downs,
        last,
        min_path,
        under,
        platform_cells,
        platform_max,
        rooms: room_ask,
        ring_below,
        ring_two_below,
        plan,
        core,
        ..
    } = *ask;
    let goal_kind = if last { Kind::Platform } else { Kind::Stairs };
    let platforms = platform_cells > 0;
    let corridors: Vec<usize> = (0..vars.len())
        .filter(|&i| platforms || vars[i].1.track == 0)
        .collect();
    let plain: Vec<usize> = (0..vars.len()).filter(|&i| vars[i].1.track == 0).collect();
    let open_corridors: Vec<usize> = corridors
        .iter()
        .copied()
        .filter(|&i| vars[i].0 != 0 && vars[i].1.track == 0)
        .collect();
    let with_track = |want: u8| -> Vec<usize> {
        (0..vars.len())
            .filter(|&i| vars[i].1.track == want)
            .collect()
    };
    let is_under = |c: wfc::Cell| under.get(c.1 * w + c.0).copied().unwrap_or(false);
    let mc = plan.map_or(2, |p| p.layout.macro_cells.max(1));
    let mut co: Option<Coarse> = None;
    // A set piece that won't come out gives up sooner: the caller plans
    // the plain corridors instead.
    let set_piece = plan.is_some_and(|p| p.kind != LevelKind::Warren);
    for attempt in 0..160 {
        if set_piece && attempt >= 80 {
            return None;
        }
        // A new coarse layout every so often (and until there is one).
        if attempt % 20 == 0 || (co.is_none() && attempt < 120) {
            co = plan.and_then(|p| plan_coarse(rng, (w, h), ask, &p));
        }
        // (The last tries settle for plain corridors.)
        let layout = if attempt < 120 { co.as_ref() } else { None };
        let meta = |c: wfc::Cell| cell_meta(layout, mc, c);
        let warren = |c: wfc::Cell| meta(c) == Meta::Warren;
        // (Over a ring the stairs can only go in its hall; otherwise they
        // go in the warren in the core.) Pick among the spots far enough
        // from the way in for the full path, so tries aren't spent on
        // stairs that can't work (any spot when none is that far).
        let far = |c: wfc::Cell| {
            attempt >= 140
                || (arrive.0.abs_diff(c.0) + arrive.1.abs_diff(c.1)) as u32 * 2 >= min_path
        };
        let spots: Vec<wfc::Cell> = (0..w * h)
            .map(|i| (i % w, i / w))
            .filter(|&c| {
                if ring_below {
                    inner_cell(w, h, c)
                } else {
                    warren(c)
                        && core.contains(c)
                        && !is_under(c)
                        && c != arrive
                        && more.iter().all(|m| m.0 != c)
                }
            })
            .collect();
        let near_enough: Vec<wfc::Cell> = spots.iter().copied().filter(|&c| far(c)).collect();
        let pick = if near_enough.is_empty() {
            &spots
        } else {
            &near_enough
        };
        let goal = pick.get(rng.below(pick.len())).copied().unwrap_or((0, 0));
        if goal == arrive || is_under(goal) || !warren(goal) || !core.contains(goal) {
            continue;
        }
        // Two above a ring the next level's stairs go in its hall: land
        // outside the hall, so the way to them is long enough.
        if ring_two_below && attempt < 140 && inner_cell(w, h, goal) {
            continue;
        }
        let sides: Vec<wfc::Side> = entries(w, h, goal, last)
            .into_iter()
            .filter(|&s| !ring_below || ring_stairs(w, h, goal, s))
            .filter(|&s| wfc::neighbour(w, h, goal, s).is_some_and(warren))
            .filter(|&s| {
                last || wfc::neighbour(w, h, goal, s.opposite()).is_some_and(|c| core.contains(c))
            })
            .collect();
        if sides.is_empty() {
            continue;
        }
        let entry = sides[rng.below(sides.len())];
        // More ways down: other spots in the core, well apart from the
        // goal, each other, the way in and the landings.
        let mut downs: Vec<(wfc::Cell, wfc::Side)> = vec![(goal, entry)];
        let apart = |a: wfc::Cell, b: wfc::Cell| a.0.abs_diff(b.0) + a.1.abs_diff(b.1) >= 3;
        let mut extra = spots.clone();
        for i in (1..extra.len()).rev() {
            extra.swap(i, rng.below(i + 1));
        }
        for c in extra {
            if downs.len() > extra_downs {
                break;
            }
            if !downs.iter().all(|d| apart(d.0, c))
                || !apart(arrive, c)
                || !more.iter().all(|m| apart(m.0, c))
            {
                continue;
            }
            let ok: Vec<wfc::Side> = entries(w, h, c, false)
                .into_iter()
                .filter(|&s| wfc::neighbour(w, h, c, s).is_some_and(warren))
                .filter(|&s| {
                    wfc::neighbour(w, h, c, s.opposite()).is_some_and(|n| core.contains(n))
                })
                .collect();
            if let Some(&s) = ok.first() {
                downs.push((c, s));
            }
        }
        let extras: Vec<(wfc::Cell, wfc::Side)> = downs[1..].to_vec();
        let is_extra = |c: wfc::Cell| extras.iter().any(|d| d.0 == c);
        let is_more = |c: wfc::Cell| more.iter().any(|m| m.0 == c);
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
        let extra_tiles: Vec<(wfc::Cell, usize)> = extras
            .iter()
            .map(|&(c, s)| {
                tiles.push(special(Kind::Stairs, s));
                (c, tiles.len() - 1)
            })
            .collect();
        let more_tiles: Vec<(wfc::Cell, usize)> = more
            .iter()
            .map(|&(c, s)| {
                tiles.push(special(Kind::Landing, s));
                (c, tiles.len() - 1)
            })
            .collect();
        tiles.push(special(Kind::Under, entry));
        let under_tile = tiles.len() - 1;
        tiles.push((
            0,
            Fill {
                mask: 0,
                track: 0,
                kind: Kind::Corridor(None),
                turns: 0,
            },
            1.0,
        ));
        let solid_tile = tiles.len() - 1;
        // The set pieces: a line's track along one row (or column) of its
        // region, a hall over all of it.
        let regions = layout.map_or(&[][..], |k| &k.regions[..]);
        let mut track_pin: Vec<Option<Vec<usize>>> = vec![None; w * h];
        let mut track_cells = 0;
        let mut halls: Vec<(wfc::Room, Vec<(wfc::Cell, wfc::Side)>, Meta)> = Vec::new();
        let mut stamped = true;
        for r in regions {
            let cells = region_cells(r, mc);
            match r.meta {
                Meta::Track => {
                    let (e, wst, along) = if r.along_x {
                        (wfc::Side::East, wfc::Side::West, cells.size.0)
                    } else {
                        (wfc::Side::South, wfc::Side::North, cells.size.1)
                    };
                    // The outer row (or column), the deck facing in.
                    let (row, inward) = if r.along_x {
                        if r.at.1 == 0 {
                            (0, wfc::Side::South)
                        } else {
                            (mc - 1, wfc::Side::North)
                        }
                    } else if r.at.0 == 0 {
                        (0, wfc::Side::East)
                    } else {
                        (mc - 1, wfc::Side::West)
                    };
                    for k in 0..along {
                        let c = if r.along_x {
                            (cells.at.0 + k, cells.at.1 + row)
                        } else {
                            (cells.at.0 + row, cells.at.1 + k)
                        };
                        let want = if k == 0 {
                            bit(e)
                        } else if k + 1 == along {
                            bit(wst)
                        } else {
                            bit(e) | bit(wst)
                        };
                        let pins: Vec<usize> = with_track(want)
                            .into_iter()
                            .filter(|&i| vars[i].1.turns % 4 == inward.index() as u8)
                            .collect();
                        track_pin[c.1 * w + c.0] = Some(pins);
                        track_cells += 1;
                    }
                }
                Meta::Hall | Meta::Atrium => {
                    // (Never over a landing from the level above.)
                    if cells.cells().any(|c| more.iter().any(|m| m.0 == c)) {
                        stamped = false;
                    }
                    let hall = plan.map_or((2, 4), |p| p.layout.hall.doors);
                    let (lo, hi) = (hall.0.max(1), hall.1.max(hall.0.max(1)));
                    let n = lo + rng.below(hi - lo + 1);
                    let doors = wfc::rooms::doors(rng, &cells, (w, h), n, |c, s| {
                        wfc::neighbour(w, h, c, s).is_some_and(|n| {
                            warren(n)
                                && !is_under(n)
                                && n != goal
                                && n != arrive
                                && !is_extra(n)
                                && !is_more(n)
                        })
                    });
                    if doors.is_empty() {
                        stamped = false;
                    }
                    halls.push((cells, doors, r.meta));
                }
                _ => {}
            }
        }
        if !stamped {
            continue;
        }
        let (platform_cells, platform_max) = if track_cells > 0 {
            (track_cells, track_cells)
        } else {
            (platform_cells, platform_max)
        };
        // Big rooms among the corridors (not in the last tries, which
        // settle for anything).
        let rooms = if attempt < 120 && rng.roll() < room_ask.chance {
            place_rooms(
                rng,
                (w, h),
                room_ask,
                |c| {
                    let near = |a: wfc::Cell| a.0.abs_diff(c.0) + a.1.abs_diff(c.1) <= 1;
                    let clear = wfc::Side::ALL
                        .into_iter()
                        .all(|s| wfc::neighbour(w, h, c, s).is_none_or(warren));
                    !is_under(c)
                        && warren(c)
                        && clear
                        && !near(arrive)
                        && !near(goal)
                        && !extras.iter().chain(more).any(|d| near(d.0))
                },
                |c| is_under(c) || !warren(c),
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
        // Set pieces first, then the small rooms.
        let mut all: Vec<(wfc::Room, Vec<(wfc::Cell, wfc::Side)>, Meta, Vec<wfc::Cell>)> = halls
            .into_iter()
            .map(|(r, d, m)| (r, d, m, Vec::new()))
            .collect();
        all.extend(
            rooms
                .into_iter()
                .zip(cuts)
                .map(|((r, d), cut)| (r, d, Meta::Warren, cut)),
        );
        let mut room_tile = vec![None; w * h];
        for (ri, (room, doors, _, cut)) in all.iter().enumerate() {
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
                } else if let Some(&(_, t)) = extra_tiles.iter().find(|e| e.0 == c) {
                    vec![t]
                } else if let Some(&(_, t)) = more_tiles.iter().find(|e| e.0 == c) {
                    vec![t]
                } else if is_under(c) {
                    vec![under_tile]
                } else if c == arrive {
                    arrive_tile.map_or_else(|| open_corridors.clone(), |t| vec![t])
                } else if let Some(t) = &track_pin[z * w + x] {
                    t.clone()
                } else if meta(c) == Meta::Solid {
                    vec![solid_tile]
                } else if track_cells > 0 || (ring_two_below && inner_cell(w, h, c)) {
                    plain.clone()
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
            rooms: all.iter().map(|r| r.0).collect(),
            room_meta: all.iter().map(|r| r.2).collect(),
            // (Plain corridors when its set piece didn't come out.)
            kind: plan
                .filter(|_| layout.is_some())
                .map_or(LevelKind::Warren, |p| p.kind),
            coarse: layout.cloned(),
            below: ask.below.to_vec(),
            ..Level::empty(w, h)
        };
        // Is cell `n` on the level below's footprint (where it lines up)?
        let on_below = |l: &Level, n: wfc::Cell| {
            ask.below.iter().all(|&size| {
                let frame = Level::empty(size.0, size.1);
                frame.local(l.lattice(n)).is_some()
            })
        };
        let solved = problem.solve_until(
            rng,
            if platforms {
                24
            } else if set_piece {
                12
            } else {
                6
            },
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
                // Nor boxed in by track beds (it needs a way on besides the stairs).
                let below_exit = downs.iter().any(|&(g, e)| {
                    wfc::neighbour(w, h, g, e.opposite()).is_none_or(|c| {
                        l.fill(c).track != 0
                            || wfc::Side::ALL.into_iter().all(|s| {
                                wfc::neighbour(w, h, c, s).is_none_or(|n| {
                                    n == g || l.fill(n).track != 0 || !on_below(&l, n)
                                })
                            })
                    })
                });
                let all_ways = extras
                    .iter()
                    .chain(more)
                    .all(|&(c, _)| d[c.1 * w + c.0].is_some());
                tracks >= platform_cells
                    && tracks <= platform_max
                    && reached
                    && !below_exit
                    && all_ways
                    && (tracks == 0 || leaves_room_below(&l))
                    && d[goal.1 * w + goal.0].is_some_and(|d| d >= need)
            },
        );
        if let Some(s) = solved {
            let mut level = build(&s);
            let d = level.distances();
            level.path = path_to(&level, &d);
            return Some(level);
        }
    }
    None
}

// ------------------------------------------------------------ train tunnels

/// The tunnel ring's tiles in order round the grid (clockwise from the top
/// left): curves at the corners, straights between. `None` when the grid is
/// too small for a ring (under 3x3 tiles).
pub fn tunnel_ring(w: usize, h: usize) -> Option<Vec<TunnelTile>> {
    let (bw, bh) = (w / TUNNEL_CELLS, h / TUNNEL_CELLS);
    if bw < 3 || bh < 3 {
        return None;
    }
    let mut blocks: Vec<(usize, usize)> = (0..bw).map(|x| (x, 0)).collect();
    blocks.extend((1..bh).map(|z| (bw - 1, z)));
    blocks.extend((0..bw - 1).rev().map(|x| (x, bh - 1)));
    blocks.extend((1..bh - 1).rev().map(|z| (0, z)));
    Some(
        blocks
            .into_iter()
            .map(|(x, z)| {
                let (cx, cz) = (x == 0 || x == bw - 1, z == 0 || z == bh - 1);
                let (piece, turns) = match (cx, cz) {
                    (true, true) => (
                        TunnelPiece::Curve,
                        // Curves open N+E; which corner needs which turn.
                        match (x == 0, z == 0) {
                            (true, true) => 1,
                            (false, true) => 2,
                            (false, false) => 3,
                            (true, false) => 0,
                        },
                    ),
                    // Along the top and bottom: E-W; down the sides: N-S.
                    (false, true) => (TunnelPiece::Straight, 1),
                    _ => (TunnelPiece::Straight, 0),
                };
                TunnelTile {
                    at: (x * TUNNEL_CELLS, z * TUNNEL_CELLS),
                    piece,
                    turns,
                }
            })
            .collect(),
    )
}

/// Is `c` a cell of the tunnel ring?
fn ring_cell(w: usize, h: usize, c: wfc::Cell) -> bool {
    let (bw, bh) = (w / TUNNEL_CELLS, h / TUNNEL_CELLS);
    let (x, z) = (c.0 / TUNNEL_CELLS, c.1 / TUNNEL_CELLS);
    bw >= 3 && bh >= 3 && x < bw && z < bh && (x == 0 || z == 0 || x == bw - 1 || z == bh - 1)
}

/// Is `c` inside the ring: the hall the stairs are in, off the tracks?
fn inner_cell(w: usize, h: usize, c: wfc::Cell) -> bool {
    let (bw, bh) = (w / TUNNEL_CELLS, h / TUNNEL_CELLS);
    let (x, z) = (c.0 / TUNNEL_CELLS, c.1 / TUNNEL_CELLS);
    bw >= 3 && bh >= 3 && x > 0 && z > 0 && x < bw - 1 && z < bh - 1
}

/// Can stairs at `goal`, entered from `entry`, land in a tunnel level below?
/// The landing is a cell inside the ring (off the tracks) whose way out, the
/// side the stairs are entered from turned round, is a doorway onto the ring.
fn ring_stairs(w: usize, h: usize, goal: wfc::Cell, entry: wfc::Side) -> bool {
    inner_cell(w, h, goal)
        && wfc::neighbour(w, h, goal, entry.opposite()).is_some_and(|n| ring_cell(w, h, n))
        && wfc::neighbour(w, h, goal, entry).is_some()
}

/// Which way a straight tunnel tile's doorway is drawn: the model (the
/// mirrored one or not) and its quarter turns, so that its doorway is on the
/// cell and side the level opens onto the hall. `None` when it has no doorway.
pub fn tunnel_door(level: &Level, tile: TunnelTile) -> Option<(bool, u8)> {
    if tile.piece != TunnelPiece::Straight {
        return None;
    }
    let n = TUNNEL_CELLS as i32;
    for dz in 0..TUNNEL_CELLS {
        for dx in 0..TUNNEL_CELLS {
            let f = level.fill((tile.at.0 + dx, tile.at.1 + dz));
            for s in wfc::Side::ALL {
                let (ox, oz) = s.offset();
                let inside =
                    (0..n).contains(&(dx as i32 + ox)) && (0..n).contains(&(dz as i32 + oz));
                if inside || f.mask & bit(s) == 0 || tile.mask() & bit(s) != 0 {
                    continue;
                }
                let off = Vec3::new((dx as f32 - 0.5) * CELL, 0.0, (dz as f32 - 0.5) * CELL);
                for mirror in [false, true] {
                    for turns in [tile.turns, tile.turns + 2] {
                        let q = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2 * turns as f32);
                        let door = Vec3::new(
                            -CELL / 2.0,
                            0.0,
                            if mirror { CELL / 2.0 } else { -CELL / 2.0 },
                        );
                        if (q * door - off).length() < 0.1
                            && (q * Vec3::NEG_X - dir(s)).length() < 0.1
                        {
                            return Some((mirror, turns % 4));
                        }
                    }
                }
            }
        }
    }
    None
}

/// Does level `l` have a ring of train tunnels instead of corridors? Every
/// [`Tunnels::every`] levels from [`Tunnels::offset`]; never the top (nothing
/// to land from), the J's, a platform level, or the one under a platform
/// level (its track beds sink into it).
pub fn tunnel_level(t: &Tuning, l: usize) -> bool {
    let tn = &t.tunnels;
    tn.every > 0
        && l > 0
        && l >= tn.offset
        && (l - tn.offset).is_multiple_of(tn.every)
        && l + 1 < t.levels.max(1)
        && !platform_level(t, l)
        && !platform_level(t, l - 1)
        && tunnel_ring(t.width.max(3), t.depth.max(3)).is_some()
}

/// A tunnel level: the ring of tiles, and in its middle a hall off the tracks
/// (the tiles' rails would run through any stairs): the stairs land in a cell
/// of it with a doorway onto the ring, and the way down is another doorway on
/// the far side of the ring (at least three tiles round either way) into a
/// hall cell and the stairs. When one arm caves in the other is still there.
/// `None` when the landing doesn't fit (the level above checks, so only if it
/// settled) or there is no hall cell piece.
fn plan_tunnel(
    rng: &mut wfc::Rng,
    (w, h): (usize, usize),
    ask: &LevelAsk,
    vars: &[Var],
) -> Option<Level> {
    let tiles = tunnel_ring(w, h)?;
    let (arrive, landing) = (ask.arrive, ask.landing?);
    if ask.last || !inner_cell(w, h, arrive) {
        return None;
    }
    let onto = wfc::neighbour(w, h, arrive, landing).filter(|&n| ring_cell(w, h, n))?;
    let solid = Fill {
        mask: 0,
        track: 0,
        kind: Kind::Corridor(None),
        turns: 0,
    };
    let mut cells = vec![solid; w * h];
    let tile_of = |c: wfc::Cell| {
        tiles.iter().position(|t| {
            (t.at.0 / TUNNEL_CELLS, t.at.1 / TUNNEL_CELLS)
                == (c.0 / TUNNEL_CELLS, c.1 / TUNNEL_CELLS)
        })
    };
    for tile in &tiles {
        for dz in 0..TUNNEL_CELLS {
            for dx in 0..TUNNEL_CELLS {
                let c = (tile.at.0 + dx, tile.at.1 + dz);
                let mut mask = 0;
                for s in wfc::Side::ALL {
                    let (ox, oz) = s.offset();
                    let (nx, nz) = (dx as i32 + ox, dz as i32 + oz);
                    let inside = (0..TUNNEL_CELLS as i32).contains(&nx)
                        && (0..TUNNEL_CELLS as i32).contains(&nz);
                    if inside || tile.mask() & bit(s) != 0 {
                        mask |= bit(s);
                    }
                }
                cells[c.1 * w + c.0] = Fill {
                    mask,
                    track: 0,
                    kind: Kind::Tunnel,
                    turns: tile.turns,
                };
            }
        }
    }
    let ring_dist = |a: usize, b: usize| {
        let d = a.abs_diff(b);
        d.min(tiles.len() - d)
    };
    let from = tile_of(onto)?;
    // Every way down: a hall cell with a doorway onto the far side of the
    // ring, and beside it the stairs (entered from the hall cell).
    let mut options = Vec::new();
    for i in 0..w * h {
        let hall = (i % w, i / w);
        if !inner_cell(w, h, hall) || hall == arrive {
            continue;
        }
        for door_side in wfc::Side::ALL {
            let Some(door) = wfc::neighbour(w, h, hall, door_side).filter(|&n| ring_cell(w, h, n))
            else {
                continue;
            };
            if tile_of(door).is_none_or(|t| ring_dist(from, t) < 3) {
                continue;
            }
            for to_stairs in wfc::Side::ALL.into_iter().filter(|&s| s != door_side) {
                let Some(goal) = wfc::neighbour(w, h, hall, to_stairs)
                    .filter(|&g| inner_cell(w, h, g) && g != arrive)
                else {
                    continue;
                };
                // The landing below opens onto the grid.
                if wfc::neighbour(w, h, goal, to_stairs).is_none() {
                    continue;
                }
                options.push((hall, door_side, door, goal, to_stairs));
            }
        }
    }
    if options.is_empty() {
        return None;
    }
    let (hall, door_side, door, goal, to_stairs) = options[rng.below(options.len())];
    let entry = to_stairs.opposite();
    let hall_mask = bit(door_side) | bit(to_stairs);
    let hall_fill = vars
        .iter()
        .find(|v| {
            v.0 == hall_mask && v.1.track == 0 && matches!(v.1.kind, Kind::Corridor(Some(_)))
        })?
        .1;
    cells[arrive.1 * w + arrive.0] = special(Kind::Landing, landing).1;
    cells[goal.1 * w + goal.0] = special(Kind::Stairs, entry).1;
    cells[hall.1 * w + hall.0] = hall_fill;
    // The ring's cells next to the landing and the hall cell open onto them.
    cells[onto.1 * w + onto.0].mask |= bit(landing.opposite());
    cells[door.1 * w + door.0].mask |= bit(door_side.opposite());
    let mut level = Level {
        width: w,
        depth: h,
        cells,
        arrive,
        goal,
        goal_entry: entry,
        tunnel: tiles,
        ..Level::empty(w, h)
    };
    let d = level.distances();
    level.path = path_to(&level, &d);
    (!level.path.is_empty()).then_some(level)
}

/// One open hall (never needed in practice; keeps generation total).
fn fallback(w: usize, h: usize, ask: &LevelAsk) -> Level {
    let LevelAsk {
        arrive,
        landing,
        more,
        last,
        under,
        core,
        ..
    } = *ask;
    let is_under = |c: wfc::Cell| under.get(c.1 * w + c.0).copied().unwrap_or(false);
    // The goal: the farthest cell with a way in that isn't the arrival, and
    // that the arrival's landing doesn't open straight onto.
    let exit_to = landing.and_then(|l| wfc::neighbour(w, h, arrive, l));
    let (goal, entry) = (0..w * h)
        .map(|i| (i % w, i / w))
        .filter(|&c| {
            c != arrive
                && Some(c) != exit_to
                && !is_under(c)
                && core.contains(c)
                && more.iter().all(|m| m.0 != c)
        })
        .filter_map(|c| {
            let s = entries(w, h, c, last).into_iter().find(|&s| {
                wfc::neighbour(w, h, c, s)
                    .is_some_and(|n| n != arrive && !is_under(n) && more.iter().all(|m| m.0 != n))
                    && (last
                        || wfc::neighbour(w, h, c, s.opposite()).is_some_and(|n| core.contains(n)))
            })?;
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
            } else if let Some(&(_, l)) = more.iter().find(|m| m.0 == c) {
                s == l
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
            } else if let Some(&(_, l)) = more.iter().find(|m| m.0 == c) {
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
    l.run = STAIRS_RUN;
    // An atrium's floor drops a whole level, down long stairs; on its level
    // nothing else is tall.
    let atrium = matches!(l.kind, LevelKind::Atrium | LevelKind::Concourse)
        && l.room_meta.contains(&Meta::Atrium);
    if atrium {
        let a = &LAYOUT.get().atrium;
        l.sink = a.drop.clamp(0.0, STATION_LEVEL_H);
        l.run = a.stair_run.clamp(STAIRS_RUN, CELL - 0.4);
    }
    let reach = |l: &Level| -> Vec<bool> { l.distances().iter().map(|d| d.is_some()).collect() };
    let before = reach(l);
    // (Not above a tunnel level: its floor under the sunken cells would be tunnel.)
    if l.sink > 0.2 && !tunnel_level(t, li + 1) && !tunnel_level(t, li + 2) {
        for ri in 0..l.rooms.len() {
            let tall = if atrium {
                l.room_meta.get(ri) == Some(&Meta::Atrium)
            } else {
                // (A big hall's floor stays up over a platform level: its
                // track needs the room.)
                if l.set_piece(ri) {
                    !platform_level(t, li + 1)
                } else {
                    dress_roll(t, [li, ri, usize::MAX - 1, 0]) < t.rooms.tall.chance
                }
            };
            if tall {
                make_tall(l, ri, |k| dress_roll(t, [li, ri, usize::MAX - 1, k + 1]));
            }
        }
    }
    let c = &t.collapse;
    let rubble = stage(t, li).rubble;
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
            let (blocked, squeezed) = (c.blocked * rubble, c.squeeze * rubble);
            let mut squeeze = roll >= blocked && roll < blocked + squeezed;
            if roll < blocked {
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
    cave_in_tunnels(t, li, l, &before);
    let d = l.distances();
    l.path = path_to(l, &d);
}

/// Train tunnel cave-ins: a straight tile may fill across its middle, both
/// cells wide (never the tiles with the stairs, and never so that anything
/// is cut off: there it's a squeeze, a gap in the heaps). With the ring's two
/// arms, one caved in means the other is the only way.
fn cave_in_tunnels(t: &Tuning, li: usize, l: &mut Level, before: &[bool]) {
    let tc = &t.tunnels;
    let w = l.width;
    for (ti, tile) in l.tunnel.clone().into_iter().enumerate() {
        if tile.piece != TunnelPiece::Straight {
            continue;
        }
        let (x0, z0) = tile.at;
        let cells = [(x0, z0), (x0 + 1, z0), (x0, z0 + 1), (x0 + 1, z0 + 1)];
        if cells.iter().any(|&c| l.fill(c).kind != Kind::Tunnel) {
            continue;
        }
        let roll = dress_roll(t, [li, ti, 0x7a77, 11]);
        if roll >= tc.blocked + tc.squeeze {
            continue;
        }
        // The two pairs of cells across the tile's middle, and the side
        // between them (from the first of each).
        let (s, pairs) = if tile.turns.is_multiple_of(2) {
            (
                wfc::Side::South,
                [((x0, z0), (x0, z0 + 1)), ((x0 + 1, z0), (x0 + 1, z0 + 1))],
            )
        } else {
            (
                wfc::Side::East,
                [((x0, z0), (x0 + 1, z0)), ((x0, z0 + 1), (x0 + 1, z0 + 1))],
            )
        };
        let mark = |v: &mut Vec<u8>, on: bool| {
            for (a, b) in pairs {
                let (i, j) = (a.1 * w + a.0, b.1 * w + b.0);
                if on {
                    v[i] |= bit(s);
                    v[j] |= bit(s.opposite());
                } else {
                    v[i] &= !bit(s);
                    v[j] &= !bit(s.opposite());
                }
            }
        };
        let mut squeeze = roll >= tc.blocked;
        if !squeeze {
            let mut rubble = std::mem::take(&mut l.rubble);
            mark(&mut rubble, true);
            l.rubble = rubble;
            let after: Vec<bool> = l.distances().iter().map(|d| d.is_some()).collect();
            if before.iter().zip(&after).any(|(b, a)| *b && !*a) {
                let mut rubble = std::mem::take(&mut l.rubble);
                mark(&mut rubble, false);
                l.rubble = rubble;
                squeeze = true;
            }
        }
        if squeeze {
            let mut sq = std::mem::take(&mut l.squeeze);
            mark(&mut sq, true);
            l.squeeze = sq;
        }
    }
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
    // (Never where the landing below opens, or next to it: the way out of
    // it stays open.)
    let exit = wfc::neighbour(w, h, l.goal, l.goal_entry.opposite());
    let by_exit = |c: wfc::Cell| exit.is_some_and(|e| e.0.abs_diff(c.0) + e.1.abs_diff(c.1) <= 1);
    let pit: Vec<bool> = (0..w * h)
        .map(|i| {
            let c = (i % w, i / w);
            mine(c) && !deck(c) && !by_exit(c)
        })
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

/// Dig train tunnels on from a platform level's tunnel ends: from each end's
/// mouth (the side across from its track), a winding run of [`Kind::Rails`]
/// cells through solid rock (never where a stairs below lands or opens),
/// a dead end at its far end. Seeded by the level, so the same every time.
fn dig_rails(t: &Tuning, li: usize, l: &mut Level) {
    let (lo, hi) = t.rails.len;
    if hi == 0 {
        return;
    }
    let (w, h) = (l.width, l.depth);
    // Cells a stairs (or the way out of a landing below) needs.
    let mut keep: Vec<wfc::Cell> = Vec::new();
    for (g, e) in l.downs().into_iter().chain(l.ups()) {
        keep.push(g);
        for s in [e, e.opposite()] {
            if let Some(n) = wfc::neighbour(w, h, g, s) {
                keep.push(n);
            }
        }
    }
    let rock = |l: &Level, c: wfc::Cell| {
        let f = l.fill(c);
        f.mask == 0
            && f.track == 0
            && matches!(f.kind, Kind::Corridor(_))
            && !l.sunk(c)
            && !keep.contains(&c)
    };
    let ends: Vec<(wfc::Cell, wfc::Side)> = (0..w * h)
        .map(|i| (i % w, i / w))
        .filter_map(|c| {
            let f = l.fill(c);
            if f.track.count_ones() != 1 {
                return None;
            }
            let t_side = wfc::Side::ALL
                .into_iter()
                .find(|&s| f.track & bit(s) != 0)?;
            Some((c, t_side.opposite()))
        })
        .collect();
    for (k, (end, mouth)) in ends.into_iter().enumerate() {
        let roll = |n: usize| dress_roll(t, [li, end.0 * 97 + end.1, k, 0xa11 + n]);
        let want = lo + (roll(0) * (hi.max(lo) - lo + 1) as f32) as usize;
        let Some(first) = wfc::neighbour(w, h, end, mouth).filter(|&c| rock(l, c)) else {
            continue;
        };
        let mut path = vec![first];
        let mut d = mouth;
        for step in 1..want.max(1) {
            let at = path[path.len() - 1];
            let turn = roll(step * 3 + 1) < t.rails.turn;
            let left = wfc::Side::ALL[(d.index() + 3) % 4];
            let right = wfc::Side::ALL[(d.index() + 1) % 4];
            let order = if !turn {
                [d, left, right]
            } else if roll(step * 3 + 2) < 0.5 {
                [left, right, d]
            } else {
                [right, left, d]
            };
            // On into rock, never back alongside itself.
            let next = order.into_iter().find_map(|s| {
                let n = wfc::neighbour(w, h, at, s)?;
                let lonely = wfc::Side::ALL.into_iter().all(|o| {
                    wfc::neighbour(w, h, n, o).is_none_or(|m| m == at || !path.contains(&m))
                });
                (rock(l, n) && !path.contains(&n) && n != end && lonely).then_some((n, s))
            });
            let Some((n, s)) = next else { break };
            path.push(n);
            d = s;
        }
        if path.len() < lo.max(2) {
            continue;
        }
        // Join it up: the mouth opens, each cell to the next.
        l.cells[end.1 * w + end.0].mask |= bit(mouth);
        let mut prev = end;
        for &c in &path {
            let back = wfc::Side::ALL
                .into_iter()
                .find(|&s| wfc::neighbour(w, h, c, s) == Some(prev))
                .map_or(0, bit);
            l.cells[c.1 * w + c.0] = Fill {
                mask: back,
                track: 0,
                kind: Kind::Rails,
                turns: 0,
            };
            if prev != end {
                let fwd = wfc::Side::ALL
                    .into_iter()
                    .find(|&s| wfc::neighbour(w, h, prev, s) == Some(c))
                    .map_or(0, bit);
                l.cells[prev.1 * w + prev.0].mask |= fwd;
            }
            prev = c;
        }
    }
}

/// How a train tunnel cell is drawn: curve or straight, and its quarter
/// turns (the straight is modelled along x, the curve from N to E).
pub fn rails_piece(mask: u8) -> (bool, u8) {
    let ns = bit(wfc::Side::North) | bit(wfc::Side::South);
    if mask.count_ones() == 2 && mask != ns && mask != (bit(wfc::Side::East) | bit(wfc::Side::West))
    {
        let ne = bit(wfc::Side::North) | bit(wfc::Side::East);
        let t = (0..4u8).find(|&t| turned(ne, t) == mask).unwrap_or(0);
        return (true, t);
    }
    // Along x unless it runs N-S.
    (false, u8::from(mask & ns != 0))
}

/// The station's plan so far, planned a level at a time (each takes a WFC
/// solve): the same levels [`generate`] makes, in the same order, from the
/// same seed, however many are planned at once. Every peer plans its own copy.
#[derive(Clone)]
pub struct StationPlan {
    rng: wfc::Rng,
    vars: Vec<Var>,
    /// The core every level shares (the tunnel levels' size).
    size: (usize, usize),
    total: usize,
    /// Where the next level is arrived at (on the lattice).
    arrive: (i32, i32),
    landing: Option<wfc::Side>,
    /// More landings on the next level (the last level's other stairs): lattice cell, open side.
    more: Vec<((i32, i32), wfc::Side)>,
    /// Lattice cells under the last planned level's track beds and sunken floors.
    under: Vec<(i32, i32)>,
    /// Doorways the last planned level's two-storey room wants on the next:
    /// lattice cell and its side towards the room.
    low: Vec<((i32, i32), wfc::Side)>,
    /// How far the last planned level's floor sinks into the next.
    above_drop: f32,
    /// The levels planned so far, top first.
    pub levels: Vec<Level>,
    /// Levels planned before `levels[0]` that this copy doesn't hold
    /// ([`StationPlan::carry_on`]; 0 for the station's own plan).
    skip: usize,
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
            arrive: {
                let o = centred((w, h));
                ((w / 2) as i32 + o.0, o.1)
            },
            landing: None,
            more: Vec::new(),
            under: Vec::new(),
            low: Vec::new(),
            above_drop: 0.0,
            levels: Vec::with_capacity(t.levels.max(1)),
            skip: 0,
        }
    }

    /// How many levels are planned (held here or not).
    pub fn count(&self) -> usize {
        self.skip + self.levels.len()
    }

    /// A copy that plans on from here without the levels planned so far,
    /// to plan the next level off the main thread ([`plan_station`]).
    pub fn carry_on(&mut self) -> StationPlan {
        let levels = std::mem::take(&mut self.levels);
        let mut next = self.clone();
        self.levels = levels;
        next.skip = self.count();
        next
    }

    /// Take the levels a [`StationPlan::carry_on`] copy planned, and its
    /// place in the plan (ignored if it didn't carry on from here).
    pub fn take(&mut self, mut next: StationPlan) {
        if next.skip != self.count() {
            return;
        }
        let mut levels = std::mem::take(&mut self.levels);
        levels.append(&mut next.levels);
        next.levels = levels;
        next.skip = self.skip;
        for i in 1..next.levels.len() {
            let (a, b) = next.levels.split_at_mut(i);
            if let (Some(above), Some(below)) = (a.last_mut(), b.first()) {
                link_low_doors(above, below);
            }
        }
        *self = next;
    }

    /// Every level is planned.
    pub fn done(&self) -> bool {
        self.count() >= self.total
    }

    /// Plan the next level down (`None` when the station is all planned).
    pub fn plan_next(&mut self, t: &Tuning) -> Option<&Level> {
        if self.done() {
            return None;
        }
        let l = self.count();
        let layout = LAYOUT.get();
        let kind = level_kind(t, &layout, l);
        // (Over a tunnel level the stairs have to come down in its ring's
        // hall: those levels keep to the plain corridors.)
        let near_ring = tunnel_level(t, l) || tunnel_level(t, l + 1) || tunnel_level(t, l + 2);
        let plan = layout
            .types
            .iter()
            .filter(|_| !near_ring)
            .find(|ty| ty.kind == kind)
            .map(|ty| LevelPlan {
                kind,
                ty,
                layout: &layout,
            });
        let size = level_size(t, &layout, l, self.size);
        let below: Vec<(usize, usize)> = if l + 1 < self.total {
            vec![level_size(t, &layout, l + 1, self.size)]
        } else {
            Vec::new()
        };
        let frame = Level::empty(size.0, size.1);
        let core = {
            let at = frame.local(centred(self.size)).unwrap_or((0, 0));
            wfc::Room {
                at,
                size: self.size,
            }
        };
        let arrive = frame.local(self.arrive).unwrap_or((size.0 / 2, 0));
        let more: Vec<(wfc::Cell, wfc::Side)> = self
            .more
            .iter()
            .filter_map(|&(at, s)| Some((frame.local(at)?, s)))
            .collect();
        // More ways down, except where the next level can't take them (a
        // tunnel ring's stairs must be in its hall) and on the J's level.
        // (Platform levels must fit their track and tunnel rings their hall:
        // they and the levels round them keep one way down.)
        let busy = |k: usize| platform_level(t, k) || tunnel_level(t, k);
        let extra_downs = if l + 1 >= self.total || near_ring || busy(l + 1) {
            0
        } else {
            let (lo, hi) = (t.downs.0.max(1), t.downs.1.max(t.downs.0.max(1)));
            lo + self.rng.below(hi - lo + 1) - 1
        };
        let under: Vec<bool> = (0..size.0 * size.1)
            .map(|i| {
                self.under
                    .contains(&frame.lattice((i % size.0, i / size.0)))
            })
            .collect();
        // A two-storey room: a concourse over a concourse (whose floor it shares).
        let two_storey = kind == LevelKind::Concourse
            && !near_ring
            && l + 2 < self.total
            && level_kind(t, &layout, l + 1) == LevelKind::Concourse
            && !busy(l + 1)
            && dress_roll(t, [l, 0x2570, 0, 0]) < layout.concourse.two_storey;
        let low: Vec<(wfc::Cell, wfc::Side)> = self
            .low
            .iter()
            .filter_map(|&(at, s)| Some((frame.local(at)?, s)))
            .collect();
        let ask = LevelAsk {
            arrive,
            landing: self.landing,
            more: &more,
            extra_downs,
            last: l + 1 == self.total,
            min_path: t.min_path,
            under: &under,
            platform_cells: if platform_level(t, l) {
                t.platform_cells.max(2)
            } else {
                0
            },
            platform_max: t.platform_max.max(t.platform_cells.max(2)),
            rooms: &t.rooms,
            ring_below: tunnel_level(t, l + 1),
            ring_two_below: tunnel_level(t, l + 2),
            plan,
            core,
            below: &below,
            two_storey,
            low: &low,
        };
        let tunnel = if tunnel_level(t, l) {
            plan_tunnel(&mut self.rng, size, &ask, &self.vars)
        } else {
            None
        };
        // A concourse is planned as a graph (one way down if more don't
        // fit); if nothing comes of it, the plain corridors.
        let concourse = if tunnel.is_none() && kind == LevelKind::Concourse && plan.is_some() {
            let c = &layout.concourse;
            graph::plan(&mut self.rng, size, &ask, &self.vars, c).or_else(|| {
                (ask.extra_downs > 0).then_some(())?;
                let one = LevelAsk {
                    extra_downs: 0,
                    ..ask
                };
                graph::plan(&mut self.rng, size, &one, &self.vars, c)
            })
        } else {
            None
        };
        let ask = if kind == LevelKind::Concourse {
            LevelAsk { plan: None, ..ask }
        } else {
            ask
        };
        let level = if let Some(level) = tunnel.or(concourse) {
            level
        } else if self.vars.is_empty() {
            fallback(size.0, size.1, &ask)
        } else {
            plan_level(&mut self.rng, size, &ask, &self.vars)
                // Too much to fit: one way down (still taking the landings).
                .or_else(|| {
                    (ask.extra_downs > 0).then_some(())?;
                    let one = LevelAsk {
                        extra_downs: 0,
                        ..ask
                    };
                    plan_level(&mut self.rng, size, &one, &self.vars)
                })
                // No set piece: plain corridors on the footprint the level
                // above planned round (its landings open onto it).
                .or_else(|| {
                    (ask.plan.is_some() || ask.extra_downs > 0).then_some(())?;
                    let plain = LevelAsk {
                        plan: None,
                        extra_downs: 0,
                        ..ask
                    };
                    plan_level(&mut self.rng, size, &plain, &self.vars)
                })
                .unwrap_or_else(|| {
                    // Nothing came of the set piece: the plain corridors, the
                    // core's size (where the way in and the stairs are).
                    let at = ask.core.at;
                    let inner = |c: wfc::Cell| (c.0 - at.0, c.1 - at.1);
                    let under: Vec<bool> = (0..self.size.0 * self.size.1)
                        .map(|i| {
                            let c = (at.0 + i % self.size.0, at.1 + i / self.size.0);
                            under[c.1 * size.0 + c.0]
                        })
                        .collect();
                    let more_inner: Vec<(wfc::Cell, wfc::Side)> =
                        more.iter().map(|&(c, s)| (inner(c), s)).collect();
                    let plain = LevelAsk {
                        arrive: inner(ask.arrive),
                        more: &more_inner,
                        extra_downs: 0,
                        under: &under,
                        plan: None,
                        core: wfc::Room {
                            at: (0, 0),
                            size: self.size,
                        },
                        ..ask
                    };
                    plan_level(&mut self.rng, self.size, &plain, &self.vars)
                        .unwrap_or_else(|| fallback(self.size.0, self.size.1, &plain))
                })
        };
        let mut level = level;
        level.above_drop = self.above_drop;
        level.below = below.clone();
        shape_level(t, l, &mut level);
        if platform_level(t, l) {
            dig_rails(t, l, &mut level);
        }
        level.service = service_doors(t, l, &level);
        // Ground sunk into the level below (tall rooms, track beds) cramps
        // it: an extra stairs whose landing would open onto that ground, or
        // any when a lot of the core sinks, turns into the corridor's dead end.
        let sunk_at: Vec<bool> = (0..level.cells.len())
            .map(|i| {
                level.sunk[i] || level.cells[i].track != 0 || level.cells[i].kind == Kind::Rails
            })
            .collect();
        let sinks = |i: usize| sunk_at[i];
        let core_at = level.local(centred(self.size)).unwrap_or((0, 0));
        let core_sunk = (0..level.cells.len())
            .filter(|&i| {
                let (x, z) = (i % level.width, i / level.width);
                level.sunk[i]
                    && (core_at.0..core_at.0 + self.size.0).contains(&x)
                    && (core_at.1..core_at.1 + self.size.1).contains(&z)
            })
            .count();
        for (c, entry) in level.downs() {
            if c == level.goal {
                continue;
            }
            let w = level.width;
            let exit_sinks = wfc::neighbour(w, level.depth, c, entry.opposite())
                .is_none_or(|n| sinks(n.1 * w + n.0));
            if core_sunk <= 4 && !exit_sinks {
                continue;
            }
            let end = self
                .vars
                .iter()
                .find(|v| v.0 == bit(entry) && v.1.track == 0)
                .map(|v| v.1);
            if let Some(f) = end {
                level.cells[c.1 * w + c.0] = f;
            }
        }
        self.above_drop = if level.sunk.iter().any(|s| *s) {
            level.sink
        } else {
            0.0
        };
        self.arrive = level.lattice(level.goal);
        self.landing = Some(level.goal_entry.opposite());
        self.more = level
            .downs()
            .into_iter()
            .filter(|&(c, _)| c != level.goal)
            .map(|(c, s)| (level.lattice(c), s.opposite()))
            .collect();
        self.under = (0..level.cells.len())
            .filter(|&i| {
                level.cells[i].track != 0 || level.sunk[i] || level.cells[i].kind == Kind::Rails
            })
            .map(|i| level.lattice((i % level.width, i / level.width)))
            .collect();
        self.low = if two_storey && level.sink >= STATION_LEVEL_H - 0.01 {
            low_doorways(t, l, &level, &below)
        } else {
            Vec::new()
        };
        if let Some(above) = self.levels.last_mut() {
            link_low_doors(above, &level);
        }
        self.levels.push(level);
        self.levels.last()
    }

    /// Plan until at least `n` levels (or all of them) are planned.
    pub fn plan_to(&mut self, t: &Tuning, n: usize) {
        while self.count() < n && self.plan_next(t).is_some() {}
    }
}

/// Where a two-storey room's sunken floor gets doorways in its lower walls
/// (on the level below, the cell outside and its side towards the room): up
/// to two walled sides of its sunken cells, onto cells outside it that the
/// level below has (and don't sink), as far apart as they come.
fn low_doorways(
    t: &Tuning,
    li: usize,
    l: &Level,
    below: &[(usize, usize)],
) -> Vec<((i32, i32), wfc::Side)> {
    let (w, h) = (l.width, l.depth);
    let mut options: Vec<((i32, i32), wfc::Side)> = Vec::new();
    for i in 0..w * h {
        let c = (i % w, i / w);
        if !l.sunk[i] || !matches!(l.fill(c).kind, Kind::Room(_)) {
            continue;
        }
        for s in wfc::Side::ALL {
            let Some(n) = wfc::neighbour(w, h, c, s) else {
                continue;
            };
            let at = l.lattice(n);
            let on_below = below
                .iter()
                .all(|&size| Level::empty(size.0, size.1).local(at).is_some());
            if l.fill(c).mask & bit(s) == 0
                && l.fill(n).kind != l.fill(c).kind
                && !l.sunk(n)
                && l.fill(n).track == 0
                && on_below
            {
                options.push((at, s.opposite()));
            }
        }
    }
    let mut out = Vec::new();
    if let Some(&first) =
        options.get((dress_roll(t, [li, 0x10d, 0, 0]) * options.len() as f32) as usize)
    {
        out.push(first);
        if let Some(&far) = options
            .iter()
            .max_by_key(|o| o.0.0.abs_diff(first.0.0) + o.0.1.abs_diff(first.0.1))
            && far.0 != first.0
        {
            out.push(far);
        }
    }
    out
}

/// A level's two-storey room's doorways, from the level below's
/// [`Level::from_above`] (the cell there, on the other side of the doorway).
fn link_low_doors(above: &mut Level, below: &Level) {
    above.low_doors = below
        .from_above
        .iter()
        .filter_map(|&(c, s)| {
            let n = wfc::neighbour(below.width, below.depth, c, s)?;
            Some((above.local(below.lattice(n))?, s.opposite()))
        })
        .filter(|&(c, _)| above.sunk(c))
        .collect();
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
pub fn cell_world(t: &Tuning, level: usize, l: &Level, c: wfc::Cell) -> Vec3 {
    let (x, z) = l.lattice(c);
    Vec3::new(
        t.station_at.0 + (x as f32 + 0.5) * CELL,
        level_floor(level),
        t.station_at.1 + (z as f32 + 0.5) * CELL,
    )
}

/// A level's floor height.
pub fn level_floor(level: usize) -> f32 {
    STATION_TOP_Y - level as f32 * STATION_LEVEL_H
}

/// The level something at `p` is in: [`level_at`], but down in an atrium
/// (whose floor is the level below's) it's the atrium's level.
pub fn level_of(t: &Tuning, levels: &[Level], p: Vec3) -> usize {
    let li = level_at(p.y);
    let Some(up) = li.checked_sub(1) else {
        return li;
    };
    let Some(l) = levels.get(up) else {
        return li;
    };
    let in_pit = cell_of(t, l, p).is_some_and(|c| c.0 < l.width && c.1 < l.depth && l.sunk(c));
    if in_pit && p.y >= level_floor(up) - l.sink - 0.5 {
        up
    } else {
        li
    }
}

/// The level a body with feet at `y` is on.
pub fn level_at(y: f32) -> usize {
    ((STATION_TOP_Y - y) / STATION_LEVEL_H).round().max(0.0) as usize
}

/// The floor's height under `p` (on the level `p` is on): lower in a tall
/// room's sunken cells, on the way down their stairs.
pub fn floor_at(t: &Tuning, levels: &[Level], p: Vec3) -> f32 {
    let li = level_of(t, levels, p);
    let top = level_floor(li);
    let Some(l) = levels.get(li) else {
        return top;
    };
    let o = cell_world(t, li, l, (0, 0));
    let (x, z) = (((p.x - o.x) / CELL).round(), ((p.z - o.z) / CELL).round());
    if x < 0.0 || z < 0.0 || x as usize >= l.width || z as usize >= l.depth {
        return top;
    }
    let c = (x as usize, z as usize);
    if l.fill(c).kind == Kind::Rails {
        return top - TRACK_DROP;
    }
    if !l.sunk(c) {
        return top;
    }
    let off = p - cell_world(t, li, l, c);
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
        if across <= STAIRS_W / 2.0 && into < l.run {
            y = y.max(top - l.sink * (into.max(0.0) / l.run));
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
    let set = TILES.get();
    let half = CELL / 2.0;
    for z in 0..level.depth {
        for x in 0..level.width {
            let c = (x, z);
            let f = level.fill(c);
            let at = cell_world(t, li, level, c);
            let r = f.rotation();
            let flat = |centre: Vec3, size: Vec3| Block {
                centre,
                size,
                rotation: Quat::IDENTITY,
            };
            if f.mask == 0 {
                // Solid; under a track bed only up to the bed's underside.
                let top = if f.kind == Kind::Under {
                    let sink = level
                        .above_drop
                        .max(TRACK_DROP)
                        .max(t.rooms.tall.sink.clamp(0.0, 1.9));
                    ROOM_H - sink - 0.2
                } else {
                    ROOM_H
                };
                // (Under an atrium there's nothing left: its floor is ours.)
                if top <= 0.05 {
                    continue;
                }
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
                // A doorway onto the level below: only the wall above it.
                let low = if level.low_doors.contains(&(c, s)) {
                    0.0
                } else {
                    low
                };
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
                        centre: at
                            + r * Vec3::new(
                                0.0,
                                ROOM_H / 2.0 - 1.2,
                                (PLATFORM_EDGE_Z + CELL / 2.0) / 2.0,
                            ),
                        size: Vec3::new(CELL, ROOM_H + 2.4, CELL / 2.0 - PLATFORM_EDGE_Z),
                        rotation: r,
                    });
                }
                Kind::Corridor(Some(i)) => {
                    // A hiding piece's lockers bank is solid: seven doors
                    // against the wall (`LOCKERS_BLOCK`, in the piece's own
                    // frame, turned with the piece).
                    if set.pieces.get(i as usize).is_some_and(|p| p.hide) {
                        out.push(Block {
                            centre: at + r * LOCKERS_BLOCK.0,
                            size: LOCKERS_BLOCK.1,
                            rotation: r,
                        });
                    }
                }
                _ => {}
            }
        }
    }
    out.extend(
        room_dressing(t, li, level)
            .into_iter()
            .chain(corridor_dressing(t, &set, li, level))
            .filter_map(|d| d.block),
    );
    out.extend(parked_cars(t, li, level).into_iter().flat_map(|c| c.blocks));
    out.extend(shapes(t, li, level).into_iter().filter_map(|s| s.block));
    out.extend(tunnel_walls(t, li, level));
    for at in room_columns(t, li, level)
        .into_iter()
        .chain(tunnel_columns(t, li, level))
        .chain(pit_columns(t, li, level))
        .chain(corridor_columns(t, li, level))
        .chain(deck_columns(t, li, level))
    {
        let tall = column_height(li, at);
        out.push(Block {
            centre: at + Vec3::Y * (tall / 2.0),
            size: Vec3::new(COLUMN_SIZE, tall, COLUMN_SIZE),
            rotation: Quat::IDENTITY,
        });
    }
    out
}

/// Where a tunnel tile's middle is, on the floor.
pub fn tunnel_centre(t: &Tuning, li: usize, level: &Level, tile: &TunnelTile) -> Vec3 {
    let (a, b) = (
        cell_world(t, li, level, tile.at),
        cell_world(
            t,
            li,
            level,
            (tile.at.0 + TUNNEL_CELLS - 1, tile.at.1 + TUNNEL_CELLS - 1),
        ),
    );
    (a + b) / 2.0
}

/// How many wall segments a curve's rounded corner is cut into, and its radius (m).
const CURVE_SEGMENTS: usize = 5;
const CURVE_RADIUS: f32 = TUNNEL_TILE / 2.0;
/// A curved wall's thickness (m).
const CURVE_WALL: f32 = 0.4;

/// A tunnel curve's rounded outer corner, as wall boxes (the tile is open N
/// and E in its own frame, so the outer corner is SW: an arc of 6 m round its
/// middle from the W edge to the S edge). The straight walls come from the
/// plan like any cell's.
pub fn tunnel_walls(t: &Tuning, li: usize, level: &Level) -> Vec<Block> {
    let mut out = Vec::new();
    for tile in level
        .tunnel
        .iter()
        .chain(&level.galleries)
        .filter(|p| p.piece == TunnelPiece::Curve)
    {
        let (mid, r) = (tunnel_centre(t, li, level, tile), tile.rotation());
        let at = |k: usize| {
            let a = std::f32::consts::PI * (1.0 - 0.5 * k as f32 / CURVE_SEGMENTS as f32);
            Vec2::new(CURVE_RADIUS * a.cos(), CURVE_RADIUS * a.sin())
        };
        for k in 0..CURVE_SEGMENTS {
            let (p, q) = (at(k), at(k + 1));
            let d = q - p;
            // The chord's middle, pushed out onto the arc (and a little past it).
            let m = (p + q) / 2.0;
            let m = m.normalize() * (CURVE_RADIUS + CURVE_WALL / 2.0);
            out.push(Block {
                centre: mid + r * Vec3::new(m.x, ROOM_H / 2.0, m.y),
                size: Vec3::new(CURVE_WALL, ROOM_H, d.length() + 0.1),
                rotation: r * Quat::from_rotation_y(d.x.atan2(d.y)),
            });
        }
    }
    out
}

/// Where a tunnel's columns stand (on the floor): a row down the middle of
/// every tile, between the two tracks, three to a straight and two along a
/// curve. None where they'd stand in the stairs or the landing.
pub fn tunnel_columns(t: &Tuning, li: usize, level: &Level) -> Vec<Vec3> {
    let mut out = Vec::new();
    for tile in level.tunnel.iter().chain(&level.galleries) {
        let (mid, r) = (tunnel_centre(t, li, level, tile), tile.rotation());
        let local: Vec<Vec3> = match tile.piece {
            TunnelPiece::Straight => [-4.0, 0.0, 4.0]
                .into_iter()
                .map(|z| Vec3::new(0.0, 0.0, z))
                .collect(),
            // Round the NE corner at 30 and 60 degrees, 6 m out.
            TunnelPiece::Curve => [30.0_f32, 60.0]
                .into_iter()
                .map(|deg| {
                    let a = deg.to_radians();
                    Vec3::new(
                        TUNNEL_TILE / 2.0 - CURVE_RADIUS * a.cos(),
                        0.0,
                        -TUNNEL_TILE / 2.0 + CURVE_RADIUS * a.sin(),
                    )
                })
                .collect(),
        };
        for p in local {
            let at = mid + r * p;
            // Clear of the cells that aren't plain tunnel floor.
            let clear = [(-0.6, -0.6), (0.6, -0.6), (-0.6, 0.6), (0.6, 0.6)]
                .into_iter()
                .all(|(dx, dz)| {
                    cell_of(t, level, at + Vec3::new(dx, 0.0, dz))
                        .is_none_or(|c| matches!(level.fill(c).kind, Kind::Tunnel | Kind::Gallery))
                });
            if clear {
                out.push(at);
            }
        }
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
        .filter(|&(ri, _)| room_kind(t, li, level, ri).is_none_or(|k| k.columns))
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
        .map(|c| cell_world(t, li, level, c) + Vec3::new(CELL / 2.0, 0.0, CELL / 2.0))
        .collect()
}

/// Where the corridors' columns stand (on the floor): every outside wall
/// corner, where three walkable cells round a grid corner join up and the
/// fourth is walled off from them. Worked out from the plan, so a column is
/// always on the corner whatever the tiles around it are (the tile models
/// used to carry their own, which missed straights and stood wrong next to
/// rooms).
pub fn corridor_columns(t: &Tuning, li: usize, level: &Level) -> Vec<Vec3> {
    use wfc::Side::{East, South};
    // Any cell you walk in (corridors, rooms, landings, stairs, platforms): the
    // wall pieces stop short of an outside corner, so a column must close it
    // whatever lies round it.
    let corridor = |c: wfc::Cell| {
        let f = level.fill(c);
        f.mask != 0 && !matches!(f.kind, Kind::Under | Kind::Tunnel | Kind::Gallery)
    };
    let mut out = Vec::new();
    for z in 0..level.depth.saturating_sub(1) {
        for x in 0..level.width.saturating_sub(1) {
            // The four cells round the corner at the SE of (x, z).
            let (tl, tr, bl, br) = ((x, z), (x + 1, z), (x, z + 1), (x + 1, z + 1));
            // Each quadrant, the two it borders across the corner's edges,
            // and the edges (cell, side) between them.
            let quads = [
                (
                    br,
                    (tl, tr, bl),
                    [(tl, East), (tl, South)],
                    [(tr, South), (bl, East)],
                ),
                (
                    bl,
                    (tr, tl, br),
                    [(tl, East), (tr, South)],
                    [(tl, South), (bl, East)],
                ),
                (
                    tr,
                    (bl, tl, br),
                    [(tl, South), (bl, East)],
                    [(tl, East), (tr, South)],
                ),
                (
                    tl,
                    (br, tr, bl),
                    [(tr, South), (bl, East)],
                    [(tl, East), (tl, South)],
                ),
            ];
            let outside = quads.iter().any(|&(_, (a, b, c), joined, walls)| {
                [a, b, c].into_iter().all(corridor)
                    && joined.iter().all(|&(cell, side)| level.open(cell, side))
                    && walls.iter().all(|&(cell, side)| !level.open(cell, side))
            });
            if outside {
                out.push(cell_world(t, li, level, tl) + Vec3::new(CELL / 2.0, 0.0, CELL / 2.0));
            }
        }
    }
    out
}

/// Where a tall room's deck corners need a post (on the sunken floor, up to
/// the ceiling): every grid corner where the same room's deck and sunken
/// floor meet other than along a straight edge. The faces under the deck stop
/// short of each other there, and the empty space under the deck showed
/// through the slot as the clear colour.
pub fn deck_columns(t: &Tuning, li: usize, level: &Level) -> Vec<Vec3> {
    let mut out = Vec::new();
    for z in 0..level.depth.saturating_sub(1) {
        for x in 0..level.width.saturating_sub(1) {
            let quads = [(x, z), (x + 1, z), (x, z + 1), (x + 1, z + 1)];
            let Kind::Room(r) = level.fill(quads[0]).kind else {
                continue;
            };
            if quads.iter().any(|&c| level.fill(c).kind != Kind::Room(r)) {
                continue;
            }
            let sunk: Vec<bool> = quads.iter().map(|&c| level.sunk(c)).collect();
            let n = sunk.iter().filter(|&&s| s).count();
            // A straight edge (two side by side) needs nothing; one, three
            // or a diagonal pair is a corner.
            let straight = n == 2 && (sunk[0] == sunk[1] || sunk[0] == sunk[2]);
            if n == 0 || n == 4 || straight {
                continue;
            }
            out.push(
                cell_world(t, li, level, quads[0]) + Vec3::new(CELL / 2.0, -level.sink, CELL / 2.0),
            );
        }
    }
    out
}

/// A tall hall's columns (on its sunken floor): every inside corner where
/// four of its sunken cells meet, standing up to the ceiling.
pub fn pit_columns(t: &Tuning, li: usize, level: &Level) -> Vec<Vec3> {
    level
        .rooms
        .iter()
        .enumerate()
        .filter(|&(ri, _)| level.set_piece(ri))
        .filter(|&(ri, _)| room_kind(t, li, level, ri).is_none_or(|k| k.columns))
        .flat_map(|(ri, r)| {
            r.inner_corners().filter(move |c| {
                [(0, 0), (1, 0), (0, 1), (1, 1)].iter().all(|(dx, dz)| {
                    let n = (c.0 + dx, c.1 + dz);
                    level.fill(n).kind == Kind::Room(ri as u8) && level.sunk(n)
                })
            })
        })
        .map(|c| cell_world(t, li, level, c) + Vec3::new(CELL / 2.0, -level.sink, CELL / 2.0))
        .collect()
}

/// A column standing at `at` on level `li` goes up to its ceiling.
fn column_height(li: usize, at: Vec3) -> f32 {
    (level_floor(li) + ROOM_H - at.y).max(ROOM_H)
}

/// Whether a level is pitch dark (no working lights): never the top levels,
/// then by chance from [`Lights::dark_from`] down. Seeded, the same on every peer.
pub fn dark_level(t: &Tuning, level: usize) -> bool {
    level >= t.light.dark_from.max(1) && dress_roll(t, [level, 0xda7c, 1, 0]) < t.light.dark_chance
}

/// Whether a cell's ceiling light still works: most are broken, and none on a
/// pitch-dark level. Seeded, the same on every peer.
pub fn cell_lit(t: &Tuning, level: usize, c: wfc::Cell) -> bool {
    !dark_level(t, level) && dress_roll(t, [level, c.0, c.1, 0x116]) < stage(t, level).working
}

/// The cell a point on a level lies in (the inverse of [`cell_world`]), if
/// it's on the grid.
pub fn cell_of(t: &Tuning, l: &Level, p: Vec3) -> Option<wfc::Cell> {
    let x = ((p.x - t.station_at.0) / CELL - 0.5).round() as i32;
    let z = ((p.z - t.station_at.1) / CELL - 0.5).round() as i32;
    l.local((x, z))
}

/// A broken light's skin of a glowing material: no glow, the colour dimmed
/// by `tint` (dead, dusty tubes). Anything that doesn't glow is left alone.
pub fn broken_skin(m: &StandardMaterial, tint: f32) -> Option<StandardMaterial> {
    let e = m.emissive;
    if e.red.max(e.green).max(e.blue) <= 0.01 && m.emissive_texture.is_none() {
        return None;
    }
    let c = m.base_color.to_linear();
    let tint = tint.clamp(0.0, 1.0);
    Some(StandardMaterial {
        emissive: LinearRgba::BLACK,
        emissive_texture: None,
        base_color: Color::LinearRgba(LinearRgba::new(
            c.red * tint,
            c.green * tint,
            c.blue * tint,
            c.alpha,
        )),
        perceptual_roughness: m.perceptual_roughness.max(0.6),
        ..m.clone()
    })
}

/// A number from 0 to 1 for this station, level, room, cell and slot (the
/// rooms' dressing doesn't touch the plan's random numbers).
/// Where a level's service doors go: on walled sides of corridor and room
/// cells you can walk to (not the stairs, landings, platforms or a locker
/// bank, nor a door side), spread out (each as far as can be from the ones
/// before). Seeded by the station and level, never the plan's own numbers.
pub fn service_doors(t: &Tuning, li: usize, level: &Level) -> Vec<(wfc::Cell, wfc::Side)> {
    let d = &t.hunt.ducts;
    if d.doors.1 == 0 || level.cells.is_empty() {
        return Vec::new();
    }
    let reach = level.distances();
    let set = TILES.get();
    let mut spots: Vec<(wfc::Cell, wfc::Side)> = Vec::new();
    for z in 0..level.depth {
        for x in 0..level.width {
            let c = (x, z);
            let i = z * level.width + x;
            let f = level.fill(c);
            let ok = match f.kind {
                Kind::Corridor(Some(p)) => !set.pieces.get(p as usize).is_some_and(|p| p.hide),
                // (The fallback's open hall.)
                Kind::Corridor(None) => f.mask != 0,
                Kind::Room(_) => !level.sunk(c),
                _ => false,
            };
            if !ok || f.track != 0 || reach[i].is_none() {
                continue;
            }
            for s in wfc::Side::ALL {
                if f.mask & bit(s) == 0 {
                    spots.push((c, s));
                }
            }
        }
    }
    if spots.is_empty() {
        return Vec::new();
    }
    let (lo, hi) = (d.doors.0.min(d.doors.1), d.doors.1);
    let n = lo + (dress_roll(t, [li, 0xd0c7, 0, 0]) * (hi - lo + 1) as f32) as usize;
    let first = (dress_roll(t, [li, 0xd0c7, 1, 0]) * spots.len() as f32) as usize;
    let mut out = vec![spots[first.min(spots.len() - 1)]];
    let gap = |a: wfc::Cell, b: wfc::Cell| a.0.abs_diff(b.0) + a.1.abs_diff(b.1);
    while out.len() < n.min(spots.len()) {
        let Some(&next) = spots.iter().max_by_key(|s| {
            out.iter().map(|o| gap(o.0, s.0)).min().unwrap_or(0) * 4
                + (dress_roll(t, [li, 0xd0c7, s.0.0, s.0.1 * 4 + s.1.index()]) * 3.0) as usize
        }) else {
            break;
        };
        if out.iter().any(|o| o.0 == next.0) {
            break;
        }
        out.push(next);
    }
    out
}

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
pub fn room_kind<'a>(t: &'a Tuning, li: usize, l: &Level, ri: usize) -> Option<&'a RoomKind> {
    let r = l.rooms.get(ri)?;
    let set_piece = l.set_piece(ri);
    let (a, b) = r.size;
    let fits: Vec<&RoomKind> = t
        .rooms
        .kinds
        .iter()
        .filter(|k| {
            k.weight > 0.0
                && k.from <= li
                && k.set_piece == set_piece
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
        let Some(kind) = room_kind(t, li, level, ri) else {
            continue;
        };
        for (ci, c) in r.cells().enumerate() {
            let f = level.fill(c);
            if f.kind != Kind::Room(ri as u8) {
                continue;
            }
            let at = cell_world(t, li, level, c) - Vec3::Y * level.drop(c);
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
            let at = cell_world(t, li, level, c);
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

/// A car standing on a platform's track, and its colliders.
#[derive(Clone, Debug, PartialEq)]
pub struct ParkedCar {
    /// Under `assets/models/`.
    pub model: String,
    pub transform: Transform,
    pub blocks: Vec<Block>,
}

/// A car's length (m, along its x) and half its width.
const CAR_LEN: f32 = 18.9;
const CAR_HALF_W: f32 = 1.525;
/// An open car: its floor's top and its roof's underside (above the rails),
/// its doors' middles along x and their width, and its seats (middle along
/// x, length).
const CAR_FLOOR: f32 = 1.1;
const CAR_ROOF: f32 = 3.3;
const CAR_DOORS: [f32; 4] = [-6.3, -2.1, 2.1, 6.3];
const CAR_DOOR_W: f32 = 1.3;
const CAR_SEATS: [(f32, f32); 5] = [(-7.9, 1.8), (-4.2, 2.8), (0.0, 2.8), (4.2, 2.8), (7.9, 1.8)];

/// The colliders of a car, in its own frame (wheels on the rails at y = 0,
/// +z the platform side): one solid box, or an open car's floor, roof,
/// walls (gaps at the doors on +z) and seats.
pub fn car_blocks(open: bool) -> Vec<Block> {
    let b = |centre: Vec3, size: Vec3| Block {
        centre,
        size,
        rotation: Quat::IDENTITY,
    };
    let (l, w) = (CAR_LEN / 2.0, CAR_HALF_W);
    if !open {
        return vec![b(
            Vec3::new(0.0, 1.875, 0.0),
            Vec3::new(CAR_LEN, 3.75, 2.0 * w),
        )];
    }
    let wall_h = CAR_ROOF - CAR_FLOOR;
    let wall_y = (CAR_ROOF + CAR_FLOOR) / 2.0;
    let mut out = vec![
        // Floor and underframe, roof.
        b(
            Vec3::new(0.0, (0.2 + CAR_FLOOR) / 2.0, 0.0),
            Vec3::new(CAR_LEN, CAR_FLOOR - 0.2, 2.0 * w),
        ),
        b(
            Vec3::new(0.0, CAR_ROOF + 0.2, 0.0),
            Vec3::new(CAR_LEN, 0.4, 2.0 * w),
        ),
        // The far side (doors shut) and the ends.
        b(
            Vec3::new(0.0, wall_y, -w + 0.04),
            Vec3::new(CAR_LEN, wall_h, 0.08),
        ),
        b(
            Vec3::new(-l + 0.04, wall_y, 0.0),
            Vec3::new(0.08, wall_h, 2.0 * w),
        ),
        b(
            Vec3::new(l - 0.04, wall_y, 0.0),
            Vec3::new(0.08, wall_h, 2.0 * w),
        ),
    ];
    // The platform side, between the open doors.
    let mut from = -l;
    for d in CAR_DOORS.iter().copied().chain([l + CAR_DOOR_W / 2.0]) {
        let to = (d - CAR_DOOR_W / 2.0).min(l);
        if to > from {
            out.push(b(
                Vec3::new((from + to) / 2.0, wall_y, w - 0.04),
                Vec3::new(to - from, wall_h, 0.08),
            ));
        }
        from = d + CAR_DOOR_W / 2.0;
    }
    for (x, len) in CAR_SEATS {
        for z in [-1.25, 1.25] {
            out.push(b(
                Vec3::new(x, CAR_FLOOR + 0.24, z),
                Vec3::new(len, 0.48, 0.45),
            ));
        }
    }
    out
}

/// The cars standing on level `li`'s tracks: on each platform (a straight
/// run of at least three track cells, tunnel ends included) with
/// `trains.chance`, as many as fit end to end, centred on it, on the rails,
/// their open sides to the platform. A car's ends reach into the tunnels.
pub fn parked_cars(t: &Tuning, li: usize, level: &Level) -> Vec<ParkedCar> {
    let mut out = Vec::new();
    let total: f32 = t.trains.cars.iter().map(|c| c.weight.max(0.0)).sum();
    if total <= 0.0 || t.trains.chance <= 0.0 {
        return out;
    }
    for axis_x in [true, false] {
        let along = if axis_x {
            bit(wfc::Side::East) | bit(wfc::Side::West)
        } else {
            bit(wfc::Side::North) | bit(wfc::Side::South)
        };
        let (outer, inner) = if axis_x {
            (level.depth, level.width)
        } else {
            (level.width, level.depth)
        };
        let cell = |o: usize, i: usize| if axis_x { (i, o) } else { (o, i) };
        for o in 0..outer {
            let mut i = 0;
            while i < inner {
                let f = level.fill(cell(o, i));
                if f.track == 0 || f.track & !along != 0 {
                    i += 1;
                    continue;
                }
                // The run of track cells along this axis, turned the same way.
                let start = i;
                while i < inner && {
                    let g = level.fill(cell(o, i));
                    g.track != 0 && g.track & !along == 0 && g.turns == f.turns
                } {
                    i += 1;
                }
                let len = i - start;
                if len < 3 {
                    continue;
                }
                let key = 2 << 20 | (o * inner + start);
                if dress_roll(t, [li, key, 0, 0]) >= t.trains.chance {
                    continue;
                }
                let r = f.rotation();
                // The deck is the piece's -z side: a car turns its +z to it.
                let turn = r * Quat::from_rotation_y(std::f32::consts::PI);
                let first = cell_world(t, li, level, cell(o, start));
                let last = cell_world(t, li, level, cell(o, i - 1));
                let n = (len / 3).max(1);
                for k in 0..n {
                    let mut pick = dress_roll(t, [li, key, 1, k]) * total;
                    let Some(car) = t.trains.cars.iter().find(|c| {
                        pick -= c.weight.max(0.0);
                        pick < 0.0
                    }) else {
                        continue;
                    };
                    let mid = first.lerp(
                        last,
                        (k as f32 + 0.5) / n as f32 * (len as f32) / (len as f32 - 1.0)
                            - 0.5 / (len as f32 - 1.0),
                    );
                    let at = mid + r * Vec3::new(0.0, RAIL_TOP, CAR_Z);
                    out.push(ParkedCar {
                        model: car.model.clone(),
                        transform: Transform::from_translation(at).with_rotation(turn),
                        blocks: car_blocks(car.open)
                            .into_iter()
                            .map(|bl| Block {
                                centre: at + turn * bl.centre,
                                size: bl.size,
                                rotation: turn * bl.rotation,
                            })
                            .collect(),
                    });
                }
            }
        }
    }
    out
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

/// How many times the stairs model is laid end to end down a drop of `drop`
/// m (once for a tall room's, more for an atrium's).
pub fn stair_flights(drop: f32) -> usize {
    (drop / STAIRS_MODEL_DROP - 0.05).ceil().max(1.0) as usize
}

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
            let at = cell_world(t, li, level, c);
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
                        // (A doorway onto the level below leaves its lower wall out.)
                        if !level.low_doors.contains(&(c, s)) {
                            out.push(put(ShapePiece::Wall, floor, turn, Vec3::ONE, None));
                        }
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
                        let run = level.run;
                        let tilt = (drop / run).atan();
                        let ramp = Quat::from_rotation_x(tilt);
                        let normal = ramp * Vec3::Y;
                        let local = Vec3::new(0.0, -drop / 2.0, run / 2.0) - normal * 0.15;
                        // A long flight (an atrium's) is the stairs model
                        // several times over, one after another.
                        let flights = stair_flights(drop);
                        let (fd, fr) = (drop / flights as f32, run / flights as f32);
                        for k in 0..flights {
                            let from = edge + down * Vec3::new(0.0, -fd * k as f32, fr * k as f32);
                            out.push(put(
                                ShapePiece::Stairs,
                                from,
                                down,
                                Vec3::new(1.0, fd / STAIRS_MODEL_DROP, fr / STAIRS_RUN),
                                (k == 0).then_some(Block {
                                    centre: edge + down * local,
                                    size: Vec3::new(
                                        STAIRS_W,
                                        0.3,
                                        (run * run + drop * drop).sqrt(),
                                    ),
                                    rotation: down * ramp,
                                }),
                            ));
                        }
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
/// The signed ways on a level: the main way (`path`), then from each other
/// landing the way to the nearest stairs down (cells already signed stop it).
pub fn signed_ways(level: &Level) -> Vec<Vec<wfc::Cell>> {
    let mut out = vec![level.path.clone()];
    let downs = level.downs();
    for (up, _) in level.ups().into_iter().filter(|u| u.0 != level.arrive) {
        let d = wfc::distances(level.width, level.depth, up, |c, s| level.open(c, s));
        let Some(&(goal, _)) = downs
            .iter()
            .filter(|g| d[g.0.1 * level.width + g.0.0].is_some())
            .min_by_key(|g| d[g.0.1 * level.width + g.0.0])
        else {
            continue;
        };
        let way = path_to(
            &Level {
                arrive: up,
                goal,
                ..level.clone()
            },
            &d,
        );
        if !way.is_empty() {
            out.push(way);
        }
    }
    out
}

pub fn signs(t: &Tuning, li: usize, level: &Level) -> Vec<Sign> {
    let mut out = Vec::new();
    let mut signed: HashSet<wfc::Cell> = HashSet::new();
    for p in &signed_ways(level) {
        signs_along(t, li, level, p, &mut signed, &mut out);
    }
    out
}

/// Signs along one way (see [`signs`]), not on cells already signed.
fn signs_along(
    t: &Tuning,
    li: usize,
    level: &Level,
    p: &[wfc::Cell],
    signed: &mut HashSet<wfc::Cell>,
    out: &mut Vec<Sign>,
) {
    let landing = p
        .first()
        .is_some_and(|&c| level.fill(c).kind == Kind::Landing);
    for (i, &c) in p.iter().enumerate() {
        if i == 0 && landing {
            continue;
        }
        if !signed.insert(c) {
            continue;
        }
        let at = cell_world(t, li, level, c);
        let step = |a: wfc::Cell, b: wfc::Cell| {
            Vec3::new(b.0 as f32 - a.0 as f32, 0.0, b.1 as f32 - a.1 as f32)
        };
        let next = p.get(i + 1).map(|&n| step(c, n));
        let came = if i > 0 { Some(step(p[i - 1], c)) } else { next };
        let Some(came) = came else { continue };
        // Its front faces back along the way you came.
        let yaw = (-came.x).atan2(-came.z);
        if i + 1 == p.len() {
            if let Some((_, side)) = level.downs().into_iter().find(|d| d.0 == c) {
                let entry = dir(side);
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
}

/// Where you arrive at the top of the station.
pub fn start_at(t: &Tuning, levels: &[Level]) -> Vec3 {
    levels
        .first()
        .map_or(Vec3::ZERO, |l| cell_world(t, 0, l, l.arrive))
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
        .then(|| cell_world(t, li, l, l.goal) + f.rotation() * Vec3::new(0.0, 0.0, -1.2))
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
    /// The next level, being planned off the main thread.
    planning: Option<bevy::tasks::Task<StationPlan>>,
}

impl Station {
    /// The top [`Tuning::plan_ahead`] levels planned.
    pub fn new(t: &Tuning, set: &TileSet) -> Self {
        let mut plan = StationPlan::new(t, set);
        plan.plan_to(t, t.plan_ahead.max(1));
        Station {
            plan,
            want: 0,
            planning: None,
        }
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
    // (One past the deepest built level: a two-storey room's walls depend on
    // the level under it, so it's planned before the level above is built.)
    let by_body = deepest.map_or(ahead + 1, |l| l + 2 + ahead);
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
/// of the plan (or a feature asked for more), one at a time on the async
/// compute pool so a slow solve never holds a frame up (in the browser the
/// pool is the main thread, so it still does). Levels follow on from the one
/// above, so they're still planned in order and come out the same.
pub fn plan_station(mut station: ResMut<Station>, walkers: Query<&Walker>) {
    let station = &mut *station;
    if let Some(task) = station.planning.as_mut() {
        let Some(next) = bevy::tasks::futures::check_ready(task) else {
            return;
        };
        station.planning = None;
        station.plan.take(next);
    }
    let t = tuning();
    // A deep start (`FRIENDSLOP_START=fulton_<n>`) plans down to its level
    // right away, before anyone lands there (testing only: it holds a frame).
    if station.planning.is_none() && station.plan.levels.len() < start_wants() {
        station.plan.plan_to(&t, start_wants());
    }
    let deepest = body_levels(walkers.iter()).into_iter().max();
    let n = levels_wanted(&t, deepest, station.want.max(start_wants()));
    if station.plan.levels.len() < n && !station.plan.done() {
        let mut next = station.plan.carry_on();
        station.planning = Some(
            bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default).spawn(
                async move {
                    next.plan_next(&t);
                    next
                },
            ),
        );
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
                flashlight,
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
    for (down, entry) in level.downs() {
        let side = dir(entry);
        let e = crate::area::doorway(
            commands,
            cell_world(t, li, level, down) + side * (CELL / 2.0 - 0.4),
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
            let a = cell_world(&t, li, l, r.at);
            let b = cell_world(&t, li, l, (r.at.0 + r.size.0 - 1, r.at.1 + r.size.1 - 1));
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
    if bot_run() || god() {
        return bot_starts();
    }
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

/// Is the profiling bot driving (`FRIENDSLOP_BOT`)? Then the hunter never
/// catches anyone and every level has starts of its own.
pub fn bot_run() -> bool {
    static BOT: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *BOT.get_or_init(|| std::env::var("FRIENDSLOP_BOT").is_ok_and(|r| !r.is_empty()))
}

/// A service door in the world (rigged: clips `closed`, `open`, `close`).
#[derive(Component)]
pub struct ServiceDoor;

/// The service door's model path, kept for the rig (which wants a `'static`
/// path; the data file names it once).
fn service_door_model(path: &str) -> &'static str {
    static PATH: std::sync::Mutex<Option<&'static str>> = std::sync::Mutex::new(None);
    let Ok(mut kept) = PATH.lock() else {
        return "models/features/fs_service_door.gltf";
    };
    match *kept {
        Some(p) if p == path => p,
        _ => {
            let p: &'static str = Box::leak(path.to_string().into_boxed_str());
            *kept = Some(p);
            p
        }
    }
}

/// How many levels a `FRIENDSLOP_START=fulton_<n>` start needs planned before
/// you land there (0 for any other start): its level, and the two under it.
fn start_wants() -> usize {
    static WANT: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *WANT.get_or_init(|| {
        std::env::var("FRIENDSLOP_START")
            .ok()
            .and_then(|s| {
                let rest = s.strip_prefix("fulton_")?;
                rest.split('_').next()?.parse::<usize>().ok()
            })
            .map_or(0, |n| n + 3)
    })
}

/// Testing (`FRIENDSLOP_GOD=1`): the hunter never catches anyone and every
/// level has starts of its own (`FRIENDSLOP_START=fulton_3`), as for the bot,
/// but you drive.
pub fn god() -> bool {
    static GOD: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *GOD.get_or_init(|| std::env::var("FRIENDSLOP_GOD").is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// The bot's starts: `fulton` (the top), and on every level `fulton_<n>`
/// where you arrive and `fulton_<n>_stairs` at its stairs down (all planned
/// once, the first time anyone looks).
/// Where on cell `c` of level `li` someone walks: its middle, or on a
/// track cell the middle of its platform deck (the bed is where a parked
/// train stands).
pub fn walk_point(t: &Tuning, li: usize, l: &Level, c: wfc::Cell) -> Vec3 {
    let at = cell_world(t, li, l, c);
    let f = l.fill(c);
    if f.track == 0 {
        return at;
    }
    at + f.rotation() * Vec3::new(0.0, 0.0, (-CELL / 2.0 + PLATFORM_EDGE_Z) / 2.0)
}

/// How many levels the bot walks down (`fulton_walk_<k>`).
const BOT_WALK_LEVELS: usize = 14;

fn bot_starts() -> Vec<(String, Vec3)> {
    static ALL: std::sync::Mutex<Vec<(String, Vec3)>> = std::sync::Mutex::new(Vec::new());
    if let Ok(all) = ALL.lock()
        && !all.is_empty()
    {
        return all.clone();
    }
    let t = tuning();
    let mut plan = StationPlan::new(&t, &TILES.get());
    plan.plan_to(&t, usize::MAX);
    if plan.levels.is_empty() {
        return Vec::new();
    }
    let up = Vec3::Y * 0.1;
    let mut out = vec![("fulton".to_string(), start_at(&t, &plan.levels))];
    for (li, l) in plan.levels.iter().enumerate() {
        // (One cell in: the arrival cell is under the stairs above.)
        let arrive = l.path.get(1).or(l.path.first()).copied().unwrap_or(l.goal);
        out.push((format!("fulton_{li}"), cell_world(&t, li, l, arrive) + up));
        out.push((
            format!("fulton_{li}_stairs"),
            cell_world(&t, li, l, l.goal) + up,
        ));
    }
    // The way down, `fulton_walk_<k>` (the bot's `Follow("fulton_walk")`):
    // each level's path from where you arrive to its stairs, through the
    // top `BOT_WALK_LEVELS` levels.
    let walk = plan
        .levels
        .iter()
        .enumerate()
        .take(BOT_WALK_LEVELS)
        .flat_map(|(li, l)| l.path.iter().map(move |&c| (li, l, c)))
        .map(|(li, l, c)| walk_point(&t, li, l, c) + up);
    for (k, at) in walk.enumerate() {
        out.push((format!("fulton_walk_{k}"), at));
    }
    // Only keep starts planned from the real data (a lookup before the data
    // files loaded plans another station, and its starts are somewhere else).
    let loaded = TUNING.loaded() && TILES.loaded() && LAYOUT.loaded();
    if loaded && let Ok(mut all) = ALL.lock() {
        *all = out.clone();
    }
    out
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
            let mid = cell_world(&t, li, level, (0, 0)).lerp(
                cell_world(&t, li, level, (level.width - 1, level.depth - 1)),
                0.5,
            );
            commands.spawn((
                friendslop_base::probes::probe(
                    &assets,
                    "fulton",
                    mid + Vec3::Y * (ROOM_H / 2.0),
                    Vec3::new(w, ROOM_H + 0.4, d),
                    // A pitch-dark level reflects nothing.
                    if dark_level(&t, li) {
                        0.0
                    } else {
                        t.light.probe
                    },
                ),
                ChildOf(root),
            ));
        }
        for z in 0..level.depth {
            for x in 0..level.width {
                let f = level.fill((x, z));
                let skin = set.skin(li);
                // A skinned cell: the skin's floor, and its walls (below).
                let skinned = skin.is_some()
                    && f.mask != 0
                    && match f.kind {
                        Kind::Corridor(Some(i)) => set
                            .pieces
                            .get(i as usize)
                            .is_some_and(|p| p.track.is_empty()),
                        Kind::Room(_) | Kind::Gallery | Kind::Corridor(None) => true,
                        _ => false,
                    };
                // A train tunnel cell: its bed and rails, walls, and a buffer
                // stop at its dead end, all a track bed's depth down.
                if f.kind == Kind::Rails {
                    let at = cell_world(&t, li, level, (x, z)) - Vec3::Y * TRACK_DROP;
                    let (curve, turns) = rails_piece(f.mask);
                    let file = if curve {
                        &set.rails.curve
                    } else {
                        &set.rails.straight
                    };
                    let turn =
                        |n: u8| Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2 * n as f32);
                    if !file.is_empty() {
                        commands.spawn((
                            model(&assets, file),
                            Merge,
                            Transform::from_translation(at).with_rotation(turn(turns)),
                            ChildOf(root),
                        ));
                    }
                    for side in wfc::Side::ALL {
                        if f.mask & bit(side) != 0 || set.rails.wall.is_empty() {
                            continue;
                        }
                        commands.spawn((
                            model(&assets, &set.rails.wall),
                            Merge,
                            Transform::from_translation(at).with_rotation(turn(side.index() as u8)),
                            ChildOf(root),
                        ));
                    }
                    if f.mask.count_ones() == 1 && !set.rails.buffer.is_empty() {
                        // Facing back along the track, its back to the end wall.
                        let open = wfc::Side::ALL
                            .into_iter()
                            .find(|&s| f.mask & bit(s) != 0)
                            .unwrap_or(wfc::Side::North);
                        let (back, ahead) = (dir(open.opposite()), dir(open));
                        // (Modelled facing +x, along the rails.)
                        commands.spawn((
                            model(&assets, &set.rails.buffer),
                            Transform::from_translation(at + back * (CELL / 2.0 - 1.2))
                                .with_rotation(Quat::from_rotation_y((-ahead.z).atan2(ahead.x))),
                            ChildOf(root),
                        ));
                    }
                    continue;
                }
                let file = match f.kind {
                    _ if skinned => skin.map(|s| s.floor.as_str()),
                    Kind::Corridor(Some(i)) => set.pieces.get(i as usize).map(|p| {
                        // A tunnel end whose mouth runs on into a train tunnel.
                        let mouth = p.track.len() == 1 && f.mask != f.track;
                        if mouth && !p.mouth_open.is_empty() {
                            p.mouth_open.as_str()
                        } else {
                            set.themed(li, p.model.as_str())
                        }
                    }),
                    Kind::Stairs => Some(set.stairs.as_str()),
                    Kind::Platform => Some(set.platform.as_str()),
                    Kind::Room(_) | Kind::Tunnel | Kind::Gallery => Some(set.room.floor.as_str()),
                    Kind::Corridor(None) | Kind::Landing | Kind::Under | Kind::Rails => None,
                };
                let Some(file) = file else { continue };
                let at = cell_world(&t, li, level, (x, z));
                let sunk = level.sunk((x, z));
                // Its ceiling tubes light it, if they still work (most are
                // broken; no shadows: there can still be a lot of them).
                if t.light.cell_lumens > 0.0 && cell_lit(&t, li, (x, z)) {
                    commands.spawn((
                        CellLight,
                        PointLight {
                            color: set
                                .skin(li)
                                .and_then(|s| s.light)
                                .or_else(|| set.theme(li).and_then(|th| th.light))
                                .map_or(Color::srgb(0.9, 0.95, 1.0), |(r, g, b)| {
                                    Color::srgb(r, g, b)
                                }),
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
                // A room cell's walls: the wall piece turned onto each walled
                // side (any skinned cell's: the skin's walls).
                if (skinned || matches!(f.kind, Kind::Room(_) | Kind::Gallery))
                    && !set.room.wall.is_empty()
                {
                    for side in wfc::Side::ALL {
                        if f.mask & bit(side) != 0 {
                            continue;
                        }
                        let wall = match skin.filter(|_| skinned) {
                            Some(s) => s.wall(&t, li, (x, z), side),
                            None => set.room.wall.as_str(),
                        };
                        let turn = Fill {
                            turns: side.index() as u8,
                            ..f
                        };
                        commands.spawn((
                            model(&assets, wall),
                            Merge,
                            Transform::from_translation(at).with_rotation(turn.rotation()),
                            ChildOf(root),
                        ));
                    }
                }
            }
        }
        // A tunnel level's tiles (walls, rails; the floor is the cells').
        for tile in &level.tunnel {
            let mut file = set.tunnel.file(tile.piece);
            let mut rotation = tile.rotation();
            // The tile with the doorway onto the hall, turned so it's on the right side.
            if let Some((mirror, turns)) = tunnel_door(level, *tile) {
                let door = if mirror {
                    &set.tunnel.door_mirror
                } else {
                    &set.tunnel.door
                };
                if !door.is_empty() {
                    file = door;
                    rotation = Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2 * turns as f32);
                }
            }
            if file.is_empty() {
                continue;
            }
            commands.spawn((
                model(&assets, file),
                Merge,
                Transform::from_translation(tunnel_centre(&t, li, level, tile))
                    .with_rotation(rotation),
                ChildOf(root),
            ));
        }
        // A gallery's curves: their rounded outer walls.
        if !set.room.arc.is_empty() {
            for tile in level
                .galleries
                .iter()
                .filter(|g| g.piece == TunnelPiece::Curve)
            {
                commands.spawn((
                    model(&assets, &set.room.arc),
                    Merge,
                    Transform::from_translation(tunnel_centre(&t, li, level, tile))
                        .with_rotation(tile.rotation()),
                    ChildOf(root),
                ));
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
        for car in parked_cars(&t, li, level) {
            commands.spawn((
                WorldAssetRoot(
                    assets
                        .load(GltfAssetLabel::Scene(0).from_asset(format!("models/{}", car.model))),
                ),
                car.transform,
                ChildOf(root),
            ));
        }
        let column = set
            .skin(li)
            .map(|s| s.column.as_str())
            .filter(|c| !c.is_empty())
            .unwrap_or(set.room.column.as_str());
        if !column.is_empty() {
            for at in room_columns(&t, li, level)
                .into_iter()
                .chain(tunnel_columns(&t, li, level))
                .chain(pit_columns(&t, li, level))
                .chain(corridor_columns(&t, li, level))
                .chain(deck_columns(&t, li, level))
            {
                commands.spawn((
                    model(&assets, column),
                    Merge,
                    Transform::from_translation(at).with_scale(Vec3::new(
                        1.0,
                        column_height(li, at) / ROOM_H,
                        1.0,
                    )),
                    ChildOf(root),
                ));
            }
        }
        // The service doors (the hunter's way into the ducts), flush on the
        // wall face, not merged (they're a handful a level).
        if !t.hunt.ducts.model.is_empty() {
            for &(c, side) in &level.service {
                let face = dir(side);
                let at = cell_world(&t, li, level, c) + face * (CELL / 2.0 - WALL_FACE_IN);
                commands.spawn((
                    ServiceDoor,
                    crate::rigged::Rigged {
                        path: service_door_model(&t.hunt.ducts.model),
                    },
                    crate::rigged::RigClip::looping("closed"),
                    // Modelled facing +z with its back on the wall: turned to face in.
                    Transform::from_translation(at)
                        .with_rotation(Quat::from_rotation_y((-face.x).atan2(-face.z))),
                    Visibility::default(),
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
    levels: Query<(Entity, &Children, &LevelModels), Without<LevelMerged>>,
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
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut skins: Local<
        bevy::platform::collections::HashMap<AssetId<StandardMaterial>, Handle<StandardMaterial>>,
    >,
    station: Option<Res<Station>>,
) {
    use friendslop_structures::batch::{MeshBatch, layout};
    let t = tuning();
    for (root, kids, level) in &levels {
        let roots: Vec<Entity> = kids.iter().filter(|&e| pieces.contains(e)).collect();
        if roots.iter().any(|&e| !matches!(pieces.get(e), Ok(true))) {
            continue;
        }
        let mut groups: Vec<(String, Handle<StandardMaterial>, MeshBatch)> = Vec::new();
        for &r in &roots {
            let cell = nodes.get(r).ok().and_then(|(tf, _)| {
                let l = station.as_ref()?.levels().get(level.0)?;
                cell_of(&t, l, tf.translation)
            });
            // Merged by block of cells (a piece off the grid: its own block).
            let block = cell.map_or((usize::MAX, usize::MAX), |c| {
                let n = t.merge_cells.max(1);
                (c.0 / n, c.1 / n)
            });
            // A piece under a broken light wears the broken skin (no glow).
            let broken = cell.is_some_and(|c| !cell_lit(&t, level.0, c));
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
                let mut handle = mat.0.clone();
                if broken {
                    if let Some(h) = skins.get(&handle.id()) {
                        handle = h.clone();
                    } else if let Some(dead) = materials
                        .get(&handle)
                        .and_then(|m| broken_skin(m, t.light.broken_tint))
                    {
                        let h = materials.add(dead);
                        skins.insert(handle.id(), h.clone());
                        handle = h;
                    }
                    if handle != mat.0 {
                        commands.entity(e).insert(MeshMaterial3d(handle.clone()));
                    }
                }
                let (Some(mesh), Some(material)) = (meshes.get(&m.0), materials.get(&handle))
                else {
                    all = false;
                    continue;
                };
                let Some(lay) = layout(mesh) else {
                    all = false;
                    continue;
                };
                let key = format!("{block:?}{}{lay:?}", material_key(material));
                let i = match groups.iter().position(|g| g.0 == key) {
                    Some(i) => i,
                    None => {
                        groups.push((key, handle.clone(), MeshBatch::default()));
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

/// The shadow-casting stand-in for the nearest cell lights: a spot light
/// under the tubes pointing straight down (one shadow map, where a point
/// light's takes six), the point light it stands in for put out.
#[derive(Component)]
struct ShadowTube;

/// How near a cell light at `p` counts as from the camera at `at` (lower is
/// nearer): squared distance, with any light off the camera's floor ranked
/// behind every one on it.
pub fn light_rank(at: Vec3, p: Vec3) -> f32 {
    let off_floor = (p.y - at.y).abs() > STATION_LEVEL_H * 0.75;
    at.distance_squared(p) + if off_floor { 1.0e6 } else { 0.0 }
}

/// Local: only the nearest [`Lights::max_lights`] cell lights are on, and
/// the nearest [`Lights::shadow_lights`] of them are [`ShadowTube`]s.
#[allow(clippy::type_complexity)]
fn light_shadows(
    mut commands: Commands,
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    quality: Option<Res<crate::graphics::GraphicsQuality>>,
    mut lights: Query<
        (&GlobalTransform, &mut PointLight, &mut Visibility),
        (With<CellLight>, Without<ShadowTube>),
    >,
    mut tubes: Query<
        (Entity, &mut Transform, &mut SpotLight, &mut Visibility),
        (With<ShadowTube>, Without<CellLight>),
    >,
) {
    let Some(at) = camera_at(&camera) else { return };
    let t = tuning();
    let n = if quality.is_none_or(|q| q.shadows) {
        t.light.shadow_lights
    } else {
        0
    };
    // (Lights on your own floor first: one a level up or down is behind
    // the floor, and lighting it would take the place of one you can see.)
    let mut all: Vec<_> = lights
        .iter_mut()
        .map(|(g, l, v)| {
            let p = g.translation();
            (light_rank(at, p), p, l, v)
        })
        .collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut tubes: Vec<_> = tubes.iter_mut().collect();
    let mut placed = 0;
    for (i, (_, pos, mut l, mut v)) in all.into_iter().enumerate() {
        if l.shadow_maps_enabled {
            l.shadow_maps_enabled = false;
        }
        let shown = if i < t.light.max_lights && i >= n {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        v.set_if_neq(shown);
        if i >= n {
            continue;
        }
        let spot = SpotLight {
            color: l.color,
            intensity: l.intensity,
            range: l.range,
            outer_angle: t.light.tube_angle.to_radians().clamp(0.1, 1.55),
            inner_angle: (t.light.tube_angle * 0.6).to_radians().clamp(0.05, 1.5),
            shadow_maps_enabled: true,
            ..default()
        };
        placed += 1;
        let place = Transform::from_translation(pos).looking_to(Vec3::NEG_Y, Vec3::Z);
        match tubes.get_mut(i) {
            Some((_, tf, sl, tv)) => {
                tf.set_if_neq(place);
                sl.color = spot.color;
                sl.intensity = spot.intensity;
                sl.range = spot.range;
                sl.outer_angle = spot.outer_angle;
                sl.inner_angle = spot.inner_angle;
                tv.set_if_neq(Visibility::Inherited);
            }
            None => {
                commands.spawn((ShadowTube, spot, place, crate::area::show()));
            }
        }
    }
    // Tubes beyond the shadows wanted (or with no cell lights near): out.
    for (_, _, _, v) in tubes.iter_mut().skip(placed) {
        v.set_if_neq(Visibility::Hidden);
    }
}

/// The station's look (`data/base/place_looks.ron`): no sky, little light, a dim haze.
pub const LOOK: &str = "fulton";

/// The kept-up top levels' look (`data/base/place_looks.ron`): a working station.
pub const LOOK_KEPT: &str = "fulton_kept";

/// The run-down level's look (`data/base/place_looks.ron`): dimmer, a grimy haze.
pub const LOOK_WORN: &str = "fulton_worn";

/// The pitch-dark levels' look (`data/base/place_looks.ron`): no light at all.
pub const LOOK_DARK: &str = "fulton_dark";

/// Local: inside the station the sky is out (`place.rs`); on a pitch-dark
/// level everything is.
fn station_look(here: Res<crate::place::Here>, mut looks: ResMut<crate::place::PlaceLooks>) {
    if in_station(here.0) {
        looks.push(LOOK);
        let t = tuning();
        let level = level_at(here.0.y);
        if let Some(look) = stage(&t, level).look.id() {
            looks.push(look);
        }
        if dark_level(&t, level) {
            looks.push(LOOK_DARK);
        }
    }
}

/// Local: a faint glow that goes with you down there (not on a pitch-dark level).
fn follow_light(
    camera: Query<&GlobalTransform, With<crate::world::MainCamera>>,
    mut light: Query<(&mut Transform, &mut Visibility), With<StationLight>>,
) {
    let Some(at) = camera_at(&camera) else { return };
    let feet = at - Vec3::Y * 1.6;
    let here = in_station(feet) && !dark_level(&tuning(), level_at(feet.y));
    for (mut tf, mut vis) in &mut light {
        tf.translation = at + Vec3::Y * 0.8;
        vis.set_if_neq(if here {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
    }
}

/// Your flashlight while you're in the station (as in the warehouse).
#[derive(Component)]
struct StationFlashlight;

/// Local: going down hands you a flashlight; coming up takes it away.
fn flashlight(
    mut commands: Commands,
    here: Res<crate::place::Here>,
    cameras: Query<Entity, With<crate::world::MainCamera>>,
    lights: Query<Entity, With<StationFlashlight>>,
    quality: Option<Res<crate::graphics::GraphicsQuality>>,
) {
    let want = in_station(here.0);
    let have = !lights.is_empty();
    if want == have {
        return;
    }
    if !want {
        for e in &lights {
            commands.entity(e).despawn();
        }
        return;
    }
    let Ok(camera) = cameras.single() else { return };
    let t = tuning();
    let l = &t.light;
    commands.spawn((
        StationFlashlight,
        SpotLight {
            color: Color::srgb(1.0, 0.95, 0.85),
            intensity: l.flashlight_lumens,
            range: l.flashlight_range,
            outer_angle: l.flashlight_outer.to_radians(),
            inner_angle: l
                .flashlight_inner
                .to_radians()
                .min(l.flashlight_outer.to_radians()),
            shadow_maps_enabled: quality.is_some_and(|q| q.shadows),
            ..default()
        },
        Transform::from_xyz(0.25, -0.2, 0.0),
        ChildOf(camera),
    ));
}

/// Local: the station's tiled halls ring (every world sound gets a long
/// tail) and the `fulton` signal tells the music it's in the station (its own
/// score: ambient, tense, chase; see `fulton_hunt::music_signals`).
fn echo(
    ears: Res<crate::spatial::Ears>,
    mut room: ResMut<crate::spatial::Room>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let t = tuning();
    let inside = in_station(ears.pos - Vec3::Y * 1.6);
    if let Some(mut signals) = signals {
        signals.flag("fulton", inside);
        let level = level_at(ears.pos.y - 1.6);
        signals.flag("fulton_creepy", inside && stage(&t, level).creepy);
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
        load(&LAYOUT);
        ((*TUNING.get()).clone(), (*TILES.get()).clone())
    }

    #[test]
    fn the_layout_file_parses_and_has_every_type() {
        let _ = data();
        let lay = LAYOUT.get();
        assert!(lay.macro_cells >= 1 && !lay.next_to.is_empty());
        for k in [
            LevelKind::Warren,
            LevelKind::Line,
            LevelKind::Hall,
            LevelKind::Atrium,
        ] {
            let ty = lay.types.iter().find(|ty| ty.kind == k);
            assert!(
                ty.is_some_and(|ty| !ty.metas.is_empty() && !ty.sizes.is_empty()),
                "{k:?}"
            );
        }
        assert!(lay.line.run.0 >= 1 && lay.line.run.1 >= lay.line.run.0);
        assert!(lay.hall.doors.0 >= 1 && lay.hall.min.0 >= 1);
        assert!(lay.atrium.drop > 0.0 && lay.atrium.stair_run > 0.0);
    }

    #[test]
    fn the_coarse_layout_is_pure_and_merges_into_rectangles() {
        let _ = data();
        let lay = LAYOUT.get();
        let mut seen_track = false;
        for ty in &lay.types {
            for seed in 0..30 {
                let run = |seed| {
                    coarse(
                        &mut wfc::Rng::new(seed),
                        (6, 4),
                        &ty.metas,
                        &lay.next_to,
                        |m| (m == (0, 0)).then(|| vec![(Meta::Warren, true)]),
                        |_| true,
                    )
                };
                let a = run(seed);
                assert_eq!(a, run(seed), "{:?} seed {seed}", ty.kind);
                let Some(a) = a else { continue };
                assert_eq!(a.meta((0, 0)), Meta::Warren);
                for r in &a.regions {
                    // Every macro of its rectangle is its kind, and none of
                    // the same kind touches it from outside (a track along
                    // its line).
                    let rect = wfc::Room {
                        at: r.at,
                        size: r.size,
                    };
                    assert!(rect.cells().all(|m| a.meta(m) == r.meta), "{r:?}");
                    if r.meta == Meta::Track {
                        seen_track = true;
                        let across = if r.along_x { r.size.1 } else { r.size.0 };
                        assert_eq!(across, 1, "a line is one macro wide: {r:?}");
                    }
                }
            }
        }
        assert!(seen_track);
    }

    #[test]
    fn atriums_drop_a_whole_level_and_the_level_below_goes_round() {
        let (t, set) = data();
        let lay = LAYOUT.get();
        let (t, levels) = station_with(&t, &set, |ls| {
            ls.iter()
                .any(|l| l.kind == LevelKind::Atrium && l.sunk.iter().any(|s| *s))
        });
        let mut atriums = 0;
        for (li, l) in levels.iter().enumerate() {
            if !l.room_meta.contains(&Meta::Atrium) || !l.sunk.iter().any(|s| *s) {
                continue;
            }
            atriums += 1;
            // (A concourse's two-storey room is one too.)
            assert!(matches!(l.kind, LevelKind::Atrium | LevelKind::Concourse));
            assert!((l.sink - lay.atrium.drop).abs() < 1e-3, "level {li}");
            assert!((l.sink - STATION_LEVEL_H).abs() < 1e-3, "a whole level");
            let below = &levels[li + 1];
            let d = below.distances();
            for i in (0..l.sunk.len()).filter(|&i| l.sunk[i]) {
                let c = (i % l.width, i / l.width);
                // Its floor is the level below's, and down there it's solid
                // ground you go round (with no collider of its own).
                let p = cell_world(&t, li, l, c) - Vec3::Y * l.sink;
                let stairs_in = wfc::Side::ALL.into_iter().any(|s| {
                    wfc::neighbour(l.width, l.depth, c, s)
                        .is_some_and(|n| l.stairs_down(n, s.opposite()))
                });
                if !stairs_in {
                    assert!((floor_at(&t, &levels, p) - level_floor(li + 1)).abs() < 1e-3);
                }
                assert_eq!(level_of(&t, &levels, p), li);
                let Some(b) = map_cell(l, below, c) else {
                    continue;
                };
                assert_eq!(below.fill(b).kind, Kind::Under, "level {}", li + 1);
                assert!(d[b.1 * below.width + b.0].is_none());
                let mid = cell_world(&t, li + 1, below, b);
                assert!(
                    !colliders(&t, li + 1, below).iter().any(|b| {
                        (b.centre - mid).xz().length() < 0.1 && b.size.x >= CELL - 0.01
                    }),
                    "a block under the atrium at level {} {c:?}",
                    li + 1
                );
            }
            assert_eq!(below.path.first(), Some(&below.arrive));
            assert_eq!(below.path.last(), Some(&below.goal));
            // Balconies: every door cell stays up, and the stairs go down from them.
            assert!(l.stairs.iter().any(|s| *s != 0), "level {li}");
            // The hunter can follow you down there.
            let pit = (0..l.sunk.len()).find(|&i| l.sunk[i]).unwrap_or(0);
            let c = (pit % l.width, pit / l.width);
            let p = cell_world(&t, li, l, c) - Vec3::Y * l.sink;
            assert_eq!(
                super::super::fulton_hunt::node_at(&t, &levels, p),
                Some((li, c.0, c.1))
            );
        }
        assert!(atriums >= 1, "no atrium came out");
    }

    #[test]
    fn levels_have_several_ways_down_that_all_land() {
        let (t0, set) = data();
        let mut several = 0;
        let mut levels_seen = 0;
        let (mut extras, mut pockets) = (0, 0);
        for seed in 1..5 {
            let t = Tuning { seed, ..t0.clone() };
            let levels = generate(&t, &set);
            for (li, l) in levels.iter().enumerate() {
                let downs = l.downs();
                levels_seen += 1;
                if downs.len() >= 2 {
                    several += 1;
                }
                assert!(downs.len() <= t.downs.1.max(1), "seed {seed} level {li}");
                let Some(below) = levels.get(li + 1) else {
                    continue;
                };
                let d = l.distances();
                let reach = below.distances();
                for (c, entry) in downs {
                    // You can walk to it, and it lands on a landing below
                    // that opens away from its entry and leads somewhere.
                    assert!(
                        d[c.1 * l.width + c.0].is_some(),
                        "seed {seed} level {li} {c:?}"
                    );
                    let Some(b) = map_cell(l, below, c) else {
                        panic!("seed {seed} level {li}: stairs {c:?} off the level below")
                    };
                    assert_eq!(
                        below.fill(b).kind,
                        Kind::Landing,
                        "seed {seed} level {li} {c:?}"
                    );
                    assert_ne!(below.fill(b).mask & bit(entry.opposite()), 0);
                    // The main stairs always lead on; an extra one may (rarely,
                    // on a cramped fallback level) land in a pocket you walk
                    // back up from.
                    if reach[b.1 * below.width + b.0].is_none() {
                        assert_ne!(c, l.goal, "seed {seed} level {li}: main landing cut off");
                        pockets += 1;
                    }
                    extras += usize::from(c != l.goal);
                }
            }
        }
        // A good share of levels have more than one way down, and hardly
        // any extra one is a dead end.
        assert!(several * 4 >= levels_seen, "{several} of {levels_seen}");
        assert!(
            pockets * 20 <= extras,
            "{pockets} of {extras} extra landings cut off"
        );
    }

    /// The first station (from the data's seed on) that has what a test is
    /// about: the station a seed makes changes with the data, so a test
    /// about something that only some stations have looks for one.
    fn station_with(
        t: &Tuning,
        set: &TileSet,
        ok: impl Fn(&[Level]) -> bool,
    ) -> (Tuning, Vec<Level>) {
        for k in 0..12 {
            let t = Tuning {
                seed: t.seed.wrapping_add(k),
                ..t.clone()
            };
            let levels = generate(&t, set);
            if ok(&levels) {
                return (t, levels);
            }
        }
        panic!("no station has it");
    }

    /// How many loops a level's ways make: each room and each gallery counts
    /// as one place, so only rings through the corridors count.
    fn loops(l: &Level) -> usize {
        let n = l.cells.len();
        let mut up: Vec<usize> = (0..n).collect();
        fn root(up: &mut [usize], mut i: usize) -> usize {
            while up[i] != i {
                up[i] = up[up[i]];
                i = up[i];
            }
            i
        }
        let mut links = Vec::new();
        for i in 0..n {
            let c = (i % l.width, i / l.width);
            for s in [wfc::Side::East, wfc::Side::South] {
                if !l.open(c, s) {
                    continue;
                }
                let Some(o) = wfc::neighbour(l.width, l.depth, c, s) else {
                    continue;
                };
                let j = o.1 * l.width + o.0;
                let (a, b) = (l.cells[i].kind, l.cells[j].kind);
                let same = a == b && matches!(a, Kind::Room(_) | Kind::Gallery);
                if same {
                    let (ra, rb) = (root(&mut up, i), root(&mut up, j));
                    up[ra] = rb;
                } else {
                    links.push((i, j));
                }
            }
        }
        let d = l.distances();
        let places: HashSet<usize> = (0..n)
            .filter(|&i| d[i].is_some())
            .map(|i| root(&mut up, i))
            .collect();
        let joins: HashSet<(usize, usize)> = links
            .into_iter()
            .filter(|&(i, _)| d[i].is_some())
            .map(|(i, j)| {
                let (a, b) = (root(&mut up, i), root(&mut up, j));
                (a.min(b), a.max(b))
            })
            .collect();
        (joins.len() + 1).saturating_sub(places.len())
    }

    #[test]
    fn concourses_have_galleries_and_loops() {
        let (t0, set) = data();
        let (mut seen, mut galleries, mut looped, mut downs, mut free) = (0, 0, 0, 0, 0);
        for seed in 1..9 {
            let t = Tuning { seed, ..t0.clone() };
            for (li, l) in generate(&t, &set).iter().enumerate() {
                if l.kind != LevelKind::Concourse {
                    continue;
                }
                seen += 1;
                galleries += usize::from(!l.galleries.is_empty());
                looped += usize::from(loops(l) >= 1);
                // (Over a platform or tunnel level there's one way down.)
                let busy = |k: usize| platform_level(&t, k) || tunnel_level(&t, k);
                if !busy(li + 1) {
                    free += 1;
                    downs += usize::from(l.downs().len() >= 2);
                }
                assert_eq!(l.path.first(), Some(&l.arrive), "seed {seed} level {li}");
                assert_eq!(l.path.last(), Some(&l.goal), "seed {seed} level {li}");
                // A gallery's cells are its own, open within its tiles.
                for tile in &l.galleries {
                    for dz in 0..TUNNEL_CELLS {
                        for dx in 0..TUNNEL_CELLS {
                            let c = (tile.at.0 + dx, tile.at.1 + dz);
                            assert_eq!(l.fill(c).kind, Kind::Gallery, "seed {seed} level {li}");
                            if dx == 0 {
                                assert!(l.open(c, wfc::Side::East), "seed {seed} level {li}");
                            }
                            if dz == 0 {
                                assert!(l.open(c, wfc::Side::South), "seed {seed} level {li}");
                            }
                        }
                    }
                }
                // Every cell you can walk on is reachable from the way in.
                let d = l.distances();
                for (i, f) in l.cells.iter().enumerate() {
                    if f.mask != 0 && f.kind != Kind::Under {
                        assert!(d[i].is_some(), "seed {seed} level {li} cell {i}");
                    }
                }
            }
        }
        assert!(seen >= 6, "only {seen} concourses");
        assert!(
            galleries * 3 >= seen * 2,
            "{galleries} of {seen} have a gallery"
        );
        assert!(looped * 3 >= seen * 2, "{looped} of {seen} loop");
        assert!(
            downs * 3 >= free * 2,
            "{downs} of {free} have several ways down"
        );
    }

    #[test]
    fn two_storey_rooms_share_a_floor_with_the_level_below() {
        let (t0, set) = data();
        let (mut rooms, mut joined) = (0, 0);
        for seed in 1..9 {
            let t = Tuning { seed, ..t0.clone() };
            let levels = generate(&t, &set);
            for (li, l) in levels.iter().enumerate() {
                if l.kind != LevelKind::Concourse || !l.room_meta.contains(&Meta::Atrium) {
                    continue;
                }
                if !l.sunk.iter().any(|s| *s) {
                    continue;
                }
                rooms += 1;
                assert!(
                    (l.sink - STATION_LEVEL_H).abs() < 1e-3,
                    "seed {seed} level {li}"
                );
                let below = &levels[li + 1];
                joined += usize::from(!l.low_doors.is_empty());
                assert_eq!(
                    l.low_doors.len(),
                    below.from_above.len(),
                    "seed {seed} level {li}"
                );
                for &(c, s) in &below.from_above {
                    let d = below.distances();
                    assert!(
                        d[c.1 * below.width + c.0].is_some(),
                        "seed {seed} level {li}"
                    );
                    let n = wfc::neighbour(below.width, below.depth, c, s).unwrap_or(c);
                    assert_eq!(below.fill(n).kind, Kind::Under);
                    let pit = map_cell(below, l, n).unwrap_or((0, 0));
                    assert!(l.sunk(pit), "seed {seed} level {li}");
                    // Both floors are one height, and the hunter goes through.
                    let (a, b) = (
                        cell_world(&t, li, l, pit) - Vec3::Y * l.sink,
                        cell_world(&t, li + 1, below, c),
                    );
                    assert!((a.y - b.y).abs() < 1e-3, "seed {seed} level {li}");
                    let hunt = super::super::fulton_hunt::neighbours(&levels, (li + 1, c.0, c.1));
                    assert!(hunt.contains(&(li, pit.0, pit.1)), "seed {seed} level {li}");
                    // No collider fills the doorway's lower half.
                    let door = (a + b) / 2.0 + Vec3::Y * 1.0;
                    assert!(
                        !colliders(&t, li, l)
                            .iter()
                            .chain(&colliders(&t, li + 1, below))
                            .any(|k| {
                                let p = k.rotation.inverse() * (door - k.centre);
                                (p.abs() - k.size / 2.0).max_element() < 0.0
                            }),
                        "seed {seed} level {li}: the doorway is blocked"
                    );
                }
            }
        }
        assert!(rooms >= 3, "only {rooms} two-storey rooms");
        assert!(
            joined * 2 >= rooms,
            "{joined} of {rooms} open onto the level below"
        );
    }

    #[test]
    fn every_landing_is_signed_to_a_way_down() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let mut extra = 0;
        for (li, l) in levels.iter().enumerate() {
            let ways = signed_ways(l);
            for way in &ways[1..] {
                extra += 1;
                let (first, last) = (way[0], way[way.len() - 1]);
                assert_eq!(l.fill(first).kind, Kind::Landing, "level {li}");
                assert_eq!(l.fill(last).kind, Kind::Stairs, "level {li}");
            }
            let downs = signs(&t, li, l)
                .iter()
                .filter(|s| s.kind == SignKind::Down)
                .count();
            // A sign at every stairs a way ends at (each once).
            let ends: HashSet<wfc::Cell> = ways
                .iter()
                .filter_map(|w| w.last().copied())
                .filter(|&c| l.fill(c).kind == Kind::Stairs)
                .collect();
            assert_eq!(downs, ends.len(), "level {li}");
        }
        assert!(extra >= 5, "only {extra} more landings signed");
    }

    #[test]
    fn lights_on_your_floor_come_first() {
        let at = Vec3::new(0.0, -40.0 + 1.7, 0.0);
        let near_above = Vec3::new(0.0, -36.0 + 3.6, 1.0);
        let far_here = Vec3::new(40.0, -40.0 + 3.6, 0.0);
        assert!(light_rank(at, far_here) < light_rank(at, near_above));
    }

    #[test]
    fn train_tunnels_run_on_from_the_platforms() {
        let (t0, set) = data();
        let mut tunnels = 0;
        for seed in 1..6 {
            let t = Tuning { seed, ..t0.clone() };
            let levels = generate(&t, &set);
            for (li, l) in levels.iter().enumerate() {
                let rails: Vec<usize> = (0..l.cells.len())
                    .filter(|&i| l.cells[i].kind == Kind::Rails)
                    .collect();
                if rails.is_empty() {
                    continue;
                }
                assert!(platform_level(&t, li), "seed {seed} level {li}");
                let d = l.distances();
                let ends = rails
                    .iter()
                    .filter(|&&i| l.cells[i].mask.count_ones() == 1)
                    .count();
                tunnels += ends;
                for &i in &rails {
                    let c = (i % l.width, i / l.width);
                    // Walkable from the way in, on the track bed.
                    assert!(d[i].is_some(), "seed {seed} level {li} {c:?}");
                    let p = cell_world(&t, li, l, c);
                    assert!(
                        (floor_at(&t, &levels, p) - (level_floor(li) - TRACK_DROP)).abs() < 1e-3
                    );
                    // Never branching: a run, ending in a dead end.
                    assert!(l.cells[i].mask.count_ones() <= 2);
                    // Solid under it on the level below.
                    if let Some(below) = levels.get(li + 1)
                        && let Some(b) = map_cell(l, below, c)
                    {
                        assert_eq!(below.fill(b).kind, Kind::Under, "seed {seed} level {li}");
                    }
                }
            }
        }
        assert!(tunnels >= 4, "only {tunnels} train tunnels");
    }

    #[test]
    fn other_seeds_build_whole_stations_too() {
        let (t0, set) = data();
        for seed in 1..7 {
            let t = Tuning { seed, ..t0.clone() };
            let levels = generate(&t, &set);
            for (li, l) in levels.iter().enumerate() {
                assert_eq!(
                    tunnel_level(&t, li),
                    !l.tunnel.is_empty(),
                    "seed {seed} level {li}"
                );
                assert_eq!(l.path.first(), Some(&l.arrive), "seed {seed} level {li}");
                assert_eq!(l.path.last(), Some(&l.goal), "seed {seed} level {li}");
                if platform_level(&t, li) {
                    assert!(
                        l.cells.iter().any(|f| f.track != 0),
                        "seed {seed} level {li}"
                    );
                }
                if let Some(next) = levels.get(li + 1) {
                    assert_eq!(Some(next.arrive), map_cell(l, next, l.goal));
                    assert!(
                        next.open(next.arrive, l.goal_entry.opposite()),
                        "seed {seed} {li}"
                    );
                }
            }
        }
    }

    /// The longest straight run of track cells on a level.
    fn longest_track(l: &Level) -> usize {
        let mut best = 0;
        for along_x in [true, false] {
            let (a, b) = if along_x {
                (l.depth, l.width)
            } else {
                (l.width, l.depth)
            };
            for i in 0..a {
                let mut run = 0;
                for j in 0..b {
                    let c = if along_x { (j, i) } else { (i, j) };
                    let f = l.fill(c);
                    let along = if along_x {
                        bit(wfc::Side::East) | bit(wfc::Side::West)
                    } else {
                        bit(wfc::Side::North) | bit(wfc::Side::South)
                    };
                    if f.track != 0 && f.track & !along == 0 {
                        run += 1;
                        best = best.max(run);
                    } else {
                        run = 0;
                    }
                }
            }
        }
        best
    }

    #[test]
    fn levels_come_in_types_with_long_lines_and_big_halls() {
        let (t, set) = data();
        let (t, levels) = station_with(&t, &set, |ls| {
            let came = |k: LevelKind| {
                ls.iter().any(|l| {
                    l.kind == k
                        && match k {
                            LevelKind::Hall => l.room_meta.contains(&Meta::Hall),
                            LevelKind::Atrium => l.room_meta.contains(&Meta::Atrium),
                            LevelKind::Concourse => !l.galleries.is_empty(),
                            _ => true,
                        }
                })
            };
            [LevelKind::Warren, LevelKind::Line, LevelKind::Hall]
                .into_iter()
                .chain([LevelKind::Atrium, LevelKind::Concourse])
                .all(came)
        });
        let mut kinds = HashSet::new();
        let (mut track, mut room) = (0, 0);
        let (mut pieces, mut tall, mut columns) = (0, 0, 0);
        for (li, l) in levels.iter().enumerate() {
            // A type counts when its set piece came out.
            let piece = match l.kind {
                LevelKind::Warren => true,
                LevelKind::Concourse => !l.galleries.is_empty(),
                LevelKind::Line => l.coarse.is_some() && longest_track(l) > 0,
                LevelKind::Hall => l.room_meta.contains(&Meta::Hall),
                LevelKind::Atrium => l.room_meta.contains(&Meta::Atrium),
            };
            if piece {
                kinds.insert(l.kind);
            }
            track = track.max(longest_track(l));
            for ri in (0..l.rooms.len()).filter(|&ri| l.set_piece(ri)) {
                let n = l
                    .cells
                    .iter()
                    .filter(|f| f.kind == Kind::Room(ri as u8))
                    .count();
                room = room.max(n);
                // Set pieces are tall (never over a platform level, and not
                // where sinking it would cut the level below up), with
                // columns down in them.
                if platform_level(&t, li + 1) {
                    let sunk = (0..l.cells.len())
                        .any(|i| l.sunk[i] && l.cells[i].kind == Kind::Room(ri as u8));
                    assert!(!sunk, "level {li}");
                    continue;
                }
                pieces += 1;
                if l.sunk.iter().any(|s| *s) {
                    tall += 1;
                    columns += pit_columns(&t, li, l).len();
                }
            }
            // The kept-up top floors stay calm.
            if li < 2 {
                assert!(
                    matches!(l.kind, LevelKind::Warren | LevelKind::Hall),
                    "level {li}"
                );
            }
            // Platform levels are lines (bar those over a tunnel ring).
            if platform_level(&t, li) && !(li..li + 3).any(|n| tunnel_level(&t, n)) {
                assert!(
                    l.kind == LevelKind::Line || l.coarse.is_none(),
                    "level {li}"
                );
            }
        }
        assert_eq!(kinds.len(), 5, "{kinds:?}");
        assert!(track >= 8, "the longest line is {track} cells");
        assert!(room >= 12, "the biggest hall is {room} cells");
        assert!(
            tall * 3 >= pieces * 2,
            "{tall} of {pieces} set pieces are tall"
        );
        assert!(columns > 0);
    }

    #[test]
    fn levels_of_different_sizes_line_up_cell_for_cell() {
        let (t, set) = data();
        let (small, big) = (Level::empty(6, 6), Level::empty(12, 6));
        // The same place in the world on both.
        for c in [(0, 0), (5, 5), (2, 3)] {
            let b = map_cell(&small, &big, c).expect("the big one covers the small one");
            assert_eq!(b, (c.0 + 3, c.1));
            assert_eq!(cell_world(&t, 3, &small, c), cell_world(&t, 3, &big, b));
            assert_eq!(map_cell(&big, &small, b), Some(c));
        }
        // Off the smaller level: nothing.
        assert_eq!(map_cell(&big, &small, (0, 0)), None);
        assert_eq!(map_cell(&big, &small, (11, 2)), None);
        // The core (6 x 6) is the old grid, where it always was.
        let core = Level::core(&t);
        assert_eq!(
            cell_world(&t, 0, &core, (0, 0)).xz(),
            Vec2::new(t.station_at.0 - 2.5 * CELL, t.station_at.1 - 2.5 * CELL)
        );
        // Down the station the stairs land where they are, and what sinks
        // into the level below is solid there, however big each level is.
        let levels = generate(&t, &set);
        let mut sizes = HashSet::new();
        for (li, pair) in levels.windows(2).enumerate() {
            let (l, next) = (&pair[0], &pair[1]);
            sizes.insert((l.width, l.depth));
            assert_eq!(
                cell_world(&t, li, l, l.goal).xz(),
                cell_world(&t, li + 1, next, next.arrive).xz()
            );
            for i in 0..l.cells.len() {
                let c = (i % l.width, i / l.width);
                if (l.fill(c).track != 0 || l.sunk(c))
                    && let Some(b) = map_cell(l, next, c)
                {
                    assert_eq!(next.fill(b).kind, Kind::Under, "level {}", li + 1);
                }
            }
        }
        assert!(sizes.len() >= 3, "{sizes:?}");
    }

    /// A level as text: its type and macro grid (W warren, H hall, A atrium,
    /// # solid, = track), then a row per fine row (a arrive, g goal, = track,
    /// R room, v sunk, _ under, # solid, T tunnel, + corridor, space shut).
    fn ascii(li: usize, l: &Level) -> String {
        let mut out = format!("level {li} {:?} {}x{}\n", l.kind, l.width, l.depth);
        if let Some(co) = &l.coarse {
            for z in 0..co.depth {
                out.push_str("  ");
                for x in 0..co.width {
                    out.push(match co.meta((x, z)) {
                        Meta::Warren => 'W',
                        Meta::Hall => 'H',
                        Meta::Atrium => 'A',
                        Meta::Solid => '#',
                        Meta::Track => '=',
                    });
                }
                out.push('\n');
            }
        }
        for z in 0..l.depth {
            out.push_str("  ");
            for x in 0..l.width {
                let c = (x, z);
                let f = l.fill(c);
                out.push(if c == l.arrive {
                    'a'
                } else if c == l.goal {
                    'g'
                } else if f.track != 0 {
                    '='
                } else if l.sunk(c) {
                    'v'
                } else {
                    match f.kind {
                        Kind::Room(_) => 'R',
                        Kind::Under => '_',
                        Kind::Tunnel => 'T',
                        Kind::Gallery => 'G',
                        Kind::Corridor(None) if f.mask == 0 => '#',
                        _ if f.mask == 0 => ' ',
                        _ => '+',
                    }
                });
            }
            out.push('\n');
        }
        // Then how the cells join up (- and | where you can walk across).
        for z in 0..l.depth {
            let (mut top, mut bottom) = (String::from("  "), String::from("  "));
            for x in 0..l.width {
                let c = (x, z);
                let ch = match l.fill(c).kind {
                    _ if c == l.arrive => 'a',
                    _ if c == l.goal => 'g',
                    Kind::Stairs => 'd',
                    Kind::Landing => 'u',
                    Kind::Room(_) => 'R',
                    Kind::Gallery => 'G',
                    Kind::Rails => 'r',
                    Kind::Tunnel => 'T',
                    _ if l.fill(c).mask == 0 => '#',
                    _ => '+',
                };
                top.push(ch);
                top.push(if l.open(c, wfc::Side::East) { '-' } else { ' ' });
                bottom.push(if l.open(c, wfc::Side::South) {
                    '|'
                } else {
                    ' '
                });
                bottom.push(' ');
            }
            out.push_str(&top);
            out.push('\n');
            out.push_str(&bottom);
            out.push('\n');
        }
        out
    }

    /// `cargo test -p friendslop-side-nyc print_the_station -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn print_the_station() {
        let (t, set) = data();
        // (FULTON_SEED=n prints another station.)
        let seed = std::env::var("FULTON_SEED")
            .ok()
            .and_then(|s| s.parse().ok());
        let t = Tuning {
            seed: seed.unwrap_or(t.seed),
            ..t
        };
        let mut plan = StationPlan::new(&t, &set);
        let mut slowest = (0, 0.0);
        while plan.levels.len() < t.levels {
            let start = std::time::Instant::now();
            let _ = plan.plan_next(&t);
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            if let Some(l) = plan.levels.last() {
                println!(
                    "planned {} {:?} {}x{} in {ms:.1} ms",
                    plan.levels.len() - 1,
                    l.kind,
                    l.width,
                    l.depth
                );
            }
            if ms > slowest.1 {
                slowest = (plan.levels.len() - 1, ms);
            }
        }
        for (li, l) in plan.levels.iter().enumerate() {
            println!("{}", ascii(li, l));
        }
        println!("slowest level to plan: {} ({:.1} ms)", slowest.0, slowest.1);
    }

    #[test]
    fn tunnel_levels_are_a_ring_with_two_arms() {
        let (t, set) = data();
        assert!(
            t.tunnels.every > 0 && !set.tunnel.straight.is_empty() && !set.tunnel.curve.is_empty()
        );
        let levels = generate(&t, &set);
        let mut seen = 0;
        for (li, l) in levels.iter().enumerate() {
            assert_eq!(tunnel_level(&t, li), !l.tunnel.is_empty(), "level {li}");
            if l.tunnel.is_empty() {
                continue;
            }
            seen += 1;
            let d = l.distances();
            assert!(d[l.goal.1 * l.width + l.goal.0].is_some(), "level {li}");
            assert!(l.fill(l.arrive).kind == Kind::Landing);
            assert!(l.fill(l.goal).kind == Kind::Stairs);
            // Without the caved-in tiles the other arm is always there: every
            // reachable cell stays reachable (shape_level guarantees it), and
            // the way is at least three tiles long.
            assert!(
                l.path.len() >= 3 * TUNNEL_CELLS,
                "level {li} path {}",
                l.path.len()
            );
            // Walls from the plan: tunnel cells are open inside their tile.
            for tile in &l.tunnel {
                let (x, z) = tile.at;
                for c in [(x, z), (x + 1, z), (x, z + 1), (x + 1, z + 1)] {
                    assert!(matches!(
                        l.fill(c).kind,
                        Kind::Tunnel | Kind::Landing | Kind::Stairs
                    ));
                }
            }
            assert!(!colliders(&t, li, l).is_empty());
        }
        assert!(seen > 0, "no tunnel levels");
    }

    #[test]
    fn the_ring_has_curves_at_the_corners() {
        let ring = tunnel_ring(6, 6).expect("a 6x6 grid has a ring");
        assert_eq!(ring.len(), 8);
        assert_eq!(
            ring.iter()
                .filter(|t| t.piece == TunnelPiece::Curve)
                .count(),
            4
        );
        // Every tile's open sides meet its neighbour's round the ring.
        for (i, a) in ring.iter().enumerate() {
            let b = &ring[(i + 1) % ring.len()];
            let step = (b.at.0 as i32 - a.at.0 as i32, b.at.1 as i32 - a.at.1 as i32);
            let side = wfc::Side::ALL
                .into_iter()
                .find(|s| {
                    let (ox, oz) = s.offset();
                    (ox * TUNNEL_CELLS as i32, oz * TUNNEL_CELLS as i32) == step
                })
                .expect("neighbours");
            assert!(a.mask() & bit(side) != 0, "tile {i} opens onto the next");
            assert!(
                b.mask() & bit(side.opposite()) != 0,
                "tile {} opens back",
                i + 1
            );
        }
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
            // ...and it's long (a tunnel ring's arm is at least three tiles; the
            // level above a ring has only the ring's hall to put its stairs in,
            // so its way can be shorter).
            assert!(
                l.path.len() as u32 > t.min_path
                    || !l.tunnel.is_empty()
                    || (tunnel_level(&t, li + 1) && l.path.len() > 2),
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
                assert_eq!(Some(next.arrive), map_cell(l, next, l.goal));
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
                if let Some(below) = levels.get(li + 1)
                    && let Some(b) = map_cell(l, below, c)
                {
                    assert_eq!(below.fill(b).kind, Kind::Under, "level {li} {c:?}");
                    assert_eq!(below.fill(b).mask, 0);
                }
            }
            // Corridor sides never open onto track sides (a tunnel level has
            // no corridors: its notches for the stairs break the pairing).
            for z in 0..if l.tunnel.is_empty() { l.depth } else { 0 } {
                for x in 0..l.width {
                    for s in wfc::Side::ALL {
                        // (A doorway into the floor of a two-storey room above
                        // opens onto ground solid on this level's plan.)
                        if l.from_above.contains(&((x, z), s)) {
                            continue;
                        }
                        if let Some(n) = wfc::neighbour(l.width, l.depth, (x, z), s) {
                            if l.from_above.contains(&(n, s.opposite())) {
                                continue;
                            }
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
        let (t0, set) = data();
        let mut seen = std::collections::BTreeMap::<String, usize>::new();
        // (Over a few stations: a kind can sit out one by chance.)
        for seed in t0.seed..t0.seed + 3 {
            let t = Tuning { seed, ..t0.clone() };
            let levels = generate(&t, &set);
            for (li, l) in levels.iter().enumerate() {
                let mut line = format!("level {li:2}:");
                for (ri, r) in l.rooms.iter().enumerate() {
                    let name = room_kind(&t, li, l, ri).map_or("hall", |k| k.name.as_str());
                    *seen.entry(name.to_string()).or_default() += 1;
                    let mid = cell_world(&t, li, l, r.at).lerp(
                        cell_world(&t, li, l, (r.at.0 + r.size.0 - 1, r.at.1 + r.size.1 - 1)),
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
                    let o = cell_world(&t, li, l, (0, 0));
                    let c = (
                        ((b.centre.x - o.x) / CELL).round() as usize,
                        ((b.centre.z - o.z) / CELL).round() as usize,
                    );
                    let mid = cell_world(&t, li, l, c);
                    let off = (b.centre - mid).with_y(0.0);
                    let reach = off.length() - b.size.x.max(b.size.z) / 2.0;
                    let inward = off.x.abs().max(off.z.abs()) - b.size.z / 2.0;
                    assert!(inward >= 1.5 || reach >= 1.5, "level {li} {p:?}");
                    assert!(matches!(l.fill(c).kind, Kind::Room(_)), "level {li} {p:?}");
                }
            }
        }
        println!("{seen:?}");
        for k in &t0.rooms.kinds {
            assert!(seen.contains_key(&k.name), "no {} in {seen:?}", k.name);
        }
    }

    #[test]
    fn parked_cars_stand_on_the_track_clear_of_the_platform() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let mut n = 0;
        for (li, l) in levels.iter().enumerate() {
            let cars = parked_cars(&t, li, l);
            assert_eq!(cars, parked_cars(&t, li, l));
            for c in &cars {
                n += 1;
                // Back in the middle cell's own frame: on the rails, and every
                // collider beyond the platform's edge (the gap included).
                let o = cell_world(&t, li, l, (0, 0));
                let p = c.transform.translation;
                let cell = (
                    ((p.x - o.x) / CELL).round() as usize,
                    ((p.z - o.z) / CELL).round() as usize,
                );
                let f = l.fill(cell);
                let inv = f.rotation().inverse();
                let local = inv * (p - cell_world(&t, li, l, cell));
                assert!(
                    (local.y - RAIL_TOP).abs() < 1e-3 && (local.z - CAR_Z).abs() < 1e-3,
                    "{local}"
                );
                for b in &c.blocks {
                    let lb = inv * (b.centre - cell_world(&t, li, l, cell));
                    let half = (inv * b.rotation * b.size).abs() / 2.0;
                    assert!(lb.z - half.z >= PLATFORM_EDGE_Z + 0.05, "{lb} {half}");
                    assert!(
                        lb.z + half.z <= CELL / 2.0 - 0.2,
                        "into the far wall: {lb} {half}"
                    );
                }
            }
        }
        assert!(n > 0, "no cars anywhere");
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
                let o = cell_world(&t, li, l, (0, 0));
                let c = (
                    ((b.centre.x - o.x) / CELL).round() as usize,
                    ((b.centre.z - o.z) / CELL).round() as usize,
                );
                let off = (b.centre - cell_world(&t, li, l, c)).with_y(0.0);
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
    fn planning_on_a_carried_on_copy_makes_the_same_station() {
        let t = tuning();
        let set = TILES.get();
        let mut plan = StationPlan::new(&t, &set);
        plan.plan_to(&t, 2);
        for _ in 0..6 {
            let mut next = plan.carry_on();
            assert!(next.levels.is_empty());
            next.plan_next(&t);
            plan.take(next);
        }
        let mut whole = StationPlan::new(&t, &set);
        whole.plan_to(&t, 8);
        assert_eq!(plan.levels.len(), 8);
        for (a, b) in plan.levels.iter().zip(&whole.levels) {
            assert_eq!(format!("{:?}", a.cells), format!("{:?}", b.cells));
        }
    }

    #[test]
    fn levels_are_planned_and_built_ahead_of_the_deepest() {
        let (t, _) = data();
        assert!(
            t.plan_ahead >= 2,
            "the level under you is built before you drop"
        );
        assert_eq!(levels_wanted(&t, None, 0), t.plan_ahead + 1);
        assert_eq!(levels_wanted(&t, Some(5), 0), 7 + t.plan_ahead);
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
    fn the_top_floors_are_a_working_station_and_it_goes_bad_by_the_fourth() {
        let (t, _) = data();
        let lit = |l: usize| {
            let n = (0..t.width)
                .flat_map(|x| (0..t.depth).map(move |z| (x, z)))
                .filter(|&c| cell_lit(&t, l, c))
                .count();
            n as f32 / (t.width * t.depth) as f32
        };
        for l in 0..2 {
            let s = stage(&t, l);
            assert!(
                !s.creepy && s.rubble == 0.0 && s.look == StageLook::Kept,
                "{l}"
            );
            assert!(lit(l) > 0.75, "floor {} is lit: {}", l + 1, lit(l));
        }
        let worn = stage(&t, 2);
        assert!(worn.creepy && worn.look == StageLook::Worn);
        assert!(lit(2) < lit(0));
        // The hunter comes on the fourth floor, and from there it's the bad station.
        assert_eq!(t.hunt.call_levels.last(), Some(&3));
        for l in 3..t.levels {
            let s = stage(&t, l);
            assert!(s.creepy && s.look == StageLook::Bad && s.rubble == 1.0);
        }
        assert!(t.light.dark_from > 3);
    }

    #[test]
    fn most_lights_are_broken_and_a_few_deep_levels_are_pitch_dark() {
        let (t, _) = data();
        let dark: Vec<usize> = (0..t.levels).filter(|&l| dark_level(&t, l)).collect();
        assert!(
            dark.iter().all(|&l| l >= t.light.dark_from),
            "the top stays lit: {dark:?}"
        );
        assert!(
            !dark.is_empty() && dark.len() < t.levels / 2,
            "a few deep levels go dark, not most: {dark:?}"
        );
        // Below the kept-up top floors (`stages`).
        let lit_level = (t.light.stages.len()..t.levels)
            .find(|&l| !dark_level(&t, l))
            .unwrap_or(0);
        let cells: Vec<bool> = (0..t.width)
            .flat_map(|x| (0..t.depth).map(move |z| (x, z)))
            .map(|c| cell_lit(&t, lit_level, c))
            .collect();
        let share = cells.iter().filter(|&&b| b).count() as f32 / cells.len() as f32;
        assert!(
            share > 0.0 && share < 0.5,
            "some lights work, most don't: {share}"
        );
        for &l in &dark {
            assert!(
                (0..t.width).all(|x| !cell_lit(&t, l, (x, 0))),
                "nothing works on a dark level"
            );
        }
    }

    #[test]
    fn a_broken_light_wears_a_dead_skin() {
        let (t, _) = data();
        // cell_of undoes cell_world
        for (w, h) in [(t.width, t.depth), (12, 6), (8, 8)] {
            let l = Level::empty(w, h);
            for c in [(0, 0), (w - 1, h - 1), (2, 3)] {
                assert_eq!(cell_of(&t, &l, cell_world(&t, 5, &l, c)), Some(c));
            }
        }
        let glowing = StandardMaterial {
            base_color: Color::WHITE,
            emissive: LinearRgba::rgb(4.0, 4.0, 4.0),
            ..default()
        };
        let dead = broken_skin(&glowing, 0.35).expect("a glowing tube gets the dead skin");
        assert_eq!(dead.emissive, LinearRgba::BLACK);
        assert!(dead.base_color.to_linear().red < 0.5);
        assert!(
            broken_skin(&StandardMaterial::default(), 0.35).is_none(),
            "walls are left alone"
        );
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
        let (_, levels) = station_with(&t, &set, |ls| {
            ls.iter()
                .flat_map(|l| {
                    l.rooms.iter().enumerate().filter(move |(ri, r)| {
                        let n = l
                            .cells
                            .iter()
                            .filter(|f| f.kind == Kind::Room(*ri as u8))
                            .count();
                        n > 0 && n < r.size.0 * r.size.1
                    })
                })
                .count()
                >= 3
        });
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
                        assert!(matches!(l.fill(c).kind, Kind::Corridor(_) | Kind::Tunnel));
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
        let (t, levels) = station_with(&t, &set, |ls| {
            ls.iter().filter(|l| l.sunk.iter().any(|s| *s)).count() >= 3
        });
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
                    let Some(b) = map_cell(l, below, (i % l.width, i / l.width)) else {
                        continue;
                    };
                    assert_eq!(below.fill(b).kind, Kind::Under, "level {}", li + 1);
                }
            }
            for sh in shapes(&t, li, l) {
                if sh.piece != ShapePiece::Stairs {
                    continue;
                }
                // (A long flight's later pieces share its first's collider.)
                let Some(b) = sh.block else {
                    continue;
                };
                stairs += 1;
                // Eight steps a flight, no taller than the controller climbs,
                // a slope it walks.
                assert!(l.sink / (8 * stair_flights(l.sink)) as f32 <= 0.3);
                assert!((l.sink / l.run).atan().to_degrees() <= 40.0);
                assert!(l.run < CELL);
                // The ramp's top meets the deck and its foot the sunken floor.
                let top_at = |z: f32| {
                    let p = b.centre + b.rotation * Vec3::new(0.0, 0.15, z);
                    p.y - level_floor(li)
                };
                let half = b.size.z / 2.0;
                assert!(top_at(-half).abs() < 0.1, "{}", top_at(-half));
                assert!((top_at(half) + l.sink).abs() < 0.1, "{}", top_at(half));
                // And the floor heights agree.
                let foot =
                    sh.transform.translation + sh.transform.rotation * Vec3::Z * (l.run - 0.3);
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
                    let p = cell_world(&t, li, l, (x, z));
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
    fn a_lockers_bank_is_solid_where_it_hides_you() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let mut seen = 0;
        for (li, l) in levels.iter().enumerate() {
            for z in 0..l.depth {
                for x in 0..l.width {
                    let f = l.fill((x, z));
                    let Kind::Corridor(Some(i)) = f.kind else {
                        continue;
                    };
                    if !set.pieces.get(i as usize).is_some_and(|p| p.hide) {
                        continue;
                    }
                    seen += 1;
                    let centre = cell_world(&t, li, l, (x, z)) + f.rotation() * LOCKERS_BLOCK.0;
                    assert!(
                        colliders(&t, li, l).iter().any(|b| {
                            b.centre.distance(centre) < 0.01
                                && b.size.distance(LOCKERS_BLOCK.1) < 0.01
                        }),
                        "level {li} cell {x},{z} has no lockers bank collider"
                    );
                }
            }
        }
        assert!(seen > 0, "no lockers anywhere in the station");
    }

    #[test]
    fn the_stairs_ramp_meets_both_floors() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let l = &levels[0];
        // The top is at the main stairs' entry edge (a level has several).
        let entry = cell_world(&t, 0, l, l.goal) + dir(l.goal_entry) * (CELL / 2.0);
        let top_of = |b: &Block| b.centre + b.rotation * Vec3::new(0.0, 0.15, -b.size.z / 2.0);
        let ramp = colliders(&t, 0, l).into_iter().find(|b| {
            b.rotation != Quat::IDENTITY
                && b.size.z > CELL
                && top_of(b).xz().distance(entry.xz()) < 0.01
        });
        let Some(ramp) = ramp else { panic!("no ramp") };
        let top = top_of(&ramp);
        let bottom = ramp.centre + ramp.rotation * Vec3::new(0.0, 0.15, ramp.size.z / 2.0);
        assert!((top.y - level_floor(0)).abs() < 0.01, "{top}");
        assert!((bottom.y - level_floor(1)).abs() < 0.01, "{bottom}");
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
    fn depth_themes_skin_the_corridors_and_their_models_exist() {
        let (_, set) = data();
        assert!(set.themes.len() >= 3, "several depth looks");
        assert!(set.theme(0).is_none(), "the top is the plain station");
        let froms: Vec<usize> = set.themes.iter().map(|t| t.from).collect();
        assert!(froms.windows(2).all(|w| w[0] < w[1]), "deeper each time");
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../game/assets/models");
        for th in &set.themes {
            assert!(!th.swap.is_empty(), "{} swaps something", th.name);
            for (from, to) in &th.swap {
                assert!(set.pieces.iter().any(|p| &p.model == from), "{from}");
                assert_ne!(from, to);
                assert!(
                    root.join("features").join(to).exists(),
                    "{} model {to} is missing",
                    th.name
                );
            }
        }
        let deepest = set.themes.last().map_or(0, |t| t.from);
        let m = &set.pieces[0].model;
        assert_ne!(set.themed(deepest, m), m.as_str());
        assert_eq!(set.themed(0, m), m.as_str());
    }

    #[test]
    fn tunnel_stairs_are_in_the_hall_off_the_tracks_behind_doorways() {
        let (t, set) = data();
        let levels = generate(&t, &set);
        let mut seen = 0;
        for (li, l) in levels
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.tunnel.is_empty())
        {
            seen += 1;
            let (w, h) = (l.width, l.depth);
            // Neither the landing nor the stairs is on a track tile.
            for c in [l.arrive, l.goal] {
                assert!(inner_cell(w, h, c), "level {li}: {c:?}");
                assert_ne!(l.fill(c).kind, Kind::Tunnel);
            }
            // Two straight tiles have a doorway each, and the doorway model fits.
            let doors: Vec<_> = l.tunnel.iter().filter_map(|&t| tunnel_door(l, t)).collect();
            assert_eq!(doors.len(), 2, "level {li}");
        }
        assert!(seen >= 2);
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
