//! The monster in the carved Fulton St. The old hunt (`fulton_hunt`) plans on
//! the generated station's grid; here it walks the map's navmesh instead
//! (landmass agents, as Jokah does) and uses the map's markers: it comes out
//! of the `hunter_spawn` doors and checks the `hide` lockers.
//!
//! It reuses what the two stations share: the replicated [`Hunter`] (so its
//! look, sounds and clips all work), Andrew's calls ([`Phone`]), the locker
//! climb ([`Climbing`], [`Hunted`]) and the kill. The moves are the old ones,
//! simpler: drifting towards you, a stare when it notices you ([`Mode::Notice`]),
//! the chase, the ball ([`Mode::Roll`]), then a search of the lockers near where
//! it lost you, each prised open ([`Mode::Check`]) with a chance to find
//! whoever is in it. Speeds, sight and timings are the old hunt's data
//! (`data/side/fulton.ron` `hunt`); the call times and spawn distance are
//! `data/side/fulton_carved.ron`.
//!
//! Pure bits ([`turn_step`], [`pick_door`], [`sees`]) are tested; the systems
//! are wiring.

use std::collections::HashMap;

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_wfc as wfc;

use super::fulton_carved::{self as carved, AREA};
use super::fulton_hunt::{Climbing, Held, Hide, Hunted, Hunter, Mode, Phone, hold_at, tell};
use crate::AppState;
use crate::features::fps::{Hiding, WalkInput, Walker, teleport};
use crate::features::interact::{SERVER_REACH, deny};
use crate::player::{Player, PlayerOwner};

/// The monster's mind (server only).
#[derive(Component, Debug)]
pub struct Stalker {
    pub target: u64,
    /// Where it is heading (the navmesh agent follows it).
    pub goal: Option<Vec3>,
    pub last_seen: Option<Vec3>,
    /// Seconds without sight of you while it chases.
    pub lost: f32,
    /// Seconds it has had you in clear sight.
    pub sighted: f32,
    pub noticing: f32,
    pub rolling: f32,
    pub roll_cd: f32,
    pub search_left: f32,
    pub checking: f32,
    /// The locker it is walking to or checking (index into the map's lockers).
    pub at_locker: Option<usize>,
    pub searched: Vec<usize>,
    pub killing: f32,
    pub victim: Option<u64>,
    pub rng: wfc::Rng,
}

impl Stalker {
    fn new(target: u64, seed: u32) -> Self {
        Self {
            target,
            goal: None,
            last_seen: None,
            lost: 0.0,
            sighted: 0.0,
            noticing: 0.0,
            rolling: 0.0,
            roll_cd: 0.0,
            search_left: 0.0,
            checking: 0.0,
            at_locker: None,
            searched: Vec::new(),
            killing: 0.0,
            victim: None,
            rng: wfc::Rng::new(seed | 1),
        }
    }
}

/// The invisible navmesh agent that finds the monster's way.
#[derive(Component)]
struct StalkAgent(Entity);

// ------------------------------------------------------------------ pure

/// Turn `yaw` towards `want` by at most `step` radians, the short way.
pub fn turn_step(yaw: f32, want: f32, step: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let d = (want - yaw + std::f32::consts::PI).rem_euclid(tau) - std::f32::consts::PI;
    yaw + d.clamp(-step, step)
}

/// The door it comes out of: the nearest one at least `min` from `you`
/// (else the farthest there is).
pub fn pick_door(doors: &[(Vec3, f32)], you: Vec3, min: f32) -> Option<(Vec3, f32)> {
    let far = doors
        .iter()
        .filter(|d| d.0.distance(you) >= min)
        .min_by(|a, b| a.0.distance(you).total_cmp(&b.0.distance(you)));
    far.or_else(|| {
        doors
            .iter()
            .max_by(|a, b| a.0.distance(you).total_cmp(&b.0.distance(you)))
    })
    .copied()
}

/// Does something at `from` facing `yaw` (degrees `fov` wide, `range` far)
/// have `to` in its cone? Walls are the caller's ray.
pub fn in_cone(from: Vec3, yaw: f32, to: Vec3, range: f32, fov_degrees: f32) -> bool {
    let d = (to - from).xz();
    if d.length() > range {
        return false;
    }
    if d.length() < 0.5 {
        return true;
    }
    let facing = Vec2::new(yaw.sin(), yaw.cos());
    facing.dot(d.normalize()) >= (fov_degrees.to_radians() / 2.0).cos()
}

/// The locker (index) nearest `near` within `reach` that it hasn't searched.
pub fn next_locker(
    lockers: &[carved::LockerSpot],
    near: Vec3,
    reach: f32,
    searched: &[usize],
) -> Option<usize> {
    lockers
        .iter()
        .enumerate()
        .filter(|(i, l)| !searched.contains(i) && l.feet.distance(near) <= reach)
        .min_by(|a, b| a.1.feet.distance(near).total_cmp(&b.1.feet.distance(near)))
        .map(|(i, _)| i)
}

// ------------------------------------------------------------------ server

/// Is there a clear line between two points (walls and props)?
fn clear(spatial: &SpatialQuery, from: Vec3, to: Vec3) -> bool {
    let d = to - from;
    let len = d.length();
    let Ok(dir) = Dir3::new(d) else {
        return true;
    };
    spatial
        .cast_ray(
            from,
            dir,
            (len - 0.4).max(0.0),
            true,
            &SpatialQueryFilter::default(),
        )
        .is_none()
}

/// Server: Andrew rings while you're in the carved station; the third call
/// sends the monster out of a door.
fn calls(
    mut commands: Commands,
    time: Res<Time>,
    mut hunted: ResMut<Hunted>,
    mut stay: Local<HashMap<u64, f32>>,
    players: Query<(&Player, &PlayerOwner, &Walker)>,
    hunters: Query<&Stalker>,
) {
    let t = carved::tuning();
    let hunt = super::fulton::tuning();
    let dt = time.delta_secs();
    for (p, owner, w) in &players {
        if !carved::inside(w.pos) {
            stay.remove(&p.id);
            hunted.coming.remove(&p.id);
            continue;
        }
        let secs = stay.entry(p.id).or_insert(0.0);
        *secs += dt;
        let had = hunted.calls.get(&p.id).copied().unwrap_or(0);
        if t.call_secs.get(had).is_some_and(|due| *secs >= *due) {
            hunted.calls.insert(p.id, had + 1);
            tell(&mut commands, owner, Phone::Call(had as u8));
            if had + 1 == t.call_secs.len() {
                hunted.coming.insert(p.id, hunt.hunt.spawn_delay);
            }
        }
        let Some(left) = hunted.coming.get_mut(&p.id) else {
            continue;
        };
        *left -= dt;
        if *left > 0.0 || hunters.iter().any(|s| s.target == p.id) {
            continue;
        }
        hunted.coming.remove(&p.id);
        let Some((at, yaw)) = pick_door(&carved::monster_doors(), w.pos, t.min_spawn_dist) else {
            continue;
        };
        hunted.ticks = hunted.ticks.wrapping_add(1);
        commands.spawn((
            Replicated,
            Hunter {
                pos: at,
                yaw,
                mode: Mode::Drift,
                pace: 1.0,
                opening: None,
                ducted: false,
                emerging: false,
                knock: 0,
            },
            Stalker::new(p.id, (p.id as u32) ^ hunted.ticks.wrapping_mul(0x9e37)),
            DespawnOnExit(AppState::InGame),
        ));
        tell(&mut commands, owner, Phone::Sent);
    }
}

/// Server: one navmesh agent per monster, aimed at its goal.
fn agents(
    mut commands: Commands,
    navs: Option<Res<friendslop_base::nav::AreaNavs>>,
    hunters: Query<(Entity, &Hunter, &Stalker)>,
    mut agents: Query<(
        Entity,
        &StalkAgent,
        &mut Transform,
        &mut bevy_landmass::AgentTarget3d,
    )>,
) {
    let Some(archipelago) = navs.as_ref().and_then(|n| n.archipelago(AREA)) else {
        return;
    };
    let mut have = Vec::new();
    for (e, a, mut tf, mut target) in &mut agents {
        let Ok((_, h, s)) = hunters.get(a.0) else {
            commands.entity(e).despawn();
            continue;
        };
        have.push(a.0);
        tf.translation = h.pos;
        let want = bevy_landmass::AgentTarget3d::Point(s.goal.unwrap_or(h.pos));
        if *target != want {
            *target = want;
        }
    }
    let nav = friendslop_base::nav::TUNING.get();
    for (e, h, s) in &hunters {
        if have.contains(&e) {
            continue;
        }
        commands.spawn((
            StalkAgent(e),
            Transform::from_translation(h.pos),
            bevy_landmass::Agent3d::default(),
            bevy_landmass::AgentSettings {
                radius: nav.agent_radius,
                desired_speed: 4.0,
                max_speed: 8.0,
            },
            bevy_landmass::ArchipelagoRef3d::new(archipelago),
            bevy_landmass::AgentTarget3d::Point(s.goal.unwrap_or(h.pos)),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

type PlayerRow<'a> = (
    Entity,
    &'a Player,
    &'a PlayerOwner,
    &'a mut Walker,
    Option<&'a mut WalkInput>,
    Option<&'a mut crate::kcc::Kcc>,
);

enum Out {
    Nothing,
    Spotted(u64),
    Caught(u64),
}

/// Server: the monster thinks, walks, searches the lockers and grabs.
#[allow(clippy::type_complexity)]
fn stalk(
    mut commands: Commands,
    time: Res<Time>,
    spatial: SpatialQuery,
    mut hunted: ResMut<Hunted>,
    mut hunters: Query<(Entity, &mut Hunter, &mut Stalker)>,
    agents: Query<(
        &StalkAgent,
        Option<&bevy_landmass::AgentDesiredVelocity3d>,
        Option<&bevy_landmass::AgentState>,
    )>,
    mut players: Query<PlayerRow>,
    held: Query<(), With<Held>>,
    climbing: Query<(), Or<(With<Climbing>, With<Held>)>>,
) {
    let dt = time.delta_secs().min(0.1);
    let tun = super::fulton::tuning();
    let h = &tun.hunt;
    let lockers = carved::lockers();
    let t = carved::tuning();

    // Walked out of a locker: you're out (the old hunt does this for its own).
    let mut left = Vec::new();
    for (e, p, owner, w, _, _) in &players {
        if !carved::inside(w.pos) || climbing.contains(e) {
            continue;
        }
        if let Some(spot) = hunted.hiding.get(&p.id)
            && w.pos.xz().distance(spot.xz()) > 0.9
        {
            left.push(p.id);
            commands.entity(e).remove::<Hiding>();
            tell(
                &mut commands,
                owner,
                Phone::Climb {
                    inside: false,
                    at: *spot,
                },
            );
            tell(&mut commands, owner, Phone::Hidden(false));
        }
    }
    for id in left {
        hunted.hiding.remove(&id);
    }

    let steer: HashMap<Entity, Vec2> = agents
        .iter()
        .filter_map(|(a, v, s)| {
            let moving = matches!(s, Some(bevy_landmass::AgentState::Moving));
            let v = v?.velocity().xz();
            (moving && v.length() > 1e-3).then(|| (a.0, v.normalize()))
        })
        .collect();

    for (e, mut hunter, mut st) in &mut hunters {
        // Killing: nothing else until it's over, then they're back on the street.
        if hunter.mode == Mode::Kill {
            st.killing -= dt;
            let victim = st
                .victim
                .and_then(|id| players.iter_mut().find(|p| p.1.id == id));
            if st.killing > 0.0 && victim.is_some() {
                continue;
            }
            if let Some((pe, _, owner, mut w, mut input, mut kcc)) = victim {
                let yaw = w.yaw;
                teleport(
                    &mut w,
                    input.as_deref_mut(),
                    kcc.as_deref_mut(),
                    super::fulton::street_at(&tun) + Vec3::Y * 0.05,
                    yaw,
                );
                commands.entity(pe).remove::<(Hiding, Held)>();
                tell(&mut commands, owner, Phone::Caught);
                commands.trigger(crate::rules::quests::QuestSignal::new("fulton_caught"));
            }
            commands.entity(e).despawn();
            continue;
        }

        // The target: who it came for, if they're still here (else it goes).
        let Some((_, tp, _, tw, tinput, _)) = players
            .iter()
            .find(|p| p.1.id == st.target && carved::inside(p.3.pos) && !held.contains(p.0))
        else {
            commands.entity(e).despawn();
            continue;
        };
        let prey_feet = tw.pos;
        let prey_yaw = tw.yaw;
        let _ = (tp, tinput, prey_yaw);
        let prey_hidden = hunted.hiding.get(&st.target).copied();

        // What it sees.
        let eye = hunter.pos + Vec3::Y * 1.6;
        let seen = prey_hidden.is_none()
            && in_cone(
                hunter.pos,
                hunter.yaw,
                prey_feet,
                h.view_range,
                h.view_degrees,
            )
            && clear(&spatial, eye, prey_feet + Vec3::Y * 1.2);
        let dist = hunter.pos.distance(prey_feet);
        let mut out = Out::Nothing;
        st.roll_cd = (st.roll_cd - dt).max(0.0);

        if seen {
            st.last_seen = Some(prey_feet);
            st.lost = 0.0;
            st.sighted += dt;
            if matches!(
                hunter.mode,
                Mode::Drift | Mode::Search | Mode::Check | Mode::Retreat
            ) {
                hunter.mode = Mode::Notice;
                hunter.opening = None;
                st.noticing = h.notice_secs;
                st.at_locker = None;
                out = Out::Spotted(st.target);
            }
            if dist < h.catch_range {
                out = Out::Caught(st.target);
            }
        } else {
            st.sighted = 0.0;
            if matches!(hunter.mode, Mode::Chase | Mode::Roll | Mode::Notice) {
                st.lost += dt;
            }
        }

        match hunter.mode {
            Mode::Notice => {
                st.noticing -= dt;
                st.goal = None;
                if st.noticing <= 0.0 {
                    hunter.mode = Mode::Chase;
                }
            }
            Mode::Chase => {
                if st.sighted >= h.roll_sight_secs && st.roll_cd <= 0.0 && seen {
                    hunter.mode = Mode::Roll;
                    st.rolling = h.roll_secs;
                }
                st.goal = Some(st.last_seen.unwrap_or(prey_feet));
                if st.lost > h.lose_secs {
                    hunter.mode = Mode::Search;
                    st.search_left = h.search_secs;
                    st.searched.clear();
                    st.at_locker = None;
                }
            }
            Mode::Roll => {
                st.rolling -= dt;
                st.goal = Some(st.last_seen.unwrap_or(prey_feet));
                if st.rolling <= 0.0 {
                    hunter.mode = Mode::Chase;
                    st.roll_cd = h.roll_cooldown_secs;
                }
            }
            Mode::Drift | Mode::Retreat => {
                st.goal = Some(prey_feet);
                // Close but blind (you're hiding, or round a corner): look about.
                if dist < 8.0 && !seen {
                    hunter.mode = Mode::Search;
                    st.search_left = h.search_secs;
                    st.searched.clear();
                    st.last_seen = Some(hunter.pos);
                    st.at_locker = None;
                }
            }
            Mode::Search => {
                st.search_left -= dt;
                let near = st.last_seen.unwrap_or(hunter.pos);
                if st.at_locker.is_none() {
                    st.at_locker = next_locker(&lockers, near, t.search_reach, &st.searched);
                }
                match st.at_locker.and_then(|i| lockers.get(i).map(|l| (i, *l))) {
                    Some((i, l)) => {
                        let front = l.feet + l.out * 1.0;
                        st.goal = Some(front);
                        if hunter.pos.xz().distance(front.xz()) < 0.7 {
                            hunter.mode = Mode::Check;
                            st.checking = h.check_secs;
                            hunter.opening = Some(l.feet);
                            hunter.yaw = (-l.out.x).atan2(-l.out.z);
                            let _ = i;
                        }
                    }
                    None => {
                        // Nothing left to look in: back to hunting you.
                        if st.search_left <= 0.0 || st.last_seen.is_some() {
                            hunter.mode = Mode::Drift;
                            st.last_seen = None;
                        }
                    }
                }
                if st.search_left <= 0.0 {
                    hunter.mode = Mode::Drift;
                    st.last_seen = None;
                    st.at_locker = None;
                }
            }
            Mode::Check => {
                st.goal = None;
                st.checking -= dt;
                if st.checking <= 0.0 {
                    let i = st.at_locker.take();
                    let spot = i.and_then(|i| lockers.get(i)).map(|l| l.feet);
                    hunter.opening = None;
                    if let Some(i) = i {
                        st.searched.push(i);
                    }
                    hunter.mode = Mode::Search;
                    let in_it = prey_hidden
                        .zip(spot)
                        .is_some_and(|(hide, s)| hide.distance(s) < 0.4);
                    if in_it && st.rng.roll() < h.find_chance {
                        out = Out::Caught(st.target);
                    }
                }
            }
            Mode::Kill => {}
        }

        // Walk: the navmesh's way, else straight at it.
        let speed = match hunter.mode {
            Mode::Roll => h.roll_speed,
            Mode::Chase => h.chase_speed,
            Mode::Check | Mode::Kill | Mode::Notice => 0.0,
            _ => h.walk_speed,
        };
        if speed > 0.0
            && let Some(goal) = st.goal
        {
            let straight = (goal - hunter.pos).xz();
            let dir = steer
                .get(&e)
                .copied()
                .or_else(|| (straight.length() > 0.3).then(|| straight.normalize()));
            if let Some(dir) = dir
                && straight.length() > 0.3
            {
                hunter.yaw = turn_step(hunter.yaw, dir.x.atan2(dir.y), h.turn_speed.max(3.0) * dt);
                let step = dir * speed * dt;
                let waist = hunter.pos + Vec3::Y * 0.9;
                let ahead = Dir3::new(Vec3::new(dir.x, 0.0, dir.y)).ok();
                let blocked = ahead.is_some_and(|d| {
                    spatial
                        .cast_ray(
                            waist,
                            d,
                            step.length() + 0.4,
                            true,
                            &SpatialQueryFilter::default(),
                        )
                        .is_some()
                });
                if !blocked || steer.contains_key(&e) {
                    hunter.pos.x += step.x;
                    hunter.pos.z += step.y;
                }
            }
        }
        // Stay on the floor (stairs and ramps).
        if let Some(hit) = spatial.cast_ray(
            hunter.pos + Vec3::Y * 1.2,
            Dir3::NEG_Y,
            3.0,
            true,
            &SpatialQueryFilter::default(),
        ) {
            let y = hunter.pos.y + 1.2 - hit.distance;
            hunter.pos.y += (y - hunter.pos.y).clamp(-0.3, 0.3);
        }
        if matches!(hunter.mode, Mode::Notice) {
            let want = (prey_feet - hunter.pos).xz();
            if want.length() > 0.1 {
                hunter.yaw = turn_step(hunter.yaw, want.x.atan2(want.y), 6.0 * dt);
            }
        }
        match out {
            Out::Nothing => {}
            Out::Spotted(id) => {
                if let Some((_, _, owner, ..)) = players.iter().find(|p| p.1.id == id) {
                    tell(&mut commands, owner, Phone::Seen);
                }
            }
            Out::Caught(id) => {
                hunted.hiding.remove(&id);
                hunted.calls.remove(&id);
                if let Some((pe, _, owner, mut w, mut input, mut kcc)) =
                    players.iter_mut().find(|p| p.1.id == id)
                {
                    let (at, its_yaw, your_yaw) = hold_at(hunter.pos, w.pos, h.kill_reach);
                    teleport(
                        &mut w,
                        input.as_deref_mut(),
                        kcc.as_deref_mut(),
                        at + Vec3::Y * 0.05,
                        your_yaw,
                    );
                    commands
                        .entity(pe)
                        .remove::<Climbing>()
                        .insert((Hiding, Held));
                    tell(&mut commands, owner, Phone::Seized { at: hunter.pos });
                    hunter.mode = Mode::Kill;
                    hunter.opening = None;
                    hunter.yaw = its_yaw;
                    st.goal = None;
                    st.killing = h.kill_secs;
                    st.victim = Some(id);
                }
            }
        }
    }
}

/// Server: climb into the locker you clicked, or back out of the one you're in.
fn hide(
    ev: On<FromClient<Hide>>,
    mut commands: Commands,
    mut hunted: ResMut<Hunted>,
    players: Query<(
        Entity,
        &Player,
        &PlayerOwner,
        &Walker,
        (Has<Climbing>, Has<Held>),
    )>,
) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((e, p, o, w, (climbing, held))) = players.iter().find(|b| b.2.0 == owner) else {
        return;
    };
    if climbing || held || !carved::inside(w.pos) {
        return;
    }
    let lockers = carved::lockers();
    if let Some(spot) = hunted.hiding.remove(&p.id) {
        let out = lockers
            .iter()
            .find(|l| l.feet.distance(spot) < 0.4)
            .map_or(Vec3::Z, |l| l.out);
        commands.entity(e).insert(Climbing {
            from: w.pos,
            to: spot + out * 1.2,
            t: 0.0,
            into: None,
        });
        tell(
            &mut commands,
            o,
            Phone::Climb {
                inside: false,
                at: spot,
            },
        );
        return;
    }
    let Some(l) = lockers.get(ev.door as usize) else {
        return;
    };
    if l.feet.xz().distance(w.pos.xz()) > SERVER_REACH || (l.feet.y - w.pos.y).abs() > 1.5 {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    commands.entity(e).insert((
        Hiding,
        Climbing {
            from: w.pos,
            to: l.feet + Vec3::Y * 0.05,
            t: 0.0,
            into: Some(l.feet),
        },
    ));
    tell(
        &mut commands,
        o,
        Phone::Climb {
            inside: true,
            at: l.feet,
        },
    );
}

pub fn plugin(app: &mut App) {
    app.add_observer(hide).add_systems(
        Update,
        (calls, stalk, agents)
            .chain()
            .run_if(in_state(ServerState::Running))
            .run_if(crate::sites::awake(AREA)),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turning_takes_the_short_way_and_stops_on_target() {
        let pi = std::f32::consts::PI;
        assert!((turn_step(0.0, 0.2, 1.0) - 0.2).abs() < 1e-5);
        assert!(
            turn_step(0.1, pi * 2.0 - 0.1, 0.05) < 0.1,
            "wraps backwards"
        );
        assert!((turn_step(0.0, 3.0, 0.5) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn it_comes_out_of_a_door_far_enough_away() {
        let doors = [
            (Vec3::new(0.0, 0.0, 0.0), 0.0),
            (Vec3::new(30.0, 0.0, 0.0), 0.0),
            (Vec3::new(80.0, 0.0, 0.0), 0.0),
        ];
        let you = Vec3::new(2.0, 0.0, 0.0);
        assert_eq!(pick_door(&doors, you, 25.0).map(|d| d.0.x), Some(30.0));
        assert_eq!(pick_door(&doors, you, 500.0).map(|d| d.0.x), Some(80.0));
        assert!(pick_door(&[], you, 5.0).is_none());
    }

    #[test]
    fn it_sees_in_a_cone() {
        let o = Vec3::ZERO;
        // Yaw 0 faces +z.
        assert!(in_cone(o, 0.0, Vec3::new(0.0, 0.0, 10.0), 25.0, 160.0));
        assert!(!in_cone(o, 0.0, Vec3::new(0.0, 0.0, -10.0), 25.0, 160.0));
        assert!(!in_cone(o, 0.0, Vec3::new(0.0, 0.0, 30.0), 25.0, 160.0));
    }

    #[test]
    fn the_map_has_lockers_and_doors_for_it() {
        let l = carved::lockers();
        assert!(l.len() >= 15, "lockers");
        assert!(carved::monster_doors().len() >= 5, "monster doors");
        // A search near a locker finds it, then the next, never one twice.
        let first = next_locker(&l, l[0].feet, 5.0, &[]).expect("a locker");
        let second = next_locker(&l, l[0].feet, 5.0, &[first]);
        assert_ne!(Some(first), second);
    }
}
