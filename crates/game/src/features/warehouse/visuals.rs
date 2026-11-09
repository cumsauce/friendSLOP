//! Every peer: the warehouse you see and hear (#130). The building and Jegg
//! outside, the inside built from the replicated seed, the soul pieces and
//! the monster, and the dark (lights, fog, flashlight, drone) while you're in.

use bevy::audio::{AudioSource, Volume};
use bevy::light::NotShadowCaster;

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
    app.init_resource::<Spooked>()
        .init_resource::<Built>()
        .add_observer(click)
        .add_observer(on_caught)
        .add_observer(on_delivered)
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
}

/// Is the local player in the warehouse? (The band goes quiet; see fusion.)
pub fn spooked(s: &Option<Res<Spooked>>) -> bool {
    s.as_ref().is_some_and(|s| s.inside)
}

fn sounds(spooked: &mut Spooked, audio: &mut Assets<AudioSource>) -> Sounds {
    if let Some(s) = &spooked.sounds {
        return s.clone();
    }
    let mut add = |v: Vec<f32>| {
        audio.add(AudioSource {
            bytes: wav_bytes(&v, sound::RATE).into(),
        })
    };
    let s = Sounds {
        drone: add(sound::drone(sound::RATE, 12.0)),
        breath: add(sound::breathing(sound::RATE)),
        screams: (0..3)
            .map(|i| add(sound::scream(sound::RATE, 11 + i)))
            .collect(),
        chime: add(sound::chime(sound::RATE)),
    };
    spooked.sounds = Some(s.clone());
    s
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
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let rust = mat(&mut materials, Color::srgb(0.36, 0.24, 0.18), 0.9);
    let rib = mat(&mut materials, Color::srgb(0.27, 0.2, 0.17), 0.85);
    let roof = mat(&mut materials, Color::srgb(0.2, 0.19, 0.2), 0.9);
    let concrete = mat(&mut materials, Color::srgb(0.42, 0.41, 0.4), 1.0);
    let black = mat(&mut materials, Color::srgb(0.03, 0.03, 0.04), 1.0);
    let boards = mat(&mut materials, Color::srgb(0.45, 0.33, 0.2), 0.9);
    let shutter = mat(&mut materials, Color::srgb(0.3, 0.3, 0.32), 0.6);
    let y0 = outside_floor();
    let c = Vec3::new(OUTSIDE_AT.x, y0, OUTSIDE_AT.y);
    let (sx, sz, hy) = (OUTSIDE_SIZE.x, OUTSIDE_SIZE.y, OUTSIDE_HEIGHT);
    // Plinth (hides the slope), the shell and its roof.
    block(
        &mut commands,
        &cube,
        &concrete,
        c - Vec3::Y * 1.4,
        Vec3::new(sx + 0.8, 3.0, sz + 0.8),
        false,
    );
    block(
        &mut commands,
        &cube,
        &rust,
        c + Vec3::Y * hy / 2.0,
        Vec3::new(sx, hy, sz),
        true,
    );
    for side in [-1.0f32, 1.0] {
        let slab = block(
            &mut commands,
            &cube,
            &roof,
            c + Vec3::new(0.0, hy + 0.9, side * sz / 4.0),
            Vec3::new(sx + 0.8, 0.25, sz / 2.0 + 0.9),
            false,
        );
        commands.entity(slab).insert(
            Transform::from_translation(c + Vec3::new(0.0, hy + 0.9, side * sz / 4.0))
                .with_rotation(Quat::from_rotation_x(side * 0.27))
                .with_scale(Vec3::new(sx + 0.8, 0.25, sz / 2.0 + 0.9)),
        );
    }
    // Corrugated ribs on the long faces.
    let ribs = (sx / 1.1) as i32;
    for i in 0..=ribs {
        let x = c.x - sx / 2.0 + i as f32 * sx / ribs as f32;
        for side in [-1.0f32, 1.0] {
            block(
                &mut commands,
                &cube,
                &rib,
                Vec3::new(x, y0 + hy / 2.0, c.z + side * (sz / 2.0 + 0.05)),
                Vec3::new(0.18, hy, 0.12),
                false,
            );
        }
    }
    // A high row of dead windows, some boarded up.
    for i in 0..6 {
        let x = c.x - sx / 2.0 + 2.0 + i as f32 * (sx - 4.0) / 5.0;
        for side in [-1.0f32, 1.0] {
            let z = c.z + side * (sz / 2.0 + 0.08);
            block(
                &mut commands,
                &cube,
                &black,
                Vec3::new(x, y0 + hy - 1.6, z),
                Vec3::new(1.6, 1.0, 0.1),
                false,
            );
            if crate::art::hash(i, side as i32, 130) < 0.5 {
                let b = block(
                    &mut commands,
                    &cube,
                    &boards,
                    Vec3::new(x, y0 + hy - 1.6, z + side * 0.06),
                    Vec3::new(1.9, 0.18, 0.06),
                    false,
                );
                commands.entity(b).insert(
                    Transform::from_translation(Vec3::new(x, y0 + hy - 1.6, z + side * 0.06))
                        .with_rotation(Quat::from_rotation_z(0.4 - i as f32 * 0.15))
                        .with_scale(Vec3::new(1.9, 0.18, 0.06)),
                );
            }
        }
    }
    // The loading door: a tall rolling shutter, slats and all.
    let d = door_at();
    for k in 0..12 {
        block(
            &mut commands,
            &cube,
            &shutter,
            Vec3::new(d.x, d.y + 0.2 + k as f32 * 0.38, d.z - 0.1),
            Vec3::new(4.2, 0.33, 0.1),
            false,
        );
    }
    block(
        &mut commands,
        &cube,
        &black,
        Vec3::new(d.x, d.y + 4.8, d.z - 0.14),
        Vec3::new(4.8, 0.5, 0.18),
        false,
    );
    commands.spawn((
        OutsideDoor,
        Transform::from_translation(d - Vec3::Z * 0.2),
        Visibility::default(),
        Clickable::at_height(1.6).radius(110.0),
        DespawnOnExit(AppState::InGame),
    ));
    // One sick, buzzing lamp over the door.
    let bulb = glow(&mut materials, Color::srgb(1.0, 0.8, 0.45), 4.0);
    block(
        &mut commands,
        &cube,
        &bulb,
        Vec3::new(d.x, d.y + 5.3, d.z - 0.35),
        Vec3::new(0.5, 0.15, 0.3),
        false,
    );
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
    let barrel = mat(&mut materials, Color::srgb(0.28, 0.32, 0.36), 0.7);
    let drum = meshes.add(Cylinder::new(0.35, 0.95));
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
                Mesh3d(drum.clone()),
                MeshMaterial3d(barrel.clone()),
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
            Clickable::at_height(1.0).radius(70.0),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|p| {
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.18, 0.26, 0.18))),
                MeshMaterial3d(glow(&mut materials, Color::srgb(1.0, 0.85, 0.5), 3.0)),
                Transform::from_xyz(0.45, 0.13, -0.2),
            ));
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
fn build_inside(
    mut commands: Commands,
    mut built: ResMut<Built>,
    warehouse: Query<&Warehouse>,
    parts: Query<Entity, With<InteriorPart>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
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
    let floor = mat(&mut materials, Color::srgb(0.22, 0.22, 0.23), 0.95);
    let wall = mat(&mut materials, Color::srgb(0.3, 0.29, 0.27), 0.95);
    let stain = mat(&mut materials, Color::srgb(0.16, 0.14, 0.12), 1.0);
    let steel = mat(&mut materials, Color::srgb(0.35, 0.3, 0.26), 0.5);
    let shelf = mat(&mut materials, Color::srgb(0.4, 0.33, 0.24), 0.9);
    let cardboard = [
        mat(&mut materials, Color::srgb(0.5, 0.4, 0.27), 1.0),
        mat(&mut materials, Color::srgb(0.42, 0.33, 0.22), 1.0),
        mat(&mut materials, Color::srgb(0.32, 0.3, 0.28), 1.0),
    ];
    let tube = glow(&mut materials, Color::srgb(0.85, 0.95, 1.0), 3.0);
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
    // Floor, ceiling and the outer shell.
    part(
        &mut commands,
        &floor,
        Vec3::new(0.0, y - 0.25, 0.0),
        Vec3::new(gw + 1.0, 0.5, gh + 1.0),
        true,
    );
    part(
        &mut commands,
        &wall,
        Vec3::new(0.0, top + 0.25, 0.0),
        Vec3::new(gw + 1.0, 0.5, gh + 1.0),
        true,
    );
    for (at, size) in [
        (
            Vec3::new(0.0, y + INSIDE_WALL / 2.0, -gh / 2.0 - 0.2),
            Vec3::new(gw + 0.8, INSIDE_WALL, 0.4),
        ),
        (
            Vec3::new(0.0, y + INSIDE_WALL / 2.0, gh / 2.0 + 0.2),
            Vec3::new(gw + 0.8, INSIDE_WALL, 0.4),
        ),
        (
            Vec3::new(-gw / 2.0 - 0.2, y + INSIDE_WALL / 2.0, 0.0),
            Vec3::new(0.4, INSIDE_WALL, gh),
        ),
        (
            Vec3::new(gw / 2.0 + 0.2, y + INSIDE_WALL / 2.0, 0.0),
            Vec3::new(0.4, INSIDE_WALL, gh),
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
        Clickable::at_height(1.4).radius(100.0),
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
    // Walls between cells, wherever the plan says an aisle doesn't go on.
    for z in 0..H {
        for x in 0..W {
            let c = cell_centre(x, z);
            if x + 1 < W && !plan.open(x, z, E) {
                part(
                    &mut commands,
                    &wall,
                    Vec3::new(c.x + CELL / 2.0, y + INSIDE_WALL / 2.0, c.y),
                    Vec3::new(0.3, INSIDE_WALL, CELL + 0.3),
                    true,
                );
            }
            if z + 1 < H && !plan.open(x, z, S) {
                part(
                    &mut commands,
                    &wall,
                    Vec3::new(c.x, y + INSIDE_WALL / 2.0, c.y + CELL / 2.0),
                    Vec3::new(CELL + 0.3, INSIDE_WALL, 0.3),
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
            if mask == 0 {
                for k in 0..4 {
                    let s = 1.4 + rng.roll() * 0.6;
                    let off =
                        Vec3::new((k % 2) as f32 * 2.0 - 1.0, 0.0, (k / 2) as f32 * 2.0 - 1.0)
                            * 1.1;
                    let m = &cardboard[rng.below(3)];
                    part(
                        &mut commands,
                        m,
                        cw + off + Vec3::Y * s / 2.0,
                        Vec3::splat(s),
                        true,
                    );
                    part(
                        &mut commands,
                        m,
                        cw + off + Vec3::Y * (s + 0.5),
                        Vec3::splat(1.0),
                        false,
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
                // A rack along this wall: uprights, three boards, some boxes.
                let along = Vec3::new(dir.z.abs(), 0.0, dir.x.abs());
                let len = CELL - 1.4;
                let back = cw + dir * (CELL / 2.0 - 0.6);
                let depth = dir.abs() * 0.6 + along * len;
                for k in 0..3 {
                    part(
                        &mut commands,
                        &shelf,
                        back + Vec3::Y * (0.3 + k as f32 * 1.0),
                        depth.with_y(0.07),
                        false,
                    );
                    for b in 0..3 {
                        if rng.roll() < 0.4 {
                            continue;
                        }
                        let s = 0.35 + rng.roll() * 0.35;
                        let at = back
                            + along * ((b as f32 - 1.0) * len / 3.2)
                            + Vec3::Y * (0.34 + k as f32 * 1.0 + s / 2.0);
                        part(
                            &mut commands,
                            &cardboard[rng.below(3)],
                            at,
                            Vec3::splat(s),
                            false,
                        );
                    }
                }
                for side in [-1.0f32, 1.0] {
                    part(
                        &mut commands,
                        &steel,
                        back + along * side * len / 2.0 + Vec3::Y * 1.3,
                        Vec3::new(0.08, 2.6, 0.08) + dir.abs() * 0.5,
                        false,
                    );
                }
                // One collider for the whole rack.
                let e = block(
                    &mut commands,
                    &cube,
                    &shelf,
                    back + Vec3::Y * 1.3,
                    depth.with_y(2.6),
                    true,
                );
                commands
                    .entity(e)
                    .insert((InteriorPart, Visibility::Hidden));
            }
            if mask.count_ones() >= 2 && rng.roll() < 0.3 {
                let at = cw + Vec3::new(rng.roll() - 0.5, 0.0, rng.roll() - 0.5) * 1.5;
                part(
                    &mut commands,
                    &shelf,
                    at + Vec3::Y * 0.07,
                    Vec3::new(1.2, 0.14, 1.0),
                    false,
                );
            }
            if lamps < 8 && rng.roll() < 0.22 {
                lamps += 1;
                part(
                    &mut commands,
                    &tube,
                    cw + Vec3::Y * (INSIDE_WALL - 0.3),
                    Vec3::new(1.4, 0.06, 0.12),
                    false,
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

fn crystal(
    p: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let shine = glow(materials, Color::srgb(0.6, 0.85, 1.0), 6.0);
    let cone = meshes.add(Cone::new(0.14, 0.3));
    p.spawn((Crystal, Transform::default(), Visibility::default()))
        .with_children(|c| {
            c.spawn((
                Mesh3d(cone.clone()),
                MeshMaterial3d(shine.clone()),
                Transform::from_xyz(0.0, 0.15, 0.0),
                NotShadowCaster,
            ));
            c.spawn((
                Mesh3d(cone),
                MeshMaterial3d(shine),
                Transform::from_xyz(0.0, -0.15, 0.0)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::PI)),
                NotShadowCaster,
            ));
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
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    for (e, piece) in &new {
        commands
            .entity(e)
            .insert((
                PieceDressed,
                Transform::from_translation(piece.pos),
                Visibility::default(),
                Clickable::at_height(0.0).radius(60.0),
            ))
            .with_children(|p| crystal(p, &mut meshes, &mut materials));
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
            commands
                .entity(e)
                .insert(Clickable::at_height(0.0).radius(60.0));
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
                .with_children(|d| crystal(d, &mut meshes, &mut materials));
            });
        // Its breathing follows it about.
        let Some(audio) = audio.as_deref_mut() else {
            continue;
        };
        let s = sounds(&mut spooked, audio);
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
                let s = sounds(&mut spooked, audio);
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
        ambient.brightness = 12.0;
        for (_, mut sun) in &mut suns {
            sun.illuminance = 0.0;
        }
        commands.entity(camera).insert(DistanceFog {
            color: Color::srgb(0.005, 0.005, 0.008),
            falloff: FogFalloff::Linear {
                start: 2.0,
                end: 24.0,
            },
            ..default()
        });
        commands.spawn((
            Flashlight,
            SpotLight {
                color: Color::srgb(1.0, 0.95, 0.85),
                intensity: 400_000.0,
                range: 22.0,
                outer_angle: 0.42,
                inner_angle: 0.22,
                shadow_maps_enabled: quality.is_some_and(|q| !q.is_low()),
                ..default()
            },
            Transform::from_xyz(0.25, -0.2, 0.0),
            ChildOf(camera),
        ));
        if let Some(mut audio) = audio {
            let s = sounds(&mut spooked, &mut audio);
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

/// A full-screen flash when it gets you, fading out.
#[derive(Component)]
struct Scare(f32);

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
        Scare(0.0),
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
        bg.0 = Color::srgba(0.35 * a, 0.0, 0.0, a * 0.9);
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
    mut text: Query<&mut Text, With<ScareText>>,
    toast: Option<ResMut<Toast>>,
) {
    let Some(mut audio) = audio else { return };
    let s = sounds(&mut spooked, &mut audio);
    let mine = me.is_some_and(|m| m.0 == ev.player);
    let scream = s.screams[(ev.player as usize) % s.screams.len()].clone();
    if mine {
        commands.spawn((
            AudioPlayer(scream),
            PlaybackSettings::DESPAWN,
            DespawnOnExit(AppState::InGame),
        ));
        for mut f in &mut flash {
            f.0 = 2.5;
        }
        for mut t in &mut text {
            t.0 = "IT GOT YOU".into();
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
        let s = sounds(&mut spooked, &mut audio);
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
