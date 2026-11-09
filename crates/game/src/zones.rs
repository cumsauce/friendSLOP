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
//! from fixed seeds, everything sits on `terrain::height_at`, models degrade
//! to nothing without LFS while their colliders and the primitive-built cover
//! stay.

use avian3d::prelude::*;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::AppState;
use crate::art;
use crate::scenery::{BUSHES, Cluster, Dresser, Rng, TREES, ground, make_kit};

pub fn plugin(app: &mut App) {
    app.init_resource::<crate::scenery::KitCache>()
        .add_systems(OnEnter(AppState::InGame), dress_zones)
        .add_systems(Update, turn_sails.run_if(in_state(AppState::InGame)));
}

/// Nothing goes past this half-extent.
const MAP_HALF: f32 = 88.0;
/// The commune's own dressing (scenery.rs) covers this half-extent.
const COMMUNE_HALF: f32 = 27.0;

pub const FOREST_CLEARING: Vec2 = Vec2::new(-28.0, -62.0);
pub const MAZE_AT: Vec2 = Vec2::new(-66.0, 44.0);
/// Maze cells per side and cell size (24 × 24 m).
const MAZE_CELLS: usize = 8;
const MAZE_CELL: f32 = 3.0;
const QUARRY_AT: Vec2 = Vec2::new(-55.0, 65.0);
pub const WINDMILL_AT: Vec2 = Vec2::new(-8.0, -31.0);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Commune,
    Village,
    Forest,
    Ruins,
    Valley,
    Meadow,
    Outside,
}

pub fn zone_of(p: Vec2) -> Zone {
    if p.x.abs() > MAP_HALF || p.y.abs() > MAP_HALF {
        Zone::Outside
    } else if p.x.abs() < COMMUNE_HALF && p.y.abs() < COMMUNE_HALF {
        Zone::Commune
    } else if (33.0..82.0).contains(&p.x) && (-37.0..37.0).contains(&p.y) {
        Zone::Village
    } else if p.y < -35.0 {
        Zone::Forest
    } else if p.x < -30.0 && p.y > 30.0 {
        Zone::Ruins
    } else if p.x < -35.0 && p.y.abs() <= 30.0 {
        Zone::Valley
    } else {
        Zone::Meadow
    }
}

/// Water and the windmill's yard stay open everywhere.
fn wet_or_taken(p: Vec2, margin: f32) -> bool {
    crate::terrain::is_pond(p.x, p.y)
        || crate::terrain::is_lake(p.x, p.y)
        || crate::terrain::is_lake(p.x + margin, p.y)
        || crate::terrain::is_pond(p.x + margin, p.y)
        || p.distance(WINDMILL_AT) < 6.0 + margin
        || crate::features::warehouse::covers(p, margin)
        || p.distance(crate::features::wendys::WENDYS_AT) < 6.0 + margin
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
    (p.x - MAZE_AT.x).abs() < half && (p.y - MAZE_AT.y).abs() < half
        || p.distance(QUARRY_AT) < 13.0 + margin
        || p.distance(FOREST_CLEARING) < 9.0 + margin
        || ((-48.0..-30.0).contains(&p.x) && (36.0..84.0).contains(&p.y))
}

/// Rejection-samples up to `n` points in `zone`, spaced `spacing` apart.
fn scatter(rng: &mut Rng, zone: Zone, n: usize, spacing: f32, margin: f32) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::new();
    for _ in 0..n * 25 {
        if out.len() >= n {
            break;
        }
        let p = Vec2::new(
            rng.range(-MAP_HALF, MAP_HALF),
            rng.range(-MAP_HALF, MAP_HALF),
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
    gold: Handle<StandardMaterial>,
    sail: Handle<StandardMaterial>,
    whitewash: Handle<StandardMaterial>,
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
        gold: art::glossy(&mut materials, Color::srgb(0.9, 0.72, 0.3)),
        sail: art::soft(&mut materials, Color::srgb(0.93, 0.9, 0.82)),
        whitewash: art::soft(&mut materials, Color::srgb(0.9, 0.87, 0.8)),
    };
    let cone = meshes.add(Cone::new(1.0, 1.0));
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
    meadows(&mut d, &x, &mut Rng(0x3E_AD0));
    windmill(&mut d, &x, cone);
}

fn v3(p: Vec2, y: f32) -> Vec3 {
    Vec3::new(p.x, y, p.y)
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
    let bark = d.kit.bark.clone();
    let len = rng.range(4.0, 7.0);
    let r = rng.range(0.35, 0.55);
    let lie = rng.yaw() * Quat::from_rotation_z(FRAC_PI_2);
    d.cylinder(v3(c, r), r, len, lie, &bark, true);
}

fn forest(d: &mut Dresser, rng: &mut Rng) {
    let trees = scatter(rng, Zone::Forest, 230, 3.6, 1.0);
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
    for c in scatter(rng, Zone::Forest, 18, 8.0, 3.0) {
        fallen_log(d, rng, c);
    }
    for c in scatter(rng, Zone::Forest, 22, 8.0, 2.5) {
        let s = rng.range(0.8, 1.6);
        d.rock(rng, c, s);
    }

    // The hidden clearing: a woodcutter's hut ruin, chopping stump, log pile.
    let c = FOREST_CLEARING;
    let (plank, wood, bark, stone) = (
        d.kit.plank.clone(),
        d.kit.wood.clone(),
        d.kit.bark.clone(),
        d.kit.stone.clone(),
    );
    let (w, dp) = (5.0, 4.0);
    // Back wall, broken side walls, front with a doorway.
    d.block(
        v3(c + Vec2::new(0.0, -dp / 2.0), 1.3),
        Vec3::new(w, 2.6, 0.25),
        Quat::IDENTITY,
        &plank,
        true,
    );
    d.block(
        v3(c + Vec2::new(-w / 2.0, 0.0), 1.0),
        Vec3::new(0.25, 2.0, dp),
        Quat::IDENTITY,
        &plank,
        true,
    );
    d.block(
        v3(c + Vec2::new(w / 2.0, -0.8), 0.7),
        Vec3::new(0.25, 1.4, dp - 1.6),
        Quat::IDENTITY,
        &plank,
        true,
    );
    d.block(
        v3(c + Vec2::new(-1.6, dp / 2.0), 0.9),
        Vec3::new(1.8, 1.8, 0.25),
        Quat::IDENTITY,
        &plank,
        true,
    );
    // Stone chimney, collapsed roof beam, chopping stump and a log pile.
    d.block(
        v3(c + Vec2::new(-w / 2.0 - 0.4, -1.0), 2.0),
        Vec3::new(0.9, 4.0, 0.9),
        Quat::IDENTITY,
        &stone,
        true,
    );
    d.block(
        v3(c + Vec2::new(0.4, 0.2), 1.1),
        Vec3::new(5.6, 0.25, 0.25),
        Quat::from_rotation_y(0.3) * Quat::from_rotation_z(0.35),
        &wood,
        false,
    );
    d.cylinder(
        v3(c + Vec2::new(3.5, 3.5), 0.35),
        0.5,
        0.7,
        Quat::IDENTITY,
        &bark,
        true,
    );
    for i in 0..3 {
        for j in 0..(3 - i) {
            let p = c + Vec2::new(-4.5, 3.0 + j as f32 * 0.6 + i as f32 * 0.3);
            d.cylinder(
                v3(p, 0.3 + i as f32 * 0.5),
                0.25,
                2.2,
                Quat::from_rotation_z(FRAC_PI_2),
                &bark,
                j == 0,
            );
        }
    }
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
    let origin = MAZE_AT - Vec2::splat(half);
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
    // The prize: a gilded statue on a plinth.
    let c = MAZE_AT;
    d.block(
        v3(c, 0.4),
        Vec3::new(1.6, 0.8, 1.6),
        Quat::IDENTITY,
        &x.castle,
        true,
    );
    d.cylinder(v3(c, 1.5), 0.35, 1.4, Quat::IDENTITY, &x.gold, true);
    d.block(
        v3(c, 2.5),
        Vec3::splat(0.6),
        Quat::from_rotation_y(PI / 4.0),
        &x.gold,
        false,
    );
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
    // The archway: two pillars and a lintel.
    for dz in [-2.2, 2.2] {
        d.block(
            Vec3::new(wall_x, 4.0, 58.0 + dz),
            Vec3::new(1.8, 8.0, 1.6),
            Quat::IDENTITY,
            &stone,
            true,
        );
    }
    d.block(
        Vec3::new(wall_x, 7.2, 58.0),
        Vec3::new(1.8, 1.6, 6.0),
        Quat::IDENTITY,
        &stone,
        true,
    );
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
    let tower = Vec2::new(-41.0, 44.0);
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
    let c = QUARRY_AT;
    // Rock faces around the rim (the pit itself is the terrain's).
    for i in 0..10 {
        let a = i as f32 / 10.0 * TAU + rng.range(-0.2, 0.2);
        let p = c + Vec2::from_angle(a) * rng.range(10.0, 12.0);
        let s = rng.range(1.4, 2.4);
        // Big rocks must not bury the hedge maze's outer walls.
        let half = MAZE_CELLS as f32 * MAZE_CELL / 2.0 + 2.0 + 3.0;
        if (p.x - MAZE_AT.x).abs() < half && (p.y - MAZE_AT.y).abs() < half {
            continue;
        }
        d.rock(rng, p, s);
    }
    // Crate stacks and a wooden crane on the east rim.
    for a in [0.3_f32, 2.4, 4.4] {
        let p = c + Vec2::from_angle(a) * 8.5;
        d.cluster(rng, Cluster::Crates, p);
    }
    let wood = d.kit.wood.clone();
    let base = c + Vec2::new(9.0, -3.0);
    // `block` snaps each part to its own ground; on the rim slope that tears
    // the crane apart. Lift every part by the difference to the base's ground.
    let g0 = ground(base);
    let v3 = |p: Vec2, y: f32| Vec3::new(p.x, y + g0 - ground(p), p.y);
    for dz in [-1.2, 1.2] {
        d.block(
            v3(base + Vec2::new(0.0, dz), 3.5),
            Vec3::new(0.35, 7.0, 0.35),
            Quat::IDENTITY,
            &wood,
            true,
        );
    }
    d.block(
        v3(base, 7.0),
        Vec3::new(0.35, 0.35, 2.8),
        Quat::IDENTITY,
        &wood,
        false,
    );
    // The jib, reaching out over the pit, with a rope and a hanging crate.
    d.block(
        v3(base + Vec2::new(-3.0, 0.0), 6.8),
        Vec3::new(6.5, 0.3, 0.3),
        Quat::from_rotation_z(-0.12),
        &wood,
        false,
    );
    d.block(
        v3(base + Vec2::new(-6.0, 0.0), 5.4),
        Vec3::new(0.06, 2.4, 0.06),
        Quat::IDENTITY,
        &wood,
        false,
    );
    let crate_wood = d.kit.crate_wood.clone();
    d.block(
        v3(base + Vec2::new(-6.0, 0.0), 3.8),
        Vec3::splat(0.8),
        Quat::IDENTITY,
        &crate_wood,
        false,
    );
}

/// A low wall (stone or hedgerow) ~1.2 m high: cover to crouch behind.
fn low_wall(d: &mut Dresser, x: &Extra, rng: &mut Rng, c: Vec2, len: f32) {
    let yaw_a = rng.f() * TAU;
    let dir = Vec2::from_angle(-yaw_a);
    let hedge = rng.f() < 0.4;
    let segs = (len / 2.0).ceil() as i32;
    let mossy = d.kit.mossy.clone();
    let stone = d.kit.stone.clone();
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
        let mat = if hedge {
            &x.hedge
        } else if rng.f() < 0.3 {
            &mossy
        } else {
            &stone
        };
        d.block(
            v3(p, h / 2.0),
            // Overlapping neighbours alternate thickness (no coplanar faces).
            Vec3::new(
                len / segs as f32 + 0.05,
                h,
                if hedge { 1.0 } else { 0.6 } + (i % 2) as f32 * 0.04,
            ),
            Quat::from_rotation_y(yaw_a),
            mat,
            true,
        );
    }
}

fn valley(d: &mut Dresser, rng: &mut Rng) {
    for p in scatter(rng, Zone::Valley, 14, 7.0, 1.5) {
        big_tree(d, rng, p, 1.0, 1.6);
    }
    for p in scatter(rng, Zone::Valley, 12, 6.0, 2.0) {
        let s = rng.range(0.6, 1.4);
        d.rock(rng, p, s);
    }
    for p in scatter(rng, Zone::Valley, 5, 10.0, 4.0) {
        d.cluster(rng, Cluster::Fence, p);
    }
    // The shepherd's shelter.
    if let Some(p) = scatter(rng, Zone::Valley, 1, 0.0, 4.0).first() {
        d.cluster(rng, Cluster::Shed, *p);
    }
}

fn meadows(d: &mut Dresser, x: &Extra, rng: &mut Rng) {
    // Small copses.
    for c in scatter(rng, Zone::Meadow, 14, 14.0, 5.0) {
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
    for c in scatter(rng, Zone::Meadow, 18, 12.0, 2.0) {
        let len = rng.range(10.0, 18.0);
        low_wall(d, x, rng, c, len);
    }
    // Haystacks.
    for c in scatter(rng, Zone::Meadow, 10, 12.0, 3.0) {
        d.cluster(rng, Cluster::Hay, c);
    }
    // Scattered single boulders.
    for c in scatter(rng, Zone::Meadow, 10, 10.0, 2.0) {
        let s = rng.range(0.5, 1.0);
        d.rock(rng, c, s);
    }
}

/// A whitewashed tower windmill (~12 m) with slowly turning sails.
fn windmill(d: &mut Dresser, x: &Extra, cone: Handle<Mesh>) {
    let c = WINDMILL_AT;
    let g = ground(c);
    // Tapered tower: stacked cylinders.
    for i in 0..5 {
        let r = 2.6 - i as f32 * 0.25;
        d.cylinder(
            v3(c, 1.0 + i as f32 * 2.0),
            r,
            2.05,
            Quat::IDENTITY,
            &x.whitewash,
            i == 0,
        );
    }
    d.blocker(
        Transform::from_xyz(c.x, 5.0, c.y),
        Collider::cylinder(2.6, 10.0),
    );
    // Door.
    let wood = d.kit.wood.clone();
    d.block(
        v3(c + Vec2::new(0.0, 2.55), 1.0),
        Vec3::new(1.1, 2.0, 0.2),
        Quat::IDENTITY,
        &wood,
        false,
    );
    let roof = d.kit.roof.clone();
    d.commands.spawn((
        Mesh3d(cone),
        MeshMaterial3d(roof),
        Transform::from_xyz(c.x, g + 11.2, c.y).with_scale(Vec3::new(1.9, 2.4, 1.9)),
        DespawnOnExit(AppState::InGame),
    ));
    // Sails on the south face (towards the commune), turning around +Z.
    let cube = d.kit.cube.clone();
    let hub = d
        .commands
        .spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(wood.clone()),
            Transform::from_xyz(c.x, g + 9.5, c.y + 2.4).with_scale(Vec3::ONE),
            Visibility::default(),
            Sails,
            DespawnOnExit(AppState::InGame),
        ))
        .id();
    for k in 0..4 {
        let a = k as f32 * FRAC_PI_2;
        let rot = Quat::from_rotation_z(a);
        let arm = d
            .commands
            .spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(wood.clone()),
                Transform::from_rotation(rot)
                    .with_translation(rot * Vec3::Y * 3.5)
                    .with_scale(Vec3::new(0.18, 7.0, 0.12)),
                NotShadowCaster,
            ))
            .id();
        let sail = d
            .commands
            .spawn((
                Mesh3d(cube.clone()),
                MeshMaterial3d(x.sail.clone()),
                Transform::from_rotation(rot)
                    .with_translation(rot * Vec3::new(0.7, 4.0, 0.0))
                    .with_scale(Vec3::new(1.2, 5.5, 0.05)),
            ))
            .id();
        d.commands.entity(hub).add_children(&[arm, sail]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
