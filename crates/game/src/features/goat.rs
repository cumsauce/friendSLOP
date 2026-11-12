//! A judgmental goat that joins the players and makes passive-aggressive
//! comments about every choice. From issue #20.
//!
//! The server spawns one replicated `Goat`. Each peer animates it locally and
//! watches the choices it can see (someone joins or leaves, the mushroom gets
//! eaten, you mute or unmute) and the goat comments on each one with on-screen
//! text. When nobody does anything for a while, it judges that too.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::mushroom::Mushroom;
use crate::features::random_dialogue;
use crate::player::Player;
use crate::voice::VoiceMuted;

/// The goat. `home` is where it stands and judges (x, z on the floor).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Goat {
    pub home: Vec2,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Goat>()
        .add_systems(OnEnter(ServerState::Running), spawn_goat);
}

fn spawn_goat(mut commands: Commands) {
    commands.spawn((
        Goat {
            home: Vec2::new(5.0, 4.0),
        },
        Replicated,
    ));
}

/// A choice the goat has noticed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Joined,
    Left,
    AteMushroom,
    Muted,
    Unmuted,
    DidNothing,
}

impl Choice {
    pub const ALL: [Choice; 6] = [
        Choice::Joined,
        Choice::Left,
        Choice::AteMushroom,
        Choice::Muted,
        Choice::Unmuted,
        Choice::DidNothing,
    ];
}

/// The goat's opinion of `choice`. `n` picks among its remarks so it doesn't
/// repeat itself every time.
pub fn remark(choice: Choice, n: usize) -> &'static str {
    let lines: &[&str] = match choice {
        Choice::Joined => &[
            "Oh, you came. Interesting choice.",
            "Another one. Wonderful. Truly.",
            "Nice of you to finally show up.",
        ],
        Choice::Left => &[
            "They left. No, it's fine. It's fine.",
            "Well. Someone had somewhere better to be.",
        ],
        Choice::AteMushroom => &[
            "You ate a floor mushroom. Bold. I'd never.",
            "Sure, eat the random mushroom. What could go wrong.",
            "I'm not saying it was a mistake. I'm just thinking it.",
        ],
        Choice::Muted => &[
            "Muting yourself. Probably for the best.",
            "Ah, the silent treatment. Very mature.",
        ],
        Choice::Unmuted => &[
            "Oh good, you're talking again.",
            "Unmuted. Brave, given last time.",
        ],
        Choice::DidNothing => &[
            "Standing around. Love that for you.",
            "No, take your time. I have all day. I'm a goat.",
            "Doing nothing is also a choice. A choice you made.",
        ],
    };
    lines[n % lines.len()]
}

/// How long a remark stays on screen.
const REMARK_SECS: f32 = 5.0;
/// How long of nobody choosing anything before the goat says so.
const IDLE_SECS: f32 = 40.0;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_goat_text)
        .add_systems(
            Update,
            (dress_goat, animate_goat, judge).run_if(in_state(AppState::InGame)),
        );
}

/// The rigged plush goat model (clips `idle`, `judge`).
pub const GOAT_MODEL: &str = "models/creatures/goat.gltf";

/// How long the goat's `judge` gesture (a look, a head shake, a hoof stomp) lasts.
pub const JUDGE_SECS: f32 = 2.4;

/// Give the goat (on every peer) its rigged plush model. A missing file only
/// makes it invisible: its remarks don't depend on it.
fn dress_goat(mut commands: Commands, new: Query<(Entity, &Goat), Added<Goat>>) {
    for (entity, goat) in &new {
        commands.entity(entity).insert((
            Transform::from_xyz(goat.home.x, 0.0, goat.home.y),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
            crate::rigged::Rigged { path: GOAT_MODEL },
            goat_clip(f32::INFINITY),
        ));
    }
}

/// The goat's clip `since` seconds after its last remark: it judges while it
/// talks, then goes back to idly chewing on its opinions.
pub fn goat_clip(since: f32) -> crate::rigged::RigClip {
    if (0.0..JUDGE_SECS).contains(&since) {
        crate::rigged::RigClip::once("judge").fade(0.2)
    } else {
        crate::rigged::RigClip::looping("idle").fade(0.4)
    }
}

/// Slowly, disapprovingly shake its head (well, its whole self).
fn animate_goat(time: Res<Time>, mut goats: Query<(&Goat, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (goat, mut transform) in &mut goats {
        transform.translation = Vec3::new(goat.home.x, 0.0, goat.home.y);
        transform.rotation = Quat::from_rotation_y(2.4 + (t * 0.7).sin() * 0.25);
    }
}

#[derive(Component)]
struct GoatText;

fn spawn_goat_text(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        GoatText,
        crate::hud::slot(crate::hud::HudSlot::Subtitle),
        DespawnOnExit(AppState::InGame),
    ));
}

/// What the goat last saw, to spot new choices.
#[derive(Default)]
struct Seen {
    /// The goat this memory belongs to: a new session spawns a new goat.
    goat: Option<Entity>,
    started: bool,
    players: usize,
    mushrooms: usize,
    muted: bool,
    count: usize,
    said_at: f32,
    last_choice_at: f32,
}

/// Spot choices and remark on them.
fn judge(
    time: Res<Time>,
    goats: Query<Entity, With<Goat>>,
    players: Query<(), With<Player>>,
    mushrooms: Query<(), With<Mushroom>>,
    muted: Option<Res<VoiceMuted>>,
    mut seen: Local<Seen>,
    mut text: Query<&mut Text, With<GoatText>>,
    mut clips: Query<&mut crate::rigged::RigClip, With<Goat>>,
) {
    let now = time.elapsed_secs();
    let players = players.iter().count();
    let mushrooms = mushrooms.iter().count();
    let muted = muted.is_some_and(|m| m.0);
    let Some(goat) = goats.iter().next() else {
        return;
    };
    // A different goat means a new session: forget the last one's judgments.
    if !seen.started || seen.goat != Some(goat) {
        *seen = Seen {
            goat: Some(goat),
            started: true,
            players,
            mushrooms,
            muted,
            said_at: f32::NEG_INFINITY,
            last_choice_at: now,
            ..default()
        };
    }
    let choice = if players > seen.players {
        Some(Choice::Joined)
    } else if players < seen.players {
        Some(Choice::Left)
    } else if mushrooms < seen.mushrooms {
        Some(Choice::AteMushroom)
    } else if muted != seen.muted {
        Some(if muted {
            Choice::Muted
        } else {
            Choice::Unmuted
        })
    } else if now - seen.last_choice_at > IDLE_SECS {
        Some(Choice::DidNothing)
    } else {
        None
    };
    seen.players = players;
    seen.mushrooms = mushrooms;
    seen.muted = muted;
    let mut want = None;
    if let Some(choice) = choice {
        want = Some(remark(choice, seen.count));
        seen.count += 1;
        seen.said_at = now;
        seen.last_choice_at = now;
    }
    // A fresh remark restarts the gesture, even mid-judgement.
    if let Ok(mut clip) = clips.get_mut(goat) {
        let next = goat_clip(now - seen.said_at);
        if want.is_some() || next.name != clip.name {
            *clip = next;
        }
    }
    for mut text in &mut text {
        if let Some(line) = want {
            text.0 = format!("Goat: \"{}\"", random_dialogue::say(line));
        } else if now - seen.said_at > REMARK_SECS && !text.0.is_empty() {
            text.0.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goat_judges_while_it_talks_then_idles() {
        assert_eq!(goat_clip(0.0).name, "judge");
        assert!(goat_clip(0.0).once);
        assert_eq!(goat_clip(JUDGE_SECS + 0.1).name, "idle");
        assert_eq!(goat_clip(f32::INFINITY).name, "idle");
        assert_eq!(goat_clip(f32::NEG_INFINITY).name, "idle");
    }

    #[test]
    fn goat_has_an_opinion_on_everything() {
        for choice in [
            Choice::Joined,
            Choice::Left,
            Choice::AteMushroom,
            Choice::Muted,
            Choice::Unmuted,
            Choice::DidNothing,
        ] {
            for n in 0..5 {
                assert!(!remark(choice, n).is_empty());
            }
            assert_ne!(remark(choice, 0), remark(choice, 1), "{choice:?} repeats");
        }
    }
}
