//! Browser build glue (#106). Only compiled for `wasm32`.
//!
//! `bevy_voice_chat` (cpal threads) can't run on a browser's main thread, so
//! this module stands in for the handful of its types the game names. They do
//! nothing: voice chat is simply off in the browser for now. The network side
//! lives in `net_web.rs`.

use bevy::prelude::*;

/// Stand-ins for `bevy_voice_chat::prelude` (same names, same shapes, no audio).
pub mod voice_chat {
    use bevy::prelude::*;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
    pub struct SpeakerId(pub u64);

    #[derive(Component, Clone, Copy, Debug)]
    pub struct VoiceSenderId(pub SpeakerId);

    #[derive(Resource, Clone, Copy, Debug, Default)]
    pub struct HostVoiceSpeaker(pub Option<SpeakerId>);

    #[derive(Component, Clone, Copy, Debug, Default)]
    pub struct VoiceListener {
        pub right_from: Option<Entity>,
    }

    #[derive(Component, Clone, Copy, Debug)]
    pub struct VoiceSpeaker {
        pub id: SpeakerId,
        pub range: Option<f32>,
    }

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum TalkMode {
        #[default]
        VoiceActivity,
        PushToTalk,
    }

    #[derive(Resource, Clone, Debug, Default)]
    pub struct VoiceInput {
        pub enabled: bool,
        pub positional: bool,
        pub mode: TalkMode,
        pub talk_held: bool,
        pub volume: f32,
    }

    #[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub enum VoiceChatSystems {
        Devices,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum FilterStage {
        Outgoing,
    }

    pub struct FilterContext;

    pub trait VoiceFilter: Send + Sync + 'static {
        fn name(&self) -> &str;
        fn process(&mut self, frame: &mut [f32], ctx: &FilterContext);
        fn reset(&mut self) {}
    }

    /// Never inserted on the web, so `Option<ResMut<VoiceFilters>>` is always `None`.
    #[derive(Resource)]
    pub struct VoiceFilters;

    impl VoiceFilters {
        pub fn add(&mut self, _stage: FilterStage, _order: i32, _f: impl VoiceFilter) {}
        pub fn get_mut<F: VoiceFilter>(&mut self) -> Option<&mut F> {
            None
        }
    }
}

/// Read a small saved file (e.g. the character) from `localStorage`.
pub fn load(key: &str) -> Option<String> {
    storage()?.get_item(&storage_key(key)).ok().flatten()
}

/// Save a small file to `localStorage`. Silently does nothing if storage is blocked.
pub fn save(key: &str, value: &str) {
    if let Some(s) = storage()
        && let Err(e) = s.set_item(&storage_key(key), value)
    {
        warn!("could not save {key} to localStorage: {e:?}");
    }
}

fn storage_key(key: &str) -> String {
    format!("friendslop/{key}")
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}
