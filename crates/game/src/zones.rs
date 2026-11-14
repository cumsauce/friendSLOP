//! Scenery districts across the expanded ±90 m map (players asked for room to
//! run around and hide, walls and scale, "more than one little area"):
//!
//! - **North forest** (z −35..−85): tall trees with trunk colliders,
//!   undergrowth, fallen logs, boulders and a hidden clearing with a
//!   woodcutter's hut ruin.
//! - **South-west ruins** (x −80..−30, z 30..85): a walled hedge maze with a
//!   statue at its heart, tall broken castle walls with an archway, a collapsed
//!   tower you can walk into, and rock faces, crates and a wooden crane around
//!   the quarry pit's rim.
//! - **West valley** (x −85..−35, z −30..30): sparse trees, rocks, a shepherd's
//!   shelter and fences.
//! - **Meadows** (everything else outside the commune): small copses, low
//!   stone walls and hedgerows dividing fields, haystacks, and a windmill whose
//!   sails turn slowly.
//!
//! The east village (x 35..80, z −35..35) is left empty for the houses.
//! Like `scenery.rs`, nothing is replicated: every peer builds the same layout
//! from fixed seeds, everything sits on `terrain::height_at`, models (logs,
//! the hut, statue, archway, crane, windmill and the field walls and
//! hedgerows come from `assets/models/scenery`) degrade to nothing without LFS
//! while their colliders and the primitive-built maze and castle walls stay.

use avian3d::prelude::*;
use bevy::prelude::*;
use std::f32::consts::TAU;

use crate::AppState;
use crate::art;
use crate::scenery::{BUSHES, Cluster, Dresser, Rng, TREES, ground, make_kit};
pub use friendslop_base::zones::*;

pub fn plugin(app: &mut App) {
    app.init_resource::<crate::scenery::KitCache>()
        .add_systems(OnEnter(AppState::InGame), dress_zones)
        .add_systems(Update, turn_sails.run_if(in_state(AppState::InGame)));
}

pub use friendslop_world::places::{forest_clearing_at, maze_at};
/// Maze cells per side and cell size (24 × 24 m).
const MAZE_CELLS: usize = 8;
const MAZE_CELL: f32 = 3.0;
use friendslop_world::places::quarry_at;
pub use friendslop_world::places::windmill_at;

/// Water and the windmill's yard stay open everywhere.
fn wet_or_taken(p: Vec2, margin: f32) -> bool {
    crate::terrain::is_pond(p.x, p.y)
        || crate::terrain::is_lake(p.x, p.y)
        || crate::terrain::is_lake(p.x + margin, p.y)
        || crate::terrain::is_pond(p.x + margin, p.y)
        || p.distance(windmill_at()) < 6.0 + margin
        || crate::features::warehouse::covers(p, margin)
        || crate::features::wendys::covers(p, margin)
        || crate::features::underground::covers(p, margin)
        || friendslop_world::footprints::claimed(p, margin)
}

/// Is `p` free for a prop of `zone` (with `margin` of footprint)?
fn free(p: Vec2, zone: Zone, margin: f32) -> bool {
    if wet_or_taken(p, margin) {
        return false;
    }
    // The footprint must not poke into another district.
    [
        Vec2::ZERO,
        Vec2::X * margin,
        -Vec2::X * margin,
        Vec2::Y * margin,
        -Vec2::Y * margin,
    ]
    .iter()
    .all(|o| zone_of(p + *o) == zone)
}

/// Is `p` part of a set piece (maze, castle, quarry, clearing)?
fn in_set_piece(p: Vec2, margin: f32) -> bool {
    let half = MAZE_CELLS as f32 * MAZE_CELL / 2.0 + 2.0 + margin;
    (p.x - maze_at().x).abs() < half && (p.y - maze_at().y).abs() < half
        || p.distance(quarry_at()) < 13.0 + margin
        || p.distance(forest_clearing_at()) < 9.0 + margin
        || ((-48.0..-30.0).contains(&p.x) && (36.0..84.0).contains(&p.y))
}

/// Rejection-samples up to `n` points in `zone`, spaced `spacing` apart.
/// How many of a thing to scatter in a zone, how far apart (m) and how far
/// in from the zone's edge (m).
#[derive(serde::Deserialize, Default, Debug, Clone, Copy, PartialEq)]
pub struct Spread {
    pub count: usize,
    pub spacing: f32,
    pub margin: f32,
}

impl Spread {
    fn scatter(&self, rng: &mut Rng, zone: Zone) -> Vec<Vec2> {
        scatter(rng, zone, self.count, self.spacing, self.margin)
    }
}

/// How much of the world's dressing there is: `data/game/scenery.ron`.
#[derive(serde::Deserialize, Default, Debug)]
pub struct Scenery {
    /// The commune's clusters (groves, outcrops, sheds...) and loose flowers.
    pub commune_clusters: Spread,
    pub commune_flora: u32,
    pub forest_trees: Spread,
    pub forest_logs: Spread,
    pub forest_rocks: Spread,
    pub valley_trees: Spread,
    pub valley_rocks: Spread,
    pub valley_fences: Spread,
    pub meadow_copses: Spread,
    pub meadow_walls: Spread,
    pub meadow_hay: Spread,
    pub meadow_boulders: Spread,
    /// Wildflower patches in the valley and the meadows.
    #[serde(default)]
    pub valley_flowers: Spread,
    #[serde(default)]
    pub meadow_flowers: Spread,
    /// Boulder sets in the valley.
    #[serde(default)]
    pub valley_boulders: Spread,
    /// Reed clumps round the lake shore: how many tries round its edge.
    #[serde(default)]
    pub lake_reeds: usize,
    /// Country props in the meadows and valley (scarecrows, beehives, wheelbarrows).
    #[serde(default)]
    pub country_props: Spread,
    /// A lantern every this many metres along the map's paths (0: none).
    #[serde(default)]
    pub path_lantern_every: f32,
    /// Trees on the rim hills past the play area, so the edge isn't bare.
    #[serde(default)]
    pub rim_trees: usize,
}

pub static SCENERY: friendslop_data::Data<Scenery> =
    friendslop_data::Data::ron("data/game/scenery.ron");

fn scatter(rng: &mut Rng, zone: Zone, n: usize, spacing: f32, margin: f32) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::new();
    for _ in 0..n * 25 {
        if out.len() >= n {
            break;
        }
        let p = Vec2::new(
            rng.range(-map_half(), map_half()),
            rng.range(-map_half(), map_half()),
        );
        if !free(p, zone, margin)
            || in_set_piece(p, margin)
            || out.iter().any(|q| q.distance(p) < spacing)
        {
            continue;
        }
        out.push(p);
    }
    out
}

/// Extra materials the districts need on top of the shared kit.
struct Extra {
    hedge: Handle<StandardMaterial>,
    castle: Handle<StandardMaterial>,
}

/// The windmill's sail hub; spins slowly around its local Z.
#[derive(Component)]
struct Sails;

fn turn_sails(time: Res<Time>, mut sails: Query<&mut Transform, With<Sails>>) {
    for mut t in &mut sails {
        t.rotate_local_z(time.delta_secs() * 0.35);
    }
}

fn dress_zones(
    commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<crate::scenery::KitCache>,
) {
    let kit = make_kit(&mut cache, &mut meshes, &mut materials);
    let x = Extra {
        hedge: art::soft(&mut materials, Color::srgb(0.24, 0.42, 0.2)),
        castle: art::soft(&mut materials, Color::srgb(0.62, 0.6, 0.57)),
    };
    let mut d = Dresser {
        commands,
        assets: &assets,
        kit,
    };
    forest(&mut d, &mut Rng(0xF0_2E57));
    maze(&mut d, &x, &mut Rng(0x3A_2E00));
    castle(&mut d, &x, &mut Rng(0xCA_571E));
    quarry(&mut d, &mut Rng(0x0_0A22));
    valley(&mut d, &mut Rng(0x7A_11E7));
    meadows(&mut d, &mut Rng(0x3E_AD0));
    wildlife_detail(&mut d, &mut Rng(0xDE_7A11));
    windmill(&mut d);
    map_blocks(&mut d);
}

fn v3(p: Vec2, y: f32) -> Vec3 {
    Vec3::new(p.x, y, p.y)
}

/// The map's static colliders (`Block` markers: the hut's walls, the
/// archway, the crane's posts, the windmill), from the embedded map data so
/// collisions never wait on a model file.
fn map_blocks(d: &mut Dresser) {
    for b in &friendslop_world::places::layout().blocks {
        // Stand on the named entity's ground (`solid_*` adds its own).
        let lift = ground(b.ground_at) - ground(b.centre.xz());
        let centre = b.centre + Vec3::Y * lift;
        let rot = Quat::from_rotation_y(b.yaw);
        if b.round {
            d.solid_cylinder(centre, b.size.x / 2.0, b.size.y, rot);
        } else {
            d.solid_box(centre, b.size, rot);
        }
    }
}

/// A big tree (8–14 m) with a trunk collider; only the biggest cast shadows.
fn big_tree(d: &mut Dresser, rng: &mut Rng, at: Vec2, lo: f32, hi: f32) {
    let scale = rng.range(lo, hi);
    let file = *rng.pick(TREES);
    d.model(file, at, rng.yaw(), scale, scale < 1.75);
    d.blocker(
        Transform::from_xyz(at.x, 1.5 * scale, at.y),
        Collider::cylinder(0.3 * scale, 3.0 * scale),
    );
}

fn fallen_log(d: &mut Dresser, rng: &mut Rng, c: Vec2) {
    let len = rng.range(4.0, 7.0);
    let r = rng.range(0.35, 0.55);
    let yaw = rng.yaw();
    d.log(c, yaw, len, r);
}

fn forest(d: &mut Dresser, rng: &mut Rng) {
    let t = SCENERY.get();
    let trees = t.forest_trees.scatter(rng, Zone::Forest);
    for p in &trees {
        big_tree(d, rng, *p, 1.2, 2.0);
    }
    // Undergrowth between the trunks (no shadows).
    for p in trees.iter().step_by(2) {
        d.undergrowth(rng, *p, 3.0, 2);
    }
    for p in trees.iter().step_by(4) {
        let s = rng.range(1.0, 1.6);
        let q = rng.around(*p, 2.5);
        d.model(rng.pick(BUSHES), q, rng.yaw(), s, true);
    }
    for c in t.forest_logs.scatter(rng, Zone::Forest) {
        fallen_log(d, rng, c);
    }
    for c in t.forest_rocks.scatter(rng, Zone::Forest) {
        let s = rng.range(0.8, 1.6);
        d.rock(rng, c, s);
    }

    // The hidden clearing's hut ruin, stump and log pile are on the map
    // (commune-map in the modeler); undergrowth grows around them.
    let c = forest_clearing_at();
    d.undergrowth(rng, c, 7.0, 10);
}

/// A perfect maze (one path between any two cells, so plenty of dead ends)
/// carved by a seeded depth-first walk, entered from the east, with a gilded
/// statue in the middle.
fn maze(d: &mut Dresser, x: &Extra, rng: &mut Rng) {
    const N: usize = MAZE_CELLS;
    // east[i][j]: wall between (i,j) and (i+1,j); south[i][j]: (i,j)–(i,j+1).
    let mut east = [[true; N]; N];
    let mut south = [[true; N]; N];
    let mut seen = [[false; N]; N];
    let mut stack = vec![(0usize, 0usize)];
    seen[0][0] = true;
    while let Some(&(i, j)) = stack.last() {
        let mut next = Vec::new();
        if i > 0 && !seen[i - 1][j] {
            next.push((i - 1, j));
        }
        if i + 1 < N && !seen[i + 1][j] {
            next.push((i + 1, j));
        }
        if j > 0 && !seen[i][j - 1] {
            next.push((i, j - 1));
        }
        if j + 1 < N && !seen[i][j + 1] {
            next.push((i, j + 1));
        }
        if next.is_empty() {
            stack.pop();
            continue;
        }
        let (a, b) = *rng.pick(&next);
        if a != i {
            east[i.min(a)][j] = false;
        } else {
            south[i][j.min(b)] = false;
        }
        seen[a][b] = true;
        stack.push((a, b));
    }
    // Entrance on the east wall, middle row.
    east[N - 1][N / 2] = false;
    // Open the centre 2×2 into a little courtyard.
    let m = N / 2;
    east[m - 1][m - 1] = false;
    east[m - 1][m] = false;
    south[m - 1][m - 1] = false;
    south[m][m - 1] = false;

    let half = N as f32 * MAZE_CELL / 2.0;
    let origin = maze_at() - Vec2::splat(half);
    let (h, t) = (3.0, 0.7);
    let mut wall = |d: &mut Dresser, a: Vec2, b: Vec2| {
        let mid = (a + b) / 2.0;
        let len = a.distance(b) + t;
        let size = if (a.x - b.x).abs() > 0.1 {
            Vec3::new(len, h, t)
        } else {
            Vec3::new(t, h, len)
        };
        // Stone stretches stand 3 cm proud of the hedge all round, so where
        // they meet a hedge their faces never coincide (z-fighting).
        let (mat, size) = if rng.f() < 0.2 {
            (&x.castle, size + Vec3::splat(0.06))
        } else {
            (&x.hedge, size)
        };
        d.block(v3(mid, size.y / 2.0), size, Quat::IDENTITY, mat, true);
    };
    let at = |i: usize, j: usize| origin + Vec2::new(i as f32, j as f32) * MAZE_CELL;
    // Outer north and west edges.
    for k in 0..N {
        wall(d, at(k, 0), at(k + 1, 0));
        wall(d, at(0, k), at(0, k + 1));
    }
    for i in 0..N {
        for j in 0..N {
            if east[i][j] {
                wall(d, at(i + 1, j), at(i + 1, j + 1));
            }
            if south[i][j] {
                wall(d, at(i, j + 1), at(i + 1, j + 1));
            }
        }
    }
    // The prize: a gilded statue on a plinth (modeler model).
    let c = maze_at();
    d.prop(
        "maze_statue",
        Transform::from_translation(v3(c, 0.0)),
        false,
    );
    d.solid_box(v3(c, 0.4), Vec3::new(1.6, 0.8, 1.6), Quat::IDENTITY);
    d.solid_cylinder(v3(c, 1.5), 0.35, 1.4, Quat::IDENTITY);
}

fn castle(d: &mut Dresser, x: &Extra, rng: &mut Rng) {
    let stone = x.castle.clone();
    let mossy = d.kit.mossy.clone();
    // Long curtain wall along x = −34, broken into tall ragged segments.
    let wall_x = -34.0;
    let mut z = 40.0;
    while z < 80.0 {
        let len = rng.range(2.5, 5.0);
        // The archway gap at z≈58.
        if z < 61.0 && z + len > 55.0 {
            z += len;
            continue;
        }
        if rng.f() < 0.15 {
            // A collapsed gap you can slip through.
            z += len + 1.5;
            continue;
        }
        let h = rng.range(6.0, 10.0);
        let mat = if rng.f() < 0.3 { &mossy } else { &stone };
        d.block(
            Vec3::new(wall_x, h / 2.0, z + len / 2.0),
            Vec3::new(1.4, h, len),
            Quat::IDENTITY,
            mat,
            true,
        );
        z += len;
    }
    // The archway (model and colliders) is on the map, in the wall's gap.
    // A cross wall at the north end, with a window gap.
    for (x0, x1, h) in [(-46.0, -41.0, 7.0), (-39.5, -34.0, 9.0)] {
        d.block(
            Vec3::new((x0 + x1) / 2.0, h / 2.0, 80.0),
            Vec3::new(x1 - x0, h, 1.4),
            Quat::IDENTITY,
            &stone,
            true,
        );
    }
    // The collapsed tower: a ring of ragged stones with a doorway, walk in.
    let tower = friendslop_world::places::ruins_tower_at();
    let segs = 14;
    let r = 3.8;
    for i in 0..segs {
        if i == 3 || i == 4 {
            continue; // doorway facing south-east-ish
        }
        let a = i as f32 / segs as f32 * TAU;
        let p = tower + Vec2::from_angle(a) * r;
        let h = if (8..12).contains(&i) {
            rng.range(1.0, 3.0) // the collapsed side
        } else {
            rng.range(5.0, 9.5)
        };
        d.block(
            v3(p, h / 2.0),
            Vec3::new(1.0, h, 1.9),
            Quat::from_rotation_y(-a),
            &stone,
            true,
        );
    }
    // Rubble everywhere around the ruins (small, walk over or crouch behind).
    for _ in 0..40 {
        let p = Vec2::new(rng.range(-47.0, -31.0), rng.range(38.0, 82.0));
        let s = rng.range(0.3, 1.1);
        let tumble = rng.yaw() * Quat::from_rotation_x(rng.range(-0.5, 0.5));
        d.block(v3(p, s * 0.4), Vec3::splat(s), tumble, &stone, s > 0.8);
    }
    for _ in 0..6 {
        let p = Vec2::new(rng.range(-47.0, -36.0), rng.range(40.0, 78.0));
        d.undergrowth(rng, p, 3.0, 4);
    }
}

fn quarry(d: &mut Dresser, rng: &mut Rng) {
    let c = quarry_at();
    // Rock faces around the rim (the pit itself is the terrain's).
    for i in 0..10 {
        let a = i as f32 / 10.0 * TAU + rng.range(-0.2, 0.2);
        let p = c + Vec2::from_angle(a) * rng.range(10.0, 12.0);
        let s = rng.range(1.4, 2.4);
        // Big rocks must not bury the hedge maze's outer walls.
        let half = MAZE_CELLS as f32 * MAZE_CELL / 2.0 + 2.0 + 3.0;
        if (p.x - maze_at().x).abs() < half && (p.y - maze_at().y).abs() < half {
            continue;
        }
        d.rock(rng, p, s);
    }
    // Crate stacks and a wooden crane on the east rim.
    for a in [0.3_f32, 2.4, 4.4] {
        let p = c + Vec2::from_angle(a) * 8.5;
        d.cluster(rng, Cluster::Crates, p);
    }
    // The wooden crane on the east rim is on the map.
}

/// The field wall and hedgerow segment models (`fn-stone-wall*`,
/// `fn-hedge-segment`): 2 m along X, base at the origin, this tall and thick
/// at scale 1, so a segment is scaled to the box its collider fills.
const WALL_SEG: Vec3 = Vec3::new(2.0, 1.2, 0.6);
const HEDGE_SEG: Vec3 = Vec3::new(2.0, 1.37, 1.0);

/// A low wall (stone or hedgerow) ~1.2 m high: cover to crouch behind.
fn low_wall(d: &mut Dresser, rng: &mut Rng, c: Vec2, len: f32) {
    let yaw_a = rng.f() * TAU;
    let dir = Vec2::from_angle(-yaw_a);
    let hedge = rng.f() < 0.4;
    let segs = (len / 2.0).ceil() as i32;
    for i in 0..segs {
        if rng.f() < 0.08 {
            continue; // a gap for slipping through
        }
        let t = (i as f32 + 0.5) / segs as f32 - 0.5;
        let p = c + dir * t * len;
        if zone_of(p) != Zone::Meadow || wet_or_taken(p, 1.0) {
            continue;
        }
        let h = if hedge {
            rng.range(1.2, 1.5)
        } else {
            rng.range(1.0, 1.3)
        };
        let (file, model) = if hedge {
            ("hedge_segment", HEDGE_SEG)
        } else if rng.f() < 0.3 {
            ("stone_wall_mossy", WALL_SEG)
        } else {
            ("stone_wall", WALL_SEG)
        };
        // Neighbouring segments overlap by 5 cm, and the colliders alternate
        // thickness as the old boxes did.
        let size = Vec3::new(
            len / segs as f32 + 0.05,
            h,
            if hedge { 1.0 } else { 0.6 } + (i % 2) as f32 * 0.04,
        );
        let rot = Quat::from_rotation_y(yaw_a);
        d.prop(
            file,
            Transform::from_translation(v3(p, 0.0))
                .with_rotation(rot)
                .with_scale(size / model),
            false,
        );
        d.solid_box(v3(p, h / 2.0), size, rot);
    }
}

fn valley(d: &mut Dresser, rng: &mut Rng) {
    let t = SCENERY.get();
    for p in t.valley_trees.scatter(rng, Zone::Valley) {
        big_tree(d, rng, p, 1.0, 1.6);
    }
    for p in t.valley_rocks.scatter(rng, Zone::Valley) {
        let s = rng.range(0.6, 1.4);
        d.rock(rng, p, s);
    }
    for p in t.valley_fences.scatter(rng, Zone::Valley) {
        d.cluster(rng, Cluster::Fence, p);
    }
    // The shepherd's shelter.
    if let Some(p) = scatter(rng, Zone::Valley, 1, 0.0, 4.0).first() {
        d.cluster(rng, Cluster::Shed, *p);
    }
}

fn meadows(d: &mut Dresser, rng: &mut Rng) {
    let t = SCENERY.get();
    // Small copses.
    for c in t.meadow_copses.scatter(rng, Zone::Meadow) {
        for _ in 0..(3 + (rng.f() * 3.0) as u32) {
            let p = rng.around(c, 4.0);
            if free(p, Zone::Meadow, 1.0) {
                big_tree(d, rng, p, 0.9, 1.5);
            }
        }
        let s = rng.range(1.0, 1.5);
        d.model(rng.pick(BUSHES), c, rng.yaw(), s, true);
        d.undergrowth(rng, c, 4.0, 4);
    }
    // Field walls and hedgerows.
    for c in t.meadow_walls.scatter(rng, Zone::Meadow) {
        let len = rng.range(10.0, 18.0);
        low_wall(d, rng, c, len);
    }
    // Haystacks.
    for c in t.meadow_hay.scatter(rng, Zone::Meadow) {
        d.cluster(rng, Cluster::Hay, c);
    }
    // Scattered single boulders.
    for c in t.meadow_boulders.scatter(rng, Zone::Meadow) {
        let s = rng.range(0.5, 1.0);
        d.rock(rng, c, s);
    }
}

/// A `models/prefabs` model on the ground at `at` (modeler `zn-*` prefabs).
fn prefab(d: &mut Dresser, file: &str, at: Vec2, yaw: Quat, scale: f32, small: bool) {
    let mut e = d.commands.spawn((
        bevy::world_serialization::WorldAssetRoot(
            d.assets
                .load(GltfAssetLabel::Scene(0).from_asset(format!("models/prefabs/{file}.gltf"))),
        ),
        Transform::from_xyz(at.x, ground(at), at.y)
            .with_rotation(yaw)
            .with_scale(Vec3::splat(scale)),
        DespawnOnExit(AppState::InGame),
    ));
    if small {
        e.insert(crate::scenery::SmallProp);
    }
}

/// Finer dressing (M4 of the detail plan): wildflower drifts, boulder sets,
/// reeds round the lake. Its own seed, so the older dressing stays put.
fn wildlife_detail(d: &mut Dresser, rng: &mut Rng) {
    let t = SCENERY.get();
    for (spread, zone) in [
        (&t.valley_flowers, Zone::Valley),
        (&t.meadow_flowers, Zone::Meadow),
    ] {
        for p in spread.scatter(rng, zone) {
            let s = rng.range(0.8, 1.6);
            prefab(d, "zn-wildflowers", p, rng.yaw(), s, true);
        }
    }
    for p in t.valley_boulders.scatter(rng, Zone::Valley) {
        let s = rng.range(0.7, 1.3);
        let yaw = rng.yaw();
        prefab(d, "zn-boulders", p, yaw, s, false);
        d.blocker(
            Transform::from_xyz(p.x, 0.6 * s, p.y).with_rotation(yaw),
            Collider::sphere(0.85 * s),
        );
    }
    // Reeds where dry ground meets the lake.
    let lake = friendslop_world::places::lake_at();
    for _ in 0..t.lake_reeds {
        let dir = Vec2::from_angle(rng.f() * TAU);
        let mut r = 2.0;
        while r < 80.0 && crate::terrain::is_lake(lake.x + dir.x * r, lake.y + dir.y * r) {
            r += 0.5;
        }
        if r >= 80.0 {
            continue;
        }
        let p = lake + dir * (r + rng.range(-0.6, 0.8));
        let s = rng.range(0.8, 1.3);
        prefab(d, "zn-reeds", p, rng.yaw(), s, true);
    }
    // Country props: scarecrows, beehives, wheelbarrows (each blocks a little).
    const COUNTRY: [(&str, f32); 3] = [
        ("zn-scarecrow", 0.4),
        ("zn-beehives", 1.1),
        ("zn-wheelbarrow", 0.5),
    ];
    let mut pts = t.country_props.scatter(rng, Zone::Meadow);
    pts.extend(
        t.country_props
            .scatter(rng, Zone::Valley)
            .into_iter()
            .take(3),
    );
    for (i, p) in pts.into_iter().enumerate() {
        let (file, r) = COUNTRY[i % COUNTRY.len()];
        let yaw = rng.yaw();
        prefab(d, file, p, yaw, 1.0, true);
        d.blocker(
            Transform::from_xyz(p.x, 0.5, p.y),
            Collider::cylinder(r, 1.0),
        );
    }
    // Lanterns along the paths, alternating sides.
    if t.path_lantern_every > 0.5 {
        for (a, b) in &friendslop_world::places::layout().paths {
            let len = a.distance(*b);
            let dir = (*b - *a) / len.max(0.01);
            let side = dir.perp() * 1.8;
            let n = (len / t.path_lantern_every) as usize;
            for i in 1..n {
                let p = *a
                    + dir * (i as f32 * t.path_lantern_every)
                    + if i % 2 == 0 { side } else { -side };
                if wet_or_taken(p, 0.5) || zone_of(p) == Zone::Outside {
                    continue;
                }
                prefab(d, "zn-path-lantern", p, Quat::IDENTITY, 1.0, true);
                d.blocker(
                    Transform::from_xyz(p.x, 0.8, p.y),
                    Collider::cuboid(0.12, 1.6, 0.12),
                );
            }
        }
    }
    // The rim hills: trees past the edge (out of reach, so no colliders).
    for _ in 0..t.rim_trees {
        let edge = rng.range(90.0, 106.0);
        let along = rng.range(-edge, edge);
        let p = match (rng.f() * 4.0) as u32 {
            0 => Vec2::new(edge, along),
            1 => Vec2::new(-edge, along),
            2 => Vec2::new(along, edge),
            _ => Vec2::new(along, -edge),
        };
        let s = rng.range(1.3, 2.2);
        d.model(rng.pick(TREES), p, rng.yaw(), s, true);
    }
}

/// A whitewashed tower windmill (~12 m) with slowly turning sails.
fn windmill(d: &mut Dresser) {
    let c = windmill_at();
    let g = ground(c);
    // The tower (model and colliders) is on the map; the sails are another
    // model, on the hub entity `turn_sails` spins.
    // Sails on the south face (towards the commune), turning around +Z.
    d.commands.spawn((
        crate::scenery::scenery_model(d.assets, "windmill_sails"),
        Transform::from_xyz(c.x, g + 9.5, c.y + 2.4),
        Sails,
        DespawnOnExit(AppState::InGame),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scenery_file_has_every_spread() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/data/game/scenery.ron");
        let Ok(bytes) = std::fs::read(path) else {
            panic!("no {path}");
        };
        assert!(SCENERY.set_bytes(&bytes), "{path} doesn't parse");
        let t = SCENERY.get();
        for s in [
            t.commune_clusters,
            t.forest_trees,
            t.forest_logs,
            t.forest_rocks,
            t.valley_trees,
            t.valley_rocks,
            t.valley_fences,
            t.meadow_copses,
            t.meadow_walls,
            t.meadow_hay,
            t.meadow_boulders,
            t.valley_flowers,
            t.meadow_flowers,
            t.valley_boulders,
            t.country_props,
        ] {
            assert!(s.count > 0 && s.spacing > 0.0, "{s:?}");
        }
        assert!(
            t.commune_flora > 0
                && t.lake_reeds > 0
                && t.rim_trees > 0
                && t.path_lantern_every > 0.0
        );
    }

    #[test]
    fn districts_are_where_the_layout_says() {
        assert_eq!(zone_of(Vec2::ZERO), Zone::Commune);
        assert_eq!(zone_of(Vec2::new(50.0, 0.0)), Zone::Village);
        assert_eq!(zone_of(Vec2::new(0.0, -60.0)), Zone::Forest);
        assert_eq!(zone_of(Vec2::new(-55.0, 55.0)), Zone::Ruins);
        assert_eq!(zone_of(Vec2::new(-60.0, 0.0)), Zone::Valley);
        assert_eq!(zone_of(Vec2::new(50.0, 60.0)), Zone::Meadow);
        assert_eq!(zone_of(Vec2::new(95.0, 0.0)), Zone::Outside);
    }

    #[test]
    fn nothing_scatters_into_the_village() {
        let pts = scatter(&mut Rng(1), Zone::Meadow, 200, 2.0, 2.0);
        assert!(!pts.is_empty());
        assert!(pts.iter().all(|p| zone_of(*p) == Zone::Meadow));
    }
}
