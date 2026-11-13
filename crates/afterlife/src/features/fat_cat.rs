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
//! [`PET_SHRINK`], and hold the mouse on it to keep stroking it down slowly; get it back under critical and the fuse goes out. If the
//! fuse runs out it explodes, flinging everyone within [`BLAST_RADIUS`] and
//! leaving a merely fat cat again.
//! While the fuse is lit the cat is also hungry (#238, corvy: "if the cat
//! gets too big, like 10x, it should gobble you up"): it stops keeping its
//! distance and closes in on its owner, and anyone alive whose feet end up
//! in its mouth is gobbled (they die, see `afterlife.rs`). A meal settles
//! it: the fuse goes out and it's a merely fat cat again.
//!
//! The cat is a rigged modeler model ([`CAT_MODEL`]) that waddles, meows,
//! gobbles and laps with its own clips; its meows are synthesized locally on
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

/// Tuning numbers: `assets/data/afterlife/fat_cat.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// How much bigger (in scale units) an unwatched cat gets per second.
    pub grow_per_sec: f32,
    /// How fast the cat waddles after you (faster than you run, it's motivated).
    pub speed: f32,
    /// It stays this far from your feet (times its size).
    pub heel: f32,
    /// Further away than this (a teleport, the warehouse) it just appears behind you.
    pub lost: f32,
    /// At this size the fuse lights.
    pub critical: f32,
    /// Seconds from lit fuse to boom.
    pub fuse_secs: f32,
    /// How much one stroke shrinks a cat (holding the mouse on it strokes it over and over).
    pub pet_shrink: f32,
    /// Seconds between strokes while you hold the mouse on a cat.
    pub pet_every: f32,
    /// Everyone this close to an exploding cat is flung.
    pub blast_radius: f32,
    /// ...this far.
    pub blast_push: f32,
    /// Its mouth is this far in front of its feet (times its size)...
    pub mouth_ahead: f32,
    /// ...and swallows feet this close to it (times its size).
    pub mouth_radius: f32,
    /// How long the model's one-off clips run.
    pub meow_secs: f32,
    pub gobble_secs: f32,
    /// Seconds between meows, at the least (#192).
    pub meow_every: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/afterlife/fat_cat.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}
/// It stops at this size, so the world still fits around it.
pub const MAX_SIZE: f32 = 25.0;
/// A hungry (lit) cat stops this close to you (times its size): mouth first.
const HUNGRY_HEEL: f32 = 0.8;

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
    let ahead = Vec3::new(-cat.yaw.sin(), 0.0, -cat.yaw.cos()) * tuning().mouth_ahead * cat.size;
    cat.pos + ahead
}

/// Whether a hungry cat swallows someone standing at `feet`.
pub fn gobbles(cat: &FatCat, feet: Vec3) -> bool {
    let m = mouth(cat);
    m.xz().distance(feet.xz()) <= tuning().mouth_radius * cat.size
        && (feet.y - m.y).abs() <= tuning().mouth_radius * cat.size + 1.0
}

/// The size after one pet.
pub fn petted(size: f32) -> f32 {
    (size - tuning().pet_shrink).max(1.0)
}

/// Where someone at `pos` lands when a cat explodes at `blast` (unchanged if
/// out of range).
pub fn flung(pos: Vec2, blast: Vec2) -> Vec2 {
    let d = pos - blast;
    if d.length() > tuning().blast_radius {
        return pos;
    }
    pos + d.normalize_or(Vec2::X) * tuning().blast_push
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
    from + gap / len * (tuning().speed * dt).min(len - heel)
}

/// The size after `dt` seconds, growing only while nobody's watching.
pub fn grown(size: f32, watched: bool, dt: f32) -> f32 {
    if watched {
        size
    } else {
        (size + tuning().grow_per_sec * dt).min(MAX_SIZE)
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
                radius: super::villagers::tuning().agent_radius,
                desired_speed: tuning().speed,
                max_speed: tuning().speed,
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
            None => (walker.pos, tuning().heel * next.size),
        };
        let here = cat.pos.xz();
        let flat = if lured.is_none() && here.distance(owner.xz()) > tuning().lost + heel {
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
                    let step = (tuning().speed * dt).min(here.distance(owner.xz()) - heel);
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
            None if cat.size >= tuning().critical => {
                commands.entity(e).insert(Fuse(tuning().fuse_secs));
            }
            None => {}
            Some(_) if cat.size < tuning().critical => {
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
    time: Res<Time>,
    mut last_stroke: Local<std::collections::HashMap<ClientId, f32>>,
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
    // Strokes come in while the mouse is held: take at most one per `pet_every` from each
    // petter, so a fast client can't shrink a cat faster than anyone else.
    let now = time.elapsed_secs();
    let last = last_stroke.entry(ev.client_id).or_insert(f32::NEG_INFINITY);
    if now - *last < tuning().pet_every * 0.8 {
        return;
    }
    *last = now;
    cat.size = petted(cat.size);
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Stroking>()
        .add_observer(click_cat)
        .add_observer(boom)
        .add_observer(gulp)
        .add_systems(
            Update,
            (
                dress_cats,
                own_fur,
                move_cats,
                report_in_view,
                meow,
                blink,
                fade_blasts,
                keep_stroking,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// Local: the cat you're stroking while the mouse is held, and when the next stroke is due.
#[derive(Resource, Default)]
struct Stroking(Option<(u64, f32)>);

/// Local: clicking a cat pets it, and keeps petting it while you hold the mouse.
fn click_cat(
    ev: On<super::interact::Clicked>,
    mut commands: Commands,
    cats: Query<&FatCat>,
    time: Res<Time>,
    mut stroking: ResMut<Stroking>,
) {
    if let Ok(cat) = cats.get(ev.entity) {
        commands.client_trigger(PetCat { owner: cat.owner });
        stroking.0 = Some((cat.owner, time.elapsed_secs() + tuning().pet_every));
    }
}

/// Local: holding the mouse on a cat strokes it again every `pet_every` seconds.
fn keep_stroking(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    mut stroking: ResMut<Stroking>,
) {
    if !mouse.pressed(MouseButton::Left) {
        stroking.0 = None;
        return;
    }
    let Some((owner, next)) = stroking.0 else {
        return;
    };
    let now = time.elapsed_secs();
    if now >= next {
        commands.client_trigger(PetCat { owner });
        stroking.0 = Some((owner, now + tuning().pet_every.max(0.05)));
    }
}

/// Local: this cat's own copy of the model's fur, so it can flash.
#[derive(Component, Default)]
struct CatFur {
    material: Option<Handle<StandardMaterial>>,
    done: bool,
}

/// Local: when this cat last beeped.
#[derive(Component, Default)]
struct Beep(bool);

/// How fast a lit cat blinks (Hz): faster as the fuse burns down.
pub fn blink_rate(left: f32) -> f32 {
    1.5 + 8.0 * (1.0 - (left / tuning().fuse_secs).clamp(0.0, 1.0))
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
        if let Some(mut m) = fur.material.as_ref().and_then(|h| materials.get_mut(h)) {
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
                0.35,
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

/// A soft blip for the fuse: a short sine "bip" that dies away like a
/// marimba, not a square alarm.
pub fn beep_samples(rate: u32) -> Vec<f32> {
    let n = (0.09 * rate as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / rate as f32;
            let env = (t * 600.0).min(1.0) * (-t * 45.0).exp();
            let w = t * 1050.0 * std::f32::consts::TAU;
            0.22 * env * (w.sin() + 0.2 * (w * 4.0).sin())
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
    mut cats: Query<(&FatCat, &mut CatAct)>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    // Chomp.
    for (cat, mut act) in &mut cats {
        if cat.owner == ev.owner {
            act.0 = Some(("gobble", tuning().gobble_secs));
        }
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
            tf.scale = Vec3::splat(1.0 + b.0 * tuning().blast_radius);
        }
    }
}

/// Local: the time until this cat's next meow.
#[derive(Component)]
struct MeowIn {
    secs: f32,
    count: u32,
}

/// The rigged cat (#145): a fat tabby, 1 m long at size 1, facing -Z with
/// its feet at the origin. Clips: `idle`, `walk` (the waddle), `meow`,
/// `gobble` (#238) and `lap` (#213).
pub const CAT_MODEL: &str = "models/creatures/fat_cat.gltf";

/// The model's fur material (its glTF name starts with this): each cat gets
/// its own copy so it can flash on its own.
const FUR_MATERIAL: &str = "ct-cat-fur";

/// Local: a one-off clip the cat is busy with (`meow`, `gobble`) and the
/// seconds left of it.
#[derive(Component, Default)]
struct CatAct(Option<(&'static str, f32)>);

fn dress_cats(mut commands: Commands, new: Query<(Entity, &FatCat), Added<FatCat>>) {
    for (entity, cat) in &new {
        commands.entity(entity).insert((
            Transform::from_translation(cat.pos)
                .with_rotation(Quat::from_rotation_y(cat.yaw))
                .with_scale(Vec3::splat(cat.size)),
            Visibility::default(),
            super::interact::Clickable::at_height(0.5)
                .radius(80.0)
                .labelled("pet the cat"),
            crate::rigged::Rigged { path: CAT_MODEL },
            crate::rigged::RigClip::looping("idle"),
            CatAct::default(),
            CatFur::default(),
            Beep::default(),
            MeowIn {
                secs: tuning().meow_every,
                count: 0,
            },
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Once a cat's model is in, give it its own copy of the fur material (so
/// only this cat blinks when its fuse is lit). The copy is already in the
/// cat palette, so it skips the filter.
#[allow(clippy::type_complexity)]
fn own_fur(
    mut cats: Query<(Entity, &mut CatFur), With<crate::rigged::RigReady>>,
    children: Query<&Children>,
    mut meshes: Query<(
        &mut MeshMaterial3d<StandardMaterial>,
        &bevy::gltf::GltfMaterialName,
    )>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut exact: Option<ResMut<super::cat_palette::ExactColors>>,
) {
    for (cat, mut fur) in &mut cats {
        if fur.done {
            continue;
        }
        let mut copy: Option<Handle<StandardMaterial>> = None;
        for child in children.iter_descendants(cat) {
            let Ok((mut mat, name)) = meshes.get_mut(child) else {
                continue;
            };
            if !name.0.starts_with(FUR_MATERIAL) {
                continue;
            }
            let handle = match &copy {
                Some(h) => h.clone(),
                None => {
                    let Some(original) = materials.get(&mat.0).cloned() else {
                        continue;
                    };
                    let h = match exact.as_deref_mut() {
                        Some(exact) => {
                            super::cat_palette::exact_material(&mut materials, exact, original)
                        }
                        None => materials.add(original),
                    };
                    copy = Some(h.clone());
                    h
                }
            };
            mat.0 = handle;
        }
        fur.done = copy.is_some();
        fur.material = copy;
    }
}

/// Glide to the replicated spot and size, and pick the clip: a one-off
/// (meow, gobble) runs out first; then lapping, waddling or sitting.
fn move_cats(
    time: Res<Time>,
    mut cats: Query<(
        &FatCat,
        Has<Lapping>,
        &mut Transform,
        &mut CatAct,
        &mut crate::rigged::RigClip,
    )>,
) {
    let dt = time.delta_secs();
    let k = (dt * 8.0).min(1.0);
    for (cat, lapping, mut tf, mut act, mut clip) in &mut cats {
        let gap = cat.pos - tf.translation;
        let moving = gap.xz().length() > 0.02 * cat.size;
        tf.translation += gap * k;
        let size = tf.scale.x + (cat.size - tf.scale.x) * k;
        tf.scale = Vec3::splat(size);
        tf.rotation = Quat::from_rotation_y(cat.yaw);
        if let Some((_, left)) = &mut act.0 {
            *left -= dt;
            if *left <= 0.0 {
                act.0 = None;
            }
        }
        let want = cat_clip(act.0.map(|(name, _)| name), lapping, moving, size);
        if clip.name != want.name || clip.once != want.once {
            *clip = want;
        }
    }
}

/// The clip a cat plays: a one-off clip it's busy with, else lapping,
/// waddling (slower the bigger it is) or sitting.
pub fn cat_clip(
    act: Option<&'static str>,
    lapping: bool,
    moving: bool,
    size: f32,
) -> crate::rigged::RigClip {
    use crate::rigged::RigClip;
    match act {
        Some(name) => RigClip::once(name).fade(0.15),
        None if lapping => RigClip::looping("lap"),
        None if moving => RigClip::looping("walk").speed((1.6 / size.sqrt()).clamp(0.5, 1.6)),
        None => RigClip::looping("idle").fade(0.3),
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

/// The meow countdown after `dt` seconds: looking at the cat puts it back to
/// the full [`MEOW_EVERY`] (#192, corvy: "only when you're not looking at it.
/// looking should reset the timer"). Returns the new countdown and whether it
/// meows now.
pub fn meow_tick(left: f32, seen: bool, dt: f32) -> (f32, bool) {
    if seen {
        return (tuning().meow_every, false);
    }
    let left = left - dt;
    if left <= 0.0 {
        (tuning().meow_every, true)
    } else {
        (left, false)
    }
}

/// A cat you can't see lets out a mew, deeper the bigger it is, at
/// most every [`MEOW_EVERY`] seconds. Each peer judges by its own screen.
fn meow(
    mut commands: Commands,
    time: Res<Time>,
    mut cats: Query<(&FatCat, &Transform, &mut MeowIn, &mut CatAct)>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
) {
    let cam = camera.single().ok();
    for (cat, tf, mut next, mut act) in &mut cats {
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
        // Mouth wide open (unless it's busy gobbling someone).
        if act.0.is_none() {
            act.0 = Some(("meow", tuning().meow_secs));
        }
        let seed = (cat.owner as u32).wrapping_mul(2_654_435_761) ^ next.count;
        let size = cat.size;
        let mouth = cat.pos + Vec3::Y * 0.7 * size;
        let carry = crate::spatial::Carry::CALL;
        crate::spatial::play_wav_at(&mut commands, mouth, 0.7, carry, move || {
            let s = meow_samples(super::synth_audio::RATE, size, seed);
            super::bear::wav_bytes(&s, super::synth_audio::RATE)
        });
    }
}

/// A soft, cartoony "mew" that fits the babbling voices: a sine with a
/// couple of gentle harmonics, gliding up then down. Bigger cats mew lower
/// and a little longer.
pub fn meow_samples(rate: u32, size: f32, seed: u32) -> Vec<f32> {
    use std::f32::consts::TAU;
    let size = size.max(0.1);
    let length = (0.45 + 0.12 * size.sqrt()).min(1.1);
    let pitch_k = (1.0 / size.sqrt()).clamp(0.45, 1.0);
    let wobble = 0.92 + (seed % 7) as f32 * 0.03;
    let n = (length * rate as f32) as usize;
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f32;
    for i in 0..n {
        let t = i as f32 / rate as f32;
        let p = t / length;
        // "mi" up, then "ew" down.
        let contour = if p < 0.3 {
            620.0 + 280.0 * (p / 0.3)
        } else {
            900.0 - 340.0 * ((p - 0.3) / 0.7)
        };
        let vib = 1.0 + 0.012 * (TAU * 5.5 * t).sin();
        let f0 = contour * vib * pitch_k * wobble;
        phase = (phase + f0 / rate as f32).fract();
        // Brighter in the middle ("ee"), rounder at the ends ("m", "w").
        let open = (p * std::f32::consts::PI).sin();
        let voice = (phase * TAU).sin()
            + 0.25 * open * (phase * TAU * 2.0).sin()
            + 0.08 * open * (phase * TAU * 3.0).sin();
        let env = (p * 14.0).min(1.0) * ((1.0 - p) * 4.0).min(1.0);
        out.push(0.45 * voice * env);
    }
    super::synth_audio::master(&mut out, rate, 5000.0, 0.5);
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
            size: tuning().critical,
        };
        let m = mouth(&cat);
        assert!(
            (m - Vec3::new(0.0, 0.0, -tuning().mouth_ahead * tuning().critical)).length() < 1e-4
        );
        // A hungry cat parks you right in its mouth...
        assert!(gobbles(
            &cat,
            Vec3::new(0.0, 0.0, -HUNGRY_HEEL * tuning().critical)
        ));
        // ...but where a calm cat heels, and behind it, you're safe.
        assert!(!gobbles(
            &cat,
            Vec3::new(0.0, 0.0, -tuning().heel * tuning().critical)
        ));
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
        assert!((p.x - tuning().speed * 0.5).abs() < 1e-4);
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
        assert!(
            (petted(tuning().critical) - (tuning().critical - tuning().pet_shrink)).abs() < 1e-5
        );
        assert_eq!(petted(1.0 + tuning().pet_shrink * 0.5), 1.0);
        let near = flung(Vec2::new(2.0, 0.0), Vec2::ZERO);
        assert!((near.x - (2.0 + tuning().blast_push)).abs() < 1e-4);
        let far = Vec2::new(tuning().blast_radius + 1.0, 0.0);
        assert_eq!(flung(far, Vec2::ZERO), far);
        assert!(blink_rate(1.0) > blink_rate(tuning().fuse_secs));
    }

    #[test]
    fn the_cat_plays_the_clip_for_what_it_is_doing() {
        assert_eq!(cat_clip(None, false, false, 1.0).name, "idle");
        assert_eq!(cat_clip(None, false, true, 1.0).name, "walk");
        assert_eq!(cat_clip(None, true, true, 1.0).name, "lap");
        let gulp = cat_clip(Some("gobble"), true, true, 1.0);
        assert!(gulp.name == "gobble" && gulp.once);
        // Giant cats waddle slower.
        assert!(cat_clip(None, false, true, 16.0).speed < cat_clip(None, false, true, 1.0).speed);
    }

    #[test]
    fn meows_wait_and_looking_resets_the_wait() {
        let (left, now) = meow_tick(tuning().meow_every, false, tuning().meow_every - 1.0);
        assert!(!now && (left - 1.0).abs() < 1e-4);
        assert_eq!(meow_tick(left, true, 0.1), (tuning().meow_every, false));
        let (left, now) = meow_tick(0.5, false, 1.0);
        assert!(now);
        assert_eq!(left, tuning().meow_every);
    }

    #[test]
    fn meow_is_soft_and_bigger_cats_meow_longer() {
        let rate = crate::features::synth_audio::RATE;
        let small = meow_samples(rate, 1.0, 3);
        let big = meow_samples(rate, 16.0, 3);
        assert!(big.len() > small.len());
        for s in [&small, &big] {
            let peak = s.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak > 0.3 && peak <= 0.55, "peak {peak}");
            assert!(s.iter().all(|x| x.is_finite()));
        }
    }
}
