//! Pigeons. From issue #355 by catherine: "research boids and boid rules for
//! birds in games and performance optimizations and use them to make pigeons
//! work. every now and then they poop and rarely they will all poop st once
//! and cause property damage with the amount".
//!
//! Flocks (`data/side/pigeons.ron`) wheel over New York's streets and inside
//! its buildings (the Oculus, Penn Station's NJ Transit side). Each pigeon is
//! a boid (Reynolds' three rules: separation, alignment, cohesion), plus a
//! pull back into its flock's box and a little wander; speeds are clamped
//! between a glide and a dash, and the `pigeon` model flaps when it climbs or
//! speeds up and glides otherwise.
//!
//! Made cheap the usual ways: neighbours come from a uniform grid of
//! view-radius cells (each bird looks at its own cell and the 26 round it,
//! not the whole flock), the rules are re-thought only a few times a second
//! (staggered across birds) while the motion integrates every frame, and a
//! flock far from the camera is hidden and doesn't think at all.
//!
//! Now and then a pigeon poops: a dropping falls from it and leaves a white
//! splat on the ground (a small pool, oldest reused); land one on you and you
//! hear about it. Rarely, every pigeon in the city poops at once: a storm a few
//! seconds long, timed from the session clock so every peer gets it at the
//! same moment, with a property damage bill on the HUD. It's all local looks:
//! nothing here changes gameplay or needs the network.

use bevy::prelude::*;
use serde::Deserialize;

use crate::AppState;
use crate::features::fps::Walker;
use crate::player::{LocalPlayer, Player};

/// One flock: a box it keeps to and how many birds.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Flock {
    /// Where (for the HUD and tests).
    pub name: String,
    /// The box's middle and half sizes (m).
    pub center: (f32, f32, f32),
    pub half: (f32, f32, f32),
    /// The ground under it (y) where droppings land.
    pub ground: f32,
    pub count: usize,
}

/// Tuning and writing: `assets/data/side/pigeons.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    pub flocks: Vec<Flock>,
    /// Slowest and fastest a pigeon flies (m/s).
    pub min_speed: f32,
    pub max_speed: f32,
    /// How far a pigeon sees its flockmates (m), and how close is too close.
    pub view: f32,
    pub too_close: f32,
    /// The rules' weights (steering accelerations, m/s²).
    pub separation: f32,
    pub alignment: f32,
    pub cohesion: f32,
    pub bounds: f32,
    pub wander: f32,
    /// How often each bird re-thinks the rules (per second).
    pub think_hz: f32,
    /// Flocks farther than this (m) from the camera are hidden and frozen.
    pub draw_range: f32,
    /// Chance a pigeon poops, per second.
    pub poop_chance: f32,
    /// How long a splat stays (s) and how many there can be at once.
    pub splat_secs: f32,
    pub max_splats: usize,
    /// A dropping landing this close (m) to you hits you.
    pub hit_radius: f32,
    /// One storm window every this many seconds, with this chance of a storm in it.
    pub storm_every: f32,
    pub storm_chance: f32,
    /// How long a storm lasts (s) and how often each pigeon poops in one (per second).
    pub storm_secs: f32,
    pub storm_rate: f32,
    /// Property damage per pigeon in a storm (dollars, for the bill).
    pub damage_per_pigeon: u32,
    /// Shown when a dropping hits you, and when a storm starts (`{damage}` is the bill).
    pub hit_line: String,
    pub storm_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/pigeons.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

const MODEL: &str = "models/pigeon.gltf";

// ------------------------------------------------------------------ boids (pure)

/// One bird.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Boid {
    pub pos: Vec3,
    pub vel: Vec3,
    /// The steering it settled on the last time it thought.
    pub steer: Vec3,
}

/// A flock's box (min, max corners).
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}

impl Bounds {
    pub fn of(f: &Flock) -> Self {
        let c = Vec3::from(f.center);
        let h = Vec3::from(f.half);
        Self {
            min: c - h,
            max: c + h,
        }
    }
}

/// A uniform grid of `cell`-sized cells over a flock: who's in which cell.
pub struct Grid {
    cell: f32,
    cells: std::collections::HashMap<IVec3, Vec<usize>>,
}

impl Grid {
    pub fn build(boids: &[Boid], cell: f32) -> Self {
        let cell = cell.max(0.1);
        let mut cells: std::collections::HashMap<IVec3, Vec<usize>> = Default::default();
        for (i, b) in boids.iter().enumerate() {
            cells
                .entry((b.pos / cell).floor().as_ivec3())
                .or_default()
                .push(i);
        }
        Self { cell, cells }
    }

    /// Everyone in `p`'s cell and the 26 round it (may include birds a bit
    /// past `cell`: callers check the distance).
    pub fn near(&self, p: Vec3) -> impl Iterator<Item = usize> + '_ {
        let c = (p / self.cell).floor().as_ivec3();
        (-1..=1).flat_map(move |x| {
            (-1..=1).flat_map(move |y| {
                (-1..=1).flat_map(move |z| {
                    self.cells
                        .get(&(c + IVec3::new(x, y, z)))
                        .into_iter()
                        .flatten()
                        .copied()
                })
            })
        })
    }
}

/// Reynolds' rules for bird `i`: separation from the too-close, alignment
/// with and cohesion towards the ones it sees, a push back into the box, and
/// a wander (`jitter`, a unit-ish vector the caller makes up).
pub fn steer(i: usize, boids: &[Boid], grid: &Grid, b: Bounds, t: &Tuning, jitter: Vec3) -> Vec3 {
    let me = boids[i];
    let (mut sep, mut vel, mut pos, mut n) = (Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, 0.0);
    for j in grid.near(me.pos) {
        if j == i {
            continue;
        }
        let d = me.pos - boids[j].pos;
        let dist = d.length();
        if dist > t.view || dist < 1e-4 {
            continue;
        }
        if dist < t.too_close {
            sep += d / (dist * dist);
        }
        vel += boids[j].vel;
        pos += boids[j].pos;
        n += 1.0;
    }
    let mut a = sep.normalize_or_zero() * t.separation;
    if n > 0.0 {
        a += (vel / n - me.vel).normalize_or_zero() * t.alignment;
        a += (pos / n - me.pos).normalize_or_zero() * t.cohesion;
    }
    // Back into the box: harder the farther out.
    let margin = 2.0;
    let mut back = Vec3::ZERO;
    for k in 0..3 {
        if me.pos[k] < b.min[k] + margin {
            back[k] += (b.min[k] + margin - me.pos[k]).min(margin) / margin;
        } else if me.pos[k] > b.max[k] - margin {
            back[k] -= (me.pos[k] - b.max[k] + margin).min(margin) / margin;
        }
    }
    a + back * t.bounds + jitter * t.wander
}

/// Move a bird `dt` along its steering, its speed held between the limits.
pub fn fly(b: Boid, dt: f32, t: &Tuning) -> Boid {
    let mut vel = b.vel + b.steer * dt;
    let s = vel.length();
    if s < 1e-4 {
        vel = Vec3::X * t.min_speed;
    } else {
        vel *= s.clamp(t.min_speed, t.max_speed) / s;
    }
    Boid {
        pos: b.pos + vel * dt,
        vel,
        steer: b.steer,
    }
}

/// Where a dropping let go at `pos` with velocity `vel` lands on `ground`.
pub fn lands(pos: Vec3, vel: Vec3, ground: f32) -> Vec3 {
    let g = 9.81;
    let h = (pos.y - ground).max(0.0);
    // y(t) = h + vy t - g t²/2 = 0
    let t = (vel.y + (vel.y * vel.y + 2.0 * g * h).sqrt()) / g;
    Vec3::new(pos.x + vel.x * t, ground, pos.z + vel.z * t)
}

fn hash(k: u64) -> f32 {
    let mut x = k.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x355;
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// The storm (if any) at session time `secs`: which window it's in, and how
/// far into it. Every peer gets the same answer from the shared clock.
pub fn storm_at(secs: f32, t: &Tuning) -> Option<(u64, f32)> {
    if t.storm_every <= 0.0 || secs < 0.0 {
        return None;
    }
    let k = (secs / t.storm_every).floor() as u64;
    // The first window is always quiet (nobody's even in the city yet).
    if k == 0 || hash(k) >= t.storm_chance {
        return None;
    }
    let into = secs - k as f32 * t.storm_every;
    (into < t.storm_secs).then_some((k, into))
}

/// The storm's bill: every pigeon there is, times the damage each does.
pub fn damage(t: &Tuning) -> u32 {
    let birds: usize = t.flocks.iter().map(|f| f.count).sum();
    birds as u32 * t.damage_per_pigeon
}

// ------------------------------------------------------------------ every peer (all local)

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn)
        .add_systems(
            Update,
            (think, fly_all, droppings, storm_line).run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct Pigeon {
    flock: usize,
    boid: Boid,
    /// Seconds until it thinks again.
    think_in: f32,
    flapping: bool,
    seed: u64,
}

#[derive(Component)]
struct Dropping {
    vel: Vec3,
    ground: f32,
}

#[derive(Component)]
struct Splat(f32);

#[derive(Resource)]
struct Looks {
    splat: Handle<Mesh>,
    splat_mat: Handle<StandardMaterial>,
    drop: Handle<Mesh>,
}

fn spawn(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let t = tuning();
    let white = mats.add(StandardMaterial {
        base_color: Color::srgb(0.88, 0.88, 0.82),
        perceptual_roughness: 0.4,
        ..default()
    });
    commands.insert_resource(Looks {
        splat: meshes.add(Cylinder::new(0.12, 0.01)),
        splat_mat: white,
        drop: meshes.add(Sphere::new(0.03)),
    });
    let mut seed = 0x0355_u64;
    for (fi, f) in t.flocks.iter().enumerate() {
        let b = Bounds::of(f);
        for _ in 0..f.count {
            seed += 1;
            let r = |k: u64| hash(seed * 7 + k);
            let pos = b.min + (b.max - b.min) * Vec3::new(r(1), r(2), r(3));
            let dir = Vec3::new(r(4) - 0.5, (r(5) - 0.5) * 0.2, r(6) - 0.5).normalize_or(Vec3::X);
            commands.spawn((
                Pigeon {
                    flock: fi,
                    boid: Boid {
                        pos,
                        vel: dir * t.min_speed,
                        steer: Vec3::ZERO,
                    },
                    think_in: r(7) / t.think_hz.max(0.1),
                    flapping: true,
                    seed,
                },
                crate::rigged::Rigged { path: MODEL },
                crate::rigged::RigClip::looping("flap"),
                Transform::from_translation(pos),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
}

fn camera(cam: &Query<&GlobalTransform, With<Camera3d>>) -> Option<Vec3> {
    crate::music::camera_at(cam)
}

/// Which flocks are near enough the camera to bother with.
fn awake(t: &Tuning, cam: Option<Vec3>) -> Vec<bool> {
    t.flocks
        .iter()
        .map(|f| {
            cam.is_some_and(|c| {
                let b = Bounds::of(f);
                let nearest = c.clamp(b.min, b.max);
                nearest.distance(c) < t.draw_range
            })
        })
        .collect()
}

/// Local: birds whose turn it is re-think the rules (on a grid per flock).
fn think(
    time: Res<Time>,
    cam: Query<&GlobalTransform, With<Camera3d>>,
    mut birds: Query<&mut Pigeon>,
) {
    let t = tuning();
    let awake = awake(&t, camera(&cam));
    let dt = time.delta_secs();
    let secs = time.elapsed_secs();
    for (fi, f) in t.flocks.iter().enumerate() {
        if !awake.get(fi).copied().unwrap_or(false) {
            continue;
        }
        let snapshot: Vec<Boid> = birds
            .iter()
            .filter(|p| p.flock == fi)
            .map(|p| p.boid)
            .collect();
        let grid = Grid::build(&snapshot, t.view);
        let b = Bounds::of(f);
        for (i, mut p) in birds.iter_mut().filter(|p| p.flock == fi).enumerate() {
            p.think_in -= dt;
            if p.think_in > 0.0 || i >= snapshot.len() {
                continue;
            }
            p.think_in += 1.0 / t.think_hz.max(0.1);
            let k = p.seed.wrapping_mul(31) + (secs * 3.0) as u64;
            let jitter =
                Vec3::new(hash(k) - 0.5, (hash(k + 1) - 0.5) * 0.4, hash(k + 2) - 0.5) * 2.0;
            p.boid.steer = steer(i, &snapshot, &grid, b, &t, jitter);
        }
    }
}

/// Local: everyone awake flies (every frame), turns to face where it's going,
/// flaps or glides, and now and then poops.
#[allow(clippy::too_many_arguments)]
fn fly_all(
    mut commands: Commands,
    time: Res<Time>,
    cam: Query<&GlobalTransform, With<Camera3d>>,
    looks: Option<Res<Looks>>,
    offset: Option<Res<crate::features::stillness::SessionOffset>>,
    mut birds: Query<(
        &mut Pigeon,
        &mut Transform,
        &mut Visibility,
        &mut crate::rigged::RigClip,
    )>,
) {
    let t = tuning();
    let awake = awake(&t, camera(&cam));
    let dt = time.delta_secs().min(0.1);
    let session = offset.map_or(time.elapsed_secs(), |o| {
        crate::features::stillness::session_secs(&time, &o)
    });
    let rate = if storm_at(session, &t).is_some() {
        t.storm_rate
    } else {
        t.poop_chance
    };
    let secs = time.elapsed_secs();
    for (mut p, mut tf, mut vis, mut clip) in &mut birds {
        let on = awake.get(p.flock).copied().unwrap_or(false);
        vis.set_if_neq(if on {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if !on {
            continue;
        }
        p.boid = fly(p.boid, dt, &t);
        let v = p.boid.vel;
        tf.translation = p.boid.pos;
        if v.length_squared() > 1e-6 {
            // The model's head is -z: look along the velocity, banked into the turn.
            let bank =
                (p.boid.steer.dot(v.cross(Vec3::Y).normalize_or_zero()) * 0.08).clamp(-0.6, 0.6);
            *tf = tf.looking_to(v, Vec3::Y);
            tf.rotate_local_z(bank);
        }
        let flap = v.y > 0.3 || v.length() > (t.min_speed + t.max_speed) * 0.5;
        if flap != p.flapping {
            p.flapping = flap;
            *clip = crate::rigged::RigClip::looping(if flap { "flap" } else { "glide" });
        }
        let k = p.seed.wrapping_mul(977) + (secs * 10.0) as u64;
        if hash(k) < rate * dt
            && let Some(looks) = &looks
        {
            let ground = t.flocks.get(p.flock).map_or(0.0, |f| f.ground);
            commands.spawn((
                Dropping {
                    vel: v * 0.8,
                    ground,
                },
                Mesh3d(looks.drop.clone()),
                MeshMaterial3d(looks.splat_mat.clone()),
                Transform::from_translation(p.boid.pos - Vec3::Y * 0.1),
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
}

/// Local: droppings fall, splat, and fade; one landing on you is news.
#[allow(clippy::too_many_arguments)]
fn droppings(
    mut commands: Commands,
    time: Res<Time>,
    looks: Option<Res<Looks>>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    mut drops: Query<(Entity, &mut Dropping, &mut Transform), Without<Splat>>,
    mut splats: Query<(Entity, &mut Splat)>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let (t, dt) = (tuning(), time.delta_secs().min(0.1));
    let my = me.as_ref().and_then(|me| {
        walkers
            .iter()
            .find(|(p, _)| p.id == me.0)
            .map(|(_, w)| w.pos)
    });
    let mut hit = false;
    let mut landed = 0;
    for (e, mut d, mut tf) in &mut drops {
        d.vel.y -= 9.81 * dt;
        tf.translation += d.vel * dt;
        if tf.translation.y > d.ground {
            continue;
        }
        let at = Vec3::new(tf.translation.x, d.ground + 0.006, tf.translation.z);
        commands.entity(e).despawn();
        landed += 1;
        if my.is_some_and(|m| {
            Vec2::new(m.x - at.x, m.z - at.z).length() < t.hit_radius && (m.y - at.y).abs() < 2.5
        }) {
            hit = true;
        }
        if let Some(looks) = &looks {
            commands.spawn((
                Splat(t.splat_secs),
                Mesh3d(looks.splat.clone()),
                MeshMaterial3d(looks.splat_mat.clone()),
                Transform::from_translation(at),
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    // Splats fade out; past the cap the oldest go first.
    let mut all: Vec<(Entity, f32)> = Vec::new();
    for (e, mut s) in &mut splats {
        s.0 -= dt;
        if s.0 <= 0.0 {
            commands.entity(e).despawn();
        } else {
            all.push((e, s.0));
        }
    }
    let over = (all.len() + landed).saturating_sub(t.max_splats);
    if over > 0 {
        all.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (e, _) in all.into_iter().take(over) {
            commands.entity(e).despawn();
        }
    }
    if hit && let Some(mut toast) = toast {
        toast.show(t.hit_line.clone());
    }
}

/// Local: the storm's bill when one starts.
fn storm_line(
    time: Res<Time>,
    offset: Option<Res<crate::features::stillness::SessionOffset>>,
    toast: Option<ResMut<crate::hud::Toast>>,
    mut was: Local<Option<u64>>,
) {
    let t = tuning();
    let session = offset.map_or(time.elapsed_secs(), |o| {
        crate::features::stillness::session_secs(&time, &o)
    });
    let now = storm_at(session, &t).map(|(k, _)| k);
    if now.is_some()
        && now != *was
        && let Some(mut toast) = toast
    {
        toast.show(t.storm_line.replace("{damage}", &damage(&t).to_string()));
    }
    *was = now;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.flocks.len() >= 3 && t.flocks.iter().all(|f| f.count > 0));
        assert!(0.0 < t.min_speed && t.min_speed < t.max_speed);
        assert!(t.too_close < t.view && t.think_hz > 0.0 && t.draw_range > 0.0);
        assert!(t.poop_chance > 0.0 && t.storm_rate > t.poop_chance);
        assert!(t.storm_chance > 0.0 && t.storm_chance < 1.0 && t.storm_secs < t.storm_every);
        assert!(t.max_splats > 0 && t.splat_secs > 0.0 && t.hit_radius > 0.0);
        assert!(!t.hit_line.is_empty() && t.storm_line.contains("{damage}"));
        // Inside buildings too (catherine), and every flock above its ground.
        assert!(t.flocks.iter().any(|f| f.name.contains("Oculus")));
        for f in &t.flocks {
            assert!(
                Bounds::of(f).min.y > f.ground,
                "{} under its ground",
                f.name
            );
        }
    }

    #[test]
    fn the_grid_finds_the_same_neighbours_as_looking_at_everyone() {
        let boids: Vec<Boid> = (0..60)
            .map(|i| Boid {
                pos: Vec3::new(hash(i) * 30.0, hash(i + 100) * 8.0, hash(i + 200) * 30.0),
                ..default()
            })
            .collect();
        let grid = Grid::build(&boids, 4.0);
        for (i, b) in boids.iter().enumerate() {
            let mut near: Vec<usize> = grid
                .near(b.pos)
                .filter(|&j| j != i && boids[j].pos.distance(b.pos) < 4.0)
                .collect();
            let mut all: Vec<usize> = (0..boids.len())
                .filter(|&j| j != i && boids[j].pos.distance(b.pos) < 4.0)
                .collect();
            near.sort();
            all.sort();
            assert_eq!(near, all);
        }
    }

    #[test]
    fn a_flock_stays_together_in_its_box_at_bird_speeds() {
        let t = t();
        let f = Flock {
            name: "test".into(),
            center: (0.0, 15.0, 0.0),
            half: (20.0, 6.0, 20.0),
            ground: 0.0,
            count: 20,
        };
        let b = Bounds::of(&f);
        let mut boids: Vec<Boid> = (0..f.count as u64)
            .map(|i| Boid {
                pos: Vec3::new(hash(i) * 10.0, 15.0 + hash(i + 50), hash(i + 99) * 10.0),
                vel: Vec3::X * t.min_speed,
                steer: Vec3::ZERO,
            })
            .collect();
        for step in 0..2000u64 {
            let grid = Grid::build(&boids, t.view);
            let snap = boids.clone();
            for (i, bd) in boids.iter_mut().enumerate() {
                let j = Vec3::new(hash(step * 40 + i as u64) - 0.5, 0.0, 0.0);
                bd.steer = steer(i, &snap, &grid, b, &t, j);
            }
            for bd in &mut boids {
                *bd = fly(*bd, 1.0 / 30.0, &t);
            }
        }
        for bd in &boids {
            let s = bd.vel.length();
            assert!(s >= t.min_speed - 1e-3 && s <= t.max_speed + 1e-3);
            let slack = Vec3::splat(6.0);
            assert!(
                bd.pos.cmpge(b.min - slack).all() && bd.pos.cmple(b.max + slack).all(),
                "flew off to {}",
                bd.pos
            );
        }
        // Nobody sits inside anybody.
        for i in 0..boids.len() {
            for j in i + 1..boids.len() {
                assert!(boids[i].pos.distance(boids[j].pos) > 0.05);
            }
        }
    }

    #[test]
    fn droppings_fall_forward_onto_the_ground() {
        let at = lands(Vec3::new(0.0, 20.0, 0.0), Vec3::new(5.0, 0.0, 0.0), 0.0);
        assert_eq!(at.y, 0.0);
        // ~2 s to fall 20 m at 5 m/s.
        assert!((at.x - 10.1).abs() < 0.3, "{at}");
    }

    #[test]
    fn storms_are_rare_shared_and_short() {
        let t = t();
        let windows = 2000;
        let mut storms = 0;
        for k in 1..=windows {
            let start = k as f32 * t.storm_every;
            if storm_at(start + 0.1, &t).is_some() {
                storms += 1;
                assert!(storm_at(start + t.storm_secs + 0.1, &t).is_none());
            }
        }
        let share = storms as f32 / windows as f32;
        assert!((share - t.storm_chance).abs() < 0.05, "{share}");
        assert!(storm_at(1.0, &t).is_none(), "not right at the start");
        assert!(damage(&t) > 0);
    }
}
