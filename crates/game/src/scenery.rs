//! Dresses the shared floor: clustered groves, rock outcrops, bush thickets,
//! fallen logs, ruins, sheds, hay bales, fences and crate stacks,
//! so there is plenty to hide behind (issue #60). Also lights a campfire
//! beside the burnt parliament and loops a little tune.
//!
//! Nothing here is replicated: every peer places the same props the same way
//! from a fixed seed. Models come from `assets/models/nature` (see CREDITS.md)
//! and the modeler-made `assets/models/scenery`; missing files simply render
//! nothing, but every collider still exists, so the cover layout never
//! depends on LFS.
//! Large cover (trunks, rocks, walls, sheds, bales, crates) gets a static
//! avian3d collider so it actually blocks.

use avian3d::prelude::*;
use bevy::gltf::GltfAssetLabel;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use crate::AppState;
use crate::art;
use crate::features::dave_commune::parliament_at;
use crate::features::duel::{ARENA_HALF, arena_center};
use crate::features::landlord::landlord_at;
use crate::features::totems::vault_at;
pub use friendslop_base::scenery::bonfire_at;

pub fn plugin(app: &mut App) {
    app.init_resource::<KitCache>()
        .add_systems(OnEnter(AppState::InGame), (dress_floor, light_bonfire))
        .add_systems(Update, flicker_bonfire)
        .add_observer(no_shadows_on_small_props);
}

/// Props stay inside this half-extent (the floor is 50×50, and the outer
/// edge may get hills, so keep a margin).
const PLAY_HALF: f32 = 22.5;
/// How many prop clusters to place.
const CLUSTER_COUNT: usize = 18;
/// Minimum distance between cluster centres.
const CLUSTER_SPACING: f32 = 6.5;
/// Scattered loose flora between clusters.
const LOOSE_FLORA: u32 = 50;

/// Small scenery whose meshes shouldn't cast shadows (cheap, and tiny shadows
/// just look noisy) and stop drawing in the distance (#151).
#[derive(Component)]
pub(crate) struct SmallProp;

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
            commands
                .entity(child)
                .insert((NotShadowCaster, crate::graphics::DrawRange::SMALL_PROP));
        }
    }
}

/// Spots that must stay open: (centre, radius).
fn keep_clear() -> Vec<(Vec2, f32)> {
    vec![
        // Centre spawn / overview focus and the mushroom patch's inner ring.
        (Vec2::ZERO, 4.5),
        (bonfire_at().xz(), 3.5),
        (landlord_at().xz(), 3.0),
        (vault_at(), 4.5),
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
        // Marmalade's wheel (#173).
        (crate::features::gamble::gamble_at(), 5.0),
    ]
}

/// Is `p` (with `margin` of footprint) clear of every landmark?
fn is_clear(p: Vec2, margin: f32) -> bool {
    let in_rect = |c: Vec2, half: f32| (p.x - c.x).abs() < half && (p.y - c.y).abs() < half;
    // The arena keeps a wide margin so its walls sit in open grass.
    if in_rect(arena_center().xz(), ARENA_HALF + 2.5 + margin) {
        return false;
    }
    // The parliament ruin is bigger than it looks (fallen tower and all).
    if in_rect(parliament_at(), 8.0 + margin) {
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
    // Nor in the players' spawn rows (fps.rs) or the first-person view from
    // them, or the first thing you see is a fern leaf.
    if p.x.abs() < 6.0 + margin && (4.0 - margin..17.0 + margin).contains(&p.y) {
        return false;
    }
    // Keep the pond and dirt paths (terrain.rs) open.
    let around = [
        Vec2::ZERO,
        Vec2::X * margin,
        -Vec2::X * margin,
        Vec2::Y * margin,
        -Vec2::Y * margin,
    ];
    if around.iter().any(|o| {
        let q = p + *o;
        crate::terrain::is_pond(q.x, q.y) || crate::terrain::is_path(q.x, q.y)
    }) {
        return false;
    }
    // The ways down into the tunnels (#127).
    if crate::features::underground::covers(p, margin) {
        return false;
    }
    if friendslop_world::footprints::claimed(p, margin) {
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
pub(crate) struct Rng(pub(crate) u32);

impl Rng {
    pub(crate) fn f(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    pub(crate) fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.f() * (hi - lo)
    }
    pub(crate) fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let i = ((self.f() * items.len() as f32) as usize).min(items.len() - 1);
        &items[i]
    }
    pub(crate) fn yaw(&mut self) -> Quat {
        Quat::from_rotation_y(self.f() * std::f32::consts::TAU)
    }
    /// A point in a disc of radius `r` around `c`.
    pub(crate) fn around(&mut self, c: Vec2, r: f32) -> Vec2 {
        c + Vec2::from_angle(self.f() * std::f32::consts::TAU) * r * self.f().sqrt()
    }
}

/// Grown with the modeler's L-systems (sources in `crates/game/.modeler/`), the
/// size of the kit trees they replaced.
pub(crate) const TREES: &[&str] = &[
    "tree_1.gltf",
    "tree_2.gltf",
    "tree_3.gltf",
    "pine_1.gltf",
    "pine_2.gltf",
];
const DEAD_TREES: &[&str] = &["dead_tree_1.gltf", "dead_tree_2.gltf"];
/// Made with the modeler (sources in `crates/game/.modeler/`), same size as the
/// kit rocks they replaced.
pub(crate) const ROCKS: &[&str] = &["rock_1.gltf", "rock_2.gltf", "rock_3.gltf"];
pub(crate) const BUSHES: &[&str] = &["bush_1.gltf", "bush_2.gltf"];
const UNDERGROWTH: &[&str] = &[
    "fern.gltf",
    "grass_1.gltf",
    "grass_2.gltf",
    "plant_1.gltf",
    "plant_7.gltf",
    "clover.gltf",
];
pub(crate) const FLORA: &[&str] = &[
    "flower_1.gltf",
    "flower_2.gltf",
    "flower_3.gltf",
    "clover.gltf",
    "grass_1.gltf",
    "grass_2.gltf",
    "mushrooms.gltf",
    "pebble_1.gltf",
    "pebble_2.gltf",
    "pebble_3.gltf",
];

/// What a cluster is.
#[derive(Clone, Copy)]
pub(crate) enum Cluster {
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
#[derive(Clone)]
pub(crate) struct Kit {
    pub(crate) cube: Handle<Mesh>,
    pub(crate) mossy: Handle<StandardMaterial>,
}

/// Spawns props with shared handles.
pub(crate) struct Dresser<'w, 's, 'a> {
    pub(crate) commands: Commands<'w, 's>,
    pub(crate) assets: &'a AssetServer,
    pub(crate) kit: Kit,
}

/// Stone models: nothing to sway.
pub(crate) fn is_rigid(file: &str) -> bool {
    file.starts_with("rock_") || file.starts_with("pebble_")
}

/// The scene root for one modeler-made `assets/models/scenery` model (logs,
/// ruins, sheds, bales, crates and the districts' set pieces; sources in
/// `crates/game/.modeler/`, prefabs named `zn-*`).
pub(crate) fn scenery_model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/scenery/{file}.gltf"))),
    )
}

/// The fallen log model is 4 m long with a 0.4 m radius (lying along X).
const LOG_LEN: f32 = 4.0;
const LOG_R: f32 = 0.4;
/// The stump model is 0.5 m in radius and 0.5 m tall, base at its origin.
const STUMP_R: f32 = 0.5;
const STUMP_H: f32 = 0.5;

/// The scene root for one `assets/models/nature` model (shared with the menu
/// backdrop).
pub(crate) fn nature_model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/nature/{file}"))),
    )
}

impl Dresser<'_, '_, '_> {
    pub(crate) fn model(
        &mut self,
        file: &str,
        at: Vec2,
        yaw: Quat,
        scale: f32,
        small: bool,
    ) -> Entity {
        let mut e = self.commands.spawn((
            nature_model(self.assets, file),
            Transform::from_xyz(at.x, ground(at), at.y)
                .with_rotation(yaw)
                .with_scale(Vec3::splat(scale)),
            DespawnOnExit(AppState::InGame),
        ));
        let tree = TREES.contains(&file) || DEAD_TREES.contains(&file);
        // Rocks and pebbles don't bend in the wind (#151: every swaying prop
        // re-propagates its whole glTF hierarchy). Trees are rigged and bend
        // their own branches instead (`modeler_bevy`, steered in lod.rs).
        if !is_rigid(file) && !tree {
            e.insert(crate::ambience::Sway::default());
        }
        // Trees get a far mesh and draw to the fog instead of vanishing like
        // small props (#227); `small` ones just cast no shadow.
        if tree {
            e.insert(crate::lod::TreeLod { shadows: !small });
        } else if small {
            e.insert(SmallProp);
        }
        e.id()
    }

    /// A `models/scenery` model at `transform`, lifted onto the ground under
    /// it. Purely visual: colliders are spawned separately, so the cover
    /// never depends on the file.
    pub(crate) fn prop(&mut self, file: &str, mut transform: Transform, small: bool) -> Entity {
        transform.translation.y += ground(transform.translation.xz());
        let mut e = self.commands.spawn((
            scenery_model(self.assets, file),
            transform,
            DespawnOnExit(AppState::InGame),
        ));
        if small {
            e.insert(SmallProp);
        }
        e.id()
    }

    /// The collider [`Self::block`] would give a solid box, with no mesh (the
    /// model draws it).
    pub(crate) fn solid_box(&mut self, centre: Vec3, size: Vec3, rot: Quat) {
        let centre = centre + Vec3::Y * ground(centre.xz());
        self.commands.spawn((
            Transform::from_translation(centre)
                .with_rotation(rot)
                .with_scale(size),
            RigidBody::Static,
            Collider::cuboid(1.0, 1.0, 1.0),
            DespawnOnExit(AppState::InGame),
        ));
    }

    /// A solid cylinder of `radius`/`length` along local Y, rotated by
    /// `rot`, with no mesh (the model draws it).
    pub(crate) fn solid_cylinder(&mut self, centre: Vec3, radius: f32, length: f32, rot: Quat) {
        let centre = centre + Vec3::Y * ground(centre.xz());
        self.commands.spawn((
            Transform::from_translation(centre)
                .with_rotation(rot)
                .with_scale(Vec3::new(radius * 2.0, length, radius * 2.0)),
            RigidBody::Static,
            Collider::cylinder(0.5, 1.0),
            DespawnOnExit(AppState::InGame),
        ));
    }

    /// A fallen log of `len` × `r` lying along `yaw`'s X, centred at `at`
    /// (its axis `r` above the ground), solid.
    pub(crate) fn log(&mut self, at: Vec2, yaw: Quat, len: f32, r: f32) {
        let centre = Vec3::new(at.x, r, at.y);
        let lie = yaw * Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        self.solid_cylinder(centre, r, len, lie);
        self.prop(
            "log",
            Transform::from_translation(centre)
                .with_rotation(yaw)
                .with_scale(Vec3::new(len / LOG_LEN, r / LOG_R, r / LOG_R)),
            false,
        );
    }

    /// A solid tree stump of `radius` × `height` standing at `at`.
    pub(crate) fn stump(&mut self, at: Vec2, radius: f32, height: f32) {
        self.solid_cylinder(
            Vec3::new(at.x, height / 2.0, at.y),
            radius,
            height,
            Quat::IDENTITY,
        );
        let w = radius / STUMP_R;
        self.prop(
            "stump",
            Transform::from_xyz(at.x, 0.0, at.y).with_scale(Vec3::new(w, height / STUMP_H, w)),
            false,
        );
    }

    /// A static collider with no visuals (for model-based cover).
    pub(crate) fn blocker(&mut self, mut transform: Transform, collider: Collider) {
        transform.translation.y += ground(transform.translation.xz());
        self.commands.spawn((
            transform,
            RigidBody::Static,
            collider,
            DespawnOnExit(AppState::InGame),
        ));
    }

    /// A box of `size` centred at `centre`, optionally solid.
    pub(crate) fn block(
        &mut self,
        centre: Vec3,
        size: Vec3,
        rot: Quat,
        mat: &Handle<StandardMaterial>,
        solid: bool,
    ) {
        let centre = centre + Vec3::Y * ground(centre.xz());
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

    pub(crate) fn tree(&mut self, rng: &mut Rng, at: Vec2, dead: bool) {
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

    pub(crate) fn rock(&mut self, rng: &mut Rng, at: Vec2, scale: f32) {
        let yaw = rng.yaw();
        self.model(rng.pick(ROCKS), at, yaw, scale, false);
        // The medium rocks are roughly 3 × 2 × 2.5 m at scale 1.
        self.blocker(
            Transform::from_xyz(at.x, 0.9 * scale, at.y).with_rotation(yaw),
            Collider::cuboid(2.6 * scale, 1.8 * scale, 2.2 * scale),
        );
    }

    pub(crate) fn undergrowth(&mut self, rng: &mut Rng, centre: Vec2, r: f32, n: u32) {
        for _ in 0..n {
            let p = rng.around(centre, r);
            let file = *rng.pick(UNDERGROWTH);
            // Ferns spread wider than the rest (~1.8 m across at scale 1).
            let s = if file == "fern.gltf" {
                rng.range(0.75, 1.25)
            } else {
                rng.range(0.8, 1.4)
            };
            let yaw = rng.yaw();
            // Draw from the rng either way so the rest of the layout is stable.
            if is_clear(p, 0.3) {
                self.model(file, p, yaw, s, true);
            }
        }
    }

    pub(crate) fn cluster(&mut self, rng: &mut Rng, kind: Cluster, c: Vec2) {
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
                    let file = *rng.pick(BUSHES);
                    let yaw = rng.yaw();
                    if is_clear(p, 0.5) {
                        self.model(file, p, yaw, s, true);
                    }
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
                    let file = *rng.pick(&FLORA[7..]);
                    let yaw = rng.yaw();
                    if is_clear(p, 0.3) {
                        self.model(file, p, yaw, s, true);
                    }
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
                self.log(c, yaw, len, r);
                // A stump where it fell from.
                let stump = local(len / 2.0 + 0.6, 0.0);
                self.stump(stump, r * 1.2, 0.5);
                self.undergrowth(rng, c, 2.5, 4);
            }
            Cluster::RuinWall => {
                // A broken stone wall, sometimes with a corner.
                let segs = 4 + (rng.f() * 3.0) as i32;
                let corner = rng.f() < 0.5;
                for i in 0..segs {
                    let h = rng.range(0.9, 2.3);
                    // The model is a 1 m tall segment, stretched to `h`.
                    let file = if rng.f() < 0.3 {
                        "ruin_block_mossy"
                    } else {
                        "ruin_block"
                    };
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
                    self.solid_box(v3(p, h / 2.0), Vec3::new(1.0, h, 0.6), rot);
                    self.prop(
                        file,
                        Transform::from_translation(v3(p, h / 2.0))
                            .with_rotation(rot)
                            .with_scale(Vec3::new(1.0, h, 1.0)),
                        false,
                    );
                }
                // Rubble.
                for _ in 0..4 {
                    let p = rng.around(c, 2.5);
                    let s = rng.range(0.25, 0.5);
                    let tumble = rng.yaw() * Quat::from_rotation_x(rng.range(-0.4, 0.4));
                    self.prop(
                        "rubble",
                        Transform::from_translation(v3(p, s * 0.4))
                            .with_rotation(tumble)
                            .with_scale(Vec3::splat(s)),
                        true,
                    );
                }
                self.undergrowth(rng, c, 3.0, 3);
            }
            Cluster::Shed => {
                // A little lean-to shed with an open front you can duck into
                // (modeler model: walls, posts and roof). The walls block.
                let (w, d, h) = (3.0, 2.4, 2.2);
                self.prop(
                    "shed",
                    Transform::from_translation(v3(c, 0.0)).with_rotation(yaw),
                    false,
                );
                self.solid_box(
                    v3(local(0.0, -d / 2.0), h / 2.0),
                    Vec3::new(w, h, 0.15),
                    yaw,
                );
                for sx in [-1.0, 1.0] {
                    self.solid_box(
                        v3(local(sx * w / 2.0, 0.0), h / 2.0 - 0.2),
                        Vec3::new(0.15, h - 0.4, d),
                        yaw,
                    );
                }
                // Half-wall at the front with a doorway gap.
                self.solid_box(
                    v3(local(-w / 4.0 - 0.2, d / 2.0), 0.6),
                    Vec3::new(w / 2.0 - 0.4, 1.2, 0.15),
                    yaw,
                );
                self.undergrowth(rng, c, 3.2, 4);
            }
            Cluster::Hay => {
                // Round bales, a couple stacked.
                let n = 2 + (rng.f() * 3.0) as i32;
                // The bale model lies along its Z, like the rolled cylinder.
                let roll = yaw * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2);
                let bale = |d: &mut Self, centre: Vec3| {
                    d.solid_cylinder(centre, 0.6, 1.1, roll);
                    d.prop(
                        "hay_bale",
                        Transform::from_translation(centre).with_rotation(yaw),
                        false,
                    );
                };
                for i in 0..n {
                    let p = local((i as f32 - n as f32 / 2.0) * 1.3, rng.range(-0.3, 0.3));
                    bale(self, v3(p, 0.6));
                }
                if n >= 3 {
                    bale(self, v3(local(-0.65, 0.0), 1.75));
                }
            }
            Cluster::Fence => {
                // A wonky stretch of fence with a missing rail or two. The post
                // (0.14 × 1.1 × 0.14) and rail (1.4 × 0.1 × 0.05) are modeler
                // models centred like the boxes they replaced (`fn-fence-*`).
                let posts = 5 + (rng.f() * 3.0) as i32;
                for i in 0..posts {
                    let x = (i as f32 - posts as f32 / 2.0) * 1.4;
                    let lean = yaw * Quat::from_rotation_z(rng.range(-0.1, 0.1));
                    self.prop(
                        "fence_post",
                        Transform::from_translation(v3(local(x, 0.0), 0.55)).with_rotation(lean),
                        false,
                    );
                    if i + 1 < posts {
                        for y in [0.45, 0.9] {
                            if rng.f() < 0.85 {
                                self.prop(
                                    "fence_rail",
                                    Transform::from_translation(v3(local(x + 0.7, 0.0), y))
                                        .with_rotation(yaw),
                                    false,
                                );
                            }
                        }
                    }
                }
                // One blocker per rail span, so the collider matches the visible
                // fence and follows the terrain like the posts and rails do.
                for i in 0..posts - 1 {
                    let mid = local((i as f32 - posts as f32 / 2.0) * 1.4 + 0.7, 0.0);
                    self.blocker(
                        Transform::from_xyz(mid.x, 0.55, mid.y).with_rotation(yaw),
                        Collider::cuboid(1.54, 1.1, 0.2),
                    );
                }
                self.undergrowth(rng, c, 3.0, 4);
            }
            Cluster::Crates => {
                let n = 3 + (rng.f() * 3.0) as i32;
                // (x, z, size) of the bottom row, for the top row to sit on.
                let mut bottom: Vec<(f32, f32, f32)> = Vec::new();
                for i in 0..n {
                    let s = rng.range(0.7, 1.0);
                    let jitter = rng.range(-0.15, 0.15);
                    let (x, z, y) = if i >= 3 {
                        // Stack on a real crate: its position, its top.
                        let (bx, bz, bs) = bottom[(i - 3) as usize];
                        (bx, bz, bs + s / 2.0)
                    } else {
                        let x = i as f32 * 0.95 - 0.95;
                        bottom.push((x, jitter, s));
                        (x, jitter, s / 2.0)
                    };
                    let p = local(x, z);
                    let rot = yaw * Quat::from_rotation_y(rng.range(-0.3, 0.3));
                    // The crate model is a 1 m cube, scaled like the block was.
                    self.solid_box(v3(p, y), Vec3::splat(s), rot);
                    self.prop(
                        "crate",
                        Transform::from_translation(v3(p, y))
                            .with_rotation(rot)
                            .with_scale(Vec3::splat(s)),
                        false,
                    );
                }
            }
        }
    }
}

/// Ground height under a point (props sit on the terrain).
pub(crate) fn ground(p: Vec2) -> f32 {
    crate::terrain::height_at(p.x, p.y)
}

/// The one [`Kit`] the commune and the districts both dress with, so their
/// stone and wood draw in the same batches (#151: each used to make its
/// own identical copies).
#[derive(Resource, Default)]
pub(crate) struct KitCache(Option<Kit>);

pub(crate) fn make_kit(
    cache: &mut KitCache,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) -> Kit {
    cache
        .0
        .get_or_insert_with(|| new_kit(meshes, materials))
        .clone()
}

fn new_kit(meshes: &mut Assets<Mesh>, materials: &mut Assets<StandardMaterial>) -> Kit {
    Kit {
        cube: meshes.add(Cuboid::new(1.0, 1.0, 1.0)),
        mossy: art::soft(materials, Color::srgb(0.45, 0.52, 0.4)),
    }
}

fn dress_floor(
    commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cache: ResMut<KitCache>,
) {
    let kit = make_kit(&mut cache, &mut meshes, &mut materials);
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

#[derive(Component)]
struct BonfireFlame;

fn light_bonfire(mut commands: Commands, assets: Res<AssetServer>) {
    spawn_bonfire(&mut commands, &assets, bonfire_at(), AppState::InGame);
}

/// The campfire model, its flame and a flickering light at `at`, despawned on
/// leaving `state` (the game and the menu backdrop share it).
pub(crate) fn spawn_bonfire(
    commands: &mut Commands,
    assets: &AssetServer,
    at: Vec3,
    state: AppState,
) {
    let scene = |file: &str| {
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/props/{file}")))
    };
    commands.spawn((
        // Modeler-made at real size (1.5 m across, sources in `.modeler/`).
        WorldAssetRoot(scene("campfire.gltf")),
        Transform::from_translation(at),
        DespawnOnExit(state),
    ));
    commands.spawn((
        WorldAssetRoot(scene("campfire_fire.gltf")),
        BonfireFlame,
        Transform::from_translation(at + Vec3::Y * 0.05),
        DespawnOnExit(state),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.55, 0.2),
            intensity: 1800.0,
            range: 12.0,
            radius: 0.4,
            shadow_maps_enabled: false,
            ..default()
        },
        BonfireFlame,
        Transform::from_translation(at + Vec3::Y * 1.2),
        DespawnOnExit(state),
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
