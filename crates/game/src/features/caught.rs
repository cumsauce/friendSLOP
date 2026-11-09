//! What happens when the villagers catch you (#140, answered by just meat and
//! cheese): "it should catch and carry you, then pick another random player
//! and on their screen show 'your friend farted and some got on you. The poop
//! police is hunting you too as an accomplice'. Then while holding you it runs
//! towards the other player and once both are caught he levitates and
//! threatens how now they're being punished by being locked in the air with
//! him for x amount of time".
//!
//! During attack mode (`punishment.rs`), the first villager to get within
//! [`CATCH_DIST`] of the hunted player catches them: a replicated [`Caught`]
//! appears and the server pins the held body above the catcher's head. The
//! chase on you ends; the server picks another random player as the
//! accomplice, tells them so, and sets attack mode on them, and the catcher
//! runs at them with you in its arms. Once it reaches them too (or straight
//! away if you're alone), it rises [`LIFT`] m into the air with everyone it
//! holds and keeps them there for [`FLOAT_SECS`], then lets go.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::punishment::{ATTACK_SECS, AttackMode};

use crate::features::villagers::Villager;
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player};

/// A chasing villager this close (on the floor) catches you.
pub const CATCH_DIST: f32 = 1.4;
/// How fast the catcher runs with you in its arms.
pub const CARRY_SPEED: f32 = 4.5;
/// How high the catcher levitates.
pub const LIFT: f32 = 7.0;
/// How long you're locked in the air ("x amount of time").
pub const FLOAT_SECS: f32 = 20.0;
/// Rise speed while levitating.
const RISE_SPEED: f32 = 2.0;

/// The accomplice's message, word for word from the friend.
pub const ACCOMPLICE_TEXT: &str =
    "your friend farted and some got on you. The poop police is hunting you too as an accomplice";

/// Server -> everyone: villager `catcher` has caught `held` and is either
/// running at `accomplice` or floating `lift` m up for `secs_left` more seconds.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Caught {
    pub catcher: u8,
    pub held: Vec<u64>,
    pub accomplice: Option<u64>,
    pub lift: f32,
    pub secs_left: u8,
}

/// Server -> one or all clients: a line about the catch.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct CaughtNotice {
    pub text: String,
}

/// Server only: seconds spent carrying, and precise float time left (`None`
/// while still carrying).
#[derive(Component, Debug, Default)]
struct FloatClock {
    carrying: f32,
    left: Option<f32>,
}

/// Server only: picks the accomplice.
#[derive(Resource)]
struct Dice(u32);

impl Default for Dice {
    fn default() -> Self {
        Self(0x0acc_01ce)
    }
}

impl Dice {
    fn pick(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as usize % n.max(1)
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<Caught>()
        .add_server_event::<CaughtNotice>(Channel::Ordered)
        .init_resource::<Dice>()
        .add_systems(
            Update,
            (
                catch,
                carry,
                hold_bodies.after(crate::features::fps::WalkSet),
            )
                .chain()
                .run_if(in_state(ServerState::Running)),
        );
}

/// Is villager `id` busy holding someone? (Its brain leaves it alone then.)
pub fn is_catcher(caught: &Query<&Caught>, id: u8) -> bool {
    caught.iter().any(|c| c.catcher == id)
}

/// How high villager `id` floats right now (0 when it isn't).
pub fn lift_of(caught: &Query<&Caught>, id: u8) -> f32 {
    caught
        .iter()
        .find(|c| c.catcher == id)
        .map_or(0.0, |c| c.lift)
}

/// The nearest villager within reach of `at`, if any.
pub fn catcher_near(at: Vec2, villagers: &[(u8, Vec2)]) -> Option<u8> {
    villagers
        .iter()
        .filter(|(_, p)| p.distance(at) <= CATCH_DIST)
        .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
        .map(|(id, _)| *id)
}

fn notice(commands: &mut Commands, targets: SendTargets, text: String) {
    commands.server_trigger(ToClients {
        targets,
        message: CaughtNotice { text },
    });
}

/// Who to send a player's message to.
fn target_of(
    id: u64,
    players: &Query<(&Player, Option<&crate::player::PlayerOwner>)>,
) -> SendTargets {
    let owner = players
        .iter()
        .find(|(p, _)| p.id == id)
        .and_then(|(_, o)| o.and_then(|o| o.0));
    match owner {
        Some(e) => SendTargets::Single(ClientId::Client(e)),
        None => SendTargets::Single(ClientId::Server),
    }
}

/// Server: a chaser that reaches the hunted player grabs them.
#[allow(clippy::too_many_arguments)]
fn catch(
    mut commands: Commands,
    mut dice: ResMut<Dice>,
    attacks: Query<(Entity, &AttackMode)>,
    caught: Query<&Caught>,
    villagers: Query<&Villager>,
    walkers: Query<(&Player, &Walker)>,
    players: Query<(&Player, Option<&crate::player::PlayerOwner>)>,
) {
    for (attack_entity, attack) in &attacks {
        // Already held by someone? Then this is the accomplice's chase; the
        // catcher (not the crowd) is the one coming for them.
        if caught.iter().any(|c| c.held.contains(&attack.target)) {
            continue;
        }
        if caught.iter().any(|c| c.accomplice == Some(attack.target)) {
            continue;
        }
        let Some(at) = walkers
            .iter()
            .find(|(p, _)| p.id == attack.target)
            .map(|(_, w)| w.pos.xz())
        else {
            continue;
        };
        let free: Vec<(u8, Vec2)> = villagers
            .iter()
            .filter(|v| !is_catcher(&caught, v.id))
            .map(|v| (v.id, v.pos))
            .collect();
        let Some(catcher) = catcher_near(at, &free) else {
            continue;
        };
        commands.entity(attack_entity).despawn();
        let others: Vec<u64> = walkers
            .iter()
            .map(|(p, _)| p.id)
            .filter(|id| *id != attack.target && !caught.iter().any(|c| c.held.contains(id)))
            .collect();
        let accomplice = (!others.is_empty()).then(|| others[dice.pick(others.len())]);
        let name = villagers
            .iter()
            .find(|v| v.id == catcher)
            .map_or("a villager", |v| v.name());
        let caught_text = match accomplice {
            Some(a) => format!("{name} caught you and is carrying you off to get player {a}!"),
            None => format!("{name} caught you!"),
        };
        notice(
            &mut commands,
            target_of(attack.target, &players),
            caught_text,
        );
        if let Some(a) = accomplice {
            notice(
                &mut commands,
                target_of(a, &players),
                ACCOMPLICE_TEXT.into(),
            );
            super::punishment::start_attack(&mut commands, a);
        }
        commands.spawn((
            Caught {
                catcher,
                held: vec![attack.target],
                accomplice,
                lift: 0.0,
                secs_left: FLOAT_SECS as u8,
            },
            FloatClock::default(),
            Replicated,
        ));
    }
}

/// Server: run at the accomplice, grab them, then float, then let go.
#[allow(clippy::too_many_arguments)]
fn carry(
    time: Res<Time>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    mut caught: Query<(Entity, &mut Caught, &mut FloatClock)>,
    mut villagers: Query<&mut Villager>,
    walkers: Query<(&Player, &Walker)>,
    attacks: Query<(Entity, &AttackMode)>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let dt = time.delta_secs().min(0.1);
    for (entity, mut c, mut clock) in &mut caught {
        let Some(mut v) = villagers.iter_mut().find(|v| v.id == c.catcher) else {
            commands.entity(entity).despawn();
            continue;
        };
        match clock.left {
            None => {
                clock.carrying += dt;
                let prey = c.accomplice.and_then(|a| {
                    walkers
                        .iter()
                        .find(|(p, _)| p.id == a)
                        .map(|(_, w)| w.pos.xz())
                });
                let grabbed = match prey {
                    // The accomplice left the game (or outran us for a whole
                    // attack): float with who we have.
                    None => true,
                    Some(_) if clock.carrying > ATTACK_SECS => {
                        c.accomplice = None;
                        true
                    }
                    Some(p) => {
                        let to = p - v.pos;
                        let d = to.length();
                        if d > CATCH_DIST {
                            let dir = to / d;
                            v.pos += dir * (CARRY_SPEED * dt).min(d);
                            v.yaw = (-dir.x).atan2(-dir.y);
                            v.sitting = false;
                            false
                        } else {
                            true
                        }
                    }
                };
                if grabbed {
                    if let Some(a) = c.accomplice.take() {
                        c.held.push(a);
                        for (e, attack) in &attacks {
                            if attack.target == a {
                                commands.entity(e).despawn();
                            }
                        }
                    }
                    clock.left = Some(FLOAT_SECS);
                    let text = format!(
                        "{}: \"now you're being punished. you're locked up here in the air \
                         with ME for {} seconds\"",
                        v.name(),
                        FLOAT_SECS as u32
                    );
                    notice(&mut commands, SendTargets::All, text);
                }
            }
            Some(mut left) => {
                left -= dt;
                clock.left = Some(left);
                let lift = (c.lift + RISE_SPEED * dt).min(LIFT);
                if c.lift != lift {
                    c.lift = lift;
                }
                let secs = left.max(0.0).ceil() as u8;
                if c.secs_left != secs {
                    c.secs_left = secs;
                }
                if left <= 0.0 {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

/// Server: held bodies stay in the catcher's arms (above its head, side by
/// side), wherever their owners try to walk.
fn hold_bodies(
    caught: Query<&Caught>,
    villagers: Query<&Villager>,
    mut walkers: Query<(&Player, &mut Walker, Option<&mut crate::kcc::Kcc>)>,
) {
    for c in &caught {
        let Some(v) = villagers.iter().find(|v| v.id == c.catcher) else {
            continue;
        };
        let ground = crate::terrain::height_at(v.pos.x, v.pos.y);
        let n = c.held.len() as f32;
        for (i, id) in c.held.iter().enumerate() {
            let Some((_, mut w, kcc)) = walkers.iter_mut().find(|(p, ..)| p.id == *id) else {
                continue;
            };
            let side = (i as f32 - (n - 1.0) / 2.0) * 0.9;
            let right = Vec2::from_angle(-v.yaw) * side;
            let pos = Vec3::new(v.pos.x + right.x, ground + c.lift + 1.9, v.pos.y + right.y);
            if w.pos != pos {
                w.pos = pos;
            }
            if let Some(mut kcc) = kcc {
                kcc.velocity = Vec3::ZERO;
            }
        }
    }
}

pub fn plugin(app: &mut App) {
    app.add_observer(show_notice)
        .add_systems(OnEnter(AppState::InGame), spawn_ui)
        .add_systems(
            Update,
            show_caught
                .run_if(crate::world::hud_tick())
                .run_if(in_state(AppState::InGame)),
        );
}

fn show_notice(ev: On<CaughtNotice>, toast: Option<ResMut<Toast>>) {
    if let Some(mut toast) = toast {
        toast.show(ev.text.clone());
    }
}

#[derive(Component)]
struct CaughtText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextColor(crate::theme::ROSE_RED),
        CaughtText,
        crate::hud::slot(crate::hud::HudSlot::Toasts),
        DespawnOnExit(AppState::InGame),
    ));
}

/// While you're held or hunted as an accomplice, say so.
fn show_caught(
    me: Option<Res<LocalPlayer>>,
    caught: Query<&Caught>,
    mut text: Query<&mut Text, With<CaughtText>>,
) {
    let me = me.map(|m| m.0);
    let want = caught
        .iter()
        .find_map(|c| {
            let held = me.is_some_and(|m| c.held.contains(&m));
            let next = me.is_some() && c.accomplice == me;
            if c.accomplice.is_none() && (held || c.lift > 0.0) {
                Some(format!("LOCKED IN THE AIR ({})", c.secs_left))
            } else if held {
                Some("CAUGHT: you're being carried off".to_string())
            } else if next {
                Some("ACCOMPLICE: the poop police is coming for you too".to_string())
            } else {
                None
            }
        })
        .unwrap_or_default();
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_villager_within_reach_catches_you() {
        let vs = [(1, Vec2::new(3.0, 0.0)), (2, Vec2::new(1.0, 0.0))];
        assert_eq!(catcher_near(Vec2::ZERO, &vs), Some(2));
        assert_eq!(catcher_near(Vec2::new(-5.0, 0.0), &vs), None);
    }
}
