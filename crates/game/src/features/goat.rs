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

fn dress_goat(
    mut commands: Commands,
    new: Query<(Entity, &Goat), Added<Goat>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, goat) in &new {
        let wool = materials.add(Color::srgb(0.9, 0.88, 0.82));
        let horn = materials.add(Color::srgb(0.35, 0.3, 0.25));
        let eye = materials.add(Color::srgb(0.75, 0.55, 0.1));
        let leg = meshes.add(Cylinder::new(0.08, 0.7));
        let horn_mesh = meshes.add(Cone::new(0.07, 0.35));
        let eye_mesh = meshes.add(Sphere::new(0.06));
        commands
            .entity(entity)
            .insert((
                Transform::from_xyz(goat.home.x, 0.0, goat.home.y),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.6, 0.5, 1.1))),
                    MeshMaterial3d(wool.clone()),
                    Transform::from_xyz(0.0, 0.9, 0.0),
                ));
                for (x, z) in [(-0.2, -0.4), (0.2, -0.4), (-0.2, 0.4), (0.2, 0.4)] {
                    p.spawn((
                        Mesh3d(leg.clone()),
                        MeshMaterial3d(wool.clone()),
                        Transform::from_xyz(x, 0.35, z),
                    ));
                }
                p.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.35, 0.4, 0.5))),
                    MeshMaterial3d(wool.clone()),
                    Transform::from_xyz(0.0, 1.35, -0.65),
                ))
                .with_children(|h| {
                    for x in [-0.12, 0.12] {
                        h.spawn((
                            Mesh3d(horn_mesh.clone()),
                            MeshMaterial3d(horn.clone()),
                            Transform::from_xyz(x, 0.3, 0.1)
                                .with_rotation(Quat::from_rotation_x(0.5)),
                        ));
                        // Half-closed, unimpressed eyes.
                        h.spawn((
                            Mesh3d(eye_mesh.clone()),
                            MeshMaterial3d(eye.clone()),
                            Transform::from_xyz(x * 1.4, 0.05, -0.22)
                                .with_scale(Vec3::new(1.0, 0.4, 1.0)),
                        ));
                    }
                    // Beard.
                    h.spawn((
                        Mesh3d(meshes.add(Cone::new(0.08, 0.25))),
                        MeshMaterial3d(wool.clone()),
                        Transform::from_xyz(0.0, -0.3, -0.15)
                            .with_rotation(Quat::from_rotation_x(std::f32::consts::PI)),
                    ));
                });
            });
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
        Node {
            position_type: PositionType::Absolute,
            bottom: px(84),
            left: percent(30),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
}

/// What the goat last saw, to spot new choices.
#[derive(Default)]
struct Seen {
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
    goats: Query<(), With<Goat>>,
    players: Query<(), With<Player>>,
    mushrooms: Query<(), With<Mushroom>>,
    muted: Option<Res<VoiceMuted>>,
    mut seen: Local<Seen>,
    mut text: Query<&mut Text, With<GoatText>>,
) {
    let now = time.elapsed_secs();
    let players = players.iter().count();
    let mushrooms = mushrooms.iter().count();
    let muted = muted.is_some_and(|m| m.0);
    if goats.is_empty() {
        return;
    }
    if !seen.started {
        *seen = Seen {
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
    for mut text in &mut text {
        if let Some(line) = want {
            text.0 = format!("Goat: \"{line}\"");
        } else if now - seen.said_at > REMARK_SECS && !text.0.is_empty() {
            text.0.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
