//! Player settings (#25): master volume and voice volume. Mic mute lives in
//! [`crate::voice::VoiceMuted`]. Local to each player; nothing is replicated.

use crate::voice_chat::*;
use bevy::audio::Volume;
use bevy::prelude::*;

/// Volumes are `0..=1`, changed in steps of [`Settings::STEP`] from the menu.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub master_volume: f32,
    pub voice_volume: f32,
    /// HALT may use your OS username in the window title and HUD.
    pub halt_real_name: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            voice_volume: 1.0,
            halt_real_name: true,
        }
    }
}

impl Settings {
    pub const STEP: f32 = 0.1;

    /// Nudge a volume by `steps` and clamp it, snapping to tenths.
    pub fn nudge(value: f32, steps: i32) -> f32 {
        ((value + steps as f32 * Self::STEP).clamp(0.0, 1.0) * 10.0).round() / 10.0
    }

    /// What voice chat plays at: the master volume applies to voices too.
    pub fn voice_output(&self) -> f32 {
        self.master_volume * self.voice_volume
    }
}

/// Where a small local file (like the saved character) lives: the OS config
/// folder, under `friendslop/`. `None` if the platform tells us nothing.
#[cfg(not(target_arch = "wasm32"))]
pub fn config_path(file: &str) -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("friendslop").join(file))
}

/// Read a small saved file: the config folder natively, `localStorage` on the web.
pub fn load_local(file: &str) -> Option<String> {
    #[cfg(target_arch = "wasm32")]
    return crate::web::load(file);
    #[cfg(not(target_arch = "wasm32"))]
    std::fs::read_to_string(config_path(file)?).ok()
}

/// Save a small file next to [`load_local`]'s. Failures only warn.
pub fn save_local(file: &str, text: &str) {
    #[cfg(target_arch = "wasm32")]
    crate::web::save(file, text);
    #[cfg(not(target_arch = "wasm32"))]
    {
        let Some(path) = config_path(file) else {
            return;
        };
        if let Some(dir) = path.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            warn!("can't create {}: {e}", dir.display());
            return;
        }
        if let Err(e) = std::fs::write(&path, text) {
            warn!("can't save {}: {e}", path.display());
        }
    }
}

/// Wall-clock time since the Unix epoch. `SystemTime::now` panics in the
/// browser, so the web build asks JavaScript's `Date.now()` instead.
pub fn since_epoch() -> std::time::Duration {
    #[cfg(target_arch = "wasm32")]
    return std::time::Duration::from_secs_f64(js_sys::Date::now().max(0.0) / 1000.0);
    #[cfg(not(target_arch = "wasm32"))]
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Settings>()
            .add_systems(Update, apply_settings.before(VoiceChatSystems::Devices));
    }
}

fn apply_settings(
    settings: Res<Settings>,
    mut global: ResMut<GlobalVolume>,
    mut input: ResMut<VoiceInput>,
) {
    // Benchmarks and test runs (`FRIENDSLOP_BENCH`, `FRIENDSLOP_MUTE`) stay
    // silent, so a run in the background never blasts music.
    let muted = ["FRIENDSLOP_MUTE", "FRIENDSLOP_BENCH"]
        .iter()
        .any(|k| std::env::var_os(k).is_some_and(|v| v != "0"));
    global.volume = Volume::Linear(if muted { 0.0 } else { settings.master_volume });
    input.volume = settings.voice_output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nudge_clamps_and_snaps() {
        assert_eq!(Settings::nudge(0.8, 1), 0.9);
        assert_eq!(Settings::nudge(0.95, 3), 1.0);
        assert_eq!(Settings::nudge(0.1, -5), 0.0);
        assert_eq!(Settings::nudge(0.30000004, -1), 0.2);
    }

    #[test]
    fn master_scales_voice() {
        let s = Settings {
            master_volume: 0.5,
            voice_volume: 0.5,
            ..default()
        };
        assert_eq!(s.voice_output(), 0.25);
    }
}
