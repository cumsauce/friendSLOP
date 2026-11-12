//! Buildings from code. A [`Kit`] lays out plain pieces (boxes, cylinders,
//! gable prisms, balls) in world space: walls with door and window holes,
//! floors, ramps, stairs, pitched roofs, lamps and furniture
//! ([`furniture`]). It is pure data, so every peer builds the same thing.
//!
//! [`build`] turns the pieces into a few merged meshes (one per group and
//! material) with UVs in metres measured in world space, so textures run on
//! across neighbouring pieces instead of restarting on each, and into static
//! colliders (one box per wall segment, a slab per roof plane, a hull per
//! gable).
//!
//! Pieces are laid so solid boxes never overlap (they may touch): a wall
//! with holes is split into separate segments, and lintels and sills only
//! fill the hole, standing a little proud of the wall so no faces are
//! coplanar (z-fighting). No game code.

use std::f32::consts::FRAC_PI_2;
use std::fmt::Debug;
use std::hash::Hash;

use bevy::prelude::*;

pub mod build;
pub mod furniture;
pub mod grammar;
pub mod graph;
pub mod hall;
pub mod placer;
pub mod props;

/// The materials a kit needs to know about. The game's own material enum
/// implements this, naming which of its materials play each role.
pub trait Palette: Copy + Eq + Hash + Debug + Send + Sync + 'static {
    /// Collider only, nothing drawn.
    const HIDDEN: Self;
    /// Dark timber: lintels, sills, legs.
    const TIMBER: Self;
    /// Warm wood: stair steps, bed frames, tables, crates.
    const WOOD: Self;
    /// Planks: shelves, small crates.
    const PLANK: Self;
    /// A blanket.
    const CLOTH: Self;
    /// A pillow.
    const LINEN: Self;
    /// Fruit and shelf goods.
    const FRUIT: Self;
    const RED: Self;
    /// Lamp cords.
    const IRON: Self;
    /// Lamp glow.
    const LAMP: Self;
}

/// Door openings (wide and tall enough for a body capsule).
pub const DOOR_W: f32 = 1.4;
pub const DOOR_H: f32 = 2.3;
/// Lintel height over a hole, and sill height under a window.
pub const LINTEL: f32 = 0.1;
pub const SILL: f32 = 0.06;
/// How far trims stand proud of a wall face.
pub const PROUD: f32 = 0.03;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    /// Box of `size`.
    Cube,
    /// Upright cylinder: `size.x` diameter, `size.y` height.
    Cylinder,
    /// Triangular prism (a gable end): `size.x` base, `size.y` height,
    /// `size.z` thickness. `centre` is the middle of its base.
    Gable,
    /// Small sphere (lamps, fruit).
    Ball,
}

/// How a piece collides.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Solid {
    None,
    /// Blocks bodies.
    Wall,
    /// Walkable floor or ramp.
    Floor,
}

#[derive(Clone, Copy, Debug)]
pub struct Piece<M> {
    pub shape: Shape,
    pub centre: Vec3,
    pub size: Vec3,
    pub rot: Quat,
    pub mat: M,
    pub solid: Solid,
    /// Small clutter: no shadow.
    pub small: bool,
    /// Which `place` call laid it: meshes are merged per group.
    pub group: u32,
    /// The prop it belongs to, an index into [`Kit::props`]: a game that has
    /// a model for that prop draws the model instead (the piece still
    /// collides).
    pub prop: Option<u32>,
}

/// A prop drawn from kit pieces, which a game may draw from a model instead:
/// the model's name (`models/props/<name>.gltf`), and where it stands in
/// world space. Props are built base-centred at y = 0, back at -z, front at
/// +z (see [`props`]), and `scale` stretches the model to the size drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PropMark {
    pub name: &'static str,
    pub at: Vec3,
    pub rot: Quat,
    pub scale: Vec3,
}

/// A doorway (or gate): floor-level centre, the way through, and its size.
#[derive(Clone, Copy, Debug)]
pub struct Door {
    pub centre: Vec3,
    pub through: Vec3,
    pub width: f32,
    pub height: f32,
}

/// The walking line of a ramp or staircase (top surface, lane centre).
#[derive(Clone, Copy, Debug)]
pub struct Ramp {
    pub from: Vec3,
    pub to: Vec3,
    pub width: f32,
}

/// An opening in a wall: centre along the wall (from its middle), width, and
/// bottom/top above the wall's base. A bottom of 0 makes it a door.
#[derive(Clone, Copy, Debug)]
pub struct Hole {
    pub c: f32,
    pub w: f32,
    pub y0: f32,
    pub y1: f32,
}

impl Hole {
    pub const fn door(c: f32) -> Hole {
        Hole {
            c,
            w: DOOR_W,
            y0: 0.0,
            y1: DOOR_H,
        }
    }

    pub const fn window(c: f32) -> Hole {
        Hole {
            c,
            w: 1.0,
            y0: 1.0,
            y1: 2.0,
        }
    }
}

/// Pieces, doors, ramps and light spots, all in world space.
pub struct Kit<M> {
    pub pieces: Vec<Piece<M>>,
    pub doors: Vec<Door>,
    pub ramps: Vec<Ramp>,
    pub lights: Vec<Vec3>,
    /// Props laid with [`Kit::begin_prop`], which their pieces point at.
    pub props: Vec<PropMark>,
    /// Outer wall thickness used by [`Kit::walls`] and gable ends.
    pub thick: f32,
    origin: Vec3,
    yaw: Quat,
    group: u32,
    /// The prop being drawn, if any.
    prop: Option<u32>,
}

impl<M> Default for Kit<M> {
    fn default() -> Self {
        Self {
            pieces: Vec::new(),
            doors: Vec::new(),
            ramps: Vec::new(),
            lights: Vec::new(),
            props: Vec::new(),
            thick: 0.25,
            origin: Vec3::ZERO,
            yaw: Quat::IDENTITY,
            group: 0,
            prop: None,
        }
    }
}

/// Turn about y that points local +x along `dir` (xz).
pub fn yaw_of(dir: Vec2) -> Quat {
    Quat::from_rotation_y((-dir.y).atan2(dir.x))
}

impl<M: Palette> Kit<M> {
    /// Build the next things in a local frame at `origin`, turned by quarter
    /// turns. Starts a new mesh group.
    pub fn place(&mut self, origin: Vec3, quarter_turns: i32) {
        self.origin = origin;
        self.yaw = Quat::from_rotation_y(FRAC_PI_2 * quarter_turns as f32);
        self.group += 1;
    }

    /// A local point in world space.
    pub fn world(&self, p: Vec3) -> Vec3 {
        self.origin + self.yaw * p
    }

    pub fn put(
        &mut self,
        shape: Shape,
        centre: Vec3,
        size: Vec3,
        rot: Quat,
        mat: M,
        solid: Solid,
        small: bool,
    ) {
        self.pieces.push(Piece {
            shape,
            centre: self.world(centre),
            size,
            rot: self.yaw * rot,
            mat,
            solid,
            small,
            group: self.group,
            prop: self.prop,
        });
    }

    /// Start drawing the prop `name` standing at local `base` (turned with
    /// the kit), `scale` times its model's size. Pieces laid until
    /// [`Kit::end_prop`] belong to it. Nested calls keep the outer prop.
    /// Returns whether this call started one (pass it to `end_prop`).
    pub fn begin_prop(&mut self, name: &'static str, base: Vec3, scale: Vec3) -> bool {
        if self.prop.is_some() {
            return false;
        }
        self.prop = Some(self.props.len() as u32);
        self.props.push(PropMark {
            name,
            at: self.world(base),
            rot: self.yaw,
            scale,
        });
        true
    }

    /// Stop drawing the prop `begin_prop` started (if `started`).
    pub fn end_prop(&mut self, started: bool) {
        if started {
            self.prop = None;
        }
    }

    /// An axis-aligned box from `min` to `max` (local).
    pub fn block(&mut self, min: Vec3, max: Vec3, mat: M, solid: Solid) {
        let (c, s) = ((min + max) / 2.0, max - min);
        self.put(Shape::Cube, c, s, Quat::IDENTITY, mat, solid, false);
    }

    /// Small solid clutter (furniture, crates): a box standing on `base`.
    pub fn prop(&mut self, base: Vec3, size: Vec3, mat: M) {
        let c = base + Vec3::Y * size.y / 2.0;
        self.put(Shape::Cube, c, size, Quat::IDENTITY, mat, Solid::Wall, true);
    }

    /// Small decoration that doesn't collide.
    pub fn trinket(&mut self, shape: Shape, centre: Vec3, size: Vec3, mat: M) {
        self.put(shape, centre, size, Quat::IDENTITY, mat, Solid::None, true);
    }

    /// A horizontal bar of `height` x `thick` from `a` to `b` (local xz),
    /// bottom at `y`. Rails, hedges, low walls, fences.
    pub fn bar(&mut self, a: Vec2, b: Vec2, y: f32, height: f32, thick: f32, mat: M, solid: Solid) {
        let d = b - a;
        let len = d.length();
        if len < 1e-3 {
            return;
        }
        let m = (a + b) / 2.0;
        self.put(
            Shape::Cube,
            Vec3::new(m.x, y + height / 2.0, m.y),
            Vec3::new(len, height, thick),
            yaw_of(d / len),
            mat,
            solid,
            false,
        );
    }

    /// A solid wall from `a` to `b` (local xz), base `y0`, `h` tall, with
    /// holes. The wall is split into segments that only touch: full-height
    /// piers between holes, and pieces above and below each hole. A timber
    /// lintel fills the top of each hole and a sill the bottom of each
    /// window, both [`PROUD`] of the wall faces.
    pub fn wall(&mut self, a: Vec2, b: Vec2, y0: f32, h: f32, thick: f32, holes: &[Hole], mat: M) {
        let d = b - a;
        let len = d.length();
        if len < 1e-3 {
            return;
        }
        let dir = d / len;
        let rot = yaw_of(dir);
        let mid = (a + b) / 2.0;
        let at = |s: f32, y: f32| {
            let p = mid + dir * s;
            Vec3::new(p.x, y, p.y)
        };
        let seg = |kit: &mut Kit<M>, s0: f32, s1: f32, ya: f32, yb: f32, m: M, t: f32| {
            if s1 - s0 > 1e-3 && yb - ya > 1e-3 {
                kit.put(
                    Shape::Cube,
                    at((s0 + s1) / 2.0, (ya + yb) / 2.0),
                    Vec3::new(s1 - s0, yb - ya, t),
                    rot,
                    m,
                    Solid::Wall,
                    false,
                );
            }
        };
        let trim = thick + PROUD * 2.0;
        let mut holes = holes.to_vec();
        holes.sort_by(|p, q| p.c.total_cmp(&q.c));
        let mut cursor = -len / 2.0;
        for hole in holes {
            let (l, r) = (
                (hole.c - hole.w / 2.0).max(cursor),
                (hole.c + hole.w / 2.0).min(len / 2.0),
            );
            seg(self, cursor, l, y0, y0 + h, mat, thick);
            let lintel = hole.y1 + LINTEL < h - 0.05;
            let top = if lintel { hole.y1 + LINTEL } else { hole.y1 };
            seg(self, l, r, y0 + top, y0 + h, mat, thick);
            if lintel {
                let (ya, yb) = (y0 + hole.y1, y0 + hole.y1 + LINTEL);
                seg(self, l, r, ya, yb, M::TIMBER, trim);
            }
            if hole.y0 <= 1e-3 {
                self.doors.push(Door {
                    centre: self.world(at(hole.c, y0)),
                    through: self.yaw * rot * Vec3::Z,
                    width: hole.w,
                    height: hole.y1,
                });
            } else if hole.y0 - SILL > 0.05 {
                let base = y0 + hole.y0 - SILL;
                seg(self, l, r, y0, base, mat, thick);
                seg(self, l, r, base, y0 + hole.y0, M::TIMBER, trim);
            } else {
                seg(self, l, r, y0, y0 + hole.y0, mat, thick);
            }
            cursor = r;
        }
        seg(self, cursor, len / 2.0, y0, y0 + h, mat, thick);
    }

    /// Four walls of a `w` x `d` storey (local, centred), base `y0`, holes
    /// for the south (+z), north, east (+x) and west walls, each read from
    /// that wall's own middle. Dark timber corner posts and top beams trim
    /// it (non-solid, proud of every wall face).
    pub fn walls(&mut self, w: f32, d: f32, y0: f32, h: f32, holes: [&[Hole]; 4], mat: M) {
        let t = self.thick;
        let (hx, hz) = (w / 2.0, d / 2.0);
        let [south, north, east, west] = holes;
        let v = Vec2::new;
        self.wall(
            v(-hx - t / 2.0, hz),
            v(hx + t / 2.0, hz),
            y0,
            h,
            t,
            south,
            mat,
        );
        // North runs east -> west so its holes read from its own middle.
        self.wall(
            v(hx + t / 2.0, -hz),
            v(-hx - t / 2.0, -hz),
            y0,
            h,
            t,
            north,
            mat,
        );
        self.wall(
            v(hx, hz - t / 2.0),
            v(hx, -hz + t / 2.0),
            y0,
            h,
            t,
            east,
            mat,
        );
        self.wall(
            v(-hx, -hz + t / 2.0),
            v(-hx, hz - t / 2.0),
            y0,
            h,
            t,
            west,
            mat,
        );
        // Posts and beams stand 2 cm proud of the wall top and faces so no
        // faces are coplanar (z-fighting).
        for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let c = Vec3::new(sx * hx, y0 + (h + 0.03) / 2.0, sz * hz);
            let s = Vec3::new(t + 0.1, h + 0.03, t + 0.1);
            self.put(
                Shape::Cube,
                c,
                s,
                Quat::IDENTITY,
                M::TIMBER,
                Solid::None,
                false,
            );
        }
        for z in [-hz, hz] {
            let c = Vec3::new(0.0, y0 + h - 0.08, z);
            let s = Vec3::new(w, 0.2, t + 0.06);
            self.put(
                Shape::Cube,
                c,
                s,
                Quat::IDENTITY,
                M::TIMBER,
                Solid::None,
                false,
            );
        }
    }

    /// A walkable slab with its top at `top`, covering `min`..`max` (local xz).
    pub fn floor(&mut self, min: Vec2, max: Vec2, top: f32, mat: M) {
        self.block(
            Vec3::new(min.x, top - 0.15, min.y),
            Vec3::new(max.x, top, max.y),
            mat,
            Solid::Floor,
        );
    }

    /// A walkable ramp whose top surface runs from `from` to `to` (local).
    pub fn ramp(&mut self, from: Vec3, to: Vec3, width: f32, mat: M) {
        let d = to - from;
        let flat = d.with_y(0.0);
        let rot = yaw_of(Vec2::new(flat.x, flat.z).normalize_or_zero())
            * Quat::from_rotation_z(d.y.atan2(flat.length()));
        let thick = 0.2;
        let centre = (from + to) / 2.0 - rot * Vec3::Y * thick / 2.0;
        let size = Vec3::new(d.length(), thick, width);
        self.put(Shape::Cube, centre, size, rot, mat, Solid::Floor, false);
        self.ramps.push(Ramp {
            from: self.world(from),
            to: self.world(to),
            width,
        });
    }

    /// A staircase on the ground: an invisible ramp to walk on, step blocks to
    /// look at, and solid blocks underneath so nobody walks into its side.
    pub fn stairs(&mut self, from: Vec3, to: Vec3, width: f32) {
        self.ramp(from, to, width, M::HIDDEN);
        let d = to - from;
        let flat = d.with_y(0.0);
        let len = flat.length();
        let dir = flat / len;
        let rot = yaw_of(Vec2::new(dir.x, dir.z));
        let steps = (d.y / 0.25).ceil().max(1.0) as usize;
        for i in 0..steps {
            let top = d.y * (i + 1) as f32 / steps as f32;
            let s = len * (i as f32 + 0.5) / steps as f32;
            let c = from + dir * s + Vec3::Y * (top - 0.1) / 2.0;
            let size = Vec3::new(len / steps as f32, top + 0.1, width);
            self.put(Shape::Cube, c, size, rot, M::WOOD, Solid::None, false);
        }
        for k in 1..5 {
            let top = d.y * k as f32 / 5.0 - 0.3;
            let s = len * (k as f32 + 0.5) / 5.0;
            let c = from + dir * s + Vec3::Y * top / 2.0;
            let size = Vec3::new(len / 5.0, top, width);
            self.put(Shape::Cube, c, size, rot, M::HIDDEN, Solid::Wall, false);
        }
    }

    /// A pitched roof over a `w` x `d` footprint centred at `c` (local),
    /// ridge along x, rows of tiles alternating `a`/`b`, gables in `wall`.
    /// Each roof plane gets one hidden solid slab and each gable a hull.
    pub fn roof(&mut self, c: Vec2, w: f32, d: f32, eave: f32, rise: f32, a: M, b: M, wall: M) {
        let over = 0.45;
        let run = d / 2.0 + over;
        let angle = rise.atan2(d / 2.0);
        let slope = run / angle.cos();
        let ridge = Vec3::new(c.x, eave + rise, c.y);
        let rows = 5;
        let width = w + over * 2.0;
        for side in [-1.0f32, 1.0] {
            let rot = Quat::from_rotation_x(angle * side);
            let down = Vec3::new(0.0, -angle.sin(), angle.cos() * side);
            let n = rot * Vec3::Y;
            for k in 0..rows {
                let s = slope * (k as f32 + 0.5) / rows as f32;
                let centre = ridge + down * s + n * (0.08 + 0.03 * (k % 2) as f32);
                let size = Vec3::new(width, 0.12, slope / rows as f32 * 1.12);
                let mat = if k % 2 == 0 { a } else { b };
                self.put(Shape::Cube, centre, size, rot, mat, Solid::None, false);
            }
            let centre = ridge + down * slope / 2.0 + n * 0.08;
            let size = Vec3::new(width, 0.12, slope);
            self.put(
                Shape::Cube,
                centre,
                size,
                rot,
                M::HIDDEN,
                Solid::Wall,
                false,
            );
        }
        // Ridge cap.
        let cap = Vec3::new(width + 0.1, 0.22, 0.32);
        let at = ridge + Vec3::Y * 0.12;
        self.put(Shape::Cube, at, cap, Quat::IDENTITY, b, Solid::None, false);
        // Gable ends, filling the wall up to the ridge.
        let t = self.thick;
        for side in [-1.0f32, 1.0] {
            let at = Vec3::new(c.x + side * (w / 2.0), eave, c.y);
            // Wider than the roof run by `t`, so keep the rake slightly
            // steeper than the roof plane or it pokes up through the tiles.
            let size = Vec3::new(d + t, rise * (d + t) / d + 0.02, t);
            let rot = Quat::from_rotation_y(FRAC_PI_2);
            self.put(Shape::Gable, at, size, rot, wall, Solid::Wall, false);
        }
    }

    /// A hanging lamp (glowing ball on a cord).
    pub fn lamp(&mut self, at: Vec3) {
        let p = self.begin_prop("hanging_lamp", at, Vec3::ONE);
        let cord = Vec3::new(0.03, 0.8, 0.03);
        self.trinket(Shape::Cube, at + Vec3::Y * 0.5, cord, M::IRON);
        self.trinket(Shape::Ball, at, Vec3::splat(0.32), M::LAMP);
        self.end_prop(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
    pub(crate) enum M {
        Hidden,
        Wall,
        Timber,
        Wood,
    }

    impl Palette for M {
        const HIDDEN: Self = M::Hidden;
        const TIMBER: Self = M::Timber;
        const WOOD: Self = M::Wood;
        const PLANK: Self = M::Wood;
        const CLOTH: Self = M::Wall;
        const LINEN: Self = M::Wall;
        const FRUIT: Self = M::Wall;
        const RED: Self = M::Wall;
        const IRON: Self = M::Timber;
        const LAMP: Self = M::Wall;
    }

    /// Boxes along a straight wall, as (s0, s1, y0, y1) in the wall's frame.
    fn spans(kit: &Kit<M>) -> Vec<(f32, f32, f32, f32)> {
        kit.pieces
            .iter()
            .map(|p| {
                (
                    p.centre.x - p.size.x / 2.0,
                    p.centre.x + p.size.x / 2.0,
                    p.centre.y - p.size.y / 2.0,
                    p.centre.y + p.size.y / 2.0,
                )
            })
            .collect()
    }

    #[test]
    fn wall_segments_never_overlap_and_holes_stay_open() {
        let mut kit = Kit::<M>::default();
        let holes = [Hole::door(-2.0), Hole::window(1.5)];
        kit.wall(
            Vec2::new(-5.0, 0.0),
            Vec2::new(5.0, 0.0),
            0.0,
            3.0,
            0.25,
            &holes,
            M::Wall,
        );
        let boxes = spans(&kit);
        let area: f32 = boxes.iter().map(|b| (b.1 - b.0) * (b.3 - b.2)).sum();
        let holes_area = DOOR_W * DOOR_H + 1.0 * 1.0;
        assert!((area - (10.0 * 3.0 - holes_area)).abs() < 1e-3, "{area}");
        for (i, a) in boxes.iter().enumerate() {
            for b in &boxes[i + 1..] {
                let ox = a.1.min(b.1) - a.0.max(b.0);
                let oy = a.3.min(b.3) - a.2.max(b.2);
                assert!(ox < 1e-4 || oy < 1e-4, "{a:?} overlaps {b:?}");
            }
        }
        assert_eq!(kit.doors.len(), 1);
        let trims = kit.pieces.iter().filter(|p| p.mat == M::Timber).count();
        assert_eq!(trims, 3, "two lintels and a sill");
    }
}
