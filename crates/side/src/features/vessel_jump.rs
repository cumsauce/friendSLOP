//! Jumping off the top of the Vessel: stand on the top ring, click the little
//! knob by the landing, and you hop over the parapet and ragdoll the whole
//! 45 m down to the plaza, where a blood splat waits for you before you get
//! back up. Everyone sees the fall (the ragdoll is replicated) and the splat
//! (a replicated marker, drawn like the pee puddles in `skills`).
//!
//! The prompt is a small glowing knob hovering over the top landing
//! (`super::vessel::landing(LEVELS - 1, 0)`); the server re-checks you are
//! actually up there before it flings you.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::deny;
use crate::player::{Player, PlayerOwner};

/// Tuning numbers: `assets/data/side/vessel_jump.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The outward push when you jump (m/s).
    pub push: f32,
    /// The little hop up over the parapet (m/s).
    pub hop: f32,
    /// How high above the plaza your feet must be to jump (m).
    pub min_height: f32,
    /// The blood splat's radius (m) and how long it lingers (s).
    pub splat_size: f32,
    pub splat_secs: f32,
    /// How long the fall ragdolls you: longer than a punt, the whole way
    /// down plus a lie on the floor (s).
    pub fall_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/vessel_jump.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Client -> server: jump off the top of the Vessel.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct JumpOff;

/// A blood splat on the ground where a jumper landed. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct BloodSplat {
    pub at: Vec3,
    pub size: f32,
}

/// Server only: seconds until the splat is cleaned up.
#[derive(Component, Debug)]
pub struct SplatLife(pub f32);

/// Marks the little knob you click at the top of the Vessel.
#[derive(Component)]
pub struct JumpSpot;

/// Is `pos` up the top of the Vessel whose foot is `foot`? The top ring flares
/// to `top_radius`, so anything over `min_height` within a couple of metres
/// clear of it counts.
pub fn at_the_top(pos: Vec3, foot: Vec3, min_height: f32, top_radius: f32) -> bool {
    pos.y - foot.y >= min_height
        && Vec2::new(pos.x - foot.x, pos.z - foot.z).length() <= top_radius + 2.0
}

/// Where a jumper from `pos` (pushed `push` m/s away from the Vessel's `axis`)
/// touches down, if the plaza is at `floor_y`: a straight ballistic drop.
pub fn landing_spot(pos: Vec3, axis: Vec3, floor_y: f32, push: f32) -> Vec3 {
    let out = (pos - axis).with_y(0.0);
    let dir = out.try_normalize().unwrap_or(Vec3::X);
    let t = (2.0 * (pos.y - floor_y) / 9.81).sqrt();
    (pos + dir * (push * t)).with_y(floor_y)
}

/// The velocity a jump starts with: a hop up, then pushed away from the
/// Vessel's middle.
pub fn jump_velocity(pos: Vec3, axis: Vec3, tu: &Tuning) -> Vec3 {
    let out = (pos - axis).with_y(0.0);
    let dir = out.try_normalize().unwrap_or(Vec3::X);
    dir * tu.push + Vec3::Y * tu.hop
}

/// The Vessel's foot on the city map, if the map is in.
fn foot() -> Option<Vec3> {
    super::vessel::foot(&super::nyc::MAP.get())
}

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    }
}

pub fn protocol(app: &mut App) {
    app.add_client_event::<JumpOff>(Channel::Ordered)
        .add_observer(jump)
        .add_systems(Update, dry_splats.run_if(in_state(ServerState::Running)))
        .replicate::<BloodSplat>();
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_prompt)
        .add_observer(jump_clicked)
        .add_systems(Update, show_splats);
}

/// A little glowing knob hovering over the top landing: click it to jump.
fn spawn_prompt(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let Some(foot) = foot() else {
        return;
    };
    let at = foot + super::vessel::landing(super::vessel::LEVELS - 1, 0);
    commands.spawn((
        JumpSpot,
        crate::features::interact::Clickable::at_height(0.9)
            .labelled("jump off the Vessel")
            .radius(60.0),
        Mesh3d(meshes.add(Sphere::new(0.12))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.25, 0.2),
            emissive: LinearRgba::rgb(2.0, 0.3, 0.2),
            ..default()
        })),
        Transform::from_translation(at + Vec3::Y * 0.9),
        DespawnOnExit(AppState::InGame),
    ));
}

fn jump_clicked(
    ev: On<crate::features::interact::Clicked>,
    spots: Query<(), With<JumpSpot>>,
    mut commands: Commands,
) {
    if spots.contains(ev.entity) {
        commands.client_trigger(JumpOff);
    }
}

#[allow(clippy::type_complexity)]
fn jump(
    ev: On<FromClient<JumpOff>>,
    mut commands: Commands,
    players: Query<(Entity, &Player, &PlayerOwner, &Walker)>,
    server: Res<State<ServerState>>,
) {
    if *server.get() != ServerState::Running {
        return;
    }
    let Some(foot) = foot() else {
        return;
    };
    let owner = owner_of(ev.client_id);
    let Some((entity, _, _, walker)) = players.iter().find(|(_, _, o, _)| o.0 == owner) else {
        return;
    };
    let top_radius = super::vessel::radius(super::vessel::LEVELS - 1);
    if !at_the_top(walker.pos, foot, tuning().min_height, top_radius) {
        deny(
            &mut commands,
            ev.client_id,
            "you have to be at the top of the Vessel to jump off it",
        );
        return;
    }
    let tu = tuning();
    let vel = jump_velocity(walker.pos, foot, &tu);
    let splat_at = landing_spot(walker.pos, foot, foot.y, tu.push);
    // The splat is already waiting on the plaza when they come down.
    commands.spawn((
        BloodSplat {
            at: splat_at,
            size: tu.splat_size,
        },
        SplatLife(tu.splat_secs),
        Replicated,
    ));
    crate::features::ragdoll::knock_for(&mut commands, entity, vel, tu.fall_secs);
}

/// Everyone: a splat is a dark red, slightly squashed puddle on the ground.
fn show_splats(
    mut commands: Commands,
    new: Query<(Entity, &BloodSplat), Added<BloodSplat>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    if new.is_empty() {
        return;
    }
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    for (e, splat) in &new {
        let mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.45, 0.04, 0.04, 0.85),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.25,
            ..default()
        });
        commands.entity(e).insert((
            Mesh3d(meshes.add(Cylinder::new(splat.size, 0.012))),
            MeshMaterial3d(mat),
            Transform::from_translation(splat.at + Vec3::Y * 0.02)
                .with_scale(Vec3::new(1.0, 1.0, 0.7)),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Server: splats get cleaned up.
fn dry_splats(
    mut commands: Commands,
    time: Res<Time>,
    mut splats: Query<(Entity, &mut SplatLife)>,
) {
    for (e, mut life) in &mut splats {
        life.0 -= time.delta_secs();
        if life.0 <= 0.0 {
            commands.entity(e).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuning() -> Tuning {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../game/assets/data/side/vessel_jump.ron"
        );
        let bytes = std::fs::read(path).unwrap_or_else(|_| panic!("no {path}"));
        TUNING.set_bytes(&bytes);
        TUNING.get().as_ref().clone()
    }

    #[test]
    fn the_tuning_file_is_sane() {
        let tu = tuning();
        assert!(tu.push > 0.5 && tu.push < 10.0, "push {}", tu.push);
        assert!(tu.hop >= 0.0 && tu.hop < 5.0, "hop {}", tu.hop);
        assert!(
            (35.0..42.0).contains(&tu.min_height),
            "the top of the Vessel is 42.5 m up, min_height {}",
            tu.min_height
        );
        assert!(tu.splat_size > 0.3, "splat_size {}", tu.splat_size);
        assert!(tu.splat_secs > 5.0, "splat_secs {}", tu.splat_secs);
        // A 45 m fall takes about 3 s; the ragdoll must outlast it.
        assert!(tu.fall_secs > 3.5, "fall_secs {}", tu.fall_secs);
    }

    #[test]
    fn only_the_top_of_the_vessel_counts() {
        let foot = Vec3::new(10.0, 0.0, 20.0);
        let top = super::super::vessel::landing(super::super::vessel::LEVELS - 1, 0) + foot;
        let tu = tuning();
        assert!(at_the_top(
            top,
            foot,
            tu.min_height,
            super::super::vessel::radius(super::super::vessel::LEVELS - 1)
        ));
        // Halfway up is not the top, and neither is the right height across town.
        let half = foot + super::super::vessel::landing(8, 0);
        assert!(!at_the_top(
            half,
            foot,
            tu.min_height,
            super::super::vessel::radius(super::super::vessel::LEVELS - 1)
        ));
        assert!(!at_the_top(
            Vec3::new(100.0, 45.0, 100.0),
            foot,
            tu.min_height,
            super::super::vessel::radius(super::super::vessel::LEVELS - 1)
        ));
    }

    #[test]
    fn you_land_pushed_outward_on_the_plaza() {
        let foot = Vec3::ZERO;
        let top = super::super::vessel::landing(super::super::vessel::LEVELS - 1, 2);
        let tu = tuning();
        let spot = landing_spot(top + foot, foot, foot.y, tu.push);
        assert!(spot.y.abs() < 1e-3, "lands on the plaza: {spot}");
        let pushed = Vec2::new(spot.x, spot.z).length();
        let started = Vec2::new(top.x, top.z).length();
        assert!(
            pushed > started + 2.0,
            "pushed out: {pushed} from {started}"
        );
        // And clear of the Vessel's base, so you don't clip the stairs on the way.
        assert!(pushed > super::super::vessel::RADIUS.0 + 1.0);
    }
}
