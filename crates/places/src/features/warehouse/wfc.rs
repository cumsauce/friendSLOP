//! The warehouse floor plan, made by wave function collapse (#130).
//!
//! The floor is a grid of cells. Every cell's tile says which of its four
//! sides are open (an aisle carries on) and which are walled. WFC starts with
//! every cell able to be any tile, then over and over collapses the most
//! constrained cell to one weighted-random tile and propagates: a neighbour
//! can only keep tiles whose shared side agrees. The outer edge is walled
//! except for the loading door. A plan where too little is reachable from the
//! door is thrown away and rolled again. Pure and seeded, so every peer builds
//! the same warehouse from the replicated seed.

use bevy::prelude::*;
use friendslop_wfc as wfc;

/// Cells across (x) and deep (z).
pub const W: usize = 7;
pub const H: usize = 7;
/// Metres per cell.
pub const CELL: f32 = 5.0;
/// The cell with the loading door (on the north, -z, outer wall).
pub const ENTRANCE: (usize, usize) = (W / 2, 0);

/// Side bits: north (-z), east (+x), south (+z), west (-x).
pub const N: u8 = 1;
pub const E: u8 = 2;
pub const S: u8 = 4;
pub const WEST: u8 = 8;
const SIDES: [(u8, i32, i32, u8); 4] = [
    (N, 0, -1, S),
    (E, 1, 0, WEST),
    (S, 0, 1, N),
    (WEST, -1, 0, E),
];

/// A side of a tile piece, as the tile set names it.
#[derive(serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    N,
    E,
    S,
    W,
}

impl Side {
    fn bit(self) -> u8 {
        match self {
            Side::N => N,
            Side::E => E,
            Side::S => S,
            Side::W => WEST,
        }
    }
}

/// One piece of the tile kit: a modeler model the size of a cell, the sides
/// it's open on as modelled, and how much each of its turns is wanted.
#[derive(serde::Deserialize, Clone, Debug)]
pub struct Piece {
    /// Under `assets/models/features/`.
    pub model: String,
    pub open: Vec<Side>,
    pub weight: f32,
}

/// The warehouse's tile kit (`assets/data/places/warehouse_tiles.ron`).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct TileSet {
    pub pieces: Vec<Piece>,
}

pub static TILES: friendslop_data::Data<TileSet> =
    friendslop_data::Data::ron("data/places/warehouse_tiles.ron");

/// A piece's open sides after turning it `turns` quarter turns clockwise
/// (seen from above: north goes east).
pub fn turned(mask: u8, turns: u8) -> u8 {
    (0..turns % 4).fold(mask & 15, |m, _| ((m << 1) | (m >> 3)) & 15)
}

/// Which piece fills a cell, and how far it's turned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    /// Index into the tile set's pieces.
    pub piece: u8,
    /// Quarter turns clockwise (seen from above).
    pub turns: u8,
}

impl Placed {
    /// The rotation that turns the model as placed.
    pub fn rotation(self) -> Quat {
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2 * self.turns as f32)
    }
}

/// Every distinct turn of every piece: (open mask, which piece, weight).
fn variants(set: &TileSet) -> Vec<(u8, Placed, f32)> {
    let mut out: Vec<(u8, Placed, f32)> = Vec::new();
    for (i, p) in set.pieces.iter().enumerate() {
        let mask = p.open.iter().fold(0, |m, s| m | s.bit());
        let mut seen = Vec::new();
        for turns in 0..4u8 {
            let m = turned(mask, turns);
            if seen.contains(&m) {
                continue;
            }
            seen.push(m);
            let placed = Placed {
                piece: i as u8,
                turns,
            };
            out.push((m, placed, p.weight.max(0.0)));
        }
    }
    out
}

/// A finished plan: the open sides of every cell, `cells[z][x]`, and the
/// piece that fills it (`None` in the fallback hall).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub cells: [[u8; W]; H],
    pub pieces: [[Option<Placed>; W]; H],
}

/// The seeded generator, shared with the rest of the warehouse.
pub use friendslop_wfc::Rng;

fn in_grid(x: i32, z: i32) -> bool {
    (0..W as i32).contains(&x) && (0..H as i32).contains(&z)
}

fn bit_of(side: wfc::Side) -> u8 {
    SIDES[side.index()].0
}

/// The kit's pieces in every turn; a side's socket is "open?".
fn problem(vars: &[(u8, Placed, f32)]) -> wfc::Wfc<bool> {
    let tiles = vars
        .iter()
        .map(|&(m, _, weight)| wfc::Tile {
            sockets: wfc::Side::ALL.map(|s| m & bit_of(s) != 0),
            weight,
        })
        .collect();
    wfc::Wfc::new(W, H, tiles)
}

/// Open sides meet open sides. The outer wall is shut except for the
/// loading door, which is always open.
fn boundary(cell: wfc::Cell, side: wfc::Side, open: &bool) -> bool {
    *open == (cell == ENTRANCE && side == wfc::Side::North)
}

fn to_plan(s: &wfc::Solution, vars: &[(u8, Placed, f32)]) -> Plan {
    let mut cells = [[0u8; W]; H];
    let mut pieces = [[None; W]; H];
    for z in 0..H {
        for x in 0..W {
            if let Some(&(m, placed, _)) = vars.get(s.get((x, z))) {
                cells[z][x] = m;
                pieces[z][x] = Some(placed);
            }
        }
    }
    Plan { cells, pieces }
}

impl Plan {
    /// Generate the plan for `seed`: WFC until it's connected enough.
    pub fn generate(seed: u32) -> Plan {
        let mut rng = Rng::new(seed);
        let vars = variants(&TILES.get());
        if vars.is_empty() {
            return Plan::fallback();
        }
        problem(&vars)
            .solve_until(
                &mut rng,
                200,
                |a, b| a == b,
                boundary,
                |s| to_plan(s, &vars).reachable().len() * 10 >= W * H * 6,
            )
            .map(|s| to_plan(&s, &vars))
            .unwrap_or_else(Plan::fallback)
    }

    /// One big open hall (never needed in practice; keeps generation total).
    pub fn fallback() -> Plan {
        let mut cells = [[0u8; W]; H];
        for (z, row) in cells.iter_mut().enumerate() {
            for (x, c) in row.iter_mut().enumerate() {
                let mut m = 0;
                for &(bit, dx, dz, _) in &SIDES {
                    if in_grid(x as i32 + dx, z as i32 + dz) || ((x, z) == ENTRANCE && bit == N) {
                        m |= bit;
                    }
                }
                *c = m;
            }
        }
        Plan {
            cells,
            pieces: [[None; W]; H],
        }
    }

    pub fn open(&self, x: usize, z: usize, bit: u8) -> bool {
        self.cells[z][x] & bit != 0
    }

    fn side_open(&self, c: wfc::Cell, side: wfc::Side) -> bool {
        self.open(c.0, c.1, bit_of(side))
    }

    /// Neighbour cells you can walk to from (x, z).
    pub fn exits(&self, x: usize, z: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
        wfc::exits(W, H, (x, z), |c, s| self.side_open(c, s))
    }

    /// Walking distance in cells from `from` to every cell (`u8::MAX` = unreachable).
    pub fn distances(&self, from: (usize, usize)) -> [[u8; W]; H] {
        let flat = wfc::distances(W, H, from, |c, s| self.side_open(c, s));
        let mut d = [[u8::MAX; W]; H];
        for (i, v) in flat.into_iter().enumerate() {
            if let Some(v) = v {
                d[i / W][i % W] = v.min(u8::MAX as u32 - 1) as u8;
            }
        }
        d
    }

    /// Cells reachable from the loading door.
    pub fn reachable(&self) -> Vec<(usize, usize)> {
        wfc::reachable(W, H, ENTRANCE, |c, s| self.side_open(c, s))
    }

    /// The first step from `from` towards `to`, or `None` if `to` can't be reached.
    pub fn step_towards(&self, from: (usize, usize), to: (usize, usize)) -> Option<(usize, usize)> {
        wfc::step_towards(W, H, from, to, |c, s| self.side_open(c, s))
    }

    /// Can you see from `a` to `b` (floor-plane offsets in metres from the
    /// grid's centre) without a wall in the way?
    pub fn sees(&self, a: Vec2, b: Vec2) -> bool {
        let len = a.distance(b);
        let steps = (len / 0.4).ceil().max(1.0) as usize;
        let mut prev = cell_of(a);
        for i in 1..=steps {
            let p = a.lerp(b, i as f32 / steps as f32);
            let c = cell_of(p);
            if c == prev {
                continue;
            }
            let (Some((px, pz)), Some(c2)) = (prev, c) else {
                return false;
            };
            let link = |a: (usize, usize), b: (usize, usize)| self.exits(a.0, a.1).any(|n| n == b);
            let ok = if c2.0 != px && c2.1 != pz {
                // Slipping diagonally through a corner: either way round will do.
                let (via1, via2) = ((c2.0, pz), (px, c2.1));
                (link((px, pz), via1) && link(via1, c2)) || (link((px, pz), via2) && link(via2, c2))
            } else {
                link((px, pz), c2)
            };
            if !ok {
                return false;
            }
            prev = c;
        }
        true
    }
}

/// Floor-plane offset (from the grid centre) of a cell's middle.
pub fn cell_centre(x: usize, z: usize) -> Vec2 {
    Vec2::new(
        (x as f32 - (W as f32 - 1.0) / 2.0) * CELL,
        (z as f32 - (H as f32 - 1.0) / 2.0) * CELL,
    )
}

/// The cell under a floor-plane offset, if it's on the grid.
pub fn cell_of(p: Vec2) -> Option<(usize, usize)> {
    let x = (p.x / CELL + W as f32 / 2.0).floor();
    let z = (p.y / CELL + H as f32 / 2.0).floor();
    (x >= 0.0 && z >= 0.0 && (x as usize) < W && (z as usize) < H)
        .then_some((x as usize, z as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn consistent(plan: &Plan) {
        for z in 0..H {
            for x in 0..W {
                for &(bit, dx, dz, back) in &SIDES {
                    let (nx, nz) = (x as i32 + dx, z as i32 + dz);
                    let open = plan.open(x, z, bit);
                    if in_grid(nx, nz) {
                        assert_eq!(open, plan.open(nx as usize, nz as usize, back), "{x},{z}");
                    } else {
                        assert_eq!(open, (x, z) == ENTRANCE && bit == N, "{x},{z} edge");
                    }
                }
            }
        }
    }

    #[test]
    fn plans_are_consistent_connected_and_seeded() {
        for seed in 0..60 {
            let plan = Plan::generate(seed);
            consistent(&plan);
            assert!(plan.reachable().len() * 10 >= W * H * 6, "seed {seed}");
            assert_eq!(plan, Plan::generate(seed));
        }
        assert_ne!(Plan::generate(1), Plan::generate(2));
        consistent(&Plan::fallback());
    }

    #[test]
    fn pieces_match_their_cells() {
        let set = TILES.get();
        assert!(!set.pieces.is_empty(), "the tile set loads");
        for seed in 0..20 {
            let plan = Plan::generate(seed);
            for z in 0..H {
                for x in 0..W {
                    let Some(p) = plan.pieces[z][x] else {
                        panic!("seed {seed}: no piece at {x},{z}");
                    };
                    let piece = &set.pieces[p.piece as usize];
                    let mask = piece.open.iter().fold(0, |m, s| m | s.bit());
                    assert_eq!(turned(mask, p.turns), plan.cells[z][x]);
                }
            }
        }
        // A quarter turn takes north to east, and the model turns the same way.
        assert_eq!(turned(N, 1), E);
        assert_eq!(turned(N | E, 3), N | WEST);
        let p = Placed { piece: 0, turns: 1 };
        assert!((p.rotation() * Vec3::NEG_Z - Vec3::X).length() < 1e-5);
    }

    #[test]
    fn paths_and_sight() {
        let plan = Plan::fallback();
        assert_eq!(plan.step_towards((0, 0), (2, 0)), Some((1, 0)));
        assert!(plan.sees(cell_centre(0, 0), cell_centre(6, 6)));
        // A wall between two cells blocks sight.
        let mut walled = plan.clone();
        walled.cells[0][0] &= !E;
        walled.cells[0][1] &= !WEST;
        assert!(!walled.sees(cell_centre(0, 0), cell_centre(1, 0)));
        assert_eq!(cell_of(cell_centre(3, 4)), Some((3, 4)));
        assert_eq!(cell_of(Vec2::splat(100.0)), None);
    }
}
