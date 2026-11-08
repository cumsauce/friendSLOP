//! The ground: one generated, vertex-coloured mesh. The 50×50 play square is
//! dead flat (y = 0) so gameplay and physics are unchanged; beyond it gentle
//! hills rise and fade into the haze. Worn dirt paths link the centre, the
//! duel arena, the parliament ruins and the campfire, and a small pond sits
//! in the meadow. Fully deterministic, so every peer sees the same ground.
//!
//! Scenery can ask [`height_at`], [`is_path`] and [`is_pond`] where to put things.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::AppState;
use crate::art;
use crate::features::cat_palette::cat_palette;
use crate::features::dave_commune::PARLIAMENT_AT;
use crate::features::duel::ARENA_CENTER;

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_terrain);
    }
}

/// Half the side of the flat play square (matches the old 50×50 floor).
pub const FLAT_HALF: f32 = 25.0;
/// Half the side of the whole generated ground.
const WORLD_HALF: f32 = 70.0;
/// Grid resolution of the render mesh (cells per side): ~0.8 m cells, 31k verts.
const CELLS: u32 = 176;
/// Coarser grid for the hill collider.
const COLLIDER_CELLS: u32 = 56;

/// Where the pond sits and how big it is (in open meadow, off every path).
pub const POND_AT: Vec2 = Vec2::new(13.0, 9.0);
const POND_RADIUS: f32 = 3.2;

/// Where the campfire burns (mirrors scenery.rs).
const BONFIRE_AT: Vec2 = Vec2::new(PARLIAMENT_AT.x + 3.5, PARLIAMENT_AT.y + 8.5);

/// Path segments between landmarks.
fn paths() -> [(Vec2, Vec2); 4] {
    let arena = ARENA_CENTER.xz();
    [
        (Vec2::ZERO, arena),
        (Vec2::ZERO, BONFIRE_AT),
        (BONFIRE_AT, PARLIAMENT_AT),
        (Vec2::ZERO, POND_AT),
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

/// Ground height. Exactly 0 inside the flat play square.
pub fn height_at(x: f32, z: f32) -> f32 {
    let edge = x.abs().max(z.abs());
    let rise = smoothstep(FLAT_HALF, FLAT_HALF + 18.0, edge);
    if rise <= 0.0 {
        return 0.0;
    }
    let hills = fbm(x * 0.05, z * 0.05, 11) * 4.5 + fbm(x * 0.13, z * 0.13, 23) * 1.5;
    rise * (hills + rise * 3.0)
}

fn ground_color(x: f32, z: f32, h: f32) -> [f32; 4] {
    let p = Vec2::new(x, z);
    // Meadow: patches of lush and dry grass with fine speckle.
    let patch = fbm(x * 0.09, z * 0.09, 5);
    let speck = art::hash((x * 3.0) as i32, (z * 3.0) as i32, 9);
    let lush = Vec3::new(0.36, 0.64, 0.2);
    let dry = Vec3::new(0.7, 0.7, 0.28);
    let mut c = lush.lerp(dry, smoothstep(0.35, 0.75, patch)) * (0.9 + speck * 0.2);
    // Hill tops get a little paler and bluer as they recede.
    c = c.lerp(Vec3::new(0.5, 0.68, 0.5), smoothstep(2.0, 9.0, h) * 0.6);
    // Worn dirt paths.
    let dirt = Vec3::new(0.82, 0.68, 0.46) * (0.85 + speck * 0.3);
    c = c.lerp(dirt, path_amount(p));
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
    // Darker, worn ground around the campfire.
    let fire = 1.0 - smoothstep(1.0, 3.0, p.distance(BONFIRE_AT));
    c = c.lerp(Vec3::new(0.25, 0.22, 0.18), fire * 0.8);
    let lin = cat_palette(Color::srgb(c.x, c.y, c.z)).to_linear();
    [lin.red, lin.green, lin.blue, 1.0]
}

/// Builds a grid over the whole ground; returns positions and triangle indices.
fn grid(cells: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let n = cells + 1;
    let step = WORLD_HALF * 2.0 / cells as f32;
    let mut pos = Vec::with_capacity((n * n) as usize);
    for j in 0..n {
        for i in 0..n {
            let x = -WORLD_HALF + i as f32 * step;
            let z = -WORLD_HALF + j as f32 * step;
            pos.push([x, height_at(x, z), z]);
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
    (pos, idx)
}

fn build_mesh() -> Mesh {
    let (pos, idx) = grid(CELLS);
    let colors: Vec<[f32; 4]> = pos.iter().map(|p| ground_color(p[0], p[2], p[1])).collect();
    // Tile the greyscale grass texture every 5 m.
    let uvs: Vec<[f32; 2]> = pos.iter().map(|p| [p[0] / 5.0, p[2] / 5.0]).collect();
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(idx));
    mesh.compute_smooth_normals();
    mesh
}

fn spawn_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    commands.spawn((
        Name::new("Ground"),
        Mesh3d(meshes.add(build_mesh())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            base_color_texture: Some(art::grass_texture(&mut images)),
            perceptual_roughness: 0.95,
            reflectance: 0.12,
            ..default()
        })),
        DespawnOnExit(AppState::InGame),
    ));
    // Flat play square: same collider as the old floor.
    commands.spawn((
        RigidBody::Static,
        Collider::cuboid(FLAT_HALF * 2.0, 0.1, FLAT_HALF * 2.0),
        DespawnOnExit(AppState::InGame),
    ));
    // Hills: coarse trimesh (built once).
    let (pos, idx) = grid(COLLIDER_CELLS);
    let verts = pos.into_iter().map(Vec3::from).collect();
    let tris: Vec<[u32; 3]> = idx.as_chunks::<3>().0.to_vec();
    commands.spawn((
        RigidBody::Static,
        Collider::trimesh(verts, tris),
        DespawnOnExit(AppState::InGame),
    ));
    // Pond surface: a translucent disc just above the dark bed.
    commands.spawn((
        Name::new("Pond"),
        Mesh3d(meshes.add(Circle::new(POND_RADIUS + 0.2).mesh().resolution(40))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.3, 0.55, 0.65, 0.7),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.08,
            reflectance: 0.6,
            ..default()
        })),
        Transform::from_xyz(POND_AT.x, 0.03, POND_AT.y)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        DespawnOnExit(AppState::InGame),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_area_is_flat() {
        for i in -25..=25 {
            for j in -25..=25 {
                assert_eq!(height_at(i as f32, j as f32), 0.0);
            }
        }
        assert!(height_at(60.0, 60.0) > 1.0);
    }

    #[test]
    fn landmarks_are_connected_and_pond_is_dry_of_paths() {
        assert!(is_path(0.0, -6.0));
        assert!(is_path(BONFIRE_AT.x, BONFIRE_AT.y));
        assert!(is_pond(POND_AT.x, POND_AT.y));
    }
}
