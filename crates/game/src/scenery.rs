//! Dresses the shared floor: clustered groves, rock outcrops, bush thickets,
//! fallen logs and a few code-built ruins, sheds, hay bales, fences and crate
//! stacks, so there is plenty to hide behind (issue #60). Also lights a
//! campfire beside the burnt parliament and loops a little tune.
//!
//! Nothing here is replicated: every peer places the same props the same way
//! from a fixed seed. Models come from `assets/models/nature` (see CREDITS.md)
//! and missing files simply render nothing, but the colliders and the
//! primitive-built cover still exist, so the cover layout never depends on LFS.
//! Large cover (trunks, rocks, walls, sheds, bales, crates) gets a static
//! avian3d collider so it actually blocks.

use avian3d::prelude::*;
use bevy::audio::Volume;
use bevy::gltf::GltfAssetLabel;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use crate::AppState;
use crate::art;
use crate::assets::Sfx;
use crate::features::dave_commune::PARLIAMENT_AT;
use crate::features::duel::{ARENA_CENTER, ARENA_HALF};
use crate::features::landlord::LANDLORD_AT;
use crate::features::totems::VAULT_AT;

pub fn plugin(app: &mut App) {
    app.add_systems(
        OnEnter(AppState::InGame),
        (dress_floor, light_bonfire, play_music),
    )
    .add_systems(Update, flicker_bonfire.run_if(in_state(AppState::InGame)))
    .add_observer(no_shadows_on_small_props);
}

/// Props stay inside this half-extent (the floor is 50×50, and the outer
/// edge may get hills, so keep a margin).
const PLAY_HALF: f32 = 22.5;
/// How many prop clusters to place.
const CLUSTER_COUNT: usize = 44;
/// Minimum distance between cluster centres.
const CLUSTER_SPACING: f32 = 4.2;
/// Scattered loose flora between clusters.
const LOOSE_FLORA: u32 = 70;

/// Small scenery whose meshes shouldn't cast shadows (cheap, and tiny shadows
/// just look noisy).
#[derive(Component)]
struct SmallProp;

fn no_shadows_on_small_props(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    small: Query<(), With<SmallProp>>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
) {
    if small.get(ready.entity).is_err() {
        return;
    }
    for child in children.iter_descendants(ready.entity) {
        if meshes.get(child).is_ok() {
            commands.entity(child).insert(NotShadowCaster);
        }
    }
}

/// Spots that must stay open: (centre, radius).
fn keep_clear() -> Vec<(Vec2, f32)> {
    vec![
        // Centre spawn / overview focus and the mushroom patch's inner ring.
        (Vec2::ZERO, 4.5),
        (BONFIRE_AT.xz(), 3.5),
        (LANDLORD_AT.xz(), 3.0),
        (VAULT_AT, 4.5),
        // Totems, storm debris, animal homes.
        (Vec2::new(8.0, 3.0), 2.5),
        (Vec2::new(9.0, -8.0), 2.5),
        (Vec2::new(10.0, -6.0), 2.0),
        (Vec2::new(-6.0, 11.0), 2.0),
        (Vec2::new(-6.0, -3.0), 2.5),
        (Vec2::new(4.0, 0.0), 2.0),
        (Vec2::new(5.0, 4.0), 2.5),
        (Vec2::new(-4.0, 4.0), 2.0),
        (Vec2::new(3.0, 8.0), 2.0),
        (Vec2::new(-9.0, 2.0), 2.0),
        (Vec2::new(7.0, -2.0), 2.0),
    ]
}

/// Is `p` (with `margin` of footprint) clear of every landmark?
fn is_clear(p: Vec2, margin: f32) -> bool {
    let in_rect = |c: Vec2, half: f32| (p.x - c.x).abs() < half && (p.y - c.y).abs() < half;
    // The arena keeps a wide margin so its walls sit in open grass.
    if in_rect(ARENA_CENTER.xz(), ARENA_HALF + 2.5 + margin) {
        return false;
    }
    // The parliament ruin is bigger than it looks (fallen tower and all).
    if in_rect(PARLIAMENT_AT, 8.0 + margin) {
        return false;
    }
    if p.x.abs() > PLAY_HALF || p.y.abs() > PLAY_HALF {
        return false;
    }
    // Nothing right at the default camera's feet, or a bush fills the screen.
    let eye = crate::camera::CameraRig::default()
        .transform()
        .translation
        .xz();
    if p.distance(eye) < 6.0 + margin {
        return false;
    }
    // Keep the pond and dirt paths (terrain.rs) open.
    if crate::terrain::is_pond(p.x, p.y)
        || crate::terrain::is_pond(p.x + margin, p.y)
        || crate::terrain::is_path(p.x, p.y)
    {
        return false;
    }
    keep_clear()
        .iter()
        .all(|(c, r)| p.distance(*c) >= r + margin)
}

/// The strip between the default overview camera and the middle of the map.
fn in_camera_corridor(p: Vec2) -> bool {
    p.x.abs() < 10.0 && p.y > 2.0
}

/// A tiny deterministic random source, same on every peer (like the mushroom
/// patch's), so everyone places an identical forest.
struct Rng(u32);

impl Rng {
    fn f(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.f() * (hi - lo)
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let i = ((self.f() * items.len() as f32) as usize).min(items.len() - 1);
        &items[i]
    }
    fn yaw(&mut self) -> Quat {
        Quat::from_rotation_y(self.f() * std::f32::consts::TAU)
    }
    /// A point in a disc of radius `r` around `c`.
    fn around(&mut self, c: Vec2, r: f32) -> Vec2 {
        c + Vec2::from_angle(self.f() * std::f32::consts::TAU) * r * self.f().sqrt()
    }
}

const TREES: &[&str] = &[
    "CommonTree_1.gltf",
    "CommonTree_2.gltf",
    "CommonTree_4.gltf",
    "Pine_1.gltf",
    "Pine_3.gltf",
];
const DEAD_TREES: &[&str] = &["DeadTree_1.gltf", "DeadTree_3.gltf"];
const ROCKS: &[&str] = &[
    "Rock_Medium_1.gltf",
    "Rock_Medium_2.gltf",
    "Rock_Medium_3.gltf",
];
const BUSHES: &[&str] = &["Bush_Common.gltf", "Bush_Common_Flowers.gltf"];
const UNDERGROWTH: &[&str] = &[
    "Fern_1.gltf",
    "Grass_Common_Tall.gltf",
    "Grass_Wispy_Tall.gltf",
    "Plant_1.gltf",
    "Plant_7.gltf",
    "Clover_1.gltf",
];
const FLORA: &[&str] = &[
    "Flower_3_Single.gltf",
    "Flower_3_Group.gltf",
    "Flower_4_Single.gltf",
    "Clover_1.gltf",
    "Grass_Common_Tall.gltf",
    "Grass_Wispy_Tall.gltf",
    "Mushroom_Common.gltf",
    "Pebble_Round_1.gltf",
    "Pebble_Round_2.gltf",
    "Pebble_Square_2.gltf",
];

/// What a cluster is.
#[derive(Clone, Copy)]
enum Cluster {
    Grove,
    Outcrop,
    Thicket,
    Log,
    RuinWall,
    Shed,
    Hay,
    Fence,
    Crates,
}

const CLUSTER_MIX: &[(Cluster, f32)] = &[
    (Cluster::Grove, 6.0),
    (Cluster::Outcrop, 3.0),
    (Cluster::Thicket, 4.0),
    (Cluster::Log, 2.0),
    (Cluster::RuinWall, 1.6),
    (Cluster::Shed, 0.8),
    (Cluster::Hay, 1.5),
    (Cluster::Fence, 1.2),
    (Cluster::Crates, 1.2),
];

/// Shared meshes and materials for the code-built props.
struct Kit {
    cube: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    stone: Handle<StandardMaterial>,
    mossy: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    plank: Handle<StandardMaterial>,
    roof: Handle<StandardMaterial>,
    hay: Handle<StandardMaterial>,
    bark: Handle<StandardMaterial>,
    crate_wood: Handle<StandardMaterial>,
}

/// Spawns props with shared handles.
struct Dresser<'w, 's, 'a> {
    commands: Commands<'w, 's>,
    assets: &'a AssetServer,
    kit: Kit,
}

/// Some kit models are authored far bigger than the meadow (the tall grasses are
/// ~1.8 m clumps), which made knee-high grass tower over people and the camera.
/// Bring them down to ground-cover size.
fn model_size_fix(file: &str) -> f32 {
    match file {
        "Grass_Common_Tall.gltf" | "Grass_Wispy_Tall.gltf" => 0.35,
        "Plant_1.gltf" => 0.6,
        _ => 1.0,
    }
}

impl Dresser<'_, '_, '_> {
    fn model(&mut self, file: &str, at: Vec2, yaw: Quat, scale: f32, small: bool) -> Entity {
        let scale = scale * model_size_fix(file);
        let mut e = self.commands.spawn((
            WorldAssetRoot(
                self.assets
                    .load(GltfAssetLabel::Scene(0).from_asset(format!("models/nature/{file}"))),
            ),
            Transform::from_xyz(at.x, 0.0, at.y)
                .with_rotation(yaw)
                .with_scale(Vec3::splat(scale)),
            crate::ambience::Sway::default(),
            DespawnOnExit(AppState::InGame),
        ));
        if small {
            e.insert(SmallProp);
        }
        e.id()
    }

    /// A static collider with no visuals (for model-based cover).
    fn blocker(&mut self, transform: Transform, collider: Collider) {
        self.commands.spawn((
            transform,
            RigidBody::Static,
            collider,
            DespawnOnExit(AppState::InGame),
        ));
    }

    /// A box of `size` centred at `centre`, optionally solid.
    fn block(
        &mut self,
        centre: Vec3,
        size: Vec3,
        rot: Quat,
        mat: &Handle<StandardMaterial>,
        solid: bool,
    ) {
        let mut e = self.commands.spawn((
            Mesh3d(self.kit.cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(centre)
                .with_rotation(rot)
                .with_scale(size),
            DespawnOnExit(AppState::InGame),
        ));
        if solid {
            // The collider is in local (unit-cube) space; the scale stretches it.
            e.insert((RigidBody::Static, Collider::cuboid(1.0, 1.0, 1.0)));
        }
    }

    /// A cylinder of `radius`/`length` along local Y, rotated by `rot`.
    fn cylinder(
        &mut self,
        centre: Vec3,
        radius: f32,
        length: f32,
        rot: Quat,
        mat: &Handle<StandardMaterial>,
        solid: bool,
    ) {
        let mut e = self.commands.spawn((
            Mesh3d(self.kit.cylinder.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(centre)
                .with_rotation(rot)
                .with_scale(Vec3::new(radius * 2.0, length, radius * 2.0)),
            DespawnOnExit(AppState::InGame),
        ));
        if solid {
            e.insert((RigidBody::Static, Collider::cylinder(0.5, 1.0)));
        }
    }

    fn tree(&mut self, rng: &mut Rng, at: Vec2, dead: bool) {
        let (file, scale) = if dead {
            (*rng.pick(DEAD_TREES), rng.range(0.35, 0.6))
        } else {
            (*rng.pick(TREES), rng.range(0.55, 1.0))
        };
        self.model(file, at, rng.yaw(), scale, false);
        self.blocker(
            Transform::from_xyz(at.x, 1.5, at.y),
            Collider::cylinder(0.35 * scale.max(0.6), 3.0),
        );
    }

    fn rock(&mut self, rng: &mut Rng, at: Vec2, scale: f32) {
        let yaw = rng.yaw();
        self.model(rng.pick(ROCKS), at, yaw, scale, false);
        // The medium rocks are roughly 3 × 2 × 2.5 m at scale 1.
        self.blocker(
            Transform::from_xyz(at.x, 0.9 * scale, at.y).with_rotation(yaw),
            Collider::cuboid(2.6 * scale, 1.8 * scale, 2.2 * scale),
        );
    }

    fn undergrowth(&mut self, rng: &mut Rng, centre: Vec2, r: f32, n: u32) {
        for _ in 0..n {
            let p = rng.around(centre, r);
            let file = *rng.pick(UNDERGROWTH);
            // The fern model is ~9 m across at scale 1.
            let s = if file == "Fern_1.gltf" {
                rng.range(0.15, 0.25)
            } else {
                rng.range(0.8, 1.4)
            };
            self.model(file, p, rng.yaw(), s, true);
        }
    }

    fn cluster(&mut self, rng: &mut Rng, kind: Cluster, c: Vec2) {
        let k = &self.kit;
        let (stone, mossy, wood, plank, roof, hay, bark, crate_wood) = (
            k.stone.clone(),
            k.mossy.clone(),
            k.wood.clone(),
            k.plank.clone(),
            k.roof.clone(),
            k.hay.clone(),
            k.bark.clone(),
            k.crate_wood.clone(),
        );
        let yaw_a = rng.f() * std::f32::consts::TAU;
        let yaw = Quat::from_rotation_y(yaw_a);
        let local = |x: f32, z: f32| c + Vec2::from_angle(-yaw_a).rotate(Vec2::new(x, z));
        let v3 = |p: Vec2, y: f32| Vec3::new(p.x, y, p.y);
        match kind {
            Cluster::Grove => {
                let trees = 3 + (rng.f() * 4.0) as u32;
                let dead_grove = rng.f() < 0.2;
                for _ in 0..trees {
                    let p = rng.around(c, 3.2);
                    if is_clear(p, 0.5) {
                        let dead = dead_grove && rng.f() < 0.7;
                        self.tree(rng, p, dead);
                    }
                }
                for _ in 0..2 {
                    let p = rng.around(c, 3.0);
                    let s = rng.range(0.8, 1.3);
                    self.model(rng.pick(BUSHES), p, rng.yaw(), s, true);
                }
                self.undergrowth(rng, c, 3.5, 5);
            }
            Cluster::Outcrop => {
                let big = rng.range(0.7, 1.05);
                self.rock(rng, c, big);
                for _ in 0..(1 + (rng.f() * 3.0) as u32) {
                    let p = rng.around(c, 2.4);
                    let s = rng.range(0.3, 0.6);
                    if p.distance(c) > 1.2 {
                        self.rock(rng, p, s);
                    }
                }
                for _ in 0..3 {
                    let p = rng.around(c, 3.0);
                    let s = rng.range(0.6, 1.4);
                    self.model(rng.pick(&FLORA[7..]), p, rng.yaw(), s, true);
                }
                self.undergrowth(rng, c, 2.8, 3);
            }
            Cluster::Thicket => {
                // A dense hedge of bushes, scaled up so a crouching friend fits behind.
                let n = 6 + (rng.f() * 4.0) as u32;
                for i in 0..n {
                    let t = i as f32 / n as f32 - 0.5;
                    let p = local(t * 5.0, rng.range(-0.8, 0.8));
                    let s = rng.range(1.1, 1.7);
                    self.model(rng.pick(BUSHES), p, rng.yaw(), s, false);
                }
                self.undergrowth(rng, c, 3.0, 4);
            }
            Cluster::Log => {
                let len = rng.range(3.0, 4.5);
                let r = rng.range(0.3, 0.42);
                let lie = yaw * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
                self.cylinder(v3(c, r), r, len, lie, &bark, true);
                // A stump where it fell from.
                let stump = local(len / 2.0 + 0.6, 0.0);
                self.cylinder(v3(stump, 0.25), r * 1.2, 0.5, Quat::IDENTITY, &bark, true);
                self.undergrowth(rng, c, 2.5, 4);
            }
            Cluster::RuinWall => {
                // A broken stone wall, sometimes with a corner.
                let segs = 4 + (rng.f() * 3.0) as i32;
                let corner = rng.f() < 0.5;
                for i in 0..segs {
                    let h = rng.range(0.9, 2.3);
                    let mat = if rng.f() < 0.3 { &mossy } else { &stone };
                    let (x, z, rot) = if corner && i > segs / 2 {
                        let j = (i - segs / 2) as f32;
                        (
                            -(segs as f32) * 0.5,
                            j * 1.0,
                            yaw * Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
                        )
                    } else {
                        ((i as f32 - segs as f32 / 2.0) * 1.0, 0.0, yaw)
                    };
                    let p = local(x, z);
                    self.block(v3(p, h / 2.0), Vec3::new(1.0, h, 0.6), rot, mat, true);
                }
                // Rubble.
                for _ in 0..4 {
                    let p = rng.around(c, 2.5);
                    let s = rng.range(0.25, 0.5);
                    let tumble = rng.yaw() * Quat::from_rotation_x(rng.range(-0.4, 0.4));
                    self.block(v3(p, s * 0.4), Vec3::splat(s), tumble, &stone, false);
                }
                self.undergrowth(rng, c, 3.0, 3);
            }
            Cluster::Shed => {
                // A little lean-to shed with an open front you can duck into.
                let (w, d, h) = (3.0, 2.4, 2.2);
                self.block(
                    v3(local(0.0, -d / 2.0), h / 2.0),
                    Vec3::new(w, h, 0.15),
                    yaw,
                    &plank,
                    true,
                );
                for sx in [-1.0, 1.0] {
                    self.block(
                        v3(local(sx * w / 2.0, 0.0), h / 2.0 - 0.2),
                        Vec3::new(0.15, h - 0.4, d),
                        yaw,
                        &plank,
                        true,
                    );
                }
                // Half-wall at the front with a doorway gap.
                self.block(
                    v3(local(-w / 4.0 - 0.2, d / 2.0), 0.6),
                    Vec3::new(w / 2.0 - 0.4, 1.2, 0.15),
                    yaw,
                    &plank,
                    true,
                );
                let tilt = yaw * Quat::from_rotation_x(-0.25);
                self.block(
                    v3(local(0.0, 0.0), h + 0.05),
                    Vec3::new(w + 0.5, 0.12, d + 0.6),
                    tilt,
                    &roof,
                    false,
                );
                for (x, z) in [
                    (-w / 2.0, -d / 2.0),
                    (w / 2.0, -d / 2.0),
                    (-w / 2.0, d / 2.0),
                    (w / 2.0, d / 2.0),
                ] {
                    self.block(
                        v3(local(x, z), h / 2.0),
                        Vec3::new(0.2, h, 0.2),
                        yaw,
                        &wood,
                        false,
                    );
                }
                self.undergrowth(rng, c, 3.2, 4);
            }
            Cluster::Hay => {
                // Round bales, a couple stacked.
                let n = 2 + (rng.f() * 3.0) as i32;
                for i in 0..n {
                    let p = local((i as f32 - n as f32 / 2.0) * 1.3, rng.range(-0.3, 0.3));
                    let roll = yaw * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                    self.cylinder(v3(p, 0.6), 0.6, 1.1, roll, &hay, true);
                }
                if n >= 3 {
                    let p = local(-0.65, 0.0);
                    let roll = yaw * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                    self.cylinder(v3(p, 1.75), 0.6, 1.1, roll, &hay, true);
                }
            }
            Cluster::Fence => {
                // A wonky stretch of fence with a missing rail or two.
                let posts = 5 + (rng.f() * 3.0) as i32;
                for i in 0..posts {
                    let x = (i as f32 - posts as f32 / 2.0) * 1.4;
                    let lean = yaw * Quat::from_rotation_z(rng.range(-0.1, 0.1));
                    self.block(
                        v3(local(x, 0.0), 0.55),
                        Vec3::new(0.14, 1.1, 0.14),
                        lean,
                        &wood,
                        false,
                    );
                    if i + 1 < posts {
                        for y in [0.45, 0.9] {
                            if rng.f() < 0.85 {
                                self.block(
                                    v3(local(x + 0.7, 0.0), y),
                                    Vec3::new(1.4, 0.1, 0.05),
                                    yaw,
                                    &wood,
                                    false,
                                );
                            }
                        }
                    }
                }
                let len = posts as f32 * 1.4;
                self.blocker(
                    Transform::from_xyz(c.x, 0.55, c.y).with_rotation(yaw),
                    Collider::cuboid(len, 1.1, 0.2),
                );
                self.undergrowth(rng, c, 3.0, 4);
            }
            Cluster::Crates => {
                let n = 3 + (rng.f() * 3.0) as i32;
                for i in 0..n {
                    let s = rng.range(0.7, 1.0);
                    let (x, y) = if i >= 3 {
                        ((i - 3) as f32 * 0.9 - 0.4, 0.85 + s / 2.0)
                    } else {
                        (i as f32 * 0.95 - 0.95, s / 2.0)
                    };
                    let p = local(x, rng.range(-0.15, 0.15));
                    let rot = yaw * Quat::from_rotation_y(rng.range(-0.3, 0.3));
                    self.block(v3(p, y), Vec3::splat(s), rot, &crate_wood, true);
                }
            }
        }
    }
}

fn dress_floor(
    commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let kit = Kit {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        cylinder: meshes.add(Cylinder::new(0.5, 1.0)),
        stone: art::soft(&mut materials, Color::srgb(0.58, 0.56, 0.54)),
        mossy: art::soft(&mut materials, Color::srgb(0.45, 0.52, 0.4)),
        wood: art::soft(&mut materials, Color::srgb(0.45, 0.32, 0.2)),
        plank: art::soft(&mut materials, Color::srgb(0.6, 0.44, 0.3)),
        roof: art::soft(&mut materials, Color::srgb(0.55, 0.25, 0.2)),
        hay: art::soft(&mut materials, Color::srgb(0.88, 0.74, 0.38)),
        bark: art::soft(&mut materials, Color::srgb(0.4, 0.3, 0.22)),
        crate_wood: art::soft(&mut materials, Color::srgb(0.72, 0.55, 0.34)),
    };
    let mut d = Dresser {
        commands,
        assets: &assets,
        kit,
    };
    let mut rng = Rng(0x5EED_5EED);
    let total: f32 = CLUSTER_MIX.iter().map(|(_, w)| w).sum();

    // Cluster centres: rejection-sampled over the play area, spaced apart and
    // clear of every landmark.
    let mut centres: Vec<Vec2> = Vec::new();
    for _ in 0..CLUSTER_COUNT * 30 {
        if centres.len() >= CLUSTER_COUNT {
            break;
        }
        let p = Vec2::new(
            rng.range(-PLAY_HALF, PLAY_HALF),
            rng.range(-PLAY_HALF, PLAY_HALF),
        );
        if !is_clear(p, 2.5) || centres.iter().any(|c| c.distance(p) < CLUSTER_SPACING) {
            continue;
        }
        centres.push(p);
    }
    for c in centres {
        let mut pick = rng.f() * total;
        let kind = CLUSTER_MIX
            .iter()
            .find(|(_, w)| {
                pick -= w;
                pick <= 0.0
            })
            .map_or(Cluster::Grove, |(k, _)| *k);
        // Tall trees between the default camera and the centre
        // would hide everything, so that corridor only gets low cover.
        let kind = match kind {
            Cluster::Grove if in_camera_corridor(c) => Cluster::Thicket,
            k => k,
        };
        d.cluster(&mut rng, kind, c);
    }

    // Loose flowers and pebbles in the open bits, so the meadow isn't bald.
    for _ in 0..LOOSE_FLORA {
        let p = Vec2::new(
            rng.range(-PLAY_HALF, PLAY_HALF),
            rng.range(-PLAY_HALF, PLAY_HALF),
        );
        let file = *rng.pick(FLORA);
        let s = rng.range(0.8, 1.5);
        let yaw = rng.yaw();
        if is_clear(p, 0.0) {
            d.model(file, p, yaw, s, true);
        }
    }
}

/// Where the commune keeps its campfire, beside the ruins and Dave.
pub(crate) const BONFIRE_AT: Vec3 = Vec3::new(PARLIAMENT_AT.x + 3.5, 0.0, PARLIAMENT_AT.y + 8.5);

/// The bonfire glbs carry a ×199 node scale (centimetre export), which made the
/// campfire a 200 m wooden dome over the whole map. This brings it back to a
/// ~1.6 m campfire.
const BONFIRE_SCALE: f32 = 1.3 / 180.0;

#[derive(Component)]
struct BonfireFlame;

fn light_bonfire(mut commands: Commands, assets: Res<AssetServer>) {
    let scene = |file: &str| {
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/props/{file}")))
    };
    commands.spawn((
        WorldAssetRoot(scene("bonfire.glb")),
        Transform::from_translation(BONFIRE_AT).with_scale(Vec3::splat(BONFIRE_SCALE)),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        WorldAssetRoot(scene("bonfire_fire.glb")),
        BonfireFlame,
        Transform::from_translation(BONFIRE_AT + Vec3::Y * 0.1)
            .with_scale(Vec3::splat(BONFIRE_SCALE)),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.55, 0.2),
            intensity: 1800.0,
            range: 14.0,
            ..default()
        },
        BonfireFlame,
        Transform::from_translation(BONFIRE_AT + Vec3::Y * 1.2),
        DespawnOnExit(AppState::InGame),
    ));
}

/// The fire breathes: gentle flicker on the campfire light.
fn flicker_bonfire(time: Res<Time>, mut flames: Query<&mut PointLight, With<BonfireFlame>>) {
    let t = time.elapsed_secs();
    for mut light in &mut flames {
        let flicker = (t * 7.0).sin() * 200.0 + (t * 13.7).sin() * 150.0;
        light.intensity = 1800.0 + flicker;
    }
}

fn play_music(mut commands: Commands, sfx: Option<Res<Sfx>>) {
    let Some(sfx) = sfx else { return };
    commands.spawn((
        AudioPlayer(sfx.music.clone()),
        PlaybackSettings::LOOP.with_volume(Volume::Linear(0.28)),
        DespawnOnExit(AppState::InGame),
    ));
}
