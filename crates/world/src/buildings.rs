//! The east village district: enterable cottages and two-storey houses with
//! stairs and balconies, a tall watchtower with zig-zag ramps up to a lookout,
//! a big barn with a hay loft, a little market square with stalls and a well,
//! and village walls, hedges and narrow alleys to run around and hide in.
//!
//! Everything is built from code (boxes, cylinders and gable prisms) by the
//! pure `layout()` function, so every peer builds exactly the same village.
//! Nothing is replicated. Walls, furniture and cover get static avian3d
//! colliders that block bodies (the host runs the collision); upper floors,
//! landings and ramps go on the `WALKABLE` layer, which `features/fps.rs`
//! stands bodies on instead of blocking them.

use avian3d::prelude::*;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use friendslop_structures::build::{batches, bodies};
use friendslop_structures::furniture::{bed, crates, shelf, table};
pub use friendslop_structures::{Door, Hole, Palette, Ramp, Shape, Solid};

use crate::AppState;
use crate::surfaces::Surfaced;
use crate::terrain;

/// Collision layer of walkable floors and ramps: bodies stand on these rather
/// than bumping into them.
pub const WALKABLE: LayerMask = LayerMask(1 << 3);

/// Outer wall thickness.
const T: f32 = 0.25;
/// Height of one storey.
const STOREY: f32 = 3.0;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_village);
}

/// What a piece is made of (one shared material each).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Mat {
    /// Collider only, nothing drawn.
    Hidden,
    Plaster,
    Rose,
    Sky,
    Timber,
    Stone,
    Cobble,
    Tile,
    TileDark,
    Slate,
    SlateDark,
    Wood,
    Plank,
    BarnRed,
    Hay,
    Hedge,
    Red,
    Linen,
    Blue,
    Water,
    Lamp,
    Iron,
    Fruit,
}

impl Palette for Mat {
    const HIDDEN: Self = Mat::Hidden;
    const TIMBER: Self = Mat::Timber;
    const WOOD: Self = Mat::Wood;
    const PLANK: Self = Mat::Plank;
    const CLOTH: Self = Mat::Blue;
    const LINEN: Self = Mat::Linen;
    const FRUIT: Self = Mat::Fruit;
    const RED: Self = Mat::Red;
    const IRON: Self = Mat::Iron;
    const LAMP: Self = Mat::Lamp;
}

impl Mat {
    fn color(self) -> Color {
        let (r, g, b) = match self {
            Mat::Hidden => (1.0, 1.0, 1.0),
            Mat::Plaster => (0.94, 0.88, 0.74),
            Mat::Rose => (0.93, 0.76, 0.7),
            Mat::Sky => (0.78, 0.85, 0.9),
            Mat::Timber => (0.36, 0.24, 0.15),
            Mat::Stone => (0.63, 0.61, 0.57),
            Mat::Cobble => (0.52, 0.5, 0.47),
            Mat::Tile => (0.74, 0.34, 0.22),
            Mat::TileDark => (0.6, 0.26, 0.18),
            Mat::Slate => (0.38, 0.42, 0.5),
            Mat::SlateDark => (0.3, 0.33, 0.4),
            Mat::Wood => (0.6, 0.42, 0.25),
            Mat::Plank => (0.52, 0.38, 0.26),
            Mat::BarnRed => (0.64, 0.22, 0.17),
            Mat::Hay => (0.9, 0.78, 0.4),
            Mat::Hedge => (0.24, 0.44, 0.2),
            Mat::Red => (0.86, 0.3, 0.3),
            Mat::Linen => (0.95, 0.93, 0.87),
            Mat::Blue => (0.32, 0.5, 0.8),
            Mat::Water => (0.25, 0.45, 0.62),
            Mat::Lamp => (1.0, 0.84, 0.5),
            Mat::Iron => (0.2, 0.2, 0.23),
            Mat::Fruit => (0.95, 0.6, 0.2),
        };
        Color::srgb(r, g, b)
    }
}

/// A village piece that has a collider. Marks the collider as village
/// architecture for the villagers' navmesh (`features/villagers.rs`).
#[derive(Component, Clone, Copy, Debug)]
pub struct Building;

/// Openings for `walls`.
const fn door(c: f32) -> Hole {
    Hole::door(c)
}

const fn window(c: f32) -> Hole {
    Hole::window(c)
}

/// The district's ground level: one height for everything, so floors, walls
/// and ramps always line up (the terrain is flat out here).
pub fn ground() -> f32 {
    terrain::height_at(MARKET_AT.x, MARKET_AT.y)
}

/// The whole district, in world space.
pub type Kit = friendslop_structures::Kit<Mat>;
/// One village piece.
pub type Piece = friendslop_structures::Piece<Mat>;

/// Placing on the district's ground level.
trait Site {
    fn site(&mut self, at: Vec2, quarter_turns: i32);
}

impl Site for Kit {
    fn site(&mut self, at: Vec2, quarter_turns: i32) {
        self.place(Vec3::new(at.x, ground(), at.y), quarter_turns);
    }
}

/// Four walls of a `w` x `d` storey (local, centred), base `y0`.
fn walls(
    k: &mut Kit,
    w: f32,
    d: f32,
    y0: f32,
    h: f32,
    south: &[Hole],
    north: &[Hole],
    east: &[Hole],
    west: &[Hole],
    mat: Mat,
) {
    k.walls(w, d, y0, h, [south, north, east, west], mat);
}

/// A one-storey cottage, door on the local south (+z) wall, back door east.
/// `split` adds an inner wall making two rooms.
fn cottage(k: &mut Kit, w: f32, d: f32, wall: Mat, roof: (Mat, Mat), split: bool) {
    let (hx, hz) = (w / 2.0, d / 2.0);
    let dx = -w * 0.2;
    walls(
        k,
        w,
        d,
        0.0,
        STOREY,
        &[door(dx), window(w * 0.25)],
        &[window(-w * 0.25), window(w * 0.25)],
        &[door(-0.5)],
        &[window(0.0)],
        wall,
    );
    k.block(
        Vec3::new(-hx, 0.0, -hz),
        Vec3::new(hx, 0.03, hz),
        Mat::Plank,
        Solid::None,
    );
    if split {
        // East room behind an inner wall with a doorway.
        let x = w * 0.15;
        k.wall(
            Vec2::new(x, -hz + T / 2.0),
            Vec2::new(x, hz - T / 2.0),
            0.0,
            STOREY,
            0.15,
            &[door(-d * 0.15)],
            wall,
        );
    }
    bed(k, Vec3::new(hx - 0.85, 0.0, -hz + 1.2));
    table(k, Vec3::new(-w * 0.15, 0.0, -d * 0.12));
    shelf(
        k,
        Vec3::new(-hx + 1.1, 0.0, -hz + 0.37),
        Vec3::new(1.4, 1.9, 0.4),
    );
    crates(k, Vec3::new(hx - 0.7, 0.0, hz - 0.7), true);
    // A wardrobe standing out from the west wall: room to squeeze behind.
    k.prop(
        Vec3::new(-hx + 0.95, 0.0, hz * 0.45),
        Vec3::new(0.6, 2.0, 1.1),
        Mat::Timber,
    );
    k.lamp(Vec3::new(-w * 0.15, 2.3, -d * 0.12));
    k.roof(Vec2::ZERO, w, d, STOREY, d * 0.45, roof.0, roof.1, wall);
    // Chimney.
    k.block(
        Vec3::new(hx - 1.2, STOREY, -0.9),
        Vec3::new(hx - 0.6, STOREY + d * 0.45 + 0.8, -0.3),
        Mat::Stone,
        Solid::None,
    );
}

/// A two-storey house: stairs up the north wall, bedrooms upstairs and a
/// balcony over the front (local south, +z) door.
fn townhouse(k: &mut Kit, w: f32, d: f32, wall: Mat, roof: (Mat, Mat)) {
    let (hx, hz) = (w / 2.0, d / 2.0);
    // Ground floor: front door, back door east.
    walls(
        k,
        w,
        d,
        0.0,
        STOREY,
        &[door(-w * 0.22), window(w * 0.25)],
        &[window(-w * 0.3), window(w * 0.3)],
        &[door(0.6)],
        &[window(0.4)],
        wall,
    );
    // Upper floor: balcony door, windows all round.
    walls(
        k,
        w,
        d,
        STOREY,
        STOREY,
        &[door(0.0), window(-w * 0.32), window(w * 0.32)],
        &[window(-w * 0.25), window(w * 0.25)],
        &[window(0.0)],
        &[window(0.0)],
        wall,
    );
    k.block(
        Vec3::new(-hx, 0.0, -hz),
        Vec3::new(hx, 0.03, hz),
        Mat::Plank,
        Solid::None,
    );
    // Stairs along the north wall, rising west -> east.
    let lane = -hz + T / 2.0 + 0.7;
    let (x0, x1) = (-hx + 0.5, -hx + 5.5);
    k.stairs(Vec3::new(x0, 0.0, lane), Vec3::new(x1, STOREY, lane), 1.1);
    // Upper floor around the stairwell, plus a rail along its edge.
    let well = -hz + 1.4;
    let inner = Vec2::new(hx - T / 2.0, hz - T / 2.0);
    k.floor(Vec2::new(-inner.x, well), inner, STOREY, Mat::Plank);
    k.floor(
        Vec2::new(x1, -inner.y),
        Vec2::new(inner.x, well),
        STOREY,
        Mat::Plank,
    );
    k.bar(
        Vec2::new(-inner.x, well + 0.05),
        Vec2::new(x1 - 0.3, well + 0.05),
        STOREY,
        1.0,
        0.08,
        Mat::Timber,
        Solid::Wall,
    );
    // Balcony over the front, with rails.
    let (bx, bz) = (1.7, hz + 1.5);
    // Starts at the wall's outer face so its top isn't coplanar with the wall.
    k.floor(
        Vec2::new(-bx, hz + T / 2.0),
        Vec2::new(bx, bz),
        STOREY,
        Mat::Plank,
    );
    let rail = |k: &mut Kit, a: Vec2, b: Vec2| {
        k.bar(a, b, STOREY, 1.0, 0.1, Mat::Timber, Solid::Wall);
    };
    rail(k, Vec2::new(-bx, bz), Vec2::new(bx, bz));
    rail(k, Vec2::new(-bx, hz + 0.15), Vec2::new(-bx, bz - 0.05));
    rail(k, Vec2::new(bx, hz + 0.15), Vec2::new(bx, bz - 0.05));
    for sx in [-bx + 0.1, bx - 0.1] {
        let c = Vec3::new(sx, 0.0, bz - 0.1);
        k.prop(c, Vec3::new(0.16, STOREY - 0.15, 0.16), Mat::Timber);
    }
    // Downstairs: kitchen table, crates, a shelf.
    table(k, Vec3::new(0.3, 0.0, 0.3));
    crates(k, Vec3::new(hx - 0.7, 0.0, hz - 0.7), true);
    shelf(
        k,
        Vec3::new(-hx + 0.35, 0.0, hz - 1.6),
        Vec3::new(0.4, 1.9, 1.4),
    );
    k.prop(
        Vec3::new(hx - 0.6, 0.0, 1.2),
        Vec3::new(0.6, 0.9, 0.6),
        Mat::Plank,
    );
    // Upstairs: two beds, a wardrobe, a chest.
    bed(k, Vec3::new(-hx + 0.9, STOREY, 0.6));
    bed(k, Vec3::new(hx - 0.9, STOREY, 0.6));
    k.prop(
        Vec3::new(-1.3, STOREY, hz - 1.5),
        Vec3::new(1.1, 2.0, 0.6),
        Mat::Timber,
    );
    k.prop(
        Vec3::new(1.4, STOREY, -0.2),
        Vec3::new(0.9, 0.55, 0.55),
        Mat::Wood,
    );
    k.lamp(Vec3::new(0.3, 2.3, 0.3));
    k.lamp(Vec3::new(0.0, STOREY + 2.3, 0.4));
    k.roof(
        Vec2::ZERO,
        w,
        d,
        STOREY * 2.0,
        d * 0.42,
        roof.0,
        roof.1,
        wall,
    );
    k.block(
        Vec3::new(-hx + 0.6, STOREY * 2.0, 0.5),
        Vec3::new(-hx + 1.2, STOREY * 2.0 + d * 0.42 + 0.9, 1.1),
        Mat::Stone,
        Solid::None,
    );
}

/// Footprint of the watchtower (x by z) and its lookout height.
const TOWER_W: f32 = 9.0;
const TOWER_D: f32 = 4.4;
pub const LOOKOUT_Y: f32 = 16.0;

/// The watchtower: stone skin, four zig-zag ramp flights inside, a parapet
/// lookout on top under a little roof. Entrance on the local west wall.
fn tower(k: &mut Kit) {
    let (hx, hz) = (TOWER_W / 2.0, TOWER_D / 2.0);
    let top = LOOKOUT_Y + 1.1;
    let slits = |c: f32| {
        [4.0, 8.0, 12.0].map(|y| Hole {
            c,
            w: 0.6,
            y0: y + 1.0,
            y1: y + 2.2,
        })
    };
    let mut west = vec![door(hz * 0.5)];
    west.extend(slits(-1.0));
    walls(
        k,
        TOWER_W,
        TOWER_D,
        0.0,
        top,
        &slits(1.5),
        &slits(-1.5),
        &slits(0.0),
        &west,
        Mat::Stone,
    );
    // Spine between the two lanes.
    k.block(
        Vec3::new(-3.0, 0.0, -0.1),
        Vec3::new(3.0, top, 0.1),
        Mat::Stone,
        Solid::Wall,
    );
    let (south, north) = (hz * 0.5 + 0.05, -hz * 0.5 - 0.05);
    let lane_w = hz - 0.1 - T / 2.0;
    let inner = Vec2::new(hx - T / 2.0, hz - T / 2.0);
    for (i, (z, rising_east)) in [(south, true), (north, false), (south, true), (north, false)]
        .into_iter()
        .enumerate()
    {
        let (y0, y1) = (i as f32 * 4.0, (i + 1) as f32 * 4.0);
        let (xa, xb) = if rising_east {
            (-3.0, 3.0)
        } else {
            (3.0, -3.0)
        };
        let mat = if i == 0 { Mat::Hidden } else { Mat::Plank };
        if i == 0 {
            k.stairs(Vec3::new(xa, y0, z), Vec3::new(xb, y1, z), lane_w);
        } else {
            k.ramp(Vec3::new(xa, y0, z), Vec3::new(xb, y1, z), lane_w, mat);
        }
        // Landing at the top of the flight, across both lanes.
        if i < 3 {
            let (l0, l1) = if rising_east {
                (3.0, inner.x)
            } else {
                (-inner.x, -3.0)
            };
            k.floor(
                Vec2::new(l0, -inner.y),
                Vec2::new(l1, inner.y),
                y1,
                Mat::Plank,
            );
        }
    }
    // Lookout: everything except the last flight's stairwell.
    k.floor(
        Vec2::new(-inner.x, -inner.y),
        Vec2::new(-3.0, inner.y),
        LOOKOUT_Y,
        Mat::Plank,
    );
    k.floor(
        Vec2::new(-3.0, 0.1),
        Vec2::new(3.0, inner.y),
        LOOKOUT_Y,
        Mat::Plank,
    );
    k.floor(
        Vec2::new(3.0, -inner.y),
        Vec2::new(inner.x, inner.y),
        LOOKOUT_Y,
        Mat::Plank,
    );
    // Roof on posts.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let c = Vec3::new(sx * hx, top, sz * hz);
        k.block(
            c - Vec3::new(0.15, 0.0, 0.15),
            c + Vec3::new(0.15, 2.4, 0.15),
            Mat::Timber,
            Solid::Wall,
        );
    }
    k.roof(
        Vec2::ZERO,
        TOWER_W,
        TOWER_D,
        top + 2.4,
        2.0,
        Mat::Slate,
        Mat::SlateDark,
        Mat::Timber,
    );
}

/// Barn footprint (x by z) and loft height.
const BARN_W: f32 = 14.0;
const BARN_D: f32 = 12.0;
const LOFT_Y: f32 = 2.8;

/// The big barn: wide door on the local south, a hay loft over the north
/// half reached by stairs along the east wall, hay bales everywhere.
fn barn(k: &mut Kit) {
    let (hx, hz) = (BARN_W / 2.0, BARN_D / 2.0);
    let h = 5.0;
    let big = Hole {
        c: 0.0,
        w: 3.6,
        y0: 0.0,
        y1: 3.6,
    };
    let high = |c: f32| Hole {
        c,
        w: 1.2,
        y0: 3.4,
        y1: 4.3,
    };
    walls(
        k,
        BARN_W,
        BARN_D,
        0.0,
        h,
        &[big, high(-4.5), high(4.5)],
        &[door(-4.0), high(2.0)],
        &[high(-2.0)],
        &[door(2.5), high(-2.5)],
        Mat::BarnRed,
    );
    k.block(
        Vec3::new(-hx, 0.0, -hz),
        Vec3::new(hx, 0.03, hz),
        Mat::Hay,
        Solid::None,
    );
    // Loft over the north half, rail along its edge (gap for the stairs).
    let edge = -0.5;
    let inner = Vec2::new(hx - T / 2.0, hz - T / 2.0);
    k.floor(
        Vec2::new(-inner.x, -inner.y),
        Vec2::new(inner.x, edge),
        LOFT_Y,
        Mat::Plank,
    );
    let lane = inner.x - 0.7;
    k.bar(
        Vec2::new(-inner.x, edge - 0.05),
        Vec2::new(lane - 0.75, edge - 0.05),
        LOFT_Y,
        1.0,
        0.1,
        Mat::Timber,
        Solid::Wall,
    );
    k.stairs(
        Vec3::new(lane, 0.0, 4.5),
        Vec3::new(lane, LOFT_Y, edge),
        1.2,
    );
    // Loft posts.
    for x in [-4.0, 0.0, 3.5] {
        k.prop(
            Vec3::new(x, 0.0, edge - 0.2),
            Vec3::new(0.25, LOFT_Y - 0.15, 0.25),
            Mat::Timber,
        );
    }
    // Hay bales: stacks on the floor, more up in the loft.
    let bale = Vec3::new(1.2, 0.75, 0.8);
    for (x, z, n) in [
        (-5.5, 3.5, 2),
        (-4.2, 3.5, 1),
        (-5.5, -2.0, 3),
        (-2.0, -3.5, 2),
        (2.0, 2.0, 1),
        (3.0, -3.5, 2),
    ] {
        for i in 0..n {
            k.prop(Vec3::new(x, bale.y * i as f32, z), bale, Mat::Hay);
        }
    }
    for (x, z, n) in [
        (-5.0, -4.5, 2),
        (-2.5, -2.0, 1),
        (1.0, -4.5, 2),
        (4.0, -2.5, 1),
    ] {
        for i in 0..n {
            k.prop(Vec3::new(x, LOFT_Y + bale.y * i as f32, z), bale, Mat::Hay);
        }
    }
    // A cart.
    k.prop(
        Vec3::new(-1.5, 0.0, 1.0),
        Vec3::new(1.4, 0.8, 2.2),
        Mat::Wood,
    );
    // A tie beam across the barn for the lamp's cord to hang from.
    k.bar(
        Vec2::new(-hx, 1.5),
        Vec2::new(hx, 1.5),
        5.1,
        0.25,
        0.25,
        Mat::Timber,
        Solid::None,
    );
    k.lights.push(k.world(Vec3::new(0.0, 4.2, 1.5)));
    k.lamp(Vec3::new(0.0, 4.2, 1.5));
    k.roof(
        Vec2::ZERO,
        BARN_W,
        BARN_D,
        h,
        4.5,
        Mat::Slate,
        Mat::SlateDark,
        Mat::BarnRed,
    );
}

/// A market stall facing local +z: counter, posts and a striped canopy.
fn stall(k: &mut Kit, cloth: Mat) {
    k.prop(
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(2.4, 0.95, 0.8),
        Mat::Wood,
    );
    // Posts stand just outside the counter's ends (touching, not inside).
    for (x, z) in [(-1.25, 0.37), (1.25, 0.37), (-1.25, -1.3), (1.25, -1.3)] {
        k.prop(Vec3::new(x, 0.0, z), Vec3::new(0.1, 2.3, 0.1), Mat::Timber);
    }
    for i in 0..4 {
        let x = -1.05 + 0.7 * i as f32;
        let mat = if i % 2 == 0 { cloth } else { Mat::Linen };
        let c = Vec3::new(x, 2.45, -0.45);
        let rot = Quat::from_rotation_x(-0.18);
        k.put(
            Shape::Cube,
            c,
            Vec3::new(0.7, 0.05, 2.3),
            rot,
            mat,
            Solid::None,
            true,
        );
    }
    for i in 0..5 {
        let x = -0.9 + 0.45 * i as f32;
        let mat = [Mat::Fruit, Mat::Red, Mat::Hedge, Mat::Fruit, Mat::Hay][i];
        k.trinket(
            Shape::Ball,
            Vec3::new(x, 1.07, 0.05),
            Vec3::splat(0.22),
            mat,
        );
    }
}

/// The well: stone ring, water, two posts and a little roof.
fn well(k: &mut Kit) {
    k.put(
        Shape::Cylinder,
        Vec3::Y * 0.45,
        Vec3::new(2.0, 0.9, 2.0),
        Quat::IDENTITY,
        Mat::Stone,
        Solid::Wall,
        false,
    );
    k.trinket(
        Shape::Cylinder,
        Vec3::Y * 0.92,
        Vec3::new(1.6, 0.04, 1.6),
        Mat::Water,
    );
    for x in [-0.9, 0.9] {
        k.prop(
            Vec3::new(x, 0.9, 0.0),
            Vec3::new(0.14, 1.6, 0.14),
            Mat::Timber,
        );
    }
    k.trinket(
        Shape::Cube,
        Vec3::Y * 2.1,
        Vec3::new(2.0, 0.08, 0.08),
        Mat::Timber,
    );
    k.trinket(
        Shape::Cube,
        Vec3::Y * 1.6,
        Vec3::new(0.3, 0.35, 0.3),
        Mat::Wood,
    );
    k.roof(
        Vec2::ZERO,
        1.8,
        1.4,
        2.5,
        0.7,
        Mat::Tile,
        Mat::TileDark,
        Mat::Timber,
    );
}

/// Where the district's market square sits (well in the middle).
pub const MARKET_AT: Vec2 = Vec2::new(57.0, 0.0);
/// Where the watchtower stands.
pub const TOWER_AT: Vec2 = Vec2::new(72.0, 16.0);
const BARN_AT: Vec2 = Vec2::new(70.5, -24.0);

/// Build the whole district (pure, deterministic).
pub fn layout() -> Kit {
    let mut k = Kit::default();
    let tiles = (Mat::Tile, Mat::TileDark);
    let slate = (Mat::Slate, Mat::SlateDark);

    // North row, doors facing the street (+z).
    k.site(Vec2::new(40.0, -11.0), 0);
    cottage(&mut k, 7.0, 6.0, Mat::Plaster, tiles, false);
    k.site(Vec2::new(49.5, -13.0), 0);
    townhouse(&mut k, 8.0, 7.0, Mat::Rose, slate);
    k.lights.push(k.world(Vec3::new(0.3, 2.5, 0.3)));
    k.site(Vec2::new(58.0, -13.0), 0);
    cottage(&mut k, 6.0, 6.0, Mat::Sky, tiles, false);
    k.site(Vec2::new(66.5, -11.0), 0);
    cottage(&mut k, 9.0, 6.0, Mat::Plaster, slate, true);
    // South row, turned to face the street (-z).
    k.site(Vec2::new(42.0, 13.0), 2);
    townhouse(&mut k, 8.0, 7.0, Mat::Plaster, tiles);
    k.site(Vec2::new(51.5, 12.5), 2);
    cottage(&mut k, 6.0, 5.0, Mat::Rose, tiles, false);
    k.site(Vec2::new(60.0, 13.0), 2);
    cottage(&mut k, 7.0, 6.0, Mat::Sky, slate, true);

    k.site(BARN_AT, 0);
    barn(&mut k);
    k.site(TOWER_AT, 0);
    tower(&mut k);

    // Market square: cobbles, the well, four stalls, a lantern.
    k.site(MARKET_AT, 0);
    k.block(
        // Just inside the low walls at its edge, so their faces don't coincide.
        Vec3::new(-6.97, 0.0, -6.97),
        Vec3::new(6.97, 0.03, 6.97),
        Mat::Cobble,
        Solid::None,
    );
    well(&mut k);
    for (x, z, turns, cloth) in [
        (-4.5, -5.0, 0, Mat::Red),
        (4.5, -5.0, 0, Mat::Blue),
        (-4.5, 5.0, 2, Mat::Blue),
        (4.5, 5.0, 2, Mat::Red),
    ] {
        k.site(MARKET_AT + Vec2::new(x, z), turns);
        stall(&mut k, cloth);
    }
    k.site(MARKET_AT, 0);
    k.prop(
        Vec3::new(-3.2, 0.0, 2.6),
        Vec3::new(0.15, 3.0, 0.15),
        Mat::Iron,
    );
    k.lamp(Vec3::new(-3.2, 3.2, 2.6));
    k.lights.push(k.world(Vec3::new(-3.2, 3.2, 2.6)));

    // Everything else in world coordinates.
    k.site(Vec2::ZERO, 0);
    village_grounds(&mut k);
    k
}

/// Village walls with gates, hedges, low walls, a paddock fence and clutter.
fn village_grounds(k: &mut Kit) {
    let v = Vec2::new;
    let gate = |c: f32| Hole {
        c,
        w: 2.6,
        y0: 0.0,
        y1: 9.0,
    };
    let (h, t) = (2.4, 0.5);
    // North and south walls run 36..79 (centre 57.5), gates in each.
    k.wall(
        v(36.0, -34.0),
        v(79.0, -34.0),
        0.0,
        h,
        t,
        &[gate(-6.0)],
        Mat::Stone,
    );
    k.wall(
        v(36.0, 34.0),
        v(79.0, 34.0),
        0.0,
        h,
        t,
        &[gate(-1.0), gate(12.0)],
        Mat::Stone,
    );
    // East wall with a gate on the street line.
    k.wall(
        v(79.0, -33.75),
        v(79.0, 33.75),
        0.0,
        h,
        t,
        &[gate(0.0)],
        Mat::Stone,
    );
    // West wall, wide open where the path comes in.
    // Both stop where the north/south walls and the gate pillars begin.
    k.wall(v(36.0, -33.75), v(36.0, -5.4), 0.0, h, t, &[], Mat::Stone);
    k.wall(v(36.0, 5.4), v(36.0, 33.75), 0.0, h, t, &[], Mat::Stone);
    for z in [-5.0, 5.0] {
        k.block(
            Vec3::new(35.6, 0.0, z - 0.4),
            Vec3::new(36.4, 3.2, z + 0.4),
            Mat::Stone,
            Solid::Wall,
        );
    }
    // Hedges behind the houses, with gaps.
    let hedge = |k: &mut Kit, a: Vec2, b: Vec2| k.bar(a, b, 0.0, 1.7, 0.9, Mat::Hedge, Solid::Wall);
    hedge(k, v(37.5, -21.0), v(45.0, -21.0));
    hedge(k, v(47.0, -21.0), v(62.0, -21.0));
    hedge(k, v(38.0, 21.0), v(49.0, 21.0));
    hedge(k, v(51.0, 21.0), v(63.0, 21.0));
    hedge(k, v(40.0, 26.0), v(40.0, 32.0));
    hedge(k, v(54.0, -27.0), v(54.0, -32.0));
    // Low walls framing the market's corners.
    let low = |k: &mut Kit, a: Vec2, b: Vec2| k.bar(a, b, 0.0, 0.9, 0.45, Mat::Stone, Solid::Wall);
    for sx in [-1.0, 1.0] {
        for sz in [-1.0, 1.0] {
            let c = MARKET_AT + v(sx * 7.0, sz * 7.0);
            low(k, c + v(sx * 0.225, 0.0), c - v(sx * 3.0, 0.0));
            low(k, c - v(0.0, sz * 0.225), c - v(0.0, sz * 3.0));
        }
    }
    // A paddock fence by the watchtower.
    let fence = |k: &mut Kit, a: Vec2, b: Vec2, first: bool| {
        k.bar(a, b, 0.0, 1.1, 0.12, Mat::Hidden, Solid::Wall);
        for y in [0.45, 0.9] {
            k.bar(a, b, y, 0.1, 0.08, Mat::Wood, Solid::None);
        }
        let n = ((a.distance(b) / 2.0).ceil() as usize).max(1);
        for i in (if first { 0 } else { 1 })..=n {
            let p = a.lerp(b, i as f32 / n as f32);
            k.prop(
                Vec3::new(p.x, 0.0, p.y),
                Vec3::new(0.14, 1.2, 0.14),
                Mat::Timber,
            );
        }
    };
    fence(k, v(65.0, 21.0), v(65.0, 30.0), true);
    fence(k, v(65.0, 30.0), v(77.0, 30.0), false);
    fence(k, v(77.0, 30.0), v(77.0, 24.0), false);
    // Crates and bales scattered in the alleys and yards.
    for (x, z, tall) in [
        (44.5, -15.0, true),
        (54.3, -15.5, false),
        (47.0, 18.5, true),
        (66.0, 24.0, false),
        (75.0, 6.0, true),
        (41.0, 28.0, false),
        (59.0, 27.0, true),
        (75.5, -9.0, false),
        (38.5, -28.0, true),
        (62.0, -32.2, false),
        (46.5, 6.5, false),
        (68.5, -5.5, true),
    ] {
        crates(k, Vec3::new(x, 0.0, z), tall);
    }
    let bale = Vec3::new(1.2, 0.75, 0.8);
    for (x, z) in [(61.0, -24.0), (61.0, -25.5), (70.0, 26.0), (45.0, 28.0)] {
        k.prop(Vec3::new(x, 0.0, z), bale, Mat::Hay);
    }
}

// ---------------------------------------------------------------- spawning

fn spawn_village(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let kit = layout();
    let mut mats: Vec<(Mat, Handle<StandardMaterial>)> = Vec::new();
    // One merged mesh per house and material, with UVs already in world
    // metres and tangents, so `surfaces` leaves it alone (`Surfaced`).
    for batch in batches(&kit.pieces) {
        let mat = match mats.iter().find(|(m, _)| *m == batch.mat) {
            Some((_, h)) => h.clone(),
            None => {
                let h = material(&mut materials, batch.mat);
                mats.push((batch.mat, h.clone()));
                h
            }
        };
        let mut e = commands.spawn((
            Transform::from_translation(batch.origin),
            Visibility::default(),
            Mesh3d(meshes.add(batch.mesh)),
            MeshMaterial3d(mat),
            Surfaced,
            DespawnOnExit(AppState::InGame),
        ));
        if batch.small || batch.mat == Mat::Lamp {
            e.insert(NotShadowCaster);
        }
    }
    for body in bodies(&kit.pieces) {
        // Village architecture stays in the villagers' navmesh (see
        // features/villagers.rs); scattered nature is excluded instead.
        let mut e = commands.spawn((
            Transform::from_translation(body.centre).with_rotation(body.rot),
            RigidBody::Static,
            body.collider,
            Building,
            DespawnOnExit(AppState::InGame),
        ));
        if body.solid == Solid::Floor {
            e.insert(CollisionLayers::new(WALKABLE, LayerMask::ALL));
        }
    }
    for at in kit.lights {
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.75, 0.45),
                intensity: 30_000.0,
                range: 11.0,
                radius: 0.15,
                shadow_maps_enabled: false,
                ..default()
            },
            Transform::from_translation(at - Vec3::Y * 0.3),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

fn material(materials: &mut Assets<StandardMaterial>, mat: Mat) -> Handle<StandardMaterial> {
    match mat {
        Mat::Lamp => materials.add(StandardMaterial {
            base_color: mat.color(),
            emissive: LinearRgba::rgb(4.0, 2.8, 1.4),
            ..default()
        }),
        Mat::Water => crate::art::glossy(materials, mat.color()),
        _ => crate::art::soft(materials, mat.color()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// World-space AABB of a piece (exact for axis turns, conservative else).
    fn aabb(p: &Piece) -> (Vec3, Vec3) {
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        let half = p.size / 2.0;
        let base = if p.shape == Shape::Gable {
            Vec3::new(0.0, half.y, 0.0)
        } else {
            Vec3::ZERO
        };
        for sx in [-1.0, 1.0] {
            for sy in [-1.0, 1.0] {
                for sz in [-1.0, 1.0] {
                    let c = p.centre + p.rot * (base + half * Vec3::new(sx, sy, sz));
                    lo = lo.min(c);
                    hi = hi.max(c);
                }
            }
        }
        (lo, hi)
    }

    fn overlaps(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> bool {
        a.0.cmplt(b.1).all() && b.0.cmplt(a.1).all()
    }

    /// Solid (blocking) pieces hit by a body-sized box.
    fn blockers(kit: &Kit, probe: (Vec3, Vec3)) -> Vec<Piece> {
        kit.pieces
            .iter()
            .filter(|p| p.solid == Solid::Wall && overlaps(aabb(p), probe))
            .copied()
            .collect()
    }

    /// The body capsule (radius 0.35, 0.1..1.8 above the feet) as a box.
    fn body_at(feet: Vec3, along: Vec3, depth: f32) -> (Vec3, Vec3) {
        let side = Vec3::Y.cross(along).normalize().abs() * 0.36;
        let fwd = along.normalize().abs() * depth / 2.0;
        let lo = feet - side - fwd + Vec3::Y * 0.1;
        let hi = feet + side + fwd + Vec3::Y * 1.8;
        (lo, hi)
    }

    #[test]
    fn layout_is_deterministic_and_in_the_east_zone() {
        let (a, b) = (layout(), layout());
        assert_eq!(a.pieces.len(), b.pieces.len());
        for (p, q) in a.pieces.iter().zip(&b.pieces) {
            assert_eq!(p.centre, q.centre);
        }
        for p in &a.pieces {
            let (lo, hi) = aabb(p);
            assert!(lo.x > 34.5 && hi.x < 80.0, "{p:?} outside x");
            assert!(lo.z > -35.0 && hi.z < 35.0, "{p:?} outside z");
        }
        assert!(a.lights.len() <= 3, "at most 3 new point lights");
    }

    #[test]
    fn every_door_lets_a_body_through() {
        let kit = layout();
        assert!(kit.doors.len() >= 20, "only {} doors", kit.doors.len());
        for door in &kit.doors {
            assert!(door.width >= 1.2 && door.height >= 2.2, "{door:?}");
            let probe = body_at(door.centre, door.through, 1.6);
            let hit = blockers(&kit, probe);
            assert!(hit.is_empty(), "door {door:?} blocked by {hit:?}");
        }
    }

    #[test]
    fn ramps_are_gentle_clear_and_reach_a_floor() {
        let kit = layout();
        assert!(kit.ramps.len() >= 7, "{} ramps", kit.ramps.len());
        let floors: Vec<_> = kit
            .pieces
            .iter()
            .filter(|p| p.solid == Solid::Floor && (p.rot * Vec3::Y).y > 0.999)
            .map(aabb)
            .collect();
        let on_floor = |p: Vec3| {
            p.y < ground() + 0.05
                || floors.iter().any(|(lo, hi)| {
                    (hi.y - p.y).abs() < 0.05
                        && p.x > lo.x - 0.15
                        && p.x < hi.x + 0.15
                        && p.z > lo.z - 0.15
                        && p.z < hi.z + 0.15
                })
        };
        for ramp in &kit.ramps {
            let d = ramp.to - ramp.from;
            let slope = d.y.atan2(d.with_y(0.0).length()).to_degrees();
            assert!(slope < 36.0, "{ramp:?} too steep ({slope})");
            assert!(ramp.width >= 1.0, "{ramp:?} too narrow");
            assert!(on_floor(ramp.from) && on_floor(ramp.to), "{ramp:?} dangles");
            for i in 1..20 {
                let feet = ramp.from + d * i as f32 / 20.0;
                let probe = body_at(feet, d.with_y(0.0), 0.7);
                let hit = blockers(&kit, probe);
                assert!(hit.is_empty(), "ramp {ramp:?} at {feet} blocked by {hit:?}");
            }
        }
    }

    #[test]
    fn there_is_a_tall_lookout_and_upper_floors() {
        let kit = layout();
        let top = kit
            .ramps
            .iter()
            .map(|r| r.to.y.max(r.from.y))
            .fold(0.0, f32::max)
            - ground();
        assert!((15.0..=18.0).contains(&top), "lookout at {top}");
        let upper = kit
            .ramps
            .iter()
            .filter(|r| (r.to.y - r.from.y - STOREY).abs() < 0.01)
            .count();
        assert!(upper >= 2, "need two two-storey houses");
    }

    /// Solid boxes may touch but never overlap: walls are split around their
    /// holes, and lintels and sills only fill the hole. Checks drawn,
    /// axis-aligned architecture and furniture (hidden helper colliders,
    /// like the solid blocks under stairs, are allowed inside things).
    #[test]
    fn solid_boxes_never_overlap() {
        let kit = layout();
        let axis_aligned = |p: &Piece| {
            let m = Mat3::from_quat(p.rot);
            [m.x_axis, m.y_axis, m.z_axis]
                .iter()
                .all(|a| a.abs().max_element() > 0.9999)
        };
        let boxes: Vec<_> = kit
            .pieces
            .iter()
            .filter(|p| {
                p.shape == Shape::Cube
                    && p.solid != Solid::None
                    && p.mat != Mat::Hidden
                    && axis_aligned(p)
            })
            .map(|p| (p, aabb(p)))
            .collect();
        let mut bad = Vec::new();
        for (i, (p, a)) in boxes.iter().enumerate() {
            for (q, b) in &boxes[i + 1..] {
                let depth = (a.1.min(b.1) - a.0.max(b.0)).min_element();
                if depth > 2e-3 {
                    bad.push(format!(
                        "{:?} {:?} {:?} vs {:?} {:?} {:?} ({depth})",
                        p.mat, p.centre, p.size, q.mat, q.centre, q.size
                    ));
                }
            }
        }
        assert!(
            bad.is_empty(),
            "{} overlaps:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }

    /// Two boxes of different materials whose same-facing faces lie in one
    /// plane and overlap flicker (z-fighting). Checks axis-aligned cubes.
    #[test]
    fn no_coplanar_faces_between_materials() {
        let kit = layout();
        let axis_aligned = |p: &Piece| {
            let m = Mat3::from_quat(p.rot);
            [m.x_axis, m.y_axis, m.z_axis]
                .iter()
                .all(|a| a.abs().max_element() > 0.999)
        };
        let boxes: Vec<_> = kit
            .pieces
            .iter()
            .filter(|p| p.shape == Shape::Cube && p.mat != Mat::Hidden && axis_aligned(p))
            .map(|p| (p, aabb(p)))
            .collect();
        let span = |lo: f32, hi: f32, lo2: f32, hi2: f32| hi.min(hi2) - lo.max(lo2);
        let mut bad = Vec::new();
        for (i, (p, a)) in boxes.iter().enumerate() {
            for (q, b) in &boxes[i + 1..] {
                if p.mat == q.mat {
                    continue;
                }
                for ax in 0..3 {
                    let (u, v) = ((ax + 1) % 3, (ax + 2) % 3);
                    let area = span(a.0[u], a.1[u], b.0[u], b.1[u]) > 1e-3
                        && span(a.0[v], a.1[v], b.0[v], b.1[v]) > 1e-3;
                    // Undersides are never seen (they sit on the ground or a floor).
                    let lo = ax != 1 && (a.0[ax] - b.0[ax]).abs() < 2e-3;
                    let same = lo || (a.1[ax] - b.1[ax]).abs() < 2e-3;
                    if area && same {
                        bad.push(format!(
                            "{:?} {:?} {:?} vs {:?} {:?} {:?} (axis {ax})",
                            p.mat, p.centre, p.size, q.mat, q.centre, q.size
                        ));
                    }
                }
            }
        }
        assert!(
            bad.is_empty(),
            "{} coplanar faces:\n{}",
            bad.len(),
            bad.join("\n")
        );
    }
}
