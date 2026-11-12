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
        .add_systems(Update, (build_graphs, attach_players, play_clips).chain());
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
    fn clips_match_with_or_without_a_skeleton_prefix() {
        let nodes: HashMap<String, u32> =
            [("idle".to_string(), 0), ("rig|walk".to_string(), 1)].into();
        assert_eq!(clip_node(&nodes, "idle"), Some(0));
        assert_eq!(clip_node(&nodes, "walk"), Some(1));
        assert_eq!(clip_node(&nodes, "run"), None);
    }
}
