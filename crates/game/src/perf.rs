//! `FRIENDSLOP_PERF=1`: log frame rate, frame time and what's being drawn every
//! 5 s (desktop only; for finding what makes a place slow). Prints nothing and
//! costs nothing without it. `=2` also breaks down, 25 s in, which models own
//! the meshes and lights; `=3` repeats that breakdown every 30 s (S9). The
//! first report logs the graphics preset, resolution and WGPU backend.

use bevy::camera::visibility::ViewVisibility;
use bevy::core_pipeline::core_3d::{AlphaMask3d, Opaque3d, Transparent3d};
use bevy::core_pipeline::prepass::{AlphaMask3dPrepass, Opaque3dPrepass};
use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::pbr::Shadow;
use bevy::prelude::*;
use bevy::render::render_phase::{BinnedPhaseItem, ViewBinnedRenderPhases, ViewSortedRenderPhases};
use bevy::render::render_resource::TextureFormat;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::window::PrimaryWindow;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct PerfPlugin;

impl Plugin for PerfPlugin {
    fn build(&self, app: &mut App) {
        if !std::env::var("FRIENDSLOP_PERF").is_ok_and(|v| v == "1" || v == "2" || v == "3") {
            return;
        }
        if !app.is_plugin_added::<FrameTimeDiagnosticsPlugin>() {
            app.add_plugins(FrameTimeDiagnosticsPlugin::default());
        }
        let counts = DrawCounts::default();
        app.insert_resource(counts.clone())
            .add_systems(Last, (report, breakdown));
        // Draw calls are counted where the batches live: the render world.
        // The count comes back through a shared atomic; a log line does not
        // need a cross-world sync point.
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(counts)
                .add_systems(Render, count_draw_calls.in_set(RenderSystems::Cleanup));
        }
    }
}

/// Draw calls counted in the render world, shared with the main world.
#[derive(Resource, Clone, Default)]
struct DrawCounts(Arc<AtomicU32>);

/// Count the frame's draw calls: one per binned batch/bin and one per sorted
/// item, summed over every view (camera passes, prepasses, shadow maps).
/// The GPU's batch lists are `pub(crate)` in `bevy_render`, so this counts the
/// bins and items queued for batching — very close to the batches the GPU
/// ends up with.
#[allow(clippy::too_many_arguments)]
fn count_draw_calls(
    opaque: Option<Res<ViewBinnedRenderPhases<Opaque3d>>>,
    alpha_mask: Option<Res<ViewBinnedRenderPhases<AlphaMask3d>>>,
    opaque_prepass: Option<Res<ViewBinnedRenderPhases<Opaque3dPrepass>>>,
    alpha_mask_prepass: Option<Res<ViewBinnedRenderPhases<AlphaMask3dPrepass>>>,
    shadow: Option<Res<ViewBinnedRenderPhases<Shadow>>>,
    transparent: Option<Res<ViewSortedRenderPhases<Transparent3d>>>,
    counts: Res<DrawCounts>,
) {
    let mut total = 0u32;
    if let Some(phases) = opaque {
        total += binned_draws(&phases);
    }
    if let Some(phases) = alpha_mask {
        total += binned_draws(&phases);
    }
    if let Some(phases) = opaque_prepass {
        total += binned_draws(&phases);
    }
    if let Some(phases) = alpha_mask_prepass {
        total += binned_draws(&phases);
    }
    if let Some(phases) = shadow {
        total += binned_draws(&phases);
    }
    if let Some(phases) = transparent {
        for phase in phases.values() {
            total += phase.items.len() as u32;
        }
    }
    counts.0.store(total, Ordering::Relaxed);
}

fn binned_draws<BPI: BinnedPhaseItem>(phases: &ViewBinnedRenderPhases<BPI>) -> u32 {
    let mut n = 0u32;
    for phase in phases.values() {
        // Multidrawable meshes are drawn per batch set (one multi-draw call
        // per set), batchable meshes per bin, and each unbatchable or
        // non-mesh entity is its own draw call.
        n += phase.multidrawable_meshes.len() as u32;
        n += phase.batchable_meshes.len() as u32;
        for unbatchable in phase.unbatchable_meshes.values() {
            n += unbatchable.entities.len() as u32;
        }
        for non_mesh in phase.non_mesh_items.values() {
            n += non_mesh.entities.len() as u32;
        }
    }
    n
}

/// The draw-call estimate when the render-world count is not there (the first
/// frames, or a build without the render app): one draw per distinct
/// (mesh, material) among visible meshes — everything sharing both batches.
fn draw_batches_proxy(
    meshes: &Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>, &ViewVisibility)>,
) -> usize {
    let mut seen = HashSet::new();
    for (mesh, mat, vis) in meshes {
        if vis.get() {
            seen.insert((mesh.id(), mat.id()));
        }
    }
    seen.len()
}

/// Rough GPU memory of every loaded image: bytes per texel (block-compressed
/// formats count their real rate) times the mip overhead (4/3). Shadow maps
/// and other render targets live in the render world and are not included.
fn texture_vram(images: &Assets<Image>) -> f64 {
    images
        .iter()
        .map(|(_, img)| {
            let w = f64::from(img.width());
            let h = f64::from(img.height());
            w * h * texel_bytes(img.texture_descriptor.format) * 4.0 / 3.0
        })
        .sum()
}

fn texel_bytes(format: TextureFormat) -> f64 {
    use TextureFormat as F;
    match format {
        // BC1/BC4: 8 bytes per 4x4 block. BC2/BC3/BC5/BC6H/BC7: 16.
        F::Bc1RgbaUnorm | F::Bc1RgbaUnormSrgb | F::Bc4RUnorm | F::Bc4RSnorm => 0.5,
        F::Bc2RgbaUnorm
        | F::Bc2RgbaUnormSrgb
        | F::Bc3RgbaUnorm
        | F::Bc3RgbaUnormSrgb
        | F::Bc5RgUnorm
        | F::Bc5RgSnorm
        | F::Bc6hRgbUfloat
        | F::Bc6hRgbFloat
        | F::Bc7RgbaUnorm
        | F::Bc7RgbaUnormSrgb => 1.0,
        // Uncompressed and anything else: RGBA8's rate, close enough for a
        // budget column.
        _ => 4.0,
    }
}

#[allow(clippy::too_many_arguments)]
fn report(
    time: Res<Time<Real>>,
    mut last: Local<f32>,
    mut first: Local<bool>,
    diag: Res<DiagnosticsStore>,
    entities: Query<()>,
    meshes: Query<&ViewVisibility, With<Mesh3d>>,
    batches: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>, &ViewVisibility)>,
    lights: Query<(), Or<(With<PointLight>, With<SpotLight>)>>,
    cam: Query<&GlobalTransform, With<Camera3d>>,
    images: Res<Assets<Image>>,
    counts: Res<DrawCounts>,
    quality: Option<Res<crate::graphics::GraphicsQuality>>,
    window: Query<&Window, With<PrimaryWindow>>,
) {
    let now = time.elapsed_secs();
    if now - *last < 5.0 {
        return;
    }
    *last = now;
    if !*first {
        *first = true;
        let preset = quality
            .as_ref()
            .and_then(|q| q.preset())
            .map_or("custom", |p| p.label());
        let size = window.iter().next().map_or("?".to_string(), |w| {
            format!("{:.0}x{:.0}", w.width(), w.height())
        });
        info!(
            "perf: graphics preset {preset} {size} backend {}",
            std::env::var("WGPU_BACKEND").unwrap_or_else(|_| "default".to_string())
        );
    }
    let fps = diag
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    let ms = diag
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|d| d.smoothed())
        .unwrap_or(0.0);
    // The median, 95th percentile and worst of the last ~120 frames: the median
    // ignores hitches (unlike the average), the other two are the hitches.
    let (p50, p95, worst) = diag
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .map(|d| {
            let mut v: Vec<f64> = d.values().copied().collect();
            v.sort_by(f64::total_cmp);
            let at = |q: f64| {
                v.get(((v.len() as f64 - 1.0) * q) as usize)
                    .copied()
                    .unwrap_or(0.0)
            };
            (at(0.5), at(0.95), v.last().copied().unwrap_or(0.0))
        })
        .unwrap_or_default();
    let total = meshes.iter().count();
    let seen = meshes.iter().filter(|v| v.get()).count();
    let counted = counts.0.load(Ordering::Relaxed) as usize;
    let (draws, note) = if counted > 0 {
        (counted, "")
    } else {
        (draw_batches_proxy(&batches), " (proxy)")
    };
    let vram_mb = texture_vram(&images) / 1e6;
    let at = cam
        .iter()
        .next()
        .map(|g| g.translation())
        .unwrap_or_default();
    info!(
        "perf: {fps:.0} fps {ms:.1} ms (median {p50:.1} p95 {p95:.1} worst {worst:.0} ms) | entities {} | meshes {total} ({seen} visible) | draw calls {draws}{note} | tex {vram_mb:.0} MB | point/spot lights {} | camera {at:.0}",
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
/// `FRIENDSLOP_PERF=3`: the same breakdown every 30 s after that (S9), with
/// per-owner batch tallies (one batch per distinct (mesh, material) among
/// the owner's visible meshes).
#[allow(clippy::too_many_arguments)]
fn breakdown(
    time: Res<Time<Real>>,
    mut last: Local<f32>,
    meshes: Query<
        (
            Entity,
            &ViewVisibility,
            &Mesh3d,
            Option<&MeshMaterial3d<StandardMaterial>>,
        ),
        With<Mesh3d>,
    >,
    lights: Query<(Entity, Option<&PointLight>, Option<&SpotLight>)>,
    parents: Query<&ChildOf>,
    scenes: Query<&bevy::world_serialization::WorldAssetRoot>,
    names: Query<&Name>,
) {
    let repeat = match std::env::var("FRIENDSLOP_PERF").as_deref() {
        Ok("3") => true,
        Ok("2") => false,
        _ => return,
    };
    let now = time.elapsed_secs();
    if *last == 0.0 {
        if now < 25.0 {
            return;
        }
    } else if !repeat || now - *last < 30.0 {
        return;
    }
    *last = now;
    let mut by: std::collections::HashMap<
        String,
        (
            usize,
            usize,
            HashSet<(
                bevy::asset::AssetId<Mesh>,
                Option<bevy::asset::AssetId<StandardMaterial>>,
            )>,
        ),
    > = default();
    for (e, v, mesh, mat) in &meshes {
        let k = by.entry(owner(e, &parents, &scenes, &names)).or_default();
        k.0 += 1;
        k.1 += v.get() as usize;
        if v.get() {
            k.2.insert((mesh.id(), mat.map(|m| m.id())));
        }
    }
    let mut rows: Vec<_> = by
        .into_iter()
        .map(|(k, (n, seen, batches))| (k, (n, seen, batches.len())))
        .collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.1.0));
    for (k, (n, seen, batches)) in rows.iter().take(40) {
        info!("perf meshes: {n:6} ({seen:5} visible, {batches:5} batches) {k}");
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
