//! Seeded room graphs on a grid (structures rework, phase 3).
//!
//! [`generate`] lays out rectangular rooms (some pinned by the caller, the
//! rest rolled from the seed), joins them with one-cell corridors along a
//! spanning tree (so every room is reachable), adds a few extra corridors
//! for loops, and pokes short dead-end spurs off the corridors. Rooms marked
//! [`Room::leaf`] hang off the rest by exactly one corridor, a guaranteed
//! chokepoint. Everything is plain data in `BTreeMap`s, so the same seed
//! gives the same graph on every peer. No game code.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// A grid cell (x, z).
pub type Cell = (i32, i32);

const STEPS: [Cell; 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// The four neighbours of a cell.
pub fn neighbours(c: Cell) -> impl Iterator<Item = Cell> {
    STEPS.into_iter().map(move |d| (c.0 + d.0, c.1 + d.1))
}

/// A rectangle of cells, `min..=max`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Room {
    pub min: Cell,
    pub max: Cell,
    /// What the caller calls it (the generator only copies it around).
    pub tag: u8,
    /// Joined to the others by exactly one corridor, and never by loops.
    pub leaf: bool,
}

impl Room {
    /// The smallest room covering both cells.
    pub fn covering(a: Cell, b: Cell, tag: u8) -> Room {
        Room {
            min: (a.0.min(b.0), a.1.min(b.1)),
            max: (a.0.max(b.0), a.1.max(b.1)),
            tag,
            leaf: false,
        }
    }

    /// A square of `r` cells around `c` (0 is one cell).
    pub fn around(c: Cell, r: i32, tag: u8) -> Room {
        Room::covering((c.0 - r, c.1 - r), (c.0 + r, c.1 + r), tag)
    }

    pub fn leaf(self) -> Room {
        Room { leaf: true, ..self }
    }

    /// Grow to also cover `c`.
    pub fn with(self, c: Cell) -> Room {
        Room {
            min: (self.min.0.min(c.0), self.min.1.min(c.1)),
            max: (self.max.0.max(c.0), self.max.1.max(c.1)),
            ..self
        }
    }

    pub fn contains(&self, c: Cell) -> bool {
        (self.min.0..=self.max.0).contains(&c.0) && (self.min.1..=self.max.1).contains(&c.1)
    }

    pub fn centre(&self) -> Cell {
        ((self.min.0 + self.max.0) / 2, (self.min.1 + self.max.1) / 2)
    }

    /// Do the two rooms overlap once each is grown by `margin` cells?
    pub fn near(&self, o: &Room, margin: i32) -> bool {
        self.min.0 - margin <= o.max.0
            && o.min.0 - margin <= self.max.0
            && self.min.1 - margin <= o.max.1
            && o.min.1 - margin <= self.max.1
    }

    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (self.min.1..=self.max.1).flat_map(move |z| (self.min.0..=self.max.0).map(move |x| (x, z)))
    }
}

/// What an open cell belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Use {
    /// Inside room `i`.
    Room(usize),
    /// On the corridor of edge `i`.
    Corridor(usize),
    /// On a dead-end spur.
    Spur,
}

/// What to generate.
#[derive(Clone, Debug)]
pub struct Spec {
    /// The area everything stays inside (inclusive).
    pub min: Cell,
    pub max: Cell,
    /// Rooms that must exist exactly here (entrances, goals).
    pub pins: Vec<Room>,
    /// How many rooms to add (inclusive range), if they fit.
    pub rooms: (usize, usize),
    /// Side length of added rooms, in cells (inclusive range).
    pub room_size: (i32, i32),
    /// Tag of added rooms.
    pub tag: u8,
    /// Extra corridors beyond the spanning tree.
    pub loops: usize,
    /// How many dead-end spurs to try, and their length range.
    pub spurs: usize,
    pub spur_len: (i32, i32),
}

/// A generated graph.
#[derive(Clone, Debug, Default)]
pub struct Graph {
    /// Pins first, in the order given, then the added rooms.
    pub rooms: Vec<Room>,
    /// Corridors, as pairs of room indices.
    pub edges: Vec<(usize, usize)>,
    pub cells: BTreeMap<Cell, Use>,
}

/// A small deterministic random stream.
#[derive(Clone, Copy, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u32) -> Self {
        Rng(u64::from(seed) ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn roll(&mut self) -> u32 {
        // splitmix64
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) as u32
    }

    /// In `lo..=hi`.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.roll() % (hi - lo + 1) as u32) as i32
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 { 0 } else { self.roll() as usize % n }
    }

    pub fn chance(&mut self, p: f32) -> bool {
        (self.roll() >> 8) as f32 / (1u32 << 24) as f32 <= p
    }
}

fn manhattan(a: Cell, b: Cell) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

impl Graph {
    pub fn get(&self, c: Cell) -> Option<Use> {
        self.cells.get(&c).copied()
    }

    pub fn open(&self, c: Cell) -> bool {
        self.cells.contains_key(&c)
    }

    /// Open neighbours of a cell.
    pub fn exits(&self, c: Cell) -> usize {
        neighbours(c).filter(|n| self.open(*n)).count()
    }

    /// Shortest walk from `a` to `b` through open cells, both included.
    pub fn path(&self, a: Cell, b: Cell) -> Option<Vec<Cell>> {
        if !self.open(a) || !self.open(b) {
            return None;
        }
        let mut prev: BTreeMap<Cell, Cell> = BTreeMap::from([(a, a)]);
        let mut todo = VecDeque::from([a]);
        while let Some(c) = todo.pop_front() {
            if c == b {
                let mut out = vec![b];
                let mut at = b;
                while at != a {
                    at = prev[&at];
                    out.push(at);
                }
                out.reverse();
                return Some(out);
            }
            for n in neighbours(c) {
                if self.open(n) && !prev.contains_key(&n) {
                    prev.insert(n, c);
                    todo.push_back(n);
                }
            }
        }
        None
    }

    pub fn connected(&self, a: Cell, b: Cell) -> bool {
        self.path(a, b).is_some()
    }

    /// Cells with one way out.
    pub fn dead_ends(&self) -> Vec<Cell> {
        self.cells
            .keys()
            .copied()
            .filter(|c| self.exits(*c) == 1)
            .collect()
    }

    /// Cells whose loss would cut the network in two (articulation points).
    pub fn chokepoints(&self) -> Vec<Cell> {
        let mut order: BTreeMap<Cell, u32> = BTreeMap::new();
        let mut low: BTreeMap<Cell, u32> = BTreeMap::new();
        let mut cut = BTreeSet::new();
        for &root in self.cells.keys() {
            if order.contains_key(&root) {
                continue;
            }
            // Iterative DFS: (cell, parent, next neighbour index, children).
            let mut stack: Vec<(Cell, Option<Cell>, usize, u32)> = vec![(root, None, 0, 0)];
            let mut t = order.len() as u32;
            order.insert(root, t);
            low.insert(root, t);
            while let Some(&mut (c, parent, ref mut i, ref mut kids)) = stack.last_mut() {
                if *i < 4 {
                    let d = STEPS[*i];
                    *i += 1;
                    let n = (c.0 + d.0, c.1 + d.1);
                    if !self.open(n) || Some(n) == parent {
                        continue;
                    }
                    if let Some(&on) = order.get(&n) {
                        let l = low[&c].min(on);
                        low.insert(c, l);
                    } else {
                        *kids += 1;
                        t += 1;
                        order.insert(n, t);
                        low.insert(n, t);
                        stack.push((n, Some(c), 0, 0));
                    }
                    continue;
                }
                let (c, parent, _, kids) = (c, parent, *i, *kids);
                stack.pop();
                match parent {
                    Some(p) => {
                        let l = low[&p].min(low[&c]);
                        low.insert(p, l);
                        if stack.len() > 1 && low[&c] >= order[&p] {
                            cut.insert(p);
                        }
                    }
                    None => {
                        if kids > 1 {
                            cut.insert(c);
                        }
                    }
                }
            }
        }
        cut.into_iter().collect()
    }

    fn carve(&mut self, c: Cell, u: Use) {
        self.cells.entry(c).or_insert(u);
    }
}

/// Generate a room graph from `spec` and `seed`.
pub fn generate(spec: &Spec, seed: u32) -> Graph {
    let mut rng = Rng::new(seed);
    let mut g = Graph {
        rooms: spec.pins.clone(),
        ..Graph::default()
    };
    let inside = |c: Cell| {
        (spec.min.0..=spec.max.0).contains(&c.0) && (spec.min.1..=spec.max.1).contains(&c.1)
    };

    // Added rooms, kept a cell of rock away from every other room.
    let want = rng.range(spec.rooms.0 as i32, spec.rooms.1 as i32).max(0) as usize;
    let mut added = 0;
    for _ in 0..want * 40 {
        if added >= want {
            break;
        }
        let w = rng.range(spec.room_size.0, spec.room_size.1).max(1);
        let h = rng.range(spec.room_size.0, spec.room_size.1).max(1);
        let x = rng.range(spec.min.0, spec.max.0 - w + 1);
        let z = rng.range(spec.min.1, spec.max.1 - h + 1);
        let room = Room {
            min: (x, z),
            max: (x + w - 1, z + h - 1),
            tag: spec.tag,
            leaf: false,
        };
        if !inside(room.max) || g.rooms.iter().any(|r| r.near(&room, 2)) {
            continue;
        }
        g.rooms.push(room);
        added += 1;
    }
    for (i, r) in g.rooms.iter().enumerate() {
        for c in r.cells() {
            g.cells.entry(c).or_insert(Use::Room(i));
        }
    }

    // Spanning tree over the non-leaf rooms (Prim, shortest first), then each
    // leaf onto its nearest non-leaf room.
    let n = g.rooms.len();
    let centre: Vec<Cell> = g.rooms.iter().map(Room::centre).collect();
    let core: Vec<usize> = (0..n).filter(|&i| !g.rooms[i].leaf).collect();
    let mut tree: Vec<(usize, usize)> = Vec::new();
    if let Some(&first) = core.first() {
        let mut joined = vec![first];
        while joined.len() < core.len() {
            let best = core
                .iter()
                .filter(|i| !joined.contains(i))
                .flat_map(|&i| joined.iter().map(move |&j| (i, j)))
                .min_by_key(|&(i, j)| (manhattan(centre[i], centre[j]), i, j));
            let Some((i, j)) = best else { break };
            tree.push((j, i));
            joined.push(i);
        }
        for i in (0..n).filter(|&i| g.rooms[i].leaf) {
            if let Some(&j) = core
                .iter()
                .min_by_key(|&&j| (manhattan(centre[i], centre[j]), j))
            {
                tree.push((j, i));
            }
        }
    }
    // Loops: a few of the shortest corridors the tree didn't use.
    let mut spare: Vec<(i32, usize, usize)> = core
        .iter()
        .flat_map(|&i| core.iter().map(move |&j| (i, j)))
        .filter(|&(i, j)| i < j)
        .filter(|&(i, j)| !tree.contains(&(i, j)) && !tree.contains(&(j, i)))
        .map(|(i, j)| (manhattan(centre[i], centre[j]), i, j))
        .collect();
    spare.sort();
    spare.truncate(spec.loops * 2 + 1);
    let mut edges = tree;
    for _ in 0..spec.loops {
        if spare.is_empty() {
            break;
        }
        let (_, i, j) = spare.remove(rng.below(spare.len()));
        edges.push((i, j));
    }

    // Carve each corridor as an L, sometimes with a jog part way along.
    for (e, &(a, b)) in edges.iter().enumerate() {
        let (from, to) = (centre[a], centre[b]);
        let mut way = vec![from];
        if rng.chance(0.5) && manhattan(from, to) > 6 {
            way.push((
                rng.range(from.0.min(to.0), from.0.max(to.0)),
                rng.range(from.1.min(to.1), from.1.max(to.1)),
            ));
        }
        way.push(to);
        for w in way.windows(2) {
            let (p, q) = (w[0], w[1]);
            let x_first = rng.chance(0.5);
            let elbow = if x_first { (q.0, p.1) } else { (p.0, q.1) };
            for (s, t) in [(p, elbow), (elbow, q)] {
                let mut c = s;
                g.carve(c, Use::Corridor(e));
                while c != t {
                    c = (c.0 + (t.0 - c.0).signum(), c.1 + (t.1 - c.1).signum());
                    g.carve(c, Use::Corridor(e));
                }
            }
        }
    }
    g.edges = edges;

    // Dead-end spurs: short walks off a corridor into solid rock that never
    // touch anything else on the way.
    let corridor: Vec<Cell> = g
        .cells
        .iter()
        .filter(|(_, u)| matches!(u, Use::Corridor(_)))
        .map(|(c, _)| *c)
        .collect();
    for _ in 0..spec.spurs {
        if corridor.is_empty() {
            break;
        }
        let start = corridor[rng.below(corridor.len())];
        let d = STEPS[rng.below(4)];
        let len = rng.range(spec.spur_len.0, spec.spur_len.1);
        let mut way = Vec::new();
        let mut c = start;
        for _ in 0..len {
            let next = (c.0 + d.0, c.1 + d.1);
            let clear = inside(next)
                && !g.open(next)
                && neighbours(next).all(|m| m == c || !g.open(m))
                && !way.contains(&next);
            // Keep the sides clear too, so the spur stays one cell wide.
            let side = (d.1, d.0);
            let sides = [
                (next.0 + side.0, next.1 + side.1),
                (next.0 - side.0, next.1 - side.1),
            ];
            if !clear || sides.iter().any(|s| g.open(*s)) {
                break;
            }
            way.push(next);
            c = next;
        }
        if way.len() >= spec.spur_len.0.max(1) as usize {
            for c in way {
                g.carve(c, Use::Spur);
            }
        }
    }
    fill_pinches(&mut g);
    g
}

/// Cells that only touch diagonally get one of the cells between them
/// opened, so walls always meet at proper corners (the hallway kit relies
/// on it).
fn fill_pinches(g: &mut Graph) {
    loop {
        let pinch = g.cells.keys().find_map(|&c| {
            [(1, 1), (1, -1)].into_iter().find_map(|(dx, dz)| {
                let d = (c.0 + dx, c.1 + dz);
                let (a, b) = ((d.0, c.1), (c.0, d.1));
                (g.open(d) && !g.open(a) && !g.open(b)).then_some(a)
            })
        });
        let Some(a) = pinch else { return };
        g.cells.insert(a, Use::Spur);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> Spec {
        Spec {
            min: (-12, -24),
            max: (27, 10),
            pins: vec![
                Room::around((0, 0), 1, 0),
                Room::around((7, 0), 0, 1),
                Room::covering((-7, 7), (-6, 7), 1),
                Room::around((24, -4), 1, 2),
                Room::around((-9, -21), 0, 3).leaf(),
            ],
            rooms: (4, 7),
            room_size: (2, 4),
            tag: 9,
            loops: 2,
            spurs: 6,
            spur_len: (2, 4),
        }
    }

    #[test]
    fn every_pin_is_kept_and_joined() {
        for seed in 0..64 {
            let s = spec();
            let g = generate(&s, seed);
            for (i, p) in s.pins.iter().enumerate() {
                assert_eq!(g.rooms[i], *p);
                for c in p.cells() {
                    assert_eq!(g.get(c), Some(Use::Room(i)), "seed {seed}");
                }
            }
            let hub = s.pins[0].centre();
            for c in g.cells.keys() {
                assert!(g.connected(hub, *c), "seed {seed}: {c:?} cut off");
                assert!(
                    (s.min.0..=s.max.0).contains(&c.0) && (s.min.1..=s.max.1).contains(&c.1),
                    "seed {seed}: {c:?} out of bounds"
                );
            }
        }
    }

    #[test]
    fn same_seed_same_graph_other_seed_other_graph() {
        let a = generate(&spec(), 5);
        let b = generate(&spec(), 5);
        assert_eq!(a.cells, b.cells);
        assert_eq!(a.edges, b.edges);
        let differ = (6..20).any(|s| generate(&spec(), s).cells != a.cells);
        assert!(differ);
    }

    #[test]
    fn rooms_never_overlap() {
        for seed in 0..64 {
            let g = generate(&spec(), seed);
            let pins = spec().pins.len();
            for (i, a) in g.rooms.iter().enumerate().skip(pins) {
                for (j, b) in g.rooms.iter().enumerate() {
                    if i != j {
                        assert!(!a.near(b, 1), "seed {seed}: room {i} touches {j}");
                    }
                }
            }
        }
    }

    #[test]
    fn loops_dead_ends_and_chokepoints() {
        let (mut loops, mut ends) = (0, 0);
        for seed in 0..32 {
            let g = generate(&spec(), seed);
            // The leaf hangs by one corridor: its way out is a chokepoint.
            let leaf = spec().pins[4].centre();
            let cuts = g.chokepoints();
            let hub = spec().pins[0].centre();
            let Some(path) = g.path(hub, leaf) else {
                panic!("seed {seed}: leaf unreachable")
            };
            assert!(
                path.iter().any(|c| cuts.contains(c)),
                "seed {seed}: no chokepoint on the way to the leaf"
            );
            // Cycles exist when cells outnumber the tree's edges.
            let links: usize = g.cells.keys().map(|c| g.exits(*c)).sum::<usize>() / 2;
            if links >= g.cells.len() {
                loops += 1;
            }
            ends += g.dead_ends().len().min(1);
        }
        assert!(loops > 24, "too few seeds with loops: {loops}");
        assert!(ends > 24, "too few seeds with dead ends: {ends}");
    }

    #[test]
    fn chokepoints_of_a_line_and_a_ring() {
        let mut g = Graph::default();
        for x in 0..5 {
            g.cells.insert((x, 0), Use::Spur);
        }
        assert_eq!(g.chokepoints(), vec![(1, 0), (2, 0), (3, 0)]);
        let mut ring = Graph::default();
        for c in [
            (0, 0),
            (1, 0),
            (2, 0),
            (2, 1),
            (2, 2),
            (1, 2),
            (0, 2),
            (0, 1),
        ] {
            ring.cells.insert(c, Use::Spur);
        }
        assert!(ring.chokepoints().is_empty());
    }
}
