//! Things to hide behind. From issue #60 by doomy ("the map is kinda sparse and
//! theres nothing realy to hide behind").
//!
//! Two parts:
//! - **Arena cover**: crate stacks, hay bales and stone walls inside the duel
//!   arena. They are solid in a duel: the server stops bullets that hit them
//!   and pushes duelists out of them. The layout is a constant, so every peer
//!   already agrees on it and nothing needs replicating.
//! - **Meadow hiding spots**: hedges, haystacks, boulders and ruined walls in
//!   the open middle of the floor. Code-generated art, visual only (nobody has
//!   a body outside a duel yet).

use bevy::prelude::*;

use super::duel::ARENA_CENTER;
use crate::AppState;
use crate::art;

/// What a piece of arena cover looks like.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoverKind {
    Crates,
    Hay,
    Stone,
}

/// A solid box of cover, `half` extents around `at` (on the floor, x/z).
#[derive(Clone, Copy, Debug)]
pub struct CoverBlock {
    pub kind: CoverKind,
    /// Centre on the floor, relative to `ARENA_CENTER`.
    pub at: Vec2,
    pub half: Vec3,
}

impl CoverBlock {
    fn min(&self) -> Vec3 {
        let c = ARENA_CENTER + Vec3::new(self.at.x, self.half.y, self.at.y);
        c - self.half
    }
    fn max(&self) -> Vec3 {
        let c = ARENA_CENTER + Vec3::new(self.at.x, self.half.y, self.at.y);
        c + self.half
    }
}

/// The arena's cover. Keep the spawn corners (±6, ±6) clear. The central crate
/// stack blocks the corner-to-corner sightline so nobody is shot on respawn.
pub const ARENA_COVER: &[CoverBlock] = &[
    CoverBlock {
        kind: CoverKind::Crates,
        at: Vec2::new(0.0, 0.0),
        half: Vec3::new(0.75, 1.0, 0.75),
    },
    CoverBlock {
        kind: CoverKind::Hay,
        at: Vec2::new(-3.5, 2.5),
        half: Vec3::new(1.2, 0.7, 0.55),
    },
    CoverBlock {
        kind: CoverKind::Hay,
        at: Vec2::new(3.5, -2.5),
        half: Vec3::new(1.2, 0.7, 0.55),
    },
    CoverBlock {
        kind: CoverKind::Stone,
        at: Vec2::new(-3.0, -3.5),
        half: Vec3::new(0.25, 1.1, 1.5),
    },
    CoverBlock {
        kind: CoverKind::Stone,
        at: Vec2::new(3.0, 3.5),
        half: Vec3::new(0.25, 1.1, 1.5),
    },
];

/// Distance along `dir` (unit) at which a ray from `origin` enters `block`.
fn ray_enters(origin: Vec3, dir: Vec3, block: &CoverBlock) -> Option<f32> {
    // Avoid 0 * inf = NaN for axis-aligned rays.
    let safe = Vec3::select(dir.abs().cmpgt(Vec3::splat(1e-9)), dir, Vec3::splat(1e-9));
    let inv = safe.recip();
    let t1 = (block.min() - origin) * inv;
    let t2 = (block.max() - origin) * inv;
    let near = t1.min(t2).max_element().max(0.0);
    let far = t1.max(t2).min_element();
    (far >= near).then_some(near)
}

/// Does arena cover stop a shot from `origin` along `dir` before it travels `dist`?
pub fn blocks_shot(origin: Vec3, dir: Vec3, dist: f32) -> bool {
    ARENA_COVER
        .iter()
        .any(|b| ray_enters(origin, dir, b).is_some_and(|t| t < dist))
}

/// Push a duelist's feet (`radius` wide) out of any cover they walked into.
pub fn push_out(mut pos: Vec3, radius: f32) -> Vec3 {
    for b in ARENA_COVER {
        let c = ARENA_CENTER.xz() + b.at;
        let h = b.half.xz();
        let d = pos.xz() - c;
        let closest = d.clamp(-h, h);
        let gap = d - closest;
        let dist = gap.length();
        if dist >= radius {
            continue;
        }
        let out = if dist > 1e-4 {
            c + closest + gap / dist * radius
        } else {
            // Feet inside the box: leave by the nearest side.
            let pen = h - d.abs();
            if pen.x < pen.y {
                Vec2::new(c.x + d.x.signum() * (h.x + radius), pos.z)
            } else {
                Vec2::new(pos.x, c.y + d.y.signum() * (h.y + radius))
            }
        };
        pos.x = out.x;
        pos.z = out.y;
    }
    pos
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), (spawn_arena_cover, spawn_meadow));
}

fn spawn_arena_cover(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wood = art::soft(&mut materials, Color::srgb(0.62, 0.45, 0.28));
    let plank = art::soft(&mut materials, Color::srgb(0.45, 0.32, 0.2));
    let hay = art::soft(&mut materials, Color::srgb(0.92, 0.8, 0.42));
    let twine = art::soft(&mut materials, Color::srgb(0.6, 0.45, 0.25));
    let stone = art::soft(&mut materials, Color::srgb(0.62, 0.62, 0.66));
    let cap = art::soft(&mut materials, Color::srgb(0.5, 0.5, 0.55));
    for b in ARENA_COVER {
        let at = ARENA_CENTER + Vec3::new(b.at.x, 0.0, b.at.y);
        let s = b.half * 2.0;
        let mut root = commands.spawn((
            Transform::from_translation(at),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ));
        match b.kind {
            CoverKind::Crates => {
                // Two crates stacked, the top one turned a little.
                let crate_mesh = meshes.add(Cuboid::new(s.x, s.y / 2.0, s.z));
                let slat = meshes.add(Cuboid::new(s.x + 0.04, 0.08, s.z + 0.04));
                root.with_children(|p| {
                    for (i, yaw) in [(0.0, 0.0), (1.0, 0.2)] {
                        let y = s.y / 4.0 + i * s.y / 2.0;
                        p.spawn((
                            Mesh3d(crate_mesh.clone()),
                            MeshMaterial3d(wood.clone()),
                            Transform::from_xyz(0.0, y, 0.0)
                                .with_rotation(Quat::from_rotation_y(yaw)),
                        ))
                        .with_children(|c| {
                            for dy in [-0.3, 0.3] {
                                c.spawn((
                                    Mesh3d(slat.clone()),
                                    MeshMaterial3d(plank.clone()),
                                    Transform::from_xyz(0.0, dy * s.y / 2.0, 0.0),
                                ));
                            }
                        });
                    }
                });
            }
            CoverKind::Hay => {
                // Two bales on the bottom, one on top, tied with twine.
                let bale = meshes.add(Cuboid::new(s.x / 2.0 - 0.04, s.y / 2.0, s.z));
                let band = meshes.add(Cuboid::new(0.06, s.y / 2.0 + 0.02, s.z + 0.02));
                root.with_children(|p| {
                    for (x, y) in [(-0.25, 0.25), (0.25, 0.25), (0.0, 0.75)] {
                        p.spawn((
                            Mesh3d(bale.clone()),
                            MeshMaterial3d(hay.clone()),
                            Transform::from_xyz(x * s.x, y * s.y, 0.0),
                        ))
                        .with_children(|c| {
                            for bx in [-0.15, 0.15] {
                                c.spawn((
                                    Mesh3d(band.clone()),
                                    MeshMaterial3d(twine.clone()),
                                    Transform::from_xyz(bx * s.x, 0.0, 0.0),
                                ));
                            }
                        });
                    }
                });
            }
            CoverKind::Stone => {
                let wall = meshes.add(Cuboid::new(s.x, s.y - 0.12, s.z));
                let top = meshes.add(Cuboid::new(s.x + 0.12, 0.12, s.z + 0.12));
                root.with_children(|p| {
                    p.spawn((
                        Mesh3d(wall),
                        MeshMaterial3d(stone.clone()),
                        Transform::from_xyz(0.0, (s.y - 0.12) / 2.0, 0.0),
                    ));
                    p.spawn((
                        Mesh3d(top),
                        MeshMaterial3d(cap.clone()),
                        Transform::from_xyz(0.0, s.y - 0.06, 0.0),
                    ));
                });
            }
        }
    }
}

/// A hiding spot out in the meadow.
#[derive(Clone, Copy)]
enum Spot {
    /// A hedge `len` metres long.
    Hedge(f32),
    Haystacks,
    Boulders,
    /// A crumbling stone wall `len` metres long (leftover from the storm of '21).
    RuinedWall(f32),
}

/// Placed by hand in the gaps between the features' homes (bear, pets, goat,
/// totems, music boxes, storm wreckage, parliament, arena), all within the
/// inner meadow that the nature props (`scenery.rs`) leave open.
const MEADOW_SPOTS: &[(Vec2, f32, Spot)] = &[
    (Vec2::new(-1.5, -2.5), 0.3, Spot::Hedge(3.5)),
    (Vec2::new(11.0, 7.5), 1.2, Spot::Hedge(4.0)),
    (Vec2::new(-9.5, 6.5), 0.0, Spot::Haystacks),
    (Vec2::new(9.5, 9.5), 0.0, Spot::Boulders),
    (Vec2::new(0.5, 4.8), 2.0, Spot::Boulders),
    (Vec2::new(1.0, 10.5), 0.1, Spot::RuinedWall(5.0)),
    (Vec2::new(-12.0, 2.5), 1.4, Spot::RuinedWall(4.0)),
];

fn spawn_meadow(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let leaf = art::soft(&mut materials, Color::srgb(0.25, 0.5, 0.25));
    let leaf_light = art::soft(&mut materials, Color::srgb(0.35, 0.6, 0.3));
    let hay = art::soft(&mut materials, Color::srgb(0.92, 0.8, 0.42));
    let rock = art::soft(&mut materials, Color::srgb(0.58, 0.58, 0.62));
    let rock_dark = art::soft(&mut materials, Color::srgb(0.45, 0.46, 0.5));
    let blob = meshes.add(
        Sphere::new(0.5)
            .mesh()
            .ico(1)
            .unwrap_or_else(|_| Sphere::new(0.5).into()),
    );
    let stack = meshes.add(Cylinder::new(0.9, 1.2));
    let roof = meshes.add(Cone::new(1.0, 1.0));
    let block = meshes.add(Cuboid::new(0.7, 0.45, 0.5));
    for (i, &(at, yaw, spot)) in MEADOW_SPOTS.iter().enumerate() {
        let seed = 600 + i as u32;
        let jitter = |k: i32, j: i32| art::hash(k, j, seed) - 0.5;
        let mut root = commands.spawn((
            Transform::from_xyz(at.x, 0.0, at.y).with_rotation(Quat::from_rotation_y(yaw)),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ));
        root.with_children(|p| match spot {
            Spot::Hedge(len) => {
                // A row of overlapping leafy blobs, about chest height.
                let n = (len / 0.6).ceil() as i32;
                for k in 0..n {
                    let x = -len / 2.0 + (k as f32 + 0.5) * len / n as f32;
                    let mat = if k % 2 == 0 { &leaf } else { &leaf_light };
                    p.spawn((
                        Mesh3d(blob.clone()),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_xyz(x, 0.7 + jitter(k, 0) * 0.2, jitter(k, 1) * 0.2)
                            .with_scale(Vec3::new(1.3, 1.6 + jitter(k, 2) * 0.3, 1.2)),
                    ));
                }
            }
            Spot::Haystacks => {
                for (k, (x, z, size)) in [(0.0, 0.0, 1.0), (2.1, 0.6, 0.8), (0.9, 2.0, 0.9)]
                    .into_iter()
                    .enumerate()
                {
                    let k = k as i32;
                    p.spawn((
                        Transform::from_xyz(x, 0.0, z)
                            .with_scale(Vec3::splat(size + jitter(k, 0) * 0.1)),
                        Visibility::default(),
                    ))
                    .with_children(|h| {
                        h.spawn((
                            Mesh3d(stack.clone()),
                            MeshMaterial3d(hay.clone()),
                            Transform::from_xyz(0.0, 0.6, 0.0),
                        ));
                        h.spawn((
                            Mesh3d(roof.clone()),
                            MeshMaterial3d(hay.clone()),
                            Transform::from_xyz(0.0, 1.7, 0.0),
                        ));
                    });
                }
            }
            Spot::Boulders => {
                for (k, (x, z, size)) in [(0.0, 0.0, 2.0), (1.4, 0.5, 1.3), (-0.9, 1.0, 0.9)]
                    .into_iter()
                    .enumerate()
                {
                    let k = k as i32;
                    let mat = if k == 0 { &rock } else { &rock_dark };
                    p.spawn((
                        Mesh3d(blob.clone()),
                        MeshMaterial3d(mat.clone()),
                        Transform::from_xyz(x, size * 0.35, z)
                            .with_rotation(Quat::from_rotation_y(jitter(k, 0) * 3.0))
                            .with_scale(Vec3::new(size * 1.2, size * 0.9, size)),
                    ));
                }
            }
            Spot::RuinedWall(len) => {
                // Courses of stone blocks, broken off unevenly along the top.
                let cols = (len / 0.72).floor() as i32;
                for k in 0..cols {
                    let height = 1 + (art::hash(k, 9, seed) * 4.0) as i32;
                    for row in 0..height {
                        let x = -len / 2.0 + (k as f32 + 0.5) * 0.72 + (row % 2) as f32 * 0.1;
                        let mat = if (k + row) % 3 == 0 {
                            &rock_dark
                        } else {
                            &rock
                        };
                        p.spawn((
                            Mesh3d(block.clone()),
                            MeshMaterial3d(mat.clone()),
                            Transform::from_xyz(x, 0.225 + row as f32 * 0.46, 0.0)
                                .with_rotation(Quat::from_rotation_y(jitter(k, row) * 0.15)),
                        ));
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::duel::spawn_points;

    #[test]
    fn spawn_corners_are_clear_of_cover() {
        for p in spawn_points() {
            assert_eq!(push_out(p, 0.45), p, "spawn {p} is inside cover");
        }
    }

    #[test]
    fn crates_block_the_corner_to_corner_shot() {
        let [a, b] = spawn_points();
        let eye = a + Vec3::Y * 1.6;
        let to = b + Vec3::Y * 1.3 - eye;
        assert!(blocks_shot(eye, to.normalize(), to.length()));
    }

    #[test]
    fn open_ground_does_not_block() {
        let [a, _] = spawn_points();
        let eye = a + Vec3::Y * 1.6;
        assert!(!blocks_shot(eye, Vec3::Z, 4.0));
        // Shooting over everything.
        assert!(!blocks_shot(eye + Vec3::Y * 3.0, Vec3::X, 20.0));
    }

    #[test]
    fn walking_into_cover_pushes_you_out() {
        let c = ARENA_CENTER;
        let pushed = push_out(c + Vec3::new(0.2, 0.0, 0.1), 0.45);
        let rel = (pushed - c).xz().abs();
        assert!(rel.x >= 1.19 || rel.y >= 1.19, "still inside: {pushed}");
        let grazing = push_out(c + Vec3::new(1.0, 0.0, 0.0), 0.45);
        assert!((grazing.x - c.x - 1.2).abs() < 1e-4);
    }
}
