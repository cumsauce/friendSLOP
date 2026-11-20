//! The concourse levels' planner: a graph of places joined by corridors,
//! instead of a maze grown cell by cell.
//!
//! The fixed points go down first (where the stairs from above land, the
//! ways down), then the set pieces: galleries (long halls two cells wide
//! that bend round rounded corners, the tunnel ring's tiles without the
//! track) and rooms. Every doorway of every one of them is an anchor, and
//! corridors are routed between them (cheapest path, with a little noise so
//! they wander and a cost on turning so they don't zig-zag), each joining
//! whatever is already built. Then extra links close loops (two places far
//! apart by the corridors but near on the grid), short dead ends branch off,
//! and each corridor cell gets the tile piece for its open sides. Pure and
//! seeded; `None` when nothing fitted in the tries it has.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use friendslop_wfc as wfc;

use super::{
    Fill, Kind, Level, LevelAsk, LevelKind, Meta, TunnelPiece, TunnelTile, Var, bit, path_to,
    special, turned,
};

/// What a concourse level asks for (`fulton_layout.ron`, `concourse`).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct ConcourseAsk {
    /// How many galleries (at least, at most).
    pub galleries: (usize, usize),
    /// Their shapes, a letter a tile (12 m): `S` straight on, `C` a quarter
    /// turn (left or right, picked as it's laid).
    pub shapes: Vec<String>,
    /// How many rooms (at least, at most; their sizes and doors are
    /// fulton.ron's `rooms`).
    pub rooms: (usize, usize),
    /// Extra corridors that close a loop (at least, at most)...
    pub loops: (usize, usize),
    /// ...between two places at least this many steps apart by the
    /// corridors, and at most `loop_len` cells long.
    pub loop_apart: u32,
    pub loop_len: usize,
    /// Dead ends off the corridors (at least, at most), each up to
    /// `spur_len` cells long.
    pub spurs: (usize, usize),
    pub spur_len: usize,
    /// How much a corridor wanders (noise on each cell's cost; 0: as
    /// straight as it can) and what a turn costs (in cells).
    pub wander: f32,
    pub turn: f32,
    /// The chance a concourse over another concourse gets a two-storey
    /// room: its floor drops a whole level, onto the floor of the one below
    /// (whose corridors come in through doorways down there), balconies at
    /// its doors up here and long stairs down.
    #[serde(default)]
    pub two_storey: f32,
}

/// How many times it tries before giving up.
const TRIES: usize = 60;

/// One try's grid: every cell's open sides and what is fixed there.
struct Grid {
    w: usize,
    h: usize,
    mask: Vec<u8>,
    /// Cells something is set on (`None`: free, or a corridor once routed).
    fixed: Vec<Option<Fill>>,
    /// Corridor cells routed so far.
    corridor: Vec<bool>,
    /// Free cells kept free (a fixed thing's doorway).
    reserved: Vec<bool>,
}

impl Grid {
    fn i(&self, c: wfc::Cell) -> usize {
        c.1 * self.w + c.0
    }
    fn free(&self, c: wfc::Cell) -> bool {
        let i = self.i(c);
        self.fixed[i].is_none() && !self.corridor[i]
    }
    fn placeable(&self, c: wfc::Cell) -> bool {
        self.free(c) && !self.reserved[self.i(c)]
    }
    fn nb(&self, c: wfc::Cell, s: wfc::Side) -> Option<wfc::Cell> {
        wfc::neighbour(self.w, self.h, c, s)
    }
    /// Open side `s` of `c` and the side back.
    fn link(&mut self, c: wfc::Cell, s: wfc::Side) {
        if let Some(n) = self.nb(c, s) {
            let (i, j) = (self.i(c), self.i(n));
            self.mask[i] |= bit(s);
            self.mask[j] |= bit(s.opposite());
        }
    }
    /// Steps from `from` to every cell along open sides.
    fn steps(&self, from: wfc::Cell) -> Vec<Option<u32>> {
        wfc::distances(self.w, self.h, from, |c, s| {
            self.nb(c, s).is_some_and(|n| {
                self.mask[self.i(c)] & bit(s) != 0 && self.mask[self.i(n)] & bit(s.opposite()) != 0
            })
        })
    }
}

/// The cheapest way from `from` to a cell `goal` accepts, through cells
/// `pass` allows (`from` itself is never the goal), costing `cost` a cell and
/// `turn` a turn. Cells from `from` to the goal, both included.
fn route(
    g: &Grid,
    from: wfc::Cell,
    pass: impl Fn(wfc::Cell) -> bool,
    goal: impl Fn(wfc::Cell) -> bool,
    cost: impl Fn(wfc::Cell) -> f32,
    turn: f32,
    max_len: usize,
) -> Option<Vec<wfc::Cell>> {
    // A state is a cell and the side it was entered across (4 = the start).
    let states = g.w * g.h * 5;
    let key = |c: wfc::Cell, d: usize| (c.1 * g.w + c.0) * 5 + d;
    let mut best = vec![u32::MAX; states];
    let mut back: Vec<Option<(usize, wfc::Cell)>> = vec![None; states];
    let mut len = vec![0usize; states];
    let mut heap = BinaryHeap::new();
    let fixed = |x: f32| (x * 1000.0) as u32;
    best[key(from, 4)] = 0;
    heap.push(Reverse((0u32, from.0, from.1, 4usize)));
    while let Some(Reverse((d, x, z, came))) = heap.pop() {
        let c = (x, z);
        let k = key(c, came);
        if d > best[k] {
            continue;
        }
        if c != from && goal(c) {
            let mut out = vec![c];
            let mut at = k;
            while let Some((prev, cell)) = back[at] {
                out.push(cell);
                at = prev;
            }
            out.reverse();
            return Some(out);
        }
        if len[k] >= max_len {
            continue;
        }
        for s in wfc::Side::ALL {
            let Some(n) = g.nb(c, s) else { continue };
            if !(pass(n) || goal(n)) {
                continue;
            }
            let turning = came != 4 && came != s.index();
            let step = fixed(cost(n) + if turning { turn } else { 0.0 });
            let nk = key(n, s.index());
            let nd = d.saturating_add(step.max(1));
            if nd < best[nk] {
                best[nk] = nd;
                back[nk] = Some((k, c));
                len[nk] = len[k] + 1;
                heap.push(Reverse((nd, n.0, n.1, s.index())));
            }
        }
    }
    None
}

/// Every way a gallery of `shape` (see [`ConcourseAsk::shapes`]) fits on
/// free cells; one of them, picked.
#[allow(clippy::type_complexity)]
fn lay_gallery(
    rng: &mut wfc::Rng,
    g: &Grid,
    shape: &str,
) -> Option<(Vec<TunnelTile>, Vec<(wfc::Cell, wfc::Side)>)> {
    let curves = shape.chars().filter(|l| matches!(l, 'C' | 'c')).count();
    let mut fits = Vec::new();
    for z in 0..g.h as i32 - 1 {
        for x in 0..g.w as i32 - 1 {
            for d in wfc::Side::ALL {
                for turns in 0..1u32 << curves {
                    if let Some(f) = gallery_at(g, shape, (x, z), d, turns) {
                        fits.push(f);
                    }
                }
            }
        }
    }
    (!fits.is_empty()).then(|| fits.swap_remove(rng.below(fits.len())))
}

/// A gallery of `shape` from top-left cell `start` heading `d`, its curves
/// turning right where `rights` has their bit: its tiles, and at each end the
/// two cells outside it and the side of theirs that opens into it.
#[allow(clippy::type_complexity)]
fn gallery_at(
    g: &Grid,
    shape: &str,
    start: (i32, i32),
    mut d: wfc::Side,
    rights: u32,
) -> Option<(Vec<TunnelTile>, Vec<(wfc::Cell, wfc::Side)>)> {
    let (w, h) = (g.w as i32, g.h as i32);
    let first = d;
    let mut curve = 0;
    let mut at = start;
    let mut tiles = Vec::new();
    let mut cells: Vec<wfc::Cell> = Vec::new();
    for letter in shape.chars() {
        let came = d.opposite();
        let (piece, out) = match letter {
            'C' | 'c' => {
                let right = rights & (1 << curve) != 0;
                curve += 1;
                (
                    TunnelPiece::Curve,
                    wfc::Side::ALL[(d.index() + if right { 1 } else { 3 }) % 4],
                )
            }
            _ => (TunnelPiece::Straight, d),
        };
        let want = bit(came) | bit(out);
        let turns = (0..4u8).find(|&t| {
            let m = match piece {
                TunnelPiece::Straight => bit(wfc::Side::North) | bit(wfc::Side::South),
                TunnelPiece::Curve => bit(wfc::Side::North) | bit(wfc::Side::East),
            };
            turned(m, t) == want
        })?;
        if at.0 < 0 || at.1 < 0 || at.0 + 2 > w || at.1 + 2 > h {
            return None;
        }
        let tl = (at.0 as usize, at.1 as usize);
        for dz in 0..2 {
            for dx in 0..2 {
                let c = (tl.0 + dx, tl.1 + dz);
                if !g.placeable(c) || cells.contains(&c) {
                    return None;
                }
                cells.push(c);
            }
        }
        tiles.push(TunnelTile {
            at: tl,
            piece,
            turns,
        });
        d = out;
        let (ox, oz) = d.offset();
        at = (at.0 + 2 * ox, at.1 + 2 * oz);
    }
    // The ends: the two cells beyond the first tile's way in and the last's way out.
    let mut ends = Vec::new();
    for (tile, side) in [(tiles[0], first.opposite()), (*tiles.last()?, d)] {
        let mut pair = Vec::new();
        for dz in 0..2 {
            for dx in 0..2 {
                let c = (tile.at.0 + dx, tile.at.1 + dz);
                if let Some(n) = g.nb(c, side).filter(|n| !cells.contains(n)) {
                    pair.push((n, side.opposite()));
                }
            }
        }
        if pair.len() != 2 || pair.iter().any(|&(c, _)| !g.placeable(c)) {
            return None;
        }
        ends.extend(pair);
    }
    Some((tiles, ends))
}

/// Plan a concourse level (see the module docs).
pub(super) fn plan(
    rng: &mut wfc::Rng,
    (w, h): (usize, usize),
    ask: &LevelAsk,
    vars: &[Var],
    c: &ConcourseAsk,
) -> Option<Level> {
    let LevelAsk {
        arrive,
        landing,
        more,
        extra_downs,
        last,
        min_path,
        under,
        rooms: room_ask,
        core,
        ..
    } = *ask;
    if last || ask.platform_cells > 0 || ask.ring_below || ask.ring_two_below {
        return None;
    }
    let is_under = |c: wfc::Cell| under.get(c.1 * w + c.0).copied().unwrap_or(false);
    let between = |r: &mut wfc::Rng, (lo, hi): (usize, usize)| lo + r.below(hi.max(lo) - lo + 1);
    let corridors: Vec<&Var> = vars
        .iter()
        .filter(|v| v.1.track == 0 && matches!(v.1.kind, Kind::Corridor(Some(_))))
        .collect();
    let piece = |r: &mut wfc::Rng, mask: u8| -> Option<Fill> {
        let fits: Vec<&&Var> = corridors.iter().filter(|v| v.0 == mask).collect();
        let total: f32 = fits.iter().map(|v| v.2).sum();
        let mut pick = r.roll() * total;
        for v in &fits {
            if pick < v.2 {
                return Some(v.1);
            }
            pick -= v.2;
        }
        fits.last().map(|v| v.1)
    };
    for attempt in 0..TRIES {
        let mut g = Grid {
            w,
            h,
            mask: vec![0; w * h],
            fixed: vec![None; w * h],
            corridor: vec![false; w * h],
            reserved: vec![false; w * h],
        };
        for (i, f) in g.fixed.iter_mut().enumerate() {
            if is_under((i % w, i / w)) {
                *f = Some(special(Kind::Under, wfc::Side::North).1);
            }
        }
        // Anchors: cells the corridors must reach.
        let mut anchors: Vec<wfc::Cell> = Vec::new();
        fn fix(
            g: &mut Grid,
            anchors: &mut Vec<wfc::Cell>,
            cell: wfc::Cell,
            kind: Kind,
            side: wfc::Side,
        ) -> bool {
            let Some(n) = g.nb(cell, side) else {
                return false;
            };
            if !g.free(cell) || !g.free(n) {
                return false;
            }
            let i = g.i(cell);
            g.fixed[i] = Some(special(kind, side).1);
            let j = g.i(n);
            g.reserved[j] = true;
            g.link(cell, side);
            anchors.push(n);
            true
        }
        // The way in and the other landings.
        let mut ok = true;
        match landing {
            Some(s) => ok &= fix(&mut g, &mut anchors, arrive, Kind::Landing, s),
            None => {
                let i = g.i(arrive);
                ok &= g.fixed[i].is_none();
                g.reserved[i] = true;
                anchors.push(arrive);
            }
        }
        for &(m, s) in more {
            ok &= fix(&mut g, &mut anchors, m, Kind::Landing, s);
        }
        if !ok {
            return None;
        }
        // Doorways into the floor of the room above that drops onto this level.
        let mut from_above = Vec::new();
        for &(x, s) in ask.low {
            let into_room = g.nb(x, s).is_some_and(&is_under);
            if into_room && g.placeable(x) {
                let i = g.i(x);
                g.reserved[i] = true;
                g.mask[i] |= bit(s);
                anchors.push(x);
                from_above.push((x, s));
            }
        }
        // The ways down, in the core (so they land on the next level),
        // the main one far enough from the way in.
        let far = |x: wfc::Cell| {
            attempt >= TRIES / 2
                || (arrive.0.abs_diff(x.0) + arrive.1.abs_diff(x.1)) as u32 * 2 >= min_path
        };
        let apart = |a: wfc::Cell, b: wfc::Cell| a.0.abs_diff(b.0) + a.1.abs_diff(b.1) >= 3;
        let mut spots: Vec<wfc::Cell> = core
            .cells()
            .filter(|&x| g.placeable(x) && apart(arrive, x) && more.iter().all(|m| apart(m.0, x)))
            .collect();
        for i in (1..spots.len()).rev() {
            spots.swap(i, rng.below(i + 1));
        }
        spots.sort_by_key(|&x| !far(x));
        let mut downs: Vec<(wfc::Cell, wfc::Side)> = Vec::new();
        for x in spots {
            if downs.len() > extra_downs {
                break;
            }
            if !downs.iter().all(|d| apart(d.0, x)) || !g.placeable(x) {
                continue;
            }
            // Entered from a free cell; the landing below opens onto the core.
            let mut sides: Vec<wfc::Side> = wfc::Side::ALL
                .into_iter()
                .filter(|&s| {
                    g.nb(x, s).is_some_and(|n| g.placeable(n))
                        && g.nb(x, s.opposite()).is_some_and(|n| core.contains(n))
                })
                .collect();
            if sides.is_empty() {
                continue;
            }
            let s = sides.swap_remove(rng.below(sides.len()));
            if fix(&mut g, &mut anchors, x, Kind::Stairs, s) {
                downs.push((x, s));
                // The cell the landing below opens onto stays clear of rooms.
                if let Some(n) = g.nb(x, s.opposite()) {
                    let j = g.i(n);
                    g.reserved[j] = true;
                }
            }
        }
        let Some(&(goal, goal_entry)) = downs.first() else {
            continue;
        };
        if downs.len() <= extra_downs {
            continue;
        }
        // Galleries.
        let mut galleries: Vec<TunnelTile> = Vec::new();
        let want = between(rng, c.galleries);
        for _ in 0..want * 12 {
            if galleries.len() >= want * 4 || c.shapes.is_empty() {
                break;
            }
            let shape = &c.shapes[rng.below(c.shapes.len())];
            let Some((tiles, ends)) = lay_gallery(rng, &g, shape) else {
                continue;
            };
            for tile in &tiles {
                for dz in 0..2 {
                    for dx in 0..2 {
                        let x = (tile.at.0 + dx, tile.at.1 + dz);
                        let i = g.i(x);
                        g.fixed[i] = Some(Fill {
                            mask: 0,
                            track: 0,
                            kind: Kind::Gallery,
                            turns: tile.turns,
                        });
                        for s in wfc::Side::ALL {
                            let (ox, oz) = s.offset();
                            let inside = (0..2).contains(&(dx as i32 + ox))
                                && (0..2).contains(&(dz as i32 + oz));
                            if inside || tile.mask() & bit(s) != 0 {
                                g.mask[i] |= bit(s);
                            }
                        }
                    }
                }
            }
            for (x, s) in ends {
                let i = g.i(x);
                g.mask[i] |= bit(s);
                g.reserved[i] = true;
                anchors.push(x);
            }
            galleries.extend(tiles);
            if galleries.len() >= want * 3 {
                break;
            }
        }
        // Rooms, a cell clear of the stairs and landings.
        let near_fixed = |g: &Grid, x: wfc::Cell| {
            (0..w * h).any(|i| {
                g.fixed[i].is_some_and(|f| matches!(f.kind, Kind::Stairs | Kind::Landing))
                    && (i % w).abs_diff(x.0) + (i / w).abs_diff(x.1) <= 1
            })
        };
        let count = between(rng, c.rooms);
        let rooms = wfc::rooms::place(rng, (w, h), count, &room_ask.sizes, 1, |x| {
            g.placeable(x) && !near_fixed(&g, x)
        });
        let mut placed: Vec<(wfc::Room, bool, usize)> = Vec::new();
        for room in rooms {
            let n = between(rng, (room_ask.doors.0.max(1), room_ask.doors.1));
            let mut doors = wfc::rooms::doors(rng, &room, (w, h), n, |x, s| {
                g.nb(x, s).is_some_and(|n| g.free(n))
            });
            // L-shaped: a corner block (half its size each way) goes back to corridor.
            let (a, b) = room.size;
            let mut cut: Vec<wfc::Cell> = Vec::new();
            if a >= 2 && b >= 2 && rng.roll() < room_ask.l_chance {
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
                cut = (z0..z0 + cb)
                    .flat_map(|z| (x0..x0 + ca).map(move |x| (x, z)))
                    .collect();
                doors.retain(|(x, _)| !cut.contains(x));
            }
            if doors.is_empty() {
                continue;
            }
            let ri = placed.len() as u8;
            for x in room.cells().filter(|x| !cut.contains(x)) {
                let mask = wfc::Side::ALL
                    .into_iter()
                    .filter(|&s| {
                        g.nb(x, s).is_none_or(|n| !cut.contains(&n))
                            && room.edge(x, s, &doors) != wfc::Edge::Wall
                    })
                    .fold(0, |m, s| m | bit(s));
                let i = g.i(x);
                g.fixed[i] = Some(Fill {
                    mask,
                    track: 0,
                    kind: Kind::Room(ri),
                    turns: 0,
                });
                g.mask[i] |= mask;
            }
            for &(x, s) in &doors {
                if let Some(n) = g.nb(x, s) {
                    let j = g.i(n);
                    g.mask[j] |= bit(s.opposite());
                    g.reserved[j] = true;
                    anchors.push(n);
                }
            }
            placed.push((room, cut.is_empty(), doors.len()));
        }
        // A two-storey room: the biggest whole one with two doors or more,
        // all of it over the level below.
        let on_below = |x: wfc::Cell| {
            ask.below.iter().all(|&(bw, bh)| {
                let frame = Level::empty(bw, bh);
                let me = Level::empty(w, h);
                frame.local(me.lattice(x)).is_some()
            })
        };
        let atrium = ask
            .two_storey
            .then(|| {
                placed
                    .iter()
                    .enumerate()
                    .filter(|(_, (r, whole, doors))| {
                        *whole
                            && *doors >= 2
                            && r.size.0 >= 2
                            && r.size.1 >= 2
                            && r.cells().all(on_below)
                    })
                    .max_by_key(|(_, (r, _, _))| r.size.0 * r.size.1)
                    .map(|(i, _)| i)
            })
            .flatten();
        let placed: Vec<wfc::Room> = placed.into_iter().map(|p| p.0).collect();
        // Corridors: each anchor in turn joins what's built, from the way in.
        let noise: Vec<f32> = (0..w * h).map(|_| rng.roll() * c.wander).collect();
        let mut joined = true;
        let (first, rest) = anchors.split_first()?;
        let mut rest = rest.to_vec();
        for i in (1..rest.len()).rev() {
            rest.swap(i, rng.below(i + 1));
        }
        // (The main way down last, so the way to it is its own.)
        rest.sort_by_key(|&a| g.nb(goal, goal_entry) == Some(a));
        let i = g.i(*first);
        g.corridor[i] = true;
        for a in rest {
            let ai = g.i(a);
            if g.corridor[ai] {
                continue;
            }
            let free_or_corridor = |x: wfc::Cell| g.free(x) || g.corridor[g.i(x)];
            let Some(path) = route(
                &g,
                a,
                |x| g.free(x) && !g.reserved[g.i(x)] || g.corridor[g.i(x)],
                |x| g.corridor[g.i(x)],
                |x| {
                    if g.corridor[g.i(x)] {
                        0.4
                    } else {
                        1.0 + noise[g.i(x)]
                    }
                },
                c.turn,
                w * h,
            )
            .filter(|p| p.iter().all(|&x| free_or_corridor(x))) else {
                joined = false;
                break;
            };
            for pair in path.windows(2) {
                let s = wfc::Side::ALL
                    .into_iter()
                    .find(|&s| g.nb(pair[0], s) == Some(pair[1]));
                if let Some(s) = s {
                    g.link(pair[0], s);
                }
            }
            for x in path {
                let j = g.i(x);
                g.corridor[j] = true;
            }
        }
        if !joined {
            continue;
        }
        // Loops: from a corridor, through free cells, to a corridor far
        // away along the corridors.
        let corridor_cells = |g: &Grid| -> Vec<wfc::Cell> {
            (0..w * h)
                .filter(|&i| g.corridor[i])
                .map(|i| (i % w, i / w))
                .collect()
        };
        let want = between(rng, c.loops);
        let mut made = 0;
        for _ in 0..want * 10 {
            if made >= want {
                break;
            }
            let cells = corridor_cells(&g);
            let Some(&a) = cells.get(rng.below(cells.len())) else {
                break;
            };
            let steps = g.steps(a);
            let path = route(
                &g,
                a,
                |x| g.free(x) && !g.reserved[g.i(x)],
                |x| {
                    g.corridor[g.i(x)]
                        && steps[g.i(x)].is_some_and(|d| d >= c.loop_apart)
                        && wfc::Side::ALL.into_iter().all(|s| g.nb(x, s) != Some(a))
                },
                |x| 1.0 + noise[g.i(x)],
                c.turn,
                c.loop_len.max(2),
            );
            let Some(path) = path.filter(|p| p.len() >= 3) else {
                continue;
            };
            for pair in path.windows(2) {
                if let Some(s) = wfc::Side::ALL
                    .into_iter()
                    .find(|&s| g.nb(pair[0], s) == Some(pair[1]))
                {
                    g.link(pair[0], s);
                }
            }
            for x in path {
                let j = g.i(x);
                g.corridor[j] = true;
            }
            made += 1;
        }
        // Dead ends.
        for _ in 0..between(rng, c.spurs) {
            let cells = corridor_cells(&g);
            let Some(&start) = cells.get(rng.below(cells.len())) else {
                break;
            };
            let mut at = start;
            let mut d = wfc::Side::ALL[rng.below(4)];
            for _ in 0..1 + rng.below(c.spur_len.max(1)) {
                let Some(n) = g.nb(at, d).filter(|&n| g.placeable(n)) else {
                    break;
                };
                g.link(at, d);
                let j = g.i(n);
                g.corridor[j] = true;
                at = n;
                if rng.roll() < 0.3 {
                    d = wfc::Side::ALL[(d.index() + if rng.roll() < 0.5 { 1 } else { 3 }) % 4];
                }
            }
        }
        // The cells.
        let solid = Fill {
            mask: 0,
            track: 0,
            kind: Kind::Corridor(None),
            turns: 0,
        };
        let mut cells = Vec::with_capacity(w * h);
        let mut fitted = true;
        for i in 0..w * h {
            let f = if let Some(f) = g.fixed[i] {
                Fill {
                    mask: if f.kind == Kind::Under { 0 } else { g.mask[i] },
                    ..f
                }
            } else if g.corridor[i] {
                match piece(rng, g.mask[i]) {
                    Some(f) => f,
                    None => {
                        fitted = false;
                        solid
                    }
                }
            } else {
                solid
            };
            cells.push(f);
        }
        if !fitted {
            continue;
        }
        let mut level = Level {
            width: w,
            depth: h,
            cells,
            arrive,
            goal,
            goal_entry,
            rooms: placed.clone(),
            room_meta: (0..placed.len())
                .map(|i| {
                    if Some(i) == atrium {
                        Meta::Atrium
                    } else {
                        Meta::Warren
                    }
                })
                .collect(),
            from_above: from_above.clone(),
            kind: LevelKind::Concourse,
            galleries,
            below: ask.below.to_vec(),
            ..Level::empty(w, h)
        };
        let d = level.distances();
        let need = if attempt < TRIES / 2 { min_path } else { 1 };
        let reached = (0..w * h).all(|i| level.cells[i].mask == 0 || d[i].is_some());
        if !reached || d[goal.1 * w + goal.0].is_none_or(|d| d < need) {
            continue;
        }
        level.path = path_to(&level, &d);
        if level.path.is_empty() {
            continue;
        }
        return Some(level);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn galleries_fit_on_an_empty_grid() {
        let (w, h) = (10, 10);
        let g = Grid {
            w,
            h,
            mask: vec![0; w * h],
            fixed: vec![None; w * h],
            corridor: vec![false; w * h],
            reserved: vec![false; w * h],
        };
        let mut rng = wfc::Rng::new(3);
        let laid = (0..200)
            .filter(|_| lay_gallery(&mut rng, &g, "SCS").is_some())
            .count();
        assert_eq!(laid, 200);
    }
}
