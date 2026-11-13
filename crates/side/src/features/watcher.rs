//! The Watcher: a tall, faceless figure that stands far off, facing one of
//! you. From issue #277 (Horror ARG 1/3, catherine's #182).
//!
//! Every `min_secs`..`max_secs` the server picks a player and puts one
//! replicated [`Watcher`] `distance` metres away from them, behind their back,
//! facing them. Everyone sees the same figure. It vanishes (the server
//! despawns it and sends [`WatcherVanished`]) when any player looks right at it
//! or walks up to it, and every peer gets a burst of static there.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::fps::Walker;
use crate::features::synth_audio::RATE;
use crate::player::Player;

/// Tuning numbers: `assets/data/side/watcher.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Shortest and longest wait between sightings, in seconds.
    pub min_secs: f32,
    pub max_secs: f32,
    /// How far from the watched player it stands (metres).
    pub distance: f32,
    /// It vanishes when anyone comes this close (metres).
    pub vanish_near: f32,
    /// ...or when anyone looks within this many degrees of it.
    pub look_degrees: f32,
    /// It gives up and fades on its own after this long, seconds.
    pub linger_secs: f32,
    /// How tall it is (metres).
    pub height: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/watcher.ron");

fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The figure: where it stands (x, z), which way it faces (yaw) and whom.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Watcher {
    pub at: Vec2,
    pub yaw: f32,
    pub watching: u64,
}

/// Server -> everyone: the figure vanished at `at` in a burst of static.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct WatcherVanished {
    pub at: Vec2,
}

/// Server only: time to the next sighting, the current one's age, a dice.
#[derive(Resource)]
struct Stalk {
    next: f32,
    age: f32,
    dice: u32,
}

impl Default for Stalk {
    fn default() -> Self {
        Self {
            next: tuning().min_secs,
            age: 0.0,
            dice: 0x77a7_c4e5,
        }
    }
}

impl Stalk {
    fn roll(&mut self) -> f32 {
        self.dice = self
            .dice
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.dice >> 8) as f32 / (1u32 << 24) as f32
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<Watcher>()
        .add_server_event::<WatcherVanished>(Channel::Ordered)
        .init_resource::<Stalk>()
        .add_systems(Update, stalk.run_if(in_state(ServerState::Running)));
}

/// The way a walker with `yaw` faces, on the ground.
pub fn forward(yaw: f32) -> Vec2 {
    let f = Quat::from_rotation_y(yaw) * Vec3::NEG_Z;
    Vec2::new(f.x, f.z)
}

/// Where the figure stands for a player at `pos` facing `yaw`: `distance`
/// behind them, turned `skew` radians off straight back, facing them.
pub fn place(pos: Vec2, yaw: f32, distance: f32, skew: f32) -> Watcher {
    let back = Vec2::from_angle(skew).rotate(-forward(yaw));
    let at = pos + back * distance;
    let to = pos - at;
    // yaw whose forward is `to`.
    Watcher {
        at,
        yaw: (-to.x).atan2(-to.y),
        watching: 0,
    }
}

/// Does a walker at `pos` facing `yaw` see the figure at `at` (looks within
/// `degrees` of it) or stand within `near` of it?
pub fn spotted(pos: Vec2, yaw: f32, at: Vec2, near: f32, degrees: f32) -> bool {
    let to = at - pos;
    if to.length() < near {
        return true;
    }
    forward(yaw).angle_to(to).abs().to_degrees() < degrees
}

fn stalk(
    time: Res<Time>,
    mut stalk: ResMut<Stalk>,
    mut commands: Commands,
    watchers: Query<(Entity, &Watcher)>,
    walkers: Query<(&Player, &Walker)>,
) {
    let t = tuning();
    let dt = time.delta_secs();
    if let Some((entity, w)) = watchers.iter().next() {
        stalk.age += dt;
        let seen = walkers
            .iter()
            .any(|(_, k)| spotted(k.pos.xz(), k.yaw, w.at, t.vanish_near, t.look_degrees));
        if seen || stalk.age > t.linger_secs {
            commands.entity(entity).despawn();
            commands.server_trigger(ToClients {
                targets: SendTargets::All,
                message: WatcherVanished { at: w.at },
            });
        }
        return;
    }
    stalk.next -= dt;
    if stalk.next > 0.0 {
        return;
    }
    stalk.next = t.min_secs + stalk.roll() * (t.max_secs - t.min_secs).max(0.0);
    let all: Vec<(u64, Vec2, f32)> = walkers
        .iter()
        .map(|(p, k)| (p.id, k.pos.xz(), k.yaw))
        .collect();
    if all.is_empty() {
        return;
    }
    let (id, pos, yaw) = all[(stalk.roll() * all.len() as f32) as usize % all.len()];
    let skew = (stalk.roll() - 0.5) * 1.6;
    let mut w = place(pos, yaw, t.distance, skew);
    w.watching = id;
    stalk.age = 0.0;
    commands.spawn((w, Replicated));
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, (dress, fizzle).run_if(in_state(AppState::InGame)))
        .add_observer(burst);
}

/// Give the figure its body on every peer: a tall black column with long
/// thin arms and a smooth blank head. Placeholder primitives.
fn dress(
    mut commands: Commands,
    new: Query<(Entity, &Watcher), Added<Watcher>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let h = tuning().height.max(1.0);
    for (entity, w) in &new {
        let dark = materials.add(StandardMaterial {
            base_color: Color::srgb(0.02, 0.02, 0.025),
            perceptual_roughness: 1.0,
            ..default()
        });
        let pale = materials.add(StandardMaterial {
            base_color: Color::srgb(0.75, 0.74, 0.72),
            perceptual_roughness: 0.9,
            ..default()
        });
        let ground = crate::terrain::height_at(w.at.x, w.at.y);
        let body = meshes.add(Capsule3d::new(0.22, h * 0.62));
        let arm = meshes.add(Capsule3d::new(0.05, h * 0.42));
        let head = meshes.add(Sphere::new(0.2));
        commands
            .entity(entity)
            .insert((
                Transform::from_xyz(w.at.x, ground, w.at.y)
                    .with_rotation(Quat::from_rotation_y(w.yaw)),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|c| {
                c.spawn((
                    Mesh3d(body),
                    MeshMaterial3d(dark.clone()),
                    Transform::from_xyz(0.0, h * 0.45, 0.0),
                ));
                for side in [-1.0, 1.0] {
                    c.spawn((
                        Mesh3d(arm.clone()),
                        MeshMaterial3d(dark.clone()),
                        Transform::from_xyz(side * 0.3, h * 0.5, 0.0)
                            .with_rotation(Quat::from_rotation_z(side * 0.05)),
                    ));
                }
                c.spawn((
                    Mesh3d(head),
                    MeshMaterial3d(pale),
                    Transform::from_xyz(0.0, h * 0.9, 0.0).with_scale(Vec3::new(0.9, 1.3, 0.9)),
                ));
            });
    }
}

/// A short-lived flicker of static where the figure stood.
#[derive(Component)]
struct Static {
    left: f32,
}

/// Every peer: a burst of static where it vanished, seen and heard.
fn burst(
    ev: On<WatcherVanished>,
    mut commands: Commands,
    state: Option<Res<State<AppState>>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    let at = Vec3::new(
        ev.at.x,
        crate::terrain::height_at(ev.at.x, ev.at.y),
        ev.at.y,
    );
    if let (Some(mut meshes), Some(mut materials)) = (meshes, materials) {
        let speck = meshes.add(Cuboid::new(0.12, 0.12, 0.12));
        let grey = materials.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.85, 0.85),
            emissive: LinearRgba::rgb(1.5, 1.5, 1.5),
            unlit: true,
            ..default()
        });
        let h = tuning().height.max(1.0);
        let mut dice = 0x5717_1c00u32 ^ ev.at.x.to_bits() ^ ev.at.y.to_bits().rotate_left(7);
        let mut roll = move || {
            dice = dice.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (dice >> 8) as f32 / (1u32 << 24) as f32
        };
        for _ in 0..60 {
            let off = Vec3::new((roll() - 0.5) * 0.9, roll() * h, (roll() - 0.5) * 0.9);
            commands.spawn((
                Mesh3d(speck.clone()),
                MeshMaterial3d(grey.clone()),
                Transform::from_translation(at + off),
                Static {
                    left: 0.3 + roll() * 0.6,
                },
                DespawnOnExit(AppState::InGame),
            ));
        }
    }
    crate::spatial::play_wav_at(
        &mut commands,
        at + Vec3::Y * 1.6,
        0.8,
        crate::spatial::Carry::LOUD,
        || wav_bytes(&static_samples(0.9, RATE), RATE),
    );
}

/// Static specks jitter and blink out.
fn fizzle(
    time: Res<Time>,
    mut commands: Commands,
    mut specks: Query<(Entity, &mut Static, &mut Transform, &mut Visibility)>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    for (entity, mut s, mut tf, mut vis) in &mut specks {
        s.left -= dt;
        if s.left <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let k = tf.translation.x * 13.0 + tf.translation.y * 7.0 + t * 60.0;
        tf.translation += Vec3::new(k.sin(), k.cos(), (k * 1.7).sin()) * 0.03;
        *vis = if (k * 3.1).sin() > -0.2 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// `secs` of crackling white noise that swells and cuts off.
pub fn static_samples(secs: f32, rate: u32) -> Vec<f32> {
    let n = (secs * rate as f32) as usize;
    let mut dice = 0x00c0_ffeeu32;
    (0..n)
        .map(|i| {
            dice = dice.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (dice >> 8) as f32 / (1u32 << 23) as f32 - 1.0;
            let p = i as f32 / n as f32;
            // Swell in, crackle (gated in short bursts), then stop dead.
            let gate = if (i / (rate as usize / 40)) % 3 == 1 {
                0.3
            } else {
                1.0
            };
            noise * gate * (p * 4.0).min(1.0) * 0.6
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_stands_behind_you_facing_you() {
        let w = place(Vec2::ZERO, 0.0, 40.0, 0.0);
        // Facing -z, so behind is +z.
        assert!((w.at - Vec2::new(0.0, 40.0)).length() < 1e-3);
        assert!(forward(w.yaw).dot(Vec2::new(0.0, -1.0)) > 0.999);
        assert!(!spotted(Vec2::ZERO, 0.0, w.at, 12.0, 9.0));
    }

    #[test]
    fn looking_or_walking_up_spots_it() {
        let at = Vec2::new(0.0, 40.0);
        assert!(spotted(Vec2::ZERO, std::f32::consts::PI, at, 12.0, 9.0));
        assert!(spotted(Vec2::new(0.0, 30.0), 0.0, at, 12.0, 9.0));
    }

    #[test]
    fn static_is_loud_and_bounded() {
        let s = static_samples(0.5, 22_050);
        assert_eq!(s.len(), 11_025);
        assert!(s.iter().all(|x| x.abs() <= 0.61));
    }
}
