//! Ambient life and motion: butterflies, pollen motes, fireflies, falling
//! leaves, bonfire smoke and sparks, birds overhead and wind sway on the
//! scenery props. Purely local visuals (nothing replicated, no colliders);
//! every particle uses shared mesh + material handles with cheap per-frame
//! math, and expired particles are recycled in place (no per-frame spawning).

use bevy::light::NotShadowCaster;
use bevy::prelude::*;

use crate::AppState;
use crate::scenery::BONFIRE_AT;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_ambience)
        .add_systems(
            Update,
            (animate_particles, flap_wings, sway_props).run_if(in_state(AppState::InGame)),
        );
}

/// Marker for scenery props that bend in the wind. Captures the prop's rest
/// rotation the first frame it is seen.
#[derive(Component, Default)]
pub struct Sway {
    base: Option<Quat>,
    phase: f32,
}

#[derive(Clone, Copy)]
enum Kind {
    Butterfly,
    Mote,
    Firefly,
    Leaf,
    Smoke,
    Spark,
    Bird,
}

#[derive(Component)]
struct Particle {
    kind: Kind,
    /// Home / anchor point (butterfly flower, leaf drop point, fire…).
    anchor: Vec3,
    vel: Vec3,
    age: f32,
    life: f32,
    seed: f32,
}

/// Wing pivots on butterflies and birds flap around their local Z.
#[derive(Component)]
struct Wing {
    side: f32,
    speed: f32,
}

/// Tiny deterministic random source (local visuals only).
struct Rng(u32);

impl Rng {
    fn f(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
    fn range(&mut self, a: f32, b: f32) -> f32 {
        a + self.f() * (b - a)
    }
    /// Point in the scenery ring at height `y`.
    fn ring(&mut self, y: f32) -> Vec3 {
        ring_point(self.f(), self.f(), y)
    }
}

/// Map two 0..1 numbers onto the prop ring (radius 12..24) at height `y`.
fn ring_point(u: f32, v: f32, y: f32) -> Vec3 {
    let r = 12.0 + u * 12.0;
    let a = v * std::f32::consts::TAU;
    Vec3::new(a.cos() * r, y, a.sin() * r)
}

/// Stateless 0..1 hash of a float seed.
fn h(x: f32) -> f32 {
    (x.sin() * 43_758.547).fract().abs()
}

// 191 particles in total.
const BUTTERFLIES: usize = 14;
const MOTES: usize = 50;
const FIREFLIES: usize = 30;
const LEAVES: usize = 40;
const SMOKE: usize = 28;
const SPARKS: usize = 24;
const BIRDS: usize = 5;

const WIND: Vec3 = Vec3::new(0.35, 0.0, 0.15);

fn glow(
    materials: &mut Assets<StandardMaterial>,
    c: Color,
    e: LinearRgba,
) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: c,
        emissive: e,
        unlit: true,
        ..default()
    })
}

fn flat(materials: &mut Assets<StandardMaterial>, c: Color) -> Handle<StandardMaterial> {
    materials.add(StandardMaterial {
        base_color: c,
        emissive: c.to_linear() * 0.25,
        double_sided: true,
        cull_mode: None,
        perceptual_roughness: 0.9,
        ..default()
    })
}

fn spawn_particle(
    commands: &mut Commands,
    rng: &mut Rng,
    kind: Kind,
    anchor: Vec3,
    life: f32,
) -> Entity {
    commands
        .spawn((
            Particle {
                kind,
                anchor,
                vel: Vec3::ZERO,
                age: rng.f() * life,
                life,
                seed: rng.f() * 100.0,
            },
            Transform::from_translation(anchor),
            Visibility::default(),
            NotShadowCaster,
            DespawnOnExit(AppState::InGame),
        ))
        .id()
}

fn spawn_ambience(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut rng = Rng(0xA1B1_E5CE);
    let wing_mesh = meshes.add(Cuboid::new(0.22, 0.01, 0.16));
    let dot = meshes.add(Sphere::new(0.035).mesh().uv(6, 4));
    let puff = meshes.add(Sphere::new(0.35).mesh().uv(10, 6));
    let leaf_mesh = meshes.add(Cuboid::new(0.16, 0.01, 0.1));
    let bird_wing = meshes.add(Cuboid::new(0.9, 0.03, 0.3));
    let bird_body = meshes.add(Cuboid::new(0.2, 0.15, 0.6));

    let wing_mats = [
        flat(&mut materials, Color::srgb(1.0, 0.6, 0.1)),
        flat(&mut materials, Color::srgb(0.95, 0.95, 0.4)),
        flat(&mut materials, Color::srgb(0.4, 0.7, 1.0)),
        flat(&mut materials, Color::srgb(1.0, 0.5, 0.8)),
    ];
    let leaf_mats = [
        flat(&mut materials, Color::srgb(0.85, 0.55, 0.15)),
        flat(&mut materials, Color::srgb(0.7, 0.3, 0.1)),
        flat(&mut materials, Color::srgb(0.6, 0.7, 0.2)),
    ];
    let mote_mat = glow(
        &mut materials,
        Color::srgb(1.0, 1.0, 0.85),
        LinearRgba::rgb(1.5, 1.5, 1.2),
    );
    let firefly_mat = glow(
        &mut materials,
        Color::srgb(0.8, 1.0, 0.3),
        LinearRgba::rgb(4.0, 6.0, 1.0),
    );
    let spark_mat = glow(
        &mut materials,
        Color::srgb(1.0, 0.6, 0.2),
        LinearRgba::rgb(8.0, 3.0, 0.5),
    );
    let smoke_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.45, 0.45, 0.48, 0.35),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let bird_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.12, 0.15),
        unlit: true,
        ..default()
    });

    for i in 0..BUTTERFLIES {
        let anchor = rng.ring(0.6);
        let e = spawn_particle(&mut commands, &mut rng, Kind::Butterfly, anchor, 12.0);
        let mat = wing_mats[i % wing_mats.len()].clone();
        commands.entity(e).with_children(|p| {
            for side in [-1.0, 1.0] {
                p.spawn((
                    Transform::default(),
                    Visibility::default(),
                    Wing { side, speed: 18.0 },
                ))
                .with_child((
                    Mesh3d(wing_mesh.clone()),
                    MeshMaterial3d(mat.clone()),
                    Transform::from_xyz(side * 0.11, 0.0, 0.0),
                    NotShadowCaster,
                ));
            }
        });
    }
    for _ in 0..MOTES {
        let y = rng.range(0.3, 3.0);
        let anchor = rng.ring(y);
        let e = spawn_particle(&mut commands, &mut rng, Kind::Mote, anchor, 1.0);
        commands
            .entity(e)
            .insert((Mesh3d(dot.clone()), MeshMaterial3d(mote_mat.clone())));
    }
    for _ in 0..FIREFLIES {
        let anchor = rng.ring(0.8);
        let e = spawn_particle(&mut commands, &mut rng, Kind::Firefly, anchor, 1.0);
        commands
            .entity(e)
            .insert((Mesh3d(dot.clone()), MeshMaterial3d(firefly_mat.clone())));
    }
    for i in 0..LEAVES {
        let anchor = rng.ring(4.0);
        let life = rng.range(5.0, 9.0);
        let e = spawn_particle(&mut commands, &mut rng, Kind::Leaf, anchor, life);
        commands.entity(e).insert((
            Mesh3d(leaf_mesh.clone()),
            MeshMaterial3d(leaf_mats[i % leaf_mats.len()].clone()),
        ));
    }
    for _ in 0..SMOKE {
        let life = rng.range(3.5, 5.5);
        let e = spawn_particle(&mut commands, &mut rng, Kind::Smoke, BONFIRE_AT, life);
        commands
            .entity(e)
            .insert((Mesh3d(puff.clone()), MeshMaterial3d(smoke_mat.clone())));
    }
    for _ in 0..SPARKS {
        let life = rng.range(0.8, 1.6);
        let e = spawn_particle(&mut commands, &mut rng, Kind::Spark, BONFIRE_AT, life);
        commands
            .entity(e)
            .insert((Mesh3d(dot.clone()), MeshMaterial3d(spark_mat.clone())));
    }
    for _ in 0..BIRDS {
        let life = rng.range(14.0, 22.0);
        let e = spawn_particle(&mut commands, &mut rng, Kind::Bird, Vec3::ZERO, life);
        commands.entity(e).with_children(|p| {
            p.spawn((
                Mesh3d(bird_body.clone()),
                MeshMaterial3d(bird_mat.clone()),
                Transform::default(),
                NotShadowCaster,
            ));
            for side in [-1.0, 1.0] {
                p.spawn((
                    Transform::default(),
                    Visibility::default(),
                    Wing { side, speed: 6.0 },
                ))
                .with_child((
                    Mesh3d(bird_wing.clone()),
                    MeshMaterial3d(bird_mat.clone()),
                    Transform::from_xyz(side * 0.45, 0.0, 0.0),
                    NotShadowCaster,
                ));
            }
        });
    }
}

fn flap_wings(time: Res<Time>, mut wings: Query<(&Wing, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (wing, mut tf) in &mut wings {
        let flap = (t * wing.speed).sin() * 0.9;
        tf.rotation = Quat::from_rotation_z(flap * wing.side);
    }
}

fn animate_particles(time: Res<Time>, mut particles: Query<(&mut Particle, &mut Transform)>) {
    let dt = time.delta_secs().min(0.1);
    let t = time.elapsed_secs();

    for (mut p, mut tf) in &mut particles {
        p.age += dt;
        let s = p.seed;
        match p.kind {
            Kind::Butterfly => {
                // Lazy loops around a flower; hop to another spot now and then.
                if p.age > p.life {
                    p.age = 0.0;
                    p.seed += 1.0;
                    p.anchor = ring_point(h(p.seed), h(p.seed * 1.7), 0.6);
                }
                let k = t * 0.7 + s;
                let target = p.anchor
                    + Vec3::new(
                        k.sin() * 1.5,
                        (k * 2.3).sin().abs() * 0.6,
                        (k * 2.0).sin() * 0.9,
                    );
                let to = target - tf.translation;
                tf.translation += to * (dt * 1.5).min(1.0);
                if to.length_squared() > 1e-4 {
                    tf.rotation = Quat::from_rotation_y(to.x.atan2(to.z));
                }
            }
            Kind::Mote => {
                let k = t * 0.3 + s;
                tf.translation = p.anchor
                    + Vec3::new(
                        k.sin() * 0.8 + (t * 0.2 + s).sin() * 2.0,
                        (k * 1.3).sin() * 0.4,
                        (k * 0.8).cos() * 0.8,
                    );
            }
            Kind::Firefly => {
                let k = t * 0.5 + s;
                tf.translation = p.anchor
                    + Vec3::new(k.sin() * 1.2, (k * 1.7).sin() * 0.5, (k * 1.3).cos() * 1.2);
                // Soft blinking pulse.
                let blink = ((t * 1.3 + s).sin() * 0.5 + 0.5).powi(2);
                tf.scale = Vec3::splat(0.4 + blink * 1.4);
            }
            Kind::Leaf => {
                if p.age > p.life {
                    p.age = 0.0;
                    p.seed += 1.0;
                    p.anchor = ring_point(h(p.seed), h(p.seed * 2.1), 3.0 + h(p.seed * 3.3) * 3.0);
                }
                let y = (p.anchor.y - p.age * 0.7).max(0.02);
                let sway = (p.age * 2.0 + s).sin();
                tf.translation = Vec3::new(
                    p.anchor.x + sway * 0.5 + WIND.x * p.age,
                    y,
                    p.anchor.z + WIND.z * p.age,
                );
                if y > 0.03 {
                    tf.rotation = Quat::from_euler(
                        EulerRot::XYZ,
                        sway * 0.9,
                        p.age * 1.5 + s,
                        (p.age * 3.0).cos() * 0.6,
                    );
                }
            }
            Kind::Smoke => {
                if p.age > p.life {
                    p.age = 0.0;
                    p.seed += 1.0;
                }
                let f = p.age / p.life;
                let jitter = Vec3::new(h(p.seed) - 0.5, 0.0, h(p.seed * 1.9) - 0.5) * 0.5;
                tf.translation =
                    p.anchor + Vec3::Y * (1.0 + p.age * 1.2) + jitter + WIND * p.age * 1.5;
                // Billow out, then shrink away as it thins.
                tf.scale = Vec3::splat((f * 3.0).min(1.0) * (1.0 - f) * 2.2 + 0.2);
            }
            Kind::Spark => {
                if p.age > p.life {
                    p.age = 0.0;
                    p.seed += 1.0;
                    p.vel = Vec3::new(
                        (h(p.seed) - 0.5) * 1.2,
                        2.0 + h(p.seed * 1.3) * 2.0,
                        (h(p.seed * 2.7) - 0.5) * 1.2,
                    );
                    tf.translation = p.anchor + Vec3::Y * 0.6;
                }
                let drag = (WIND - p.vel * 0.3) * dt;
                p.vel += drag;
                tf.translation += p.vel * dt
                    + Vec3::new((t * 9.0 + s).sin(), 0.0, (t * 8.0 + s).cos()) * dt * 0.4;
                tf.scale = Vec3::splat(1.0 - p.age / p.life);
            }
            Kind::Bird => {
                // Cross the sky along a random chord, then wait off-stage.
                if p.age > p.life {
                    p.age = 0.0;
                    p.seed += 1.0;
                }
                let a = h(p.seed) * std::f32::consts::TAU;
                let dir = Vec3::new(a.cos(), 0.0, a.sin());
                let side = Vec3::new(-dir.z, 0.0, dir.x) * (h(p.seed * 1.4) - 0.5) * 30.0;
                let height = 16.0 + h(p.seed * 2.2) * 8.0;
                let travel = p.age * 7.0 - 60.0;
                tf.translation = side + dir * travel + Vec3::Y * (height + (p.age * 0.8).sin());
                tf.rotation = Quat::from_rotation_y(dir.x.atan2(dir.z));
            }
        }
    }
}

fn sway_props(time: Res<Time>, mut props: Query<(&mut Sway, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (mut sway, mut tf) in &mut props {
        let base = match sway.base {
            Some(b) => b,
            None => {
                sway.phase = tf.translation.x * 0.3 + tf.translation.z * 0.2;
                sway.base = Some(tf.rotation);
                tf.rotation
            }
        };
        let k = t * 1.1 + sway.phase;
        let bend = k.sin() * 0.025 + (k * 2.7).sin() * 0.008;
        tf.rotation = Quat::from_rotation_x(bend * 0.4) * Quat::from_rotation_z(bend) * base;
    }
}
