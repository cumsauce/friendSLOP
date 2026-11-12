//! Commune villagers to hide among. From issue #61 by doomy ("environment
//! generated and npcs and then hide and seek with players").
//!
//! The server spawns [`COUNT`] replicated `Villager`s that look like player
//! bodies (same capsule, same colour scheme, googly eyes). Their brain runs on
//! the server at [`AI_HZ`]: wander between points of interest (bonfire, ruins,
//! pond, cover spots), pause or sit for a bit, never step into the duel arena.
//! Only the position (updated at the AI rate) is replicated; every peer glides
//! the body there.
//!
//! Once the host is in the game, it builds a navigation mesh from the world's
//! colliders (rerecast, off the main thread) and gives every villager a
//! `bevy_landmass` agent, so longer trips go overland between the districts:
//! commune centre, the east village's market, the south-west maze and ruins,
//! the forest clearing and the lake shore. Until the mesh is ready (and in
//! headless tests, which never build one) the brain falls back to the old
//! straight-line steering between the local spots.
//!
//! Ten of them are the friends from the Discord (#159, ids from
//! [`FIRST_FRIEND`]); clicking one is a chat, handled in `discord_friends.rs`.
//!
//! Clicking a villager outside hide and seek just gets a hello. A seeker who
//! clicks one during the seeking phase is told "that's just Kevin from the
//! commune" and is frozen for [`PENALTY_SECS`]. Now and then a villager near
//! you mutters something (HUD toast).

use bevy::platform::time::Instant;

use avian_rerecast::AvianBackendPlugin;
use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;
use bevy_landmass::{
    Agent3d, AgentDesiredVelocity3d, AgentSettings, AgentState, AgentTarget3d, Archipelago3d,
    ArchipelagoOptions, ArchipelagoRef3d, FromAgentRadius, Landmass3dPlugin,
};
use bevy_replicon::prelude::*;
use bevy_rerecast::prelude::{NavmeshGenerator, NavmeshPlugins, NavmeshReady, NavmeshSettings};
use landmass_rerecast::{Island3dBundle, LandmassRerecastPlugin, NavMeshHandle3d};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::duel::{arena_center, arena_half};
use crate::features::fps::{WalkSet, Walker};
use crate::features::hide_and_seek::{HideSeek, HideSeekNews, Phase, Role};
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};
pub use crate::rules::villagers::*;

/// AI decisions (and replicated moves) per second.
pub const AI_HZ: f32 = 5.0;
/// Tuning numbers: `assets/data/village/villagers.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    pub penalty_secs: f32,
    pub speed: f32,
    /// How far outside the arena walls villagers stay.
    pub arena_margin: f32,
    /// The villagers' body on the navmesh (handoff #94).
    pub agent_radius: f32,
    /// Ledges up to this height are walkable (the hut steps, the quarry terraces).
    pub max_climb: f32,
    /// Steeper than this and the mesh ends (the quarry walls stay out of reach).
    pub max_slope_degs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/village/villagers.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

const AGENT_HEIGHT: f32 = 1.7;

/// Where villagers like to hang out (x, z): the map's `Hangout` markers.
fn points() -> Vec<Vec2> {
    friendslop_world::places::layout().hangouts.clone()
}

/// Where the navmesh lets villagers travel to (handoff #94): one spot per
/// district (`Hangout` markers with `trip`), so trips between them cross the
/// map overland. Only picked while a villager has a landmass agent; without
/// the mesh the brain sticks to [`points`], which straight-line steering can
/// always reach.
fn districts() -> Vec<Vec2> {
    friendslop_world::places::layout().trips.clone()
}

/// Server only: what a villager is up to.
#[derive(Component, Debug)]
struct Brain {
    target: Vec2,
    wait: f32,
    /// Where it stood before the last brain step (for cover avoidance).
    prev: Vec2,
}

/// Server only: a seeker who mistook a villager for a hider stands here until `until`.
#[derive(Component, Debug)]
pub struct Fooled {
    pub until: f32,
    pub at: Vec3,
}

/// Client -> server: I clicked villager `id`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PokeVillager {
    pub id: u8,
}

/// Server only: villager dice.
#[derive(Resource)]
struct VillageRng(u64);

/// Server: the villagers' navigation. Created when the host enters the game;
/// the mesh arrives in the background a moment later.
#[derive(Resource)]
pub struct VillageNav {
    /// The landmass archipelago every villager agent joins (the fat cats too).
    pub archipelago: Entity,
    /// The rerecast mesh, held so it stays loaded.
    mesh: Handle<bevy_rerecast::Navmesh>,
    /// When the build was queued, for the ready log line.
    started: Instant,
    /// The mesh is built and hung on an island: only now can agents path
    /// across the map (before that, long trips would straight-line through walls).
    pub ready: bool,
}

impl VillageRng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<Villager>()
        .add_client_event::<PokeVillager>(Channel::Ordered)
        .insert_resource(VillageRng(0x5EED_CAFE_F00D_1234))
        .add_observer(on_poke)
        .add_systems(OnEnter(ServerState::Running), spawn_villagers)
        .add_systems(
            Update,
            (think, hold_fooled.after(WalkSet)).run_if(in_state(ServerState::Running)),
        );
}

/// Push a point out of the duel arena (plus margin), if it's inside.
pub fn outside_arena(p: Vec2) -> Vec2 {
    let c = arena_center().xz();
    let half = arena_half() + tuning().arena_margin;
    let d = p - c;
    if d.x.abs() >= half || d.y.abs() >= half {
        return p;
    }
    // Out through the nearest wall.
    let (px, py) = (half - d.x.abs(), half - d.y.abs());
    if px < py {
        Vec2::new(c.x + half * d.x.signum(), p.y)
    } else {
        Vec2::new(p.x, c.y + half * d.y.signum())
    }
}

fn spawn_villagers(mut commands: Commands, mut rng: ResMut<VillageRng>) {
    for i in 0..COUNT {
        let home = points()
            .get(i % points().len().max(1))
            .copied()
            .unwrap_or_default();
        let pos = outside_arena(home + Vec2::new(rng.f() - 0.5, rng.f() - 0.5) * 2.0);
        commands.spawn((
            Villager {
                id: i as u8,
                pos,
                yaw: rng.f() * std::f32::consts::TAU,
                sitting: false,
            },
            Brain {
                target: pos,
                wait: rng.f() * 4.0,
                prev: pos,
            },
            Replicated,
        ));
    }
}

/// Server: villager brains, ticked at `AI_HZ`. With a landmass agent the
/// villager walks the navmesh path to its target (and trips may cross the map
/// to another district); without one it straight-lines it to nearby spots.
fn think(
    time: Res<Time>,
    mut acc: Local<f32>,
    mut rng: ResMut<VillageRng>,
    nav: Option<Res<VillageNav>>,
    still: Query<&super::stillness::Stillness>,
    hunted: super::punishment::Hunted,
    caught: Query<&super::caught::Caught>,
    mut villagers: Query<(
        &mut Villager,
        &mut Brain,
        Option<&mut AgentTarget3d>,
        Option<&AgentDesiredVelocity3d>,
        Option<&AgentState>,
    )>,
) {
    *acc += time.delta_secs();
    let dt = 1.0 / AI_HZ;
    if *acc < dt {
        return;
    }
    *acc = (*acc - dt).min(dt);
    if super::stillness::is_frozen(&still) {
        return; // villagers respect the stillness too
    }
    let prey = hunted.spots();
    for (mut v, mut brain, agent_target, desired, state) in &mut villagers {
        // Holding someone it caught (#140): `caught.rs` moves it.
        if super::caught::is_catcher(&caught, v.id) {
            brain.target = v.pos;
            brain.wait = 1.5;
            continue;
        }
        // Attack mode (#123): drop everything and chase the farter.
        if let Some((pos, dir)) = super::punishment::chase_step(v.pos, &prey, dt) {
            v.pos = outside_arena(pos);
            v.yaw = (-dir.x).atan2(-dir.y);
            if v.sitting {
                v.sitting = false;
            }
            brain.target = v.pos;
            brain.wait = 1.5; // catch their breath, then wander off again
            continue;
        }
        if brain.wait > 0.0 {
            brain.wait -= dt;
            if brain.wait <= 0.0 {
                // Up and off somewhere new. With the navmesh, most trips are
                // overland to another district; the rest stay local.
                let spots = if agent_target.is_some()
                    && nav.as_ref().is_some_and(|n| n.ready)
                    && rng.f() < 0.55
                {
                    districts()
                } else {
                    points()
                };
                let pick = (rng.f() * spots.len() as f32) as usize;
                let Some(&p) = spots.get(pick % spots.len().max(1)) else {
                    continue;
                };
                brain.target = outside_arena(p + Vec2::new(rng.f() - 0.5, rng.f() - 0.5) * 3.0);
                if let Some(mut target) = agent_target {
                    *target = AgentTarget3d::Point(Vec3::new(
                        brain.target.x,
                        crate::terrain::height_at(brain.target.x, brain.target.y),
                        brain.target.y,
                    ));
                }
                if v.sitting {
                    v.sitting = false;
                }
            }
            continue;
        }
        // Arrived (whichever way we walked)?
        let to = brain.target - v.pos;
        let step = tuning().speed * dt;
        if to.length() <= step {
            v.pos = brain.target;
            brain.wait = 2.0 + rng.f() * 8.0;
            v.sitting = rng.f() < 0.35;
            continue;
        }
        // Follow the navmesh path while landmass is actually driving us.
        let on_path = matches!(state, Some(AgentState::Moving));
        if on_path && let Some(desired) = desired {
            let dir = desired.velocity().xz();
            let len = dir.length();
            if len > 1e-4 {
                let dir = dir / len;
                v.pos = outside_arena(v.pos + dir * step);
                v.yaw = (-dir.x).atan2(-dir.y);
                continue;
            }
        }
        // Fallback: straight-line steering (also covers NoPath and being off
        // the mesh, and is all headless tests ever use).
        let dir = to / to.length();
        v.pos = outside_arena(v.pos + dir * step);
        v.yaw = (-dir.x).atan2(-dir.y);
    }
}

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    }
}

fn on_poke(
    ev: On<FromClient<PokeVillager>>,
    mut commands: Commands,
    time: Res<Time>,
    state: Query<&HideSeek>,
    villagers: Query<&Villager>,
    walkers: Query<(&PlayerOwner, &Walker)>,
    players: Query<(
        Entity,
        &Player,
        &PlayerOwner,
        &Walker,
        Option<&Role>,
        Has<Fooled>,
    )>,
    still: Query<&super::stillness::Stillness>,
) {
    if super::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Some(v) = villagers.iter().find(|v| v.id == ev.id) else {
        return;
    };
    let seeking = state.single().is_ok_and(|s| s.phase == Phase::Seeking);
    let owner = owner_of(ev.client_id);
    let Some((me, player, _, walker, role, fooled)) =
        players.iter().find(|(_, _, o, ..)| o.0 == owner)
    else {
        return;
    };
    if !seeking || !role.is_some_and(|r| r.seeks()) || fooled {
        return; // just a friendly poke; the client already said hello
    }
    let at = Vec3::new(v.pos.x, 0.0, v.pos.y);
    if !in_reach(&mut commands, ev.client_id, &walkers, at) {
        return;
    }
    commands.entity(me).insert(Fooled {
        until: time.elapsed_secs() + tuning().penalty_secs,
        at: walker.pos,
    });
    deny(
        &mut commands,
        ev.client_id,
        format!(
            "that's just {} from the commune. you stand there, embarrassed ({}s)",
            v.name(),
            tuning().penalty_secs as u32
        ),
    );
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: HideSeekNews {
            text: format!(
                "player {} tried to tag {}. {} is confused.",
                player.id,
                v.name(),
                v.name()
            ),
        },
    });
}

/// Server: fooled seekers can't move for a moment.
fn hold_fooled(
    mut commands: Commands,
    time: Res<Time>,
    mut fooled: Query<(Entity, &mut Walker, &Fooled)>,
) {
    let now = time.elapsed_secs();
    for (e, mut w, f) in &mut fooled {
        if now >= f.until {
            commands.entity(e).remove::<Fooled>();
        } else if w.pos != f.at {
            w.pos = f.at;
        }
    }
}

// ---------------------------------------------------------------- local side

pub fn plugin(app: &mut App) {
    app.add_plugins((
        // Navmesh plumbing for the villagers' brains: rerecast builds the mesh
        // from the world's avian colliders, landmass walks agents over it. The
        // mesh is only built on the host (see `build_navmesh`).
        NavmeshPlugins::default(),
        AvianBackendPlugin::default(),
        Landmass3dPlugin::default(),
        LandmassRerecastPlugin::default(),
    ))
    .add_observer(on_navmesh_ready)
    .init_resource::<NavBuild>()
    .add_systems(OnExit(ServerState::Running), forget_navmesh)
    .add_observer(click_villager)
    .add_systems(
        Update,
        (dress_villagers, move_villagers, mutter).run_if(in_state(AppState::InGame)),
    )
    .add_systems(
        Update,
        (tag_navmesh_exclusions, build_navmesh, attach_agents)
            .chain()
            .run_if(in_state(ServerState::Running))
            .run_if(in_state(AppState::InGame)),
    )
    .add_systems(
        Update,
        avoid_cover
            .run_if(in_state(ServerState::Running))
            .run_if(in_state(AppState::InGame)),
    );
}

/// Server: every scattered collider (tree trunks, rocks, fences, logs, crates,
/// rubble) is left out of the navmesh. Small obstacles punch little holes into
/// the walkable area, and one bad hole aborts rerecast's whole build
/// (janhohenheim/rerecast#64) — while the big village walls and the maze, which
/// sit on the ground, only notch the region edges and are safe. Villagers dodge
/// the excluded props exactly like players do: the host-side `avoid_cover`
/// sweep veers them around whatever the mesh doesn't know about.
fn tag_navmesh_exclusions(
    mut build: ResMut<NavBuild>,
    mut commands: Commands,
    colliders: Query<(Entity, &RigidBody), With<Collider>>,
    keep: Query<
        (),
        Or<(
            With<crate::terrain::GroundCollider>,
            With<crate::buildings::Building>,
        )>,
    >,
) {
    if build.tagged {
        return;
    }
    build.tagged = true;
    let mut tagged = 0;
    for (entity, body) in &colliders {
        if !body.is_static() || keep.contains(entity) {
            continue;
        }
        commands
            .entity(entity)
            .insert(avian_rerecast::ExcludeColliderFromNavmesh);
        tagged += 1;
    }
    debug!("navmesh: leaving {tagged} scattered colliders out of the mesh");
}

/// The mesh settings the host builds with (and `tests/it/navmesh.rs` asserts on).
pub fn navmesh_settings() -> NavmeshSettings {
    let mut settings = NavmeshSettings::from_agent_3d(tuning().agent_radius, AGENT_HEIGHT);
    settings.walkable_climb = tuning().max_climb;
    settings.walkable_slope_angle = tuning().max_slope_degs.to_radians();
    // The recommended 2 voxels per agent radius: coarser voxels made contour
    // simplification fail ("invalid contour") on this 200x200 m map.
    settings.cell_size_fraction = 2.0;
    // rerecast's watershed partitioning gets noisy on the huge, dead-flat
    // commune plane, and one bad simplified contour aborts the whole build
    // (janhohenheim/rerecast#64). Keeping regions big and merging eagerly
    // gives it fewer, simpler contours to chew.
    settings.min_region_size = 32;
    settings.merge_region_size = 80;
    settings
}

/// Server: how far this session's navmesh build got. A resource (not
/// `Local`s) so a new session tags its own fresh colliders and rebuilds.
#[derive(Resource, Default)]
struct NavBuild {
    tagged: bool,
    built: bool,
}

/// Session over: drop the mesh, archipelago and islands so the next session
/// builds its own (and gets a retry if this build failed).
fn forget_navmesh(
    mut commands: Commands,
    mut build: ResMut<NavBuild>,
    nav: Option<Res<VillageNav>>,
    islands: Query<Entity, With<bevy_landmass::Island>>,
) {
    *build = NavBuild::default();
    if let Some(nav) = nav {
        commands.entity(nav.archipelago).try_despawn();
    }
    for e in &islands {
        commands.entity(e).try_despawn();
    }
    commands.remove_resource::<VillageNav>();
}

/// Server, once the whole world is in and every collider that stays out of the
/// mesh is tagged: queue the navmesh build and set up the landmass archipelago
/// it will hang off. rerecast builds off the main thread.
fn build_navmesh(
    mut build: ResMut<NavBuild>,
    mut commands: Commands,
    mut generator: NavmeshGenerator,
    pending: Query<
        (Entity, &RigidBody),
        (
            With<Collider>,
            Without<avian_rerecast::ExcludeColliderFromNavmesh>,
        ),
    >,
    keep: Query<
        (),
        Or<(
            With<crate::terrain::GroundCollider>,
            With<crate::buildings::Building>,
        )>,
    >,
) {
    if build.built {
        return;
    }
    // The tagging from last frame's `tag_navmesh_exclusions` must be applied
    // before the obstacles are sampled (this frame's PostUpdate), so wait until
    // nothing that should be tagged remains.
    if pending
        .iter()
        .any(|(e, body)| body.is_static() && !keep.contains(e))
    {
        return;
    }
    build.built = true;
    let archipelago = commands
        .spawn(Archipelago3d::new(ArchipelagoOptions::from_agent_radius(
            tuning().agent_radius,
        )))
        .id();
    let mesh = generator.generate(navmesh_settings());
    commands.insert_resource(VillageNav {
        archipelago,
        mesh,
        started: Instant::now(),
        ready: false,
    });
    info!("building the villagers' navmesh…");
}

/// Server: the mesh is built — log how long it took and hang it on an island
/// so landmass can path over it.
fn on_navmesh_ready(
    ready: On<NavmeshReady>,
    mut nav: ResMut<VillageNav>,
    navmeshes: Res<Assets<bevy_rerecast::Navmesh>>,
    mut commands: Commands,
) {
    if ready.event().0 != nav.mesh.id() {
        return;
    }
    let (verts, secs) = (
        navmeshes
            .get(&nav.mesh)
            .map_or(0, |m| m.polygon.vertices.len()),
        nav.started.elapsed().as_secs_f32(),
    );
    info!("villager navmesh ready: {verts} vertices, built in {secs:.1}s");
    nav.ready = true;
    commands.spawn(Island3dBundle {
        island: bevy_landmass::Island,
        archipelago_ref: ArchipelagoRef3d::new(nav.archipelago),
        nav_mesh: NavMeshHandle3d(nav.mesh.clone()),
    });
}

/// Server: once the archipelago exists, every villager gets a landmass agent
/// (same radius as the mesh was built for) targeting wherever they are now.
fn attach_agents(
    mut commands: Commands,
    nav: Option<Res<VillageNav>>,
    fresh: Query<(Entity, &Villager), Without<Agent3d>>,
) {
    let Some(nav) = nav else {
        return;
    };
    for (entity, v) in &fresh {
        commands.entity(entity).insert((
            Agent3d::default(),
            AgentSettings {
                radius: tuning().agent_radius,
                desired_speed: tuning().speed,
                max_speed: tuning().speed,
            },
            ArchipelagoRef3d::new(nav.archipelago),
            AgentTarget3d::Point(Vec3::new(
                v.pos.x,
                crate::terrain::height_at(v.pos.x, v.pos.y),
                v.pos.y,
            )),
        ));
    }
}

/// Host only (the cover colliders live in the host's scenery): after each brain
/// step, sweep a ball along it. If it would walk into a tree, rock or wall,
/// veer to the side instead; if every way is blocked, stop and pick somewhere
/// else to go. Only runs when a villager moved, i.e. at most `AI_HZ`.
fn avoid_cover(
    spatial: avian3d::prelude::SpatialQuery,
    mut villagers: Query<(&mut Villager, &mut Brain), Changed<Villager>>,
) {
    use avian3d::prelude::{Collider, ShapeCastConfig, SpatialQueryFilter};
    let ball = Collider::sphere(0.3);
    let filter = SpatialQueryFilter::default();
    let clear = |from: Vec2, dir: Vec2, len: f32| -> bool {
        let Ok(d) = Dir3::new(Vec3::new(dir.x, 0.0, dir.y)) else {
            return true;
        };
        let at = Vec3::new(
            from.x,
            crate::terrain::height_at(from.x, from.y) + 0.9,
            from.y,
        );
        let config = ShapeCastConfig {
            ignore_origin_penetration: true,
            ..ShapeCastConfig::from_max_distance(len)
        };
        spatial
            .cast_shape(&ball, at, Quat::IDENTITY, d, &config, &filter)
            .is_none()
    };
    for (mut v, mut brain) in &mut villagers {
        let delta = v.pos - brain.prev;
        let len = delta.length();
        if len < 1e-4 || len > 2.0 {
            brain.prev = v.pos; // standing still, or pushed/teleported
            continue;
        }
        let dir = delta / len;
        let look = len + 0.4;
        if !clear(brain.prev, dir, look) {
            let detour = [0.9_f32, -0.9, 1.6, -1.6]
                .into_iter()
                .map(|a| Vec2::from_angle(a).rotate(dir))
                .find(|d| clear(brain.prev, *d, look));
            match detour {
                Some(d) => {
                    v.pos = outside_arena(brain.prev + d * len);
                    v.yaw = (-d.x).atan2(-d.y);
                }
                None => {
                    v.pos = brain.prev;
                    brain.wait = 0.5; // boxed in: go somewhere else
                }
            }
        }
        brain.prev = v.pos;
    }
}

#[derive(Component)]
struct Dressed;

/// The same rigged character system as players (people.rs).
fn dress_villagers(mut commands: Commands, new: Query<(Entity, &Villager), Without<Dressed>>) {
    for (entity, v) in &new {
        commands.entity(entity).insert((
            Dressed,
            crate::people::Gait::default(),
            Transform::from_xyz(v.pos.x, 0.0, v.pos.y).with_rotation(Quat::from_rotation_y(v.yaw)),
            Visibility::default(),
            Clickable::at_height(1.0)
                .radius(60.0)
                .labelled(if v.is_friend() { "chat up" } else { "poke" }),
            // Drawn from the same pool as players' looks, so hiders blend in.
            crate::people::CharacterLook::random(1000 + v.id as u64 * 13),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Glide to the replicated spot; sit by squashing down, bob while walking.
fn move_villagers(
    time: Res<Time>,
    mut villagers: Query<(&Villager, &mut Transform, &Children), With<Dressed>>,
    mut bodies: Query<&mut Transform, (With<crate::people::RigRoot>, Without<Dressed>)>,
    caught: Query<&super::caught::Caught>,
) {
    let t = 1.0 - (-6.0 * time.delta_secs()).exp();
    let now = time.elapsed_secs();
    for (v, mut tf, children) in &mut villagers {
        let target = Vec3::new(
            v.pos.x,
            crate::terrain::height_at(v.pos.x, v.pos.y) + super::caught::lift_of(&caught, v.id),
            v.pos.y,
        );
        let moving = tf.translation.xz().distance(v.pos) > 0.05;
        // Settle exactly onto the spot, then stop writing: an idle or seated
        // villager's whole rig stays untouched (#151).
        let facing = Quat::from_rotation_y(v.yaw);
        let at = if tf.translation.distance_squared(target) < 1e-6 {
            target
        } else {
            tf.translation.lerp(target, t)
        };
        let turn = if tf.rotation.angle_between(facing) < 1e-3 {
            facing
        } else {
            tf.rotation.slerp(facing, t)
        };
        if tf.translation != at || tf.rotation != turn {
            tf.translation = at;
            tf.rotation = turn;
        }
        for child in children.iter() {
            let Ok(mut body) = bodies.get_mut(child) else {
                continue;
            };
            let (y, sy) = if v.sitting {
                (-0.3, 0.75)
            } else if moving {
                ((now * 9.0 + v.id as f32).sin().abs() * 0.06, 1.0)
            } else {
                (0.0, 1.0)
            };
            let settle = |from: f32, to: f32| {
                if (to - from).abs() < 1e-3 {
                    to
                } else {
                    from + (to - from) * t
                }
            };
            let (by, bs) = (settle(body.translation.y, y), settle(body.scale.y, sy));
            if body.translation.y != by || body.scale.y != bs {
                body.translation.y = by;
                body.scale.y = bs;
            }
        }
    }
}

fn click_villager(
    ev: On<Clicked>,
    mut commands: Commands,
    villagers: Query<&Villager>,
    me: Option<Res<LocalPlayer>>,
    state: Query<&HideSeek>,
    roles: Query<(&Player, &Role)>,
    toast: Option<ResMut<Toast>>,
) {
    let Ok(v) = villagers.get(ev.entity) else {
        return;
    };
    commands.client_trigger(PokeVillager { id: v.id });
    let seeking = state.single().is_ok_and(|s| s.phase == Phase::Seeking)
        && me
            .as_deref()
            .is_some_and(|m| roles.iter().any(|(p, r)| p.id == m.0 && r.seeks()));
    // Friends answer from the server (`discord_friends.rs`).
    if !seeking
        && !v.is_friend()
        && let Some(mut toast) = toast
    {
        toast.show(format!("{}: {}", v.name(), greeting(v.id as usize)));
    }
}

fn greeting(n: usize) -> &'static str {
    const LINES: [&str; 5] = [
        "oh! hello. lovely day for it.",
        "have you seen my good spoon?",
        "don't tell Dave I'm out here.",
        "I'm on my way to the bonfire. probably.",
        "I am simply standing. like a person.",
    ];
    LINES[n % LINES.len()]
}

const MUTTERS: [&str; 6] = [
    "Kevin: is it soup night?",
    "Brenda: the goat's been looking at me again.",
    "Old Tom: back in my day the ruins were just ruins.",
    "Moss: I think I left the bonfire on.",
    "Kevin: nobody's hiding behind me. nope.",
    "Brenda: the pond has opinions, you know.",
];

/// Now and then, a nearby villager says something under their breath.
fn mutter(
    time: Res<Time>,
    mut next: Local<f32>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    villagers: Query<&Villager>,
    toast: Option<ResMut<Toast>>,
) {
    let now = time.elapsed_secs();
    if *next == 0.0 {
        *next = now + 20.0;
    }
    if now < *next {
        return;
    }
    *next = now + 25.0;
    let Some(me) = me else { return };
    let Some(here) = walkers
        .iter()
        .find(|(p, _)| p.id == me.0)
        .map(|(_, w)| w.pos)
    else {
        return;
    };
    // The nearest of the four mutterers in earshot gets to speak.
    let Some(speaker) = villagers
        .iter()
        .filter(|v| v.id < 4)
        .map(|v| (v.id, v.pos.distance(here.xz())))
        .filter(|(_, d)| *d < 8.0)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| name(id as usize))
    else {
        return;
    };
    let lines: Vec<&str> = MUTTERS
        .iter()
        .copied()
        .filter(|l| l.split(':').next() == Some(speaker.as_str()))
        .collect();
    if !lines.is_empty()
        && let Some(mut toast) = toast
    {
        toast.show(lines[(now as usize / 25) % lines.len()]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_and_pushes_stay_out_of_the_arena() {
        let half = arena_half() + tuning().arena_margin;
        for p in points().iter().chain(districts().iter()) {
            let q = outside_arena(*p);
            assert_eq!(*p, q, "{p} is inside the arena");
        }
        let q = outside_arena(arena_center().xz() + Vec2::new(1.0, 0.5));
        let d = q - arena_center().xz();
        assert!(d.x.abs() >= half - 1e-4 || d.y.abs() >= half - 1e-4);
    }

    #[test]
    fn districts_are_walkable_dry_land() {
        for p in districts() {
            assert!(p.x.abs() < 88.0 && p.y.abs() < 88.0, "{p} leaves the map");
            assert!(
                !crate::terrain::is_lake(p.x, p.y),
                "{p} is a swim spot, not a stroll spot"
            );
            assert!(
                crate::terrain::height_at(p.x, p.y) > -1.5,
                "{p} is at the bottom of something"
            );
        }
        // The lake stop in particular is on the shore, not in the water.
        let shore = districts()[6];
        assert!(
            (shore - crate::terrain::lake_at()).length() < 13.0,
            "{shore} isn't by the lake"
        );
        assert!(
            crate::terrain::height_at(shore.x, shore.y) > 0.0,
            "{shore} is wet"
        );
    }
}
