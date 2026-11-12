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

/// A rigged model drawn under this entity.
#[derive(Component, Clone, Debug)]
pub struct Rigged {
    /// Asset path of the `.gltf`, e.g. `models/creatures/bear.gltf`.
    pub path: &'static str,
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
}

impl RigClip {
    pub fn looping(name: &'static str) -> Self {
        Self {
            name,
            speed: 1.0,
            once: false,
            fade: 0.2,
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
        active.set_speed(clip.speed);
        if !clip.once {
            active.repeat();
        }
        commands
            .entity(holder)
            .insert((AnimationGraphHandle(graph), transitions));
        commands.entity(entity).insert(RigReady(holder));
    }
}

fn play_clips(
    rigs: Query<(&Rigged, &RigClip, &RigReady), Changed<RigClip>>,
    mut players: Query<(&mut AnimationPlayer, &mut AnimationTransitions)>,
    library: Res<RigLibrary>,
) {
    for (rig, clip, ready) in &rigs {
        let Some((_, node)) = library.node(rig.path, clip.name) else {
            continue;
        };
        let Ok((mut player, mut transitions)) = players.get_mut(ready.0) else {
            continue;
        };
        // Only the speed changed on a looping clip that's already playing:
        // keep its phase instead of restarting it.
        if !clip.once
            && transitions.get_main_animation() == Some(node)
            && let Some(active) = player.animation_mut(node)
        {
            active.set_speed(clip.speed);
            continue;
        }
        let fade = Duration::from_secs_f32(clip.fade.max(0.0));
        let active = transitions.play(&mut player, node, fade);
        active.set_speed(clip.speed);
        if !clip.once {
            active.repeat();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
