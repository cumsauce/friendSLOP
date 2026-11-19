//! A navmesh for every area (`areas.rs`), so anything can find its way
//! around a place: the host builds each area's mesh from that area's own
//! colliders once they've loaded (rerecast, off the main thread) and hangs it
//! on its own landmass archipelago, and drops both when the area unloads.
//! An agent joins an area by taking [`AreaNavs::archipelago`]
//! (`ArchipelagoRef3d`). A feature that builds an area's mesh itself (the
//! villagers build the commune's) registers it with [`AreaNavs::provide`] and
//! this leaves that area alone. Tuning: `data/base/nav.ron`.

use std::collections::HashMap;

use bevy::platform::collections::HashSet;

use avian3d::prelude::{Collider, RigidBody};
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy_landmass::{Archipelago3d, ArchipelagoOptions, ArchipelagoRef3d, FromAgentRadius};
use bevy_replicon::prelude::*;
use bevy_rerecast::prelude::{NavmeshGenerator, NavmeshReady, NavmeshSettings};
use serde::Deserialize;

use crate::AppState;
use crate::areas::{self, Layer, Loaded, Tags};

/// Tuning: `assets/data/base/nav.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The walker the meshes are built for: radius and height (m).
    pub agent_radius: f32,
    pub agent_height: f32,
    /// Steps it can climb (m) and the steepest ground it walks (degrees).
    pub max_climb: f32,
    pub max_slope_degs: f32,
    /// Seconds an area's colliders get to settle after loading before its
    /// mesh is built from them.
    pub settle_secs: f32,
    /// rerecast's region sizes (voxels): bigger regions, fewer contours.
    pub min_region_size: u16,
    pub merge_region_size: u16,
    /// Areas a feature builds the mesh for itself (the villagers: the commune).
    pub provided: Vec<String>,
    /// Areas nobody walks in (the surf course: you slide), so no mesh at all.
    #[serde(default)]
    pub skip: Vec<String>,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/base/nav.ron");

/// The settings an area's mesh is built with.
pub fn settings() -> NavmeshSettings {
    let t = TUNING.get();
    let mut s = NavmeshSettings::from_agent_3d(t.agent_radius, t.agent_height);
    s.walkable_climb = t.max_climb;
    s.walkable_slope_angle = t.max_slope_degs.to_radians();
    // Two voxels per agent radius: coarser ones made contour simplification
    // fail (janhohenheim/rerecast#64).
    s.cell_size_fraction = 2.0;
    s.min_region_size = t.min_region_size;
    s.merge_region_size = t.merge_region_size;
    s
}

/// An archipelago's options: the agent radius, and a generous reach for
/// finding the mesh under a body (a body on a step, a ledge or mid-hop would
/// otherwise count as off the mesh and get no path).
pub fn archipelago_options(radius: f32) -> ArchipelagoOptions<bevy_landmass::coords::ThreeD> {
    let mut o = ArchipelagoOptions::<bevy_landmass::coords::ThreeD>::from_agent_radius(radius);
    o.point_sample_distance.horizontal_distance = 3.0;
    o.point_sample_distance.distance_above = 2.0;
    o.point_sample_distance.distance_below = 3.0;
    o
}

/// A rerecast mesh as a landmass one, ready to hang on an island. Landmass
/// refuses a whole mesh over a single bad polygon (a sliver rerecast leaves
/// flat, wound backwards or a hair concave), which left every agent off the
/// mesh; this drops each polygon the validator names and tries again.
pub fn to_landmass(mesh: &bevy_rerecast::Navmesh) -> Option<bevy_landmass::NavMesh3d> {
    use bevy_landmass::ValidationError as E;
    let mut raw = landmass_rerecast::convert_rerecast_navmesh_to_landmass_navmesh(mesh);
    let mut dropped = 0;
    loop {
        let bad = match raw.clone().validate() {
            Ok(valid) => {
                if dropped > 0 {
                    debug!("nav: dropped {dropped} bad polygons from a navmesh");
                }
                return Some(bevy_landmass::NavMesh3d {
                    nav_mesh: std::sync::Arc::new(valid),
                });
            }
            Err(
                E::ConcavePolygon(i)
                | E::NotEnoughVerticesInPolygon(i)
                | E::InvalidVertexIndexInPolygon(i)
                | E::DegenerateEdgeInPolygon(i),
            ) => i,
            Err(e) => {
                error!("nav: landmass refused a navmesh: {e}");
                return None;
            }
        };
        if bad >= raw.polygons.len() || dropped > 10_000 {
            return None;
        }
        raw.polygons.remove(bad);
        if bad < raw.polygon_type_indices.len() {
            raw.polygon_type_indices.remove(bad);
        }
        if let Some(h) = raw.height_mesh.as_mut()
            && bad < h.polygons.len()
        {
            h.polygons.remove(bad);
        }
        dropped += 1;
    }
}

/// Hang a built rerecast mesh on `archipelago` (see [`to_landmass`]).
pub fn spawn_island(
    commands: &mut Commands,
    rerecast: &Assets<bevy_rerecast::Navmesh>,
    landmass: &mut Assets<bevy_landmass::NavMesh3d>,
    mesh: &Handle<bevy_rerecast::Navmesh>,
    archipelago: Entity,
) -> Option<Entity> {
    let converted = to_landmass(rerecast.get(mesh)?)?;
    let handle = landmass.add(converted);
    Some(
        commands
            .spawn((
                bevy_landmass::Island3dBundle {
                    island: bevy_landmass::Island,
                    archipelago_ref: ArchipelagoRef3d::new(archipelago),
                    nav_mesh: bevy_landmass::NavMeshHandle(handle),
                },
                areas::Global,
            ))
            .id(),
    )
}

/// Always part of the navmesh, whatever its size: walls and closed doors a
/// path must go round (the commune's mesh leaves small props out).
#[derive(Component, Default, Clone, Copy, Debug)]
pub struct NavSolid;

/// One area's navigation.
#[derive(Debug)]
pub struct AreaNav {
    /// The landmass archipelago its agents join.
    pub archipelago: Entity,
    /// The mesh is built and hung on an island: agents can path now.
    pub ready: bool,
    /// Built by a feature (see [`AreaNavs::provide`]), not here.
    provided: bool,
    mesh: Option<Handle<bevy_rerecast::Navmesh>>,
    island: Option<Entity>,
    started: Instant,
}

/// Server: every area's navigation, by area id.
#[derive(Resource, Default, Debug)]
pub struct AreaNavs {
    navs: HashMap<String, AreaNav>,
    /// When each loaded area without a mesh first had its colliders.
    seen: HashMap<String, f32>,
}

impl AreaNavs {
    /// `area`'s archipelago once its mesh is ready.
    pub fn archipelago(&self, area: &str) -> Option<Entity> {
        self.navs
            .get(area)
            .filter(|n| n.ready)
            .map(|n| n.archipelago)
    }

    /// A feature built `area`'s mesh itself (`ready` once it's on an island).
    pub fn provide(&mut self, area: &str, archipelago: Entity, ready: bool) {
        self.navs.insert(
            area.to_owned(),
            AreaNav {
                archipelago,
                ready,
                provided: true,
                mesh: None,
                island: None,
                started: Instant::now(),
            },
        );
    }

    /// The feature's mesh for `area` went.
    pub fn forget(&mut self, area: &str) {
        if self.navs.get(area).is_some_and(|n| n.provided) {
            self.navs.remove(area);
        }
    }
}

pub fn plugin(app: &mut App) {
    if !app.is_plugin_added::<avian_rerecast::AvianBackendPlugin>() {
        app.add_plugins((
            bevy_rerecast::prelude::NavmeshPlugins::default(),
            avian_rerecast::AvianBackendPlugin::default(),
            bevy_landmass::Landmass3dPlugin::default(),
            landmass_rerecast::LandmassRerecastPlugin::default(),
        ));
    }
    app.init_resource::<AreaNavs>()
        .add_observer(on_ready)
        .add_systems(
            Update,
            (build, drop_unloaded)
                .run_if(in_state(ServerState::Running))
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnExit(ServerState::Running), forget_all);
}

/// Server: queue a mesh for each loaded area that has none, once its
/// colliders have settled, from just that area's static colliders.
#[allow(clippy::too_many_arguments)]
fn build(
    mut navs: ResMut<AreaNavs>,
    loaded: Res<Loaded>,
    time: Res<Time>,
    mut generator: NavmeshGenerator,
    mut commands: Commands,
    colliders: Query<
        (Entity, Option<&RigidBody>),
        (
            With<Collider>,
            Without<avian_rerecast::ExcludeColliderFromNavmesh>,
        ),
    >,
    bodies: Query<&RigidBody>,
    parents: Query<&ChildOf>,
    tags: Query<Tags>,
) {
    let now = time.elapsed_secs();
    let settle = TUNING.get().settle_secs;
    let list = areas::areas();
    for area in &list.areas {
        let id = area.id.as_str();
        let t = TUNING.get();
        let theirs = t.provided.iter().chain(&t.skip).any(|p| p == id);
        if theirs
            || navs.navs.contains_key(id)
            || !loaded.is(id, Layer::Sim)
            || loaded.pending(id, Layer::Sim)
        {
            navs.seen.remove(id);
            continue;
        }
        let first = *navs.seen.entry(id.to_owned()).or_insert(now);
        if now - first < settle {
            continue;
        }
        let mine: HashSet<Entity> = colliders
            .iter()
            // A collider's body is its own or its parent's: only static ones
            // (a train's car moves).
            .filter(|(e, body)| {
                body.copied()
                    .or_else(|| {
                        parents
                            .get(*e)
                            .ok()
                            .and_then(|p| bodies.get(p.parent()).ok().copied())
                    })
                    .is_none_or(|b| b.is_static())
            })
            .filter(|(e, _)| areas::owner(*e, &parents, &tags) == Some(Some(id)))
            .map(|(e, _)| e)
            .collect();
        navs.seen.remove(id);
        if mine.is_empty() {
            debug!("nav: {id} has no static colliders yet");
            continue;
        }
        let mut s = settings();
        s.filter = Some(mine);
        let archipelago = commands
            .spawn((
                Archipelago3d::new(archipelago_options(TUNING.get().agent_radius)),
                areas::Global,
            ))
            .id();
        let mesh = generator.generate(s);
        info!("nav: building {id}'s navmesh");
        navs.navs.insert(
            id.to_owned(),
            AreaNav {
                archipelago,
                ready: false,
                provided: false,
                mesh: Some(mesh),
                island: None,
                started: Instant::now(),
            },
        );
    }
}

/// A mesh is ready: hang it on its area's archipelago.
fn on_ready(
    ready: On<NavmeshReady>,
    mut navs: ResMut<AreaNavs>,
    meshes: Res<Assets<bevy_rerecast::Navmesh>>,
    mut landmass: ResMut<Assets<bevy_landmass::NavMesh3d>>,
    mut commands: Commands,
) {
    for (id, nav) in navs.navs.iter_mut() {
        let Some(mesh) = nav.mesh.clone() else {
            continue;
        };
        if mesh.id() != ready.event().0 {
            continue;
        }
        let verts = meshes.get(&mesh).map_or(0, |m| m.polygon.vertices.len());
        info!(
            "nav: {id}'s navmesh ready: {verts} vertices in {:.1}s",
            nav.started.elapsed().as_secs_f32()
        );
        let Some(island) = spawn_island(
            &mut commands,
            &meshes,
            &mut landmass,
            &mesh,
            nav.archipelago,
        ) else {
            continue;
        };
        nav.island = Some(island);
        nav.ready = true;
    }
}

/// Server: an area unloaded: its mesh and archipelago go with it.
fn drop_unloaded(mut navs: ResMut<AreaNavs>, loaded: Res<Loaded>, mut commands: Commands) {
    let gone: Vec<String> = navs
        .navs
        .iter()
        .filter(|(id, n)| !n.provided && !loaded.is(id, Layer::Sim))
        .map(|(id, _)| id.clone())
        .collect();
    for id in gone {
        if let Some(n) = navs.navs.remove(&id) {
            despawn(&mut commands, &n);
            info!("nav: dropped {id}'s navmesh");
        }
    }
}

fn despawn(commands: &mut Commands, n: &AreaNav) {
    commands.entity(n.archipelago).try_despawn();
    if let Some(i) = n.island {
        commands.entity(i).try_despawn();
    }
}

fn forget_all(mut navs: ResMut<AreaNavs>, mut commands: Commands) {
    for n in navs.navs.values().filter(|n| !n.provided) {
        despawn(&mut commands, n);
    }
    *navs = AreaNavs::default();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        let t = TUNING.get();
        assert!(t.agent_radius > 0.0 && t.agent_height > t.agent_radius);
        assert!(t.max_climb > 0.0 && t.max_slope_degs > 0.0 && t.settle_secs >= 0.0);
        assert!(t.min_region_size > 0 && t.merge_region_size >= t.min_region_size);
    }
}
