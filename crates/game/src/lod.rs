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

use std::collections::HashMap;
use std::ops::Range;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::GltfExtras;
use bevy::light::NotShadowCaster;
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

/// A tree model: give its meshes far versions. `shadows: false` also keeps
/// it out of the shadow map (the smaller forest trees, as before).
#[derive(Component, Clone, Copy, Debug)]
pub struct TreeLod {
    pub shadows: bool,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<FarMeshes>()
        .add_observer(add_lods)
        .add_observer(modeler_lods);
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
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut far: ResMut<FarMeshes>,
) {
    let Ok(tree) = trees.get(ready.entity) else {
        return;
    };
    for child in children.iter_descendants(ready.entity) {
        let Ok((mesh, material, transform, parent)) = parts.get(child) else {
            continue;
        };
        if !tree.shadows {
            commands.entity(child).insert(NotShadowCaster);
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
