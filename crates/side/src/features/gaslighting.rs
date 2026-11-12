//! Gaslighting: villagers fart too, then quickly blame someone else.
//! From issue #168 by Kori.exe.
//!
//! Every [`MIN_SECS`]..[`MAX_SECS`] seconds the server picks a villager who
//! lets one go and points at the nearest player (the blame), and tells
//! everyone with [`NpcFarted`]. Every peer hears the fart from that villager
//! and gets a line saying who did it and who got blamed. The blame is just
//! talk: it doesn't count as being caught.

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::bear::wav_bytes;
use crate::features::fart_sound::{FartTone, fart_samples};
use crate::features::fps::Walker;
use crate::features::synth_audio::RATE;
use crate::features::villagers::{Villager, name};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player};

/// Shortest and longest gap between villager farts.
pub const MIN_SECS: f32 = 50.0;
pub const MAX_SECS: f32 = 110.0;

/// Server -> everyone: villager `villager` farted at `at` and blamed `blamed`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct NpcFarted {
    pub villager: u8,
    pub at: Vec2,
    pub blamed: u64,
}

/// Server only: seconds to the next villager fart, and a dice.
#[derive(Resource)]
struct Gut {
    next: f32,
    dice: u32,
}

impl Default for Gut {
    fn default() -> Self {
        Self {
            next: MIN_SECS,
            dice: 0x6a5_1197,
        }
    }
}

impl Gut {
    fn roll(&mut self) -> f32 {
        self.dice = self
            .dice
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.dice >> 8) as f32 / (1u32 << 24) as f32
    }
}

pub fn protocol(app: &mut App) {
    app.add_server_event::<NpcFarted>(Channel::Ordered)
        .init_resource::<Gut>()
        .add_systems(Update, rumble.run_if(in_state(ServerState::Running)));
}

/// The player nearest to `at` (the one who gets blamed).
pub fn scapegoat(at: Vec2, players: &[(u64, Vec2)]) -> Option<u64> {
    players
        .iter()
        .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
        .map(|(id, _)| *id)
}

fn rumble(
    time: Res<Time>,
    mut gut: ResMut<Gut>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    villagers: Query<&Villager>,
    walkers: Query<(&Player, &Walker)>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    gut.next -= time.delta_secs();
    if gut.next > 0.0 {
        return;
    }
    gut.next = MIN_SECS + gut.roll() * (MAX_SECS - MIN_SECS);
    let all: Vec<&Villager> = villagers.iter().collect();
    if all.is_empty() {
        return;
    }
    let v = all[(gut.roll() * all.len() as f32) as usize % all.len()];
    let players: Vec<(u64, Vec2)> = walkers.iter().map(|(p, w)| (p.id, w.pos.xz())).collect();
    let Some(blamed) = scapegoat(v.pos, &players) else {
        return;
    };
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: NpcFarted {
            villager: v.id,
            at: v.pos,
            blamed,
        },
    });
}

pub fn plugin(app: &mut App) {
    app.add_observer(gaslight);
}

/// The excuses, picked by the villager.
const EXCUSES: [&str; 5] = [
    "ew, that was them!",
    "who did that?? oh it was definitely them",
    "smells like someone ate at Wendy's",
    "don't look at me, look at THEM",
    "I would never. they would though",
];

fn gaslight(
    ev: On<NpcFarted>,
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<Toast>>,
    state: Option<Res<State<AppState>>>,
    audio: Option<Res<Assets<AudioSource>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    let name = name(ev.villager as usize);
    let excuse = EXCUSES[ev.villager as usize % EXCUSES.len()];
    let who = if me.is_some_and(|m| m.0 == ev.blamed) {
        "YOU".to_string()
    } else {
        format!("player {}", ev.blamed)
    };
    if let Some(mut toast) = toast {
        toast.show(format!(
            "{name} farted, then pointed at {who}: \"{excuse}\""
        ));
    }
    if audio.is_some() {
        // Each villager has its own tone.
        let k = ev.villager as f32;
        let tone = FartTone {
            pitch: 60.0 + (k * 37.0) % 160.0,
            length: 0.6 + (k * 0.31) % 1.2,
            timbre: (k * 0.17) % 1.0,
        }
        .clamped();
        let seed = 900 + ev.villager as u32;
        let at = Vec3::new(
            ev.at.x,
            crate::terrain::height_at(ev.at.x, ev.at.y) + 0.9,
            ev.at.y,
        );
        crate::spatial::play_wav_at(
            &mut commands,
            at,
            1.0,
            crate::spatial::Carry::LOUD,
            move || wav_bytes(&fart_samples(&tone, 0.8, seed, RATE), RATE),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_nearest_player_gets_blamed() {
        let ps = [(0, Vec2::new(10.0, 0.0)), (3, Vec2::new(2.0, 0.0))];
        assert_eq!(scapegoat(Vec2::ZERO, &ps), Some(3));
        assert_eq!(scapegoat(Vec2::ZERO, &[]), None);
    }
}
