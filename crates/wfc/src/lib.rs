//! Wave function collapse on a rectangular grid (#130). No game code.
//!
//! A [`Wfc`] is a `width × height` grid and a set of [`Tile`]s. Each tile has
//! one socket per [`Side`] and a weight. Two neighbouring cells agree when
//! their facing sockets are `compatible`; a cell on the edge must satisfy the
//! `boundary` rule for each side that faces off the grid. Cells can be pinned
//! to a subset of tiles before solving.
//!
//! Solving starts with every cell able to be any allowed tile, then
//! repeatedly collapses the cell with the fewest options left (ties broken at
//! random) to one weighted-random tile and propagates. A contradiction gives
//! `None`; [`Wfc::solve_until`] retries until an acceptance predicate passes.
//! Everything is pure and seeded by the crate's own [`Rng`], so every peer
//! gets the same result from the same seed. No threads, files or clocks.

use std::collections::VecDeque;

/// A grid cell, `(x, y)`. `y` grows "south".
pub type Cell = (usize, usize);

/// One of a cell's four sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Side {
    /// `-y`
    North,
    /// `+x`
    East,
    /// `+y`
    South,
    /// `-x`
    West,
}

impl Side {
    /// All sides, in the order the solver visits them.
    pub const ALL: [Side; 4] = [Side::North, Side::East, Side::South, Side::West];

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn opposite(self) -> Side {
        Side::ALL[(self.index() + 2) % 4]
    }

    /// `(dx, dy)` towards the neighbour on this side.
    pub fn offset(self) -> (i32, i32) {
        match self {
            Side::North => (0, -1),
            Side::East => (1, 0),
            Side::South => (0, 1),
            Side::West => (-1, 0),
        }
    }
}

/// The neighbour of `cell` across `side`, if it's on a `width × height` grid.
pub fn neighbour(width: usize, height: usize, cell: Cell, side: Side) -> Option<Cell> {
    let (dx, dy) = side.offset();
    let (x, y) = (cell.0 as i64 + dx as i64, cell.1 as i64 + dy as i64);
    (x >= 0 && y >= 0 && (x as usize) < width && (y as usize) < height)
        .then_some((x as usize, y as usize))
}

/// Tiny seeded generator (xorshift32).
#[derive(Clone, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9) | 1)
    }
    /// Uniform in `[0, 1)`.
    pub fn roll(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    /// Uniform in `0..n` (0 when `n` is 0).
    pub fn below(&mut self, n: usize) -> usize {
        ((self.roll() * n as f32) as usize).min(n.saturating_sub(1))
    }
}

/// A tile: a socket per side (indexed by [`Side::index`]) and a weight.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile<S> {
    pub sockets: [S; 4],
    pub weight: f32,
}

impl<S> Tile<S> {
    pub fn socket(&self, side: Side) -> &S {
        &self.sockets[side.index()]
    }
}

/// A solved grid: the tile index of every cell, row-major (`y * width + x`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Solution {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<usize>,
}

impl Solution {
    pub fn get(&self, cell: Cell) -> usize {
        self.tiles[cell.1 * self.width + cell.0]
    }
}

/// A WFC problem: grid size, tiles and pins.
#[derive(Clone, Debug)]
pub struct Wfc<S> {
    pub width: usize,
    pub height: usize,
    pub tiles: Vec<Tile<S>>,
    /// Cells restricted to a subset of tile indices before solving.
    pub pins: Vec<(Cell, Vec<usize>)>,
}

/// A per-cell set of tile indices.
type Domain = Vec<u64>;

fn has(d: &[u64], t: usize) -> bool {
    d[t / 64] & (1 << (t % 64)) != 0
}

fn count(d: &[u64]) -> u32 {
    d.iter().map(|w| w.count_ones()).sum()
}

impl<S> Wfc<S> {
    pub fn new(width: usize, height: usize, tiles: Vec<Tile<S>>) -> Self {
        Self {
            width,
            height,
            tiles,
            pins: Vec::new(),
        }
    }

    /// Restrict `cell` to `allowed` tiles (one tile = pre-collapsed).
    pub fn pin(mut self, cell: Cell, allowed: impl IntoIterator<Item = usize>) -> Self {
        self.pins.push((cell, allowed.into_iter().collect()));
        self
    }

    fn words(&self) -> usize {
        self.tiles.len().div_ceil(64).max(1)
    }

    /// One attempt. `compatible(a, b)`: may socket `a` face socket `b`?
    /// `boundary(cell, side, socket)`: may this socket face off the grid
    /// here? `None` on a contradiction.
    pub fn solve(
        &self,
        rng: &mut Rng,
        compatible: impl Fn(&S, &S) -> bool,
        boundary: impl Fn(Cell, Side, &S) -> bool,
    ) -> Option<Solution> {
        let (w, h, n) = (self.width, self.height, self.tiles.len());
        let words = self.words();
        let idx = |c: Cell| c.1 * w + c.0;
        let mut dom: Vec<Domain> = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let mut d = vec![0u64; words];
                for (t, tile) in self.tiles.iter().enumerate() {
                    let ok = Side::ALL.iter().all(|&s| {
                        neighbour(w, h, (x, y), s).is_some() || boundary((x, y), s, tile.socket(s))
                    });
                    if ok {
                        d[t / 64] |= 1 << (t % 64);
                    }
                }
                dom.push(d);
            }
        }
        let mut stack = Vec::new();
        for (cell, allowed) in &self.pins {
            if cell.0 >= w || cell.1 >= h {
                return None;
            }
            let mut keep = vec![0u64; words];
            for &t in allowed.iter().filter(|&&t| t < n) {
                keep[t / 64] |= 1 << (t % 64);
            }
            for (a, k) in dom[idx(*cell)].iter_mut().zip(&keep) {
                *a &= k;
            }
            stack.push(*cell);
        }
        self.propagate(&mut dom, &mut stack, &compatible)?;
        loop {
            // The most constrained open cell (ties broken at random).
            let mut best: Option<Cell> = None;
            let mut best_n = u32::MAX;
            for y in 0..h {
                for x in 0..w {
                    let c = count(&dom[idx((x, y))]);
                    if c == 0 {
                        return None;
                    }
                    if c > 1 && (c < best_n || (c == best_n && rng.roll() < 0.3)) {
                        best = Some((x, y));
                        best_n = c;
                    }
                }
            }
            let Some(cell) = best else { break };
            // Weighted pick among what's left.
            let d = &dom[idx(cell)];
            let options: Vec<usize> = (0..n).filter(|&t| has(d, t)).collect();
            let total: f32 = options.iter().map(|&t| self.tiles[t].weight).sum();
            let mut roll = rng.roll() * total;
            let mut pick = *options.last()?;
            for &t in &options {
                roll -= self.tiles[t].weight;
                if roll <= 0.0 {
                    pick = t;
                    break;
                }
            }
            let mut one = vec![0u64; words];
            one[pick / 64] |= 1 << (pick % 64);
            dom[idx(cell)] = one;
            stack.push(cell);
            self.propagate(&mut dom, &mut stack, &compatible)?;
        }
        let tiles = dom
            .iter()
            .map(|d| (0..n).find(|&t| has(d, t)))
            .collect::<Option<Vec<_>>>()?;
        Some(Solution {
            width: w,
            height: h,
            tiles,
        })
    }

    fn propagate(
        &self,
        dom: &mut [Domain],
        stack: &mut Vec<Cell>,
        compatible: &impl Fn(&S, &S) -> bool,
    ) -> Option<()> {
        let (w, h, n) = (self.width, self.height, self.tiles.len());
        while let Some(c) = stack.pop() {
            for side in Side::ALL {
                let Some(nb) = neighbour(w, h, c, side) else {
                    continue;
                };
                let here = dom[c.1 * w + c.0].clone();
                let ni = nb.1 * w + nb.0;
                let before = dom[ni].clone();
                let mut after = before.clone();
                for m in (0..n).filter(|&m| has(&before, m)) {
                    let theirs = self.tiles[m].socket(side.opposite());
                    let supported = (0..n)
                        .any(|t| has(&here, t) && compatible(self.tiles[t].socket(side), theirs));
                    if !supported {
                        after[m / 64] &= !(1 << (m % 64));
                    }
                }
                if count(&after) == 0 {
                    return None;
                }
                if after != before {
                    dom[ni] = after;
                    stack.push(nb);
                }
            }
        }
        Some(())
    }

    /// Up to `attempts` solves with the same generator, returning the first
    /// one `accept` likes.
    pub fn solve_until(
        &self,
        rng: &mut Rng,
        attempts: usize,
        compatible: impl Fn(&S, &S) -> bool,
        boundary: impl Fn(Cell, Side, &S) -> bool,
        mut accept: impl FnMut(&Solution) -> bool,
    ) -> Option<Solution> {
        (0..attempts).find_map(|_| {
            self.solve(rng, &compatible, &boundary)
                .filter(|s| accept(s))
        })
    }
}

/// Neighbours of `cell` you can step to: `open(cell, side)` says whether a
/// side is passable (both cells' sides are usually checked by the caller).
pub fn exits(
    width: usize,
    height: usize,
    cell: Cell,
    open: impl Fn(Cell, Side) -> bool,
) -> impl Iterator<Item = Cell> {
    Side::ALL
        .into_iter()
        .filter_map(move |s| neighbour(width, height, cell, s).filter(|_| open(cell, s)))
}

/// Steps from `from` to every cell, row-major (`None` = unreachable).
pub fn distances(
    width: usize,
    height: usize,
    from: Cell,
    open: impl Fn(Cell, Side) -> bool,
) -> Vec<Option<u32>> {
    let mut d = vec![None; width * height];
    if from.0 >= width || from.1 >= height {
        return d;
    }
    d[from.1 * width + from.0] = Some(0);
    let mut queue = VecDeque::from([from]);
    while let Some(c) = queue.pop_front() {
        let here = d[c.1 * width + c.0].unwrap_or(0);
        for nb in exits(width, height, c, &open) {
            let slot = &mut d[nb.1 * width + nb.0];
            if slot.is_none() {
                *slot = Some(here + 1);
                queue.push_back(nb);
            }
        }
    }
    d
}

/// Cells reachable from `from` (row-major order).
pub fn reachable(
    width: usize,
    height: usize,
    from: Cell,
    open: impl Fn(Cell, Side) -> bool,
) -> Vec<Cell> {
    distances(width, height, from, open)
        .iter()
        .enumerate()
        .filter(|(_, d)| d.is_some())
        .map(|(i, _)| (i % width, i / width))
        .collect()
}

/// The first step from `from` towards `to` on a shortest path, or `None` if
/// `to` can't be reached. `Some(to)` when already there.
pub fn step_towards(
    width: usize,
    height: usize,
    from: Cell,
    to: Cell,
    open: impl Fn(Cell, Side) -> bool,
) -> Option<Cell> {
    if from == to {
        return Some(to);
    }
    let d = distances(width, height, to, &open);
    let at = |c: Cell| d.get(c.1 * width + c.0).copied().flatten();
    let here = at(from)?;
    exits(width, height, from, &open)
        .filter_map(|c| at(c).map(|v| (c, v)))
        .min_by_key(|&(_, v)| v)
        .filter(|&(_, v)| v < here)
        .map(|(c, _)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Open/closed sides: 16 tiles, tile index = open-side bitmask.
    fn pipes(w: usize, h: usize) -> Wfc<bool> {
        let tiles = (0..16u8)
            .map(|m| Tile {
                sockets: [m & 1 != 0, m & 2 != 0, m & 4 != 0, m & 8 != 0],
                weight: 1.0 + m.count_ones() as f32,
            })
            .collect();
        Wfc::new(w, h, tiles)
    }

    fn eq(a: &bool, b: &bool) -> bool {
        a == b
    }
    fn walled(_: Cell, _: Side, s: &bool) -> bool {
        !*s
    }

    fn check(p: &Wfc<bool>, s: &Solution) {
        for y in 0..s.height {
            for x in 0..s.width {
                let t = &p.tiles[s.get((x, y))];
                for side in Side::ALL {
                    match neighbour(s.width, s.height, (x, y), side) {
                        Some(nb) => {
                            assert_eq!(t.socket(side), p.tiles[s.get(nb)].socket(side.opposite()))
                        }
                        None => assert!(!t.socket(side)),
                    }
                }
            }
        }
    }

    #[test]
    fn deterministic_and_consistent() {
        let p = pipes(9, 5);
        for seed in 0..40 {
            let a = p.solve_until(&mut Rng::new(seed), 50, eq, walled, |_| true);
            let b = p.solve_until(&mut Rng::new(seed), 50, eq, walled, |_| true);
            assert_eq!(a, b);
            check(&p, &a.expect("solvable"));
        }
    }

    #[test]
    fn pins_are_respected() {
        let p = pipes(6, 6).pin((2, 2), [15]).pin((0, 0), [0]);
        for seed in 0..20 {
            let s = p
                .solve_until(&mut Rng::new(seed), 50, eq, walled, |_| true)
                .expect("solvable");
            assert_eq!(s.get((2, 2)), 15);
            assert_eq!(s.get((0, 0)), 0);
            check(&p, &s);
        }
    }

    #[test]
    fn contradictions_give_none() {
        // A corner pinned to a tile open towards the outer wall.
        let p = pipes(3, 3).pin((0, 0), [1]);
        assert_eq!(p.solve(&mut Rng::new(1), eq, walled), None);
        // Two neighbours that can't agree.
        let p = pipes(3, 3).pin((0, 0), [2]).pin((1, 0), [0]);
        assert_eq!(p.solve(&mut Rng::new(1), eq, walled), None);
        // An acceptance predicate that never passes.
        let p = pipes(3, 3);
        assert_eq!(
            p.solve_until(&mut Rng::new(1), 5, eq, walled, |_| false),
            None
        );
    }

    #[test]
    fn graph_helpers() {
        let open = |_: Cell, _: Side| true;
        let d = distances(4, 3, (0, 0), open);
        assert_eq!(d[2 * 4 + 3], Some(5));
        assert_eq!(reachable(4, 3, (0, 0), open).len(), 12);
        assert_eq!(step_towards(4, 3, (0, 0), (2, 0), open), Some((1, 0)));
        let shut = |_: Cell, _: Side| false;
        assert_eq!(reachable(4, 3, (1, 1), shut), vec![(1, 1)]);
        assert_eq!(step_towards(4, 3, (0, 0), (2, 0), shut), None);
    }
}
