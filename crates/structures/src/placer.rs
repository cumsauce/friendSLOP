//! Seeded furniture and detail placement for one room: a pure function from
//! a [`Room`], a [`Theme`] and a seed to a list of [`Placement`]s, and
//! [`apply`] to emit them into a [`Kit`].
//!
//! Passes, in order:
//! 1. wall and corner props along the walls, skipping door and window spans;
//! 2. centrepieces (and ceiling props) on free cells;
//! 3. small props into the surface sockets of what was placed;
//! 4. scattered detail.
//!
//! A 0.25 m occupancy grid per [`Layer`] keeps props apart, reserved spots
//! (gameplay pins) are kept clear first, and every solid prop is checked
//! against walkability: each door must still reach every other door and the
//! room centre through a corridor at least three cells wide. Props that
//! would break it are dropped. No game code.

use std::collections::VecDeque;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::props::{Anchor, Layer, Params, PropId, Rng};
use crate::{Kit, Palette};

/// Grid cell size (m).
pub const CELL: f32 = 0.25;
/// Gap between a wall-anchored prop and the wall.
const GAP: f32 = 0.02;
/// Floor kept clear beside a door along its wall, and how deep into the room.
const DOOR_SIDE: f32 = 0.3;
const DOOR_DEPTH: f32 = 1.0;

/// A room's interior: floor at `min.y`, ceiling at `max.y`.
#[derive(Clone, Debug, Default)]
pub struct Room {
    pub min: Vec3,
    pub max: Vec3,
    /// Door centres on the wall line (floor height) and their widths.
    pub doors: Vec<(Vec3, f32)>,
    /// Window bottom-centres on the wall line (y = sill) and their widths.
    pub windows: Vec<(Vec3, f32)>,
    /// Gameplay pins (spawns, goals, pickups) to keep clear: xz centre, radius.
    pub reserved: Vec<(Vec2, f32)>,
}

/// One weighted line of a theme: place between `min` and `max` of `prop`.
#[derive(Clone, Copy, Debug)]
pub struct Entry {
    pub prop: PropId,
    pub weight: f32,
    pub min: u32,
    pub max: u32,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: &'static str,
    pub entries: Vec<Entry>,
}

const fn e(prop: PropId, weight: f32, min: u32, max: u32) -> Entry {
    Entry {
        prop,
        weight,
        min,
        max,
    }
}

impl Theme {
    pub const NAMES: [&'static str; 6] =
        ["bedroom", "storage", "kitchen", "shop", "hallway", "sewer"];

    /// A built-in theme by name.
    pub fn named(name: &str) -> Option<Theme> {
        use PropId::*;
        let (name, entries) = match name {
            "bedroom" => (
                "bedroom",
                vec![
                    e(Bed, 3.0, 1, 1),
                    e(Cupboard, 1.0, 0, 1),
                    // Somewhere for the books.
                    e(Shelf, 1.0, 1, 1),
                    e(Chair, 1.0, 0, 1),
                    e(Rug, 2.0, 0, 1),
                    e(WallLamp, 2.0, 1, 2),
                    e(Books, 2.0, 1, 3),
                ],
            ),
            "storage" => (
                "storage",
                vec![
                    e(Crates, 4.0, 2, 6),
                    e(Barrel, 3.0, 1, 4),
                    e(Shelf, 2.0, 1, 2),
                    e(Sack, 2.0, 1, 3),
                    e(HangingLamp, 1.0, 0, 1),
                    e(Cobweb, 1.0, 0, 2),
                    e(Debris, 1.0, 0, 2),
                    e(Bucket, 1.0, 0, 1),
                ],
            ),
            "kitchen" => (
                "kitchen",
                vec![
                    e(Counter, 3.0, 1, 2),
                    e(Table, 3.0, 1, 1),
                    e(Chair, 3.0, 2, 4),
                    e(Cupboard, 2.0, 0, 1),
                    e(Barrel, 1.0, 0, 1),
                    e(Sack, 1.0, 0, 2),
                    e(HangingLamp, 2.0, 1, 1),
                    e(CupPlate, 3.0, 1, 4),
                    e(Stool, 1.0, 0, 1),
                    e(Bucket, 1.0, 0, 1),
                ],
            ),
            "shop" => (
                "shop",
                vec![
                    e(Counter, 3.0, 1, 1),
                    e(Shelf, 3.0, 2, 3),
                    e(Crates, 2.0, 1, 3),
                    e(Sack, 2.0, 0, 2),
                    e(Stool, 1.0, 0, 1),
                    e(Books, 2.0, 1, 3),
                    e(WallLamp, 2.0, 1, 2),
                    e(HangingLamp, 1.0, 0, 1),
                ],
            ),
            "hallway" => (
                "hallway",
                vec![
                    e(Rug, 2.0, 0, 2),
                    e(WallLamp, 3.0, 1, 3),
                    e(Shelf, 1.0, 0, 1),
                    e(Debris, 1.0, 0, 1),
                ],
            ),
            "sewer" => (
                "sewer",
                vec![
                    e(PipeRun, 3.0, 1, 3),
                    e(Barrel, 2.0, 0, 2),
                    e(Crates, 1.0, 0, 2),
                    e(Debris, 3.0, 1, 4),
                    e(Cobweb, 2.0, 1, 3),
                    e(Bucket, 1.0, 0, 2),
                    e(Stool, 1.0, 0, 1),
                    e(WallLamp, 1.0, 0, 1),
                ],
            ),
            _ => return None,
        };
        Some(Theme { name, entries })
    }

    pub fn builtin() -> Vec<Theme> {
        Theme::NAMES
            .iter()
            .filter_map(|n| Theme::named(n))
            .collect()
    }
}

/// One placed prop. `at` is its base centre in world space, `turns` quarter
/// turns about y (0: front faces +z, 1: +x, 2: -z, 3: -x), `min`/`max` its
/// world box, `on` the index of the placement it stands on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub prop: PropId,
    pub at: Vec3,
    pub turns: i32,
    pub params: Params,
    pub min: Vec3,
    pub max: Vec3,
    pub on: Option<usize>,
}

/// Emit placements into a kit. Each starts its own [`Kit::place`] frame (so
/// the kit's frame is left at the last prop afterwards).
pub fn apply<M: Palette>(kit: &mut Kit<M>, placements: &[Placement]) {
    for p in placements {
        kit.place(p.at, p.turns);
        p.prop.emit(kit, &p.params);
    }
}

fn turn(turns: i32) -> Quat {
    Quat::from_rotation_y(FRAC_PI_2 * turns as f32)
}

/// World box of a prop at `at`, turned.
fn world_box(prop: PropId, p: &Params, at: Vec3, turns: i32) -> (Vec3, Vec3) {
    let (lo, hi) = prop.aabb(p);
    let r = turn(turns);
    let (mut a, mut b) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for i in 0..8 {
        let c = Vec3::new(
            if i & 1 == 0 { lo.x } else { hi.x },
            if i & 2 == 0 { lo.y } else { hi.y },
            if i & 4 == 0 { lo.z } else { hi.z },
        );
        let w = at + r * c;
        a = a.min(w);
        b = b.max(w);
    }
    // Snap away float noise from the rotation.
    let snap = |v: Vec3| (v * 1e4).round() / 1e4;
    (snap(a), snap(b))
}

/// Strict 3D box overlap (touching is fine).
pub fn overlaps(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> bool {
    let e = 1e-3;
    a.0.x < b.1.x - e
        && b.0.x < a.1.x - e
        && a.0.y < b.1.y - e
        && b.0.y < a.1.y - e
        && a.0.z < b.1.z - e
        && b.0.z < a.1.z - e
}

/// Distance from a point to a box in xz.
fn box_dist(a: Vec2, b: Vec2, p: Vec2) -> f32 {
    (p.clamp(a, b) - p).length()
}

/// The walls of the room: (side, inward normal, quarter turns facing in).
fn sides() -> [(usize, Vec2, i32); 4] {
    [
        (0, Vec2::Y, 0),  // north wall (z = min), facing +z
        (1, -Vec2::Y, 2), // south wall (z = max), facing -z
        (2, Vec2::X, 1),  // west wall (x = min), facing +x
        (3, -Vec2::X, 3), // east wall (x = max), facing -x
    ]
}

/// Which wall a point on the wall line is on, and its coordinate along it.
fn wall_of(room: &Room, p: Vec3) -> (usize, f32) {
    let d = [
        (p.z - room.min.z).abs(),
        (p.z - room.max.z).abs(),
        (p.x - room.min.x).abs(),
        (p.x - room.max.x).abs(),
    ];
    let mut best = 0;
    for i in 1..4 {
        if d[i] < d[best] {
            best = i;
        }
    }
    (best, if best < 2 { p.x } else { p.z })
}

#[derive(Clone)]
struct Grid {
    o: Vec2,
    nx: usize,
    nz: usize,
    solid: Vec<bool>,
    floor: Vec<bool>,
    air: Vec<bool>,
    /// No solid props (door approaches, clearances).
    keep: Vec<bool>,
    /// No floor-standing props at all (gameplay pins).
    reserved: Vec<bool>,
    starts: Vec<usize>,
    centre: Vec2,
}

impl Grid {
    fn new(room: &Room) -> Grid {
        let o = Vec2::new(room.min.x, room.min.z);
        let size = Vec2::new(room.max.x - room.min.x, room.max.z - room.min.z).max(Vec2::ZERO);
        let nx = ((size.x / CELL - 1e-3).ceil() as usize).max(1);
        let nz = ((size.y / CELL - 1e-3).ceil() as usize).max(1);
        let n = nx * nz;
        let mut g = Grid {
            o,
            nx,
            nz,
            solid: vec![false; n],
            floor: vec![false; n],
            air: vec![false; n],
            keep: vec![false; n],
            reserved: vec![false; n],
            starts: Vec::new(),
            centre: o + size / 2.0,
        };
        for &(c, r) in &room.reserved {
            for i in 0..n {
                let (a, b) = g.cell_box(i);
                if box_dist(a, b, c) < r {
                    g.reserved[i] = true;
                }
            }
        }
        for &(p, w) in &room.doors {
            let (side, _) = wall_of(room, p);
            let (_, normal, _) = sides()[side];
            let half = w / 2.0 + DOOR_SIDE;
            let wall = Vec2::new(p.x, p.z);
            let along = Vec2::new(normal.y.abs(), normal.x.abs());
            let a = wall - along * half;
            let b = wall + along * half + normal * DOOR_DEPTH;
            let (lo, hi) = (a.min(b), a.max(b));
            for i in g.cells(lo, hi) {
                g.keep[i] = true;
            }
            let q = Vec2::new(p.x, p.z).clamp(o + 0.01, o + size - 0.01);
            g.starts.push(g.index(q));
        }
        g
    }

    fn index(&self, p: Vec2) -> usize {
        let i = (((p.x - self.o.x) / CELL) as usize).min(self.nx - 1);
        let j = (((p.y - self.o.y) / CELL) as usize).min(self.nz - 1);
        j * self.nx + i
    }

    fn cell_box(&self, i: usize) -> (Vec2, Vec2) {
        let (x, z) = ((i % self.nx) as f32, (i / self.nx) as f32);
        let a = self.o + Vec2::new(x, z) * CELL;
        (a, a + Vec2::splat(CELL))
    }

    /// Cells a box in xz touches (with a little slack so touching boxes share none).
    fn cells(&self, a: Vec2, b: Vec2) -> Vec<usize> {
        let lo = ((a - self.o) / CELL + 1e-3).floor().max(Vec2::ZERO);
        let hi = ((b - self.o) / CELL - 1e-3).ceil();
        let (i0, j0) = (lo.x as usize, lo.y as usize);
        let (i1, j1) = (
            (hi.x.max(0.0) as usize).min(self.nx),
            (hi.y.max(0.0) as usize).min(self.nz),
        );
        let mut out = Vec::new();
        for j in j0..j1 {
            for i in i0..i1 {
                out.push(j * self.nx + i);
            }
        }
        out
    }

    /// A cell a body can stand in: it and its eight neighbours are free of solids.
    fn walkable(&self, i: usize) -> bool {
        let (x, z) = ((i % self.nx) as i64, (i / self.nx) as i64);
        for dz in -1..=1 {
            for dx in -1..=1 {
                let (u, v) = (x + dx, z + dz);
                if u < 0 || v < 0 || u >= self.nx as i64 || v >= self.nz as i64 {
                    continue;
                }
                if self.solid[v as usize * self.nx + u as usize] {
                    return false;
                }
            }
        }
        true
    }

    /// Every door reaches every other door and the room centre.
    fn connected(&self) -> bool {
        let Some(&first) = self.starts.first() else {
            return true;
        };
        let n = self.nx * self.nz;
        let walk: Vec<bool> = (0..n).map(|i| self.walkable(i)).collect();
        // The centre: the walkable cell nearest the middle of the room.
        let mut centre = None;
        let mut best = f32::MAX;
        for (i, &w) in walk.iter().enumerate() {
            if !w {
                continue;
            }
            let (a, b) = self.cell_box(i);
            let d = ((a + b) / 2.0).distance_squared(self.centre);
            if d < best - 1e-6 {
                best = d;
                centre = Some(i);
            }
        }
        let Some(centre) = centre else {
            return false;
        };
        if !walk[first] {
            return false;
        }
        let mut seen = vec![false; n];
        let mut q = VecDeque::from([first]);
        seen[first] = true;
        while let Some(i) = q.pop_front() {
            let (x, z) = (i % self.nx, i / self.nx);
            let mut next = |j: usize| {
                if walk[j] && !seen[j] {
                    seen[j] = true;
                    q.push_back(j);
                }
            };
            if x > 0 {
                next(i - 1);
            }
            if x + 1 < self.nx {
                next(i + 1);
            }
            if z > 0 {
                next(i - self.nx);
            }
            if z + 1 < self.nz {
                next(i + self.nx);
            }
        }
        seen[centre] && self.starts.iter().all(|&s| seen[s])
    }
}

/// Whether every door can still reach every other door and the room centre
/// with these placements (solid-layer, floor-standing ones block).
pub fn doors_reachable(room: &Room, placements: &[Placement]) -> bool {
    let mut g = Grid::new(room);
    for p in placements {
        if p.prop.spec().layer == Layer::Solid && p.on.is_none() {
            let a = Vec2::new(p.min.x, p.min.z);
            let b = Vec2::new(p.max.x, p.max.z);
            for i in g.cells(a, b) {
                g.solid[i] = true;
            }
        }
    }
    g.connected()
}

struct Slot {
    host: usize,
    centre: Vec3,
    size: Vec2,
    used: Vec<(Vec2, Vec2)>,
}

struct Placer<'a> {
    room: &'a Room,
    grid: Grid,
    out: Vec<Placement>,
    slots: Vec<Slot>,
    reach: f32,
}

impl Placer<'_> {
    fn inside(&self, a: Vec3, b: Vec3) -> bool {
        let e = 1e-3;
        let r = self.room;
        a.x >= r.min.x - e
            && a.z >= r.min.z - e
            && b.x <= r.max.x + e
            && b.z <= r.max.z + e
            && a.y >= r.min.y - e
            && b.y <= r.max.y - 0.01
    }

    /// Nothing non-floor in the way in 3D (except `host`).
    fn clear_3d(&self, bx: (Vec3, Vec3), host: Option<usize>) -> bool {
        self.out.iter().enumerate().all(|(i, p)| {
            Some(i) == host || p.prop.spec().layer == Layer::Floor || !overlaps(bx, (p.min, p.max))
        })
    }

    /// Try to stand `prop` at `at`. `front` is the clearance zone to keep.
    fn try_floor(
        &mut self,
        prop: PropId,
        params: Params,
        at: Vec3,
        turns: i32,
        front: Option<(Vec2, Vec2)>,
    ) -> bool {
        let spec = prop.spec();
        let (a, b) = world_box(prop, &params, at, turns);
        if !self.inside(a, b) {
            return false;
        }
        let cells = self.grid.cells(Vec2::new(a.x, a.z), Vec2::new(b.x, b.z));
        let g = &self.grid;
        let ok = match spec.layer {
            Layer::Solid => cells
                .iter()
                .all(|&i| !g.solid[i] && !g.keep[i] && !g.reserved[i]),
            Layer::Floor => cells.iter().all(|&i| !g.floor[i] && !g.reserved[i]),
            Layer::Air => cells.iter().all(|&i| !g.air[i]),
        };
        if !ok || !self.clear_3d((a, b), None) {
            return false;
        }
        let mut zone = Vec::new();
        if let Some((za, zb)) = front {
            let r = self.room;
            let e = 1e-3;
            if za.x < r.min.x - e || za.y < r.min.z - e || zb.x > r.max.x + e || zb.y > r.max.z + e
            {
                return false;
            }
            zone = g.cells(za, zb);
            if zone.iter().any(|&i| g.solid[i]) {
                return false;
            }
        }
        if spec.layer == Layer::Solid {
            for &i in &cells {
                self.grid.solid[i] = true;
            }
            if !self.grid.connected() {
                for &i in &cells {
                    self.grid.solid[i] = false;
                }
                return false;
            }
        }
        let layer = match spec.layer {
            Layer::Solid => None,
            Layer::Floor => Some(&mut self.grid.floor),
            Layer::Air => Some(&mut self.grid.air),
        };
        if let Some(layer) = layer {
            for &i in &cells {
                layer[i] = true;
            }
        }
        for i in zone {
            self.grid.keep[i] = true;
        }
        let index = self.out.len();
        let r = turn(turns);
        for s in prop.sockets(&params) {
            let size = if turns % 2 == 0 {
                s.size
            } else {
                Vec2::new(s.size.y, s.size.x)
            };
            self.slots.push(Slot {
                host: index,
                centre: at + r * s.centre,
                size,
                used: Vec::new(),
            });
        }
        self.out.push(Placement {
            prop,
            at,
            turns,
            params,
            min: a,
            max: b,
            on: None,
        });
        true
    }

    /// Pass 1: against a wall (or in a corner), skipping doors and windows.
    fn wall(&mut self, prop: PropId, params: Params, rng: &mut Rng) -> bool {
        let spec = prop.spec();
        let (w, d) = (spec.footprint.x, spec.footprint.y);
        let r = self.room;
        let mut cands = Vec::new();
        for (side, normal, turns) in sides() {
            let (lo, hi) = if side < 2 {
                (r.min.x, r.max.x)
            } else {
                (r.min.z, r.max.z)
            };
            let (s0, s1) = (lo + w / 2.0, hi - w / 2.0);
            if s1 < s0 - 1e-4 {
                continue;
            }
            let along: Vec<f32> = if spec.anchor == Anchor::Corner {
                vec![s0, s1]
            } else {
                let n = ((s1 - s0) / CELL).floor() as usize;
                (0..=n).map(|k| s0 + k as f32 * CELL).collect()
            };
            for s in along {
                cands.push((side, normal, turns, s));
            }
        }
        rng.shuffle(&mut cands);
        let top = prop.aabb(&params).1.y + r.min.y;
        for (side, normal, turns, s) in cands {
            let (sa, sb) = (s - w / 2.0, s + w / 2.0);
            let hits = |list: &[(Vec3, f32)], margin: f32, sill: bool| {
                list.iter().any(|&(p, pw)| {
                    let (ws, c) = wall_of(r, p);
                    ws == side
                        && sa < c + pw / 2.0 + margin
                        && c - pw / 2.0 - margin < sb
                        && (!sill || top > p.y - 0.02)
                })
            };
            if hits(&r.doors, DOOR_SIDE, false) || hits(&r.windows, 0.1, true) {
                continue;
            }
            let wall = match side {
                0 => r.min.z,
                1 => r.max.z,
                2 => r.min.x,
                _ => r.max.x,
            };
            let depth = d / 2.0 + GAP;
            let xz = if side < 2 {
                Vec2::new(s, wall) + normal * depth
            } else {
                Vec2::new(wall, s) + normal * depth
            };
            let at = Vec3::new(xz.x, r.min.y, xz.y);
            let front = (spec.layer == Layer::Solid && spec.clearance > 0.0).then(|| {
                let along = Vec2::new(normal.y.abs(), normal.x.abs());
                let face = xz + normal * (d / 2.0);
                let a = face - along * (w / 2.0);
                let b = face + along * (w / 2.0) + normal * spec.clearance;
                (a.min(b), a.max(b))
            });
            if self.try_floor(prop, params, at, turns, front) {
                return true;
            }
        }
        false
    }

    /// Passes 2 and 4: anywhere on the floor (or the ceiling).
    fn free(&mut self, prop: PropId, params: Params, rng: &mut Rng) -> bool {
        let spec = prop.spec();
        let r = self.room;
        let margin = if spec.anchor == Anchor::Ceiling {
            0.3
        } else {
            0.0
        };
        let mut cands = Vec::new();
        for turns in 0..4 {
            let f = if turns % 2 == 0 {
                spec.footprint
            } else {
                Vec2::new(spec.footprint.y, spec.footprint.x)
            };
            let lo = Vec2::new(r.min.x, r.min.z) + f / 2.0 + margin;
            let hi = Vec2::new(r.max.x, r.max.z) - f / 2.0 - margin;
            if hi.x < lo.x || hi.y < lo.y {
                continue;
            }
            let n = ((hi - lo) / CELL).floor();
            for j in 0..=n.y as usize {
                for i in 0..=n.x as usize {
                    cands.push((lo + Vec2::new(i as f32, j as f32) * CELL, turns, f));
                }
            }
        }
        rng.shuffle(&mut cands);
        cands.truncate(400);
        for (xz, turns, f) in cands {
            let at = Vec3::new(xz.x, r.min.y, xz.y);
            let front = (spec.layer == Layer::Solid && spec.clearance > 0.0).then(|| {
                let c = spec.clearance;
                (xz - f / 2.0 - c, xz + f / 2.0 + c)
            });
            if self.try_floor(prop, params, at, turns, front) {
                return true;
            }
        }
        false
    }

    /// Pass 3: on a surface socket.
    fn surface(&mut self, prop: PropId, params: Params, rng: &mut Rng) -> bool {
        let mut order: Vec<usize> = (0..self.slots.len()).collect();
        rng.shuffle(&mut order);
        for k in order {
            for _ in 0..12 {
                let turns = rng.below(4) as i32;
                let f = prop.spec().footprint;
                let f = if turns % 2 == 0 {
                    f
                } else {
                    Vec2::new(f.y, f.x)
                };
                let slot = &self.slots[k];
                let room = (slot.size - f) / 2.0;
                if room.x < 0.0 || room.y < 0.0 {
                    break;
                }
                let off = Vec2::new(rng.range(-room.x, room.x), rng.range(-room.y, room.y));
                let c = Vec2::new(slot.centre.x, slot.centre.z) + off;
                let (a2, b2) = (c - f / 2.0, c + f / 2.0);
                if slot
                    .used
                    .iter()
                    .any(|&(ua, ub)| a2.x < ub.x && ua.x < b2.x && a2.y < ub.y && ua.y < b2.y)
                {
                    continue;
                }
                let at = Vec3::new(c.x, slot.centre.y, c.y);
                let (a, b) = world_box(prop, &params, at, turns);
                let host = slot.host;
                if !self.inside(a, b) || !self.clear_3d((a, b), Some(host)) {
                    continue;
                }
                self.slots[k].used.push((a2, b2));
                self.out.push(Placement {
                    prop,
                    at,
                    turns,
                    params,
                    min: a,
                    max: b,
                    on: Some(host),
                });
                return true;
            }
        }
        false
    }
}

fn pass_of(prop: PropId) -> usize {
    let s = prop.spec();
    match s.anchor {
        Anchor::OnSurface => 3,
        _ if s.detail => 4,
        Anchor::Wall | Anchor::Corner => 1,
        Anchor::Centre | Anchor::Ceiling => 2,
    }
}

/// Lay out a room. Deterministic per `seed`.
pub fn place(room: &Room, theme: &Theme, seed: u64) -> Vec<Placement> {
    let mut rng = Rng::new(seed);
    let mut p = Placer {
        room,
        grid: Grid::new(room),
        out: Vec::new(),
        slots: Vec::new(),
        reach: room.max.y - room.min.y,
    };
    // Roll every instance up front: required copies first, then extras in a
    // weighted random order.
    let mut required = Vec::new();
    let mut extra = Vec::new();
    for e in &theme.entries {
        let hi = e.max.max(e.min);
        let count = e.min + rng.below((hi - e.min + 1) as usize) as u32;
        for k in 0..count {
            if k < e.min {
                required.push(e.prop);
            } else {
                let key = rng.f32().max(1e-6).powf(1.0 / e.weight.max(1e-3));
                extra.push((key, e.prop));
            }
        }
    }
    extra.sort_by(|a, b| b.0.total_cmp(&a.0));
    let order: Vec<PropId> = required
        .into_iter()
        .chain(extra.into_iter().map(|(_, prop)| prop))
        .collect();
    for pass in 1..=4 {
        for &prop in order.iter().filter(|&&prop| pass_of(prop) == pass) {
            let params = Params::roll(rng.next_u64(), p.reach);
            let mut sub = Rng::new(rng.next_u64());
            match pass {
                1 => p.wall(prop, params, &mut sub),
                3 => p.surface(prop, params, &mut sub),
                _ => p.free(prop, params, &mut sub),
            };
        }
    }
    p.out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::M;

    fn room(w: f32, d: f32) -> Room {
        Room {
            min: Vec3::new(-w / 2.0, 0.0, -d / 2.0),
            max: Vec3::new(w / 2.0, 2.8, d / 2.0),
            doors: vec![
                (Vec3::new(0.0, 0.0, d / 2.0), 1.4),
                (Vec3::new(-w / 2.0, 0.0, 0.5), 1.4),
            ],
            windows: vec![(Vec3::new(1.0, 1.0, -d / 2.0), 1.0)],
            reserved: vec![(Vec2::new(1.0, 1.0), 0.5)],
        }
    }

    fn all() -> impl Iterator<Item = (Theme, u64)> {
        Theme::builtin()
            .into_iter()
            .flat_map(|t| (0..25u64).map(move |s| (t.clone(), s)))
    }

    #[test]
    fn no_overlaps_and_inside() {
        for (t, s) in all() {
            let r = room(6.0, 5.0);
            let out = place(&r, &t, s);
            for (i, a) in out.iter().enumerate() {
                assert!(a.min.cmpge(r.min - 1e-3).all() && a.max.cmple(r.max + 1e-3).all());
                for (j, b) in out.iter().enumerate().skip(i + 1) {
                    let la = a.prop.spec().layer;
                    let lb = b.prop.spec().layer;
                    let floor_pair = la == Layer::Floor || lb == Layer::Floor;
                    let floor_both = la == Layer::Floor && lb == Layer::Floor;
                    let hosted = a.on == Some(j) || b.on == Some(i);
                    if floor_both {
                        let sa = (a.min.with_y(0.0), a.max.with_y(1.0));
                        let sb = (b.min.with_y(0.0), b.max.with_y(1.0));
                        assert!(!overlaps(sa, sb), "{} {s}: floor overlap", t.name);
                    } else if !floor_pair && !hosted {
                        assert!(
                            !overlaps((a.min, a.max), (b.min, b.max)),
                            "{} {s}: {a:?} overlaps {b:?}",
                            t.name
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn doors_stay_reachable_and_reserved_kept() {
        for (t, s) in all() {
            let r = room(5.0, 4.0);
            let out = place(&r, &t, s);
            assert!(doors_reachable(&r, &out), "{} {s}", t.name);
            for p in &out {
                if p.prop.spec().layer == Layer::Air {
                    continue;
                }
                for &(c, rad) in &r.reserved {
                    let d = box_dist(Vec2::new(p.min.x, p.min.z), Vec2::new(p.max.x, p.max.z), c);
                    assert!(d >= rad - 1e-3, "{} {s}: {:?} on a pin", t.name, p.prop);
                }
            }
        }
    }

    #[test]
    fn min_counts_in_a_big_room() {
        for (t, s) in all() {
            let r = room(10.0, 8.0);
            let out = place(&r, &t, s);
            for e in &t.entries {
                let n = out.iter().filter(|p| p.prop == e.prop).count() as u32;
                assert!(n >= e.min, "{} {s}: {:?} {n} < {}", t.name, e.prop, e.min);
                assert!(n <= e.max, "{} {s}: {:?} {n} > {}", t.name, e.prop, e.max);
            }
        }
    }

    #[test]
    fn deterministic_and_seeds_differ() {
        let r = room(6.0, 5.0);
        for t in Theme::builtin() {
            assert_eq!(place(&r, &t, 7), place(&r, &t, 7));
            let differ = (0..8u64).any(|s| place(&r, &t, s) != place(&r, &t, s + 100));
            assert!(differ, "{}", t.name);
        }
    }

    #[test]
    fn apply_emits_pieces() {
        let r = room(6.0, 5.0);
        let Some(t) = Theme::named("kitchen") else {
            panic!("no kitchen");
        };
        let out = place(&r, &t, 3);
        let mut kit = Kit::<M>::default();
        apply(&mut kit, &out);
        assert!(!out.is_empty());
        assert!(kit.pieces.len() >= out.len());
        assert!(!kit.lights.is_empty(), "the kitchen has a lamp");
    }

    #[test]
    fn tiny_room_stays_walkable() {
        let r = room(2.0, 2.0);
        for t in Theme::builtin() {
            for s in 0..10 {
                assert!(doors_reachable(&r, &place(&r, &t, s)));
            }
        }
    }
}
