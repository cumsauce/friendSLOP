//! Voice chat for everyone in the session (`bevy_voice_chat` over replicon).
//! Global hearing for now. Proximity voice is one `positional = true` away,
//! once players have positions.

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
        // Voice needs cpal's capture/playback threads, which a browser tab
        // doesn't have: the web build (#106) keeps the M toggle but no audio.
        #[cfg(not(target_arch = "wasm32"))]
        app.add_plugins(VoiceChatPlugin::default())
            .add_plugins(VoiceRepliconPlugin::default());
        #[cfg(target_arch = "wasm32")]
        app.init_resource::<VoiceInput>().add_systems(Startup, || {
            info!("voice chat is not available in the browser yet");
        });
        app.init_resource::<VoiceMuted>()
            .add_observer(tag_speaker)
            .add_systems(Update, drive_voice.before(VoiceChatSystems::Devices));
    }
}

fn drive_voice(
    keys: Res<ButtonInput<KeyCode>>,
    server: Res<State<ServerState>>,
    client: Res<State<ClientState>>,
    mut muted: ResMut<VoiceMuted>,
    mut input: ResMut<VoiceInput>,
) {
    if keys.just_pressed(KeyCode::KeyM) {
        muted.0 = !muted.0;
    }
    let in_session =
        *server.get() == ServerState::Running || *client.get() == ClientState::Connected;
    input.enabled = in_session;
    input.positional = false;
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
