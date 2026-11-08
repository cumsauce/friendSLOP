//! Player settings (#25): master volume and voice volume. Mic mute lives in
//! [`crate::voice::VoiceMuted`]. Local to each player; nothing is replicated.

use bevy::audio::Volume;
use bevy::prelude::*;
use bevy_voice_chat::prelude::*;

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
    global.volume = Volume::Linear(settings.master_volume);
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
