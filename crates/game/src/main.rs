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
        .run()
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
