//! Where the clues lead (#279, Horror ARG 3/3, catherine's #182).
//!
//! In the middle of the ruins' collapsed tower an old TV lies half buried,
//! dark. Once the commune has found every piece of the Watcher's message
//! (`clues`), the TV answers a crowd: when enough of you stand around it
//! together, the server sends a [`Broadcast`] and on every peer the screen
//! lights up, static crackles and voices whisper out of it while the
//! broadcast's lines show on screen. The last line is a riddle in a simple
//! cipher that points outside the game, at `SIGNAL.txt` in the game's
//! repository: something friends can find and pass around, the ARG part.
//!
//! The lines are writing: `assets/data/text/broadcast.ron`.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::clues::{Clues, WRITING as CLUE_WRITING};
use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::fps::Walker;
use crate::features::synth_audio::RATE;
use crate::features::whispers::whisper_samples;
use crate::spatial::{Carry, play_wav_at};

/// Tuning numbers: `assets/data/side/broadcast.ron` (read at run time, edit without a rebuild).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// How close to the TV counts as gathered round it, in metres.
    pub radius: f32,
    /// How many must gather (or everyone, if fewer are playing).
    pub crowd: usize,
    /// Seconds before it will broadcast again.
    pub cooldown_secs: f32,
    /// Seconds each line stays on screen.
    pub line_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/broadcast.ron");

/// The broadcast: `assets/data/text/broadcast.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Writing {
    /// What it says, a line at a time.
    pub lines: Vec<String>,
    /// The last line, written in ROT13 (letters turned 13 places): the riddle
    /// that points outside the game.
    pub cipher: String,
}

pub static WRITING: friendslop_data::Data<Writing> =
    friendslop_data::Data::ron("data/text/broadcast.ron");

/// Server -> everyone: the TV in the ruins is broadcasting.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Broadcast;

/// Where the TV lies: the middle of the ruins' tower.
pub fn tv_at() -> Vec2 {
    friendslop_world::places::ruins_tower_at()
}

pub fn protocol(app: &mut App) {
    app.add_server_event::<Broadcast>(Channel::Ordered)
        .add_systems(Update, gather.run_if(in_state(ServerState::Running)));
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), (spawn_tv, spawn_text))
        .add_systems(Update, play.run_if(in_state(AppState::InGame)))
        .add_observer(start);
}

/// Has the commune found every piece of a message with `total` pieces?
pub fn complete(total: usize, found: &[u8]) -> bool {
    total > 0 && (0..total.min(u8::MAX as usize) as u8).all(|p| found.contains(&p))
}

/// Does a crowd of `near` (of `playing`) answer, needing `crowd`?
pub fn crowd_gathered(near: usize, playing: usize, crowd: usize) -> bool {
    playing > 0 && near >= crowd.min(playing).max(1)
}

/// ROT13: letters turned 13 places, everything else kept. Its own inverse.
pub fn rot13(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'a'..='z' => (((c as u8 - b'a') + 13) % 26 + b'a') as char,
            'A'..='Z' => (((c as u8 - b'A') + 13) % 26 + b'A') as char,
            _ => c,
        })
        .collect()
}

/// Server: when the message is whole and a crowd stands round the TV, it
/// broadcasts (then rests for `cooldown_secs`).
fn gather(
    mut commands: Commands,
    time: Res<Time>,
    clues: Query<&Clues>,
    walkers: Query<&Walker>,
    mut rest: Local<f32>,
) {
    *rest -= time.delta_secs();
    if *rest > 0.0 {
        return;
    }
    let found = clues
        .iter()
        .next()
        .map(|c| c.found.as_slice())
        .unwrap_or(&[]);
    if !complete(CLUE_WRITING.get().pieces.len(), found) {
        return;
    }
    let t = TUNING.get();
    let tv = tv_at();
    let near = walkers
        .iter()
        .filter(|w| w.pos.xz().distance(tv) <= t.radius)
        .count();
    if !crowd_gathered(near, walkers.iter().count(), t.crowd) {
        return;
    }
    *rest = t.cooldown_secs.max(5.0);
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: Broadcast,
    });
}

/// The TV's screen, lit while it broadcasts.
#[derive(Component)]
struct Screen(Handle<StandardMaterial>);

/// Every peer: the dark TV half sunk in the tower floor, its screen tilted up.
/// Placeholder primitives in flat colours.
fn spawn_tv(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let at = tv_at();
    let ground = crate::terrain::height_at(at.x, at.y);
    let casing = materials.add(StandardMaterial {
        base_color: Color::srgb(0.24, 0.2, 0.16),
        perceptual_roughness: 0.85,
        ..default()
    });
    let glass = materials.add(StandardMaterial {
        base_color: Color::srgb(0.04, 0.05, 0.05),
        perceptual_roughness: 0.2,
        ..default()
    });
    commands
        .spawn((
            Transform::from_xyz(at.x, ground - 0.15, at.y)
                .with_rotation(Quat::from_rotation_y(0.6) * Quat::from_rotation_x(-0.35)),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|c| {
            c.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.8, 0.6, 0.6))),
                MeshMaterial3d(casing),
                Transform::from_xyz(0.0, 0.3, 0.0),
            ));
            c.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.62, 0.44, 0.02))),
                MeshMaterial3d(glass.clone()),
                Transform::from_xyz(0.0, 0.32, 0.305),
                Screen(glass),
            ));
        });
}

#[derive(Component)]
struct BroadcastText;

fn spawn_text(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextColor(Color::srgb(0.75, 0.95, 0.85)),
        BroadcastText,
        crate::hud::slot(crate::hud::HudSlot::CenterBanner),
        DespawnOnExit(AppState::InGame),
    ));
}

/// This peer's broadcast in progress: seconds since it started.
#[derive(Resource)]
struct OnAir(f32);

/// Every peer: the TV comes on, crackles and whispers.
fn start(_: On<Broadcast>, mut commands: Commands, state: Option<Res<State<AppState>>>) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    commands.insert_resource(OnAir(0.0));
    let at = tv_at();
    let pos = Vec3::new(at.x, crate::terrain::height_at(at.x, at.y) + 0.4, at.y);
    play_wav_at(&mut commands, pos, 0.7, Carry::LOUD, || {
        wav_bytes(&super::watcher::static_samples(1.6, RATE), RATE)
    });
    let seed = crate::settings::since_epoch().subsec_nanos() as u64;
    for i in 0..2u64 {
        play_wav_at(&mut commands, pos, 0.8, Carry::NEARBY, move || {
            wav_bytes(&whisper_samples(seed.wrapping_add(i * 977), RATE), RATE)
        });
    }
}

/// The line showing `secs` into the broadcast, or `None` once it's over:
/// each line in turn, then the cipher.
pub fn line_at(w: &Writing, line_secs: f32, secs: f32) -> Option<String> {
    let i = (secs / line_secs.max(0.5)) as usize;
    w.lines
        .get(i)
        .cloned()
        .or_else(|| (i == w.lines.len()).then(|| w.cipher.clone()))
}

fn play(
    mut commands: Commands,
    time: Res<Time>,
    on_air: Option<ResMut<OnAir>>,
    mut text: Query<&mut Text, With<BroadcastText>>,
    screens: Query<&Screen>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let Some(mut on_air) = on_air else {
        return;
    };
    on_air.0 += time.delta_secs();
    let line = line_at(&WRITING.get(), TUNING.get().line_secs, on_air.0);
    let flicker = (on_air.0 * 37.0).sin() * 0.5 + 0.5;
    for screen in &screens {
        let Some(mut m) = materials.get_mut(&screen.0) else {
            continue;
        };
        m.emissive = if line.is_some() {
            LinearRgba::rgb(0.6, 0.9, 0.8) * (1.5 + 2.0 * flicker)
        } else {
            LinearRgba::BLACK
        };
    }
    if let Ok(mut text) = text.single_mut() {
        text.0 = line.clone().unwrap_or_default();
    }
    if line.is_none() {
        commands.remove_resource::<OnAir>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_needs_the_whole_message() {
        assert!(!complete(0, &[]));
        assert!(!complete(3, &[0, 2]));
        assert!(complete(3, &[2, 0, 1]));
    }

    #[test]
    fn it_answers_a_crowd_or_everyone() {
        assert!(!crowd_gathered(1, 4, 2));
        assert!(crowd_gathered(2, 4, 2));
        // Alone in the session: you are the crowd.
        assert!(crowd_gathered(1, 1, 2));
        assert!(!crowd_gathered(0, 1, 2));
        assert!(!crowd_gathered(0, 0, 2));
    }

    #[test]
    fn lines_then_cipher_then_off() {
        let w = Writing {
            lines: vec!["a".into(), "b".into()],
            cipher: "c".into(),
        };
        assert_eq!(line_at(&w, 4.0, 1.0).as_deref(), Some("a"));
        assert_eq!(line_at(&w, 4.0, 5.0).as_deref(), Some("b"));
        assert_eq!(line_at(&w, 4.0, 9.0).as_deref(), Some("c"));
        assert_eq!(line_at(&w, 4.0, 13.0), None);
    }

    #[test]
    fn the_cipher_points_at_the_signal_file() {
        assert_eq!(rot13(&rot13("Hello, Commune!")), "Hello, Commune!");
        let plain = rot13(&WRITING.get().cipher);
        assert!(plain.contains("SIGNAL.txt"), "{plain}");
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../SIGNAL.txt");
        assert!(repo.exists(), "the riddle's file is missing");
    }
}
