//! Hallway kit (structures rework, phase 3): turns a set of open grid cells
//! into detailed tunnels laid into a [`Kit`]: floor tiles (with a sunken
//! water channel and grates for sewers), brick wall panels with baseboards
//! and a top trim, pillars every other cell, ceiling beams across straight
//! runs, pipes along one wall and light fixtures.
//!
//! Solid boxes never overlap (walls shorten at inside corners and grow at
//! outside ones), and trims are kept off each other's faces, so nothing
//! z-fights. No game code.

use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::graph::Cell;
use crate::{Kit, Palette, Shape, Solid};

/// The sizes of a hallway network.
#[derive(Clone, Copy, Debug)]
pub struct Hall {
    /// Cell size (metres); the tunnel is one cell wide.
    pub cell: f32,
    /// Floor height (top of the walkways).
    pub floor: f32,
    /// Floor to ceiling.
    pub height: f32,
    /// Wall and slab thickness.
    pub thick: f32,
}

/// Which light fixture a stretch gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixture {
    None,
    /// A caged bulb under the ceiling.
    Ceiling,
    /// A torch in a bracket on a wall.
    Torch,
}

/// How one cell looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style<M> {
    pub floor: M,
    pub wall: M,
    pub ceiling: M,
    /// Baseboards, top trim and pillars.
    pub trim: M,
    /// Beams, pipes, grates and brackets.
    pub metal: M,
    pub water: M,
    /// Fixture glow.
    pub glow: M,
    /// A sunken water channel joining wet neighbours.
    pub wet: bool,
    pub pipes: bool,
    pub beams: bool,
    pub fixture: Fixture,
    /// One fixture every this many cells (by grid position).
    pub every: i32,
}

/// The tunnels for every cell `style` returns a style for.
pub fn hall<M: Palette>(
    kit: &mut Kit<M>,
    h: &Hall,
    cells: &[Cell],
    style: impl Fn(Cell) -> Option<Style<M>>,
) {
    let open = |c: Cell| style(c).is_some();
    for &c in cells {
        let Some(s) = style(c) else { continue };
        floor(kit, h, c, &s, &|n| style(n).is_some_and(|t| t.wet));
        ceiling(kit, h, c, &s);
        for side in 0..4 {
            wall(kit, h, c, side, &s, &open);
        }
        detail(kit, h, c, &s, &open);
    }
}

fn centre(h: &Hall, c: Cell) -> Vec2 {
    Vec2::new(c.0 as f32 * h.cell, c.1 as f32 * h.cell)
}

/// Floor: a 3 x 3 grid of tiles; a wet cell sinks the middle tile and the
/// edge tiles towards wet neighbours into a channel with water in it.
fn floor<M: Palette>(
    kit: &mut Kit<M>,
    h: &Hall,
    c: Cell,
    s: &Style<M>,
    wet: &dyn Fn(Cell) -> bool,
) {
    let o = centre(h, c);
    let third = h.cell / 3.0;
    let sink = 0.14;
    for i in -1..=1 {
        for j in -1..=1 {
            let lo = o + Vec2::new(i as f32 - 0.5, j as f32 - 0.5) * third;
            let hi = lo + Vec2::splat(third);
            let channel =
                s.wet && ((i, j) == (0, 0) || ((i == 0) != (j == 0) && wet((c.0 + i, c.1 + j))));
            let top = if channel { h.floor - sink } else { h.floor };
            kit.block(
                Vec3::new(lo.x, h.floor - h.thick, lo.y),
                Vec3::new(hi.x, top, hi.y),
                s.floor,
                Solid::Floor,
            );
            if !channel {
                continue;
            }
            // Water a little below the walkway, inset from the tile edges.
            let inset = 0.01;
            kit.block(
                Vec3::new(lo.x + inset, top, lo.y + inset),
                Vec3::new(hi.x - inset, h.floor - 0.06, hi.y - inset),
                s.water,
                Solid::None,
            );
            // A grate across the middle of every third wet cell.
            if (i, j) == (0, 0) && (c.0 * 7 + c.1 * 3).rem_euclid(3) == 0 {
                for k in 0..5 {
                    let x = lo.x + third * (k as f32 + 0.5) / 5.0;
                    kit.put(
                        Shape::Cube,
                        Vec3::new(x, h.floor - 0.035, (lo.y + hi.y) / 2.0),
                        Vec3::new(0.05, 0.05, third - 0.04),
                        Quat::IDENTITY,
                        s.metal,
                        Solid::None,
                        true,
                    );
                }
            }
        }
    }
}

fn ceiling<M: Palette>(kit: &mut Kit<M>, h: &Hall, c: Cell, s: &Style<M>) {
    let o = centre(h, c);
    let half = h.cell / 2.0;
    let top = h.floor + h.height;
    kit.block(
        Vec3::new(o.x - half, top, o.y - half),
        Vec3::new(o.x + half, top + h.thick, o.y + half),
        s.ceiling,
        Solid::Wall,
    );
}

/// Side `0..4`: +x, -x, +z, -z.
fn dir(side: usize) -> Cell {
    [(1, 0), (-1, 0), (0, 1), (0, -1)][side]
}

/// One wall panel where a cell meets rock, with its trims. The panel sits
/// just outside the cell. Along x-facing sides it spans the cell exactly
/// (short of an inside corner); z-facing ones grow over outside corners.
fn wall<M: Palette>(
    kit: &mut Kit<M>,
    h: &Hall,
    c: Cell,
    side: usize,
    s: &Style<M>,
    open: &dyn Fn(Cell) -> bool,
) {
    let d = dir(side);
    if open((c.0 + d.0, c.1 + d.1)) {
        return;
    }
    let o = centre(h, c);
    let half = h.cell / 2.0;
    let t = h.thick;
    let normal = Vec2::new(d.0 as f32, d.1 as f32);
    // Along the wall, in world terms.
    let along = Vec2::new(normal.y.abs(), normal.x.abs());
    let a_cell = (along.x as i32, along.y as i32);
    let mut ends = [-half, half];
    let mut trim_ends = [-half, half];
    for (k, e) in [-1, 1].into_iter().enumerate() {
        let next = (c.0 + a_cell.0 * e, c.1 + a_cell.1 * e);
        let diag = (next.0 + d.0, next.1 + d.1);
        let sign = e as f32;
        if !open(next) {
            // A corner of the room: z-facing panels cover the corner square
            // (x-facing ones step back from it if a cell sits diagonally
            // beyond), and x-facing trims butt into the z-facing ones.
            if d.0 == 0 {
                ends[k] += sign * t;
            } else {
                if open(diag) {
                    ends[k] -= sign * t;
                }
                trim_ends[k] -= sign * 0.1;
            }
        } else if open(diag) && d.0 != 0 {
            // The wall ends: the diagonal cell's wall runs across its end.
            ends[k] -= sign * t;
        }
    }
    let at = |s_: f32, off: f32| o + along * s_ + normal * off;
    let (y0, y1) = (h.floor, h.floor + h.height);
    let rot = if d.0 != 0 {
        Quat::from_rotation_y(FRAC_PI_2)
    } else {
        Quat::IDENTITY
    };
    // `size` is (along, height, depth).
    let slab = |kit: &mut Kit<M>,
                s0: f32,
                s1: f32,
                off: f32,
                ya: f32,
                yb: f32,
                depth: f32,
                m: M,
                solid: Solid,
                small: bool| {
        if s1 - s0 < 1e-3 {
            return;
        }
        let p = at((s0 + s1) / 2.0, off);
        kit.put(
            Shape::Cube,
            Vec3::new(p.x, (ya + yb) / 2.0, p.y),
            Vec3::new(s1 - s0, yb - ya, depth),
            rot,
            m,
            solid,
            small,
        );
    };
    // The panel: from the floor slab's bottom to the ceiling slab's top.
    slab(
        kit,
        ends[0],
        ends[1],
        half + t / 2.0,
        y0 - t,
        y1 + t,
        t,
        s.wall,
        Solid::Wall,
        false,
    );
    // Baseboard and top trim, proud of the wall face.
    let [ta, tb] = trim_ends;
    slab(
        kit,
        ta,
        tb,
        half - 0.04,
        y0,
        y0 + 0.22,
        0.08,
        s.trim,
        Solid::None,
        false,
    );
    slab(
        kit,
        ta,
        tb,
        half - 0.05,
        y1 - 0.3,
        y1 - 0.16,
        0.1,
        s.trim,
        Solid::None,
        false,
    );
    // Pipes along the +x / +z walls.
    if s.pipes && (side == 0 || side == 2) {
        for (k, y) in [(0, y1 - 0.55), (1, y1 - 0.8)] {
            let r = 0.07 + 0.03 * k as f32;
            let off = half - 0.14 - 0.1 * k as f32;
            let (pa, pb) = (ta, tb);
            let len = pb - pa;
            if len < 0.1 {
                continue;
            }
            let p = at((pa + pb) / 2.0, off);
            let lie = if d.0 != 0 {
                Quat::from_rotation_x(FRAC_PI_2)
            } else {
                Quat::from_rotation_z(FRAC_PI_2)
            };
            kit.put(
                Shape::Cylinder,
                Vec3::new(p.x, y, p.y),
                Vec3::new(r * 2.0, len, r * 2.0),
                lie,
                s.metal,
                Solid::None,
                true,
            );
        }
    }
}

/// Pillars, beams and fixtures.
fn detail<M: Palette>(
    kit: &mut Kit<M>,
    h: &Hall,
    c: Cell,
    s: &Style<M>,
    open: &dyn Fn(Cell) -> bool,
) {
    let o = centre(h, c);
    let half = h.cell / 2.0;
    let (y0, y1) = (h.floor, h.floor + h.height);
    let ox = open((c.0 + 1, c.1)) || open((c.0 - 1, c.1));
    let oz = open((c.0, c.1 + 1)) || open((c.0, c.1 - 1));
    let walls_x = !open((c.0 + 1, c.1)) && !open((c.0 - 1, c.1));
    let walls_z = !open((c.0, c.1 + 1)) && !open((c.0, c.1 - 1));
    // A straight run along z (walls on both x sides), or along x.
    let run_z = walls_x && oz;
    let run_x = walls_z && ox;
    let beat = (c.0 + c.1).rem_euclid(2) == 0;
    if beat && (run_x || run_z) {
        // Pillars against both walls at the middle of the cell, and a beam
        // across between them under the ceiling.
        let across = if run_z { Vec2::X } else { Vec2::Y };
        for sgn in [-1.0f32, 1.0] {
            let p = o + across * sgn * (half - 0.07);
            let size = if run_z {
                Vec3::new(0.16, h.height - 0.02, 0.42)
            } else {
                Vec3::new(0.42, h.height - 0.02, 0.16)
            };
            kit.put(
                Shape::Cube,
                Vec3::new(p.x, y0 + 0.01 + size.y / 2.0, p.y),
                size,
                Quat::IDENTITY,
                s.trim,
                Solid::None,
                false,
            );
        }
        if s.beams {
            let size = if run_z {
                Vec3::new(h.cell - 0.3, 0.26, 0.3)
            } else {
                Vec3::new(0.3, 0.26, h.cell - 0.3)
            };
            kit.put(
                Shape::Cube,
                Vec3::new(o.x, y1 - 0.01 - size.y / 2.0, o.y),
                size,
                Quat::IDENTITY,
                s.metal,
                Solid::None,
                false,
            );
        }
    }
    if s.every <= 0 || (c.0 * 3 + c.1 * 5).rem_euclid(s.every) != 0 {
        return;
    }
    match s.fixture {
        Fixture::None => {}
        Fixture::Ceiling => {
            // Off the beam line so the two never meet.
            let shift = if run_z { Vec2::Y } else { Vec2::X } * 0.7;
            let p = o + shift;
            let top = y1;
            kit.put(
                Shape::Cube,
                Vec3::new(p.x, top - 0.02, p.y),
                Vec3::new(0.34, 0.04, 0.34),
                Quat::IDENTITY,
                s.metal,
                Solid::None,
                true,
            );
            kit.put(
                Shape::Cube,
                Vec3::new(p.x, top - 0.16, p.y),
                Vec3::new(0.03, 0.24, 0.03),
                Quat::IDENTITY,
                s.metal,
                Solid::None,
                true,
            );
            let bulb = Vec3::new(p.x, top - 0.36, p.y);
            kit.put(
                Shape::Ball,
                bulb,
                Vec3::splat(0.2),
                Quat::IDENTITY,
                s.glow,
                Solid::None,
                true,
            );
            kit.lights.push(bulb - Vec3::Y * 0.2);
        }
        Fixture::Torch => {
            // On the first wall found, beside the middle (clear of pillars).
            let Some(side) = (0..4).find(|&k| {
                let d = dir(k);
                !open((c.0 + d.0, c.1 + d.1))
            }) else {
                return;
            };
            let d = dir(side);
            let n = Vec2::new(d.0 as f32, d.1 as f32);
            let along = Vec2::new(n.y.abs(), n.x.abs());
            let p = o + n * (half - 0.12) + along * 0.6;
            kit.put(
                Shape::Cube,
                Vec3::new(p.x, y0 + 1.75, p.y),
                Vec3::new(0.08, 0.3, 0.08),
                Quat::IDENTITY,
                s.metal,
                Solid::None,
                true,
            );
            let flame = Vec3::new(p.x, y0 + 1.98, p.y);
            kit.put(
                Shape::Ball,
                flame,
                Vec3::new(0.14, 0.2, 0.14),
                Quat::IDENTITY,
                s.glow,
                Solid::None,
                true,
            );
            let q = o + n * (half - 0.6) + along * 0.6;
            kit.lights.push(Vec3::new(q.x, y0 + 1.9, q.y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Room, Spec, generate};
    use crate::tests::M;

    fn style(wet: bool) -> Style<M> {
        Style {
            floor: M::Wood,
            wall: M::Wall,
            ceiling: M::Wall,
            trim: M::Timber,
            metal: M::Timber,
            water: M::Hidden,
            glow: M::Wall,
            wet,
            pipes: true,
            beams: true,
            fixture: Fixture::Ceiling,
            every: 3,
        }
    }

    /// World-space bounds of an axis-aligned (quarter-turned) box.
    fn aabb(p: &crate::Piece<M>) -> (Vec3, Vec3) {
        let half = (p.rot * (p.size / 2.0)).abs();
        (p.centre - half, p.centre + half)
    }

    #[test]
    fn solid_boxes_never_overlap() {
        let spec = Spec {
            min: (-10, -10),
            max: (10, 10),
            pins: vec![Room::around((0, 0), 1, 0)],
            rooms: (3, 5),
            room_size: (2, 3),
            tag: 1,
            loops: 2,
            spurs: 4,
            spur_len: (2, 3),
        };
        for seed in 0..16 {
            let g = generate(&spec, seed);
            let mut kit = Kit::<M>::default();
            kit.place(Vec3::ZERO, 0);
            let hall_ = Hall {
                cell: 3.0,
                floor: -30.0,
                height: 3.2,
                thick: 0.3,
            };
            let cells: Vec<Cell> = g.cells.keys().copied().collect();
            hall(&mut kit, &hall_, &cells, |c| {
                g.open(c).then(|| style(c.0 % 2 == 0))
            });
            let solids: Vec<_> = kit
                .pieces
                .iter()
                .filter(|p| p.solid != Solid::None && p.shape == Shape::Cube)
                .map(aabb)
                .collect();
            assert!(solids.len() > cells.len() * 3);
            for (i, a) in solids.iter().enumerate() {
                for b in &solids[i + 1..] {
                    let o = a.1.min(b.1) - a.0.max(b.0);
                    assert!(
                        o.x < 1e-3 || o.y < 1e-3 || o.z < 1e-3,
                        "seed {seed}: {a:?} overlaps {b:?}"
                    );
                }
            }
            assert!(!kit.lights.is_empty());
        }
    }

    #[test]
    fn open_cells_stay_walkable() {
        // Nothing solid stands in the walking space of an open cell.
        let g = generate(
            &Spec {
                min: (-8, -8),
                max: (8, 8),
                pins: vec![Room::around((0, 0), 1, 0), Room::around((6, 6), 0, 0)],
                rooms: (2, 3),
                room_size: (2, 3),
                tag: 1,
                loops: 1,
                spurs: 3,
                spur_len: (2, 3),
            },
            3,
        );
        let h = Hall {
            cell: 3.0,
            floor: 0.0,
            height: 3.2,
            thick: 0.3,
        };
        let mut kit = Kit::<M>::default();
        kit.place(Vec3::ZERO, 0);
        let cells: Vec<Cell> = g.cells.keys().copied().collect();
        hall(&mut kit, &h, &cells, |c| g.open(c).then(|| style(true)));
        for c in &cells {
            let o = Vec3::new(c.0 as f32 * 3.0, 1.0, c.1 as f32 * 3.0);
            for p in kit.pieces.iter().filter(|p| p.solid != Solid::None) {
                let (lo, hi) = aabb(p);
                let inside = (lo.x..hi.x).contains(&o.x)
                    && (lo.y..hi.y).contains(&o.y)
                    && (lo.z..hi.z).contains(&o.z);
                assert!(!inside, "{c:?} blocked by {p:?}");
            }
        }
    }
}
