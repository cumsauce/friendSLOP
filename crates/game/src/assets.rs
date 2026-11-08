//! Handles for the files in `crates/game/assets` (credits and sources in
//! `crates/game/assets/CREDITS.md`). Handles are loaded lazily: if the files
//! are missing (a clone without LFS data), systems that use them quietly skip
//! and the game keeps its procedural-primitive look.
//!
//! Gameplay logic never depends on any of this — these are decoration only.

use bevy::audio::Volume;
use bevy::prelude::*;

use crate::AppState;

/// Sound effects. All short loops/one-shots in `assets/audio/sfx`.
#[derive(Resource, Clone)]
pub struct Sfx {
    pub gunshot: Handle<AudioSource>,
    pub punch: Handle<AudioSource>,
    pub crow_caw: Handle<AudioSource>,
    pub eat: Handle<AudioSource>,
    /// Ambient music, looped while in game (`assets/audio/music`).
    pub music: Handle<AudioSource>,
}

/// The friendly rounded HUD font (`assets/fonts/Nunito.ttf`, OFL). Only use it
/// once [`UiFont::ready`] says so; until then (or if the file is missing) text
/// keeps Bevy's built-in default font.
#[derive(Resource, Clone)]
pub struct UiFont(pub Handle<Font>);

impl UiFont {
    pub fn ready(&self, assets: &AssetServer) -> bool {
        assets.is_loaded_with_dependencies(&self.0)
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, (load_sfx, load_font));
}

fn load_font(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    let Some(assets) = assets else { return };
    commands.insert_resource(UiFont(assets.load("fonts/Nunito.ttf")));
}

fn load_sfx(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    let Some(assets) = assets else { return };
    let sfx = |name: &str| assets.load(format!("audio/sfx/{name}"));
    commands.insert_resource(Sfx {
        gunshot: sfx("gunshot.wav"),
        punch: sfx("punch.wav"),
        crow_caw: sfx("crow_caw.wav"),
        eat: sfx("eat.ogg"),
        music: assets.load("audio/music/catsong.ogg"),
    });
}

/// Play a sound effect once for this peer, at the given linear volume.
pub fn play_sfx(commands: &mut Commands, which: &Handle<AudioSource>, volume: f32) {
    commands.spawn((
        AudioPlayer(which.clone()),
        PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
        DespawnOnExit(AppState::InGame),
    ));
}
