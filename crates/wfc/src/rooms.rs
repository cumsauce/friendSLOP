//! Rooms: rectangles of cells bigger than one tile, stamped on a grid before
//! WFC fills the rest. A room's cells open onto each other, its outside edge
//! is wall except at its doors, and the solver is left to run corridors up to
//! the doors. The caller turns each room cell into one pinned tile (see
//! [`Room::edge`]) so the rest of the grid has to meet it. No game code.

use crate::{Cell, Rng, Side, neighbour};

/// A room: its top-left (north-west) cell and its size in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Room {
    pub at: Cell,
    pub size: (usize, usize),
}

/// What one side of a room cell is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// The next cell is the same room: no wall.
    Inside,
    /// The room's outside wall.
    Wall,
    /// A door in the outside wall.
    Door,
}

impl Room {
    pub fn contains(&self, c: Cell) -> bool {
        (self.at.0..self.at.0 + self.size.0).contains(&c.0)
            && (self.at.1..self.at.1 + self.size.1).contains(&c.1)
    }

    /// Every cell, row by row.
    pub fn cells(&self) -> impl Iterator<Item = Cell> + '_ {
        (0..self.size.1)
            .flat_map(move |y| (0..self.size.0).map(move |x| (self.at.0 + x, self.at.1 + y)))
    }

    /// Does the room overlap `other`, or touch it (`gap` cells apart or less)?
    pub fn near(&self, other: &Room, gap: usize) -> bool {
        let apart = |a0: usize, a1: usize, b0: usize, b1: usize| a1 + gap <= b0 || b1 + gap <= a0;
        !(apart(
            self.at.0,
            self.at.0 + self.size.0,
            other.at.0,
            other.at.0 + other.size.0,
        ) || apart(
            self.at.1,
            self.at.1 + self.size.1,
            other.at.1,
            other.at.1 + other.size.1,
        ))
    }

    /// The outside sides of its cells: (cell, side) facing out of the room,
    /// on a `width × height` grid only (sides off the grid are left out).
    pub fn border(&self, width: usize, height: usize) -> Vec<(Cell, Side)> {
        self.cells()
            .flat_map(|c| Side::ALL.map(|s| (c, s)))
            .filter(|&(c, s)| neighbour(width, height, c, s).is_some_and(|n| !self.contains(n)))
            .collect()
    }

    /// What side `s` of room cell `c` is, given the room's doors.
    pub fn edge(&self, c: Cell, s: Side, doors: &[(Cell, Side)]) -> Edge {
        let (dx, dy) = s.offset();
        let (nx, ny) = (c.0 as i64 + dx as i64, c.1 as i64 + dy as i64);
        if nx >= 0 && ny >= 0 && self.contains((nx as usize, ny as usize)) {
            Edge::Inside
        } else if doors.contains(&(c, s)) {
            Edge::Door
        } else {
            Edge::Wall
        }
    }

    /// The room's inside corners: cells whose south-east corner has room on
    /// all four sides (where a column can stand).
    pub fn inner_corners(&self) -> impl Iterator<Item = Cell> + '_ {
        self.cells()
            .filter(|c| c.0 + 1 < self.at.0 + self.size.0 && c.1 + 1 < self.at.1 + self.size.1)
    }
}

/// Up to `count` rooms of the given `sizes` (each tried both ways round) on a
/// `width × height` grid, each wholly on cells `free` allows and at least
/// `gap` cells from every other. Seeded; fewer if they don't fit.
pub fn place(
    rng: &mut Rng,
    (width, height): (usize, usize),
    count: usize,
    sizes: &[(usize, usize)],
    gap: usize,
    free: impl Fn(Cell) -> bool,
) -> Vec<Room> {
    let mut out: Vec<Room> = Vec::new();
    if sizes.is_empty() {
        return out;
    }
    for _ in 0..count * 24 {
        if out.len() >= count {
            break;
        }
        let (a, b) = sizes[rng.below(sizes.len())];
        let size = if rng.roll() < 0.5 { (a, b) } else { (b, a) };
        if size.0 == 0 || size.1 == 0 || size.0 > width || size.1 > height {
            continue;
        }
        let at = (
            rng.below(width - size.0 + 1),
            rng.below(height - size.1 + 1),
        );
        let room = Room { at, size };
        if room.cells().all(&free) && out.iter().all(|o| !o.near(&room, gap)) {
            out.push(room);
        }
    }
    out
}

/// `count` doors (fewer if there aren't enough places) on the room's outside
/// wall, among the sides `ok` allows, no two on the same cell. Seeded.
pub fn doors(
    rng: &mut Rng,
    room: &Room,
    (width, height): (usize, usize),
    count: usize,
    ok: impl Fn(Cell, Side) -> bool,
) -> Vec<(Cell, Side)> {
    let mut sides: Vec<(Cell, Side)> = room
        .border(width, height)
        .into_iter()
        .filter(|&(c, s)| ok(c, s))
        .collect();
    let mut out = Vec::new();
    while out.len() < count && !sides.is_empty() {
        let (c, s) = sides.swap_remove(rng.below(sides.len()));
        out.push((c, s));
        sides.retain(|&(c2, _)| c2 != c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rooms_fit_apart_and_on_free_cells() {
        for seed in 0..50 {
            let mut rng = Rng::new(seed);
            let rooms = place(&mut rng, (8, 8), 3, &[(2, 2), (3, 2)], 1, |c| c != (0, 0));
            assert!(!rooms.is_empty(), "seed {seed}");
            for (i, r) in rooms.iter().enumerate() {
                assert!(r.at.0 + r.size.0 <= 8 && r.at.1 + r.size.1 <= 8);
                assert!(!r.contains((0, 0)));
                for o in &rooms[i + 1..] {
                    assert!(!r.near(o, 1), "{r:?} {o:?}");
                }
            }
        }
        // Nothing fits on a grid too small.
        assert!(place(&mut Rng::new(1), (1, 1), 2, &[(2, 2)], 0, |_| true).is_empty());
    }

    #[test]
    fn edges_doors_and_corners() {
        let r = Room {
            at: (1, 1),
            size: (3, 2),
        };
        assert_eq!(r.cells().count(), 6);
        // 3 + 3 + 2 + 2 outside sides, all on a big grid.
        assert_eq!(r.border(10, 10).len(), 10);
        // At the grid's edge, the sides facing off it are left out.
        let edge = Room {
            at: (0, 0),
            size: (2, 2),
        };
        assert_eq!(edge.border(5, 5).len(), 4);
        let mut rng = Rng::new(3);
        let d = doors(&mut rng, &r, (10, 10), 2, |_, s| s != Side::North);
        assert_eq!(d.len(), 2);
        assert_ne!(d[0].0, d[1].0);
        assert!(d.iter().all(|&(c, s)| s != Side::North && r.contains(c)));
        assert_eq!(r.edge((1, 1), Side::East, &d), Edge::Inside);
        assert_eq!(r.edge(d[0].0, d[0].1, &d), Edge::Door);
        assert_eq!(r.edge((1, 1), Side::North, &d), Edge::Wall);
        let corners: Vec<_> = r.inner_corners().collect();
        assert_eq!(corners, [(1, 1), (2, 1)]);
    }
}
