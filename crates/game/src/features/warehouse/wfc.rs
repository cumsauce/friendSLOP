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

/// How much each tile (a 4-bit mask of open sides) is wanted: aisles and
/// corners mostly, some junctions and dead ends, the odd blocked-up cell.
fn weight(mask: u8) -> f32 {
    match mask.count_ones() {
        0 => 0.6,
        1 => 1.6,
        2 if mask == N | S || mask == E | WEST => 3.0,
        2 => 2.6,
        3 => 1.8,
        _ => 1.0,
    }
}

/// A finished plan: the open sides of every cell, `cells[z][x]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub cells: [[u8; W]; H],
}

/// Tiny seeded generator (xorshift).
#[derive(Clone, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9) | 1)
    }
    pub fn roll(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    pub fn below(&mut self, n: usize) -> usize {
        ((self.roll() * n as f32) as usize).min(n.saturating_sub(1))
    }
}

fn in_grid(x: i32, z: i32) -> bool {
    (0..W as i32).contains(&x) && (0..H as i32).contains(&z)
}

/// Which tiles a cell may be before anything collapses: no openings through
/// the outer wall, except the loading door, which is always open.
fn start_domain(x: usize, z: usize) -> u16 {
    let mut dom = 0u16;
    for mask in 0..16u8 {
        let ok = SIDES.iter().all(|&(bit, dx, dz, _)| {
            let outside = !in_grid(x as i32 + dx, z as i32 + dz);
            let door = (x, z) == ENTRANCE && bit == N;
            if door {
                mask & bit != 0
            } else {
                !outside || mask & bit == 0
            }
        });
        if ok {
            dom |= 1 << mask;
        }
    }
    dom
}

/// One WFC attempt. `None` on a contradiction.
fn collapse(rng: &mut Rng) -> Option<Plan> {
    let mut dom = [[0u16; W]; H];
    for (z, row) in dom.iter_mut().enumerate() {
        for (x, d) in row.iter_mut().enumerate() {
            *d = start_domain(x, z);
        }
    }
    loop {
        // The most constrained open cell (ties broken at random).
        let mut best: Option<(usize, usize)> = None;
        let mut best_n = u32::MAX;
        for (z, row) in dom.iter().enumerate() {
            for (x, d) in row.iter().enumerate() {
                let n = d.count_ones();
                if n == 0 {
                    return None;
                }
                if n > 1 && (n < best_n || (n == best_n && rng.roll() < 0.3)) {
                    best = Some((x, z));
                    best_n = n;
                }
            }
        }
        let Some((x, z)) = best else { break };
        // Weighted pick among what's left.
        let options: Vec<u8> = (0..16u8).filter(|m| dom[z][x] & (1 << m) != 0).collect();
        let total: f32 = options.iter().map(|&m| weight(m)).sum();
        let mut roll = rng.roll() * total;
        let mut pick = options[options.len() - 1];
        for &m in &options {
            roll -= weight(m);
            if roll <= 0.0 {
                pick = m;
                break;
            }
        }
        dom[z][x] = 1 << pick;
        // Propagate.
        let mut stack = vec![(x, z)];
        while let Some((cx, cz)) = stack.pop() {
            for &(bit, dx, dz, back) in &SIDES {
                let (nx, nz) = (cx as i32 + dx, cz as i32 + dz);
                if !in_grid(nx, nz) {
                    continue;
                }
                let (nx, nz) = (nx as usize, nz as usize);
                // Which states of our shared side are still possible?
                let mut can_open = false;
                let mut can_close = false;
                for m in 0..16u8 {
                    if dom[cz][cx] & (1 << m) != 0 {
                        if m & bit != 0 {
                            can_open = true;
                        } else {
                            can_close = true;
                        }
                    }
                }
                let before = dom[nz][nx];
                let mut after = 0u16;
                for m in 0..16u8 {
                    if before & (1 << m) == 0 {
                        continue;
                    }
                    let open = m & back != 0;
                    if (open && can_open) || (!open && can_close) {
                        after |= 1 << m;
                    }
                }
                if after == 0 {
                    return None;
                }
                if after != before {
                    dom[nz][nx] = after;
                    stack.push((nx, nz));
                }
            }
        }
    }
    let mut cells = [[0u8; W]; H];
    for z in 0..H {
        for x in 0..W {
            cells[z][x] = dom[z][x].trailing_zeros() as u8;
        }
    }
    Some(Plan { cells })
}

impl Plan {
    /// Generate the plan for `seed`: WFC until it's connected enough.
    pub fn generate(seed: u32) -> Plan {
        let mut rng = Rng::new(seed);
        for _ in 0..200 {
            let Some(plan) = collapse(&mut rng) else {
                continue;
            };
            if plan.reachable().len() * 10 >= W * H * 6 {
                return plan;
            }
        }
        Plan::fallback()
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
        Plan { cells }
    }

    pub fn open(&self, x: usize, z: usize, bit: u8) -> bool {
        self.cells[z][x] & bit != 0
    }

    /// Neighbour cells you can walk to from (x, z).
    pub fn exits(&self, x: usize, z: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
        SIDES.iter().filter_map(move |&(bit, dx, dz, _)| {
            let (nx, nz) = (x as i32 + dx, z as i32 + dz);
            (self.open(x, z, bit) && in_grid(nx, nz)).then_some((nx as usize, nz as usize))
        })
    }

    /// Walking distance in cells from `from` to every cell (`u8::MAX` = unreachable).
    pub fn distances(&self, from: (usize, usize)) -> [[u8; W]; H] {
        let mut d = [[u8::MAX; W]; H];
        d[from.1][from.0] = 0;
        let mut queue = std::collections::VecDeque::from([from]);
        while let Some((x, z)) = queue.pop_front() {
            let here = d[z][x];
            for (nx, nz) in self.exits(x, z) {
                if d[nz][nx] == u8::MAX {
                    d[nz][nx] = here + 1;
                    queue.push_back((nx, nz));
                }
            }
        }
        d
    }

    /// Cells reachable from the loading door.
    pub fn reachable(&self) -> Vec<(usize, usize)> {
        let d = self.distances(ENTRANCE);
        let mut out = Vec::new();
        for (z, row) in d.iter().enumerate() {
            for (x, v) in row.iter().enumerate() {
                if *v != u8::MAX {
                    out.push((x, z));
                }
            }
        }
        out
    }

    /// The first step from `from` towards `to`, or `None` if `to` can't be reached.
    pub fn step_towards(&self, from: (usize, usize), to: (usize, usize)) -> Option<(usize, usize)> {
        if from == to {
            return Some(to);
        }
        let d = self.distances(to);
        if d[from.1][from.0] == u8::MAX {
            return None;
        }
        self.exits(from.0, from.1)
            .min_by_key(|&(x, z)| d[z][x])
            .filter(|&(x, z)| d[z][x] < d[from.1][from.0])
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
