//! Voice chat for everyone in the session (`bevy_voice_chat` over replicon).
//!
//! Proximity voice in the game: a friend is at full voice within a few metres,
//! fades with distance and is panned to where they stand, heard from your
//! head (the [`crate::spatial::Ears`]). But never silent: past where the
//! falloff would drop under [`VOICE_FLOOR`] everyone stays faintly audible,
//! so the whole session can still talk wherever they wander.

use crate::voice_chat::*;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::player::{LocalPlayer, Player, PlayerOwner};
use serde::{Deserialize, Serialize};

/// Press M to toggle. Muting stops your mic; you still hear others.
#[derive(Resource, Default, Debug)]
pub struct VoiceMuted(pub bool);

/// Replicated on every [`Player`]: their mic is muted (M), so the team strip
/// can show it on everyone's badge, not just your own.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MicMuted(pub bool);

/// Client -> server: "my mic is (un)muted now".
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SetMicMuted {
    pub muted: bool,
}

/// Mic-state registration (appended last in the protocol; same on every peer).
pub fn mic_protocol(app: &mut App) {
    app.replicate::<MicMuted>()
        .add_client_event::<SetMicMuted>(Channel::Ordered)
        .add_observer(set_mic_muted);
}

/// Server: put the sender's mic state on their player.
fn set_mic_muted(
    ev: On<FromClient<SetMicMuted>>,
    mut commands: Commands,
    players: Query<(Entity, &PlayerOwner, Option<&MicMuted>)>,
) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((e, _, now)) = players.iter().find(|(_, o, _)| o.0 == owner) else {
        return;
    };
    if now.map(|m| m.0) != Some(ev.muted) {
        commands.entity(e).insert(MicMuted(ev.muted));
    }
}

/// Tell the server when M flips (and once our player exists).
fn send_mic_state(
    mut commands: Commands,
    muted: Res<VoiceMuted>,
    me: Option<Res<LocalPlayer>>,
    players: Query<(&Player, Option<&MicMuted>)>,
    server: Res<State<ServerState>>,
    client: Res<State<ClientState>>,
) {
    let in_session =
        *server.get() == ServerState::Running || *client.get() == ClientState::Connected;
    let Some(me) = me else { return };
    if !in_session {
        return;
    }
    let Some((_, shown)) = players.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    // Send when what everyone sees differs from the truth (covers joining,
    // M presses, and a lost first message); at most once per change.
    if shown.map(|m| m.0).unwrap_or(false) != muted.0 && (muted.is_changed() || shown.is_none()) {
        commands.client_trigger(SetMicMuted { muted: muted.0 });
    }
}

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        // Added in the browser too (#106): `VoiceRepliconPlugin` must
        // register the same messages on every peer or browser and desktop
        // protocols wouldn't match.
        app.add_plugins(VoiceChatPlugin {
            config: voice_config(),
        })
        .add_plugins(VoiceRepliconPlugin::default());
        // The browser talks to Web Audio instead of cpal threads (#166).
        #[cfg(target_arch = "wasm32")]
        app.insert_resource(VoiceIo(std::sync::Arc::new(crate::voice_web::WebAudioIo)));
        app.init_resource::<VoiceMuted>()
            .add_observer(tag_speaker)
            .add_systems(Update, drive_voice.before(VoiceChatSystems::Devices))
            .add_systems(Update, send_mic_state.after(drive_voice))
            .add_systems(
                Update,
                keep_everyone_audible.before(VoiceChatSystems::Playback),
            );
    }
}

/// Quietest a voice gets however far away (linear gain).
pub const VOICE_FLOOR: f32 = 0.15;

/// Full voice within 3 m, fading out by 45 m, panned to where they stand.
pub fn voice_config() -> VoiceChatConfig {
    VoiceChatConfig {
        hearing: Hearing::Proximity,
        range: 45.0,
        full_volume_distance: 3.0,
        rolloff: 1.6,
        pan_strength: 0.7,
        output_queue_ms: OUTPUT_QUEUE_MS,
        ..default()
    }
}

/// Mixed voice queued ahead of the speakers. The browser's audio callback
/// takes about 21 ms at once and phone frames run long, so it keeps more.
pub const OUTPUT_QUEUE_MS: f32 = if cfg!(target_arch = "wasm32") {
    120.0
} else {
    40.0
};

/// Split interleaved stereo into left and right (the browser wants channels
/// apart). Stops at the shortest buffer.
pub fn deinterleave(stereo: &[f32], left: &mut [f32], right: &mut [f32]) {
    let pairs = stereo.as_chunks::<2>().0;
    for (([a, b], l), r) in pairs.iter().zip(left.iter_mut()).zip(right.iter_mut()) {
        *l = *a;
        *r = *b;
    }
}

/// The per-speaker range that makes `bevy_voice_chat`'s falloff
/// (`((range - d) / (range - full)) ^ rolloff`) come out at exactly `floor` at
/// distance `d`, when the normal range would make them quieter than that.
pub fn floor_range(d: f32, full: f32, range: f32, rolloff: f32, floor: f32) -> Option<f32> {
    if !(d.is_finite() && rolloff > 0.0 && floor > 0.0 && floor < 1.0) {
        return None;
    }
    let natural = if d >= range {
        0.0
    } else {
        ((range - d) / (range - full)).clamp(0.0, 1.0).powf(rolloff)
    };
    if natural >= floor {
        return None;
    }
    let q = floor.powf(1.0 / rolloff);
    Some(((d - q * full) / (1.0 - q)).max(range))
}

/// Stretch far speakers' range so they bottom out at [`VOICE_FLOOR`].
fn keep_everyone_audible(
    ears: Res<crate::spatial::Ears>,
    cfg: Res<VoiceChatConfig>,
    mut speakers: Query<(&GlobalTransform, &mut VoiceSpeaker)>,
) {
    for (at, mut speaker) in &mut speakers {
        let d = at.translation().distance(ears.pos);
        let want = floor_range(
            d,
            cfg.full_volume_distance,
            cfg.range,
            cfg.rolloff,
            VOICE_FLOOR,
        );
        if speaker.range != want {
            speaker.range = want;
        }
    }
}

pub fn drive_voice(
    keys: Res<ButtonInput<KeyCode>>,
    server: Res<State<ServerState>>,
    client: Res<State<ClientState>>,
    app_state: Option<Res<State<crate::AppState>>>,
    mut muted: ResMut<VoiceMuted>,
    mut input: ResMut<VoiceInput>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        muted.0 = !muted.0;
    }
    let in_session =
        *server.get() == ServerState::Running || *client.get() == ClientState::Connected;
    input.enabled = in_session;
    // Where people stand only means something in the world.
    input.positional = app_state.is_some_and(|s| *s.get() == crate::AppState::InGame);
    if muted.0 {
        input.mode = TalkMode::PushToTalk;
        input.talk_held = false;
    } else {
        input.mode = TalkMode::VoiceActivity;
    }
}

fn tag_speaker(add: On<Add<Player>>, players: Query<&Player>, mut commands: Commands) {
    if let Ok(player) = players.get(add.entity) {
        commands.entity(add.entity).insert(VoiceSpeaker {
            id: SpeakerId(player.id),
            range: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same falloff `bevy_voice_chat` uses.
    fn gain(d: f32, range: Option<f32>) -> f32 {
        let c = voice_config();
        bevy_voice_chat::spatial::spatial_gain(
            d,
            c.full_volume_distance,
            range.unwrap_or(c.range),
            c.rolloff,
        )
    }

    #[test]
    fn deinterleave_splits_channels() {
        let (mut l, mut r) = ([0.0; 3], [0.0; 3]);
        deinterleave(&[1.0, -1.0, 2.0, -2.0, 3.0, -3.0, 9.0], &mut l, &mut r);
        assert_eq!(l, [1.0, 2.0, 3.0]);
        assert_eq!(r, [-1.0, -2.0, -3.0]);
    }

    #[test]
    fn far_friends_stay_faintly_audible() {
        let c = voice_config();
        let range = |d| floor_range(d, c.full_volume_distance, c.range, c.rolloff, VOICE_FLOOR);
        assert_eq!(range(2.0), None, "close: untouched");
        assert!((gain(2.0, range(2.0)) - 1.0).abs() < 1e-6);
        let mut last = 1.0;
        for d in [5.0, 10.0, 20.0, 30.0, 44.0, 60.0, 200.0, 2000.0] {
            let g = gain(d, range(d));
            assert!(g >= VOICE_FLOOR - 1e-4, "{d} m: {g}");
            assert!(g <= last + 1e-6, "never louder further away");
            last = g;
        }
        assert!((gain(500.0, range(500.0)) - VOICE_FLOOR).abs() < 1e-3);
        assert!(gain(10.0, range(10.0)) > 0.4, "10 m is still a chat");
    }
}
