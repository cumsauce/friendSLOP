//! Every peer: the warehouse you see and hear (#130). The building and Jegg
//! outside, the inside built from the replicated seed, the soul pieces and
//! the monster, and the dark (lights, fog, flashlight, drone) while you're in.
//! The nightmare bathroom's look and sound (#161) is in [`bathroom`].

pub(crate) mod bathroom;

use bevy::audio::{AudioSource, Volume};
use std::f32::consts::FRAC_PI_2;

use bevy::light::NotShadowCaster;
use bevy::world_serialization::WorldInstanceReady;

use super::wfc::{CELL, E, ENTRANCE, H, N, Plan, Rng, S, W, WEST, cell_centre};
use super::*;
use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::interact::{Clickable, Clicked};
use crate::hud::{HudSlot, Toast};
use crate::people::{CharacterLook, Gait};
use crate::player::LocalPlayer;
use crate::world::MainCamera;

pub fn plugin(app: &mut App) {
    bathroom::plugin(app);
    app.init_resource::<Spooked>()
        .init_resource::<Built>()
        .add_observer(click)
        .add_observer(on_caught)
        .add_observer(on_delivered)
        .add_observer(no_shadows)
        .add_systems(OnEnter(AppState::InGame), (build_outside, spawn_ui, reset))
        .add_systems(OnExit(AppState::InGame), reset)
        .add_systems(
            Update,
            (
                build_inside,
                dress_pieces,
                dress_monsters,
                animate_pieces,
                animate_monsters,
                flicker,
                darkness,
                fade_scare,
                show_status.run_if(crate::world::hud_tick()),
            )
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            Update,
            finish_sound_bake.run_if(any_with_component::<SoundBake>),
        );
}

/// Local: is my body in the warehouse, and what the dark replaced.
#[derive(Resource, Default)]
pub struct Spooked {
    pub inside: bool,
    saved: Option<Saved>,
    drone: Option<Entity>,
    sounds: Option<Sounds>,
}

struct Saved {
    fog: Option<DistanceFog>,
    ambient: f32,
    suns: Vec<(Entity, f32)>,
}

#[derive(Clone)]
struct Sounds {
    drone: Handle<AudioSource>,
    breath: Handle<AudioSource>,
    screams: Vec<Handle<AudioSource>>,
    chime: Handle<AudioSource>,
    /// The fart monster's farts and the bathroom's buzz (#161).
    farts: Vec<Handle<AudioSource>>,
    still: Handle<AudioSource>,
}

/// Is the local player in the warehouse? (The band goes quiet; see fusion.)
pub fn spooked(s: &Option<Res<Spooked>>) -> bool {
    s.as_ref().is_some_and(|s| s.inside)
}

/// The warehouse's generated sounds, synthesised off the main thread the
/// first time they're wanted (#151: ~0.5 M samples used to hitch every peer's
/// frame as the expedition started). The handles are reserved up front; a
/// sound asked for before its samples are ready just starts a moment late.
fn sounds(
    spooked: &mut Spooked,
    audio: &mut Assets<AudioSource>,
    commands: &mut Commands,
) -> Sounds {
    if let Some(s) = &spooked.sounds {
        return s.clone();
    }
    let s = Sounds {
        drone: audio.reserve_handle(),
        breath: audio.reserve_handle(),
        screams: (0..3).map(|_| audio.reserve_handle()).collect(),
        chime: audio.reserve_handle(),
        farts: (0..bathroom::FARTS)
            .map(|_| audio.reserve_handle())
            .collect(),
        still: audio.reserve_handle(),
    };
    let mut handles = vec![s.drone.clone(), s.breath.clone()];
    handles.extend(s.screams.iter().cloned());
    handles.push(s.chime.clone());
    handles.extend(s.farts.iter().cloned());
    handles.push(s.still.clone());
    let pool = bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default);
    let task = pool.spawn(async move {
        let mut wavs = vec![
            sound::drone(sound::RATE, 12.0),
            sound::breathing(sound::RATE),
        ];
        wavs.extend((0..3).map(|i| sound::scream(sound::RATE, 11 + i)));
        wavs.push(sound::chime(sound::RATE));
        wavs.extend(bathroom::fart_samples());
        wavs.push(sound::bathroom(sound::RATE, 8.0));
        wavs.iter()
            .map(|v| wav_bytes(v, sound::RATE))
            .collect::<Vec<_>>()
    });
    commands.spawn(SoundBake(task, handles));
    spooked.sounds = Some(s.clone());
    s
}

/// Warehouse sounds being synthesised, and the handles they fill.
#[derive(Component)]
struct SoundBake(bevy::tasks::Task<Vec<Vec<u8>>>, Vec<Handle<AudioSource>>);

fn finish_sound_bake(
    mut commands: Commands,
    mut bakes: Query<(Entity, &mut SoundBake)>,
    audio: Option<ResMut<Assets<AudioSource>>>,
) {
    let Some(mut audio) = audio else { return };
    for (e, mut bake) in &mut bakes {
        let Some(wavs) = bevy::tasks::futures::check_ready(&mut bake.0) else {
            continue;
        };
        for (handle, bytes) in bake.1.iter().zip(wavs) {
            let _ = audio.insert(
                handle,
                AudioSource {
                    bytes: bytes.into(),
                },
            );
        }
        commands.entity(e).despawn();
    }
}

/// Which floor plan is built (and its parts).
#[derive(Resource, Default)]
struct Built(Option<u32>);

fn reset(mut spooked: ResMut<Spooked>, mut built: ResMut<Built>) {
    let sounds = spooked.sounds.take();
    *spooked = Spooked {
        sounds,
        ..default()
    };
    built.0 = None;
}

#[derive(Component)]
struct OutsideDoor;

#[derive(Component)]
struct InsideDoor;

#[derive(Component)]
struct Jegg;

#[derive(Component)]
struct InteriorPart;

/// A light that flickers (dying fluorescent tubes, a bad bulb).
#[derive(Component)]
struct Flicker {
    base: f32,
    seed: f32,
    /// How badly: 0 steady .. 1 barely working.
    bad: f32,
}

fn mat(materials: &mut Assets<StandardMaterial>, c: Color, rough: f32) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: c,
        perceptual_roughness: rough,
        ..default()
    })
}

fn glow(
    materials: &mut Assets<StandardMaterial>,
    c: Color,
    power: f32,
) -> Handle<StandardMaterial> {
    let l = c.to_linear();
    materials.add(StandardMaterial {
        base_color: c,
        emissive: LinearRgba::rgb(l.red * power, l.green * power, l.blue * power),
        ..default()
    })
}

/// A box (mesh, material, centre, size), optionally solid.
fn block(
    commands: &mut Commands,
    cube: &Handle<Mesh>,
    m: &Handle<StandardMaterial>,
    at: Vec3,
    size: Vec3,
    solid: bool,
) -> Entity {
    let mut e = commands.spawn((
        Mesh3d(cube.clone()),
        MeshMaterial3d(m.clone()),
        Transform::from_translation(at).with_scale(size),
        DespawnOnExit(AppState::InGame),
    ));
    if solid {
        e.insert((
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(1.0, 1.0, 1.0),
        ));
    }
    e.id()
}

/// The building outside, its door and lamp, and Jegg.
fn build_outside(
    mut commands: Commands,
    assets: Option<Res<AssetServer>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(assets), Some(mut meshes), Some(mut materials)) = (assets, meshes, materials) else {
        return;
    };
    // Modeler-made (sources in `.modeler/`, prefabs `sp-warehouse-exterior`,
    // `sp-oil-drum`, `sp-lantern`).
    let model = |file: &str| -> WorldAssetRoot {
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
        )
    };
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let y0 = outside_floor();
    let c = Vec3::new(outside_at().x, y0, outside_at().y);
    let (sx, sz, hy) = (outside_size().x, outside_size().y, outside_height());
    // The building outside: plinth (hides the slope), rusty ribbed shell,
    // roof, dead windows (some boarded up), the rolling shutter and the lamp
    // over the door, all one model at the floor centre.
    commands.spawn((
        model("warehouse_exterior.gltf"),
        Transform::from_translation(c),
        DespawnOnExit(AppState::InGame),
    ));
    // The shell still blocks bodies (the same box as before the model).
    commands.spawn((
        Transform::from_translation(c + Vec3::Y * hy / 2.0).with_scale(Vec3::new(sx, hy, sz)),
        avian3d::prelude::RigidBody::Static,
        avian3d::prelude::Collider::cuboid(1.0, 1.0, 1.0),
        DespawnOnExit(AppState::InGame),
    ));
    let d = door_at();
    commands.spawn((
        OutsideDoor,
        Transform::from_translation(d - Vec3::Z * 0.2),
        Visibility::default(),
        Clickable::at_height(1.6)
            .radius(110.0)
            .labelled("vote to go in"),
        DespawnOnExit(AppState::InGame),
    ));
    // One sick, buzzing lamp over the door.
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.75, 0.45),
            intensity: 120_000.0,
            range: 10.0,
            ..default()
        },
        Flicker {
            base: 120_000.0,
            seed: 3.0,
            bad: 0.7,
        },
        Transform::from_translation(Vec3::new(d.x, d.y + 5.0, d.z - 0.8)),
        DespawnOnExit(AppState::InGame),
    ));
    // Junk in the yard.
    let crate_mat = mat(&mut materials, Color::srgb(0.5, 0.37, 0.22), 0.9);
    for (i, off) in [
        Vec2::new(-4.5, -1.0),
        Vec2::new(-5.4, -0.6),
        Vec2::new(-4.9, -2.0),
    ]
    .iter()
    .enumerate()
    {
        let at = Vec3::new(d.x + off.x, 0.0, d.z + off.y);
        let at = at.with_y(crate::terrain::height_at(at.x, at.z));
        if i == 2 {
            block(
                &mut commands,
                &cube,
                &crate_mat,
                at + Vec3::Y * 0.45,
                Vec3::splat(0.9),
                true,
            );
        } else {
            commands.spawn((
                model("oil_drum.gltf"),
                Transform::from_translation(at + Vec3::Y * 0.48),
                avian3d::prelude::RigidBody::Static,
                avian3d::prelude::Collider::cylinder(0.35, 0.95),
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    // Jegg, waiting right outside, with a lantern at their feet.
    let j = jegg_at();
    commands
        .spawn((
            Jegg,
            Transform::from_translation(j).looking_at(j + Vec3::new(-0.4, 0.0, -1.0), Vec3::Y),
            Visibility::default(),
            Gait::default(),
            CharacterLook::random(0x7e66_0130),
            Clickable::at_height(1.0)
                .radius(70.0)
                .labelled("talk to Jegg"),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|p| {
            p.spawn((model("lantern.gltf"), Transform::from_xyz(0.45, 0.13, -0.2)));
            p.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.8, 0.5),
                    intensity: 25_000.0,
                    range: 5.0,
                    ..default()
                },
                Transform::from_xyz(0.45, 0.4, -0.2),
            ));
        });
}

/// Every peer: build the inside for the current seed (and take down the old one).
#[allow(clippy::too_many_arguments)]
fn build_inside(
    mut commands: Commands,
    mut built: ResMut<Built>,
    mut spooked: ResMut<Spooked>,
    warehouse: Query<&Warehouse>,
    parts: Query<Entity, With<InteriorPart>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    images: Option<ResMut<Assets<Image>>>,
    audio: Option<ResMut<Assets<AudioSource>>>,
    assets: Option<Res<AssetServer>>,
) {
    let want = warehouse.iter().next().and_then(|w| w.seed);
    if built.0 == want {
        return;
    }
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    for e in &parts {
        commands.entity(e).despawn();
    }
    built.0 = want;
    let Some(seed) = want else { return };
    let plan = Plan::generate(seed);
    let mut rng = Rng::new(seed ^ 0xdec0);
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    // The nightmare bathroom (#161), if this warehouse has one.
    let bath = super::bathroom::zone(seed);
    let in_bath = |c: (usize, usize)| bath.is_some_and(|b| b.contains(c));
    let ceiling = |c: (usize, usize)| super::bathroom::ceiling(bath, c);
    let kit = bath.map(|_| {
        bathroom::kit(
            &mut meshes,
            &mut materials,
            images.map(|i| i.into_inner()),
            &cube,
        )
    });
    let hum = audio.map(|mut a| sounds(&mut spooked, &mut a, &mut commands).still);
    let tall = if bath.is_some() {
        super::bathroom::CEILING
    } else {
        INSIDE_WALL
    };
    let floor = mat(&mut materials, Color::srgb(0.22, 0.22, 0.23), 0.95);
    let wall = mat(&mut materials, Color::srgb(0.3, 0.29, 0.27), 0.95);
    let stain = mat(&mut materials, Color::srgb(0.16, 0.14, 0.12), 1.0);
    let exit_sign = glow(&mut materials, Color::srgb(0.3, 1.0, 0.45), 3.0);
    let shutter = mat(&mut materials, Color::srgb(0.3, 0.3, 0.32), 0.6);
    let (gw, gh) = (W as f32 * CELL, H as f32 * CELL);
    let y = INSIDE_Y;
    let top = y + INSIDE_WALL;
    let part = |commands: &mut Commands,
                m: &Handle<StandardMaterial>,
                at: Vec3,
                size: Vec3,
                solid: bool| {
        let e = block(commands, &cube, m, at, size, solid);
        commands.entity(e).insert(InteriorPart);
    };
    // Modeler-made props (sources in `.modeler/`, prefabs `wh-*`): the
    // boxes are unit cubes centred on their origin, scaled per instance; the
    // three looks stand in for the three cardboard tones. A missing file just
    // draws nothing (the colliders are separate from the models).
    let prop = |commands: &mut Commands, file: &str, tf: Transform| -> Entity {
        let mut e = commands.spawn((
            tf,
            Visibility::default(),
            InteriorPart,
            DespawnOnExit(AppState::InGame),
        ));
        if let Some(assets) = &assets {
            e.insert(WorldAssetRoot(assets.load(
                GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}")),
            )));
        }
        e.id()
    };
    const BOXES: [&str; 3] = ["wh_box_a.gltf", "wh_box_b.gltf", "wh_box_c.gltf"];
    let solid = |commands: &mut Commands, e: Entity| {
        commands.entity(e).insert((
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(1.0, 1.0, 1.0),
        ));
    };
    // Floor, ceiling and the outer shell.
    part(
        &mut commands,
        &floor,
        Vec3::new(0.0, y - 0.25, 0.0),
        Vec3::new(gw + 1.0, 0.5, gh + 1.0),
        true,
    );
    if let Some(kit) = &kit {
        // A ceiling per cell: the bathroom's is far higher.
        for z in 0..H {
            for x in 0..W {
                let c = cell_centre(x, z);
                let m = if in_bath((x, z)) { &kit.wall } else { &wall };
                part(
                    &mut commands,
                    m,
                    Vec3::new(c.x, y + ceiling((x, z)) + 0.25, c.y),
                    Vec3::new(CELL + 0.02, 0.5, CELL + 0.02),
                    true,
                );
            }
        }
    } else {
        part(
            &mut commands,
            &wall,
            Vec3::new(0.0, top + 0.25, 0.0),
            Vec3::new(gw + 1.0, 0.5, gh + 1.0),
            true,
        );
    }
    for (at, size) in [
        (
            Vec3::new(0.0, y + tall / 2.0, -gh / 2.0 - 0.2),
            Vec3::new(gw + 0.8, tall, 0.4),
        ),
        (
            Vec3::new(0.0, y + tall / 2.0, gh / 2.0 + 0.2),
            Vec3::new(gw + 0.8, tall, 0.4),
        ),
        (
            Vec3::new(-gw / 2.0 - 0.2, y + tall / 2.0, 0.0),
            Vec3::new(0.4, tall, gh),
        ),
        (
            Vec3::new(gw / 2.0 + 0.2, y + tall / 2.0, 0.0),
            Vec3::new(0.4, tall, gh),
        ),
    ] {
        part(&mut commands, &wall, at, size, true);
    }
    // The loading door from the inside, with a weak green sign over it.
    let ex = exit_at();
    for k in 0..9 {
        part(
            &mut commands,
            &shutter,
            Vec3::new(ex.x, y + 0.2 + k as f32 * 0.38, ex.z + 0.05),
            Vec3::new(3.0, 0.33, 0.08),
            false,
        );
    }
    part(
        &mut commands,
        &exit_sign,
        Vec3::new(ex.x, y + 3.75, ex.z + 0.08),
        Vec3::new(0.9, 0.3, 0.06),
        false,
    );
    commands.spawn((
        InsideDoor,
        InteriorPart,
        Transform::from_translation(ex + Vec3::Z * 0.3),
        Visibility::default(),
        Clickable::at_height(1.4)
            .radius(100.0)
            .labelled("leave the warehouse"),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        InteriorPart,
        PointLight {
            color: Color::srgb(0.4, 1.0, 0.5),
            intensity: 6_000.0,
            range: 6.0,
            ..default()
        },
        Transform::from_translation(Vec3::new(ex.x, y + 3.4, ex.z + 0.6)),
        DespawnOnExit(AppState::InGame),
    ));
    // Walls between cells, wherever the plan says an aisle doesn't go on
    // (as tall as the taller ceiling), and over the way into the bathroom.
    for z in 0..H {
        for x in 0..W {
            let c = cell_centre(x, z);
            for (bit, n, at, size) in [
                (
                    E,
                    (x + 1, z),
                    Vec2::new(c.x + CELL / 2.0, c.y),
                    Vec2::new(0.3, CELL + 0.3),
                ),
                (
                    S,
                    (x, z + 1),
                    Vec2::new(c.x, c.y + CELL / 2.0),
                    Vec2::new(CELL + 0.3, 0.3),
                ),
            ] {
                if n.0 >= W || n.1 >= H {
                    continue;
                }
                let m = match &kit {
                    Some(kit) if in_bath((x, z)) || in_bath(n) => &kit.wall,
                    _ => &wall,
                };
                let (lo, hi) = if plan.open(x, z, bit) {
                    (
                        ceiling((x, z)).min(ceiling(n)),
                        ceiling((x, z)).max(ceiling(n)),
                    )
                } else {
                    (0.0, ceiling((x, z)).max(ceiling(n)))
                };
                if hi - lo < 0.01 {
                    continue;
                }
                part(
                    &mut commands,
                    m,
                    Vec3::new(at.x, y + (lo + hi) / 2.0, at.y),
                    Vec3::new(size.x, hi - lo, size.y),
                    true,
                );
            }
        }
    }
    // Furnish each cell: stacked crates where it's blocked up, shelving
    // against its walls, a stain, a pallet; a few lights that barely work.
    let mut lamps = 0;
    for z in 0..H {
        for x in 0..W {
            let c = cell_centre(x, z);
            let cw = Vec3::new(c.x, y, c.y);
            let mask = plan.cells[z][x];
            if let Some(kit) = kit.as_ref().filter(|_| in_bath((x, z))) {
                bathroom::furnish(&mut commands, kit, seed, (x, z), mask, hum.as_ref());
                continue;
            }
            if mask == 0 {
                for k in 0..4 {
                    let s = 1.4 + rng.roll() * 0.6;
                    let off =
                        Vec3::new((k % 2) as f32 * 2.0 - 1.0, 0.0, (k / 2) as f32 * 2.0 - 1.0)
                            * 1.1;
                    let look = BOXES[rng.below(3)];
                    let e = prop(
                        &mut commands,
                        look,
                        Transform::from_translation(cw + off + Vec3::Y * s / 2.0)
                            .with_rotation(Quat::from_rotation_y(k as f32 * FRAC_PI_2))
                            .with_scale(Vec3::splat(s)),
                    );
                    solid(&mut commands, e);
                    prop(
                        &mut commands,
                        look,
                        Transform::from_translation(cw + off + Vec3::Y * (s + 0.5))
                            .with_rotation(Quat::from_rotation_y(0.3 * k as f32 - 0.45)),
                    );
                }
                continue;
            }
            if rng.roll() < 0.6 {
                let at = cw + Vec3::new(rng.roll() - 0.5, 0.01, rng.roll() - 0.5) * 2.5;
                part(
                    &mut commands,
                    &stain,
                    at,
                    Vec3::new(0.8 + rng.roll(), 0.02, 0.6 + rng.roll()),
                    false,
                );
            }
            for (bit, dir) in [(N, -Vec3::Z), (E, Vec3::X), (S, Vec3::Z), (WEST, -Vec3::X)] {
                let door = (x, z) == ENTRANCE && bit == N;
                if mask & bit != 0 || door || rng.roll() < 0.45 {
                    continue;
                }
                // A rack along this wall (the model runs along x, origin
                // on the floor under its middle), and some boxes on it.
                let along = Vec3::new(dir.z.abs(), 0.0, dir.x.abs());
                let len = CELL - 1.4;
                let back = cw + dir * (CELL / 2.0 - 0.6);
                let depth = dir.abs() * 0.6 + along * len;
                let yaw = if dir.x != 0.0 { FRAC_PI_2 } else { 0.0 };
                prop(
                    &mut commands,
                    "wh_rack.gltf",
                    Transform::from_translation(back).with_rotation(Quat::from_rotation_y(yaw)),
                );
                for k in 0..3 {
                    for b in 0..3 {
                        if rng.roll() < 0.4 {
                            continue;
                        }
                        let s = 0.35 + rng.roll() * 0.35;
                        let at = back
                            + along * ((b as f32 - 1.0) * len / 3.2)
                            + Vec3::Y * (0.34 + k as f32 * 1.0 + s / 2.0);
                        prop(
                            &mut commands,
                            BOXES[rng.below(3)],
                            Transform::from_translation(at)
                                .with_rotation(Quat::from_rotation_y(yaw))
                                .with_scale(Vec3::splat(s)),
                        );
                    }
                }
                // One collider for the whole rack.
                commands.spawn((
                    InteriorPart,
                    Transform::from_translation(back + Vec3::Y * 1.3).with_scale(depth.with_y(2.6)),
                    avian3d::prelude::RigidBody::Static,
                    avian3d::prelude::Collider::cuboid(1.0, 1.0, 1.0),
                    DespawnOnExit(AppState::InGame),
                ));
            }
            if mask.count_ones() >= 2 && rng.roll() < 0.3 {
                let at = cw + Vec3::new(rng.roll() - 0.5, 0.0, rng.roll() - 0.5) * 1.5;
                // The pallet model is 1.2 x 0.14 x 1.0, centred on its origin.
                prop(
                    &mut commands,
                    "wh_pallet.gltf",
                    Transform::from_translation(at + Vec3::Y * 0.07),
                );
            }
            if lamps < 8 && rng.roll() < 0.22 {
                lamps += 1;
                // Two glowing tubes (1.4 m, centred on the origin) under a
                // housing, hung from the ceiling 0.3 m above.
                prop(
                    &mut commands,
                    "wh_tube.gltf",
                    Transform::from_translation(cw + Vec3::Y * (INSIDE_WALL - 0.3)),
                );
                let base = 18_000.0 + rng.roll() * 10_000.0;
                commands.spawn((
                    InteriorPart,
                    PointLight {
                        color: Color::srgb(0.8, 0.9, 1.0),
                        intensity: base,
                        range: 8.0,
                        ..default()
                    },
                    Flicker {
                        base,
                        seed: rng.roll() * 100.0,
                        bad: rng.roll(),
                    },
                    Transform::from_translation(cw + Vec3::Y * (INSIDE_WALL - 0.6)),
                    DespawnOnExit(AppState::InGame),
                ));
            }
        }
    }
}

/// Dying lights: mostly on, sometimes stuttering off.
fn flicker(time: Res<Time>, mut lights: Query<(&Flicker, &mut PointLight)>) {
    let t = time.elapsed_secs();
    for (f, mut light) in &mut lights {
        let slow = ((t * 0.7 + f.seed).sin() * 0.5 + 0.5).powf(3.0);
        let fast = ((t * 31.0 + f.seed * 7.0).sin() * (t * 17.0 + f.seed).cos()).abs();
        let off = slow * f.bad > 0.35 && fast < 0.6;
        let want = if off {
            f.base * 0.03
        } else {
            f.base * (0.85 + 0.15 * fast)
        };
        if (light.intensity - want).abs() > 1.0 {
            light.intensity = want;
        }
    }
}

#[derive(Component)]
struct PieceDressed;

#[derive(Component)]
struct Crystal;

/// A model whose meshes cast no shadows (the glowing soul crystal).
#[derive(Component)]
struct NoShadows;

fn no_shadows(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    marked: Query<(), With<NoShadows>>,
    children: Query<&Children>,
    meshes: Query<(), With<Mesh3d>>,
) {
    if marked.get(ready.entity).is_err() {
        return;
    }
    for child in children.iter_descendants(ready.entity) {
        if meshes.get(child).is_ok() {
            commands.entity(child).insert(NotShadowCaster);
        }
    }
}

/// The soul crystal: a modeler-made glowing bipyramid (0.28 wide, 0.6 tall,
/// centred on its origin; prefab `wh-crystal`) and its light.
fn crystal(p: &mut ChildSpawnerCommands, assets: Option<&AssetServer>) {
    p.spawn((Crystal, Transform::default(), Visibility::default()))
        .with_children(|c| {
            if let Some(assets) = assets {
                c.spawn((
                    WorldAssetRoot(assets.load(
                        GltfAssetLabel::Scene(0).from_asset("models/features/wh_crystal.gltf"),
                    )),
                    NoShadows,
                    Transform::default(),
                ));
            }
            c.spawn((
                PointLight {
                    color: Color::srgb(0.55, 0.8, 1.0),
                    intensity: 9_000.0,
                    range: 5.0,
                    ..default()
                },
                Transform::default(),
            ));
        });
}

fn dress_pieces(
    mut commands: Commands,
    new: Query<(Entity, &SoulPiece), Without<PieceDressed>>,
    assets: Option<Res<AssetServer>>,
) {
    let Some(assets) = assets else {
        return;
    };
    for (e, piece) in &new {
        commands
            .entity(e)
            .insert((
                PieceDressed,
                Transform::from_translation(piece.pos),
                Visibility::default(),
                Clickable::at_height(0.0)
                    .radius(60.0)
                    .labelled("take the soul piece"),
            ))
            .with_children(|p| crystal(p, Some(&assets)));
    }
}

/// Pieces glide to where they are, spin and bob. A carried one can't be clicked.
fn animate_pieces(
    mut commands: Commands,
    time: Res<Time>,
    mut pieces: Query<(Entity, &SoulPiece, &mut Transform, Has<Clickable>), With<PieceDressed>>,
) {
    let t = time.elapsed_secs();
    let k = 1.0 - (-18.0 * time.delta_secs()).exp();
    for (e, piece, mut tf, clickable) in &mut pieces {
        let bob = Vec3::Y * (t * 2.0 + piece.id as f32).sin() * 0.08;
        let to = piece.pos + bob;
        tf.translation = if tf.translation.distance(to) > 4.0 {
            to
        } else {
            tf.translation.lerp(to, k)
        };
        tf.rotation = Quat::from_rotation_y(t * 1.3 + piece.id as f32);
        let carried = piece.carrier.is_some();
        if carried && clickable {
            commands.entity(e).remove::<Clickable>();
        } else if !carried && !clickable {
            commands.entity(e).insert(
                Clickable::at_height(0.0)
                    .radius(60.0)
                    .labelled("take the soul piece"),
            );
        }
    }
}

#[derive(Component)]
struct MonsterDressed {
    last: MonsterMode,
}

#[derive(Component)]
struct MonsterBody;

#[derive(Component)]
struct MonsterDecoy;

#[derive(Component)]
struct MonsterArm(f32);

#[derive(Component)]
struct MonsterEye;

fn dress_monsters(
    mut commands: Commands,
    new: Query<(Entity, &Monster), Without<MonsterDressed>>,
    mut spooked: ResMut<Spooked>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    audio: Option<ResMut<Assets<AudioSource>>>,
    assets: Option<Res<AssetServer>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let mut audio = audio;
    for (e, m) in &new {
        let skin = mat(&mut materials, Color::srgb(0.03, 0.03, 0.035), 0.4);
        let eye = glow(&mut materials, Color::srgb(1.0, 0.95, 0.85), 8.0);
        let limb = meshes.add(Cylinder::new(0.06, 1.0));
        commands
            .entity(e)
            .insert((
                MonsterDressed { last: m.mode },
                Transform::from_translation(m.pos),
                Visibility::default(),
            ))
            .with_children(|p| {
                p.spawn((MonsterBody, Transform::default(), Visibility::default()))
                    .with_children(|b| {
                        // Tall, thin and hunched, arms down past its knees.
                        b.spawn((
                            Mesh3d(meshes.add(Capsule3d::new(0.22, 1.3))),
                            MeshMaterial3d(skin.clone()),
                            Transform::from_xyz(0.0, 1.6, 0.0)
                                .with_rotation(Quat::from_rotation_x(-0.25))
                                .with_scale(Vec3::new(1.0, 1.0, 0.7)),
                        ));
                        for side in [-1.0f32, 1.0] {
                            b.spawn((
                                Mesh3d(limb.clone()),
                                MeshMaterial3d(skin.clone()),
                                Transform::from_xyz(side * 0.14, 0.5, 0.05)
                                    .with_scale(Vec3::new(1.0, 1.05, 1.0)),
                            ));
                            b.spawn((
                                MonsterArm(side),
                                Transform::from_xyz(side * 0.3, 2.25, -0.15),
                                Visibility::default(),
                            ))
                            .with_children(|a| {
                                a.spawn((
                                    Mesh3d(limb.clone()),
                                    MeshMaterial3d(skin.clone()),
                                    Transform::from_xyz(0.0, -0.85, 0.0)
                                        .with_scale(Vec3::new(0.8, 1.7, 0.8)),
                                ));
                            });
                        }
                        b.spawn((
                            Mesh3d(meshes.add(Sphere::new(0.2))),
                            MeshMaterial3d(skin.clone()),
                            Transform::from_xyz(0.0, 2.5, -0.35)
                                .with_scale(Vec3::new(0.9, 1.25, 0.9)),
                        ))
                        .with_children(|h| {
                            for side in [-1.0f32, 1.0] {
                                h.spawn((
                                    MonsterEye,
                                    Mesh3d(meshes.add(Sphere::new(0.035))),
                                    MeshMaterial3d(eye.clone()),
                                    Transform::from_xyz(side * 0.08, 0.03, -0.18),
                                    NotShadowCaster,
                                ));
                            }
                        });
                    });
                p.spawn((
                    MonsterDecoy,
                    Transform::from_xyz(0.0, 0.8, 0.0),
                    Visibility::Hidden,
                ))
                .with_children(|d| crystal(d, assets.as_deref()));
            });
        // Its breathing follows it about.
        let Some(audio) = audio.as_deref_mut() else {
            continue;
        };
        let s = sounds(&mut spooked, audio, &mut commands);
        let breath = crate::spatial::loop_at(
            &mut commands,
            &s.breath,
            m.pos + Vec3::Y * 2.2,
            0.8,
            crate::spatial::Carry::STEP,
        );
        commands
            .entity(breath)
            .insert((ChildOf(e), Transform::from_xyz(0.0, 2.2, 0.0)));
    }
}

#[allow(clippy::type_complexity)]
fn animate_monsters(
    mut commands: Commands,
    time: Res<Time>,
    mut spooked: ResMut<Spooked>,
    audio: Option<ResMut<Assets<AudioSource>>>,
    mut monsters: Query<(&Monster, &mut MonsterDressed, &mut Transform, &Children)>,
    mut bodies: Query<
        (&mut Visibility, &mut Transform),
        (
            With<MonsterBody>,
            Without<MonsterDressed>,
            Without<MonsterDecoy>,
        ),
    >,
    mut decoys: Query<
        (&mut Visibility, &mut Transform),
        (
            With<MonsterDecoy>,
            Without<MonsterDressed>,
            Without<MonsterBody>,
        ),
    >,
    mut arms: Query<
        (&MonsterArm, &mut Transform),
        (
            Without<MonsterDressed>,
            Without<MonsterBody>,
            Without<MonsterDecoy>,
        ),
    >,
) {
    let t = time.elapsed_secs();
    let k = 1.0 - (-14.0 * time.delta_secs()).exp();
    let mut audio = audio;
    for (m, mut dressed, mut tf, children) in &mut monsters {
        let moved = tf.translation.distance(m.pos);
        tf.translation = if moved > 5.0 {
            m.pos
        } else {
            tf.translation.lerp(m.pos, k)
        };
        let lure = m.mode == MonsterMode::Lure;
        let hunting = matches!(m.mode, MonsterMode::Hunt(_));
        tf.rotation = tf.rotation.slerp(Quat::from_rotation_y(m.yaw), k);
        if dressed.last != m.mode {
            // It springs: a scream where it is.
            if hunting && let Some(audio) = audio.as_deref_mut() {
                let s = sounds(&mut spooked, audio, &mut commands);
                let i = (t as usize) % s.screams.len();
                crate::spatial::play_at(
                    &mut commands,
                    &s.screams[i],
                    m.pos + Vec3::Y * 2.0,
                    0.9,
                    crate::spatial::Carry::CALL,
                );
            }
            dressed.last = m.mode;
        }
        let pace = if hunting { 9.0 } else { 3.5 };
        for child in children.iter() {
            if let Ok((mut vis, mut body)) = bodies.get_mut(child) {
                vis.set_if_neq(if lure {
                    Visibility::Hidden
                } else {
                    Visibility::Inherited
                });
                let lean = if hunting { 0.35 } else { 0.0 };
                body.rotation = Quat::from_rotation_x(-lean)
                    * Quat::from_rotation_z((t * pace * 0.5).sin() * 0.05);
                body.translation.y = (t * pace).sin().abs() * 0.06;
            }
            if let Ok((mut vis, mut decoy)) = decoys.get_mut(child) {
                vis.set_if_neq(if lure {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                });
                decoy.translation.y = 0.8 + (t * 2.0).sin() * 0.08;
                decoy.rotation = Quat::from_rotation_y(t * 1.3);
            }
        }
        for (arm, mut a) in &mut arms {
            a.rotation = Quat::from_rotation_x(
                (t * pace + arm.0).sin() * if hunting { 0.9 } else { 0.25 }
                    - if hunting { 0.6 } else { 0.0 },
            );
        }
    }
}

/// The ambient light left in the dark.
const DARK_AMBIENT: f32 = 12.0;

/// The black fog of the dark.
fn dark_fog() -> DistanceFog {
    DistanceFog {
        color: Color::srgb(0.005, 0.005, 0.008),
        falloff: FogFalloff::Linear {
            start: 2.0,
            end: 24.0,
        },
        ..default()
    }
}

/// Local: going in turns the world off; coming out turns it back on.
#[allow(clippy::too_many_arguments)]
fn darkness(
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    mut spooked: ResMut<Spooked>,
    mut cameras: Query<(Entity, Option<&mut DistanceFog>), With<MainCamera>>,
    mut suns: Query<(Entity, &mut DirectionalLight)>,
    ambient: Option<ResMut<GlobalAmbientLight>>,
    flashlights: Query<Entity, With<Flashlight>>,
    settings: Option<Res<crate::settings::Settings>>,
    quality: Option<Res<crate::graphics::GraphicsQuality>>,
    audio: Option<ResMut<Assets<AudioSource>>>,
) {
    let mine = me
        .and_then(|me| walkers.iter().find(|(p, _)| p.id == me.0))
        .is_some_and(|(_, w)| inside(w.pos));
    if mine == spooked.inside {
        return;
    }
    let Ok((camera, fog)) = cameras.single_mut() else {
        return;
    };
    let Some(mut ambient) = ambient else { return };
    spooked.inside = mine;
    if mine {
        let saved = Saved {
            fog: fog.as_deref().cloned(),
            ambient: ambient.brightness,
            suns: suns.iter().map(|(e, s)| (e, s.illuminance)).collect(),
        };
        spooked.saved = Some(saved);
        ambient.brightness = DARK_AMBIENT;
        for (_, mut sun) in &mut suns {
            sun.illuminance = 0.0;
        }
        commands.entity(camera).insert(dark_fog());
        commands.spawn((
            Flashlight,
            SpotLight {
                color: Color::srgb(1.0, 0.95, 0.85),
                intensity: 400_000.0,
                range: 22.0,
                outer_angle: 0.42,
                inner_angle: 0.22,
                shadow_maps_enabled: quality.is_some_and(|q| q.shadows),
                ..default()
            },
            Transform::from_xyz(0.25, -0.2, 0.0),
            ChildOf(camera),
        ));
        if let Some(mut audio) = audio {
            let s = sounds(&mut spooked, &mut audio, &mut commands);
            let master = settings.map_or(0.8, |s| s.master_volume);
            let drone = commands
                .spawn((
                    AudioPlayer(s.drone),
                    PlaybackSettings::LOOP.with_volume(Volume::Linear(0.55 * master)),
                    DespawnOnExit(AppState::InGame),
                ))
                .id();
            spooked.drone = Some(drone);
        }
    } else {
        if let Some(saved) = spooked.saved.take() {
            ambient.brightness = saved.ambient;
            for (e, lux) in saved.suns {
                if let Ok((_, mut sun)) = suns.get_mut(e) {
                    sun.illuminance = lux;
                }
            }
            match saved.fog {
                Some(f) => {
                    commands.entity(camera).insert(f);
                }
                None => {
                    commands.entity(camera).remove::<DistanceFog>();
                }
            }
        }
        for e in &flashlights {
            commands.entity(e).despawn();
        }
        if let Some(d) = spooked.drone.take() {
            commands.entity(d).try_despawn();
        }
    }
}

#[derive(Component)]
struct Flashlight;

/// Local clicks: the doors, the soul pieces, Jegg.
#[allow(clippy::type_complexity)]
fn click(
    ev: On<Clicked>,
    mut commands: Commands,
    doors: Query<(Has<OutsideDoor>, Has<InsideDoor>, Has<Jegg>)>,
    pieces: Query<&SoulPiece>,
) {
    if let Ok(piece) = pieces.get(ev.entity) {
        commands.client_trigger(TakeSoul { id: piece.id });
        return;
    }
    let Ok((outside, inside, jegg)) = doors.get(ev.entity) else {
        return;
    };
    if outside {
        commands.client_trigger(KnockDoor);
    } else if inside {
        commands.client_trigger(LeaveWarehouse);
    } else if jegg {
        commands.client_trigger(GiveToJegg);
    }
}

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct ScareText;

/// A full-screen flash when it gets you, fading out: seconds left, and its
/// colour (red for the monster, green for the fart monster).
#[derive(Component)]
struct Scare(f32, Vec3);

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        StatusText,
        crate::hud::slot(HudSlot::TopCenter),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(72.0),
        TextColor(Color::srgb(0.9, 0.1, 0.1)),
        ScareText,
        crate::hud::slot(HudSlot::CenterBanner),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        BackgroundColor(Color::NONE),
        GlobalZIndex(50),
        Scare(0.0, Vec3::ZERO),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(format!(
            "the warehouse (south of the commune): everyone clicks its door to go in together. \
             Find the glowing soul pieces, carry them out and click Jegg. {SOUL_PIECES} make Jegg \
             whole. Something lives in there."
        )),
        crate::hud::slot(HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

fn fade_scare(
    time: Res<Time>,
    mut flash: Query<(&mut Scare, &mut BackgroundColor)>,
    mut text: Query<&mut Text, With<ScareText>>,
) {
    for (mut s, mut bg) in &mut flash {
        if s.0 <= 0.0 {
            continue;
        }
        s.0 = (s.0 - time.delta_secs()).max(0.0);
        let a = (s.0 / 2.5).powf(0.6);
        let c = s.1 * a;
        bg.0 = Color::srgba(c.x, c.y, c.z, a * 0.9);
        if s.0 <= 0.0 {
            for mut t in &mut text {
                t.0.clear();
            }
        }
    }
}

/// Every peer: the monster got someone. A jump scare for them, a scream for
/// whoever's near.
#[allow(clippy::too_many_arguments)]
fn on_caught(
    ev: On<Caught>,
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    mut spooked: ResMut<Spooked>,
    audio: Option<ResMut<Assets<AudioSource>>>,
    mut flash: Query<&mut Scare>,
    mut text: Query<(&mut Text, &mut TextColor), With<ScareText>>,
    toast: Option<ResMut<Toast>>,
) {
    let Some(mut audio) = audio else { return };
    let s = sounds(&mut spooked, &mut audio, &mut commands);
    let mine = me.is_some_and(|m| m.0 == ev.player);
    let scream = s.screams[(ev.player as usize) % s.screams.len()].clone();
    if mine {
        commands.spawn((
            AudioPlayer(scream),
            PlaybackSettings::DESPAWN,
            DespawnOnExit(AppState::InGame),
        ));
        for mut f in &mut flash {
            *f = Scare(2.5, Vec3::new(0.35, 0.0, 0.0));
        }
        for (mut t, mut c) in &mut text {
            t.0 = "IT GOT YOU".into();
            c.0 = Color::srgb(0.9, 0.1, 0.1);
        }
    } else {
        crate::spatial::play_at(
            &mut commands,
            &scream,
            ev.at + Vec3::Y * 1.6,
            1.0,
            crate::spatial::Carry::CALL,
        );
        if let Some(mut toast) = toast {
            toast.show(format!(
                "something in the warehouse got player {}",
                ev.player
            ));
        }
    }
}

fn on_delivered(
    ev: On<SoulDelivered>,
    mut commands: Commands,
    mut spooked: ResMut<Spooked>,
    audio: Option<ResMut<Assets<AudioSource>>>,
    toast: Option<ResMut<Toast>>,
) {
    if let Some(mut audio) = audio {
        let s = sounds(&mut spooked, &mut audio, &mut commands);
        crate::spatial::play_at(
            &mut commands,
            &s.chime,
            jegg_at() + Vec3::Y * 1.4,
            0.7,
            crate::spatial::Carry::CALL,
        );
    }
    if let Some(mut toast) = toast {
        toast.show(if ev.total >= SOUL_PIECES {
            "Jegg: \"I'm... whole. I'm whole again. Thank you.\"".to_string()
        } else {
            format!(
                "player {} gave Jegg a soul piece ({}/{SOUL_PIECES}). Jegg: \"Thank you. More...\"",
                ev.player, ev.total
            )
        });
    }
}

#[allow(clippy::too_many_arguments)]
fn show_status(
    me: Option<Res<LocalPlayer>>,
    warehouse: Query<&Warehouse>,
    walkers: Query<(&Player, &Walker)>,
    pieces: Query<&SoulPiece>,
    monsters: Query<&Monster>,
    mut text: Query<&mut Text, With<StatusText>>,
) {
    let Ok(wh) = warehouse.single() else { return };
    let me = me.map(|m| m.0);
    let pos = walkers
        .iter()
        .find(|(p, _)| Some(p.id) == me)
        .map(|(_, w)| w.pos);
    let carrying = pieces
        .iter()
        .any(|s| s.carrier.is_some() && s.carrier == me);
    let players = walkers.iter().count();
    let have = format!("Jegg has {}/{SOUL_PIECES} soul pieces", wh.delivered);
    let want = match pos {
        Some(p) if inside(p) => {
            let mut s = format!("THE WAREHOUSE: find the soul pieces and carry them out. {have}.");
            if carrying {
                s.push_str("\nYou carry a soul piece: get it out to Jegg.");
            }
            if let Some(line) = bathroom::status_line(wh.seed, p) {
                s.push_str(line);
            }
            if monsters
                .iter()
                .any(|m| me.is_some_and(|id| m.mode == MonsterMode::Hunt(id)))
            {
                s.push_str("\nIT'S COMING. RUN.");
            }
            s
        }
        Some(p) if within(p, door_at(), 25.0) || carrying || !wh.votes.is_empty() => {
            if carrying {
                format!("Bring the soul piece to Jegg, beside the warehouse door. {have}.")
            } else if wh.delivered >= SOUL_PIECES {
                "Jegg is whole again. The warehouse is quiet.".to_string()
            } else if wh.seed.is_some() {
                format!("The warehouse is open: click its door to go back in. {have}.")
            } else {
                let agreed = if me.is_some_and(|id| wh.votes.contains(&id)) {
                    " (you agreed; click again to take it back)"
                } else {
                    ""
                };
                format!(
                    "The warehouse: {}/{} agree to go in{agreed}. Everyone must click the door. {have}.",
                    wh.votes.len(),
                    players
                )
            }
        }
        _ => String::new(),
    };
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}
