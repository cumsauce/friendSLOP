//! `FRIENDSLOP_PERF=1`: log frame rate, frame time and what's being drawn every
//! 5 s (desktop only; for finding what makes a place slow). Prints nothing and
//! costs nothing without it.

use bevy::camera::visibility::ViewVisibility;
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        if std::env::var("FRIENDSLOP_PERF").is_ok_and(|v| v == "1" || v == "2") {
            if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
                app.add_plugins(FrameTimeDiagnosticsPlugin::default());
            }
            app.add_systems(Last, (report, breakdown));
        }
    }
}

fn report(
    time: Res<Time<Real>>,
    mut last: Local<f32>,
    diag: Res<DiagnosticsStore>,
    entities: Query<()>,
    meshes: Query<&ViewVisibility, With<Mesh3d>>,
    lights: Query<(), Or<(With<PointLight>, With<SpotLight>)>>,
    cam: Query<&GlobalTransform, With<Camera3d>>,
) {
    let now = time.elapsed_secs();
    if now - *last < 5.0 {
        return;
    }
    *last = now;
    let fps = diag
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let ms = diag
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let total = meshes.iter().count();
    let seen = meshes.iter().filter(|v| v.get()).count();
    let at = cam
        .iter()
        .next()
        .map(|g| g.translation())
        .unwrap_or_default();
    info!(
        "perf: {fps:.0} fps {ms:.1} ms | entities {} | meshes {total} ({seen} visible) | point/spot lights {} | camera {at:.0}",
        entities.iter().count(),
        lights.iter().count()
    );
}

/// What owns an entity, for the breakdown: the scene file it came from, else
/// its top ancestor's name, else its own name.
fn owner(
    e: Entity,
    parents: &Query<&ChildOf>,
    scenes: &Query<&bevy::world_serialization::WorldAssetRoot>,
    names: &Query<&Name>,
) -> String {
    let mut at = e;
    let mut top = e;
    for _ in 0..64 {
        if let Ok(s) = scenes.get(at) {
            return s
                .0
                .path()
                .map_or("scene".to_string(), |p| p.path().display().to_string());
        }
        top = at;
        let Ok(p) = parents.get(at) else { break };
        at = p.parent();
    }
    names
        .get(top)
        .or_else(|_| names.get(e))
        .map_or("(unnamed, built in code)".to_string(), |n| {
            n.as_str().to_string()
        })
}

/// `FRIENDSLOP_PERF=2`: once, 25 s in, who owns the meshes and lights.
#[allow(clippy::too_many_arguments)]
fn breakdown(
    time: Res<Time<Real>>,
    mut done: Local<bool>,
    meshes: Query<(Entity, &ViewVisibility), With<Mesh3d>>,
    lights: Query<(Entity, Option<&PointLight>, Option<&SpotLight>)>,
    parents: Query<&ChildOf>,
    scenes: Query<&bevy::world_serialization::WorldAssetRoot>,
    names: Query<&Name>,
) {
    if *done
        || time.elapsed_secs() < 25.0
        || !std::env::var("FRIENDSLOP_PERF").is_ok_and(|v| v == "2")
    {
        return;
    }
    *done = true;
    let mut by: std::collections::HashMap<String, (usize, usize)> = default();
    for (e, v) in &meshes {
        let k = by.entry(owner(e, &parents, &scenes, &names)).or_default();
        k.0 += 1;
        k.1 += v.get() as usize;
    }
    let mut rows: Vec<_> = by.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.0));
    for (k, (n, seen)) in rows.iter().take(40) {
        info!("perf meshes: {n:6} ({seen:5} visible) {k}");
    }
    let mut by: std::collections::HashMap<String, (usize, usize)> = default();
    for (e, p, s) in &lights {
        let shadows = match (p, s) {
            (Some(p), _) => p.shadow_maps_enabled,
            (_, Some(s)) => s.shadow_maps_enabled,
            _ => continue,
        };
        let k = by.entry(owner(e, &parents, &scenes, &names)).or_default();
        k.0 += 1;
        k.1 += shadows as usize;
    }
    let mut rows: Vec<_> = by.into_iter().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.0));
    for (k, (n, sh)) in rows.iter().take(25) {
        info!("perf lights: {n:5} ({sh} with shadows) {k}");
    }
}
