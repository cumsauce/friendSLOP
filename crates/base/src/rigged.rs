//! Rigged modeler models with named clips (`idle`, `walk`, ...).
//!
//! Put [`Rigged`] on an entity (with a `Transform`) and this module spawns
//! the model's scene under it, builds one animation graph per file from every
//! clip in it, and wires the graph into the scene's `AnimationPlayer`. Pick
//! what plays with [`RigClip`]: change it and the rig crossfades to the new
//! clip. Looks only: a missing or broken file just draws nothing, and the
//! entity's colliders and gameplay never depend on it.
//!
//! Clip names match exactly or after a `skeleton|` prefix (`rig|walk`).
//!
//! Dynamic motion on top of the clips: once a rig is wired, its soft bones
//! (tails, ears, Dave's bobble; see [`springy`]) get a spring from the
//! modeler's simulation (`modeler_anim`), so they lag, swing and settle as
//! the clip moves the body. `modeler_bevy` runs it after the animation.

use std::collections::HashMap;
use std::time::Duration;

use bevy::asset::LoadState;
use bevy::gltf::{Gltf, GltfAssetLabel};
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;
use serde::Deserialize;

/// Ground speed of each looping clip, measured by `scripts/gait-speed.py`
/// (`data/base/gaits.ron`).
pub static GAITS: friendslop_data::Data<Gaits> = friendslop_data::Data::ron("data/base/gaits.ron");

#[derive(Deserialize, Default, Debug, Clone)]
pub struct Gaits {
    /// Slowest and fastest a clip is played to match a walker's speed
    /// (a multiple of its authored speed).
    pub min_playback: f32,
    pub max_playback: f32,
    /// Model asset path -> clip name -> m/s the clip covers at speed 1.
    pub models: HashMap<String, HashMap<String, f32>>,
}

impl Gaits {
    /// The ground speed of a model's clip, if it was measured.
    pub fn ground_speed(&self, model: &str, clip: &str) -> Option<f32> {
        self.models.get(model)?.get(clip).copied()
    }
}

/// The playback speed that keeps a walker's feet planted: it moves at
/// `moving` m/s and the clip covers `ground` m/s at speed 1. A clip with no
/// measurement (or a nonsense one) plays as authored.
pub fn playback(moving: f32, ground: Option<f32>, min: f32, max: f32) -> f32 {
    match ground {
        Some(g) if g > 0.01 && moving.is_finite() => (moving / g).clamp(min, max.max(min)),
        _ => 1.0,
    }
}

/// A rigged model drawn under this entity.
#[derive(Component, Clone, Debug)]
pub struct Rigged {
    /// Asset path of the `.gltf`, e.g. `models/creatures/bear.gltf`.
    pub path: &'static str,
}

/// Take a rig off an entity (its scene child goes separately), so inserting
/// [`Rigged`] again builds it afresh.
pub fn forget(e: &mut EntityCommands) {
    e.remove::<(Rigged, RigReady, Springs, modeler_bevy::Sway)>();
}

/// The clip a [`Rigged`] entity plays. Change it to crossfade to another.
#[derive(Component, Clone, Debug, PartialEq)]
pub struct RigClip {
    pub name: &'static str,
    /// Playback speed (1 = as authored).
    pub speed: f32,
    /// Play once and hold the last frame instead of looping.
    pub once: bool,
    /// Crossfade time from the previous clip, in seconds.
    pub fade: f32,
    /// How fast the walker is really moving (m/s), from [`RigClip::moving`]:
    /// the speed is then worked out from the model's measured gait.
    pub moving: Option<f32>,
}

impl RigClip {
    pub fn looping(name: &'static str) -> Self {
        Self {
            name,
            speed: 1.0,
            once: false,
            fade: 0.2,
            moving: None,
        }
    }
    pub fn once(name: &'static str) -> Self {
        Self {
            once: true,
            ..Self::looping(name)
        }
    }
    pub fn speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }
    /// Play at the pace that keeps the feet planted while the body moves at
    /// `m_per_s`: the clip's measured ground speed (`data/base/gaits.ron`,
    /// `scripts/gait-speed.py`) decides the playback speed. Overrides `speed`
    /// when the clip is measured; plays as authored when it isn't.
    pub fn moving(mut self, m_per_s: f32) -> Self {
        self.moving = Some(m_per_s);
        self
    }
    pub fn fade(mut self, seconds: f32) -> Self {
        self.fade = seconds;
        self
    }
}

impl Default for RigClip {
    fn default() -> Self {
        Self::looping("idle")
    }
}

/// On a [`Rigged`] entity once its scene's animation player is wired up:
/// the entity holding the `AnimationPlayer`.
#[derive(Component, Clone, Copy, Debug)]
pub struct RigReady(pub Entity);

/// The child holding the model's scene.
#[derive(Component)]
struct RigScene;

#[derive(Default)]
enum Graph {
    #[default]
    Loading,
    Failed,
    Ready(Handle<AnimationGraph>, HashMap<String, AnimationNodeIndex>),
}

#[derive(Default)]
struct Entry {
    gltf: Handle<Gltf>,
    graph: Graph,
}

/// One entry per rigged file: its glTF and the graph built from its clips.
#[derive(Resource, Default)]
struct RigLibrary(HashMap<&'static str, Entry>);

impl RigLibrary {
    fn node(&self, path: &str, clip: &str) -> Option<(Handle<AnimationGraph>, AnimationNodeIndex)> {
        let Graph::Ready(graph, nodes) = &self.0.get(path)?.graph else {
            return None;
        };
        Some((graph.clone(), clip_node(nodes, clip)?))
    }
}

/// The node of `clip` among the graph's named clips: an exact name, or one
/// ending in `|clip` (glTF exporters prefix the skeleton's name).
pub fn clip_node<V: Copy>(nodes: &HashMap<String, V>, clip: &str) -> Option<V> {
    nodes.get(clip).copied().or_else(|| {
        nodes
            .iter()
            .find(|(name, _)| name.rsplit('|').next() == Some(clip))
            .map(|(_, v)| *v)
    })
}

pub fn plugin(app: &mut App) {
    app.init_resource::<RigLibrary>()
        .add_observer(spawn_scene)
        .add_systems(
            Update,
            (build_graphs, attach_players, play_clips, add_springs).chain(),
        )
        .add_systems(
            PostUpdate,
            run_overlay
                .after(bevy::app::AnimationSystems)
                .before(bevy::transform::TransformSystems::Propagate),
        );
}

/// Soft bones that get a spring: tails, ears and bobbles.
pub fn springy(bone: &str) -> bool {
    bone.starts_with("tail") || bone.starts_with("ear") || bone == "bobble"
}

/// How the soft bones move: a loose, slightly bouncy spring.
pub const SPRING: modeler_anim::Spring = modeler_anim::Spring {
    stiffness: 60.0,
    damping: 6.0,
    gravity: 0.15,
    wind: None,
};

/// One joint of a rest pose: name, parent (index), local translation and
/// rotation. Parents come before children.
pub type RestJoint = (String, Option<usize>, Vec3, Quat);

/// The spring rig for a rest pose, or `None` if no bone is springy. Every
/// joint is a bone (so the chain's maths matches the skeleton), but only
/// springy ones move: the rest follow their animation. A soft bone points
/// at its child, or on along its parent's direction.
pub fn spring_rig(pose: &[RestJoint]) -> Option<modeler_anim::SwayRig> {
    if !pose.iter().any(|(name, ..)| springy(name)) {
        return None;
    }
    let mut world: Vec<(Vec3, Quat)> = Vec::with_capacity(pose.len());
    for (_, parent, t, r) in pose {
        let w = match parent.and_then(|p| world.get(p).copied()) {
            Some((pp, pr)) => (pp + pr * *t, pr * *r),
            None => (*t, *r),
        };
        world.push(w);
    }
    let bones = pose
        .iter()
        .enumerate()
        .map(|(i, (name, parent, ..))| {
            let head = world[i].0;
            let child = pose.iter().position(|(_, p, ..)| *p == Some(i));
            let from_parent = parent.map(|p| head - world[p].0).unwrap_or(Vec3::Y);
            let tail = match child {
                Some(c) => world[c].0,
                None => {
                    let len = (from_parent.length() * 0.6).clamp(0.05, 0.6);
                    head + from_parent.normalize_or(Vec3::Y) * len
                }
            };
            modeler_anim::SwayBone {
                name: name.clone(),
                parent: parent.map(|p| p as u32),
                head: head.to_array(),
                tail: tail.to_array(),
                radius: 0.05,
                weight: if springy(name) { 1.0 } else { 0.0 },
            }
        })
        .collect();
    Some(modeler_anim::SwayRig {
        bones,
        kind: modeler_anim::SwayKind::Spring(SPRING),
        period: None,
        clip: None,
    })
}

/// Marks a rig whose springs have been looked for.
#[derive(Component)]
struct Springs;

/// Gives a wired rig's soft bones springs (once).
fn add_springs(
    mut commands: Commands,
    rigs: Query<Entity, (With<RigReady>, Without<Springs>)>,
    children: Query<&Children>,
    skins: Query<&bevy::mesh::skinning::SkinnedMesh>,
    joints: Query<(&Name, &Transform, Option<&ChildOf>)>,
) {
    for rig in &rigs {
        commands.entity(rig).insert(Springs);
        let Some(skin) = children
            .iter_descendants(rig)
            .find_map(|c| skins.get(c).ok())
        else {
            continue;
        };
        // Parents before children.
        let set = &skin.joints;
        let parent_of = |j: Entity| {
            joints
                .get(j)
                .ok()
                .and_then(|(_, _, p)| p.map(ChildOf::parent))
        };
        let mut order: Vec<Entity> = Vec::with_capacity(set.len());
        let mut left: Vec<Entity> = set.clone();
        while !left.is_empty() {
            let before = left.len();
            let mut next = Vec::new();
            for j in left {
                let ready = parent_of(j).is_none_or(|p| !set.contains(&p) || order.contains(&p));
                if ready {
                    order.push(j);
                } else {
                    next.push(j);
                }
            }
            if next.len() == before {
                break;
            }
            left = next;
        }
        let pose: Vec<RestJoint> = order
            .iter()
            .filter_map(|&j| {
                let (name, tf, _) = joints.get(j).ok()?;
                let parent = parent_of(j).and_then(|p| order.iter().position(|&o| o == p));
                Some((
                    name.as_str().to_string(),
                    parent,
                    tf.translation,
                    tf.rotation,
                ))
            })
            .collect();
        let Some(spring) = spring_rig(&pose) else {
            continue;
        };
        let entities = order.iter().map(|&j| Some(j)).collect();
        commands
            .entity(rig)
            .insert(modeler_bevy::Sway::new(spring, entities, 0.0));
    }
}

fn spawn_scene(
    add: On<Add<Rigged>>,
    mut commands: Commands,
    rigged: Query<&Rigged>,
    assets: Option<Res<AssetServer>>,
    mut library: ResMut<RigLibrary>,
) {
    let (Ok(rig), Some(assets)) = (rigged.get(add.entity), assets) else {
        return;
    };
    library.0.entry(rig.path).or_insert_with(|| Entry {
        gltf: assets.load(rig.path),
        graph: Graph::Loading,
    });
    commands.spawn((
        RigScene,
        WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(rig.path))),
        Transform::default(),
        ChildOf(add.entity),
    ));
}

fn build_graphs(
    assets: Option<Res<AssetServer>>,
    gltfs: Res<Assets<Gltf>>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut library: ResMut<RigLibrary>,
) {
    let Some(assets) = assets else {
        return;
    };
    for entry in library.0.values_mut() {
        if !matches!(entry.graph, Graph::Loading) {
            continue;
        }
        if matches!(assets.load_state(entry.gltf.id()), LoadState::Failed(_)) {
            entry.graph = Graph::Failed;
            continue;
        }
        let Some(gltf) = gltfs.get(&entry.gltf) else {
            continue;
        };
        let names: Vec<_> = gltf.named_animations.keys().cloned().collect();
        let clips = names
            .iter()
            .filter_map(|n| gltf.named_animations.get(n).cloned());
        let (graph, nodes) = AnimationGraph::from_clips(clips);
        let nodes = names.iter().map(|n| n.to_string()).zip(nodes).collect();
        entry.graph = Graph::Ready(graphs.add(graph), nodes);
    }
}

/// Wires each rig's graph into its scene's animation player once both exist.
fn attach_players(
    mut commands: Commands,
    rigs: Query<(Entity, &Rigged, Option<&RigClip>), Without<RigReady>>,
    children: Query<&Children>,
    mut players: Query<&mut AnimationPlayer>,
    library: Res<RigLibrary>,
) {
    for (entity, rig, clip) in &rigs {
        let clip = clip.cloned().unwrap_or_default();
        let Some((graph, node)) = library
            .node(rig.path, clip.name)
            .or_else(|| library.node(rig.path, "idle"))
        else {
            continue;
        };
        let Some(holder) = children
            .iter_descendants(entity)
            .find(|&c| players.contains(c))
        else {
            continue;
        };
        let Ok(mut player) = players.get_mut(holder) else {
            continue;
        };
        let mut transitions = AnimationTransitions::new();
        let active = transitions.play(&mut player, node, Duration::ZERO);
        active.set_speed(clip_speed(&clip, rig.path));
        if !clip.once {
            active.repeat();
        }
        commands
            .entity(holder)
            .insert((AnimationGraphHandle(graph), transitions));
        commands.entity(entity).insert(RigReady(holder));
    }
}

/// How fast a clip plays: as asked, or to match the pace of a walker that
/// is [`RigClip::moving`].
fn clip_speed(clip: &RigClip, path: &str) -> f32 {
    let Some(m) = clip.moving else {
        return clip.speed;
    };
    let g = GAITS.get();
    playback(
        m,
        g.ground_speed(path, clip.name),
        g.min_playback,
        g.max_playback,
    )
}

fn play_clips(
    rigs: Query<(&Rigged, &RigClip, &RigReady), Changed<RigClip>>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    library: Res<RigLibrary>,
) {
    if crate::settings::frozen() {
        return;
    }
    for (rig, clip, ready) in &rigs {
        let Some((_, node)) = library.node(rig.path, clip.name) else {
            continue;
        };
        let Ok((mut player, mut transitions)) = players.get_mut(ready.0) else {
            continue;
        };
        let speed = clip_speed(clip, rig.path);
        // Only the speed changed on a looping clip that's already playing:
        // keep its phase instead of restarting it.
        if !clip.once
            && transitions.get_main_animation() == Some(node)
            && let Some(active) = player.animation_mut(node)
        {
            active.set_speed(speed);
            continue;
        }
        let fade = Duration::from_secs_f32(clip.fade.max(0.0));
        let active = transitions.play(&mut player, node, fade);
        active.set_speed(speed);
        if !clip.once {
            active.repeat();
        }
    }
}

// ---------------------------------------------------------------- body overlay

/// Per-profile tuning of the body overlay (`data/base/overlay.ron`).
pub static OVERLAY: friendslop_data::Data<OverlayTuning> =
    friendslop_data::Data::ron("data/base/overlay.ron");

#[derive(Deserialize, Default, Debug, Clone)]
pub struct OverlayTuning {
    pub profiles: HashMap<String, OverlayProfile>,
}

/// One rig's overlay: bones by role and the gains (see the data file).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct OverlayProfile {
    pub chest: String,
    pub neck: String,
    pub head: String,
    pub torso: String,
    pub hip: String,
    pub forward_deg: f32,
    pub max_look_deg: f32,
    pub max_pitch_deg: f32,
    pub chest_follow: f32,
    pub head_share: f32,
    pub look_stiffness: f32,
    pub lean_stiffness: f32,
    pub measure_rate: f32,
    pub min_speed: f32,
    pub max_speed: f32,
    pub roll_per_turn: f32,
    pub max_roll_deg: f32,
    pub pitch_per_accel: f32,
    pub max_lean_pitch_deg: f32,
}

/// Put on a [`Rigged`] entity: a few bones are rotated on top of the clip so the
/// body looks grounded (head leads, leans into turns and with speed changes).
/// Everything is measured from the entity's own movement.
#[derive(Component, Clone, Debug)]
#[require(OverlayState)]
pub struct Overlay {
    /// Entry in `data/base/overlay.ron`.
    pub profile: String,
    /// A world point to look at; `None` looks where it is heading.
    pub look_at: Option<Vec3>,
}

impl Overlay {
    pub fn new(profile: &str) -> Self {
        Self {
            profile: profile.to_string(),
            look_at: None,
        }
    }
}

/// A bone the overlay drives: the animation's rotation (`base`) and what we wrote last.
#[derive(Clone, Copy)]
struct Slot {
    entity: Entity,
    base: Quat,
    written: Quat,
}

#[derive(Clone, Copy, Default)]
pub struct Spring1 {
    pub x: f32,
    pub v: f32,
}

#[derive(Component, Default)]
pub struct OverlayState {
    prev: Option<(Vec3, f32)>,
    vel: Vec3,
    fwd_speed: f32,
    accel: f32,
    yaw_rate: f32,
    look_yaw: Spring1,
    look_pitch: Spring1,
    roll: Spring1,
    lean: Spring1,
    // chest, neck, head, torso, hip
    slots: [Option<Slot>; 5],
    found: bool,
}

/// One critically damped spring step (implicit, stable at any `dt`).
pub fn spring_step(s: Spring1, target: f32, omega: f32, dt: f32) -> Spring1 {
    let f = 1.0 + 2.0 * dt * omega;
    let hoo = dt * omega * omega;
    let hhoo = dt * hoo;
    let det = f + hhoo;
    Spring1 {
        x: (f * s.x + dt * s.v + hhoo * target) / det,
        v: (s.v + hoo * (target - s.x)) / det,
    }
}

/// An angle wrapped to (-PI, PI].
pub fn wrap_angle(a: f32) -> f32 {
    let t = std::f32::consts::TAU;
    let w = (a + std::f32::consts::PI).rem_euclid(t) - std::f32::consts::PI;
    if w <= -std::f32::consts::PI { w + t } else { w }
}

/// Yaw (left is positive) and pitch (up is positive) of a direction given in the
/// frame of a body that faces -Z.
pub fn look_angles(d: Vec3) -> (f32, f32) {
    let flat = Vec2::new(d.x, d.z).length();
    if flat < 1e-4 && d.y.abs() < 1e-4 {
        return (0.0, 0.0);
    }
    (wrap_angle((-d.x).atan2(-d.z)), d.y.atan2(flat))
}

/// Gaze angles clamped to the allowed turn (radians).
pub fn clamp_gaze(yaw: f32, pitch: f32, max_yaw: f32, max_pitch: f32) -> (f32, f32) {
    (
        yaw.clamp(-max_yaw.abs(), max_yaw.abs()),
        pitch.clamp(-max_pitch.abs(), max_pitch.abs()),
    )
}

/// How a gaze yaw is shared: (chest, neck, head). The bones chain, so they add up
/// to `yaw`: the chest takes `chest_follow`, neck and head split the rest.
pub fn split_yaw(yaw: f32, chest_follow: f32, head_share: f32) -> (f32, f32, f32) {
    let chest = yaw * chest_follow.clamp(0.0, 1.0);
    let rest = yaw - chest;
    let head = rest * head_share.clamp(0.0, 1.0);
    (chest, rest - head, head)
}

/// Roll into a turn (positive = lean left, as when turning left).
pub fn lean_roll(yaw_rate: f32, speed: f32, per_turn: f32, cap: f32) -> f32 {
    (yaw_rate * speed * per_turn).clamp(-cap.abs(), cap.abs())
}

/// Pitch with acceleration (positive = forward when speeding up).
pub fn lean_pitch(accel: f32, per_accel: f32, cap: f32) -> f32 {
    (accel * per_accel).clamp(-cap.abs(), cap.abs())
}

/// Exponential smoothing factor for `dt` at `rate` per second.
pub fn smooth_factor(rate: f32, dt: f32) -> f32 {
    1.0 - (-rate.max(0.0) * dt).exp()
}

/// A bone's new local rotation: `delta` (in the rig's frame) laid over the animated
/// `base`, given the bone's parent rotation in the rig's frame.
pub fn overlay_local(parent_in_rig: Quat, delta: Quat, base: Quat) -> Quat {
    (parent_in_rig.inverse() * delta * parent_in_rig * base).normalize()
}

fn find_bone(
    rig: Entity,
    name: &str,
    children: &Query<&Children>,
    names: &Query<&Name>,
    tfs: &Query<&Transform>,
) -> Option<Slot> {
    if name.is_empty() {
        return None;
    }
    let e = children
        .iter_descendants(rig)
        .find(|&c| names.get(c).is_ok_and(|n| n.as_str() == name))?;
    let q = tfs.get(e).ok()?.rotation;
    Some(Slot {
        entity: e,
        base: q,
        written: q,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_overlay(
    time: Res<Time>,
    mut rigs: Query<(Entity, &Overlay, &mut OverlayState, &GlobalTransform), With<RigReady>>,
    children: Query<&Children>,
    names: Query<&Name>,
    parents: Query<&ChildOf>,
    globals: Query<&GlobalTransform>,
    mut tfs: Query<&mut Transform>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 || crate::settings::frozen() {
        return;
    }
    let tuning = OVERLAY.get();
    for (rig, overlay, mut st, gt) in &mut rigs {
        let Some(p) = tuning.profiles.get(&overlay.profile) else {
            continue;
        };
        if !st.found {
            st.found = true;
            let roles = [&p.chest, &p.neck, &p.head, &p.torso, &p.hip];
            for (i, role) in roles.into_iter().enumerate() {
                let slot = find_bone(rig, role, &children, &names, &tfs.as_readonly());
                st.slots[i] = slot;
            }
        }
        let (pos, rot) = (gt.translation(), gt.rotation());
        let fwd_flat = (rot * Vec3::NEG_Z).xz();
        let yaw = (-fwd_flat.x).atan2(-fwd_flat.y);
        // Measure how it moves.
        let prev = st.prev.replace((pos, yaw));
        if let Some((pp, py)) = prev {
            let v = (pos - pp) / dt;
            if v.length() <= p.max_speed {
                let k = smooth_factor(p.measure_rate, dt);
                let old_speed = st.fwd_speed;
                st.vel = st.vel.lerp(v, k);
                let speed = st.vel.xz().length();
                st.fwd_speed = speed;
                let a = (speed - old_speed) / dt;
                st.accel += (a - st.accel) * k;
                let r = wrap_angle(yaw - py) / dt;
                st.yaw_rate += (r - st.yaw_rate) * k;
            }
        }
        // Where to look, in the rig's own frame.
        let moving = st.fwd_speed >= p.min_speed;
        let dir_world = match overlay.look_at {
            Some(t) if (t - pos).length_squared() > 1e-4 => Some(t - pos),
            _ if moving => Some(Vec3::new(st.vel.x, 0.0, st.vel.z)),
            _ => None,
        };
        let (ty, tp) = dir_world.map_or((0.0, 0.0), |d| {
            // Relative to where the body faces (yaw only: its lean must not skew the gaze).
            let facing = Quat::from_rotation_y(yaw + p.forward_deg.to_radians());
            let (y, pi) = look_angles(facing.inverse() * d);
            clamp_gaze(
                y,
                pi,
                p.max_look_deg.to_radians(),
                p.max_pitch_deg.to_radians(),
            )
        });
        let w = p.look_stiffness;
        st.look_yaw = spring_step(st.look_yaw, ty, w, dt);
        st.look_pitch = spring_step(st.look_pitch, tp, w, dt);
        let fade = (st.fwd_speed / p.min_speed.max(0.01)).min(1.0);
        let roll_t = lean_roll(
            st.yaw_rate,
            st.fwd_speed,
            p.roll_per_turn.to_radians(),
            p.max_roll_deg.to_radians(),
        ) * fade;
        let lean_t = lean_pitch(
            st.accel,
            p.pitch_per_accel.to_radians(),
            p.max_lean_pitch_deg.to_radians(),
        ) * fade;
        st.roll = spring_step(st.roll, roll_t, p.lean_stiffness, dt);
        st.lean = spring_step(st.lean, lean_t, p.lean_stiffness, dt);

        let (chest_y, neck_y, head_y) = split_yaw(st.look_yaw.x, p.chest_follow, p.head_share);
        let pitch = st.look_pitch.x;
        // Deltas in the rig's frame (facing -Z), turned to the model's real forward.
        let fwd = Quat::from_rotation_y(p.forward_deg.to_radians());
        let frame = |q: Quat| fwd * q * fwd.inverse();
        let body = Quat::from_rotation_z(st.roll.x) * Quat::from_rotation_x(-st.lean.x);
        let deltas = [
            frame(Quat::from_rotation_y(chest_y)),
            frame(Quat::from_rotation_y(neck_y) * Quat::from_rotation_x(pitch * 0.4)),
            frame(Quat::from_rotation_y(head_y) * Quat::from_rotation_x(pitch * 0.6)),
            frame(body),
            Quat::IDENTITY,
        ];
        let root_inv = rot.inverse();
        for (slot, delta) in st.slots.iter_mut().zip(deltas) {
            let Some(s) = slot else { continue };
            let Ok(mut tf) = tfs.get_mut(s.entity) else {
                continue;
            };
            if tf.rotation != s.written {
                s.base = tf.rotation;
            }
            let parent_in_rig = parents
                .get(s.entity)
                .ok()
                .and_then(|c| globals.get(c.parent()).ok())
                .map_or(Quat::IDENTITY, |g| root_inv * g.rotation());
            let q = overlay_local(parent_in_rig, delta, s.base);
            tf.rotation = q;
            s.written = q;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_gaits_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            GAITS.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(GAITS.set_bytes(&bytes), "{path} doesn't parse");
        let g = GAITS.get();
        assert!(g.min_playback > 0.0 && g.max_playback >= 1.0);
        for (model, clips) in &g.models {
            assert!(!clips.is_empty(), "{model} lists no clips");
            assert!(clips.values().all(|&v| v > 0.0), "{model} has a bad speed");
        }
        // The hunter's strides match how fast it moves (data/side/fulton.ron).
        let walk = g.ground_speed("models/features/fs_hunter.gltf", "walk");
        let run = g.ground_speed("models/features/fs_hunter.gltf", "run");
        assert!((walk.unwrap_or(0.0) - 3.0).abs() < 0.3, "{walk:?}");
        assert!((run.unwrap_or(0.0) - 6.5).abs() < 0.65, "{run:?}");
    }

    #[test]
    fn playback_matches_the_walkers_pace() {
        assert_eq!(playback(3.0, Some(3.0), 0.4, 2.5), 1.0);
        assert_eq!(playback(1.5, Some(3.0), 0.4, 2.5), 0.5);
        assert_eq!(playback(0.1, Some(3.0), 0.4, 2.5), 0.4);
        assert_eq!(playback(30.0, Some(3.0), 0.4, 2.5), 2.5);
        assert_eq!(playback(3.0, None, 0.4, 2.5), 1.0);
        assert_eq!(playback(3.0, Some(0.0), 0.4, 2.5), 1.0);
    }

    #[test]
    fn only_soft_bones_spring() {
        let pose = vec![
            ("root".to_string(), None, Vec3::ZERO, Quat::IDENTITY),
            ("body".to_string(), Some(0), Vec3::Y * 0.3, Quat::IDENTITY),
            (
                "tail1".to_string(),
                Some(1),
                Vec3::NEG_Z * 0.2,
                Quat::IDENTITY,
            ),
            (
                "tail2".to_string(),
                Some(2),
                Vec3::NEG_Z * 0.15,
                Quat::IDENTITY,
            ),
        ];
        let Some(rig) = spring_rig(&pose) else {
            panic!("rig")
        };
        let w: Vec<f32> = rig.bones.iter().map(|b| b.weight).collect();
        assert_eq!(w, vec![0.0, 0.0, 1.0, 1.0]);
        // tail1 points at tail2; tail2 carries on backwards.
        assert_eq!(rig.bones[2].tail, rig.bones[3].head);
        assert!(rig.bones[3].tail[2] < rig.bones[3].head[2]);
        let stiff = vec![("root".to_string(), None, Vec3::ZERO, Quat::IDENTITY)];
        assert!(spring_rig(&stiff).is_none());
    }

    #[test]
    fn clips_match_with_or_without_a_skeleton_prefix() {
        let nodes: HashMap<String, u32> =
            [("idle".to_string(), 0), ("rig|walk".to_string(), 1)].into();
        assert_eq!(clip_node(&nodes, "idle"), Some(0));
        assert_eq!(clip_node(&nodes, "walk"), Some(1));
        assert_eq!(clip_node(&nodes, "run"), None);
    }

    #[test]
    fn overlay_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            OVERLAY.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(OVERLAY.set_bytes(&bytes), "{path} doesn't parse");
        let o = OVERLAY.get();
        let p = &o.profiles["fs_hunter"];
        assert!(!p.head.is_empty() && p.look_stiffness > 0.0 && p.max_look_deg > 0.0);
        assert!(p.max_roll_deg <= 30.0 && p.max_lean_pitch_deg <= 30.0);
    }

    #[test]
    fn spring_settles_without_overshoot() {
        let mut s = Spring1::default();
        let mut peak = 0.0_f32;
        for _ in 0..240 {
            s = spring_step(s, 1.0, 20.0, 1.0 / 60.0);
            peak = peak.max(s.x);
        }
        assert!((s.x - 1.0).abs() < 1e-3 && peak <= 1.0 + 1e-4);
        // Stable even with a huge step.
        let s = spring_step(Spring1::default(), 1.0, 20.0, 5.0);
        assert!(s.x.is_finite() && s.x <= 1.0);
    }

    #[test]
    fn gaze_angles_and_limits() {
        let (y, p) = look_angles(Vec3::NEG_Z);
        assert!(y.abs() < 1e-5 && p.abs() < 1e-5);
        // Left of a body facing -Z is -X: positive yaw.
        let (y, _) = look_angles(Vec3::NEG_X);
        assert!((y - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
        let (_, p) = look_angles(Vec3::new(0.0, 1.0, -1.0));
        assert!((p - std::f32::consts::FRAC_PI_4).abs() < 1e-5);
        let (y, p) = clamp_gaze(3.0, -2.0, 1.0, 0.3);
        assert_eq!((y, p), (1.0, -0.3));
        assert_eq!(look_angles(Vec3::ZERO), (0.0, 0.0));
    }

    #[test]
    fn yaw_split_adds_up() {
        let (c, n, h) = split_yaw(1.0, 0.5, 0.6);
        assert!((c + n + h - 1.0).abs() < 1e-6);
        assert!((c - 0.5).abs() < 1e-6 && h > n);
    }

    #[test]
    fn leans_follow_turns_and_speed_and_cap() {
        assert!(lean_roll(1.0, 3.0, 0.07, 0.26) > 0.0);
        assert!(lean_roll(-1.0, 3.0, 0.07, 0.26) < 0.0);
        assert_eq!(lean_roll(10.0, 10.0, 1.0, 0.26), 0.26);
        assert!(lean_pitch(2.0, 0.03, 0.17) > 0.0 && lean_pitch(-2.0, 0.03, 0.17) < 0.0);
        assert_eq!(lean_pitch(-100.0, 1.0, 0.17), -0.17);
    }

    #[test]
    fn overlay_rotates_in_the_rigs_frame() {
        // A parent turned 90 degrees about Y: a yaw delta still turns the bone about
        // the rig's up, so world up is unchanged.
        let parent = Quat::from_rotation_x(0.7);
        let delta = Quat::from_rotation_y(0.5);
        let q = overlay_local(parent, delta, Quat::IDENTITY);
        let world = parent * q;
        let want = delta * parent;
        assert!(world.angle_between(want) < 1e-5);
        assert!(
            overlay_local(Quat::IDENTITY, Quat::IDENTITY, Quat::from_rotation_z(0.3))
                .angle_between(Quat::from_rotation_z(0.3))
                < 1e-6
        );
    }

    #[test]
    fn angles_wrap() {
        assert!((wrap_angle(3.0 * std::f32::consts::PI) - std::f32::consts::PI).abs() < 1e-5);
        assert!(wrap_angle(0.2).abs() - 0.2 < 1e-6);
    }
}
