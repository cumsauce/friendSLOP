//! Every peer: the nightmare bathroom you see and hear (#161). Tiled floors,
//! a ceiling far too high, buzzing fluorescent tubes, rows of stalls with
//! broken or too-high doors and dirty toilets, some stalls opening into the
//! next, and every cell laid out the same way so you lose track of where you
//! are. Plus the fart monster's body, its clouds and the gassing.

use std::f32::consts::PI;

use bevy::asset::RenderAssetUsages;
use bevy::audio::Volume;
use bevy::image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Affine2;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::super::bathroom::{CEILING, FartMonster, Gassed, Zone, zone};
use super::*;

pub fn plugin(app: &mut App) {
    app.add_observer(on_gassed).add_systems(
        Update,
        (
            dress_fart_monsters,
            animate_fart_monsters,
            load_bath_models,
            tile_floors,
            drift_clouds,
            bathroom_air.after(darkness).in_set(crate::place::PlaceSet),
        )
            .run_if(in_state(AppState::InGame)),
    );
}

/// The fart monster's farts: deep, long and wet.
/// Pure samples, so they can be synthesised off the main thread (#151).
pub(super) const FARTS: usize = 3;

pub(super) fn fart_samples() -> Vec<Vec<f32>> {
    (0..FARTS as u32)
        .map(|i| {
            let tone = crate::features::fart_sound::FartTone {
                pitch: 48.0 + i as f32 * 14.0,
                length: 1.8 + i as f32 * 0.5,
                timbre: 0.85,
            };
            crate::features::fart_sound::fart_samples(&tone, 1.0, 0x0161 + i, sound::RATE)
        })
        .collect()
}

/// Greyscale tiles with dark grout and grime, 4x4 to a texture.
fn tile_pixels(size: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((size * size * 4) as usize);
    let tile = size / 4;
    for y in 0..size {
        for x in 0..size {
            let (tx, ty) = ((x / tile) as i32, (y / tile) as i32);
            let (gx, gy) = (x % tile, y % tile);
            let grout = gx < 2 || gy < 2;
            let grime = crate::art::tiled_noise(
                x as f32 / size as f32 * 8.0,
                y as f32 / size as f32 * 8.0,
                8,
                161,
            );
            let l = if grout {
                0.3 + grime * 0.15
            } else {
                0.86 + crate::art::hash(tx, ty, 162) * 0.1 - grime.powi(3) * 0.35
            };
            let b = (l.clamp(0.0, 1.0) * 255.0) as u8;
            data.extend_from_slice(&[b, b, b, 255]);
        }
    }
    data
}

fn tile_texture(images: &mut Assets<Image>) -> Handle<Image> {
    const SIZE: u32 = 128;
    let mut image = Image::new(
        Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        tile_pixels(SIZE),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    images.add(image)
}

/// Meshes and materials for the bathroom, made once per build. Everything
/// else in it is a modeler model (`assets/models/features/`).
pub(super) struct Kit {
    cube: Handle<Mesh>,
    floor: Handle<Mesh>,
    tiles: Handle<StandardMaterial>,
    pub wall: Handle<StandardMaterial>,
}

pub(super) fn kit(
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: Option<&mut Assets<Image>>,
    cube: &Handle<Mesh>,
) -> Kit {
    let tiles = materials.add(StandardMaterial {
        base_color: Color::srgb(0.78, 0.82, 0.78),
        base_color_texture: images.map(tile_texture),
        perceptual_roughness: 0.35,
        uv_transform: Affine2::from_scale(Vec2::splat(3.0)),
        ..default()
    });
    Kit {
        cube: cube.clone(),
        floor: meshes.add(Cuboid::new(CELL, 0.02, CELL)),
        tiles,
        wall: mat(materials, Color::srgb(0.66, 0.72, 0.66), 0.6),
    }
}

/// The modeler's tile surface (`models/surfaces/bath_tiles.gltf`, source
/// `bt-swatch-tiles`): 4 x 4 tiles to a 1.67 m texture, the same size as
/// the generated one, which stays until (or unless) the swatch loads.
const TILE_SWATCH: &str = "models/surfaces/bath_tiles.gltf";

/// A bathroom floor still wearing the generated tiles.
#[derive(Component)]
struct BathFloor;

/// Swap the generated tile texture for the modeler swatch's once it loads.
fn tile_floors(
    mut commands: Commands,
    mut swatch: Local<Option<Handle<Gltf>>>,
    floors: Query<(Entity, &MeshMaterial3d<StandardMaterial>), With<BathFloor>>,
    assets: Option<Res<AssetServer>>,
    gltfs: Option<Res<Assets<Gltf>>>,
    gltf_materials: Option<Res<Assets<bevy::gltf::GltfMaterial>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    if floors.is_empty() {
        return;
    }
    let (Some(assets), Some(gltfs), Some(gltf_materials), Some(mut materials)) =
        (assets, gltfs, gltf_materials, materials)
    else {
        return;
    };
    let h = swatch
        .get_or_insert_with(|| assets.load(TILE_SWATCH))
        .clone();
    if assets.load_state(&h).is_failed() {
        // No swatch (no LFS data): keep the generated tiles for good.
        for (e, _) in &floors {
            commands.entity(e).remove::<BathFloor>();
        }
        return;
    }
    if !assets.is_loaded_with_dependencies(&h) {
        return;
    }
    let look = (gltfs.get(&h))
        .and_then(|g| g.materials.first())
        .and_then(|m| gltf_materials.get(m));
    for (e, m) in &floors {
        commands.entity(e).remove::<BathFloor>();
        let (Some(look), Some(mut floor)) = (look, materials.get_mut(&m.0)) else {
            continue;
        };
        if let Some(t) = look.base_color_texture.clone() {
            floor.base_color = Color::srgb(0.9, 0.95, 0.9);
            floor.base_color_texture = Some(t);
            floor.metallic_roughness_texture = look.metallic_roughness_texture.clone();
            floor.perceptual_roughness = 1.0;
        }
    }
}

/// One bathroom part: a mesh at a transform, optionally solid.
fn bit(
    commands: &mut Commands,
    mesh: &Handle<Mesh>,
    m: &Handle<StandardMaterial>,
    tf: Transform,
    solid: Option<avian3d::prelude::Collider>,
) {
    let mut e = commands.spawn((
        Mesh3d(mesh.clone()),
        MeshMaterial3d(m.clone()),
        tf,
        InteriorPart,
        DespawnOnExit(AppState::InGame),
    ));
    if let Some(c) = solid {
        e.insert((avian3d::prelude::RigidBody::Static, c));
    }
}

/// A box part (unit cube scaled), optionally solid.
fn slab(
    commands: &mut Commands,
    kit: &Kit,
    m: &Handle<StandardMaterial>,
    tf: Transform,
    solid: bool,
) {
    let c = solid.then(|| avian3d::prelude::Collider::cuboid(1.0, 1.0, 1.0));
    bit(commands, &kit.cube, m, tf, c);
}

fn at(p: Vec3, size: Vec3) -> Transform {
    Transform::from_translation(p).with_scale(size)
}

/// A modeler-made bathroom model (a file in `assets/models/features`),
/// loaded onto its entity by [`load_bath_models`].
#[derive(Component)]
struct BathModel(&'static str);

fn load_bath_models(
    mut commands: Commands,
    new: Query<(Entity, &BathModel), Added<BathModel>>,
    assets: Option<Res<AssetServer>>,
) {
    let Some(assets) = assets else {
        return;
    };
    for (e, m) in &new {
        commands.entity(e).try_insert(WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{}", m.0))),
        ));
    }
}

/// One bathroom model at a transform (no collider of its own).
fn model(commands: &mut Commands, file: &'static str, tf: Transform) {
    commands.spawn((
        BathModel(file),
        tf,
        Visibility::default(),
        InteriorPart,
        DespawnOnExit(AppState::InGame),
    ));
}

/// An invisible solid box (unit cube scaled), where a model stands.
fn solid(commands: &mut Commands, tf: Transform, collider: avian3d::prelude::Collider) {
    commands.spawn((
        tf,
        avian3d::prelude::RigidBody::Static,
        collider,
        InteriorPart,
        DespawnOnExit(AppState::InGame),
    ));
}

/// The stall door model is 0.92 m wide along its x: turn that to `along`.
fn door_turn(along: Vec3) -> Quat {
    Quat::from_rotation_arc(Vec3::X, along)
}

/// Furnish one bathroom cell. The layout comes from the seed and which way a
/// wall faces, not the cell, so every cell is laid out the same way (that's
/// the confusing part); only the dirt differs.
pub(super) fn furnish(
    commands: &mut Commands,
    kit: &Kit,
    seed: u32,
    (x, z): (usize, usize),
    mask: u8,
    hum: Option<&Handle<AudioSource>>,
) {
    let c = cell_centre(x, z);
    let cw = Vec3::new(c.x, INSIDE_Y, c.y);
    let mut dirt = Rng::new(seed ^ (x as u32 * 31 + z as u32 * 977) ^ 0xd127);
    // Tiles underfoot.
    commands.spawn((
        Mesh3d(kit.floor.clone()),
        MeshMaterial3d(kit.tiles.clone()),
        Transform::from_translation(cw + Vec3::Y * 0.011),
        BathFloor,
        InteriorPart,
        DespawnOnExit(AppState::InGame),
    ));
    if mask == 0 {
        // Bricked up: solid tile, with a door far out of reach.
        slab(
            commands,
            kit,
            &kit.wall,
            at(cw + Vec3::Y * CEILING / 2.0, Vec3::new(CELL, CEILING, CELL)),
            true,
        );
        return;
    }
    // A buzzing tube on long wires, far overhead, and its hum.
    // The model's rods are 1.6 m long, up from the tube to the ceiling.
    let hang = CEILING - 1.6;
    model(
        commands,
        "bath_tube.gltf",
        Transform::from_translation(cw + Vec3::Y * hang),
    );
    let base = 240_000.0;
    commands.spawn((
        InteriorPart,
        PointLight {
            color: Color::srgb(0.88, 1.0, 0.92),
            intensity: base,
            range: 20.0,
            ..default()
        },
        Flicker {
            base,
            seed: (x * 7 + z * 13) as f32,
            bad: 0.12 + dirt.roll() * 0.1,
        },
        Transform::from_translation(cw + Vec3::Y * (hang - 0.3)),
        DespawnOnExit(AppState::InGame),
    ));
    if let Some(hum) = hum {
        let e = crate::spatial::loop_at(
            commands,
            hum,
            cw + Vec3::Y * 2.5,
            0.32,
            crate::spatial::Carry {
                near: 2.5,
                far: 16.0,
            },
        );
        commands.entity(e).insert(InteriorPart);
    }
    // Something brown on the floor.
    if dirt.roll() < 0.6 {
        let p = cw + Vec3::new(dirt.roll() - 0.5, 0.0, dirt.roll() - 0.5) * 2.0;
        // The puddle model is a 1 m blot: stretch it to this one's size.
        model(
            commands,
            "bath_puddle.gltf",
            Transform::from_translation(p + Vec3::Y * 0.025).with_scale(Vec3::new(
                0.6 + dirt.roll() * 0.8,
                1.0,
                0.5 + dirt.roll() * 0.7,
            )),
        );
    }
    let mut stalled: Vec<Vec3> = Vec::new();
    for (i, (side, dir)) in [(N, -Vec3::Z), (E, Vec3::X), (S, Vec3::Z), (WEST, -Vec3::X)]
        .into_iter()
        .enumerate()
    {
        if mask & side != 0 {
            continue;
        }
        let mut layout = Rng::new(seed ^ 0x57a1 ^ (i as u32 * 0x1f1f));
        // Stalls only on facing walls: rows on walls that meet would collide.
        let corner = stalled.iter().any(|&d: &Vec3| d.dot(dir).abs() < 0.5);
        if layout.roll() < 0.8 && !corner {
            stalled.push(dir);
            stalls(commands, cw, dir, &mut layout, &mut dirt);
        } else {
            high_door(commands, cw, dir, &mut layout);
        }
    }
}

/// A door on the wall, metres off the floor. Nobody can reach it.
fn high_door(commands: &mut Commands, cw: Vec3, dir: Vec3, layout: &mut Rng) {
    let along = Vec3::new(dir.z.abs(), 0.0, dir.x.abs());
    let lift = 3.0 + layout.roll() * 4.0;
    let p = cw + dir * (CELL / 2.0 - 0.17) + along * (layout.roll() - 0.5) * 2.0;
    // The partition it hangs on (1.1 x 2.3 m along the model's x).
    model(
        commands,
        "high_partition.gltf",
        Transform::from_translation(p + Vec3::Y * (lift + 1.05)).with_rotation(door_turn(along)),
    );
    // The stall door model, stretched to this door's 0.9 x 2 m (its latch
    // stands in for the old chrome knob).
    model(
        commands,
        "stall_door.gltf",
        Transform::from_translation(p - dir * 0.03 + Vec3::Y * (lift + 1.0))
            .with_rotation(door_turn(along))
            .with_scale(Vec3::new(0.9 / 0.92, 2.0 / 1.7, 1.0)),
    );
}

/// Three stalls in a row against the wall on `dir`'s side.
fn stalls(commands: &mut Commands, cw: Vec3, dir: Vec3, layout: &mut Rng, dirt: &mut Rng) {
    const DEPTH: f32 = 1.45;
    const WIDE: f32 = 1.2;
    const TALL: f32 = 2.0;
    const GAP: f32 = 0.25;
    let along = Vec3::new(dir.z.abs(), 0.0, dir.x.abs());
    let thin = dir.abs();
    let back = cw + dir * (CELL / 2.0 - 0.15);
    let front = back - dir * DEPTH;
    let y = GAP + TALL / 2.0;
    // Partitions, with a gap under them. Some middle ones have a hole right
    // through, so one stall opens into the next.
    for k in 0..4 {
        let off = (k as f32 - 1.5) * WIDE;
        let line = back - dir * (DEPTH / 2.0) + along * off;
        if (k == 1 || k == 2) && layout.roll() < 0.5 {
            for (from, len) in [(0.0, 0.35), (DEPTH - 0.35, 0.35)] {
                let mid = back - dir * (from + len / 2.0) + along * off;
                solid(
                    commands,
                    at(
                        mid + Vec3::Y * y,
                        thin * len + along * 0.05 + Vec3::Y * TALL,
                    ),
                    avian3d::prelude::Collider::cuboid(1.0, 1.0, 1.0),
                );
                // The stub model is 0.35 m deep along its z.
                model(
                    commands,
                    "stall_stub.gltf",
                    Transform::from_translation(mid + Vec3::Y * y)
                        .with_rotation(Quat::from_rotation_arc(Vec3::Z, thin)),
                );
            }
            // Over the hole, the partition carries on (1.45 m along z).
            model(
                commands,
                "stall_bar.gltf",
                Transform::from_translation(line + Vec3::Y * (GAP + TALL - 0.2))
                    .with_rotation(Quat::from_rotation_arc(Vec3::Z, thin)),
            );
        } else {
            // A whole partition: the modeler-made panel (1.45 m deep along
            // its z, on chrome feet) and the same solid box as before.
            solid(
                commands,
                at(
                    line + Vec3::Y * y,
                    thin * DEPTH + along * 0.05 + Vec3::Y * TALL,
                ),
                avian3d::prelude::Collider::cuboid(1.0, 1.0, 1.0),
            );
            model(
                commands,
                "stall_panel.gltf",
                Transform::from_translation(line + Vec3::Y * y)
                    .with_rotation(Quat::from_rotation_arc(Vec3::Z, thin)),
            );
        }
    }
    // The rail along the front (3.65 m along the model's x).
    model(
        commands,
        "stall_rail.gltf",
        Transform::from_translation(front + Vec3::Y * (GAP + TALL)).with_rotation(door_turn(along)),
    );
    for i in 0..3 {
        let mid = (i as f32 - 1.0) * WIDE;
        door(commands, front, dir, along, mid, layout);
        toilet(commands, back, dir, along, mid, dirt);
    }
}

/// A stall door: ajar, hanging off one hinge, fallen in, hung far too high,
/// or gone.
fn door(commands: &mut Commands, front: Vec3, dir: Vec3, along: Vec3, mid: f32, layout: &mut Rng) {
    let size = along * 0.92 + Vec3::Y * 1.7 + dir.abs() * 0.04;
    let hinge = front + along * (mid - 0.46);
    // Yaw that swings the door's free edge into the stall.
    let swing = |a: f32| {
        let into = dir.cross(along).y.signum();
        Quat::from_rotation_y(a * into)
    };
    let roll = layout.roll();
    let tf = if roll < 0.25 {
        // Ajar.
        let r = swing(0.5 + layout.roll() * 0.8);
        Transform::from_translation(hinge + r * (along * 0.46) + Vec3::Y * 1.1)
            .with_rotation(r)
            .with_scale(size)
    } else if roll < 0.45 {
        // Hanging off one hinge, askew.
        let r = swing(0.25) * Quat::from_axis_angle(dir, 0.45);
        Transform::from_translation(hinge + r * (along * 0.46) + Vec3::Y * 0.85)
            .with_rotation(r)
            .with_scale(size)
    } else if roll < 0.6 {
        // Fallen flat into the stall.
        let r =
            Quat::from_rotation_y(layout.roll() - 0.5) * Quat::from_axis_angle(along, FRAC_PI_2);
        Transform::from_translation(front + dir * 0.75 + along * mid + Vec3::Y * 0.04)
            .with_rotation(r)
            .with_scale(size)
    } else if roll < 0.85 {
        // Shut, but hung so high you walk right under it.
        let lift = 1.4 + layout.roll() * 1.4;
        Transform::from_translation(front + along * mid + Vec3::Y * (lift + 0.85)).with_scale(size)
    } else {
        return;
    };
    // The modeler-made door (hinges on its -x edge) in the same pose.
    let tf = Transform {
        rotation: tf.rotation * door_turn(along),
        scale: Vec3::ONE,
        ..tf
    };
    model(commands, "stall_door.gltf", tf);
}

/// A dirty toilet at the back of a stall.
fn toilet(commands: &mut Commands, back: Vec3, dir: Vec3, along: Vec3, mid: f32, dirt: &mut Rng) {
    let base = back - dir * 0.45 + along * mid;
    // The modeler-made toilet (tank, bowl, seat, filthy water), its tank
    // towards the wall, and the bowl's solid cylinder as before.
    solid(
        commands,
        Transform::from_translation(base + Vec3::Y * 0.2),
        avian3d::prelude::Collider::cylinder(0.2, 0.4),
    );
    let up = dirt.roll() < 0.4;
    model(
        commands,
        if up {
            "toilet_seat_up.gltf"
        } else {
            "toilet.gltf"
        },
        Transform::from_translation(base).looking_to(dir, Vec3::Y),
    );
    // Poop: on the seat, on the floor, smeared up the wall.
    let mess = dirt.roll();
    if mess < 0.45 {
        for k in 0..(1 + dirt.below(3)) {
            // The log model lies along its x.
            let r = Quat::from_rotation_y(dirt.roll() * PI);
            let spot = if up || k > 0 {
                base - dir * (0.35 + dirt.roll() * 0.25) + along * (dirt.roll() - 0.5) * 0.5
            } else {
                base + along * 0.1
            };
            let h = if up || k > 0 { 0.05 } else { 0.46 };
            model(
                commands,
                "poop_log.gltf",
                Transform::from_translation(spot + Vec3::Y * h).with_rotation(r),
            );
        }
    }
    if mess > 0.3 {
        for _ in 0..(2 + dirt.below(4)) {
            let p = base
                + along * (dirt.roll() - 0.5) * 0.6
                + dir * (0.05 + dirt.roll() * 0.1)
                + Vec3::Y * (0.42 + dirt.roll() * 0.05);
            model(
                commands,
                "poop_blob.gltf",
                Transform::from_translation(p)
                    .with_rotation(Quat::from_rotation_y(dirt.roll() * PI)),
            );
        }
    }
    if mess > 0.7 {
        // Smeared up the wall: the 1 m smear model (in its x/y, facing
        // +z) turned to face into the stall and shrunk to size.
        let p = back - dir * 0.01 + along * (mid + dirt.roll() * 0.4 - 0.2) + Vec3::Y * 1.2;
        model(
            commands,
            "poop_smear.gltf",
            Transform::from_translation(p)
                .looking_to(dir, Vec3::Y)
                .with_scale(Vec3::new(0.25, 0.4 + dirt.roll() * 0.5, 1.0)),
        );
    }
    if mess > 0.85 {
        // Overflowing.
        model(
            commands,
            "bath_puddle.gltf",
            Transform::from_translation(base - dir * 0.4 + Vec3::Y * 0.02)
                .with_rotation(door_turn(along))
                .with_scale(Vec3::new(0.9, 1.0, 0.8)),
        );
    }
}

/// Is the local player in the bathroom right now?
fn in_bathroom(seed: Option<u32>, feet: Option<Vec3>) -> Option<Zone> {
    let (seed, feet) = (seed?, feet?);
    let zone = zone(seed)?;
    (inside(feet) && zone.holds(feet.xz())).then_some(zone)
}

/// The bathroom's look (`data/base/place_looks.ron`): a flat, pale haze.
pub const LOOK: &str = "warehouse_bathroom";

/// Local: the air changes in the bathroom (a flat, pale haze instead of the
/// black fog, more light: `place.rs`), and the drone of the dark ducks
/// under the hum.
#[allow(clippy::too_many_arguments)]
fn bathroom_air(
    mut was: Local<bool>,
    here: Res<crate::place::Here>,
    mut looks: ResMut<crate::place::PlaceLooks>,
    warehouse: Query<&Warehouse>,
    spooked: Res<Spooked>,
    mut sinks: Query<&mut AudioSink>,
    settings: Option<Res<crate::settings::Settings>>,
) {
    let seed = warehouse.iter().next().and_then(|w| w.seed);
    let now = spooked.inside && in_bathroom(seed, Some(here.0)).is_some();
    if now {
        looks.push(LOOK);
    }
    if now == *was {
        return;
    }
    *was = now;
    let master = settings.map_or(0.8, |s| s.master_volume);
    if let Some(mut sink) = spooked.drone.and_then(|d| sinks.get_mut(d).ok()) {
        let v = if now { 0.1 } else { 0.55 };
        sink.set_volume(Volume::Linear(v * master));
    }
}

#[derive(Component)]
struct FartDressed {
    puffs: u32,
    cloud: Handle<StandardMaterial>,
    puff: Handle<Mesh>,
}

#[derive(Component)]
struct FartBody;

/// A green cloud, growing and drifting up, gone after a few seconds.
#[derive(Component)]
struct FartCloud(f32);

/// Tuning numbers: `assets/data/places/warehouse_visuals_bathroom.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    pub cloud_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/places/warehouse_visuals_bathroom.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

fn dress_fart_monsters(
    mut commands: Commands,
    new: Query<(Entity, &FartMonster), Without<FartDressed>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    for (e, m) in &new {
        let cloud = materials.add(StandardMaterial {
            base_color: Color::srgba(0.55, 0.7, 0.2, 0.35),
            emissive: LinearRgba::rgb(0.05, 0.09, 0.01),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        commands
            .entity(e)
            .insert((
                FartDressed {
                    puffs: m.puffs,
                    cloud,
                    puff: meshes.add(Sphere::new(1.0)),
                },
                Transform::from_translation(m.pos),
                Visibility::default(),
            ))
            .with_children(|p| {
                // The rigged modeler body (squat, swollen, its enormous
                // behind towards +z), shuffling with `idle` / `waddle`.
                p.spawn((
                    FartBody,
                    crate::rigged::Rigged { path: FART_MONSTER },
                    crate::rigged::RigClip::looping("idle"),
                    Transform::default(),
                    Visibility::default(),
                ));
            });
    }
}

/// The fart monster's rigged model (source `bt-fart-monster`).
const FART_MONSTER: &str = "models/features/fart_monster.gltf";

/// The clip for a monster that is (or isn't) waddling.
fn fart_clip(waddling: bool) -> crate::rigged::RigClip {
    crate::rigged::RigClip::looping(if waddling { "waddle" } else { "idle" }).fade(0.3)
}

#[allow(clippy::type_complexity)]
fn animate_fart_monsters(
    mut commands: Commands,
    time: Res<Time>,
    mut spooked: ResMut<Spooked>,
    audio: Option<ResMut<Assets<AudioSource>>>,
    mut monsters: Query<(&FartMonster, &mut FartDressed, &mut Transform, &Children)>,
    mut bodies: Query<&mut crate::rigged::RigClip, With<FartBody>>,
) {
    let k = 1.0 - (-12.0 * time.delta_secs()).exp();
    let mut audio = audio;
    for (m, mut dressed, mut tf, children) in &mut monsters {
        tf.translation = if tf.translation.distance(m.pos) > 5.0 {
            m.pos
        } else {
            tf.translation.lerp(m.pos, k)
        };
        tf.rotation = tf.rotation.slerp(Quat::from_rotation_y(m.yaw), k);
        for child in children.iter() {
            if let Ok(mut clip) = bodies.get_mut(child) {
                let want = fart_clip(m.waddling);
                if *clip != want {
                    *clip = want;
                }
            }
        }
        if dressed.puffs == m.puffs {
            continue;
        }
        dressed.puffs = m.puffs;
        let butt = tf.translation + tf.rotation * Vec3::new(0.0, 0.8, 0.8);
        if let Some(audio) = audio.as_deref_mut() {
            let s = sounds(&mut spooked, audio, &mut commands);
            let i = (m.puffs as usize) % s.farts.len();
            crate::spatial::play_at(
                &mut commands,
                &s.farts[i],
                butt,
                1.0,
                crate::spatial::Carry::CALL,
            );
        }
        commands
            .spawn((
                FartCloud(0.0),
                Transform::from_translation(butt).with_scale(Vec3::splat(0.3)),
                Visibility::default(),
                InteriorPart,
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|c| {
                for (i, off) in [
                    Vec3::ZERO,
                    Vec3::new(0.6, 0.3, 0.2),
                    Vec3::new(-0.5, 0.2, 0.4),
                    Vec3::new(0.1, 0.6, -0.4),
                ]
                .into_iter()
                .enumerate()
                {
                    c.spawn((
                        Mesh3d(dressed.puff.clone()),
                        MeshMaterial3d(dressed.cloud.clone()),
                        Transform::from_translation(off)
                            .with_scale(Vec3::splat(0.7 + i as f32 * 0.1)),
                        NotShadowCaster,
                    ));
                }
            });
    }
}

fn drift_clouds(
    mut commands: Commands,
    time: Res<Time>,
    mut clouds: Query<(Entity, &mut FartCloud, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (e, mut cloud, mut tf) in &mut clouds {
        cloud.0 += dt;
        if cloud.0 > tuning().cloud_secs {
            commands.entity(e).despawn();
            continue;
        }
        let f = cloud.0 / tuning().cloud_secs;
        tf.scale = Vec3::splat(0.3 + 1.1 * f.sqrt() * (1.0 - f * f).max(0.05).sqrt());
        tf.translation.y += dt * 0.25;
    }
}

/// Every peer: the fart monster gassed someone. A green flash and a fart for
/// them; a fart and a toast for everyone else.
#[allow(clippy::too_many_arguments)]
fn on_gassed(
    ev: On<Gassed>,
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    mut spooked: ResMut<Spooked>,
    audio: Option<ResMut<Assets<AudioSource>>>,
    mut flash: Query<&mut Scare>,
    mut text: Query<(&mut Text, &mut TextColor), With<ScareText>>,
    toast: Option<ResMut<Toast>>,
) {
    let mine = me.is_some_and(|m| m.0 == ev.player);
    let fart = audio.map(|mut a| {
        let s = sounds(&mut spooked, &mut a, &mut commands);
        s.farts[(ev.player as usize) % s.farts.len()].clone()
    });
    if mine {
        if let Some(fart) = fart {
            commands.spawn((
                AudioPlayer(fart),
                PlaybackSettings::DESPAWN,
                DespawnOnExit(AppState::InGame),
            ));
        }
        for mut f in &mut flash {
            *f = Scare(3.0, Vec3::new(0.2, 0.35, 0.02));
        }
        for (mut t, mut c) in &mut text {
            t.0 = "GASSED".into();
            c.0 = Color::srgb(0.55, 0.85, 0.1);
        }
    } else {
        if let Some(fart) = fart {
            crate::spatial::play_at(
                &mut commands,
                &fart,
                ev.at + Vec3::Y,
                1.0,
                crate::spatial::Carry::CALL,
            );
        }
        if let Some(mut toast) = toast {
            toast.show(format!(
                "the fart monster in the warehouse bathroom gassed player {}",
                ev.player
            ));
        }
    }
}

/// For the status line: is the local player in the bathroom?
pub(super) fn status_line(seed: Option<u32>, feet: Vec3) -> Option<&'static str> {
    in_bathroom(seed, Some(feet))
        .map(|_| "\nThe bathroom. Something in here farts. Keep your distance.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_monster_waddles_when_it_moves() {
        assert_eq!(fart_clip(true).name, "waddle");
        assert_eq!(fart_clip(false).name, "idle");
    }

    #[test]
    fn tiles_have_grout() {
        let px = tile_pixels(64);
        assert_eq!(px.len(), 64 * 64 * 4);
        let dark = px.chunks(4).filter(|p| p[0] < 120).count();
        assert!(dark > 64 * 4 && dark < 64 * 64 / 2, "{dark} grout pixels");
    }
}
