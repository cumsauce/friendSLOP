//! Far trees draw lighter meshes (#227, WeiYa: "use ... lods ... too much
//! popin, especially seen with the trees").
//!
//! Every tree model is marked [`TreeLod`]. Once its glTF is in the world,
//! each of its meshes gets two simplified twins (`art::lod`: trunks and
//! branches clustered, leaf cards thinned and grown to keep the canopy full),
//! built once per mesh and shared by every tree using it so they still batch.
//! Each level dithers into the next across a band of distance ([`SWITCH`]),
//! and the last draws all the way to the fog. Trees used to vanish at 35 m
//! when they were tagged as small props (#151); now they never pop. Purely
//! local.
//!
//! Models exported by the modeler with `lods` carry their own lighter
//! levels: sibling nodes `<part>.lod1`, `.lod2` whose glTF extras say the
//! camera distances each is for (`{"lod": {"level", "near", "far"}}`).
//! [`modeler_lods`] turns those into a [`VisibilityRange`] on the node's
//! meshes, crossfading over [`FADE`] metres, so every such model swaps to
//! its lighter meshes with distance on its own.
//!
//! Trees sway in the wind: the modeler rigs each one along its L-system
//! branches and `modeler_bevy` bends the bones live, every tree with its own
//! phase. The far twins are skinned to the same joints, so they bend with
//! the tree. Only trees within [`SWAY_RANGE`] move ([`sway_near`]): past it a
//! breeze is invisible and each moving tree re-propagates its joints. The
//! wind blows harder in the great storm's gusts and stops in the Stillness
//! ([`steer_wind`]).

use std::collections::HashMap;
use std::ops::Range;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::GltfExtras;
use bevy::light::NotShadowCaster;
use bevy::mesh::skinning::SkinnedMesh;
use bevy::mesh::{Indices, VertexAttributeValues};
use bevy::prelude::*;
use bevy::world_serialization::WorldInstanceReady;

/// The distance bands (metres from the mesh's origin, near the trunk's
/// foot) over which each level crossfades into the next.
pub const SWITCH: [Range<f32>; 2] = [20.0..28.0, 55.0..70.0];
/// The simplified levels: (keep details down to 1/n of the tree's size,
/// keep one leaf card in n).
const LEVELS: [(f32, usize); 2] = [(40.0, 2), (16.0, 4)];
/// Further than anything is ever drawn (fog and the far plane end it first).
const FOREVER: f32 = 100_000.0;

/// Leaf tuning: `assets/data/game/trees.ron` (read at run time).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Trees {
    /// Diffuse transmission for leaf materials (the models' own is replaced).
    pub leaf_transmission: f32,
    /// Emissive glow from the leaf's base colour, in place of transmission.
    pub leaf_glow: f32,
}

pub static TREES: friendslop_data::Data<Trees> = friendslop_data::Data::ron("data/game/trees.ron");

/// Pure: a leaf material made cheap. Diffuse transmission runs the whole
/// lighting (every light, the environment map) a second time per pixel, and
/// a canopy close up is all leaf pixels, so it's set from [`Trees`], with a
/// faint glow of the leaf's colour standing in for the light through it.
pub fn cheap_leaf(m: &mut StandardMaterial, t: &Trees) {
    if m.diffuse_transmission <= 0.0 {
        return;
    }
    m.diffuse_transmission = t.leaf_transmission;
    if t.leaf_glow > 0.0 {
        m.emissive = (m.base_color.to_linear() * t.leaf_glow).with_alpha(1.0);
    }
}

/// A tree model: give its meshes far versions (data lives in
/// `friendslop-core`; the systems below drive it).
pub use friendslop_core::TreeLod;

pub fn plugin(app: &mut App) {
    app.init_resource::<FarMeshes>()
        .add_observer(add_lods)
        .add_observer(modeler_lods)
        .add_observer(calm_trees)
        .add_systems(Update, (sway_near, steer_wind));
}

/// Trees further than this from the camera hold still (metres).
pub const SWAY_RANGE: f32 = 40.0;
/// The wind's strength between gusts (0.5 is the breeze the trees were
/// rigged with) and at the height of a great-storm gust.
const BREEZE: f32 = 0.5;
const GALE: f32 = 1.6;

/// Whether a tree at `distance` metres from the camera sways.
pub fn sways(distance: f32) -> bool {
    distance < SWAY_RANGE
}

/// The wind strength for a great-storm gust of `gust` (0 calm .. 1 peak).
pub fn wind_strength(gust: f32) -> f32 {
    BREEZE + (GALE - BREEZE) * gust.clamp(0.0, 1.0)
}

/// Tree wind as rigged swings each bone at about 1 Hz and flutters the
/// twigs at 3-6 Hz, which reads as vibration at game scale. Slow it to a
/// lazy sway with a soft leaf flutter.
pub const TREE_TEMPO: f32 = 0.4;
pub const TREE_FLUTTER: f32 = 0.3;

/// The wind settings a tree actually sways with (tempo and flutter tamed).
pub fn calm(w: &modeler_anim::Wind) -> modeler_anim::Wind {
    modeler_anim::Wind {
        frequency: w.frequency * TREE_TEMPO,
        flutter: w.flutter * TREE_FLUTTER,
        ..w.clone()
    }
}

fn calm_trees(add: On<Add<modeler_bevy::Sway>>, mut rigs: Query<&mut modeler_bevy::Sway>) {
    let Ok(mut sway) = rigs.get_mut(add.entity) else {
        return;
    };
    if let modeler_anim::SwayKind::Wind(w) = &sway.rig.kind {
        let calmer = calm(w);
        sway.rig.kind = modeler_anim::SwayKind::Wind(calmer);
    }
}

fn sway_near(
    camera: Query<&GlobalTransform, (With<Camera3d>, With<crate::world::MainCamera>)>,
    mut rigs: Query<(&mut modeler_bevy::Sway, &GlobalTransform)>,
) {
    let Some(eye) = camera.iter().next().map(GlobalTransform::translation) else {
        return;
    };
    let frozen = crate::screenshot::frozen();
    for (mut sway, at) in &mut rigs {
        // Screenshot captures hold every tree still (goldens, S3).
        let live = !frozen && sways(eye.distance(at.translation()));
        if sway.live != live {
            sway.live = live;
        }
    }
}

fn steer_wind(
    gusts: Option<Res<crate::features::great_storm::Gusts>>,
    still: Query<&crate::features::stillness::Stillness>,
    mut wind: ResMut<modeler_bevy::Wind>,
) {
    if crate::screenshot::frozen() {
        if wind.enabled {
            wind.enabled = false;
        }
        return;
    }
    let strength = wind_strength(gusts.map_or(0.0, |g| g.strength()));
    let enabled = !crate::features::stillness::is_frozen(&still);
    if wind.strength != Some(strength) || wind.enabled != enabled {
        wind.strength = Some(strength);
        wind.enabled = enabled;
    }
}

/// Far meshes already built, by detailed mesh: one per [`LEVELS`] entry
/// (`None`: not worth one).
#[derive(Resource, Default)]
struct FarMeshes(HashMap<AssetId<Mesh>, [Option<Handle<Mesh>>; 2]>);

/// The visibility of level `level` (0 = the detailed mesh) when the levels
/// in `present` exist: from its own band to the next present level's.
pub fn range(level: usize, present: [bool; 3]) -> VisibilityRange {
    let start = match level {
        0 => 0.0..0.0,
        l => SWITCH[l - 1].clone(),
    };
    let end = (level + 1..3)
        .find(|&l| present[l])
        .map_or(FOREVER..FOREVER, |l| SWITCH[l - 1].clone());
    VisibilityRange {
        start_margin: start,
        end_margin: end,
        use_aabb: false,
    }
}

/// Copies `values` at the given vertex indices, for the attribute formats
/// glTF models use.
fn pick(values: &VertexAttributeValues, at: &[u32]) -> Option<VertexAttributeValues> {
    use VertexAttributeValues as V;
    fn take<T: Copy>(v: &[T], at: &[u32]) -> Option<Vec<T>> {
        at.iter().map(|&i| v.get(i as usize).copied()).collect()
    }
    Some(match values {
        V::Float32(v) => V::Float32(take(v, at)?),
        V::Float32x2(v) => V::Float32x2(take(v, at)?),
        V::Float32x3(v) => V::Float32x3(take(v, at)?),
        V::Float32x4(v) => V::Float32x4(take(v, at)?),
        V::Uint16x4(v) => V::Uint16x4(take(v, at)?),
        V::Unorm16x4(v) => V::Unorm16x4(take(v, at)?),
        V::Unorm8x4(v) => V::Unorm8x4(take(v, at)?),
        _ => return None,
    })
}

/// A far version of `mesh` (see [`friendslop_art::lod::simplify`]), if it's
/// an indexed triangle list (with its data still on the CPU) that simplifies
/// well.
pub fn far_mesh(mesh: &Mesh, detail: f32, keep_one_in: usize) -> Option<Mesh> {
    if mesh.primitive_topology() != bevy::mesh::PrimitiveTopology::TriangleList {
        return None;
    }
    let indices: Vec<u32> = mesh.try_indices().ok()?.iter().map(|i| i as u32).collect();
    let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.try_attribute(Mesh::ATTRIBUTE_POSITION).ok()
    else {
        return None;
    };
    let normals = match mesh.try_attribute(Mesh::ATTRIBUTE_NORMAL).ok() {
        Some(VertexAttributeValues::Float32x3(n)) => Some(n.as_slice()),
        _ => None,
    };
    let lod = friendslop_art::lod::simplify(positions, normals, &indices, detail, keep_one_in)?;
    debug!(
        "lod: {} -> {} triangles",
        indices.len() / 3,
        lod.indices.len() / 3
    );
    let mut far = Mesh::new(
        bevy::mesh::PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    for (attribute, values) in mesh.try_attributes().ok()? {
        let values = if attribute.id == Mesh::ATTRIBUTE_POSITION.id {
            VertexAttributeValues::Float32x3(lod.positions.clone())
        } else {
            pick(values, &lod.source)?
        };
        far.try_insert_attribute(*attribute, values).ok()?;
    }
    far.insert_indices(Indices::U32(lod.indices));
    Some(far)
}

fn add_lods(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    trees: Query<&TreeLod>,
    children: Query<&Children>,
    parts: Query<(
        &Mesh3d,
        &MeshMaterial3d<StandardMaterial>,
        &Transform,
        &ChildOf,
        Option<&SkinnedMesh>,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut far: ResMut<FarMeshes>,
    mut leaves_done: Local<std::collections::HashSet<AssetId<StandardMaterial>>>,
) {
    let Ok(tree) = trees.get(ready.entity) else {
        return;
    };
    let tuning = TREES.get();
    for child in children.iter_descendants(ready.entity) {
        let Ok((mesh, material, transform, parent, skin)) = parts.get(child) else {
            continue;
        };
        if !tree.shadows {
            commands.entity(child).insert(NotShadowCaster);
        }
        // Materials are shared by every tree using them: change each once.
        if leaves_done.insert(material.0.id())
            && let Some(mut m) = materials.get_mut(material.0.id())
        {
            cheap_leaf(&mut m, &tuning);
        }
        let id = mesh.0.id();
        let levels = far
            .0
            .entry(id)
            .or_insert_with(|| {
                LEVELS.map(|(detail, keep)| {
                    meshes
                        .get(id)
                        .and_then(|m| far_mesh(m, detail, keep))
                        .map(|m| meshes.add(m))
                })
            })
            .clone();
        if levels.iter().all(Option::is_none) {
            continue;
        }
        let present = [true, levels[0].is_some(), levels[1].is_some()];
        commands.entity(child).insert(range(0, present));
        for (i, handle) in levels.into_iter().enumerate() {
            let Some(handle) = handle else {
                continue;
            };
            let mut twin = commands.spawn((
                Mesh3d(handle),
                material.clone(),
                *transform,
                range(i + 1, present),
                ChildOf(parent.parent()),
            ));
            if !tree.shadows {
                twin.insert(NotShadowCaster);
            }
            // A rigged tree's twins keep its joints and weights (`pick`
            // copies them) and bend with it.
            if let Some(skin) = skin {
                twin.insert(skin.clone());
            }
        }
    }
}

/// How far (metres) a modeler LOD level crossfades into the next.
pub const FADE: f32 = 4.0;

/// The distances a modeler-exported node is drawn over, from its glTF extras
/// (`{"lod": {"level": 1, "near": 20, "far": 60}}`): `None` for a node
/// without LOD extras. `far` of the last level is effectively forever.
pub fn modeler_lod(extras: &str) -> Option<VisibilityRange> {
    let value: serde_json::Value = serde_json::from_str(extras).ok()?;
    let lod = value.get("lod")?;
    let near = lod
        .get("near")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0) as f32;
    let far = lod
        .get("far")
        .and_then(serde_json::Value::as_f64)
        .map_or(FOREVER, |f| f as f32);
    let far = if far.is_finite() && far > near {
        far
    } else {
        FOREVER
    };
    // Each level starts fading in where the previous one starts fading out,
    // so the two overlap over FADE metres and never leave a gap.
    let start = if near > 0.0 {
        near..near + FADE
    } else {
        0.0..0.0
    };
    let end = if far >= FOREVER {
        FOREVER..FOREVER
    } else {
        far..far + FADE
    };
    Some(VisibilityRange {
        start_margin: start,
        end_margin: end,
        use_aabb: false,
    })
}

/// Gives the meshes of every modeler LOD node in a freshly spawned model
/// the distances they're drawn over.
fn modeler_lods(
    ready: On<WorldInstanceReady>,
    mut commands: Commands,
    children: Query<&Children>,
    nodes: Query<&GltfExtras>,
    meshes: Query<(), With<Mesh3d>>,
) {
    for node in children.iter_descendants(ready.entity) {
        let Some(range) = nodes.get(node).ok().and_then(|e| modeler_lod(&e.value)) else {
            continue;
        };
        if meshes.contains(node) {
            commands.entity(node).insert(range.clone());
        }
        for part in children.iter_descendants(node) {
            if meshes.contains(part) {
                commands.entity(part).insert(range.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trees_file_parses_and_leaves_get_cheap() {
        let path = format!("{}/assets/{}", env!("CARGO_MANIFEST_DIR"), TREES.path());
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(TREES.set_bytes(&bytes), "{path} doesn't parse");
        let t = TREES.get();
        assert!((0.0..=1.0).contains(&t.leaf_transmission) && t.leaf_glow >= 0.0);
        let mut leaf = StandardMaterial {
            diffuse_transmission: 0.25,
            ..default()
        };
        cheap_leaf(
            &mut leaf,
            &Trees {
                leaf_transmission: 0.0,
                leaf_glow: 0.1,
            },
        );
        assert_eq!(leaf.diffuse_transmission, 0.0);
        assert!(leaf.emissive.red > 0.0);
        // Bark (no transmission) is left alone.
        let mut bark = StandardMaterial::default();
        cheap_leaf(
            &mut bark,
            &Trees {
                leaf_transmission: 0.0,
                leaf_glow: 0.1,
            },
        );
        assert_eq!(bark.emissive, LinearRgba::BLACK);
    }

    #[test]
    fn trees_sway_slower_and_flutter_less() {
        let w = modeler_anim::Wind::default();
        let c = calm(&w);
        assert!(c.frequency < w.frequency && c.flutter <= w.flutter);
        assert_eq!(c.strength, w.strength);
    }

    #[test]
    fn modeler_lod_extras_become_ranges() {
        let full = r#"{"part":"bed","lod":{"level":0,"near":0,"far":20}}"#;
        let Some(r0) = modeler_lod(full) else {
            panic!("level 0 has a range");
        };
        assert_eq!(r0.start_margin, 0.0..0.0);
        assert_eq!(r0.end_margin, 20.0..20.0 + FADE);
        let Some(r1) = modeler_lod(r#"{"lod":{"level":1,"near":20,"far":60}}"#) else {
            panic!("level 1 has a range");
        };
        // Level 1 fades in exactly while level 0 fades out.
        assert_eq!(r1.start_margin, r0.end_margin);
        let Some(r2) = modeler_lod(r#"{"lod":{"level":2,"near":60}}"#) else {
            panic!("the last level has a range");
        };
        assert!(r2.end_margin.start >= FOREVER);
        assert!(modeler_lod(r#"{"part":"bed"}"#).is_none());
        assert!(modeler_lod("not json").is_none());
    }

    #[test]
    fn far_meshes_keep_their_attributes() {
        // A finely cut sphere simplifies; its UVs and normals come along.
        let mesh = Sphere::new(1.0).mesh().uv(64, 48);
        let Some(far) = far_mesh(&mesh, 40.0, 2) else {
            panic!("a 6k-triangle sphere should simplify");
        };
        let n = far.count_vertices();
        assert!(n > 0 && n < mesh.count_vertices());
        assert!(far.attribute(Mesh::ATTRIBUTE_UV_0).is_some());
        assert!(far.attribute(Mesh::ATTRIBUTE_NORMAL).is_some());
        let tris = |m: &Mesh| m.indices().map_or(0, |i| i.len() / 3);
        assert!(tris(&far) * 3 <= tris(&mesh) * 2);
    }

    #[test]
    fn far_meshes_keep_their_skin_weights() {
        let mut mesh = Sphere::new(1.0).mesh().uv(64, 48);
        let n = mesh.count_vertices();
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_INDEX,
            VertexAttributeValues::Uint16x4(vec![[0, 1, 0, 0]; n]),
        );
        mesh.insert_attribute(
            Mesh::ATTRIBUTE_JOINT_WEIGHT,
            vec![[0.5f32, 0.5, 0.0, 0.0]; n],
        );
        let Some(far) = far_mesh(&mesh, 40.0, 2) else {
            panic!("a skinned sphere still simplifies");
        };
        assert!(far.attribute(Mesh::ATTRIBUTE_JOINT_INDEX).is_some());
        assert!(far.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT).is_some());
    }

    #[test]
    fn near_trees_sway_and_gusts_blow_harder() {
        assert!(sways(5.0));
        assert!(!sways(SWAY_RANGE + 1.0));
        assert_eq!(wind_strength(0.0), BREEZE);
        assert!(wind_strength(1.0) > wind_strength(0.5));
        assert_eq!(wind_strength(3.0), GALE);
    }

    #[test]
    fn ranges_hand_over_without_a_gap() {
        let all = [true; 3];
        assert_eq!(range(0, all).end_margin, range(1, all).start_margin);
        assert_eq!(range(1, all).end_margin, range(2, all).start_margin);
        assert!(range(2, all).end_margin.start >= FOREVER);
        // A missing middle level: the detailed one hands straight to the last.
        let gap = [true, false, true];
        assert_eq!(range(0, gap).end_margin, range(2, gap).start_margin);
        // No far level at all: the detailed one stays forever.
        assert!(range(1, [true, true, false]).end_margin.start >= FOREVER);
    }
}
