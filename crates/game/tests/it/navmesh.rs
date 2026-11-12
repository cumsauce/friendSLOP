//! The villagers' navmesh builds from the real world layout. Regression for
//! rerecast aborting the whole bake with "invalid contour" over the 200x200 m
//! map (janhohenheim/rerecast#64): the mesh covers the terrain and the village
//! architecture, and these are the settings `features/villagers.rs` ships with.

use std::time::{Duration, Instant};

use avian_rerecast::ColliderToTriMesh;
use avian3d::prelude::*;
use bevy::asset::AssetPlugin;
use bevy::ecs::system::RunSystemOnce;
use bevy::math::{UVec3, Vec3A};
use bevy::prelude::*;
use bevy_rerecast::prelude::*;
use bevy_rerecast::{NavmeshApp, rerecast::TriMesh};
use friendslop::buildings::{self, Shape};

/// The ground collider's shape: [`terrain::height_at`] sampled every 2 m over
/// ±110 m, exactly like `terrain::ground_collider()`.
fn terrain_trimesh() -> TriMesh {
    const HALF: f32 = 110.0;
    const CELLS: usize = 110;
    let n = CELLS + 1;
    let step = HALF * 2.0 / CELLS as f32;
    let mut vertices = Vec::with_capacity(n * n);
    for j in 0..n {
        for i in 0..n {
            let (x, z) = (-HALF + i as f32 * step, -HALF + j as f32 * step);
            vertices.push(Vec3A::new(x, friendslop::terrain::height_at(x, z), z));
        }
    }
    let at = |i: usize, j: usize| (j * n + i) as u32;
    let mut indices = Vec::with_capacity(CELLS * CELLS * 2);
    for j in 0..CELLS {
        for i in 0..CELLS {
            // The same up-facing winding parry's heightfield->trimesh yields
            // (checked against it with a scratch test): [p00, p01, p10],
            // [p01, p11, p10].
            let (a, b, c, d) = (at(i, j), at(i + 1, j), at(i, j + 1), at(i + 1, j + 1));
            indices.push(UVec3::new(a, c, b));
            indices.push(UVec3::new(c, d, b));
        }
    }
    TriMesh {
        vertices,
        area_types: vec![rerecast_area_type(); indices.len()],
        indices,
    }
}

/// `rerecast::AreaType::NOT_WALKABLE`, spelled out to stay import-light.
fn rerecast_area_type() -> bevy_rerecast::rerecast::AreaType {
    bevy_rerecast::rerecast::AreaType::NOT_WALKABLE
}

/// The village's colliders, converted the way `avian_rerecast` does it, so the
/// test sees the same obstacles the host's backend gathers. The game spawns
/// unit shapes scaled by the transform, which avian bakes into the shape, so
/// the test builds them at full size directly.
fn buildings_trimesh() -> TriMesh {
    let mut mesh = TriMesh::default();
    for piece in &buildings::layout(0).pieces {
        let collider = match piece.shape {
            Shape::Cube => Collider::cuboid(piece.size.x, piece.size.y, piece.size.z),
            Shape::Cylinder => Collider::cylinder(piece.size.x / 2.0, piece.size.y),
            // Gables and balls spawn without colliders.
            _ => continue,
        };
        if piece.solid == buildings::Solid::None {
            continue;
        }
        if let Some(part) = collider.to_trimesh(piece.centre, piece.rot, 10) {
            mesh.extend(part);
        }
    }
    mesh
}

#[derive(Resource, Default)]
struct Pending(Option<Handle<Navmesh>>);

#[test]
fn navmesh_builds_from_the_world() {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin::default(),
        // rerecast logs (and our patch warns) through tracing; show it, since
        // a silently failed bake would otherwise time out without a clue.
        bevy::log::LogPlugin::default(),
    ));
    // The debug gizmos (part of NavmeshPlugins) touch the PBR material, gizmo
    // and mesh stores; provide them headlessly.
    app.init_asset::<StandardMaterial>();
    app.init_asset::<bevy::gizmos::GizmoAsset>();
    app.init_asset::<Mesh>();
    app.add_plugins(NavmeshPlugins::default());
    app.set_navmesh_backend(|_input: In<NavmeshSettings>| {
        let mut mesh = terrain_trimesh();
        mesh.extend(buildings_trimesh());
        mesh
    });
    app.init_resource::<Pending>();
    app.world_mut()
        .run_system_once(
            |mut generator: NavmeshGenerator, mut pending: ResMut<Pending>| {
                pending.0 =
                    Some(generator.generate(friendslop::features::villagers::navmesh_settings()));
            },
        )
        .expect("generate the navmesh");
    let handle = app.world().resource::<Pending>().0.clone().expect("queued");
    let started = Instant::now();
    loop {
        app.update();
        if app
            .world()
            .resource::<Assets<Navmesh>>()
            .get(&handle)
            .is_some()
        {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "the navmesh never finished building"
        );
    }
    let navmesh = app
        .world()
        .resource::<Assets<Navmesh>>()
        .get(&handle)
        .expect("just built");
    println!(
        "navmesh: {} vertices in {:.1}s",
        navmesh.polygon.vertices.len(),
        started.elapsed().as_secs_f32()
    );
    assert!(
        navmesh.polygon.vertices.len() > 1000,
        "a mesh over 200x200 m of map plus a village should have more than a few vertices"
    );
}
