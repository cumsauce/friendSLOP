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

use crate::player::Player;

/// Press M to toggle. Muting stops your mic; you still hear others.
#[derive(Resource, Default, Debug)]
pub struct VoiceMuted(pub bool);

pub struct VoicePlugin;

impl Plugin for VoicePlugin {
    fn build(&self, app: &mut App) {
        // Added in the browser too (#106): cpal can't start its capture and
        // playback threads there, so a browser peer has no audio, but
        // `VoiceRepliconPlugin` must register the same messages on every peer
        // or browser and desktop protocols wouldn't match.
        app.add_plugins(VoiceChatPlugin {
            config: voice_config(),
        })
        .add_plugins(VoiceRepliconPlugin::default());
        #[cfg(target_arch = "wasm32")]
        app.add_systems(Startup, || {
            info!("voice chat has no audio in the browser yet (you can play, not talk)");
        });
        app.init_resource::<VoiceMuted>()
            .add_observer(tag_speaker)
            .add_systems(Update, drive_voice.before(VoiceChatSystems::Devices))
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
        ..default()
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

fn drive_voice(
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
