//! Climbing, and floating islands to climb up to. From issue #99 by doomy.
//!
//! Walk into anything steep (a wall, a rock, a tree trunk, a house) and hold
//! Space while pushing forward to climb it. A/D shuffle sideways on the face.
//! At the top you haul yourself over the edge; let go of Space and you drop.
//!
//! Above the south-east meadow three islands float in the sky, each higher
//! than the last. A tall rock spire rises to the edge of each one, so the way
//! up is climbing: the ground to the first island, then from each island up
//! to the next.
//!
//! Climbing runs on the host next to the kinematic controller (`crate::kcc`):
//! while a body climbs, this module moves it instead of the controller. Like
//! all walking, clients only send input. The islands are local scenery built
//! identically on every peer, colliders included.

use avian3d::character_controller::move_and_slide::{
    MoveAndSlide, MoveAndSlideConfig, MoveAndSlideHitResponse,
};
use avian3d::prelude::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::AppState;
use crate::features::duel::Duelist;
use crate::features::fps::{WalkInput, Walker};
use crate::player::Player;

/// Climbing speed up a face (m/s).
pub const CLIMB_SPEED: f32 = 2.5;
/// Sideways shuffle speed on a face (m/s).
const SHUFFLE_SPEED: f32 = 1.5;
/// How far in front of the body a face still counts as in reach.
const REACH: f32 = crate::kcc::BODY_RADIUS + 0.35;
/// Faces steeper than this (normal's up part below it) are climbable; anything
/// flatter is just ground you can walk on.
const MAX_FACE_UP: f32 = 0.5;
/// Ray heights above the feet: chest (still climbing) and knees (at the lip).
const CHEST: f32 = 1.2;
const KNEES: f32 = 0.3;

/// One floating island: centre (x, z), grassy top height, radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Island {
    pub at: Vec2,
    pub top: f32,
    pub radius: f32,
}

/// The islands, lowest first. Each one's spire stands on the one below it
/// (the first one's on the ground).
pub const ISLANDS: [Island; 3] = [
    Island {
        at: Vec2::new(55.0, 60.0),
        top: 10.0,
        radius: 7.0,
    },
    Island {
        at: Vec2::new(63.0, 60.0),
        top: 16.0,
        radius: 5.0,
    },
    Island {
        at: Vec2::new(63.0, 67.0),
        top: 22.0,
        radius: 4.0,
    },
];
/// The solid slab of each island (the rocky cone below is only looks).
const ISLAND_THICKNESS: f32 = 1.5;
const SPIRE_WIDTH: f32 = 1.6;
/// The island model's radius and the spire model's height (see `raise_islands`).
const ISLAND_MODEL_RADIUS: f32 = 5.0;
const SPIRE_MODEL_HEIGHT: f32 = 8.0;

/// Where island `i`'s spire stands (x, z): on its rim, towards the island
/// below (or, for the first, towards the commune).
pub fn spire_at(i: usize) -> Vec2 {
    let island = ISLANDS[i];
    let towards = if i == 0 {
        -island.at
    } else {
        ISLANDS[i - 1].at - island.at
    };
    island.at + towards.normalize_or_zero() * (island.radius - 0.3)
}

/// What the spire under island `i` stands on.
pub fn spire_base(i: usize) -> f32 {
    if i == 0 {
        let p = spire_at(0);
        crate::terrain::height_at(p.x, p.y) - 0.5
    } else {
        ISLANDS[i - 1].top - ISLAND_THICKNESS
    }
}

pub fn protocol(_app: &mut App) {}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), raise_islands)
        .add_systems(
            Update,
            climb
                .before(super::fps::kcc_move_walkers)
                .run_if(in_state(ServerState::Running))
                .run_if(in_state(AppState::InGame))
                .run_if(
                    |trees: Option<Res<avian3d::collider_tree::ColliderTrees>>| trees.is_some(),
                ),
        )
        .add_systems(OnEnter(AppState::InGame), climb_help);
}

fn climb_help(mut commands: Commands) {
    commands.spawn((
        Text::new("climbing: hold W and Space against a wall, rock or tree to climb it."),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

/// What a body pressing on a face should do this tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hold {
    /// Nothing to climb (or not trying): walk normally.
    None,
    /// A face in front of the chest: climb up it.
    Face,
    /// Only the knees still touch: we're at the lip, haul over it.
    Lip,
}

/// Decide from whether there's a steep face at chest and knee height.
pub fn hold(trying: bool, chest: bool, knees: bool) -> Hold {
    match (trying, chest, knees) {
        (false, _, _) => Hold::None,
        (true, true, _) => Hold::Face,
        (true, false, true) => Hold::Lip,
        (true, false, false) => Hold::None,
    }
}

/// The velocity of a climbing body: up the face, sideways with the stick,
/// and over the edge at the lip. `yaw` is where the body faces.
pub fn climb_velocity(hold: Hold, yaw: f32, stick_x: f32) -> Vec3 {
    let rot = Quat::from_rotation_y(yaw);
    let forward = rot * Vec3::NEG_Z;
    let right = rot * Vec3::X;
    match hold {
        Hold::None => Vec3::ZERO,
        // A little forward keeps us against the face.
        Hold::Face => Vec3::Y * CLIMB_SPEED + right * stick_x * SHUFFLE_SPEED + forward * 0.3,
        Hold::Lip => Vec3::Y * CLIMB_SPEED + forward * 2.0,
    }
}

/// One climbing tick for a body whose feet are at `feet`, facing `yaw`, with
/// `stick` held (x = right, y = forward). `None` if it isn't climbing (not
/// pushing forward, or nothing steep in reach), so the controller walks it.
/// Otherwise the new feet position and the velocity it is moving at.
pub fn climb_step(
    move_and_slide: &MoveAndSlide,
    feet: Vec3,
    yaw: f32,
    stick: Vec2,
    dt: f32,
) -> Option<(Vec3, Vec3)> {
    if stick.y <= 0.1 {
        return None;
    }
    let filter = SpatialQueryFilter::default();
    let dir = Dir3::new(Quat::from_rotation_y(yaw) * Vec3::NEG_Z).ok()?;
    let face_at = |height: f32| {
        move_and_slide
            .spatial_query
            .cast_ray(feet + Vec3::Y * height, dir, REACH, true, &filter)
            .is_some_and(|hit| hit.normal.y.abs() < MAX_FACE_UP && hit.distance > 0.0)
    };
    let h = hold(true, face_at(CHEST), face_at(KNEES));
    if h == Hold::None {
        return None;
    }
    let velocity = climb_velocity(h, yaw, stick.x);
    let out = move_and_slide.move_and_slide(
        &crate::kcc::body(),
        feet + Vec3::Y * crate::kcc::FEET_TO_CENTRE,
        Quat::from_rotation_y(yaw),
        velocity,
        std::time::Duration::from_secs_f32(dt),
        &MoveAndSlideConfig::default(),
        &filter,
        |_| MoveAndSlideHitResponse::Accept,
    );
    Some((
        out.position - Vec3::Y * crate::kcc::FEET_TO_CENTRE,
        velocity,
    ))
}

/// Host: bodies holding W + Space against something steep climb it; the
/// kinematic controller leaves those bodies alone this tick.
fn climb(
    time: Res<Time>,
    still: Query<&super::stillness::Stillness>,
    duelists: Query<&Duelist>,
    move_and_slide: MoveAndSlide,
    mut walkers: Query<(&Player, &mut Walker, &mut WalkInput, &mut crate::kcc::Kcc)>,
) {
    let frozen = super::stillness::is_frozen(&still);
    let dt = time.delta_secs().min(0.1);
    for (player, mut walker, mut input, mut kcc) in &mut walkers {
        let duelling = duelists.iter().any(|d| d.player == player.id);
        let step = (!frozen && !duelling && input.jump)
            .then(|| climb_step(&move_and_slide, walker.pos, walker.yaw, input.stick, dt))
            .flatten();
        input.climbing = step.is_some();
        let Some((pos, velocity)) = step else {
            continue;
        };
        // Let go and the controller carries on from this motion.
        kcc.velocity = velocity;
        kcc.grounded = None;
        if walker.pos != pos {
            walker.pos = pos;
        }
    }
}

/// Every peer: the floating islands and their spires, with colliders. The
/// looks are modeler-made: prefab `by-sky-island` (radius 5, origin at the
/// slab's centre, the rocky underside hanging below) scaled out to each
/// island's radius, and `by-sky-spire` (1.6 m square, 8 m tall, centred)
/// stretched up to each spire's height.
fn raise_islands(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    let model =
        |file: &str, tf: Transform| {
            assets.as_ref().map(|assets| {
                (
                    WorldAssetRoot(assets.load(
                        GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}")),
                    )),
                    tf,
                )
            })
        };
    for (i, island) in ISLANDS.iter().enumerate() {
        // The grassy slab you stand on, with its rocky underside.
        let mut slab = commands.spawn((
            Transform::from_xyz(
                island.at.x,
                island.top - ISLAND_THICKNESS / 2.0,
                island.at.y,
            ),
            Visibility::default(),
            RigidBody::Static,
            Collider::cylinder(island.radius, ISLAND_THICKNESS),
            DespawnOnExit(AppState::InGame),
        ));
        let r = island.radius / ISLAND_MODEL_RADIUS;
        if let Some(look) = model(
            "sky_island.gltf",
            Transform::from_scale(Vec3::new(r, 1.0, r)),
        ) {
            slab.with_child(look);
        }
        // The spire up to its rim.
        let at = spire_at(i);
        let base = spire_base(i);
        // Turn a flat face out, towards whoever comes to climb it.
        let out = (at - island.at).normalize_or_zero();
        let turn = Quat::from_rotation_y((-out.y).atan2(out.x));
        let height = island.top - base;
        let mut spire = commands.spawn((
            Transform::from_xyz(at.x, base + height / 2.0, at.y).with_rotation(turn),
            Visibility::default(),
            RigidBody::Static,
            Collider::cuboid(SPIRE_WIDTH, height, SPIRE_WIDTH),
            DespawnOnExit(AppState::InGame),
        ));
        if let Some(look) = model(
            "sky_spire.gltf",
            Transform::from_scale(Vec3::new(1.0, height / SPIRE_MODEL_HEIGHT, 1.0)),
        ) {
            spire.with_child(look);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn climbing_needs_trying_and_a_face() {
        assert_eq!(hold(false, true, true), Hold::None);
        assert_eq!(hold(true, true, true), Hold::Face);
        assert_eq!(hold(true, false, true), Hold::Lip);
        assert_eq!(hold(true, false, false), Hold::None);
    }

    #[test]
    fn climbing_goes_up_and_over_the_lip() {
        let up = climb_velocity(Hold::Face, 0.0, 0.0);
        assert!(up.y > 2.0 && up.z < 0.0 && up.z > -0.5);
        let over = climb_velocity(Hold::Lip, 0.0, 0.0);
        assert!(over.y > 2.0 && over.z < -1.0, "forward over the edge");
        let side = climb_velocity(Hold::Face, 0.0, 1.0);
        assert!(side.x > 1.0);
    }

    #[test]
    fn each_spire_stands_on_the_island_below() {
        for i in 1..ISLANDS.len() {
            let below = ISLANDS[i - 1];
            let s = spire_at(i);
            assert!(
                s.distance(below.at) < below.radius - SPIRE_WIDTH,
                "spire {i} is over island {}",
                i - 1
            );
            assert!(ISLANDS[i].top > below.top + 3.0, "each island is higher");
        }
        // The first spire reaches from the ground in the open meadow.
        assert_eq!(
            crate::zones::zone_of(spire_at(0)),
            crate::zones::Zone::Meadow
        );
        assert!(spire_base(0) < ISLANDS[0].top - 5.0);
    }
}
