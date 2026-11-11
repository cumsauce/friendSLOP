//! Everyone gets a fat cat that follows them around, meows horrifically and
//! grows whenever it isn't in view. From issue #145 by hadalade.
//!
//! The server gives every player a [`FatCat`] (replicated) that waddles after
//! their body. Each client watches its *own* cat: whenever none of it is on
//! that player's screen, the client tells the server ([`CatInView`]) and the
//! server grows the cat. Looking at it stops the growth (it never shrinks).
//! It walks the villagers' navmesh to you (round trees, walls and the pond),
//! straight-lining only where there's no mesh (indoors, underground).
//! At [`CRITICAL`] size its fuse lights (critical mass): it blinks and
//! beeps like a bomb for [`FUSE_SECS`]. Click (pet) a cat to shrink it by
//! [`PET_SHRINK`]; get it back under critical and the fuse goes out. If the
//! fuse runs out it explodes, flinging everyone within [`BLAST_RADIUS`] and
//! leaving a merely fat cat again.
//! While the fuse is lit the cat is also hungry (#238, corvy: "if the cat
//! gets too big, like 10x, it should gobble you up"): it stops keeping its
//! distance and closes in on its owner, and anyone alive whose feet end up
//! in its mouth is gobbled (they die, see `afterlife.rs`). A meal settles
//! it: the fuse goes out and it's a merely fat cat again.
//!
//! The cat is built from primitives; its meows are synthesized locally on
//! every peer (lower and more horrible the bigger it gets), heard in 3D, at
//! most every 30 s and only while it's off your screen (#192).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::world::MainCamera;

/// Player `owner`'s fat cat. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct FatCat {
    pub owner: u64,
    /// Feet position.
    pub pos: Vec3,
    pub yaw: f32,
    /// Scale: 1.0 is a merely very fat cat.
    pub size: f32,
}

/// Client -> server: "my cat is (or isn't) on my screen".
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct CatInView {
    pub seen: bool,
}

/// Server only: whether the owner can currently see their cat.
#[derive(Component, Default, Debug)]
pub struct Watched(pub bool);

/// How much bigger (in scale units) an unwatched cat gets per second.
pub const GROW_PER_SEC: f32 = 0.008;
/// It stops at this size, so the world still fits around it.
pub const MAX_SIZE: f32 = 25.0;
/// How fast the cat waddles after you (faster than you run, it's motivated).
const SPEED: f32 = 7.0;
/// It stays this far from your feet (times its size).
const HEEL: f32 = 1.3;
/// Further away than this (a teleport, the warehouse) it just appears behind you.
const LOST: f32 = 30.0;
/// At this size the fuse lights.
pub const CRITICAL: f32 = 10.0;
/// Seconds from lit fuse to boom.
pub const FUSE_SECS: f32 = 30.0;
/// How much one pet shrinks a cat.
pub const PET_SHRINK: f32 = 0.6;
/// Everyone this close to an exploding cat is flung.
pub const BLAST_RADIUS: f32 = 10.0;
/// ...this far.
pub const BLAST_PUSH: f32 = 8.0;
/// A hungry (lit) cat stops this close to you (times its size): mouth first.
const HUNGRY_HEEL: f32 = 0.8;
/// Its mouth is this far in front of its feet (times its size)...
pub const MOUTH_AHEAD: f32 = 0.8;
/// ...and swallows feet this close to it (times its size).
pub const MOUTH_RADIUS: f32 = 0.4;

/// Server only: the cat goes here instead of after its owner (a pee
/// puddle, see `kitty_lemonade.rs`, #213), stopping with its head over it.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct Lured(pub Vec3);

/// On a cat busy drinking (seconds left). Replicated, for the HUD and its
/// nodding head.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Lapping(pub f32);

/// A lured cat stops this far from the spot (times its size): head over it.
pub const LAP_HEEL: f32 = 0.55;

/// On a cat whose fuse is lit: seconds until it explodes. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Fuse(pub f32);

/// Client -> server: I pet `owner`'s cat.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PetCat {
    pub owner: u64,
}

/// Server -> everyone: `owner`'s cat exploded at `at`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CatExploded {
    pub owner: u64,
    pub at: Vec3,
}

/// Server -> everyone: `owner`'s hungry cat gobbled up `victim` at `at`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CatGobbled {
    pub owner: u64,
    pub victim: u64,
    pub at: Vec3,
}

/// Where a cat's mouth is: in front of its head, the way it faces.
pub fn mouth(cat: &FatCat) -> Vec3 {
    let ahead = Vec3::new(-cat.yaw.sin(), 0.0, -cat.yaw.cos()) * MOUTH_AHEAD * cat.size;
    cat.pos + ahead
}

/// Whether a hungry cat swallows someone standing at `feet`.
pub fn gobbles(cat: &FatCat, feet: Vec3) -> bool {
    let m = mouth(cat);
    m.xz().distance(feet.xz()) <= MOUTH_RADIUS * cat.size
        && (feet.y - m.y).abs() <= MOUTH_RADIUS * cat.size + 1.0
}

/// The size after one pet.
pub fn petted(size: f32) -> f32 {
    (size - PET_SHRINK).max(1.0)
}

/// Where someone at `pos` lands when a cat explodes at `blast` (unchanged if
/// out of range).
pub fn flung(pos: Vec2, blast: Vec2) -> Vec2 {
    let d = pos - blast;
    if d.length() > BLAST_RADIUS {
        return pos;
    }
    pos + d.normalize_or(Vec2::X) * BLAST_PUSH
}

pub fn protocol(app: &mut App) {
    app.replicate::<FatCat>()
        .add_client_event::<CatInView>(Channel::Ordered)
        .add_observer(on_in_view)
        .replicate::<Fuse>()
        .add_client_event::<PetCat>(Channel::Ordered)
        .add_server_event::<CatExploded>(Channel::Ordered)
        .add_observer(pet_cat)
        .replicate::<Lapping>()
        .add_server_event::<CatGobbled>(Channel::Ordered)
        .add_systems(
            Update,
            (
                give_cats,
                attach_cat_agents,
                follow_and_grow,
                burn_fuses,
                gobble,
            )
                .chain()
                .in_set(CatSet)
                .run_if(in_state(ServerState::Running)),
        );
}

/// Server: the cats' following, growing and fuses (order lures before it).
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct CatSet;

/// Server: one cat per player (spawned at their feet), and none for leavers.
fn give_cats(
    mut commands: Commands,
    players: Query<(&Player, &Walker)>,
    cats: Query<(Entity, &FatCat)>,
) {
    for (entity, cat) in &cats {
        if !players.iter().any(|(p, _)| p.id == cat.owner) {
            commands.entity(entity).despawn();
        }
    }
    for (player, walker) in &players {
        if cats.iter().any(|(_, c)| c.owner == player.id) {
            continue;
        }
        commands.spawn((
            FatCat {
                owner: player.id,
                pos: walker.pos + Vec3::new(1.0, 0.0, 1.0),
                yaw: 0.0,
                size: 1.0,
            },
            Watched::default(),
            Replicated,
        ));
    }
}

fn on_in_view(
    ev: On<FromClient<CatInView>>,
    players: Query<(&Player, &PlayerOwner)>,
    mut cats: Query<(&FatCat, &mut Watched)>,
) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((me, _)) = players.iter().find(|(_, o)| o.0 == owner) else {
        return;
    };
    for (cat, mut watched) in &mut cats {
        if cat.owner == me.id {
            watched.0 = ev.seen;
        }
    }
}

/// Where a cat at `from` ends up after `dt` seconds of waddling towards
/// `target`, keeping `heel` metres away. Pure so it can be tested.
pub fn waddle(from: Vec2, target: Vec2, heel: f32, dt: f32) -> Vec2 {
    let gap = target - from;
    let len = gap.length();
    if len <= heel {
        return from;
    }
    from + gap / len * (SPEED * dt).min(len - heel)
}

/// The size after `dt` seconds, growing only while nobody's watching.
pub fn grown(size: f32, watched: bool, dt: f32) -> f32 {
    if watched {
        size
    } else {
        (size + GROW_PER_SEC * dt).min(MAX_SIZE)
    }
}

/// Server: once the villagers' navmesh is up, each cat gets a landmass agent
/// (only where cats have a transform, i.e. on a host that renders them).
fn attach_cat_agents(
    mut commands: Commands,
    nav: Option<Res<super::villagers::VillageNav>>,
    fresh: Query<(Entity, &FatCat), (With<Transform>, Without<bevy_landmass::Agent3d>)>,
) {
    let Some(nav) = nav else { return };
    for (e, cat) in &fresh {
        commands.entity(e).insert((
            bevy_landmass::Agent3d::default(),
            bevy_landmass::AgentSettings {
                radius: super::villagers::AGENT_RADIUS,
                desired_speed: SPEED,
                max_speed: SPEED,
            },
            bevy_landmass::ArchipelagoRef3d::new(nav.archipelago),
            bevy_landmass::AgentTarget3d::Point(cat.pos),
        ));
    }
}

#[allow(clippy::type_complexity)]
fn follow_and_grow(
    time: Res<Time>,
    players: Query<(&Player, &Walker)>,
    mut cats: Query<(
        &mut FatCat,
        &Watched,
        Option<&Lured>,
        Has<Fuse>,
        Option<&mut bevy_landmass::AgentTarget3d>,
        Option<&bevy_landmass::AgentDesiredVelocity3d>,
        Option<&bevy_landmass::AgentState>,
    )>,
    still: Query<&super::stillness::Stillness>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let dt = time.delta_secs();
    for (mut cat, watched, lured, hungry, target, desired, state) in &mut cats {
        let Some((_, walker)) = players.iter().find(|(p, _)| p.id == cat.owner) else {
            continue;
        };
        let mut next = *cat;
        next.size = grown(cat.size, watched.0, dt);
        // Something better than you (a puddle) pulls it there instead.
        let (owner, heel) = match lured {
            Some(l) => (l.0, LAP_HEEL * next.size),
            // A hungry cat comes for you, mouth first (#238).
            None if hungry => (walker.pos, HUNGRY_HEEL * next.size),
            None => (walker.pos, HEEL * next.size),
        };
        let here = cat.pos.xz();
        let flat = if lured.is_none() && here.distance(owner.xz()) > LOST + heel {
            // Lost you: pop up behind you.
            let behind = Vec2::new(walker.yaw.sin(), walker.yaw.cos()) * heel;
            owner.xz() + behind
        } else {
            // Walk the navmesh to you when it has a path; else straight at you.
            if let Some(mut target) = target {
                let want = bevy_landmass::AgentTarget3d::Point(owner);
                if *target != want {
                    *target = want;
                }
            }
            let path = matches!(state, Some(bevy_landmass::AgentState::Moving))
                .then(|| desired.map(|d| d.velocity().xz()))
                .flatten()
                .filter(|v| v.length() > 1e-3);
            match path {
                Some(v) if here.distance(owner.xz()) > heel => {
                    let step = (SPEED * dt).min(here.distance(owner.xz()) - heel);
                    here + v.normalize() * step
                }
                _ => waddle(here, owner.xz(), heel, dt),
            }
        };
        let gap = owner.xz() - flat;
        if gap.length() > 0.05 {
            next.yaw = (-gap.x).atan2(-gap.y);
        }
        // On the meadow it sits on the ground; indoors or underground it
        // stands on your floor instead.
        let ground = crate::terrain::height_at(flat.x, flat.y);
        let y = if (owner.y - ground).abs() > 2.0 {
            owner.y
        } else {
            ground
        };
        next.pos = Vec3::new(flat.x, y, flat.y);
        if next != *cat {
            *cat = next;
        }
    }
}

/// Server: light the fuse at critical mass, put it out below it, and blow
/// up cats whose fuse runs out.
fn burn_fuses(
    time: Res<Time>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    mut cats: Query<(Entity, &mut FatCat, Option<&mut Fuse>)>,
    mut walkers: Query<(&mut Walker, Option<&mut crate::kcc::Kcc>)>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    for (e, mut cat, fuse) in &mut cats {
        match fuse {
            None if cat.size >= CRITICAL => {
                commands.entity(e).insert(Fuse(FUSE_SECS));
            }
            None => {}
            Some(_) if cat.size < CRITICAL => {
                commands.entity(e).remove::<Fuse>();
            }
            Some(mut f) => {
                let before = f.0.ceil();
                f.bypass_change_detection().0 -= time.delta_secs();
                if f.0.ceil() != before {
                    f.set_changed();
                }
                if f.0 > 0.0 {
                    continue;
                }
                commands.entity(e).remove::<Fuse>();
                let at = cat.pos;
                cat.size = 1.0;
                for (mut w, kcc) in &mut walkers {
                    let to = flung(w.pos.xz(), at.xz());
                    if to != w.pos.xz() {
                        w.pos = Vec3::new(
                            to.x,
                            crate::terrain::height_at(to.x, to.y).max(w.pos.y),
                            to.y,
                        );
                        if let Some(mut kcc) = kcc {
                            kcc.velocity = Vec3::ZERO;
                        }
                    }
                }
                commands.server_trigger(ToClients {
                    targets: SendTargets::All,
                    message: CatExploded {
                        owner: cat.owner,
                        at,
                    },
                });
            }
        }
    }
}

/// Server: a hungry (lit) cat gobbles up the first living player in its
/// mouth (#238). They die, and the meal settles it back to 1x.
fn gobble(
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    mut cats: Query<(Entity, &mut FatCat), With<Fuse>>,
    players: Query<(&Player, &Walker), Without<super::afterlife::Dead>>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    for (e, mut cat) in &mut cats {
        let Some((victim, _)) = players.iter().find(|(_, w)| gobbles(&cat, w.pos)) else {
            continue;
        };
        let victim = victim.id;
        let at = mouth(&cat);
        commands.trigger(super::afterlife::Die { player: victim });
        commands.entity(e).remove::<Fuse>();
        cat.size = 1.0;
        commands.server_trigger(ToClients {
            targets: SendTargets::All,
            message: CatGobbled {
                owner: cat.owner,
                victim,
                at,
            },
        });
    }
}

/// Server: a pet shrinks the cat (anyone may pet any cat, from close by).
fn pet_cat(
    ev: On<FromClient<PetCat>>,
    mut commands: Commands,
    mut cats: Query<&mut FatCat>,
    walkers: Query<(&PlayerOwner, &Walker)>,
) {
    let Some(mut cat) = cats.iter_mut().find(|c| c.owner == ev.owner) else {
        return;
    };
    // A big cat is easier to reach: its body is that much bigger.
    let reach = super::interact::SERVER_REACH + 0.5 * cat.size;
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let near = walkers
        .iter()
        .find(|(o, _)| o.0 == owner)
        .is_some_and(|(_, w)| super::interact::within(w.pos, cat.pos, reach));
    if !near {
        super::interact::deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    cat.size = petted(cat.size);
}

pub fn plugin(app: &mut App) {
    app.add_observer(click_cat)
        .add_observer(boom)
        .add_observer(gulp)
        .add_systems(
            Update,
            (
                dress_cats,
                move_cats,
                report_in_view,
                meow,
                blink,
                fade_blasts,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// Local: clicking a cat pets it.
fn click_cat(ev: On<super::interact::Clicked>, mut commands: Commands, cats: Query<&FatCat>) {
    if let Ok(cat) = cats.get(ev.entity) {
        commands.client_trigger(PetCat { owner: cat.owner });
    }
}

/// Local: the cat's fur, so it can flash.
#[derive(Component)]
struct CatFur(Handle<StandardMaterial>);

/// Local: when this cat last beeped.
#[derive(Component, Default)]
struct Beep(bool);

/// How fast a lit cat blinks (Hz): faster as the fuse burns down.
pub fn blink_rate(left: f32) -> f32 {
    1.5 + 8.0 * (1.0 - (left / FUSE_SECS).clamp(0.0, 1.0))
}

/// A lit cat flashes red and beeps on each flash, faster as the fuse runs out.
fn blink(
    mut commands: Commands,
    time: Res<Time>,
    mut cats: Query<(&FatCat, Option<&Fuse>, &CatFur, &mut Beep, &Transform)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = time.elapsed_secs();
    for (cat, fuse, fur, mut beep, tf) in &mut cats {
        let on = fuse.is_some_and(|f| (t * blink_rate(f.0)).fract() < 0.5);
        if let Some(mut m) = materials.get_mut(&fur.0) {
            let want = if on {
                LinearRgba::rgb(3.0, 0.2, 0.1)
            } else {
                LinearRgba::BLACK
            };
            if m.emissive != want {
                m.emissive = want;
            }
        }
        if on && !beep.0 {
            let at = tf.translation + Vec3::Y * 0.7 * cat.size;
            crate::spatial::play_wav_at(
                &mut commands,
                at,
                0.6,
                crate::spatial::Carry::CALL,
                || {
                    super::bear::wav_bytes(
                        &beep_samples(super::synth_audio::RATE),
                        super::synth_audio::RATE,
                    )
                },
            );
        }
        beep.0 = on;
    }
}

/// A short square-ish bomb beep.
pub fn beep_samples(rate: u32) -> Vec<f32> {
    let n = (0.08 * rate as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let env = (1.0 - i as f32 / n as f32).min(t * 400.0);
            0.35 * env * (t * 1800.0 * std::f32::consts::TAU).sin().signum()
        })
        .collect()
}

/// The toast for a gobbling, from `me`'s point of view.
pub fn gobble_text(owner: u64, victim: u64, me: Option<u64>) -> String {
    let whose = match (me == Some(owner), owner == victim) {
        (true, true) => return "GULP. YOUR fat cat got too big and gobbled you up".into(),
        (false, true) => return format!("GULP. player {owner}'s fat cat gobbled them up"),
        (true, false) => "YOUR fat cat".to_string(),
        (false, false) => format!("player {owner}'s fat cat"),
    };
    if me == Some(victim) {
        format!("GULP. {whose} gobbled you up")
    } else {
        format!("GULP. {whose} gobbled up player {victim}")
    }
}

/// Local: a cat ate someone. Word to everyone.
fn gulp(
    ev: On<CatGobbled>,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<crate::hud::Toast>>,
    state: Option<Res<State<AppState>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    if let Some(mut toast) = toast {
        toast.show(gobble_text(ev.owner, ev.victim, me.map(|m| m.0)));
    }
}

/// A fading blast ball.
#[derive(Component)]
struct Blast(f32);

/// Local: a cat went off. A big orange flash, and word to everyone.
fn boom(
    ev: On<CatExploded>,
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<crate::hud::Toast>>,
    state: Option<Res<State<AppState>>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    if let Some(mut toast) = toast {
        let whose = if me.is_some_and(|m| m.0 == ev.owner) {
            "YOUR fat cat".to_string()
        } else {
            format!("player {}'s fat cat", ev.owner)
        };
        toast.show(format!(
            "KABOOM! {whose} reached critical mass and exploded"
        ));
    }
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 0.6, 0.15, 0.8),
        emissive: LinearRgba::rgb(6.0, 2.5, 0.4),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.spawn((
        Blast(0.0),
        Mesh3d(meshes.add(Sphere::new(1.0))),
        MeshMaterial3d(mat),
        Transform::from_translation(ev.at + Vec3::Y),
        DespawnOnExit(AppState::InGame),
    ));
}

fn fade_blasts(
    mut commands: Commands,
    time: Res<Time>,
    mut blasts: Query<(Entity, &mut Blast, &mut Transform)>,
) {
    for (e, mut b, mut tf) in &mut blasts {
        b.0 += time.delta_secs();
        if b.0 > 0.8 {
            commands.entity(e).despawn();
        } else {
            tf.scale = Vec3::splat(1.0 + b.0 * BLAST_RADIUS);
        }
    }
}

/// Local: the time until this cat's next meow.
#[derive(Component)]
struct MeowIn {
    secs: f32,
    count: u32,
}

const FUR: Color = Color::srgb(0.95, 0.55, 0.2);
const STRIPE: Color = Color::srgb(0.7, 0.35, 0.1);
const BELLY: Color = Color::srgb(1.0, 0.92, 0.8);

fn dress_cats(
    mut commands: Commands,
    new: Query<(Entity, &FatCat), Added<FatCat>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, cat) in &new {
        let fur = crate::art::soft(&mut materials, FUR);
        let stripe = crate::art::soft(&mut materials, STRIPE);
        let belly = crate::art::soft(&mut materials, BELLY);
        let pink = crate::art::soft(&mut materials, Color::srgb(1.0, 0.6, 0.65));
        let whisker = crate::art::soft(&mut materials, Color::WHITE);
        commands
            .entity(entity)
            .insert((
                Transform::from_translation(cat.pos)
                    .with_rotation(Quat::from_rotation_y(cat.yaw))
                    .with_scale(Vec3::splat(cat.size)),
                Visibility::default(),
                super::interact::Clickable::at_height(0.5)
                    .radius(80.0)
                    .labelled("pet the cat"),
                CatFur(fur.clone()),
                Beep::default(),
                MeowIn {
                    secs: MEOW_EVERY,
                    count: 0,
                },
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                // The enormous body: wider than it is long.
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.4))),
                    MeshMaterial3d(fur.clone()),
                    Transform::from_xyz(0.0, 0.38, 0.0).with_scale(Vec3::new(1.35, 0.95, 1.4)),
                ));
                // Belly sagging underneath.
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.3))),
                    MeshMaterial3d(belly.clone()),
                    Transform::from_xyz(0.0, 0.24, -0.05).with_scale(Vec3::new(1.4, 0.7, 1.5)),
                ));
                // Tabby stripes across the back.
                let band = meshes.add(Capsule3d::new(0.05, 0.6));
                for z in [-0.2, 0.0, 0.2] {
                    p.spawn((
                        Mesh3d(band.clone()),
                        MeshMaterial3d(stripe.clone()),
                        Transform::from_xyz(0.0, 0.73, z)
                            .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2))
                            .with_scale(Vec3::new(1.0, 1.0, 0.5)),
                    ));
                }
                // Stubby legs, barely visible under all that cat.
                let leg = meshes.add(Capsule3d::new(0.07, 0.08));
                for (x, z) in [(-0.3, -0.3), (0.3, -0.3), (-0.3, 0.3), (0.3, 0.3)] {
                    p.spawn((
                        Mesh3d(leg.clone()),
                        MeshMaterial3d(fur.clone()),
                        Transform::from_xyz(x, 0.08, z),
                    ));
                }
                // Tail curling up behind.
                p.spawn((
                    Mesh3d(meshes.add(Capsule3d::new(0.06, 0.4))),
                    MeshMaterial3d(fur.clone()),
                    Transform::from_xyz(0.0, 0.6, 0.6).with_rotation(Quat::from_rotation_x(0.7)),
                ));
                // Head with ears, eyes, nose and whiskers (facing -Z).
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.24))),
                    MeshMaterial3d(fur.clone()),
                    Transform::from_xyz(0.0, 0.7, -0.5).with_scale(Vec3::new(1.15, 0.95, 1.0)),
                ))
                .with_children(|h| {
                    crate::art::eyes(
                        h,
                        &mut meshes,
                        &mut materials,
                        Vec3::new(0.0, 0.05, -0.2),
                        0.18,
                        0.05,
                    );
                    let ear = meshes.add(Cone::new(0.08, 0.16).mesh().resolution(4));
                    for x in [-0.13, 0.13] {
                        h.spawn((
                            Mesh3d(ear.clone()),
                            MeshMaterial3d(fur.clone()),
                            Transform::from_xyz(x, 0.21, 0.0)
                                .with_rotation(Quat::from_rotation_z(-x * 2.5)),
                        ));
                    }
                    h.spawn((
                        Mesh3d(meshes.add(Sphere::new(0.03))),
                        MeshMaterial3d(pink.clone()),
                        Transform::from_xyz(0.0, -0.03, -0.235),
                    ));
                    let hair = meshes.add(Cylinder::new(0.004, 0.22));
                    for side in [-1.0f32, 1.0] {
                        for tilt in [-0.15f32, 0.15] {
                            h.spawn((
                                Mesh3d(hair.clone()),
                                MeshMaterial3d(whisker.clone()),
                                Transform::from_xyz(side * 0.15, -0.05, -0.19).with_rotation(
                                    Quat::from_rotation_z(std::f32::consts::FRAC_PI_2 + tilt),
                                ),
                            ));
                        }
                    }
                });
            });
    }
}

/// Glide to the replicated spot and size, rolling side to side as it waddles.
fn move_cats(time: Res<Time>, mut cats: Query<(&FatCat, Has<Lapping>, &mut Transform)>) {
    let t = time.elapsed_secs();
    let k = (time.delta_secs() * 8.0).min(1.0);
    for (cat, lapping, mut tf) in &mut cats {
        let gap = cat.pos - tf.translation;
        let moving = gap.xz().length() > 0.02 * cat.size;
        tf.translation += gap * k;
        let size = tf.scale.x + (cat.size - tf.scale.x) * k;
        tf.scale = Vec3::splat(size);
        let roll = if moving {
            (t * 9.0 + cat.owner as f32).sin() * 0.12
        } else {
            0.0
        };
        // Drinking: nose down, lapping away.
        let nod = if lapping {
            -0.18 - 0.07 * (t * 11.0).sin()
        } else {
            0.0
        };
        tf.rotation = Quat::from_rotation_y(cat.yaw)
            * Quat::from_rotation_z(roll)
            * Quat::from_rotation_x(nod);
    }
}

/// Points spread over a cat of `size` at `pos`: if any is on screen, it's in view.
fn sample_points(pos: Vec3, size: f32) -> [Vec3; 7] {
    let r = 0.5 * size;
    let c = pos + Vec3::Y * 0.4 * size;
    [
        c,
        c + Vec3::Y * r,
        c - Vec3::Y * 0.3 * size,
        c + Vec3::X * r,
        c - Vec3::X * r,
        c + Vec3::Z * r,
        c - Vec3::Z * r,
    ]
}

/// True if `p` lands inside the camera's picture (in front of it).
fn on_screen(camera: &Camera, cam_tf: &GlobalTransform, p: Vec3) -> bool {
    let view = cam_tf.affine().inverse().transform_point3(p);
    if view.z >= 0.0 {
        return false; // behind the camera
    }
    camera
        .world_to_ndc(cam_tf, p)
        .is_some_and(|n| n.x.abs() <= 1.0 && n.y.abs() <= 1.0)
}

/// Tell the server whether my cat is on my screen: whenever it changes, and
/// twice a second anyway.
fn report_in_view(
    mut commands: Commands,
    time: Res<Time>,
    mut last: Local<Option<bool>>,
    mut every: Local<f32>,
    me: Option<Res<LocalPlayer>>,
    cats: Query<(&FatCat, &Transform)>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    let Some(me) = me else { return };
    let Some((cat, tf)) = cats.iter().find(|(c, _)| c.owner == me.0) else {
        return;
    };
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };
    let seen = sample_points(tf.translation, cat.size.max(tf.scale.x))
        .iter()
        .any(|p| on_screen(camera, cam_tf, *p));
    *every -= time.delta_secs();
    if *last != Some(seen) || *every <= 0.0 {
        *last = Some(seen);
        *every = 0.5;
        commands.client_trigger(CatInView { seen });
    }
}

/// Seconds between meows, at the least (#192).
pub const MEOW_EVERY: f32 = 30.0;

/// The meow countdown after `dt` seconds: looking at the cat puts it back to
/// the full [`MEOW_EVERY`] (#192, corvy: "only when you're not looking at it.
/// looking should reset the timer"). Returns the new countdown and whether it
/// meows now.
pub fn meow_tick(left: f32, seen: bool, dt: f32) -> (f32, bool) {
    if seen {
        return (MEOW_EVERY, false);
    }
    let left = left - dt;
    if left <= 0.0 {
        (MEOW_EVERY, true)
    } else {
        (left, false)
    }
}

/// A cat you can't see lets out a horrible meow, deeper the bigger it is, at
/// most every [`MEOW_EVERY`] seconds. Each peer judges by its own screen.
fn meow(
    mut commands: Commands,
    time: Res<Time>,
    mut cats: Query<(&FatCat, &Transform, &mut MeowIn)>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    let cam = camera.single().ok();
    for (cat, tf, mut next) in &mut cats {
        let seen = cam.is_some_and(|(camera, cam_tf)| {
            sample_points(tf.translation, cat.size.max(tf.scale.x))
                .iter()
                .any(|p| on_screen(camera, cam_tf, *p))
        });
        let (left, now) = meow_tick(next.secs, seen, time.delta_secs());
        next.secs = left;
        if !now {
            continue;
        }
        next.count += 1;
        let seed = (cat.owner as u32).wrapping_mul(2_654_435_761) ^ next.count;
        let size = cat.size;
        let mouth = cat.pos + Vec3::Y * 0.7 * size;
        let carry = if size > 3.0 {
            crate::spatial::Carry::LOUD
        } else {
            crate::spatial::Carry::CALL
        };
        crate::spatial::play_wav_at(&mut commands, mouth, 1.0, carry, move || {
            let s = meow_samples(super::synth_audio::RATE, size, seed);
            super::bear::wav_bytes(&s, super::synth_audio::RATE)
        });
    }
}

/// A horrific meow: a detuned, ring-modulated, distorted "mrrrEEEOOOWWW"
/// whose vowel sweeps from m to ee to ow. Bigger cats meow lower and longer.
pub fn meow_samples(rate: u32, size: f32, seed: u32) -> Vec<f32> {
    use std::f32::consts::{PI, TAU};
    let size = size.max(0.1);
    let length = (1.4 + 0.25 * size.sqrt()).min(3.5);
    let pitch_k = (1.0 / size.sqrt()).clamp(0.3, 1.0);
    let wobble = 0.9 + (seed % 7) as f32 * 0.04;
    let n = (length * rate as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let detune = [1.0f32, 1.017, 0.983, 1.49];
    let mut phases = [0.0f32; 4];
    // Two resonant band-pass filters for the vowel (state-variable).
    let mut f1 = (0.0f32, 0.0f32);
    let mut f2 = (0.0f32, 0.0f32);
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let p = t / length;
        // "mrr" low, then up into the scream, then a long sag down.
        let contour = if p < 0.25 {
            380.0 + 600.0 * (p / 0.25)
        } else {
            980.0 - 700.0 * ((p - 0.25) / 0.75).powf(0.7)
        };
        // Vibrato that gets wilder as the meow goes on.
        let vib = 1.0 + (0.01 + 0.06 * p) * (TAU * 6.5 * wobble * t).sin();
        let f0 = contour * vib * pitch_k * wobble;
        let mut voice = 0.0;
        for (k, ph) in phases.iter_mut().enumerate() {
            let inc = f0 * detune[k] / rate as f32;
            *ph = (*ph + inc).fract();
            let gain = if k == 3 { 0.4 } else { 1.0 };
            voice += super::synth_audio::saw(*ph, inc) * gain;
        }
        // Ring modulation: a sickly metallic growl under it.
        voice *= 0.6 + 0.4 * (TAU * 47.0 * pitch_k * t).sin();
        // Vowel: m (300 Hz) -> ee (2400 Hz) -> ow (700 Hz).
        let vowel = (p * PI).sin();
        let form1 = 300.0 + 500.0 * vowel;
        let form2 = 700.0 + 1700.0 * vowel * (1.0 - p * 0.6);
        let band = |state: &mut (f32, f32), freq: f32, x: f32| {
            let f = 2.0 * (PI * freq / rate as f32).sin();
            let q = 0.25;
            let high = x - state.0 - q * state.1;
            state.1 += f * high;
            state.0 += f * state.1;
            state.1
        };
        let shaped = band(&mut f1, form1, voice) + 0.7 * band(&mut f2, form2, voice);
        // Clip it hard: horrific, as requested.
        let crushed = (shaped * 3.0).tanh();
        let env = (p * 12.0).min(1.0) * ((1.0 - p) * 3.0).min(1.0);
        out.push(crushed * env);
    }
    super::synth_audio::master(&mut out, rate, 6000.0, 0.75);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hungry_cat_gobbles_what_is_in_its_mouth() {
        let cat = FatCat {
            owner: 0,
            pos: Vec3::ZERO,
            yaw: 0.0, // facing -Z
            size: CRITICAL,
        };
        let m = mouth(&cat);
        assert!((m - Vec3::new(0.0, 0.0, -MOUTH_AHEAD * CRITICAL)).length() < 1e-4);
        // A hungry cat parks you right in its mouth...
        assert!(gobbles(&cat, Vec3::new(0.0, 0.0, -HUNGRY_HEEL * CRITICAL)));
        // ...but where a calm cat heels, and behind it, you're safe.
        assert!(!gobbles(&cat, Vec3::new(0.0, 0.0, -HEEL * CRITICAL)));
        assert!(!gobbles(&cat, Vec3::new(0.0, 0.0, 5.0)));
        // Not from a floor far above.
        assert!(!gobbles(&cat, m + Vec3::Y * 20.0));
    }

    #[test]
    fn gobbling_is_announced() {
        assert!(gobble_text(1, 1, Some(1)).contains("gobbled you up"));
        assert_eq!(
            gobble_text(1, 2, Some(2)),
            "GULP. player 1's fat cat gobbled you up"
        );
        assert_eq!(
            gobble_text(1, 2, Some(1)),
            "GULP. YOUR fat cat gobbled up player 2"
        );
    }

    #[test]
    fn cat_waddles_to_heel() {
        let p = waddle(Vec2::ZERO, Vec2::new(10.0, 0.0), 1.0, 0.5);
        assert!((p.x - SPEED * 0.5).abs() < 1e-4);
        let p = waddle(Vec2::ZERO, Vec2::new(3.0, 0.0), 1.0, 5.0);
        assert!((p.x - 2.0).abs() < 1e-4, "stops at heel: {p}");
        assert_eq!(
            waddle(Vec2::ZERO, Vec2::new(0.5, 0.0), 1.0, 1.0),
            Vec2::ZERO
        );
    }

    #[test]
    fn grows_only_when_unwatched_and_stops_at_max() {
        assert_eq!(grown(2.0, true, 10.0), 2.0);
        assert!(grown(2.0, false, 1.0) > 2.0);
        assert_eq!(grown(MAX_SIZE - 0.01, false, 100.0), MAX_SIZE);
    }

    #[test]
    fn petting_shrinks_and_blasts_fling_only_the_nearby() {
        assert!((petted(CRITICAL) - (CRITICAL - PET_SHRINK)).abs() < 1e-5);
        assert_eq!(petted(1.2), 1.0);
        let near = flung(Vec2::new(2.0, 0.0), Vec2::ZERO);
        assert!((near.x - (2.0 + BLAST_PUSH)).abs() < 1e-4);
        let far = Vec2::new(BLAST_RADIUS + 1.0, 0.0);
        assert_eq!(flung(far, Vec2::ZERO), far);
        assert!(blink_rate(1.0) > blink_rate(FUSE_SECS));
    }

    #[test]
    fn meows_wait_thirty_seconds_and_looking_resets_the_wait() {
        let (left, now) = meow_tick(MEOW_EVERY, false, 29.0);
        assert!(!now && (left - 1.0).abs() < 1e-4);
        assert_eq!(meow_tick(left, true, 0.1), (MEOW_EVERY, false));
        let (left, now) = meow_tick(0.5, false, 1.0);
        assert!(now);
        assert_eq!(left, MEOW_EVERY);
    }

    #[test]
    fn meow_is_loud_but_safe_and_bigger_cats_meow_longer() {
        let rate = crate::features::synth_audio::RATE;
        let small = meow_samples(rate, 1.0, 3);
        let big = meow_samples(rate, 16.0, 3);
        assert!(big.len() > small.len());
        for s in [&small, &big] {
            let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak > 0.5 && peak <= 0.76, "peak {peak}");
            assert!(s.iter().all(|x| x.is_finite()));
        }
    }
}
