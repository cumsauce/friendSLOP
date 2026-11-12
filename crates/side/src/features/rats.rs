//! Evil rats in the tunnels: the friends' inverted twins. From issue #159 by
//! catherine ("evil variants of all of us with inverted names that live
//! underground that were turned into rats in the games lore to save the
//! world. the rats are adoptable as pets").
//!
//! For every Discord friend walking the commune (`discord_friends.rs`) there
//! is a rat in the tunnels (`underground`) with their name backwards:
//! catherine's is "enirehtac". The lore, as the rats tell it: they were the
//! evil halves of everyone, and they let themselves be turned into rats to
//! save the world. They are not happy about it.
//!
//! Strays scurry from tunnel cell to tunnel cell. Click one to adopt it (one
//! rat each, separate from the surface pets): it squeaks, tells you the lore
//! and follows you everywhere, up ladders too. Click your own rat to hear it
//! say something evil. A rat whose owner leaves goes back underground.
//! The rats are a rigged modeler model ([`RAT_MODEL`]) that scurries and sniffs.

use bevy::audio::AudioSource;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, SERVER_REACH, deny, refuse, within};
use crate::features::underground::{CELL, Cell, Layout, TUNNEL_Y, Tunnels, cell_centre};
use crate::features::villagers::{COUNT, FIRST_FRIEND, NAMES};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// One rat per Discord friend.
pub const RATS: usize = COUNT - FIRST_FRIEND as usize;
/// How fast a stray scurries (m/s).
const SCURRY: f32 = 1.6;
/// How fast an adopted rat keeps up with you (m/s).
const KEEP_UP: f32 = 5.5;
/// Adopted rats stop this short of your feet.
const HEEL: f32 = 0.9;
/// Further than this behind you (a ladder, a fall) and it just appears at
/// your heel: rats get everywhere.
const CATCH_UP: f32 = 8.0;
/// Server moves (and replicated updates) per second.
const TICK_HZ: f32 = 10.0;

/// A friend's name, inverted: backwards ("catherine" -> "enirehtac").
pub fn evil_name(name: &str) -> String {
    name.chars().rev().collect()
}

/// The evil twin's name of rat `id`.
pub fn rat_name(id: u8) -> String {
    evil_name(NAMES[(FIRST_FRIEND as usize + id as usize % RATS) % COUNT])
}

/// What a rat says when you adopt it: the lore.
const LORE: [&str; 3] = [
    "we were the evil halves of all of you. we became rats to save the world. you're welcome.",
    "the world was ending, so we gave up our faces. now we live down here. with the water.",
    "they made us rats so the world could go on. nobody up there even said thanks.",
];

/// What your rat says when you click it.
const EVIL: [&str; 6] = [
    "squeak. i know where you sleep.",
    "i saved the world and all i got was this tail.",
    "one day, the surface. one day, all of it.",
    "your twin up there doesn't deserve you. i do.",
    "the world owes us. it owes us cheese.",
    "let's ruin someone's day. little steps.",
];

/// Server -> everyone: one rat. `at` is its feet; `owner` the `Player::id`
/// it follows, if adopted.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Rat {
    pub id: u8,
    pub at: Vec3,
    pub yaw: f32,
    pub owner: Option<u64>,
}

/// Client -> server: "I adopt rat `id`."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct AdoptRat {
    pub id: u8,
}

/// Server only: where a stray is scurrying to, and its dice.
#[derive(Component, Debug)]
struct Scurry {
    to: Vec2,
    seed: u32,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Rat>()
        .add_client_event::<AdoptRat>(Channel::Ordered)
        .add_observer(adopt)
        .add_observer(release_on_leave)
        .add_systems(
            Update,
            (spawn_rats.run_if(resource_exists::<Tunnels>), move_rats)
                .chain()
                .run_if(in_state(ServerState::Running)),
        );
}

/// Where rat `id` lives: tunnel cells spread out along the network.
pub fn home(l: &Layout, id: u8) -> Vec2 {
    let cells: Vec<Cell> = l.cells.keys().copied().collect();
    if cells.is_empty() {
        return Vec2::ZERO;
    }
    let step = (cells.len() / RATS).max(1);
    cell_centre(cells[(id as usize * step + step / 2) % cells.len()])
}

/// A stray's next stop from `at`: the centre of a random open cell next to
/// it (or its own, if it's boxed in).
pub fn next_stop(l: &Layout, at: Vec2, roll: u32) -> Vec2 {
    let c = crate::features::underground::cell_of(at);
    let open: Vec<Cell> = [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .into_iter()
        .map(|d| (c.0 + d.0, c.1 + d.1))
        .filter(|n| l.open(*n))
        .collect();
    let jitter = Vec2::new(
        ((roll >> 3) % 7) as f32 / 7.0 - 0.5,
        ((roll >> 7) % 7) as f32 / 7.0 - 0.5,
    ) * CELL
        * 0.5;
    match open.get(roll as usize % open.len().max(1)) {
        Some(n) => cell_centre(*n) + jitter,
        None => cell_centre(c),
    }
}

/// Where a rat at `from` ends up after stepping `step` metres towards
/// `target`, stopping `heel` short.
pub fn step_towards(from: Vec2, target: Vec2, step: f32, heel: f32) -> Vec2 {
    let gap = target - from;
    let len = gap.length();
    if len <= heel {
        return from;
    }
    from + gap / len * step.min(len - heel)
}

/// Server: once the tunnels exist, the rats move in. When the world seed
/// changes the tunnels, strays go back to their (new) homes.
fn spawn_rats(
    mut commands: Commands,
    tunnels: Res<Tunnels>,
    mut rats: Query<(&mut Rat, &mut Scurry)>,
) {
    if !rats.is_empty() {
        if tunnels.is_changed() {
            for (mut rat, mut scurry) in &mut rats {
                if rat.owner.is_none() {
                    let at = home(&tunnels, rat.id);
                    rat.at = Vec3::new(at.x, TUNNEL_Y, at.y);
                    scurry.to = at;
                }
            }
        }
        return;
    }
    for id in 0..RATS as u8 {
        let at = home(&tunnels, id);
        commands.spawn((
            Rat {
                id,
                at: Vec3::new(at.x, TUNNEL_Y, at.y),
                yaw: id as f32 * 1.7,
                owner: None,
            },
            Scurry {
                to: at,
                seed: 0x5eed_0000 ^ (id as u32).wrapping_mul(2_654_435_761),
            },
            Replicated,
        ));
    }
}

fn player_id(client: ClientId, players: &Query<(&Player, &PlayerOwner, &Walker)>) -> Option<u64> {
    let owner = match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    players
        .iter()
        .find(|(_, o, _)| o.0 == owner)
        .map(|(p, ..)| p.id)
}

/// Server: a stray goes to whoever asks, if they don't have a rat yet.
fn adopt(
    ev: On<FromClient<AdoptRat>>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    players: Query<(&Player, &PlayerOwner, &Walker)>,
    mut rats: Query<&mut Rat>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let Some(me) = player_id(ev.client_id, &players) else {
        return;
    };
    let Some(feet) = players
        .iter()
        .find(|(p, ..)| p.id == me)
        .map(|(.., w)| w.pos)
    else {
        return;
    };
    if rats.iter().any(|r| r.owner == Some(me)) {
        deny(&mut commands, ev.client_id, "you already have a rat");
        return;
    }
    let Some(mut rat) = rats.iter_mut().find(|r| r.id == ev.id) else {
        return;
    };
    if rat.owner.is_some() {
        deny(&mut commands, ev.client_id, "that rat already has an owner");
        return;
    }
    if !within(feet, rat.at, SERVER_REACH) || (feet.y - rat.at.y).abs() > 3.0 {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    rat.owner = Some(me);
}

/// Server: a leaving player's rat scurries home.
fn release_on_leave(
    remove: On<Remove<Player>>,
    server: Res<State<ServerState>>,
    tunnels: Option<Res<Tunnels>>,
    players: Query<&Player>,
    mut rats: Query<(&mut Rat, &mut Scurry)>,
) {
    if *server.get() != ServerState::Running {
        return;
    }
    let Some(tunnels) = tunnels else { return };
    let Ok(player) = players.get(remove.entity) else {
        return;
    };
    for (mut rat, mut scurry) in &mut rats {
        if rat.owner == Some(player.id) {
            let at = home(&tunnels, rat.id);
            rat.owner = None;
            rat.at = Vec3::new(at.x, TUNNEL_Y, at.y);
            scurry.to = at;
        }
    }
}

/// Server: strays scurry around their tunnels; adopted rats follow their
/// owner, popping up at their heel when left far behind.
fn move_rats(
    time: Res<Time>,
    mut acc: Local<f32>,
    still: Query<&super::stillness::Stillness>,
    walkers: Query<(&Player, &Walker)>,
    tunnels: Option<Res<Tunnels>>,
    mut rats: Query<(&mut Rat, &mut Scurry)>,
) {
    let Some(tunnels) = tunnels else { return };
    *acc += time.delta_secs();
    let dt = 1.0 / TICK_HZ;
    if *acc < dt {
        return;
    }
    *acc = (*acc - dt).min(dt);
    if super::stillness::is_frozen(&still) {
        return;
    }
    for (mut rat, mut scurry) in &mut rats {
        let (target, y, speed, heel) = match rat.owner {
            Some(o) => {
                let Some(w) = walkers.iter().find(|(p, _)| p.id == o).map(|(_, w)| w) else {
                    continue;
                };
                if w.pos.distance(rat.at) > CATCH_UP {
                    let behind = w.pos.xz() + Vec2::from_angle(w.yaw).rotate(Vec2::Y) * HEEL;
                    rat.at = Vec3::new(behind.x, w.pos.y, behind.y);
                    continue;
                }
                (w.pos.xz(), w.pos.y, KEEP_UP, HEEL)
            }
            None => {
                if rat.at.xz().distance(scurry.to) < 0.1 {
                    scurry.seed = scurry
                        .seed
                        .wrapping_mul(1_664_525)
                        .wrapping_add(1_013_904_223);
                    // A rat pauses now and then, sniffing.
                    if (scurry.seed >> 12).is_multiple_of(3) {
                        continue;
                    }
                    scurry.to = next_stop(&tunnels, rat.at.xz(), scurry.seed >> 8);
                }
                (scurry.to, TUNNEL_Y, SCURRY, 0.0)
            }
        };
        let here = rat.at.xz();
        let next = step_towards(here, target, speed * dt, heel);
        if next != here || rat.at.y != y {
            let d = next - here;
            let yaw = if d.length() > 1e-4 {
                (-d.x).atan2(-d.y)
            } else {
                rat.yaw
            };
            *rat = Rat {
                at: Vec3::new(next.x, y, next.y),
                yaw,
                ..*rat
            };
        }
    }
}

// ---------------------------------------------------------------- local side

pub fn plugin(app: &mut App) {
    app.add_observer(click_rat).add_systems(
        Update,
        (dress_rats, glide_rats).run_if(in_state(AppState::InGame)),
    );
}

#[derive(Component)]
struct Dressed;

/// The rigged rat: grey knitted body, pink ears and tail, glowing red eyes
/// (evil). Faces -Z, feet at the origin; clips `idle` (sniffing) and `walk`
/// (scurrying).
pub const RAT_MODEL: &str = "models/creatures/rat.gltf";

fn dress_rats(mut commands: Commands, new: Query<(Entity, &Rat), Without<Dressed>>) {
    for (entity, rat) in &new {
        commands.entity(entity).insert((
            Dressed,
            Transform::from_translation(rat.at).with_rotation(Quat::from_rotation_y(rat.yaw)),
            Visibility::default(),
            Clickable::at_height(0.15).labelled("adopt the rat"),
            crate::rigged::Rigged { path: RAT_MODEL },
            crate::rigged::RigClip::looping("idle"),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// The clip a rat plays: scurrying while it moves (faster when it's keeping
/// up with its owner), sniffing about when it stops.
pub fn rat_clip(moving: bool, adopted: bool) -> crate::rigged::RigClip {
    use crate::rigged::RigClip;
    match (moving, adopted) {
        (true, true) => RigClip::looping("walk").speed(2.0).fade(0.1),
        (true, false) => RigClip::looping("walk").fade(0.1),
        (false, _) => RigClip::looping("idle").fade(0.25),
    }
}

/// Glide to the replicated spot, scurrying while it moves.
fn glide_rats(
    time: Res<Time>,
    mut rats: Query<(&Rat, &mut Transform, &mut crate::rigged::RigClip), With<Dressed>>,
) {
    let k = (time.delta_secs() * 12.0).min(1.0);
    for (rat, mut tf, mut clip) in &mut rats {
        let gap = rat.at - tf.translation;
        if gap.length() > CATCH_UP {
            tf.translation = rat.at; // popped up somewhere new
        }
        let moving = gap.xz().length() > 0.02;
        let mut at = tf.translation + gap * k;
        at.y = rat.at.y;
        let facing = Quat::from_rotation_y(rat.yaw);
        if tf.translation != at || tf.rotation != facing {
            tf.translation = at;
            tf.rotation = tf.rotation.slerp(facing, k);
        }
        let want = rat_clip(moving, rat.owner.is_some());
        if clip.name != want.name || clip.speed != want.speed {
            *clip = want;
        }
    }
}

/// The unified click landed on a rat.
fn click_rat(
    click: On<Clicked>,
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    rats: Query<&Rat>,
    toast: Option<ResMut<Toast>>,
    audio: Option<Res<Assets<AudioSource>>>,
    mut said: Local<usize>,
) {
    let Ok(rat) = rats.get(click.entity) else {
        return;
    };
    let me = me.map(|m| m.0);
    let name = rat_name(rat.id);
    let n = rat.id as usize;
    if rat.owner.is_some() && rat.owner == me {
        if let Some(mut toast) = toast {
            *said += 1;
            toast.show(format!("{name}: {}", EVIL[(n + *said) % EVIL.len()]));
        }
    } else if rat.owner.is_some() {
        refuse(toast, "that rat already has an owner");
        return;
    } else if rats.iter().any(|r| r.owner.is_some() && r.owner == me) {
        refuse(toast, "you already have a rat");
        return;
    } else {
        commands.client_trigger(AdoptRat { id: rat.id });
        if let Some(mut toast) = toast {
            toast.show(format!(
                "{name} climbs into your pocket: \"{}\"",
                LORE[n % LORE.len()]
            ));
        }
    }
    if audio.is_some() {
        let at = rat.at + Vec3::Y * 0.2;
        let pitch = 1.0 + (n % 5) as f32 * 0.08;
        crate::spatial::play_wav_at(
            &mut commands,
            at,
            0.6,
            crate::spatial::Carry::CALL,
            move || super::bear::wav_bytes(&squeak(SQUEAK_RATE, pitch), SQUEAK_RATE),
        );
    }
}

const SQUEAK_RATE: u32 = 22_050;

/// Two quick, high chirps sliding up: a rat squeak. `pitch` scales it.
pub fn squeak(rate: u32, pitch: f32) -> Vec<f32> {
    let len = 0.22;
    let n = (len * rate as f32) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            // Two chirps of 0.09 s with a gap.
            let (u, on) = if t < 0.09 {
                (t / 0.09, true)
            } else if t > 0.12 {
                ((t - 0.12) / 0.09, t < 0.21)
            } else {
                (0.0, false)
            };
            let f = pitch * (2600.0 + 1400.0 * u);
            phase = (phase + f / rate as f32).fract();
            if !on {
                return 0.0;
            }
            let env = (u * 12.0).min(1.0) * (1.0 - u).max(0.0);
            (phase * std::f32::consts::TAU).sin() * env * 0.35
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evil_twins_have_inverted_names() {
        assert_eq!(evil_name("catherine"), "enirehtac");
        assert_eq!(evil_name("Kori.exe"), "exe.iroK");
        assert_eq!(rat_name(0), evil_name(NAMES[FIRST_FRIEND as usize]));
        assert_eq!(rat_name(1), "enirehtac");
    }

    #[test]
    fn rats_live_in_the_tunnels() {
        use crate::features::underground::{SEED_SALT, WorldSeed, layout};
        let l = layout(WorldSeed(3).for_(SEED_SALT));
        let mut homes = Vec::new();
        for id in 0..RATS as u8 {
            let h = home(&l, id);
            assert!(l.open(crate::features::underground::cell_of(h)), "{id}");
            assert!(!homes.contains(&h), "two rats share a home");
            homes.push(h);
            // Their scurrying never leaves the tunnels either.
            let mut at = h;
            for roll in 0..50u32 {
                at = next_stop(&l, at, roll.wrapping_mul(2_654_435_761));
                assert!(l.open(crate::features::underground::cell_of(at)), "{at}");
            }
        }
    }

    #[test]
    fn scurrying_stops_at_heel() {
        let a = step_towards(Vec2::ZERO, Vec2::new(10.0, 0.0), 1.0, 0.9);
        assert_eq!(a, Vec2::new(1.0, 0.0));
        let b = step_towards(Vec2::new(9.5, 0.0), Vec2::new(10.0, 0.0), 1.0, 0.9);
        assert_eq!(b, Vec2::new(9.5, 0.0));
    }

    #[test]
    fn rats_scurry_while_moving_and_sniff_when_still() {
        assert_eq!(rat_clip(true, false).name, "walk");
        assert!(rat_clip(true, true).speed > rat_clip(true, false).speed);
        assert_eq!(rat_clip(false, true).name, "idle");
    }

    #[test]
    fn squeaks_are_short_and_sane() {
        let s = squeak(SQUEAK_RATE, 1.0);
        assert!(s.iter().all(|x| x.is_finite() && x.abs() <= 0.35));
        assert!(s.iter().any(|x| x.abs() > 0.1));
    }
}
