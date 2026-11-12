//! Dressing for the map's focal points so they read as cosy landmarks: the
//! pond (ripples, lily pads, reeds, rocks, a little jetty and two bobbing
//! ducks), the campfire (stone ring, log seats, ember glow) and strings of
//! lantern bulbs along the paths between them. Purely local visuals, spawned
//! identically on every peer; nothing here is replicated or collides. The
//! jetty, log seats, light posts, reeds and lily pads are modeler models in
//! `assets/models/scenery` (sources in `crates/game/.modeler/`); a missing
//! file just draws nothing.

use bevy::gltf::GltfAssetLabel;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

use crate::AppState;
use crate::art;
/// The bonfire on the floor plane.
const BONFIRE_AT: Vec3 = Vec3::new(
    crate::places::BONFIRE_AT.x,
    0.0,
    crate::places::BONFIRE_AT.y,
);
use crate::terrain::POND_AT;

pub fn plugin(app: &mut App) {
    app.add_systems(
        OnEnter(AppState::InGame),
        (dress_pond, dress_campfire, string_lights),
    )
    .add_systems(
        Update,
        (ripple, bob, paddle, glow_embers).run_if(in_state(AppState::InGame)),
    )
    .add_observer(no_model_shadows);
}

/// A small model whose meshes cast no shadow once its scene has spawned.
#[derive(Component)]
struct NoShadows;

fn no_model_shadows(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    small: Query<(), With<NoShadows>>,
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

/// The scene root for one `assets/models/scenery` model.
fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/scenery/{file}.gltf"))),
    )
}

/// Radius of the open water (matches the pond surface in `terrain.rs`).
const WATER_R: f32 = 3.3;
/// Height of the water surface.
const WATER_Y: f32 = 0.03;

/// An expanding, fading ring on the water.
#[derive(Component)]
struct Ripple {
    phase: f32,
    material: Handle<StandardMaterial>,
}

/// Gently rises and falls around `y` (lily pads, ducks).
#[derive(Component)]
struct Bob {
    y: f32,
    phase: f32,
}

/// A duck paddling a slow circle round the pond.
#[derive(Component)]
struct Duck {
    radius: f32,
    speed: f32,
    phase: f32,
}

/// The glowing coals under the campfire.
#[derive(Component)]
struct Embers(Handle<StandardMaterial>);

/// Small, cheap scenery: no shadow, despawned with the game.
fn small() -> (NotShadowCaster, DespawnOnExit<AppState>) {
    (NotShadowCaster, DespawnOnExit(AppState::InGame))
}

/// [`small`] for a model: its meshes cast no shadow once spawned.
fn small_model() -> (NoShadows, DespawnOnExit<AppState>) {
    (NoShadows, DespawnOnExit(AppState::InGame))
}

fn flat(x: f32, y: f32, z: f32) -> Transform {
    Transform::from_xyz(x, y, z).with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2))
}

fn glow(
    materials: &mut Assets<StandardMaterial>,
    c: Color,
    strength: f32,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: c,
        emissive: c.to_linear() * strength,
        ..default()
    })
}

/// Where the jetty points from: the side facing the middle of the map.
fn jetty_dir() -> Vec2 {
    (-POND_AT).normalize_or(Vec2::NEG_X)
}

fn dress_pond(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let c = POND_AT;
    let jd = jetty_dir();
    // Angle of the jetty around the pond, so the reeds can leave it room.
    let jetty_angle = jd.y.atan2(jd.x);
    let around = |a: f32, r: f32| c + Vec2::new(a.cos(), a.sin()) * r;

    // Ripples: a few rings that swell and fade, staggered.
    let ring = meshes.add(Annulus::new(0.92, 1.0));
    for (i, p) in [
        Vec2::new(0.8, 0.6),
        Vec2::new(-1.1, -0.4),
        Vec2::new(0.3, -1.4),
    ]
    .into_iter()
    .enumerate()
    {
        let material = materials.add(StandardMaterial {
            base_color: Color::srgba(0.9, 0.97, 1.0, 0.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            ..default()
        });
        commands.spawn((
            Mesh3d(ring.clone()),
            MeshMaterial3d(material.clone()),
            flat(c.x + p.x, WATER_Y + 0.015, c.y + p.y),
            Ripple {
                phase: i as f32 * 1.3,
                material,
            },
            small(),
        ));
    }

    // Lily pads (a notched disc), every third with a pink flower.
    for (i, (a, r)) in [
        (2.3, 2.2),
        (2.6, 1.6),
        (3.4, 2.5),
        (4.1, 2.0),
        (4.6, 2.7),
        (5.4, 1.4),
        (0.6, 2.6),
    ]
    .into_iter()
    .enumerate()
    {
        let p = around(jetty_angle + a, r);
        // High enough that bobbing never dips it into the ripples (z-fighting).
        let y = WATER_Y + 0.035;
        commands.spawn((
            model(
                &assets,
                if i % 3 == 0 {
                    "lily_pad_bloom"
                } else {
                    "lily_pad"
                },
            ),
            Transform::from_xyz(p.x, y, p.y).with_rotation(Quat::from_rotation_y(a * 3.7)),
            Bob {
                y,
                phase: i as f32 * 0.9,
            },
            small_model(),
        ));
    }

    // Reeds and cattails round the far bank, swaying in the breeze: clumps
    // about 1 m tall, scaled to each one's height, every other with cattails.
    for i in 0..26 {
        let k = i as f32;
        // Skip a wedge either side of the jetty.
        let a = jetty_angle
            + 0.9
            + k * (std::f32::consts::TAU - 1.8) / 26.0
            + art::hash(i, 3, 7) * 0.15;
        let r = WATER_R + 0.05 + art::hash(i, 1, 7) * 0.45;
        let p = around(a, r);
        let h = 0.7 + art::hash(i, 2, 7) * 0.6;
        let lean = Quat::from_rotation_z((art::hash(i, 4, 7) - 0.5) * 0.25);
        commands.spawn((
            model(&assets, if i % 2 == 0 { "reeds_a" } else { "reeds_b" }),
            Transform::from_xyz(p.x, 0.0, p.y)
                .with_rotation(lean)
                .with_scale(Vec3::splat(h)),
            art::Wiggle::sway(lean, Vec3::X, 0.06, 1.1, k * 0.7),
            small_model(),
        ));
    }

    // A couple of mossy rocks on the bank.
    let stone = art::soft(&mut materials, Color::srgb(0.55, 0.55, 0.52));
    for (a, r, s) in [(1.2, 3.6, 0.35), (1.45, 3.9, 0.22), (-1.4, 3.7, 0.3)] {
        let p = around(jetty_angle + a, r);
        commands.spawn((
            Mesh3d(art::chunk(&mut meshes, s)),
            MeshMaterial3d(stone.clone()),
            Transform::from_xyz(p.x, s * 0.35, p.y)
                .with_rotation(Quat::from_rotation_y(a * 2.0))
                .with_scale(Vec3::new(1.2, 0.75, 1.0)),
            DespawnOnExit(AppState::InGame),
        ));
    }

    // The jetty: planks on two stringers, four posts, a lantern at the end
    // (one model, along its X), and the lantern's light.
    let (start, end) = (WATER_R + 1.0, 1.6);
    let mid = c + jd * (start + end) / 2.0;
    let len = start - end;
    let yaw = Quat::from_rotation_y(-jetty_angle);
    commands
        .spawn((
            model(&assets, "jetty"),
            Transform::from_xyz(mid.x, 0.0, mid.y).with_rotation(yaw),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|j| {
            // Lantern post at the water end.
            let lx = -len / 2.0 + 0.2;
            j.spawn((
                PointLight {
                    color: Color::srgb(1.0, 0.72, 0.4),
                    intensity: 9000.0,
                    range: 5.0,
                    radius: 0.1,
                    shadow_maps_enabled: false,
                    ..default()
                },
                Transform::from_xyz(lx, 1.55, 0.42),
            ));
        });

    // Two ducks paddling round.
    let white = art::soft(&mut materials, Color::srgb(0.97, 0.96, 0.9));
    let green = art::glossy(&mut materials, Color::srgb(0.1, 0.4, 0.2));
    let orange = art::soft(&mut materials, Color::srgb(1.0, 0.6, 0.1));
    let eye = art::glossy(&mut materials, Color::srgb(0.03, 0.03, 0.03));
    let body = meshes.add(Sphere::new(0.16));
    let head = meshes.add(Sphere::new(0.09));
    let beak = meshes.add(Cuboid::new(0.08, 0.025, 0.06));
    let dot = meshes.add(Sphere::new(0.015));
    for (i, (radius, speed)) in [(1.9_f32, 0.12_f32), (1.2, -0.09)].into_iter().enumerate() {
        let head_mat = if i == 0 { green.clone() } else { white.clone() };
        commands
            .spawn((
                Transform::from_xyz(c.x, WATER_Y, c.y),
                Visibility::default(),
                Duck {
                    radius,
                    speed,
                    phase: i as f32 * 2.8 + 1.0,
                },
                Bob {
                    y: WATER_Y,
                    phase: i as f32 * 1.7,
                },
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|d| {
                // The duck faces +X; the paddle system yaws it along its path.
                d.spawn((
                    Mesh3d(body.clone()),
                    MeshMaterial3d(white.clone()),
                    Transform::from_xyz(0.0, 0.06, 0.0).with_scale(Vec3::new(1.4, 0.8, 1.0)),
                    NotShadowCaster,
                ));
                d.spawn((
                    Mesh3d(body.clone()),
                    MeshMaterial3d(white.clone()),
                    Transform::from_xyz(-0.18, 0.12, 0.0).with_scale(Vec3::new(0.35, 0.35, 0.35)),
                    NotShadowCaster,
                ));
                d.spawn((
                    Mesh3d(head.clone()),
                    MeshMaterial3d(head_mat),
                    Transform::from_xyz(0.15, 0.24, 0.0),
                    NotShadowCaster,
                ));
                d.spawn((
                    Mesh3d(beak.clone()),
                    MeshMaterial3d(orange.clone()),
                    Transform::from_xyz(0.25, 0.22, 0.0),
                    NotShadowCaster,
                ));
                for z in [-0.06, 0.06] {
                    d.spawn((
                        Mesh3d(dot.clone()),
                        MeshMaterial3d(eye.clone()),
                        Transform::from_xyz(0.2, 0.27, z),
                        NotShadowCaster,
                    ));
                }
            });
    }
}

fn dress_campfire(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let c = BONFIRE_AT.xz();
    // Stone ring.
    let stone = art::soft(&mut materials, Color::srgb(0.5, 0.48, 0.46));
    let sooty = art::soft(&mut materials, Color::srgb(0.3, 0.28, 0.27));
    let rock = art::chunk(&mut meshes, 0.2);
    for i in 0..12 {
        let a = i as f32 * std::f32::consts::TAU / 12.0;
        let p = c + Vec2::new(a.cos(), a.sin()) * 1.05;
        let s = 0.8 + art::hash(i, 0, 5) * 0.5;
        commands.spawn((
            Mesh3d(rock.clone()),
            MeshMaterial3d(if i % 3 == 0 {
                sooty.clone()
            } else {
                stone.clone()
            }),
            Transform::from_xyz(p.x, 0.06, p.y)
                .with_rotation(Quat::from_rotation_y(a * 3.1))
                .with_scale(Vec3::new(s, s * 0.7, s)),
            small(),
        ));
    }
    // Ember bed: a glowing disc that pulses with the fire.
    let ember_mat = glow(&mut materials, Color::srgb(1.0, 0.35, 0.08), 4.0);
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(0.8))),
        MeshMaterial3d(ember_mat.clone()),
        flat(c.x, 0.025, c.y),
        Embers(ember_mat),
        small(),
    ));
    // Log seats round the fire, each facing it (the model lies along its Z,
    // origin at the log's centre).
    for i in 0..4 {
        let a = 0.4 + i as f32 * std::f32::consts::FRAC_PI_2;
        let p = c + Vec2::new(a.cos(), a.sin()) * 2.3;
        // Lying along the tangent of the circle.
        let yaw = Quat::from_rotation_y(-a);
        commands.spawn((
            model(&assets, "log_seat"),
            Transform::from_xyz(p.x, 0.2, p.y).with_rotation(yaw),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Lantern posts along the paths from the middle to the campfire and the
/// pond, strung with emissive bulbs (no lights of their own).
fn string_lights(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let wire_mat = art::soft(&mut materials, Color::srgb(0.12, 0.1, 0.08));
    let bulbs = [
        glow(&mut materials, Color::srgb(1.0, 0.8, 0.45), 8.0),
        glow(&mut materials, Color::srgb(1.0, 0.55, 0.35), 8.0),
        glow(&mut materials, Color::srgb(0.95, 0.9, 0.6), 8.0),
    ];
    let bulb = meshes.add(Sphere::new(0.05));
    let wire = meshes.add(Cylinder::new(0.008, 1.0));
    const TOP: f32 = 2.3;
    for (a, b, side) in [
        (Vec2::ZERO, BONFIRE_AT.xz(), 1.0),
        (Vec2::ZERO, POND_AT, -1.0),
    ] {
        let dir = (b - a).normalize_or(Vec2::X);
        let perp = Vec2::new(-dir.y, dir.x) * side * 1.3;
        let posts: Vec<Vec2> = [0.3, 0.52, 0.74]
            .into_iter()
            .map(|t| a.lerp(b, t) + perp)
            .collect();
        for p in &posts {
            // Pole and glowing cap (one model); the wire hangs from TOP.
            commands.spawn((
                model(&assets, "light_post"),
                Transform::from_xyz(p.x, 0.0, p.y),
                DespawnOnExit(AppState::InGame),
            ));
        }
        for (k, w) in posts.windows(2).enumerate() {
            let (p, q) = (w[0], w[1]);
            let point = |t: f32| {
                let sag = 4.0 * t * (1.0 - t) * 0.45;
                let xz = p.lerp(q, t);
                Vec3::new(xz.x, TOP - sag, xz.y)
            };
            const SEGS: usize = 8;
            for s in 0..SEGS {
                let (u, v) = (
                    point(s as f32 / SEGS as f32),
                    point((s + 1) as f32 / SEGS as f32),
                );
                let d = v - u;
                commands.spawn((
                    Mesh3d(wire.clone()),
                    MeshMaterial3d(wire_mat.clone()),
                    Transform::from_translation((u + v) / 2.0)
                        .with_rotation(Quat::from_rotation_arc(Vec3::Y, d.normalize_or(Vec3::Y)))
                        .with_scale(Vec3::new(1.0, d.length(), 1.0)),
                    small(),
                ));
                if s > 0 {
                    commands.spawn((
                        Mesh3d(bulb.clone()),
                        MeshMaterial3d(bulbs[(s + k) % bulbs.len()].clone()),
                        Transform::from_translation(u - Vec3::Y * 0.06),
                        small(),
                    ));
                }
            }
        }
    }
}

fn ripple(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut q: Query<(&Ripple, &mut Transform)>,
) {
    const PERIOD: f32 = 4.0;
    let t = time.elapsed_secs();
    for (r, mut tr) in &mut q {
        let age = ((t + r.phase) / PERIOD).fract();
        let s = 0.15 + age * 1.3;
        tr.scale = Vec3::new(s, s, 1.0);
        // Each material edit re-uploads it to the GPU, so only touch it when
        // the fade has visibly moved (~8 times a second per ripple, #151).
        let alpha = ((1.0 - age) * age * 1.6 * 64.0).round() / 64.0;
        if materials
            .get(&r.material)
            .is_some_and(|m| m.base_color.alpha() != alpha)
            && let Some(mut m) = materials.get_mut(&r.material)
        {
            m.base_color.set_alpha(alpha);
        }
    }
}

fn bob(time: Res<Time>, mut q: Query<(&Bob, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (b, mut tr) in &mut q {
        tr.translation.y = b.y + (t * 1.8 + b.phase).sin() * 0.012;
    }
}

fn paddle(time: Res<Time>, mut q: Query<(&Duck, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (d, mut tr) in &mut q {
        let a = t * d.speed + d.phase;
        let p = POND_AT + Vec2::new(a.cos(), a.sin()) * d.radius;
        tr.translation.x = p.x;
        tr.translation.z = p.y;
        // Heading along the circle (the duck's nose is +X).
        let heading = Vec2::new(-a.sin(), a.cos()) * d.speed.signum();
        tr.rotation = Quat::from_rotation_y(-heading.y.atan2(heading.x));
    }
}

fn glow_embers(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<&Embers>,
    mut last: Local<f32>,
) {
    // A material edit re-uploads it to the GPU: flicker at 12 Hz, not every
    // frame (#151).
    let t = time.elapsed_secs();
    if (t - *last).abs() < 1.0 / 12.0 {
        return;
    }
    *last = t;
    for e in &q {
        let k = 3.5 + (t * 2.3).sin() * 0.8 + (t * 5.1).sin() * 0.5;
        if let Some(mut m) = materials.get_mut(&e.0) {
            m.emissive = LinearRgba::rgb(1.0, 0.35, 0.08) * k;
        }
    }
}
