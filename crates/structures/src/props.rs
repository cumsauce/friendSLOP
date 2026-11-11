//! A prop library: furniture and detail as small seeded generators that emit
//! kit pieces, plus the placement metadata [`crate::placer`] needs.
//!
//! Every prop is built in a local frame: base centred on the origin at floor
//! height (y = 0), footprint `w` along x and `d` along z, its back at -z and
//! its front (the side you use it from) facing +z. Everything a generator
//! emits stays inside [`PropId::aabb`]. No game code.

use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

use bevy::prelude::*;

use crate::{Kit, Palette, Shape, Solid, furniture};

/// A tiny deterministic generator (SplitMix64), the same on every peer.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x2545_F491_4F6C_DD1D)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `0..1`.
    pub fn f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f32()
    }

    /// Uniform in `0..n` (0 when `n` is 0).
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }

    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            v.swap(i, self.below(i + 1));
        }
    }
}

/// Where a prop wants to stand.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Anchor {
    /// Back against a wall.
    Wall,
    /// Back against a wall, at one end of it.
    Corner,
    /// Anywhere on the floor.
    Centre,
    /// Hanging from the ceiling.
    Ceiling,
    /// On another prop's surface socket (a tabletop, a shelf).
    OnSurface,
}

/// What a prop occupies. Props in the same layer never share floor cells;
/// floor-layer props (rugs, debris) may lie under solid ones.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    /// Flat on the floor, walkable.
    Floor,
    /// Stands on the floor and blocks walking.
    Solid,
    /// Up on a wall or the ceiling, out of the way.
    Air,
}

/// A flat surface small props can stand on, in the prop's local frame:
/// `centre.y` is the surface height, `size` its extent along x and z.
#[derive(Clone, Copy, Debug)]
pub struct Socket {
    pub centre: Vec3,
    pub size: Vec2,
}

/// Placement metadata.
#[derive(Clone, Copy, Debug)]
pub struct PropSpec {
    /// Width (x) and depth (z) in the local frame.
    pub footprint: Vec2,
    /// Height of its box (for air props: how tall the hanging part is).
    pub height: f32,
    pub anchor: Anchor,
    pub layer: Layer,
    /// Floor to keep free in front of it (all around for centre props).
    pub clearance: f32,
    /// Scattered last, as detail.
    pub detail: bool,
    pub tags: &'static [&'static str],
}

/// Small seeded parameters for one prop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Params {
    pub seed: u64,
    /// 0.85..1.0: how much of its footprint the body fills.
    pub scale: f32,
    /// 0..1: how battered it looks (gaps, missing bits, tilt).
    pub wear: f32,
    /// Colour / shape variant.
    pub variant: u8,
    /// Floor to ceiling, for hanging props.
    pub reach: f32,
}

impl Params {
    pub fn roll(seed: u64, reach: f32) -> Params {
        let mut r = Rng::new(seed);
        Params {
            seed,
            scale: r.range(0.85, 1.0),
            wear: r.f32(),
            variant: (r.next_u64() % 4) as u8,
            reach,
        }
    }
}

impl Default for Params {
    fn default() -> Self {
        Params {
            seed: 0,
            scale: 1.0,
            wear: 0.0,
            variant: 0,
            reach: 3.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PropId {
    Bed,
    Table,
    Crates,
    Shelf,
    Chair,
    Stool,
    Counter,
    Barrel,
    Cupboard,
    Rug,
    WallLamp,
    HangingLamp,
    PipeRun,
    Cobweb,
    Debris,
    Books,
    CupPlate,
    Bucket,
    Sack,
}

const fn spec(
    w: f32,
    d: f32,
    height: f32,
    anchor: Anchor,
    layer: Layer,
    clearance: f32,
    detail: bool,
    tags: &'static [&'static str],
) -> PropSpec {
    PropSpec {
        footprint: Vec2::new(w, d),
        height,
        anchor,
        layer,
        clearance,
        detail,
        tags,
    }
}

/// Height the hanging lamp drops from the ceiling.
const LAMP_DROP: f32 = 1.06;
const WALL_LAMP_Y: f32 = 1.7;

impl PropId {
    pub const ALL: [PropId; 19] = [
        PropId::Bed,
        PropId::Table,
        PropId::Crates,
        PropId::Shelf,
        PropId::Chair,
        PropId::Stool,
        PropId::Counter,
        PropId::Barrel,
        PropId::Cupboard,
        PropId::Rug,
        PropId::WallLamp,
        PropId::HangingLamp,
        PropId::PipeRun,
        PropId::Cobweb,
        PropId::Debris,
        PropId::Books,
        PropId::CupPlate,
        PropId::Bucket,
        PropId::Sack,
    ];

    pub fn name(self) -> &'static str {
        match self {
            PropId::Bed => "bed",
            PropId::Table => "table",
            PropId::Crates => "crates",
            PropId::Shelf => "shelf",
            PropId::Chair => "chair",
            PropId::Stool => "stool",
            PropId::Counter => "counter",
            PropId::Barrel => "barrel",
            PropId::Cupboard => "cupboard",
            PropId::Rug => "rug",
            PropId::WallLamp => "wall_lamp",
            PropId::HangingLamp => "hanging_lamp",
            PropId::PipeRun => "pipe_run",
            PropId::Cobweb => "cobweb",
            PropId::Debris => "debris",
            PropId::Books => "books",
            PropId::CupPlate => "cup_plate",
            PropId::Bucket => "bucket",
            PropId::Sack => "sack",
        }
    }

    pub fn from_name(name: &str) -> Option<PropId> {
        PropId::ALL.into_iter().find(|p| p.name() == name)
    }

    pub fn spec(self) -> PropSpec {
        use Anchor::*;
        use Layer::{Air, Floor, Solid as S};
        match self {
            PropId::Bed => spec(1.0, 2.0, 0.7, Wall, S, 0.6, false, &["bedroom", "cosy"]),
            PropId::Table => spec(
                1.4,
                0.9,
                0.95,
                Centre,
                S,
                0.3,
                false,
                &["kitchen", "cosy", "shop"],
            ),
            PropId::Crates => spec(
                0.85,
                0.85,
                1.45,
                Corner,
                S,
                0.3,
                false,
                &["storage", "shop", "sewer"],
            ),
            PropId::Shelf => spec(
                1.2,
                0.4,
                1.8,
                Wall,
                S,
                0.6,
                false,
                &["storage", "shop", "bedroom"],
            ),
            PropId::Chair => spec(
                0.45,
                0.45,
                0.9,
                Centre,
                S,
                0.2,
                false,
                &["kitchen", "cosy", "bedroom"],
            ),
            PropId::Stool => spec(
                0.35,
                0.35,
                0.45,
                Centre,
                S,
                0.2,
                false,
                &["kitchen", "sewer", "shop"],
            ),
            PropId::Counter => spec(1.85, 0.62, 0.95, Wall, S, 0.8, false, &["kitchen", "shop"]),
            PropId::Barrel => spec(
                0.62,
                0.62,
                0.9,
                Corner,
                S,
                0.3,
                false,
                &["storage", "sewer", "kitchen"],
            ),
            PropId::Cupboard => spec(
                1.0,
                0.55,
                1.9,
                Wall,
                S,
                0.7,
                false,
                &["kitchen", "bedroom", "storage"],
            ),
            PropId::Rug => spec(
                1.6,
                1.0,
                0.03,
                Centre,
                Floor,
                0.0,
                false,
                &["bedroom", "cosy", "hallway"],
            ),
            PropId::WallLamp => spec(
                0.25,
                0.25,
                0.25,
                Wall,
                Air,
                0.0,
                false,
                &["hallway", "cosy", "bedroom", "shop"],
            ),
            PropId::HangingLamp => spec(
                0.32,
                0.32,
                LAMP_DROP,
                Ceiling,
                Air,
                0.0,
                false,
                &["kitchen", "shop", "storage"],
            ),
            PropId::PipeRun => spec(2.0, 0.2, 0.34, Wall, Air, 0.0, false, &["sewer", "storage"]),
            PropId::Cobweb => spec(0.6, 0.6, 0.5, Corner, Air, 0.0, true, &["sewer", "storage"]),
            PropId::Debris => spec(
                0.6,
                0.6,
                0.12,
                Centre,
                Floor,
                0.0,
                true,
                &["sewer", "storage"],
            ),
            PropId::Books => spec(
                0.3,
                0.2,
                0.25,
                OnSurface,
                S,
                0.0,
                false,
                &["bedroom", "cosy", "shop"],
            ),
            PropId::CupPlate => spec(
                0.24,
                0.24,
                0.12,
                OnSurface,
                S,
                0.0,
                false,
                &["kitchen", "cosy"],
            ),
            PropId::Bucket => spec(
                0.36,
                0.36,
                0.42,
                Centre,
                S,
                0.0,
                true,
                &["sewer", "kitchen", "storage"],
            ),
            PropId::Sack => spec(
                0.55,
                0.45,
                0.8,
                Wall,
                S,
                0.3,
                false,
                &["storage", "shop", "kitchen"],
            ),
        }
    }

    /// Lowest point above the floor (air props hang high).
    pub fn lift(self, p: &Params) -> f32 {
        match self {
            PropId::WallLamp => WALL_LAMP_Y - 0.11,
            PropId::HangingLamp | PropId::PipeRun | PropId::Cobweb => {
                p.reach - self.spec().height - 0.02
            }
            _ => 0.0,
        }
    }

    /// The local box everything it emits fits in.
    pub fn aabb(self, p: &Params) -> (Vec3, Vec3) {
        let s = self.spec();
        let (hw, hd) = (s.footprint.x / 2.0, s.footprint.y / 2.0);
        let y0 = self.lift(p);
        (Vec3::new(-hw, y0, -hd), Vec3::new(hw, y0 + s.height, hd))
    }

    /// The surfaces it offers small props, in its local frame.
    pub fn sockets(self, p: &Params) -> Vec<Socket> {
        let at = |x, y, z, w, d| Socket {
            centre: Vec3::new(x, y, z),
            size: Vec2::new(w, d),
        };
        match self {
            // Left of the fruit bowl.
            PropId::Table => vec![at(-0.35, 0.78, 0.0, 0.6, 0.8)],
            PropId::Crates if !crates_tall(p) => vec![at(0.0, 0.85, 0.0, 0.8, 0.8)],
            PropId::Shelf => vec![at(0.0, 1.8, 0.0, 1.15, 0.38)],
            PropId::Counter => vec![at(0.0, 0.95, 0.0, 1.75, 0.55)],
            PropId::Barrel => vec![at(0.0, 0.9, 0.0, 0.4, 0.4)],
            PropId::Cupboard => vec![at(0.0, 1.9, 0.0, 0.95, 0.5)],
            _ => Vec::new(),
        }
    }

    /// Emit it into `k` at the kit's current local origin.
    pub fn emit<M: Palette>(self, k: &mut Kit<M>, p: &Params) {
        let wood = if p.variant.is_multiple_of(2) {
            M::WOOD
        } else {
            M::PLANK
        };
        let cloth = [M::CLOTH, M::RED, M::LINEN, M::CLOTH][p.variant as usize % 4];
        let s = p.scale;
        let (name, base, scale) = self.model(p);
        let started = k.begin_prop(name, base, scale);
        match self {
            PropId::Bed => furniture::bed(k, Vec3::ZERO),
            PropId::Table => furniture::table(k, Vec3::ZERO),
            PropId::Crates => furniture::crates(k, Vec3::ZERO, crates_tall(p)),
            PropId::Shelf => furniture::shelf(k, Vec3::ZERO, Vec3::new(1.2, 1.8, 0.4)),
            PropId::Chair => {
                let h = 0.42;
                for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    let c = Vec3::new(sx * 0.18, 0.0, sz * 0.18);
                    k.prop(c, Vec3::new(0.05, h, 0.05), M::TIMBER);
                }
                k.prop(Vec3::Y * h, Vec3::new(0.45, 0.05, 0.45), wood);
                let back = Vec3::new(0.0, h + 0.05, -0.205);
                k.prop(back, Vec3::new(0.4, 0.4 * s + 0.02, 0.04), wood);
            }
            PropId::Stool => {
                let h = 0.45 * s;
                k.put(
                    Shape::Cylinder,
                    Vec3::Y * h / 2.0,
                    Vec3::new(0.35, h, 0.35),
                    Quat::IDENTITY,
                    wood,
                    Solid::Wall,
                    true,
                );
            }
            PropId::Counter => {
                let body = Vec3::new(1.8, 0.9, 0.58);
                k.prop(Vec3::new(0.0, 0.0, -0.02), body, M::WOOD);
                k.prop(Vec3::Y * 0.9, Vec3::new(1.85, 0.05, 0.62), M::PLANK);
                // Drawer fronts, a hair proud of the body.
                for x in [-0.6, 0.0, 0.6] {
                    let c = Vec3::new(x, 0.72, 0.275);
                    k.trinket(Shape::Cube, c, Vec3::new(0.5, 0.14, 0.02), M::TIMBER);
                }
            }
            PropId::Barrel => {
                k.put(
                    Shape::Cylinder,
                    Vec3::Y * 0.45,
                    Vec3::new(0.58, 0.9, 0.58),
                    Quat::IDENTITY,
                    M::WOOD,
                    Solid::Wall,
                    true,
                );
                let hoops: &[f32] = if p.wear > 0.7 { &[0.2] } else { &[0.2, 0.7] };
                for &y in hoops {
                    k.trinket(
                        Shape::Cylinder,
                        Vec3::Y * y,
                        Vec3::new(0.62, 0.04, 0.62),
                        M::IRON,
                    );
                }
            }
            PropId::Cupboard => {
                k.prop(Vec3::new(0.0, 0.0, -0.025), Vec3::new(1.0, 1.9, 0.5), wood);
                for x in [-0.245, 0.245] {
                    let c = Vec3::new(x, 0.95, 0.235);
                    k.trinket(Shape::Cube, c, Vec3::new(0.46, 1.7, 0.02), M::TIMBER);
                    let knob = Vec3::new(x * 0.25, 1.0, 0.255);
                    k.trinket(Shape::Ball, knob, Vec3::splat(0.04), M::IRON);
                }
            }
            PropId::Rug => {
                let size = Vec3::new(1.6 * s, 0.01, 1.0 * s);
                k.trinket(Shape::Cube, Vec3::Y * 0.01, size, cloth);
                let stripe = Vec3::new(1.2 * s, 0.006, 0.2 * s);
                k.trinket(Shape::Cube, Vec3::Y * 0.018, stripe, M::LINEN);
            }
            PropId::WallLamp => {
                let y = WALL_LAMP_Y + 0.1;
                let arm = Vec3::new(0.04, 0.04, 0.17);
                k.trinket(Shape::Cube, Vec3::new(0.0, y, -0.04), arm, M::IRON);
                let glow = Vec3::new(0.0, WALL_LAMP_Y, 0.0);
                k.trinket(Shape::Ball, glow, Vec3::splat(0.2), M::LAMP);
                let l = k.world(glow);
                k.lights.push(l);
            }
            PropId::HangingLamp => {
                // `Kit::lamp` hangs a cord 0.9 m above the bulb's centre.
                let at = Vec3::Y * (p.reach - 0.92);
                k.lamp(at);
                let l = k.world(at);
                k.lights.push(l);
            }
            PropId::PipeRun => {
                let lay = Quat::from_rotation_z(FRAC_PI_2);
                let top = p.reach - 0.02;
                for (i, y) in [top - 0.08, top - 0.26].into_iter().enumerate() {
                    if i == 1 && p.wear > 0.8 {
                        continue;
                    }
                    k.put(
                        Shape::Cylinder,
                        Vec3::new(0.0, y, -0.02),
                        Vec3::new(0.12, 2.0 * s, 0.12),
                        lay,
                        M::IRON,
                        Solid::None,
                        true,
                    );
                }
                for x in [-0.75, 0.75] {
                    let c = Vec3::new(x * s, top - 0.17, -0.09);
                    k.trinket(Shape::Cube, c, Vec3::new(0.05, 0.3, 0.02), M::TIMBER);
                }
            }
            PropId::Cobweb => {
                let top = p.reach - 0.02;
                let c = Vec3::new(0.0, top - 0.2, -0.13);
                k.put(
                    Shape::Cube,
                    c,
                    Vec3::new(0.55 * s, 0.01, 0.4),
                    Quat::from_rotation_x(FRAC_PI_4),
                    M::LINEN,
                    Solid::None,
                    true,
                );
            }
            PropId::Debris => {
                let mut r = Rng::new(p.seed);
                let n = 3 + (p.wear * 4.0) as usize;
                for _ in 0..n {
                    let c = Vec3::new(r.range(-0.22, 0.22), 0.0, r.range(-0.22, 0.22));
                    let size = Vec3::new(
                        r.range(0.04, 0.14),
                        r.range(0.02, 0.08),
                        r.range(0.04, 0.14),
                    );
                    let mat = [M::TIMBER, M::PLANK, M::IRON][r.below(3)];
                    let c = c + Vec3::Y * (size.y / 2.0 + 0.005);
                    if r.f32() < 0.3 {
                        k.trinket(Shape::Ball, c, Vec3::splat(size.y), mat);
                    } else {
                        k.trinket(Shape::Cube, c, size, mat);
                    }
                }
            }
            PropId::Books => {
                let mut r = Rng::new(p.seed);
                let n = 3 + p.variant as usize % 2;
                let w = 0.065;
                let x0 = -(n as f32) * w / 2.0;
                for i in 0..n {
                    let h = r.range(0.17, 0.24);
                    let c = Vec3::new(x0 + w * (i as f32 + 0.5), h / 2.0, 0.0);
                    let mat = [M::RED, M::CLOTH, M::LINEN, M::WOOD][r.below(4)];
                    k.trinket(Shape::Cube, c, Vec3::new(w - 0.008, h, 0.16), mat);
                }
            }
            PropId::CupPlate => {
                let plate = Vec3::new(0.22, 0.02, 0.22);
                k.trinket(Shape::Cylinder, Vec3::Y * 0.01, plate, M::LINEN);
                let cup = Vec3::new(0.08, 0.09, 0.08);
                k.trinket(Shape::Cylinder, Vec3::new(0.03, 0.065, 0.0), cup, M::RED);
            }
            PropId::Bucket => {
                k.put(
                    Shape::Cylinder,
                    Vec3::Y * 0.175,
                    Vec3::new(0.34, 0.35, 0.34),
                    Quat::IDENTITY,
                    M::IRON,
                    Solid::Wall,
                    true,
                );
                let handle = Vec3::new(0.34, 0.02, 0.02);
                k.trinket(Shape::Cube, Vec3::Y * 0.4, handle, M::IRON);
            }
            PropId::Sack => {
                k.prop(Vec3::ZERO, Vec3::new(0.5 * s, 0.5, 0.4), M::CLOTH);
                let top = Vec3::new(0.0, 0.6, 0.0);
                k.trinket(Shape::Ball, top, Vec3::splat(0.32), M::LINEN);
            }
        }
        k.end_prop(started);
    }

    /// The model that draws this prop instead of its pieces
    /// (`models/props/<name>.gltf`): its name, where its origin sits in the
    /// prop's frame, and how much to stretch it. Ceiling props hang from
    /// `reach`; the hanging lamp is marked by [`Kit::lamp`] itself.
    pub fn model(self, p: &Params) -> (&'static str, Vec3, Vec3) {
        let s = p.scale;
        let ceiling = Vec3::Y * p.reach;
        match self {
            PropId::Crates if !crates_tall(p) => ("crates_single", Vec3::ZERO, Vec3::ONE),
            PropId::Stool => (self.name(), Vec3::ZERO, Vec3::new(1.0, s, 1.0)),
            PropId::Rug => (self.name(), Vec3::ZERO, Vec3::new(s, 1.0, s)),
            PropId::Sack => (self.name(), Vec3::ZERO, Vec3::new(s, 1.0, 1.0)),
            PropId::PipeRun | PropId::Cobweb => (self.name(), ceiling, Vec3::new(s, 1.0, 1.0)),
            PropId::HangingLamp => (self.name(), Vec3::Y * (p.reach - 0.92), Vec3::ONE),
            _ => (self.name(), Vec3::ZERO, Vec3::ONE),
        }
    }
}

fn crates_tall(p: &Params) -> bool {
    p.variant.is_multiple_of(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::M;

    /// Every piece a generator emits stays inside its declared box.
    #[test]
    fn props_stay_inside_their_box() {
        for id in PropId::ALL {
            for v in 0..4u8 {
                let p = Params {
                    variant: v,
                    wear: v as f32 / 3.0,
                    seed: v as u64,
                    ..Params::default()
                };
                let mut k = Kit::<M>::default();
                id.emit(&mut k, &p);
                let (lo, hi) = id.aabb(&p);
                for piece in &k.pieces {
                    let half = match piece.shape {
                        Shape::Cylinder => {
                            Vec3::new(piece.size.x, piece.size.y, piece.size.x) / 2.0
                        }
                        _ => piece.size / 2.0,
                    };
                    // Rotated pieces: bound by the rotated box's extent.
                    let ext = (piece.rot * Vec3::new(half.x, 0.0, 0.0)).abs()
                        + (piece.rot * Vec3::new(0.0, half.y, 0.0)).abs()
                        + (piece.rot * Vec3::new(0.0, 0.0, half.z)).abs();
                    let (a, b) = (piece.centre - ext, piece.centre + ext);
                    let e = 0.011;
                    assert!(
                        a.cmpge(lo - e).all() && b.cmple(hi + e).all(),
                        "{} v{v}: {a:?}..{b:?} outside {lo:?}..{hi:?}",
                        id.name()
                    );
                }
                for s in id.sockets(&p) {
                    assert!(s.centre.y <= hi.y + 1e-3, "{} socket above box", id.name());
                }
            }
        }
    }

    /// Every piece a prop draws belongs to its one mark, so a model can
    /// replace all of it.
    #[test]
    fn props_mark_all_their_pieces() {
        for id in PropId::ALL {
            for v in 0..4u8 {
                let p = Params {
                    variant: v,
                    reach: 2.8,
                    ..Params::default()
                };
                let mut k = Kit::<M>::default();
                id.emit(&mut k, &p);
                assert_eq!(k.props.len(), 1, "{}", id.name());
                assert!(k.pieces.iter().all(|piece| piece.prop == Some(0)));
                let dropped = crate::build::drawn(&k.pieces, &k.props, |_| true);
                assert!(dropped.is_empty(), "{}", id.name());
            }
        }
    }

    #[test]
    fn names_round_trip() {
        for id in PropId::ALL {
            assert_eq!(PropId::from_name(id.name()), Some(id));
        }
    }
}
