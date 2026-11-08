use std::path::{Path, PathBuf};

use avian3d::prelude::PhysicsPlugins;
use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy_replicon::prelude::RepliconPlugins;
use friendslop::net::NetCommand;

fn main() -> AppExit {
    App::new()
        .add_plugins((
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
                        ..default()
                    }),
                    ..default()
                })
                .set(AssetPlugin {
                    file_path: asset_root().display().to_string(),
                    ..default()
                }),
            RepliconPlugins,
            PhysicsPlugins::default(),
        ))
        .add_plugins(friendslop::GamePlugin)
        .add_systems(Startup, cli_args)
        .add_systems(Update, bench.run_if(bench_enabled))
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

fn bench(
    time: Res<Time<Real>>,
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
    exit.write(AppExit::Success);
}

/// Where the game keeps its assets (`crates/game/assets`). Bevy only knows
/// how to look next to the source crate (when run through cargo), next to
/// the executable, or in the working directory — and a bare `target/debug/`
/// binary satisfies none of those. Search the usual spots and walk up out of
/// `target/` if we have to; the game runs fine without assets (procedural
/// fallbacks), so a miss just falls back to Bevy's default.
fn asset_root() -> PathBuf {
    let found = |dir: &Path| dir.join("audio/music/catsong.ogg").is_file();
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
