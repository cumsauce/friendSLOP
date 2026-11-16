//! The wastes past the edge of the map (#391, bben: "when the player hits the
//! edge, it should be procedurally generated wasteland"). See
//! `docs/design/infinite-world.md`.
//!
//! The ground beyond the map's north edge is a grid of [`CHUNK`]-metre
//! chunks, each a pure function of the world seed and its coordinates
//! ([`Chunk::generate`]), so every peer builds the same ground and nothing is
//! sent over the wire. Chunks near a [`Viewers`] point are built (heightfield
//! colliders near `sim` points, meshes near `show` points, at most `per_frame` a frame, nearest first) and
//! dropped again once everyone is far away. [`crate::terrain::height_at`]
//! answers for the wastes too, so everything that stands on the ground works
//! out here unchanged.
//!
//! Generators use only `+ - * /`, `floor` and the integer hash, never libm
//! (`sin`, `exp`, ...), so native and web builds agree to the bit.

use avian3d::prelude::*;
use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::AppState;
use crate::terrain;

/// A chunk's side (m). The map's ground (±110 m) is exactly chunks -2..=1.
pub const CHUNK: f32 = 55.0;
/// Height samples per chunk side, minus one (~1.1 m cells).
pub const CELLS: usize = 50;
/// Heightfield collider cells per chunk side (~2.2 m).
const COLLIDER_CELLS: usize = 25;
/// The map's ground ends here; the wastes start past it.
const MAP_EDGE: f32 = 110.0;
/// How far past the map edge the wastes blend in from the map's own ground.
const BLEND: f32 = 40.0;
/// Bumped whenever the generator's output changes (saves keep theirs).
pub const GEN_VERSION: u32 = 1;

/// Tuning numbers: `assets/data/world/wastes.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The world seed: the same seed is the same wastes.
    pub seed: u32,
    /// Whether the map's north edge is open (walk out into the wastes).
    pub open: bool,
    /// A chunk is built when someone is this close to it (m).
    pub load: f32,
    /// A chunk is dropped when everyone is further than this (m); more than `load`.
    pub unload: f32,
    /// Chunks built per frame at most.
    pub per_frame: usize,
    /// The wastes end this far from the middle of the map (m).
    pub limit: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/world/wastes.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Which chunk: `(i, j)` covers x in `[CHUNK i, CHUNK (i+1))`, z likewise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChunkCoord {
    pub i: i32,
    pub j: i32,
}

impl ChunkCoord {
    /// The chunk holding (x, z).
    pub fn containing(p: Vec2) -> Self {
        Self {
            i: (p.x / CHUNK).floor() as i32,
            j: (p.y / CHUNK).floor() as i32,
        }
    }

    /// Its north-west corner (smallest x and z).
    pub fn min(self) -> Vec2 {
        Vec2::new(self.i as f32, self.j as f32) * CHUNK
    }

    /// How far `p` is from the chunk (0 inside it).
    pub fn gap(self, p: Vec2) -> f32 {
        let lo = self.min();
        let hi = lo + Vec2::splat(CHUNK);
        (p - p.clamp(lo, hi)).length()
    }

    /// Is this chunk part of the wastes (north of the map, inside the limit)?
    pub fn wild(self, limit: f32) -> bool {
        let lo = self.min();
        lo.y + CHUNK <= -MAP_EDGE && lo.y >= -limit && lo.x >= -limit && lo.x + CHUNK <= limit
    }
}

/// Is (x, z) out in the wastes (past the map's north edge)?
pub fn covers(_x: f32, z: f32) -> bool {
    z < -MAP_EDGE
}

/// Wasteland ground before it meets the map: broad dunes, flat-topped mesas
/// and dry washes.
fn wild(seed: u32, x: f32, z: f32) -> f32 {
    let s = seed.wrapping_mul(7919);
    let dunes = fbm(x * 0.012, z * 0.02, s) * 9.0 + fbm(x * 0.06, z * 0.06, s + 10) * 1.2;
    // Mesas: where the broad noise is high, cut flat at a shelf.
    let m = fbm(x * 0.006, z * 0.006, s + 20);
    let mesa = smoothstep(0.55, 0.62, m) * 14.0;
    // Washes: shallow dry channels along the noise's middle line.
    let w = fbm(x * 0.009, z * 0.009, s + 30) - 0.5;
    let wash = (1.0 - smoothstep(0.0, 0.04, w.abs())) * 2.5;
    6.0 + dunes + mesa - wash
}

/// Ground height in the wastes: the map's own edge height at first,
/// blending into the wasteland over [`BLEND`] metres, so the seam never
/// steps.
pub fn height(seed: u32, x: f32, z: f32) -> f32 {
    let t = smoothstep(0.0, BLEND, -MAP_EDGE - z);
    let w = wild(seed, x, z);
    if t >= 1.0 {
        return w;
    }
    let edge = terrain::map_height_at(x, -MAP_EDGE);
    edge + (w - edge) * t
}

/// One chunk of the wastes, as generated.
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    pub at: ChunkCoord,
    /// `(CELLS + 1)²` heights, row by row along x, rows going +z.
    pub heights: Vec<f32>,
}

impl Chunk {
    /// The chunk at `at` for `seed`. Pure: same inputs, same chunk.
    pub fn generate(seed: u32, at: ChunkCoord) -> Self {
        let n = CELLS + 1;
        let step = CHUNK / CELLS as f32;
        let lo = at.min();
        let heights = (0..n * n)
            .map(|k| {
                let (i, j) = ((k % n) as f32, (k / n) as f32);
                height(seed, lo.x + i * step, lo.y + j * step)
            })
            .collect();
        Self { at, heights }
    }

    /// Its bytes: `WST1`, version, coords, then the heights (all little-endian).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.heights.len() * 4);
        out.extend_from_slice(b"WST1");
        out.extend_from_slice(&GEN_VERSION.to_le_bytes());
        out.extend_from_slice(&self.at.i.to_le_bytes());
        out.extend_from_slice(&self.at.j.to_le_bytes());
        for h in &self.heights {
            out.extend_from_slice(&h.to_le_bytes());
        }
        out
    }

    fn h(&self, i: usize, j: usize) -> f32 {
        self.heights[j * (CELLS + 1) + i]
    }

    /// Its ground mesh, vertex-coloured, in world coordinates.
    pub fn mesh(&self) -> Mesh {
        let n = CELLS + 1;
        let step = CHUNK / CELLS as f32;
        let lo = self.at.min();
        let mut pos = Vec::with_capacity(n * n);
        let mut nrm = Vec::with_capacity(n * n);
        let mut col = Vec::with_capacity(n * n);
        for j in 0..n {
            for i in 0..n {
                let (x, z) = (lo.x + i as f32 * step, lo.y + j as f32 * step);
                let y = self.h(i, j);
                pos.push([x, y, z]);
                // Edge normals from the height function, so seams light alike.
                let dx = self.h((i + 1).min(n - 1), j) - self.h(i.saturating_sub(1), j);
                let dz = self.h(i, (j + 1).min(n - 1)) - self.h(i, j.saturating_sub(1));
                let wide = |k: usize| if k == 0 || k == n - 1 { 1.0 } else { 2.0 };
                nrm.push(
                    Vec3::new(-dx / wide(i), step, -dz / wide(j))
                        .normalize()
                        .to_array(),
                );
                col.push(dust(x, z, y));
            }
        }
        let uvs: Vec<[f32; 2]> = pos.iter().map(|p| [p[0] / 5.0, p[2] / 5.0]).collect();
        let mut idx = Vec::with_capacity(CELLS * CELLS * 6);
        for j in 0..CELLS as u32 {
            for i in 0..CELLS as u32 {
                let a = j * n as u32 + i;
                let (b, c, d) = (a + 1, a + n as u32, a + n as u32 + 1);
                idx.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, pos)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, nrm)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, col)
        .with_inserted_indices(Indices::U32(idx))
    }

    /// Its physics shape: a heightfield over every other sample, placed at
    /// the chunk's middle (see [`Self::middle`]).
    pub fn collider(&self) -> Collider {
        let every = CELLS / COLLIDER_CELLS;
        // Rows run along x, columns along z.
        let heights = (0..=COLLIDER_CELLS)
            .map(|i| {
                (0..=COLLIDER_CELLS)
                    .map(|j| self.h(i * every, j * every))
                    .collect()
            })
            .collect();
        Collider::heightfield(heights, Vec3::new(CHUNK, 1.0, CHUNK))
    }

    /// The chunk's middle on the ground plane (where its collider sits).
    pub fn middle(&self) -> Vec3 {
        let c = self.at.min() + Vec2::splat(CHUNK / 2.0);
        Vec3::new(c.x, 0.0, c.y)
    }
}

/// Dusty ground colour: pale sand, darker on the mesas, reddish in washes.
fn dust(x: f32, z: f32, y: f32) -> [f32; 4] {
    let patch = fbm(x * 0.05, z * 0.05, 131);
    let sand = Vec3::new(0.72, 0.62, 0.45) * (0.85 + patch * 0.3);
    let rock = Vec3::new(0.52, 0.42, 0.34);
    let c = sand.lerp(rock, smoothstep(14.0, 20.0, y));
    let lin = friendslop_art::palette::cat_palette(Color::srgb(c.x, c.y, c.z)).to_linear();
    [lin.red, lin.green, lin.blue, 1.0]
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let s = |v: f32| v * v * (3.0 - 2.0 * v);
    let at = |i, j| friendslop_art::art::hash(i, j, seed);
    let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * s(fx);
    let bot = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * s(fx);
    top + (bot - top) * s(fy)
}

fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    noise(x, y, seed) * 0.6
        + noise(x * 2.1, y * 2.1, seed.wrapping_add(1)) * 0.3
        + noise(x * 4.3, y * 4.3, seed.wrapping_add(2)) * 0.1
}

/// Where the game keeps a walker: inside the map's soft wall, or, with the
/// north edge open, out into the wastes up to their limit. `edge` is the
/// map's soft wall (half side).
pub fn keep_in(p: Vec2, edge: f32) -> Vec2 {
    let t = tuning();
    if !t.open {
        return p.clamp(Vec2::splat(-edge), Vec2::splat(edge));
    }
    let z = p.y.clamp(-t.limit.max(edge), edge);
    // Between the soft wall and the map's edge the ground only spans the map.
    let side = if z < -MAP_EDGE {
        t.limit.max(edge)
    } else {
        edge
    };
    Vec2::new(p.x.clamp(-side, side), z)
}

/// Every point the wastes stream around, written by the game each frame,
/// split like an area's layers (`friendslop_base::areas`): `sim` gets
/// colliders (on the host everyone's feet, on a client only yours), `show`
/// gets meshes (only you).
#[derive(Resource, Default, Debug, Clone)]
pub struct Viewers {
    pub sim: Vec<Vec2>,
    pub show: Vec<Vec2>,
}

/// Where chunks are built and dropped; fill [`Viewers`] before it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct StreamSet;

/// The chunks built now: colliders and meshes, each streamed on its own.
#[derive(Resource, Default)]
struct Loaded {
    sim: HashMap<ChunkCoord, Entity>,
    show: HashMap<ChunkCoord, Entity>,
}

/// The wastes' shared ground material.
#[derive(Resource)]
struct Ground(Handle<StandardMaterial>);

/// Marks a built chunk's collider or mesh (the census counts these as the
/// `wastes` line).
#[derive(Component, Debug)]
pub struct WasteChunk(pub ChunkCoord);

pub struct WastesPlugin;

impl Plugin for WastesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Viewers>()
            .init_resource::<Loaded>()
            .add_systems(OnExit(AppState::InGame), forget)
            .add_systems(
                Update,
                stream.in_set(StreamSet).run_if(in_state(AppState::InGame)),
            );
    }
}

/// The chunks to build (nearest first, at most `budget`) and the built ones
/// to drop, for these viewers. Pure, so tests drive it without an app.
pub fn plan(
    viewers: &[Vec2],
    built: impl Iterator<Item = ChunkCoord>,
    t: &Tuning,
) -> (Vec<ChunkCoord>, Vec<ChunkCoord>) {
    let near = |c: ChunkCoord, r: f32| viewers.iter().any(|v| c.gap(*v) <= r);
    let built: Vec<ChunkCoord> = built.collect();
    let drop = built
        .iter()
        .copied()
        .filter(|c| !near(*c, t.unload))
        .collect();
    let reach = (t.load / CHUNK).ceil() as i32 + 1;
    let mut want: Vec<(f32, ChunkCoord)> = Vec::new();
    for v in viewers {
        let home = ChunkCoord::containing(*v);
        for j in -reach..=reach {
            for i in -reach..=reach {
                let c = ChunkCoord {
                    i: home.i + i,
                    j: home.j + j,
                };
                let gap = viewers
                    .iter()
                    .map(|v| c.gap(*v))
                    .fold(f32::INFINITY, f32::min);
                if c.wild(t.limit) && gap <= t.load && !built.contains(&c) {
                    want.push((gap, c));
                }
            }
        }
    }
    want.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    want.dedup_by_key(|w| w.1);
    let build = want.into_iter().take(t.per_frame).map(|w| w.1).collect();
    (build, drop)
}

fn stream(
    mut commands: Commands,
    viewers: Res<Viewers>,
    mut loaded: ResMut<Loaded>,
    ground: Option<Res<Ground>>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let t = tuning();
    let loaded = &mut *loaded;
    // Colliders: anyone near (the host simulates for everyone).
    let (build, drop) = plan(&viewers.sim, loaded.sim.keys().copied(), &t);
    for c in drop {
        if let Some(e) = loaded.sim.remove(&c) {
            commands.entity(e).despawn();
        }
    }
    for c in build {
        let chunk = Chunk::generate(t.seed, c);
        let e = commands.spawn((
            Name::new("Wastes chunk collider"),
            WasteChunk(c),
            RigidBody::Static,
            chunk.collider(),
            terrain::GroundCollider,
            Transform::from_translation(chunk.middle()),
            DespawnOnExit(AppState::InGame),
        ));
        loaded.sim.insert(c, e.id());
    }
    // Meshes: you near (none headless).
    let material = match (&ground, materials.as_mut()) {
        (Some(g), _) => Some(g.0.clone()),
        (None, Some(m)) => {
            let h = m.add(StandardMaterial {
                base_color: Color::WHITE,
                perceptual_roughness: 0.97,
                reflectance: 0.08,
                ..default()
            });
            commands.insert_resource(Ground(h.clone()));
            Some(h)
        }
        (None, None) => None,
    };
    let (Some(meshes), Some(material)) = (meshes.as_mut(), material) else {
        return;
    };
    let (build, drop) = plan(&viewers.show, loaded.show.keys().copied(), &t);
    for c in drop {
        if let Some(e) = loaded.show.remove(&c) {
            commands.entity(e).despawn();
        }
    }
    for c in build {
        let chunk = Chunk::generate(t.seed, c);
        let e = commands.spawn((
            Name::new("Wastes chunk"),
            WasteChunk(c),
            Mesh3d(meshes.add(chunk.mesh())),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ));
        loaded.show.insert(c, e.id());
    }
}

fn forget(mut loaded: ResMut<Loaded>, mut commands: Commands) {
    loaded.sim.clear();
    loaded.show.clear();
    commands.remove_resource::<Ground>();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tune() -> Tuning {
        Tuning {
            seed: 391,
            open: true,
            load: 220.0,
            unload: 280.0,
            per_frame: 2,
            limit: 4000.0,
        }
    }

    #[test]
    fn same_seed_same_bytes() {
        let at = ChunkCoord { i: 3, j: -7 };
        let a = Chunk::generate(391, at).to_bytes();
        let b = Chunk::generate(391, at).to_bytes();
        assert_eq!(a, b);
        assert_eq!(a.len(), 16 + (CELLS + 1) * (CELLS + 1) * 4);
        assert_ne!(a, Chunk::generate(392, at).to_bytes(), "the seed matters");
    }

    const PINNED: u64 = 0xce05_d4df_6194_b49a;

    #[test]
    fn generator_is_pinned() {
        // FNV-1a of a far chunk's bytes (far from the map, so the map's
        // files don't matter). Changing the generator changes this: bump
        // GEN_VERSION and update it.
        let bytes = Chunk::generate(391, ChunkCoord { i: -4, j: -9 }).to_bytes();
        let fnv = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
            (h ^ *b as u64).wrapping_mul(0x0100_0000_01b3)
        });
        assert_eq!(fnv, PINNED, "{fnv:#x}");
    }

    #[test]
    fn neighbours_share_their_edge() {
        let a = Chunk::generate(391, ChunkCoord { i: 0, j: -4 });
        let b = Chunk::generate(391, ChunkCoord { i: 1, j: -4 });
        let c = Chunk::generate(391, ChunkCoord { i: 0, j: -5 });
        for k in 0..=CELLS {
            assert_eq!(a.h(CELLS, k), b.h(0, k));
            assert_eq!(c.h(k, CELLS), a.h(k, 0));
        }
    }

    #[test]
    fn the_seam_with_the_map_does_not_step() {
        for x in [-100.0, -30.0, 0.0, 47.0, 100.0] {
            let inside = terrain::height_at(x, -MAP_EDGE + 0.01);
            let outside = terrain::height_at(x, -MAP_EDGE - 0.01);
            assert!(
                (inside - outside).abs() < 0.05,
                "{x}: {inside} vs {outside}"
            );
        }
    }

    #[test]
    fn only_north_of_the_map_is_wild() {
        let t = tune();
        assert!(ChunkCoord { i: 0, j: -3 }.wild(t.limit));
        assert!(!ChunkCoord { i: 0, j: -2 }.wild(t.limit), "the map");
        assert!(!ChunkCoord { i: 0, j: 4 }.wild(t.limit), "New York");
        assert!(
            !ChunkCoord { i: 0, j: -100 }.wild(t.limit),
            "past the limit"
        );
    }

    #[test]
    fn streams_in_nearest_first_and_out_when_left() {
        let t = tune();
        let at = Vec2::new(10.0, -200.0);
        let (build, drop) = plan(&[at], std::iter::empty(), &t);
        assert_eq!(build.len(), t.per_frame);
        assert!(drop.is_empty());
        assert_eq!(build[0], ChunkCoord::containing(at));
        // Everything in reach, then walk away.
        let mut built = Vec::new();
        loop {
            let (b, _) = plan(&[at], built.iter().copied(), &t);
            if b.is_empty() {
                break;
            }
            built.extend(b);
        }
        assert!(built.len() > 20, "{}", built.len());
        let (_, drop) = plan(&[Vec2::new(0.0, 0.0)], built.iter().copied(), &t);
        assert!(drop.iter().any(|c| c.j <= -6));
        let (_, none) = plan(&[at], built.iter().copied(), &t);
        assert!(none.is_empty(), "nothing dropped while you're there");
    }

    #[test]
    fn the_north_wall_opens() {
        TUNING.set(tune());
        assert_eq!(
            keep_in(Vec2::new(0.0, -500.0), 88.0),
            Vec2::new(0.0, -500.0)
        );
        assert_eq!(keep_in(Vec2::new(300.0, -100.0), 88.0).x, 88.0);
        assert_eq!(keep_in(Vec2::new(300.0, -200.0), 88.0).x, 300.0);
        assert_eq!(keep_in(Vec2::new(0.0, 300.0), 88.0).y, 88.0);
    }

    /// `cargo test -p friendslop-world --release wastes::tests::perf -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn perf() {
        let n = 64;
        let start = std::time::Instant::now();
        for k in 0..n {
            let _ = Chunk::generate(
                391,
                ChunkCoord {
                    i: k % 8,
                    j: -3 - k / 8,
                },
            );
        }
        let made = start.elapsed() / n as u32;
        let chunk = Chunk::generate(391, ChunkCoord { i: 0, j: -5 });
        let start = std::time::Instant::now();
        for _ in 0..n {
            let _ = chunk.mesh();
        }
        let mesh = start.elapsed() / n as u32;
        let start = std::time::Instant::now();
        for _ in 0..n {
            let _ = chunk.collider();
        }
        let col = start.elapsed() / n as u32;
        println!("per chunk: generate {made:?}, mesh {mesh:?}, collider {col:?}");
    }
}
