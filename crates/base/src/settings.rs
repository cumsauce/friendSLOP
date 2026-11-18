//! Player settings (#25): master volume, voice volume and the mic (on or
//! muted, open or push-to-talk). [`crate::voice::VoiceMuted`] follows
//! [`Settings::mic_on`]. Local to each player; nothing is replicated.

use crate::voice_chat::*;
use crate::sound::{MainBus, Volume, VolumeNode};
use bevy::prelude::*;

/// Volumes are `0..=1`, changed in steps of [`Settings::STEP`] from the menu.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub master_volume: f32,
    pub voice_volume: f32,
    /// HALT may use your OS username in the window title and HUD.
    pub halt_real_name: bool,
    /// World sounds are placed with head-related filters (headphones), so
    /// you can hear above, below and behind. See [`crate::spatial`].
    pub hrtf: bool,
    /// Your mic is on. Off mutes you (there's no hotkey: it's a setting).
    pub mic_on: bool,
    /// Your mic only sends while you hold the `talk` action (V).
    pub push_to_talk: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            voice_volume: 1.0,
            halt_real_name: true,
            hrtf: true,
            mic_on: true,
            push_to_talk: false,
        }
    }
}

impl Settings {
    pub const STEP: f32 = 0.1;
    /// The saved file (see [`load_local`]).
    pub const FILE: &'static str = "settings.txt";

    /// Saved as `key=value` lines.
    pub fn to_text(self) -> String {
        format!(
            "master={}\nvoice={}\nhalt_name={}\nhrtf={}\nmic={}\nptt={}\n",
            self.master_volume,
            self.voice_volume,
            u8::from(self.halt_real_name),
            u8::from(self.hrtf),
            u8::from(self.mic_on),
            u8::from(self.push_to_talk)
        )
    }

    /// Reads [`to_text`](Self::to_text); a missing or unreadable key keeps
    /// its default and volumes are clamped to `0..=1`.
    pub fn from_text(text: &str) -> Self {
        let mut s = Self::default();
        for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
            let v = v.trim();
            let volume = || v.parse::<f32>().ok().filter(|f| f.is_finite());
            match k.trim() {
                "master" => s.master_volume = volume().unwrap_or(s.master_volume),
                "voice" => s.voice_volume = volume().unwrap_or(s.voice_volume),
                "halt_name" => s.halt_real_name = v != "0",
                "hrtf" => s.hrtf = v != "0",
                "mic" => s.mic_on = v != "0",
                "ptt" => s.push_to_talk = v != "0",
                _ => {}
            }
        }
        s.master_volume = s.master_volume.clamp(0.0, 1.0);
        s.voice_volume = s.voice_volume.clamp(0.0, 1.0);
        s
    }

    /// What was saved last time, or the defaults.
    pub fn load() -> Self {
        load_local(Self::FILE).map_or_else(Self::default, |t| Self::from_text(&t))
    }

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

/// Screenshot capture is running (`FRIENDSLOP_SCREENSHOT` is set to a path):
/// animation freezes and RNGs pin so the picture is deterministic (goldens).
/// The game side is `crate::screenshot::frozen()`.
pub fn frozen() -> bool {
    std::env::var("FRIENDSLOP_SCREENSHOT").is_ok_and(|p| !p.trim().is_empty())
}

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Settings::load()).add_systems(
            Update,
            (
                save_settings,
                apply_settings.before(VoiceChatSystems::Devices),
            ),
        );
    }
}

fn save_settings(s: Res<Settings>) {
    if s.is_changed() && !s.is_added() {
        save_local(Settings::FILE, &s.to_text());
    }
}

fn apply_settings(
    settings: Res<Settings>,
    mut master: Query<&mut VolumeNode, With<MainBus>>,
    mut input: ResMut<VoiceInput>,
    mut muted: Local<Option<bool>>,
) {
    // Benchmarks and test runs (`FRIENDSLOP_BENCH`, `FRIENDSLOP_MUTE`) stay
    // silent, so a run in the background never blasts music. Read once:
    // the environment doesn't change while we run (#151).
    let muted = *muted.get_or_insert_with(|| {
        ["FRIENDSLOP_MUTE", "FRIENDSLOP_BENCH"]
            .iter()
            .any(|k| std::env::var_os(k).is_some_and(|v| v != "0"))
    });
    let volume = Volume::Linear(if muted { 0.0 } else { settings.master_volume });
    for mut node in &mut master {
        if node.volume != volume {
            node.volume = volume;
        }
    }
    let voice = settings.voice_output();
    if input.volume != voice {
        input.volume = voice;
    }
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
    fn settings_save_and_load() {
        let s = Settings {
            master_volume: 0.2,
            voice_volume: 0.55,
            halt_real_name: false,
            hrtf: false,
            mic_on: false,
            push_to_talk: true,
        };
        assert_eq!(Settings::from_text(&s.to_text()), s);
        // Missing keys keep the defaults; junk and out-of-range values are tamed.
        assert_eq!(Settings::from_text(""), Settings::default());
        let odd = Settings::from_text("master=9\nvoice=nan\nhalt_name=0\nhrtf=0\nstray\n");
        assert_eq!(odd.master_volume, 1.0);
        assert_eq!(odd.voice_volume, Settings::default().voice_volume);
        assert!(!odd.halt_real_name);
        assert!(!odd.hrtf);
        assert!(Settings::from_text("").hrtf, "on unless turned off");
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
