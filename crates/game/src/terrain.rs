//! The ground: a generated, vertex-coloured, chunked mesh over a 180 m play
//! area (±90 m) with hills beyond it. The 50×50 commune square at the centre
//! and the east village district are dead flat (y = 0); the north forest
//! rolls gently, the west valley has a climbable 12 m hill, a ridge and a
//! lake, and the south-west ruins have a sunken quarry pit. Worn dirt paths
//! link the commune's landmarks and run out to every district. Fully
//! deterministic, so every peer sees the same ground.
//!
//! Scenery can ask [`height_at`], [`is_path`], [`is_pond`] and [`is_lake`]
//! where to put things.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::AppState;
use crate::art;
use crate::features::cat_palette::cat_palette;
use crate::features::dave_commune::PARLIAMENT_AT;
use crate::features::duel::{ARENA_CENTER, ARENA_HALF};

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_terrain);
    }
}

/// Half the side of the flat commune square at the centre.
pub const FLAT_HALF: f32 = 25.0;
/// Half the side of the walkable world (the soft wall is a little inside it).
pub const PLAY_HALF: f32 = 90.0;
/// Half the side of the whole generated ground (edge hills beyond the play area).
const WORLD_HALF: f32 = 110.0;
/// The ground is split into CHUNKS × CHUNKS meshes so off-screen parts are culled.
const CHUNKS: u32 = 8;
/// Cells per chunk side: ~1.06 m cells, ~47k vertices for the whole ground.
const CHUNK_CELLS: u32 = 26;
/// Heightfield collider spacing (cells per side): 2 m.
const COLLIDER_CELLS: usize = 110;

/// Where the pond sits and how big it is (in open meadow, off every path).
pub const POND_AT: Vec2 = Vec2::new(13.0, 9.0);
const POND_RADIUS: f32 = 3.2;

/// The east village district: dead flat so houses can stand anywhere in it.
pub const VILLAGE_MIN: Vec2 = Vec2::new(35.0, -35.0);
pub const VILLAGE_MAX: Vec2 = Vec2::new(80.0, 35.0);

/// The west valley: a climbable hill, a ridge along the edge and a lake.
pub const HILL_AT: Vec2 = Vec2::new(-62.0, -6.0);
const HILL_HEIGHT: f32 = 12.0;
const HILL_RADIUS: f32 = 24.0;
pub const LAKE_AT: Vec2 = Vec2::new(-44.0, 20.0);
const LAKE_RADIUS: f32 = 7.0;
/// Height of the lake's water surface.
pub const LAKE_LEVEL: f32 = -0.35;
const LAKE_BED: f32 = -1.6;

/// The sunken quarry pit in the south-west ruins.
pub const QUARRY_AT: Vec2 = Vec2::new(-55.0, 65.0);
const QUARRY_RADIUS: f32 = 12.0;
const QUARRY_DEPTH: f32 = 5.0;

/// Where the campfire burns (mirrors scenery.rs).
const BONFIRE_AT: Vec2 = Vec2::new(PARLIAMENT_AT.x + 3.5, PARLIAMENT_AT.y + 8.5);

/// Path segments between landmarks and out to every district.
fn paths() -> [(Vec2, Vec2); 12] {
    let arena = ARENA_CENTER.xz();
    let v = Vec2::new;
    [
        (Vec2::ZERO, arena),
        (Vec2::ZERO, BONFIRE_AT),
        (BONFIRE_AT, PARLIAMENT_AT),
        (Vec2::ZERO, POND_AT),
        // East, to the village.
        (Vec2::ZERO, v(40.0, 0.0)),
        // North, into the forest.
        (arena, v(-4.0, -40.0)),
        (v(-4.0, -40.0), v(3.0, -72.0)),
        // West: along the valley floor to the lake, and up the hill.
        (BONFIRE_AT, v(-36.0, 2.0)),
        (v(-36.0, 2.0), v(-41.0, 10.0)),
        (v(-36.0, 2.0), HILL_AT + v(6.0, 0.0)),
        // South-west, to the ruins and quarry.
        (v(-3.0, 5.0), v(-32.0, 34.0)),
        (v(-32.0, 34.0), v(-44.0, 52.0)),
    ]
}
fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smooth value noise in [0, 1).
fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let s = |v: f32| v * v * (3.0 - 2.0 * v);
    let at = |i, j| art::hash(i, j, seed);
    let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * s(fx);
    let bot = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * s(fx);
    top + (bot - top) * s(fy)
}

fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    noise(x, y, seed) * 0.6
        + noise(x * 2.1, y * 2.1, seed + 1) * 0.3
        + noise(x * 4.3, y * 4.3, seed + 2) * 0.1
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-4)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// How much "path" is at a point, 0..1 (wobbly edges so it looks walked-in).
fn path_amount(p: Vec2) -> f32 {
    let wobble = (noise(p.x * 0.6, p.y * 0.6, 77) - 0.5) * 0.7;
    let d = paths()
        .iter()
        .map(|&(a, b)| seg_dist(p, a, b))
        .fold(f32::MAX, f32::min)
        + wobble;
    1.0 - smoothstep(0.55, 1.25, d)
}

/// Is this point on a worn dirt path?
pub fn is_path(x: f32, z: f32) -> bool {
    path_amount(Vec2::new(x, z)) > 0.3
}

/// Is this point in (or right at the edge of) the pond?
pub fn is_pond(x: f32, z: f32) -> bool {
    Vec2::new(x, z).distance(POND_AT) < POND_RADIUS + 0.6
}

/// Is this point in (or right at the edge of) the west valley's lake?
pub fn is_lake(x: f32, z: f32) -> bool {
    Vec2::new(x, z).distance(LAKE_AT) < LAKE_RADIUS + 4.0 && height_at(x, z) < LAKE_LEVEL + 0.25
}

/// 1 inside the rectangle, fading to 0 over `soft` metres outside it.
fn rect_mask(p: Vec2, min: Vec2, max: Vec2, soft: f32) -> f32 {
    let d = (min.x - p.x)
        .max(p.x - max.x)
        .max(min.y - p.y)
        .max(p.y - max.y);
    1.0 - smoothstep(0.0, soft, d)
}

/// Ground height. Exactly 0 in the central commune square (±25 m) and the
/// east village; rolling everywhere else, with the west hill, the lake, the
/// quarry pit and hills rising beyond the edge of the world.
pub fn height_at(x: f32, z: f32) -> f32 {
    let p = Vec2::new(x, z);
    let edge = x.abs().max(z.abs());
    let centre = 1.0 - smoothstep(FLAT_HALF, FLAT_HALF + 8.0, edge);
    let village = rect_mask(p, VILLAGE_MIN, VILLAGE_MAX, 7.0);
    let flat = centre.max(village);
    // Beyond the play area: hills that hide the edge of the world.
    let rim = smoothstep(80.0, WORLD_HALF, edge) * (10.0 + fbm(x * 0.04, z * 0.04, 3) * 8.0);
    if flat >= 1.0 {
        return rim;
    }
    // Meadow undulation (never below 0, so only the lake and quarry dip).
    let mut h = fbm(x * 0.03, z * 0.03, 11) * 1.8 + fbm(x * 0.11, z * 0.11, 23) * 0.4;
    // North forest: gentle rolling ground.
    let forest = smoothstep(-30.0, -42.0, z);
    h += forest * (fbm(x * 0.05, z * 0.05, 47) - 0.3) * 4.0;
    // South-west ruins: mostly flat.
    let ruins = rect_mask(p, Vec2::new(-80.0, 30.0), Vec2::new(-30.0, 85.0), 8.0);
    h *= 1.0 - ruins * 0.8;
    // West valley: a real hill and a ridge along the far edge.
    let west = rect_mask(p, Vec2::new(-88.0, -32.0), Vec2::new(-35.0, 32.0), 10.0);
    let t = p.distance(HILL_AT) / HILL_RADIUS;
    let hill = HILL_HEIGHT * (1.0 - smoothstep(0.0, 1.0, t));
    let ridge = 7.0 * smoothstep(-68.0, -84.0, x) * (0.8 + fbm(z * 0.08, 5.0, 53) * 0.4);
    h += west * (hill + ridge);
    h *= 1.0 - flat;
    // The lake: a smooth bowl below the water line.
    let ld = p.distance(LAKE_AT) + (noise(x * 0.3, z * 0.3, 71) - 0.5) * 1.5;
    h = h + (LAKE_BED - h) * (1.0 - smoothstep(LAKE_RADIUS - 2.0, LAKE_RADIUS + 3.5, ld));
    // The quarry: a pit with steep, walkable sides and a flat floor.
    let qd = p.distance(QUARRY_AT) + (noise(x * 0.2, z * 0.2, 89) - 0.5) * 1.5;
    h -= QUARRY_DEPTH * (1.0 - smoothstep(QUARRY_RADIUS - 9.0, QUARRY_RADIUS + 1.0, qd));
    h + rim
}

/// Ground normal from the height function (smooth across chunk seams).
fn normal_at(x: f32, z: f32) -> Vec3 {
    let e = 0.5;
    let dx = height_at(x + e, z) - height_at(x - e, z);
    let dz = height_at(x, z + e) - height_at(x, z - e);
    Vec3::new(-dx, 2.0 * e, -dz).normalize()
}

/// Ground colour before the cat palette (pre-filter sRGB).
fn ground_rgb(x: f32, z: f32, h: f32) -> Vec3 {
    let p = Vec2::new(x, z);
    // Meadow: big patches of lush and dry grass, with brightness swings so the
    // cat palette (which turns greens yellow) still has light and dark to show.
    let patch = fbm(x * 0.09, z * 0.09, 5);
    let bright = fbm(x * 0.2, z * 0.2, 41);
    let speck = art::hash((x * 3.0) as i32, (z * 3.0) as i32, 9);
    let lush = Vec3::new(0.2, 0.42, 0.12);
    let dry = Vec3::new(0.78, 0.74, 0.3);
    let mut c = lush.lerp(dry, smoothstep(0.3, 0.75, patch));
    c *= 0.5 + bright * 1.0 + speck * 0.12;
    // Cool, blue-ish hollows, like shade under clouds.
    let cool = smoothstep(0.55, 0.75, fbm(x * 0.06 + 40.0, z * 0.06, 61));
    c = c.lerp(Vec3::new(0.22, 0.34, 0.5), cool * 0.8);
    // Clover patches: darker, denser green (deep olive for the cat).
    c = c.lerp(Vec3::new(0.12, 0.3, 0.14), clover_amount(x, z) * 0.6);
    // Worn, trampled spots: pale and dusty.
    let worn = smoothstep(0.7, 0.82, noise(x * 0.25 + 9.0, z * 0.25, 97));
    c = c.lerp(Vec3::new(0.85, 0.78, 0.6), worn * 0.55);
    // Hill tops get a little paler and bluer as they recede.
    c = c.lerp(Vec3::new(0.5, 0.62, 0.62), smoothstep(2.0, 9.0, h) * 0.55);
    // Worn dirt paths: a dark trodden rim around a paler middle.
    let path = path_amount(p);
    let rim = Vec3::new(0.32, 0.22, 0.14) * (0.85 + speck * 0.3);
    let dirt = Vec3::new(0.8, 0.62, 0.42) * (0.85 + speck * 0.3);
    c = c.lerp(rim, smoothstep(0.0, 0.4, path) * 0.85);
    c = c.lerp(dirt, smoothstep(0.45, 0.95, path));
    // Pond: muddy bank, then dark bed under the water.
    let pd = p.distance(POND_AT) + (noise(x * 0.8, z * 0.8, 31) - 0.5) * 0.8;
    c = c.lerp(
        Vec3::new(0.35, 0.30, 0.2),
        1.0 - smoothstep(POND_RADIUS, POND_RADIUS + 0.8, pd),
    );
    c = c.lerp(
        Vec3::new(0.12, 0.18, 0.16),
        1.0 - smoothstep(POND_RADIUS - 1.2, POND_RADIUS, pd),
    );
    // Lake: muddy shore, dark bed (by depth, so it follows the real shoreline).
    let near_lake = 1.0 - smoothstep(LAKE_RADIUS + 1.0, LAKE_RADIUS + 4.0, p.distance(LAKE_AT));
    let shore = near_lake * smoothstep(LAKE_LEVEL + 0.8, LAKE_LEVEL, h);
    c = c.lerp(Vec3::new(0.35, 0.30, 0.2), shore);
    c = c.lerp(
        Vec3::new(0.12, 0.18, 0.16),
        smoothstep(LAKE_LEVEL - 0.1, LAKE_LEVEL - 0.6, h),
    );
    // Quarry: pale, dusty stone in the pit.
    let in_pit = 1.0 - smoothstep(QUARRY_RADIUS, QUARRY_RADIUS + 3.0, p.distance(QUARRY_AT));
    let pit = smoothstep(-0.6, -2.5, h) * in_pit;
    c = c.lerp(Vec3::new(0.72, 0.66, 0.58) * (0.85 + speck * 0.3), pit);
    // Darker, worn ground around the campfire.
    let fire = 1.0 - smoothstep(1.0, 3.0, p.distance(BONFIRE_AT));
    c = c.lerp(Vec3::new(0.25, 0.22, 0.18), fire * 0.8);
    c
}

fn clover_amount(x: f32, z: f32) -> f32 {
    smoothstep(0.62, 0.72, noise(x * 0.35, z * 0.35, 83))
}

fn cat_linear(c: Vec3) -> [f32; 4] {
    let lin = cat_palette(Color::srgb(c.x, c.y, c.z)).to_linear();
    [lin.red, lin.green, lin.blue, 1.0]
}

fn ground_color(x: f32, z: f32, h: f32) -> [f32; 4] {
    cat_linear(ground_rgb(x, z, h))
}

/// Where grass tufts may not grow: the pond, the duel arena's sand and the
/// middle of paths.
fn tuft_blocked(x: f32, z: f32) -> bool {
    let a = ARENA_CENTER.xz();
    let in_arena = (x - a.x).abs() < ARENA_HALF + 0.6 && (z - a.y).abs() < ARENA_HALF + 0.6;
    in_arena
        || is_pond(x, z)
        || path_amount(Vec2::new(x, z)) > 0.5
        || (Vec2::new(x, z).distance(LAKE_AT) < LAKE_RADIUS + 4.0
            && height_at(x, z) < LAKE_LEVEL + 0.15)
}

/// Push one coloured triangle (colours are pre-filter sRGB).
fn tri(pos: &mut Vec<[f32; 3]>, col: &mut Vec<[f32; 4]>, v: [Vec3; 3], c: [Vec3; 3]) {
    for k in 0..3 {
        pos.push(v[k].to_array());
        col.push(cat_linear(c[k]));
    }
}

const PETALS: [Vec3; 4] = [
    Vec3::new(1.0, 1.0, 0.95),
    Vec3::new(1.0, 0.85, 0.2),
    Vec3::new(0.45, 0.55, 1.0),
    Vec3::new(0.7, 0.5, 1.0),
];

/// Dense little grass tufts and flowers over a square region as ONE merged,
/// vertex-coloured mesh, built once. Each tuft is two soft, leaf-shaped
/// blades; flowers are tiny diamonds on a stem. Tufts thin out away from the
/// centre (and stop at the edge of the play area) to keep the vertex count sane.
fn tufts_in(min: Vec2, size: f32) -> Mesh {
    const STEP: f32 = 0.55;
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut col: Vec<[f32; 4]> = Vec::new();
    // Global grid indices, so the hash (and so the tufts) don't depend on chunking.
    let cell = |v: f32| ((v + WORLD_HALF) / STEP).ceil() as i32;
    for j in cell(min.y)..cell(min.y + size) {
        for i in cell(min.x)..cell(min.x + size) {
            let r = |s: u32| art::hash(i, j, s);
            let x = -WORLD_HALF + (i as f32 + r(1)) * STEP;
            let z = -WORLD_HALF + (j as f32 + r(2)) * STEP;
            let edge = x.abs().max(z.abs());
            if edge > PLAY_HALF || tuft_blocked(x, z) {
                continue;
            }
            // Clumpy density: thick in places, bare in others.
            let dense = noise(x * 0.18, z * 0.18, 131);
            let clover = clover_amount(x, z);
            // Sparser right around the spawn, where the camera starts close.
            let near_spawn = (Vec2::new(x, z).distance(Vec2::new(0.0, 11.0)) / 7.0).min(1.0);
            // And thinner far out, where you only see it from a distance.
            let far = 1.0 - 0.72 * smoothstep(28.0, 55.0, edge);
            let keep = (dense * 1.1 + clover * 0.3 - 0.1) * (0.35 + 0.65 * near_spawn) * 0.55 * far;
            if r(3) > keep {
                continue;
            }
            let y = height_at(x, z);
            let base = ground_rgb(x, z, y);
            let shade = 0.85 + r(4) * 0.3;
            let root = base * 0.62 * shade;
            let mid = base * 0.95 * shade;
            let tip = (base * 1.2 + Vec3::new(0.05, 0.07, 0.0)).min(Vec3::ONE) * shade;
            let height = 0.08 + r(5) * 0.11;
            let at = Vec3::new(x, y, z);
            // Two soft, wide leaf-shaped blades: narrow root, round belly, blunt tip.
            for b in 0..2u32 {
                let a = r(6) * std::f32::consts::TAU + b as f32 * 1.9;
                let side = Vec3::new(a.cos(), 0.0, a.sin());
                let out = Vec3::new(-side.z, 0.0, side.x);
                let lean = out * (0.03 + r(7 + b) * 0.05);
                let h = height * (0.85 + b as f32 * 0.25);
                let off = out * 0.03;
                let w0 = side * 0.03;
                let w1 = side * 0.06;
                let m = at + off + lean * 0.45 + Vec3::Y * h * 0.5;
                let top = at + off + lean + Vec3::Y * h;
                let (r0, r1) = (at + off - w0, at + off + w0);
                let (m0, m1) = (m - w1, m + w1);
                tri(&mut pos, &mut col, [r0, r1, m1], [root, root, mid]);
                tri(&mut pos, &mut col, [r0, m1, m0], [root, mid, mid]);
                tri(&mut pos, &mut col, [m0, m1, top], [mid, mid, tip]);
            }
            // Flowers in clover patches (and the odd one anywhere).
            if r(11) < clover * 0.35 + 0.015 {
                let petal = PETALS[((r(12) * 4.0) as usize).min(3)];
                let top = at + Vec3::Y * (height + 0.06);
                let (e, f) = (Vec3::X * 0.06, Vec3::Z * 0.06);
                tri(&mut pos, &mut col, [top - e, top + f, top + e], [petal; 3]);
                tri(&mut pos, &mut col, [top + e, top - f, top - e], [petal; 3]);
                tri(
                    &mut pos,
                    &mut col,
                    [at - Vec3::X * 0.012, at + Vec3::X * 0.012, top],
                    [root; 3],
                );
            }
        }
    }
    let normals = vec![[0.0, 1.0, 0.0]; pos.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
}

/// Grass around the commune (the menu backdrop uses this).
pub(crate) fn build_tufts() -> Mesh {
    tufts_in(Vec2::splat(-40.0), 80.0)
}

/// A vertex-coloured ground mesh over a square region, `cells` per side.
fn ground_in(min: Vec2, size: f32, cells: u32) -> Mesh {
    let n = cells + 1;
    let step = size / cells as f32;
    let mut pos = Vec::with_capacity((n * n) as usize);
    let mut nrm = Vec::with_capacity((n * n) as usize);
    for j in 0..n {
        for i in 0..n {
            let x = min.x + i as f32 * step;
            let z = min.y + j as f32 * step;
            pos.push([x, height_at(x, z), z]);
            nrm.push(normal_at(x, z).to_array());
        }
    }
    let mut idx = Vec::with_capacity((cells * cells * 6) as usize);
    for j in 0..cells {
        for i in 0..cells {
            let a = j * n + i;
            let (b, c, d) = (a + 1, a + n, a + n + 1);
            idx.extend_from_slice(&[a, c, b, b, c, d]);
        }
    }
    let colors: Vec<[f32; 4]> = pos.iter().map(|p| ground_color(p[0], p[2], p[1])).collect();
    // Tile the greyscale grass texture every 5 m.
    let uvs: Vec<[f32; 2]> = pos.iter().map(|p| [p[0] / 5.0, p[2] / 5.0]).collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(idx))
}

/// Ground around the commune (the menu backdrop uses this).
pub(crate) fn build_mesh() -> Mesh {
    ground_in(Vec2::splat(-70.0), 140.0, 140)
}

/// The north-west corner and side length of every ground chunk.
fn chunks() -> impl Iterator<Item = (Vec2, f32)> {
    let size = WORLD_HALF * 2.0 / CHUNKS as f32;
    (0..CHUNKS * CHUNKS).map(move |k| {
        let (i, j) = ((k % CHUNKS) as f32, (k / CHUNKS) as f32);
        (
            Vec2::new(-WORLD_HALF + i * size, -WORLD_HALF + j * size),
            size,
        )
    })
}

/// The ground's physics shape: one heightfield sampled from [`height_at`].
pub fn ground_collider() -> Collider {
    let n = COLLIDER_CELLS + 1;
    let step = WORLD_HALF * 2.0 / COLLIDER_CELLS as f32;
    let at = |k: usize| -WORLD_HALF + k as f32 * step;
    // Rows run along X, columns along Z, centred on the origin.
    let heights = (0..n)
        .map(|i| (0..n).map(|j| height_at(at(i), at(j))).collect())
        .collect();
    Collider::heightfield(heights, Vec3::new(WORLD_HALF * 2.0, 1.0, WORLD_HALF * 2.0))
}

/// Marks the ground's collider. Walkers follow [`height_at`] instead of
/// bumping into it.
#[derive(Component, Debug)]
pub struct GroundCollider;

fn spawn_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let ground = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(art::grass_texture(&mut images)),
        perceptual_roughness: 0.95,
        reflectance: 0.12,
        ..default()
    });
    let grass = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 0.9,
        reflectance: 0.1,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    // Ground and grass in chunks, built once, so off-screen chunks are culled.
    for (min, size) in chunks() {
        commands.spawn((
            Name::new("Ground"),
            Mesh3d(meshes.add(ground_in(min, size, CHUNK_CELLS))),
            MeshMaterial3d(ground.clone()),
            DespawnOnExit(AppState::InGame),
        ));
        let tufts = tufts_in(min, size);
        if tufts.count_vertices() > 0 {
            commands.spawn((
                Name::new("Grass tufts"),
                Mesh3d(meshes.add(tufts)),
                MeshMaterial3d(grass.clone()),
                NotShadowCaster,
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    commands.spawn((
        Name::new("Ground collider"),
        RigidBody::Static,
        ground_collider(),
        GroundCollider,
        DespawnOnExit(AppState::InGame),
    ));
    let water = StandardMaterial {
        base_color: Color::srgba(0.3, 0.55, 0.65, 0.7),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.08,
        reflectance: 0.6,
        ..default()
    };
    let flat = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    // Pond surface: a translucent disc just above the dark bed.
    commands.spawn((
        Name::new("Pond"),
        Mesh3d(meshes.add(Circle::new(POND_RADIUS + 0.2).mesh().resolution(40))),
        MeshMaterial3d(materials.add(water.clone())),
        Transform::from_xyz(POND_AT.x, 0.03, POND_AT.y).with_rotation(flat),
        DespawnOnExit(AppState::InGame),
    ));
    // Lake surface: a bigger disc; the shore rises above it all round.
    commands.spawn((
        Name::new("Lake"),
        Mesh3d(meshes.add(Circle::new(LAKE_RADIUS + 4.0).mesh().resolution(64))),
        MeshMaterial3d(materials.add(water)),
        Transform::from_xyz(LAKE_AT.x, LAKE_LEVEL, LAKE_AT.y).with_rotation(flat),
        DespawnOnExit(AppState::InGame),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commune_and_village_are_flat() {
        for i in -25..=25 {
            for j in -25..=25 {
                assert_eq!(height_at(i as f32, j as f32), 0.0);
            }
        }
        for i in 35..=80 {
            for j in -35..=35 {
                assert_eq!(height_at(i as f32, j as f32), 0.0, "village {i},{j}");
            }
        }
        assert!(height_at(100.0, 100.0) > 5.0, "edge hills");
    }

    #[test]
    fn west_hill_lake_and_quarry() {
        assert!(height_at(HILL_AT.x, HILL_AT.y) > 10.0, "a real hill");
        assert!(is_lake(LAKE_AT.x, LAKE_AT.y));
        assert!(!is_lake(-40.0, 0.0));
        assert!(height_at(QUARRY_AT.x, QUARRY_AT.y) < -3.5, "quarry pit");
        // Nothing but the lake, quarry and forest dips below zero, and the
        // lake's shore stays above the water.
        for i in (-88..=88).step_by(2) {
            for j in (-88..=88).step_by(2) {
                let p = Vec2::new(i as f32, j as f32);
                let h = height_at(p.x, p.y);
                let dips = p.distance(LAKE_AT) < LAKE_RADIUS + 4.0
                    || p.distance(QUARRY_AT) < QUARRY_RADIUS + 1.0
                    || p.y < -30.0;
                assert!(dips || h >= 0.0, "{p}: {h}");
                let shore = p.distance(LAKE_AT);
                assert!(!(LAKE_RADIUS + 4.0..LAKE_RADIUS + 6.0).contains(&shore) || h > LAKE_LEVEL);
            }
        }
    }

    #[test]
    fn slopes_are_walkable() {
        for i in -176..=176 {
            for j in -176..=176 {
                let (x, z) = (i as f32 * 0.5, j as f32 * 0.5);
                let n = normal_at(x, z);
                assert!(n.y > 0.6, "too steep at {x},{z}: {n}");
            }
        }
    }

    #[test]
    fn collider_matches_height() {
        let collider = ground_collider();
        let spots = [
            (HILL_AT.x + 0.3, HILL_AT.y + 0.7),
            (-57.3, 4.6),
            (QUARRY_AT.x + 0.5, QUARRY_AT.y + 0.5),
            (10.7, -60.3),
            (-70.3, 20.9),
        ];
        for (x, z) in spots {
            let from = Vec3::new(x, 50.0, z);
            let hit = collider.cast_ray(Vec3::ZERO, Quat::IDENTITY, from, Vec3::NEG_Y, 200.0, true);
            let Some((t, _)) = hit else {
                panic!("no ground under {x},{z}");
            };
            let y = 50.0 - t;
            let h = height_at(x, z);
            assert!((y - h).abs() < 0.4, "{x},{z}: {y} vs {h}");
        }
    }

    #[test]
    fn tufts_stay_modest() {
        let verts: usize = chunks().map(|(m, s)| tufts_in(m, s).count_vertices()).sum();
        assert!(verts > 50_000 && verts < 600_000, "{verts}");
        let menu = build_tufts().count_vertices();
        assert!(menu > 10_000 && menu < 200_000, "{menu}");
    }

    #[test]
    fn landmarks_are_connected_and_pond_is_dry_of_paths() {
        assert!(is_path(0.0, -6.0));
        assert!(is_path(BONFIRE_AT.x, BONFIRE_AT.y));
        assert!(is_path(38.0, 0.0), "path to the village");
        assert!(is_pond(POND_AT.x, POND_AT.y));
    }
}
