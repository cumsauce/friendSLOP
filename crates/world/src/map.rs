//! Maps made in the modeler: a `.bsn` scene (jackdaw's format) with the
//! area's prefabs, spots and their components, loaded through the asset
//! server (so it works in the browser too) and spawned by reflection.
//!
//! The modeler writes each placed prefab as an entity with a [`GltfSource`]
//! (which becomes a `WorldAssetRoot`, so the loading curtain waits for it),
//! each marker as an entity with its components, and the area's own
//! geometry as one more glTF. Components come from the vocabulary in
//! `crates/game/.modeler/components.json`: a type listed there must be
//! registered here (`app.register_type`) or it is skipped with a warning.
//!
//! Spawn one with `commands.spawn(Map(assets.load("maps/commune.bsn")))`;
//! the scene's entities become its children.

use bevy::asset::{AssetLoader, LoadContext, io::Reader};
use bevy::prelude::*;
use bevy::world_serialization::WorldAssetRoot;

pub struct MapPlugin;

impl Plugin for MapPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<MapText>()
            .register_asset_loader(MapLoader)
            .register_type::<GltfSource>()
            .register_type::<SceneNodeId>()
            .register_type::<Spot>()
            .register_type::<Feature>()
            .register_type::<Path>()
            .register_type::<District>()
            .register_type::<Plot>()
            .register_type::<Hangout>()
            .register_type::<Clear>()
            .register_type::<Block>()
            .register_type::<Entrance>()
            .register_type::<Hole>()
            .add_systems(Update, (spawn_maps, load_gltf_sources).chain());
    }
}

/// A map to spawn: its entities become this entity's children.
#[derive(Component, Clone, Debug)]
pub struct Map(pub Handle<MapText>);

/// Set once a [`Map`]'s scene has been spawned (or failed to parse).
#[derive(Component)]
pub struct MapSpawned;

/// A model the map places, by path in the asset folder. Same type path as
/// jackdaw's, so the modeler's scenes read as they are.
#[derive(Component, Reflect, Clone, Debug, Default)]
#[reflect(Component, Default)]
#[type_path = "jackdaw_scene_types::types"]
pub struct GltfSource {
    pub path: String,
    pub scene_index: usize,
}

/// The modeler's stable id for a map entity (a hash of map and name).
#[derive(Component, Reflect, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[reflect(Component, Default)]
#[type_path = "jackdaw_scene_types::node_id"]
pub struct SceneNodeId(pub u64);

/// A named spot on a map (a modeler marker): features find their place by
/// name instead of a constant in code.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct Spot {
    pub name: String,
}

/// Sizes of a spot's ground feature (the pond's radius, the hill's height,
/// the lake's level and bed, the village's flat rectangle).
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct Feature {
    pub radius: f32,
    pub height: f32,
    pub depth: f32,
}

/// Paths from this marker to the markers it names.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct Path {
    pub to: Vec<String>,
}

/// A district: the marker's box. Where boxes overlap the lower `order`
/// wins; `kind: "bounds"` is the map's edge.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct District {
    pub kind: String,
    pub order: u32,
}

/// A house plot: the marker's box is the footprint and its yaw the door
/// side; `upper`: the house must have an upper floor.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct Plot {
    pub upper: bool,
}

/// Where villagers hang out (a marker of kind `trip`: a far destination
/// they reach over the navmesh).
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct Hangout;

/// Keep the map's dressing (trees, bushes, rocks) this far from here.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct Clear {
    pub radius: f32,
}

/// A way into the underground (`kind`: manhole, cellar_door or trapdoor);
/// you come up facing the marker's yaw.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct Entrance {
    pub kind: String,
}

/// A hole in the ground (see `crate::holes`): the marker's box (x by z,
/// turned by its yaw), or a circle its box's width across (`round`), lined
/// `depth` metres down from `lip` above the floor.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct Hole {
    pub round: bool,
    pub depth: f32,
    pub lip: f32,
}

/// A static collider filling the marker's box (`round`: an upright
/// cylinder), its y above the ground; `on` names the entity whose ground it
/// stands on instead of its own.
#[derive(Component, Reflect, Clone, Debug, Default, PartialEq)]
#[reflect(Component, Default)]
pub struct Block {
    pub round: bool,
    pub on: String,
}

/// A `.bsn` file's text.
#[derive(Asset, TypePath, Debug)]
pub struct MapText(pub String);

#[derive(Default, TypePath)]
struct MapLoader;

impl AssetLoader for MapLoader {
    type Asset = MapText;
    type Settings = ();
    type Error = std::io::Error;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &(),
        _: &mut LoadContext<'_>,
    ) -> Result<MapText, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        String::from_utf8(bytes)
            .map(MapText)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    fn extensions(&self) -> &[&str] {
        &["bsn"]
    }
}

/// Spawns `text` into `world` under `parent`: returns the top-level
/// entities, or the parse error.
pub fn spawn_map_text(
    world: &mut World,
    text: &str,
    parent: Entity,
) -> Result<Vec<Entity>, String> {
    let ast = jackdaw_bsn::parse_bsn_text(text).map_err(|e| e.to_string())?;
    world.insert_resource(ast);
    let roots = jackdaw_bsn::apply::spawn_from_ast(world);
    jackdaw_bsn::apply::apply_dirty_ast_patches(world);
    let top: Vec<Entity> = roots
        .iter()
        .copied()
        .filter(|e| world.get::<ChildOf>(*e).is_none())
        .collect();
    for &e in &top {
        world.entity_mut(e).insert(ChildOf(parent));
    }
    for &e in &roots {
        if world.get::<Transform>(e).is_none() {
            world.entity_mut(e).insert(Transform::default());
        }
        world
            .entity_mut(e)
            .remove::<jackdaw_bsn::document::AstNodeRef>();
    }
    if let Some(ast) = world.remove_resource::<jackdaw_bsn::SceneBsnAst>() {
        jackdaw_bsn::drop_document(ast);
    }
    Ok(top)
}

fn spawn_maps(world: &mut World) {
    let pending: Vec<(Entity, Handle<MapText>)> = world
        .query_filtered::<(Entity, &Map), Without<MapSpawned>>()
        .iter(world)
        .map(|(e, m)| (e, m.0.clone()))
        .collect();
    for (e, handle) in pending {
        let Some(text) = world
            .get_resource::<Assets<MapText>>()
            .and_then(|a| a.get(&handle))
            .map(|t| t.0.clone())
        else {
            continue;
        };
        if let Err(err) = spawn_map_text(world, &text, e) {
            warn!("map {:?}: {err}", handle.path());
        }
        world.entity_mut(e).insert(MapSpawned);
    }
}

/// Put on a [`Map`] whose models stand on their own flat floor at the heights the map
/// gives them (New York's street level), instead of on the valley's terrain.
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct FlatGround;

fn load_gltf_sources(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut added: Query<(Entity, &GltfSource, &mut Transform, Option<&ChildOf>), Added<GltfSource>>,
    flat: Query<(), With<FlatGround>>,
) {
    for (e, g, mut t, parent) in &mut added {
        // A map places models above its ground, like the rest of the map, unless the map
        // has its own flat floor.
        if !parent.is_some_and(|p| flat.contains(p.parent())) {
            t.translation.y += crate::terrain::height_at(t.translation.x, t.translation.z);
        }
        commands.entity(e).insert((
            WorldAssetRoot(
                assets.load(GltfAssetLabel::Scene(g.scene_index).from_asset(g.path.clone())),
            ),
            Visibility::default(),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SALT: We're not needing `Cargo.lock` anyway. x didn't set us up for
    // success. We need more SME's. We haven't considered the problem space
    // enough. BSN syntax needs more changes. And `Children` sugaring? Ask
    // Sam "Salt" Saltman.
    const TEXT: &str = r#"// jackdaw 0.20.2 | bevy 0.20
bevy_ecs::hierarchy::Children [
    #crate1
    jackdaw_scene_types::node_id::SceneNodeId(42)
    bevy_transform::components::transform::Transform {
        translation: glam::Vec3 { x: 2.0, y: 0.0, z: 1.0 },
    }
    jackdaw_scene_types::types::GltfSource {
        path: "models/prefabs/crate.gltf",
        scene_index: 0,
    }
    --
    #well
    bevy_transform::components::transform::Transform
    friendslop_world::map::Spot {
        name: "well",
    }
]
"#;

    #[test]
    fn a_modeler_map_spawns_its_entities() {
        let mut world = World::new();
        world.init_resource::<AppTypeRegistry>();
        {
            let reg = world.resource::<AppTypeRegistry>().clone();
            let mut r = reg.write();
            r.register::<Transform>();
            r.register::<Name>();
            r.register::<GltfSource>();
            r.register::<SceneNodeId>();
            r.register::<Spot>();
        }
        let root = world.spawn_empty().id();
        let top = spawn_map_text(&mut world, TEXT, root).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(top.len(), 2);
        let mut q = world.query::<(
            &Name,
            &Transform,
            Option<&GltfSource>,
            Option<&Spot>,
            &ChildOf,
        )>();
        let mut seen = 0;
        for (name, t, g, s, parent) in q.iter(&world) {
            assert_eq!(parent.parent(), root);
            match name.as_str() {
                "crate1" => {
                    assert_eq!(t.translation, Vec3::new(2.0, 0.0, 1.0));
                    assert_eq!(
                        g.map(|g| g.path.as_str()),
                        Some("models/prefabs/crate.gltf")
                    );
                }
                "well" => assert_eq!(s.map(|s| s.name.as_str()), Some("well")),
                other => panic!("unexpected {other}"),
            }
            seen += 1;
        }
        assert_eq!(seen, 2);
    }
}
