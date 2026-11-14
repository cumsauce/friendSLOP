use std::path::{Path, PathBuf};

use avian3d::prelude::PhysicsPlugins;
use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_replicon::prelude::RepliconPlugins;
use friendslop::net::NetCommand;

fn main() -> AppExit {
    let mut app = App::new();
    // Browser (#242): models ship gzipped; inflate them as they arrive.
    #[cfg(target_arch = "wasm32")]
    app.register_asset_source(
        bevy::asset::io::AssetSourceId::Default,
        bevy::asset::io::AssetSourceBuilder::new(|| {
            let mut http = bevy::asset::io::AssetSource::get_default_reader("assets".into());
            Box::new(friendslop::gz_assets::GzReader(http()))
        }),
    );
    app.add_plugins((
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "friendSLOP".into(),
                    // Uncapped while benchmarking so frame times are real.
                    present_mode: if bench_enabled() {
                        bevy::window::PresentMode::AutoNoVsync
                    } else {
                        bevy::window::PresentMode::AutoVsync
                    },
                    // Web (#106): draw into index.html's canvas, fill the
                    // page, and keep the browser from scrolling/zooming.
                    canvas: Some("#bevy".into()),
                    fit_canvas_to_parent: true,
                    prevent_default_event_handling: true,
                    ..default()
                }),
                ..default()
            })
            .set(AssetPlugin {
                file_path: {
                    // Data files (maps, tuning, writing) read from the same folder.
                    friendslop_data::set_root(asset_root());
                    asset_root().display().to_string()
                },
                // No .meta files ship; on the web each check is a 404 round trip.
                meta_check: bevy::asset::AssetMetaCheck::Never,
                ..default()
            }),
        RepliconPlugins,
        PhysicsPlugins::default(),
    ))
    .add_plugins(friendslop::GamePlugin)
    .add_plugins(friendslop::screenshot::ScreenshotPlugin)
    .add_plugins(friendslop::perf::PerfPlugin)
    .add_systems(Startup, cli_args)
    .init_resource::<Churn>()
    .add_systems(Update, (churn, bench).chain().run_if(bench_enabled))
    .add_plugins(PipelineLog)
    .run()
}

struct PipelineLog;
impl Plugin for PipelineLog {
    fn build(&self, _app: &mut App) {}
    fn finish(&self, app: &mut App) {
        if !bench_enabled() {
            return;
        }
        use bevy::render::{Render, RenderApp, RenderSystems};
        app.sub_app_mut(RenderApp)
            .add_systems(Render, log_pipelines.in_set(RenderSystems::Cleanup));
    }
}

fn log_pipelines(cache: Res<bevy::render::render_resource::PipelineCache>, mut seen: Local<usize>) {
    use bevy::render::render_resource::PipelineDescriptor as D;
    let all: Vec<_> = cache.pipelines().collect();
    if all.len() <= *seen {
        return;
    }
    for p in &all[*seen..] {
        let (kind, label, defs) = match &p.descriptor {
            D::RenderPipelineDescriptor(d) => {
                ("render", d.label.clone(), d.vertex.shader_defs.len())
            }
            D::ComputePipelineDescriptor(d) => ("compute", d.label.clone(), d.shader_defs.len()),
            D::MeshPipelineDescriptor(d) => ("mesh", d.label.clone(), 0),
        };
        println!("BENCH pipeline {kind} {label:?} defs={defs}");
    }
    *seen = all.len();
}

/// `FRIENDSLOP_BENCH=1` logs average / p99 frame time over 10 s of gameplay
/// (after an 8 s warm-up in game) to stdout, then exits.
fn bench_enabled() -> bool {
    std::env::var_os("FRIENDSLOP_BENCH").is_some_and(|v| v != "0")
}

/// Asset events seen during the bench window, per asset type: (added,
/// modified, removed). Steady churn here means GPU buffers rebuilt every
/// frame.
#[derive(Resource, Default)]
struct Churn {
    mesh: [u32; 3],
    material: [u32; 3],
    image: [u32; 3],
}

fn tally<A: Asset>(events: &mut MessageReader<AssetEvent<A>>, n: &mut [u32; 3]) {
    for e in events.read() {
        match e {
            AssetEvent::Added { .. } => n[0] += 1,
            AssetEvent::Modified { .. } => n[1] += 1,
            AssetEvent::Removed { .. } => n[2] += 1,
            _ => {}
        }
    }
}

fn churn(
    mut meshes: MessageReader<AssetEvent<Mesh>>,
    mut materials: MessageReader<AssetEvent<StandardMaterial>>,
    mut images: MessageReader<AssetEvent<Image>>,
    mut churn: ResMut<Churn>,
) {
    tally(&mut meshes, &mut churn.mesh);
    tally(&mut materials, &mut churn.material);
    tally(&mut images, &mut churn.image);
}

fn bench(
    time: Res<Time<Real>>,
    mut churn: ResMut<Churn>,
    entities: Query<()>,
    mesh_ids: Query<&Mesh3d>,
    colliders: Query<(), With<avian3d::prelude::Collider>>,
    mesh_assets: Res<Assets<Mesh>>,
    state: Res<State<friendslop::AppState>>,
    mut samples: Local<Vec<f32>>,
    mut started: Local<Option<f32>>,
    ready: Option<Res<friendslop::loading::WorldReady>>,
    mut reported: Local<bool>,
    mut exit: MessageWriter<AppExit>,
) {
    if *state.get() != friendslop::AppState::InGame {
        return;
    }
    let now = time.elapsed_secs();
    let start = *started.get_or_insert(now + 8.0);
    let dt = time.delta_secs() * 1000.0;
    if dt > 50.0 {
        println!("BENCH hitch t={:.2}s dt_ms={dt:.0}", now - (start - 8.0));
    }
    if let Some(ready) = ready
        && ready.ready
        && !*reported
    {
        *reported = true;
        println!("BENCH world_ready_s={:.2}", ready.took_secs);
    }
    if now < start {
        *churn = Churn::default();
        return;
    }
    samples.push(dt);
    if now - start < 10.0 {
        return;
    }
    let mut sorted = samples.clone();
    sorted.sort_by(f32::total_cmp);
    let avg = sorted.iter().sum::<f32>() / sorted.len().max(1) as f32;
    let p99 = sorted
        .get(sorted.len() * 99 / 100)
        .copied()
        .unwrap_or_default();
    let slow = sorted.iter().filter(|&&ms| ms > 33.0).count();
    println!(
        "BENCH frames={} avg_ms={avg:.2} p99_ms={p99:.2} over33ms={slow} fps={:.1}",
        sorted.len(),
        1000.0 / avg.max(1e-3)
    );
    let secs = (now - start).max(1e-3);
    let per = |n: [u32; 3]| {
        format!(
            "+{:.1}/s ~{:.1}/s -{:.1}/s",
            n[0] as f32 / secs,
            n[1] as f32 / secs,
            n[2] as f32 / secs
        )
    };
    println!(
        "BENCH entities={} mesh {} material {} image {}",
        entities.iter().count(),
        per(churn.mesh),
        per(churn.material),
        per(churn.image)
    );
    // What there is to draw (#227): mesh entities, how many distinct meshes
    // they share (fewer = better batching), and the vertices of those still
    // on the CPU (render-only meshes count 0).
    let mut distinct = std::collections::HashSet::new();
    let mut verts = 0usize;
    for m in &mesh_ids {
        distinct.insert(m.0.id());
        verts += mesh_assets
            .get(&m.0)
            .and_then(|m| m.try_attribute(Mesh::ATTRIBUTE_POSITION).ok())
            .map_or(0, |a| a.len());
    }
    println!(
        "BENCH meshes={} distinct={} cpu_verts={verts} colliders={}",
        mesh_ids.iter().count(),
        distinct.len(),
        colliders.iter().count()
    );
    exit.write(AppExit::Success);
}

/// Where the game keeps its assets (`crates/game/assets`). Bevy only knows
/// how to look next to the source crate (when run through cargo), next to
/// the executable, or in the working directory — and a bare `target/debug/`
/// binary satisfies none of those. Search the usual spots and walk up out of
/// `target/` if we have to; the game runs fine without assets (procedural
/// fallbacks), so a miss just falls back to Bevy's default.
fn asset_root() -> PathBuf {
    // In the browser assets are fetched over HTTP from `./assets/`.
    if cfg!(target_arch = "wasm32") {
        return PathBuf::from("assets");
    }
    // CREDITS.md is always there (unlike any one LFS asset).
    let found = |dir: &Path| dir.join("CREDITS.md").is_file();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(manifest) = std::env::var("CARGO_MANIFEST_DIR") {
        candidates.push(PathBuf::from(manifest).join("assets"));
    }
    if let Ok(dir) = std::env::current_dir() {
        candidates.push(dir.join("assets"));
        candidates.push(dir.join("crates/game/assets"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join("assets"));
        // Walk up out of e.g. target/debug towards the checkout root.
        let mut up = dir.to_path_buf();
        for _ in 0..5 {
            up = match up.parent() {
                Some(parent) => parent.to_path_buf(),
                None => break,
            };
            candidates.push(up.join("crates/game/assets"));
            candidates.push(up.join("assets"));
        }
    }
    candidates
        .into_iter()
        .find(|dir| found(dir))
        .unwrap_or(PathBuf::from("assets"))
}

/// `friendslop host` or `friendslop join <code>` skip the menu (handy for testing).
fn cli_args(mut commands: MessageWriter<NetCommand>) {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [cmd] if cmd == "host" => {
            commands.write(NetCommand::Host);
        }
        [cmd, code] if cmd == "join" => {
            commands.write(NetCommand::Join(code.clone()));
        }
        _ => {}
    }
}
