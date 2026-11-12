//! Sesbian lex (#1 by Freyja-moth): a group smooch pile with climbing heat.
//!
//! Press L where you stand to start a pile, L near one to join it, L again to
//! peel off (walking away does it too). The request goes to the server as a
//! client event, so the server stays authoritative. The pile is one
//! replicated `SesbianLex` entity: where it is, who's in it in join order,
//! and a heat level that climbs once a second while two or more are piling in
//! and collapses when someone leaves. Join order hands out titles, straight
//! from the issue: first in is daddy, second the he/him butch, everyone after
//! rotates through a few more. Every peer shows hearts swirling over the pile
//! (more, higher and brighter as it heats up), a pink glow, and a line of
//! narration. Suggestive comedy by design, never explicit: the "no holds
//! barred" part of the issue is not something we build.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Heat stops here. Escalation has a comedy ceiling.
pub const MAX_HEAT: u32 = 10;
/// How close you need to be to the pile to pile in.
pub const JOIN_RANGE: f32 = 8.0;
/// Wander further than this and you've peeled off.
pub const DRIFT_RANGE: f32 = 14.0;

/// The lex pile. At most one exists at a time; the server owns it.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SesbianLex {
    /// Where it formed (where its first member stood).
    pub at: Vec2,
    /// Player ids in join order: index 0 is daddy, index 1 the he/him butch.
    pub participants: Vec<u64>,
    /// 0..=MAX_HEAT; climbs while 2+ participants are in the pile.
    pub heat: u32,
}

/// Client intent: put me in the pile (starting one if there isn't one), or
/// take me out.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SesbianLexToggle;

/// Server-local: seconds of company since the heat last collapsed, so `heat`
/// only changes (and replicates) once a second.
#[derive(Component, Default)]
struct HeatAccum(f32);

/// Titles after the first two, by join order.
const EXTRA_ROLES: [&str; 4] = [
    "the comfortable one",
    "morally involved bystander",
    "just here for the organs",
    "voice of reason (overruled)",
];

pub fn role(index: usize) -> &'static str {
    match index {
        0 => "daddy",
        1 => "he/him butch",
        _ => EXTRA_ROLES[(index - 2) % EXTRA_ROLES.len()],
    }
}

/// What the server should do to the pile after someone presses L.
#[derive(Debug, Clone, PartialEq)]
pub enum PileUpdate {
    /// No pile existed: start one where they stand.
    Spawn(SesbianLex),
    /// Change the existing pile to this.
    Update(SesbianLex),
    /// The pile is empty: remove it.
    Despawn,
    /// They're not near enough to join.
    TooFar,
}

/// Pure toggle logic: player `id`, standing at `pos`, presses L.
pub fn toggle_pile(id: u64, pos: Vec2, pile: Option<&SesbianLex>) -> PileUpdate {
    let Some(pile) = pile else {
        return PileUpdate::Spawn(SesbianLex {
            at: pos,
            participants: vec![id],
            heat: 0,
        });
    };
    if let Some(i) = pile.participants.iter().position(|&p| p == id) {
        let mut without = pile.clone();
        without.participants.remove(i);
        without.heat = 0;
        return if without.participants.is_empty() {
            PileUpdate::Despawn
        } else {
            PileUpdate::Update(without)
        };
    }
    if pos.distance(pile.at) > JOIN_RANGE {
        return PileUpdate::TooFar;
    }
    let mut with = pile.clone();
    with.participants.push(id);
    PileUpdate::Update(with)
}

/// Advance the heat by `dt` seconds.
fn next_heat(pile: &mut SesbianLex, accum: &mut HeatAccum, dt: f32) {
    if pile.participants.len() < 2 {
        pile.heat = 0;
        accum.0 = 0.0;
        return;
    }
    accum.0 += dt;
    let heat = (accum.0 as u32).min(MAX_HEAT);
    if pile.heat != heat {
        pile.heat = heat;
    }
}

/// The narration line for the pile, naming `me` as "you".
pub fn status_line(pile: &SesbianLex, me: Option<u64>) -> String {
    let who: Vec<String> = pile
        .participants
        .iter()
        .enumerate()
        .map(|(i, &id)| {
            let name = if Some(id) == me {
                "you".to_string()
            } else {
                format!("player {id}")
            };
            format!("{name} ({})", role(i))
        })
        .collect();
    if who.len() == 1 {
        return format!(
            "{} warming up for the sesbian lex... [L] nearby to join",
            who[0]
        );
    }
    let tier = match pile.heat {
        0 => "holding hands at a responsible pace",
        1..=3 => "the eye contact has turned prolonged",
        4..=6 => "a cold urn has been deployed for bystanders",
        7..=9 => "this has escalated beyond handholding territory",
        _ => "MAXIMUM HEAT - avert your eyes or join the pile",
    };
    format!("sesbian lex: {} - {tier}", who.join(" + "))
}

pub fn protocol(app: &mut App) {
    app.replicate::<SesbianLex>()
        .add_client_event::<SesbianLexToggle>(Channel::Ordered)
        .add_observer(on_toggle)
        .add_systems(
            Update,
            (drift_apart, advance_heat)
                .chain()
                .run_if(in_state(ServerState::Running)),
        );
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_visuals)
        .add_systems(
            Update,
            (lex_key, show_pile, narrate.run_if(crate::world::hud_tick()))
                .run_if(in_state(AppState::InGame)),
        );
}

/// Server: someone pressed L.
fn on_toggle(
    ev: On<FromClient<SesbianLexToggle>>,
    mut commands: Commands,
    players: Query<(&Player, &PlayerOwner, &Walker)>,
    mut piles: Query<(Entity, &mut SesbianLex)>,
    still: Query<&super::stillness::Stillness>,
) {
    if super::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((player, _, walker)) = players.iter().find(|(_, o, _)| o.0 == owner) else {
        return;
    };
    let current = piles.iter().next().map(|(e, p)| (e, p.clone()));
    match toggle_pile(player.id, walker.pos.xz(), current.as_ref().map(|(_, p)| p)) {
        PileUpdate::Spawn(pile) => {
            commands.spawn((pile, HeatAccum::default(), Replicated));
        }
        PileUpdate::Update(pile) => {
            if let Some((e, _)) = current
                && let Ok((_, mut now)) = piles.get_mut(e)
            {
                *now = pile;
            }
        }
        PileUpdate::Despawn => {
            if let Some((e, _)) = current {
                commands.entity(e).despawn();
            }
        }
        PileUpdate::TooFar => {
            super::interact::deny(
                &mut commands,
                ev.client_id,
                "walk over to the pile to join the sesbian lex",
            );
        }
    }
}

/// Server: anyone who left the game or wandered off has peeled off.
fn drift_apart(
    mut commands: Commands,
    players: Query<(&Player, &Walker)>,
    mut piles: Query<(Entity, &mut SesbianLex, &mut HeatAccum)>,
) {
    for (entity, mut pile, mut accum) in &mut piles {
        let at = pile.at;
        let stays = |id: &u64| {
            players
                .iter()
                .any(|(p, w)| p.id == *id && w.pos.xz().distance(at) <= DRIFT_RANGE)
        };
        if pile.participants.iter().all(stays) {
            continue;
        }
        pile.participants.retain(stays);
        pile.heat = 0;
        accum.0 = 0.0;
        if pile.participants.is_empty() {
            commands.entity(entity).despawn();
        }
    }
}

/// Server: heat climbs once a second while two or more are piling in.
fn advance_heat(time: Res<Time>, mut piles: Query<(&mut SesbianLex, &mut HeatAccum)>) {
    for (mut pile, mut accum) in &mut piles {
        let mut next = pile.clone();
        next_heat(&mut next, &mut accum, time.delta_secs());
        if *pile != next {
            *pile = next;
        }
    }
}

fn lex_key(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::KeyL) {
        commands.client_trigger(SesbianLexToggle);
    }
}

const HEART_COUNT: usize = 16;

#[derive(Component)]
struct LexHeart(usize);

#[derive(Component)]
struct LexGlow;

#[derive(Component)]
struct LexLine;

fn spawn_visuals(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    commands.spawn((
        Text::new(""),
        LexLine,
        crate::hud::slot(crate::hud::HudSlot::BottomLeft),
        DespawnOnExit(AppState::InGame),
    ));
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let pink = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.36, 0.55),
        emissive: LinearRgba::rgb(0.6, 0.12, 0.25),
        ..default()
    });
    let deeper = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.18, 0.38),
        emissive: LinearRgba::rgb(0.4, 0.05, 0.15),
        ..default()
    });
    let lobe = meshes.add(Sphere::new(0.16));
    let tip = meshes.add(Cuboid::new(0.24, 0.24, 0.14));
    for i in 0..HEART_COUNT {
        commands
            .spawn((
                LexHeart(i),
                Visibility::Hidden,
                Transform::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|heart| {
                for x in [-0.14, 0.14] {
                    heart.spawn((
                        Mesh3d(lobe.clone()),
                        MeshMaterial3d(pink.clone()),
                        Transform::from_xyz(x, 0.08, 0.0),
                    ));
                }
                heart.spawn((
                    Mesh3d(tip.clone()),
                    MeshMaterial3d(deeper.clone()),
                    Transform::from_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)),
                ));
            });
    }
    commands.spawn((
        LexGlow,
        PointLight {
            color: Color::srgb(1.0, 0.55, 0.7),
            intensity: 60_000.0,
            range: 10.0,
            ..default()
        },
        Transform::default(),
        Visibility::Hidden,
        DespawnOnExit(AppState::InGame),
    ));
}

/// Where heart `i` floats over the pile's middle, given its heat and the time.
fn heart_placement(i: usize, heat: u32, t: f32) -> Transform {
    let fi = i as f32;
    let layer = fi / HEART_COUNT as f32;
    let angle = fi * 2.4 + t * (0.4 + heat as f32 * 0.08);
    let radius = 0.8 + layer * 0.8 + heat as f32 * 0.06;
    let bob = (t * 1.6 + fi * 0.9).sin() * 0.12;
    let y = 1.9 + layer * (0.5 + heat as f32 * 0.15) + bob;
    let scale = 0.7 + layer * 0.5 + (t * 2.5 + fi).sin().max(0.0) * 0.15;
    Transform::from_translation(Vec3::new(angle.cos() * radius, y, angle.sin() * radius))
        .with_scale(Vec3::splat(scale))
}

fn show_pile(
    time: Res<Time>,
    piles: Query<&SesbianLex>,
    mut hearts: Query<(&LexHeart, &mut Transform, &mut Visibility), Without<LexGlow>>,
    mut glow: Query<(&mut Transform, &mut Visibility, &mut PointLight), With<LexGlow>>,
) {
    let Some(pile) = piles.iter().next() else {
        for (_, _, mut vis) in &mut hearts {
            vis.set_if_neq(Visibility::Hidden);
        }
        for (_, mut vis, _) in &mut glow {
            vis.set_if_neq(Visibility::Hidden);
        }
        return;
    };
    let ground = Vec3::new(
        pile.at.x,
        crate::terrain::height_at(pile.at.x, pile.at.y),
        pile.at.y,
    );
    let lit = (pile.participants.len() + pile.heat as usize).min(HEART_COUNT);
    let t = time.elapsed_secs();
    for (heart, mut tf, mut vis) in &mut hearts {
        if heart.0 < lit {
            vis.set_if_neq(Visibility::Visible);
            let mut at = heart_placement(heart.0, pile.heat, t);
            at.translation += ground;
            *tf = at;
        } else {
            vis.set_if_neq(Visibility::Hidden);
        }
    }
    for (mut tf, mut vis, mut light) in &mut glow {
        vis.set_if_neq(Visibility::Visible);
        tf.translation = ground + Vec3::Y * 1.8;
        light.intensity = 40_000.0 + 12_000.0 * pile.heat as f32;
    }
}

fn narrate(
    piles: Query<&SesbianLex>,
    me: Option<Res<LocalPlayer>>,
    mut line: Query<&mut Text, With<LexLine>>,
) {
    let want = piles.iter().next().map_or(String::new(), |p| {
        status_line(p, me.as_deref().map(|m| m.0))
    });
    for mut text in &mut line {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn pile(ids: &[u64]) -> SesbianLex {
        SesbianLex {
            at: Vec2::ZERO,
            participants: ids.to_vec(),
            heat: 0,
        }
    }

    #[test]
    fn first_two_roles_match_the_issue() {
        assert_eq!(role(0), "daddy");
        assert_eq!(role(1), "he/him butch");
        for i in 2..8 {
            assert_ne!(role(i), "daddy");
            assert_ne!(role(i), "he/him butch");
        }
    }

    #[test]
    fn joining_and_leaving_the_pile() {
        let here = Vec2::new(3.0, 4.0);
        let PileUpdate::Spawn(first) = toggle_pile(0, here, None) else {
            panic!("first press starts a pile");
        };
        assert_eq!(first.at, here, "where its first member stands");
        assert_eq!(first.participants, vec![0]);
        let two = SesbianLex {
            heat: 4,
            ..pile(&[1, 0])
        };
        assert_eq!(
            toggle_pile(2, Vec2::new(1.0, 0.0), Some(&two)),
            PileUpdate::Update(SesbianLex {
                heat: 4,
                ..pile(&[1, 0, 2])
            }),
            "joining keeps the order and the heat"
        );
        assert_eq!(
            toggle_pile(2, Vec2::new(30.0, 0.0), Some(&two)),
            PileUpdate::TooFar
        );
        let three = SesbianLex {
            heat: 4,
            ..pile(&[1, 0, 2])
        };
        assert_eq!(
            toggle_pile(2, Vec2::new(99.0, 0.0), Some(&three)),
            PileUpdate::Update(pile(&[1, 0])),
            "peeling off works from anywhere, and the heat collapses"
        );
        assert_eq!(
            toggle_pile(0, Vec2::ZERO, Some(&pile(&[0]))),
            PileUpdate::Despawn
        );
    }

    #[test]
    fn heat_needs_company_and_climbs_then_collapses() {
        let mut accum = HeatAccum::default();
        let mut pile = pile(&[0]);
        for _ in 0..10 {
            next_heat(&mut pile, &mut accum, 0.5);
        }
        assert_eq!(pile.heat, 0, "solo piling in is not spicy");
        pile.participants.push(1);
        for _ in 0..10 {
            next_heat(&mut pile, &mut accum, 0.5);
        }
        assert_eq!(pile.heat, 5);
        for _ in 0..100 {
            next_heat(&mut pile, &mut accum, 0.5);
        }
        assert_eq!(pile.heat, MAX_HEAT, "heat has a comedy ceiling");
        pile.participants.pop();
        next_heat(&mut pile, &mut accum, 0.5);
        assert_eq!(pile.heat, 0, "losing the company collapses the heat");
    }

    #[test]
    fn narration_names_the_roles_and_escalates() {
        let line = status_line(&pile(&[1, 0]), Some(0));
        assert!(line.contains("sesbian lex"));
        assert!(line.contains("player 1 (daddy)"), "{line}");
        assert!(line.contains("you (he/him butch)"), "{line}");
        assert!(status_line(&pile(&[3]), None).contains("warming up"));
        let tiers: HashSet<_> = (0..=MAX_HEAT)
            .map(|heat| {
                status_line(
                    &SesbianLex {
                        heat,
                        ..pile(&[1, 0])
                    },
                    None,
                )
            })
            .collect();
        assert_eq!(tiers.len(), 5, "five heat tiers, all distinct");
    }
}
