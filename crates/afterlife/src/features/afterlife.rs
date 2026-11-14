//! What dying means: heaven, begging, the Tome of Uncancelling and the hell
//! pit. From cumsauce, answering #209 ("What should dying actually mean?"):
//! "you go to heaven. players must beg for your innocence. if they succeed,
//! you may return. players may also use a Tome of Uncancelling. if you are
//! thrown to hell, you must navigate the hell pit before coming back."
//!
//! - When you die (today: an RPG rocket from the mute spell, #146) a
//!   gravestone goes up where you fell and you wake up in heaven, the top
//!   floating island with Abuela. You can't leave it: walk off the edge and
//!   you're back in the clouds.
//! - Heaven judges you after [`JUDGEMENT_SECS`]. Until then the living can
//!   click your grave to beg for your innocence. Each plea counts one, or
//!   two if you beg out loud (your mic is sending voice). [`PLEAS_NEEDED`]
//!   pleas in time and you're innocent: you return at your grave.
//! - Not enough pleas in time and you're guilty: thrown into the hell pit, a
//!   lava cave far under the map. Hop the rising ring of stepping stones to
//!   the white light in the middle and you come back to your spawn point.
//!   Touch the lava and you start the pit again.
//! - The Tome of Uncancelling sits on a lectern in the commune. Take it,
//!   click a grave and that friend comes straight back, from heaven or hell.
//!   It's used up, and a new one appears on the lectern after
//!   [`TOME_RESPAWN`] seconds.
//!
//! The server owns all of it: [`Dead`] sits on the dead player's `Player`
//! entity, [`Grave`]s and the [`Tome`] are replicated entities. Other
//! features kill a player by triggering [`Die`] on the server.

use std::collections::HashMap;

use avian3d::prelude::{Collider, RigidBody};
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::abuela::heaven;
use crate::features::fps::{Walker, spawn_point};
use crate::features::interact::{Clickable, Clicked, SERVER_REACH, deny, in_reach, within};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::voice_chat::{DeviceState, VoiceActivity, VoiceChatState};

/// Lectern top (where the tome lies), above the ground.
pub const LECTERN_TOP: f32 = 1.05;

/// Tuning numbers: `assets/data/afterlife/afterlife.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Seconds heaven waits before it judges you.
    pub judgement_secs: f32,
    /// Pleas it takes to prove you innocent.
    pub pleas_needed: u32,
    /// Seconds between one player's pleas (no auto-clickers in heaven).
    pub plea_gap: f32,
    /// Seconds before a used Tome of Uncancelling is back on its lectern.
    pub tome_respawn: f32,
    /// How far from the centre the climb starts.
    pub start_ring: f32,
    /// Stepping stones: the gap between edges and how much each one rises.
    pub stone_gap: f32,
    pub stone_rise: f32,
    /// The first stone's top above the lava.
    pub start_top: f32,
    /// The white light's platform in the middle.
    pub exit_radius: f32,
    /// Stand this close to the white light to get out.
    pub exit_reach: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/afterlife/afterlife.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}
/// The lectern the tome sits on, in the commune by the spawn.
pub use friendslop_world::places::lectern_at;

/// The hell pit: centre (x, z) far under the forest, and its lava level.
pub use friendslop_world::places::hell_at;
/// The lava level: the hell spot's `Feature` depth.
pub fn lava_y() -> f32 {
    friendslop_world::places::sizes("hell").depth
}
/// The pit's wall radius and ceiling height above the lava (the hell spot's
/// `Feature` radius and height).
pub fn pit_radius() -> f32 {
    friendslop_world::places::sizes("hell").radius
}
fn pit_height() -> f32 {
    friendslop_world::places::sizes("hell").height
}

/// On a dead player's `Player` entity. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Dead {
    /// In heaven, waiting to be judged: whole seconds left, pleas so far.
    Heaven { secs_left: u32, pleas: u32 },
    /// Thrown to hell: cross the pit to come back.
    Hell,
}

/// Where player `player` died. Replicated; gone once they're back.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Grave {
    pub player: u64,
    pub at: Vec3,
}

/// The Tome of Uncancelling: on its lectern, or carried by `holder`.
/// Replicated. There's at most one.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct Tome {
    pub holder: Option<u64>,
}

/// Server, local: kill player `player`. Off they go to heaven.
#[derive(Event, Clone, Copy, Debug)]
pub struct Die {
    pub player: u64,
}

/// Client -> server: I beg for `player`'s innocence (out loud if `aloud`).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Plead {
    pub player: u64,
    pub aloud: bool,
}

/// Client -> server: let me take the tome off its lectern.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TakeTome;

/// Client -> server: read the tome over `player`'s grave.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct UseTome {
    pub player: u64,
}

/// What just happened to a dead player.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Fate {
    Died,
    Begged { by: u64, line: u8 },
    Innocent,
    Guilty,
    Uncancelled { by: u64 },
    Lava,
    Escaped,
}

/// Server -> everyone: `player`'s afterlife moved on.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Afterlife {
    pub player: u64,
    pub fate: Fate,
}

/// What a plea sounds like in the toast.
pub const PLEAS: [&str; 8] = [
    "they didn't mean it",
    "they're a good person, mostly",
    "they owe me money, please",
    "they were going to pay the rent",
    "they only farted the once",
    "please, they have a fat cat at home",
    "they've changed, I swear",
    "it was the landlord's fault",
];

/// Server only: seconds left until judgement.
#[derive(Component)]
struct Judgement(f32);

/// Server only: when each player last pleaded, and when the tome comes back.
#[derive(Resource, Default)]
struct Ledger {
    last_plea: HashMap<u64, f32>,
    tome_back_in: f32,
}

pub fn protocol(app: &mut App) {
    friendslop_world::footprints::claim(lectern_at(), 1.2);
    app.replicate::<Dead>()
        .replicate::<Grave>()
        .replicate::<Tome>()
        .add_client_event::<Plead>(Channel::Ordered)
        .add_client_event::<TakeTome>(Channel::Ordered)
        .add_client_event::<UseTome>(Channel::Ordered)
        .add_server_event::<Afterlife>(Channel::Ordered)
        .init_resource::<Ledger>()
        .add_observer(on_die)
        .add_observer(on_plead)
        .add_observer(on_take_tome)
        .add_observer(on_use_tome)
        .add_systems(
            Update,
            (judge, keep_the_dead_in, tidy_up, shelve_tome)
                .chain()
                .run_if(in_state(ServerState::Running)),
        );
}

// ---------------------------------------------------------------- pure rules

/// How much one plea counts: begging out loud counts double.
pub fn plea_worth(aloud: bool) -> u32 {
    if aloud { 2 } else { 1 }
}

/// Heaven's verdict, if it has one yet: `Some(true)` innocent, `Some(false)`
/// guilty, `None` still waiting.
pub fn verdict(pleas: u32, secs_left: f32) -> Option<bool> {
    if pleas >= tuning().pleas_needed {
        Some(true)
    } else if secs_left <= 0.0 {
        Some(false)
    } else {
        None
    }
}

/// Where the dead wake up in heaven: across the island from Abuela's chair.
pub fn heaven_arrival() -> Vec3 {
    let h = heaven();
    Vec3::new(h.at.x, h.top, h.at.y - h.radius * 0.4)
}

/// Has a dead body with feet at `feet` wandered out of heaven?
pub fn left_heaven(feet: Vec3) -> bool {
    let h = heaven();
    feet.xz().distance(h.at) > h.radius + 1.0 || feet.y < h.top - 2.0
}

/// One stepping stone in the hell pit: top centre and radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stone {
    pub top: Vec3,
    pub radius: f32,
}

/// The stepping stones, first (the landing) to last, spiralling in and up
/// towards the white light in the middle. A pure function of nothing, so
/// every peer builds the same pit.
pub fn hell_stones() -> Vec<Stone> {
    let at = |ring: f32, angle: f32| hell_at() + Vec2::new(angle.cos(), angle.sin()) * ring;
    let mut stones = vec![Stone {
        top: at(tuning().start_ring, 0.0)
            .extend(lava_y() + tuning().start_top)
            .xzy(),
        radius: 2.5,
    }];
    let mut last = (at(tuning().start_ring, 0.0), 2.5);
    let mut angle = 0.0f32;
    loop {
        angle += 0.002;
        let ring = tuning().start_ring - angle * 0.75;
        if ring < 4.0 {
            break;
        }
        let p = at(ring, angle);
        // Every third stone is a small one.
        let radius = if stones.len() % 3 == 0 { 0.8 } else { 1.1 };
        if p.distance(last.0) >= last.1 + radius + tuning().stone_gap {
            let n = stones.len() as f32;
            stones.push(Stone {
                top: Vec3::new(
                    p.x,
                    lava_y() + tuning().start_top + n * tuning().stone_rise,
                    p.y,
                ),
                radius,
            });
            last = (p, radius);
        }
    }
    stones
}

/// Where you land in the pit: on the first, wide stone.
pub fn hell_start() -> Vec3 {
    hell_stones()[0].top
}

/// The white light's platform in the middle, a step above the last stone.
pub fn hell_exit() -> Vec3 {
    let top = hell_stones()
        .last()
        .map_or(lava_y() + tuning().start_top, |s| s.top.y)
        + tuning().stone_rise;
    Vec3::new(hell_at().x, top, hell_at().y)
}

/// Has a body in hell with feet at `feet` touched the lava (or got out of
/// the pit some other way)?
pub fn burnt(feet: Vec3) -> bool {
    feet.y < lava_y() + 0.2
        || feet.y > lava_y() + pit_height()
        || feet.xz().distance(hell_at()) > pit_radius() + 1.0
}

/// Has a body in hell with feet at `feet` reached the white light?
pub fn escaped(feet: Vec3) -> bool {
    let exit = hell_exit();
    feet.xz().distance(exit.xz()) <= tuning().exit_reach && (feet.y - exit.y).abs() < 1.5
}

// ---------------------------------------------------------------- server side

fn tell(commands: &mut Commands, player: u64, fate: Fate) {
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: Afterlife { player, fate },
    });
}

/// Move a body somewhere, stopping it dead.
fn put(walker: &mut Walker, kcc: Option<Mut<crate::kcc::Kcc>>, at: Vec3) {
    walker.pos = at;
    if let Some(mut kcc) = kcc {
        kcc.velocity = Vec3::ZERO;
    }
}

/// The player who sent a client event: (entity, id).
fn sender(
    client: ClientId,
    players: &Query<(Entity, &Player, &PlayerOwner)>,
) -> Option<(Entity, u64)> {
    let owner = match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    players
        .iter()
        .find(|(_, _, o)| o.0 == owner)
        .map(|(e, p, _)| (e, p.id))
}

fn on_die(
    ev: On<Die>,
    mut commands: Commands,
    mut players: Query<(
        Entity,
        &Player,
        &mut Walker,
        Option<&mut crate::kcc::Kcc>,
        Option<&Dead>,
    )>,
    mut tomes: Query<&mut Tome>,
) {
    let Some((entity, _, mut walker, kcc, dead)) =
        players.iter_mut().find(|(_, p, ..)| p.id == ev.player)
    else {
        return;
    };
    if dead.is_some() {
        return; // already dead
    }
    commands.spawn((
        Grave {
            player: ev.player,
            at: walker.pos,
        },
        Replicated,
    ));
    commands.entity(entity).insert((
        Dead::Heaven {
            secs_left: tuning().judgement_secs as u32,
            pleas: 0,
        },
        Judgement(tuning().judgement_secs),
    ));
    put(&mut walker, kcc, heaven_arrival());
    // You can't take it with you: the tome goes back on its lectern.
    for mut tome in &mut tomes {
        if tome.holder == Some(ev.player) {
            tome.holder = None;
        }
    }
    tell(&mut commands, ev.player, Fate::Died);
}

/// Server: bring `entity` back to life at `at`.
fn revive(
    commands: &mut Commands,
    entity: Entity,
    walker: &mut Walker,
    kcc: Option<Mut<crate::kcc::Kcc>>,
    at: Vec3,
) {
    commands.entity(entity).remove::<(Dead, Judgement)>();
    put(walker, kcc, at);
}

fn on_plead(
    ev: On<FromClient<Plead>>,
    time: Res<Time>,
    mut commands: Commands,
    mut ledger: ResMut<Ledger>,
    who: Query<(Entity, &Player, &PlayerOwner)>,
    bodies: Query<(&PlayerOwner, &Walker)>,
    mut dead: Query<(&Player, &mut Dead)>,
    graves: Query<&Grave>,
) {
    let Some((me, my_id)) = sender(ev.client_id, &who) else {
        return;
    };
    if dead.get(me).is_ok() {
        return; // the dead can't beg
    }
    let Some(grave) = graves.iter().find(|g| g.player == ev.player) else {
        return;
    };
    let Some((_, mut state)) = dead.iter_mut().find(|(p, _)| p.id == ev.player) else {
        return;
    };
    let Dead::Heaven { pleas, .. } = &mut *state else {
        deny(
            &mut commands,
            ev.client_id,
            "too late: they're in hell. Only the Tome of Uncancelling can help now",
        );
        return;
    };
    if !in_reach(&mut commands, ev.client_id, &bodies, grave.at) {
        return;
    }
    let now = time.elapsed_secs();
    if ledger
        .last_plea
        .get(&my_id)
        .is_some_and(|t| now - t < tuning().plea_gap)
    {
        return;
    }
    ledger.last_plea.insert(my_id, now);
    *pleas += plea_worth(ev.aloud);
    let line = (*pleas as usize + my_id as usize) % PLEAS.len();
    tell(
        &mut commands,
        ev.player,
        Fate::Begged {
            by: my_id,
            line: line as u8,
        },
    );
}

fn on_take_tome(
    ev: On<FromClient<TakeTome>>,
    mut commands: Commands,
    who: Query<(Entity, &Player, &PlayerOwner)>,
    bodies: Query<(&PlayerOwner, &Walker)>,
    dead: Query<&Dead>,
    mut tomes: Query<&mut Tome>,
) {
    let Some((me, my_id)) = sender(ev.client_id, &who) else {
        return;
    };
    if dead.get(me).is_ok() {
        return;
    }
    let Some(mut tome) = tomes.iter_mut().next() else {
        return;
    };
    if let Some(holder) = tome.holder {
        if holder != my_id {
            deny(
                &mut commands,
                ev.client_id,
                format!("player {holder} has the tome"),
            );
        }
        return;
    }
    let lectern = lectern_at().extend(0.0).xzy().with_y(lectern_ground());
    if !in_reach(&mut commands, ev.client_id, &bodies, lectern) {
        return;
    }
    tome.holder = Some(my_id);
}

#[allow(clippy::type_complexity)]
fn on_use_tome(
    ev: On<FromClient<UseTome>>,
    mut commands: Commands,
    mut ledger: ResMut<Ledger>,
    tomes: Query<(Entity, &Tome)>,
    graves: Query<(Entity, &Grave)>,
    mut players: Query<(
        Entity,
        &Player,
        &PlayerOwner,
        &mut Walker,
        Option<&mut crate::kcc::Kcc>,
        Has<Dead>,
    )>,
) {
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((my_id, my_feet)) = players
        .iter()
        .find(|(_, _, o, ..)| o.0 == owner)
        .map(|(_, p, _, w, ..)| (p.id, w.pos))
    else {
        return;
    };
    let Some((tome, _)) = tomes.iter().find(|(_, t)| t.holder == Some(my_id)) else {
        return;
    };
    let Some((grave_entity, grave)) = graves.iter().find(|(_, g)| g.player == ev.player) else {
        return;
    };
    let at = grave.at;
    if !within(my_feet, at, SERVER_REACH) {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    let Some((entity, _, _, mut walker, kcc, true)) =
        players.iter_mut().find(|(_, p, ..)| p.id == ev.player)
    else {
        return;
    };
    revive(&mut commands, entity, &mut walker, kcc, at);
    commands.entity(grave_entity).despawn();
    commands.entity(tome).despawn();
    ledger.tome_back_in = tuning().tome_respawn;
    tell(&mut commands, ev.player, Fate::Uncancelled { by: my_id });
}

/// Server: count down judgement, and pass it.
#[allow(clippy::type_complexity)]
fn judge(
    time: Res<Time>,
    mut commands: Commands,
    graves: Query<(Entity, &Grave)>,
    mut dead: Query<(
        Entity,
        &Player,
        &mut Dead,
        &mut Walker,
        Option<&mut crate::kcc::Kcc>,
        Option<&mut Judgement>,
    )>,
) {
    let dt = time.delta_secs().min(0.1);
    for (entity, player, mut state, mut walker, kcc, judgement) in &mut dead {
        let Dead::Heaven { secs_left, pleas } = *state else {
            continue;
        };
        let Some(mut judgement) = judgement else {
            continue;
        };
        judgement.0 -= dt;
        match verdict(pleas, judgement.0) {
            Some(true) => {
                let grave = graves.iter().find(|(_, g)| g.player == player.id);
                let at = grave.map_or(spawn_point(player.id), |(_, g)| g.at);
                if let Some((g, _)) = grave {
                    commands.entity(g).despawn();
                }
                revive(&mut commands, entity, &mut walker, kcc, at);
                tell(&mut commands, player.id, Fate::Innocent);
            }
            Some(false) => {
                *state = Dead::Hell;
                commands.entity(entity).remove::<Judgement>();
                put(&mut walker, kcc, hell_start());
                tell(&mut commands, player.id, Fate::Guilty);
            }
            None => {
                let whole = judgement.0.ceil().max(0.0) as u32;
                if whole != secs_left {
                    *state = Dead::Heaven {
                        secs_left: whole,
                        pleas,
                    };
                }
            }
        }
    }
}

/// Server: heaven keeps hold of you, the lava sends you back, and the white
/// light lets you out.
#[allow(clippy::type_complexity)]
fn keep_the_dead_in(
    mut commands: Commands,
    graves: Query<(Entity, &Grave)>,
    mut dead: Query<(
        Entity,
        &Player,
        &Dead,
        &mut Walker,
        Option<&mut crate::kcc::Kcc>,
    )>,
) {
    for (entity, player, state, mut walker, kcc) in &mut dead {
        match state {
            Dead::Heaven { .. } => {
                if left_heaven(walker.pos) {
                    put(&mut walker, kcc, heaven_arrival());
                }
            }
            Dead::Hell => {
                if escaped(walker.pos) {
                    for (g, grave) in &graves {
                        if grave.player == player.id {
                            commands.entity(g).despawn();
                        }
                    }
                    revive(
                        &mut commands,
                        entity,
                        &mut walker,
                        kcc,
                        spawn_point(player.id),
                    );
                    tell(&mut commands, player.id, Fate::Escaped);
                } else if burnt(walker.pos) {
                    put(&mut walker, kcc, hell_start());
                    tell(&mut commands, player.id, Fate::Lava);
                }
            }
        }
    }
}

/// Server: graves of players who left go, and so does their hold on the tome.
fn tidy_up(
    mut commands: Commands,
    players: Query<&Player>,
    graves: Query<(Entity, &Grave)>,
    mut tomes: Query<&mut Tome>,
) {
    let here = |id: u64| players.iter().any(|p| p.id == id);
    for (entity, grave) in &graves {
        if !here(grave.player) {
            commands.entity(entity).despawn();
        }
    }
    for mut tome in &mut tomes {
        if tome.holder.is_some_and(|h| !here(h)) {
            tome.holder = None;
        }
    }
}

/// Server: a new tome on the lectern once the last one's been used.
fn shelve_tome(
    time: Res<Time>,
    mut commands: Commands,
    mut ledger: ResMut<Ledger>,
    tomes: Query<(), With<Tome>>,
) {
    if !tomes.is_empty() {
        return;
    }
    ledger.tome_back_in -= time.delta_secs();
    if ledger.tome_back_in <= 0.0 {
        commands.spawn((Tome::default(), Replicated));
    }
}

fn lectern_ground() -> f32 {
    crate::terrain::height_at(lectern_at().x, lectern_at().y)
}

// ---------------------------------------------------------------- client side

/// Local: seconds since this peer last sent voice.
#[derive(Resource)]
struct LastSpoke(f32);

impl Default for LastSpoke {
    fn default() -> Self {
        Self(f32::INFINITY)
    }
}

/// You have to have been talking this recently for a plea to count as out loud.
const ALOUD_WITHIN: f32 = 2.0;

pub fn plugin(app: &mut App) {
    app.init_resource::<LastSpoke>()
        .add_observer(click_grave)
        .add_observer(click_tome)
        .add_observer(announce)
        .add_systems(
            OnEnter(AppState::InGame),
            (dig_hell, build_lectern, spawn_hud, afterlife_help),
        )
        .add_systems(
            Update,
            (
                listen,
                dress_graves,
                label_graves,
                dress_tome,
                place_tome,
                glow_exit,
                update_hud.run_if(crate::world::hud_tick()),
            )
                .run_if(in_state(AppState::InGame)),
        );
}

fn listen(time: Res<Time>, activity: Option<Res<VoiceActivity>>, mut spoke: ResMut<LastSpoke>) {
    if activity.is_some_and(|a| a.transmitting) {
        spoke.0 = 0.0;
    } else {
        spoke.0 += time.delta_secs();
    }
}

fn holding_tome(me: Option<u64>, tomes: &Query<&Tome>) -> bool {
    me.is_some() && tomes.iter().any(|t| t.holder == me)
}

fn click_grave(
    click: On<Clicked>,
    mut commands: Commands,
    me: Option<Res<LocalPlayer>>,
    graves: Query<&Grave>,
    tomes: Query<&Tome>,
    spoke: Res<LastSpoke>,
    voice: Option<Res<VoiceChatState>>,
    self_muted: Option<Res<crate::voice::VoiceMuted>>,
) {
    let Ok(grave) = graves.get(click.entity) else {
        return;
    };
    if holding_tome(me.map(|m| m.0), &tomes) {
        commands.client_trigger(UseTome {
            player: grave.player,
        });
        return;
    }
    let mic_live = voice.is_some_and(|v| matches!(v.mic, DeviceState::Live(_)));
    let aloud = mic_live && !self_muted.is_some_and(|m| m.0) && spoke.0 <= ALOUD_WITHIN;
    commands.client_trigger(Plead {
        player: grave.player,
        aloud,
    });
}

fn click_tome(click: On<Clicked>, mut commands: Commands, tomes: Query<&Tome>) {
    if tomes.get(click.entity).is_ok() {
        commands.client_trigger(TakeTome);
    }
}

fn who(id: u64, me: Option<u64>) -> String {
    if me == Some(id) {
        "you".to_string()
    } else {
        format!("player {id}")
    }
}

/// The toast for what just happened to `player`, as seen by `me`.
pub fn fate_text(player: u64, fate: Fate, me: Option<u64>) -> String {
    let mine = me == Some(player);
    let them = who(player, me);
    match fate {
        Fate::Died if mine => format!(
            "you died. Heaven judges you in {} s: your friends must beg at your grave",
            tuning().judgement_secs as u32
        ),
        Fate::Died => {
            format!("{them} died and went to heaven. Beg at their grave to bring them back")
        }
        Fate::Begged { by, line } => {
            let plea = PLEAS[line as usize % PLEAS.len()];
            if me == Some(by) {
                format!("you beg: \"{plea}\"")
            } else {
                format!("{} begs for {them}: \"{plea}\"", who(by, me))
            }
        }
        Fate::Innocent if mine => "found innocent! You return to the living".to_string(),
        Fate::Innocent => format!("{them} was found innocent and is back"),
        Fate::Guilty if mine => {
            "GUILTY. Thrown to hell: hop across the lava to the white light".to_string()
        }
        Fate::Guilty => format!("{them} was found guilty and thrown to hell"),
        Fate::Uncancelled { by } => format!(
            "{} read the Tome of Uncancelling: {them} {} back",
            who(by, me),
            if mine { "are" } else { "is" }
        ),
        Fate::Lava if mine => "the lava. Back to the start of the pit".to_string(),
        Fate::Lava => String::new(),
        Fate::Escaped if mine => "you climbed out of hell".to_string(),
        Fate::Escaped => format!("{them} climbed out of hell"),
    }
}

fn announce(
    ev: On<Afterlife>,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<Toast>>,
    state: Option<Res<State<AppState>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    let Some(mut toast) = toast else { return };
    let text = fate_text(ev.player, ev.fate, me.map(|m| m.0));
    if !text.is_empty() {
        toast.show(text);
    }
}

const BEG_LABEL: &str = "beg for their innocence";
const TOME_LABEL: &str = "read the Tome of Uncancelling";

/// Local: a gravestone where someone died.
fn dress_graves(
    mut commands: Commands,
    new: Query<(Entity, &Grave), Added<Grave>>,
    assets: Option<Res<AssetServer>>,
) {
    if new.is_empty() {
        return;
    }
    let Some(assets) = assets else {
        return;
    };
    for (entity, grave) in &new {
        commands
            .entity(entity)
            .insert((
                Transform::from_translation(grave.at),
                Visibility::default(),
                Clickable::at_height(0.6).labelled(BEG_LABEL),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                // Headstone and its mound (+z): modeler-made, prefab `sp-gravestone`.
                p.spawn((model(&assets, "gravestone.gltf"), Transform::default()));
            });
    }
}

/// A modeler-made feature model in `assets/models/features`.
fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

/// Local: holding the tome turns "beg" into "read the tome".
fn label_graves(
    me: Option<Res<LocalPlayer>>,
    tomes: Query<&Tome>,
    mut graves: Query<&mut Clickable, With<Grave>>,
) {
    let label = if holding_tome(me.map(|m| m.0), &tomes) {
        TOME_LABEL
    } else {
        BEG_LABEL
    };
    for mut c in &mut graves {
        if c.label != Some(label) {
            c.label = Some(label);
        }
    }
}

/// Local: the tome's look, a fat purple book with gold edges (modeler-made,
/// prefab `by-tome`, centred on the tome's own transform; it glows).
fn dress_tome(
    mut commands: Commands,
    new: Query<Entity, Added<Tome>>,
    assets: Option<Res<AssetServer>>,
) {
    if new.is_empty() {
        return;
    }
    let Some(assets) = assets else {
        return;
    };
    for entity in &new {
        commands
            .entity(entity)
            .insert((
                Transform::from_translation(lectern_top()),
                Visibility::default(),
                Clickable::at_height(0.1).labelled("take the Tome of Uncancelling"),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((model(&assets, "tome.gltf"), Transform::default()));
            });
    }
}

fn lectern_top() -> Vec3 {
    Vec3::new(
        lectern_at().x,
        lectern_ground() + LECTERN_TOP,
        lectern_at().y,
    )
}

/// Local: the tome lies on its lectern, or floats at its holder's side.
fn place_tome(
    time: Res<Time>,
    mut commands: Commands,
    walkers: Query<(&Player, &Walker)>,
    mut tomes: Query<(Entity, &Tome, &mut Transform, Has<Clickable>)>,
) {
    let t = time.elapsed_secs();
    for (entity, tome, mut tf, clickable) in &mut tomes {
        let held = tome
            .holder
            .and_then(|h| walkers.iter().find(|(p, _)| p.id == h));
        let at = match held {
            Some((_, w)) => {
                let side = Quat::from_rotation_y(w.yaw) * Vec3::new(0.45, 0.0, -0.2);
                w.pos + side + Vec3::Y * (1.0 + (t * 2.0).sin() * 0.05)
            }
            None => lectern_top() + Vec3::Y * (t * 1.5).sin().abs() * 0.04,
        };
        tf.translation = at;
        tf.rotation = Quat::from_rotation_y(held.map_or(0.3, |(_, w)| w.yaw));
        match (held.is_some(), clickable) {
            (true, true) => {
                commands.entity(entity).remove::<Clickable>();
            }
            (false, false) => {
                commands
                    .entity(entity)
                    .insert(Clickable::at_height(0.1).labelled("take the Tome of Uncancelling"));
            }
            _ => {}
        }
    }
}

/// Every peer: the lectern the tome lies on (modeler-made, prefab
/// `sp-lectern`: its top is at `LECTERN_TOP`, turned 0.3 rad like the tome).
fn build_lectern(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    let Some(assets) = assets else {
        return;
    };
    commands.spawn((
        model(&assets, "lectern.gltf"),
        Transform::from_xyz(lectern_at().x, lectern_ground(), lectern_at().y),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Local: the white light at the top of the pit.
#[derive(Component)]
struct ExitLight;

fn glow_exit(time: Res<Time>, mut lights: Query<&mut Transform, With<ExitLight>>) {
    let s = 1.0 + (time.elapsed_secs() * 2.0).sin() * 0.08;
    for mut tf in &mut lights {
        tf.scale = Vec3::new(s, 1.0, s);
    }
}

/// Every peer: the hell pit, a closed lava cave with stepping stones.
fn dig_hell(
    mut commands: Commands,
    assets: Option<Res<AssetServer>>,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let lava = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.35, 0.05),
        emissive: LinearRgba::rgb(6.0, 1.2, 0.1),
        unlit: true,
        ..default()
    });
    let light = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.7),
        emissive: LinearRgba::rgb(8.0, 8.0, 7.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    let centre = hell_at().extend(lava_y()).xzy();
    // Walls and roof are modeler rock (prefabs `sp-hell-wall`, 4.3 x 28 x 1
    // with its rough face toward -z, and `sp-hell-roof`, 36 x 1 x 36) stretched
    // over code colliders, so the pit closes even without the files.
    let spawn_solid =
        |commands: &mut Commands, file: &str, fit: Vec3, tf: Transform, collider: Collider| {
            let mut solid = commands.spawn((
                tf,
                Visibility::default(),
                RigidBody::Static,
                collider,
                avian_rerecast::ExcludeColliderFromNavmesh,
                DespawnOnExit(AppState::InGame),
            ));
            if let Some(assets) = &assets {
                solid.with_child((model(assets, file), Transform::from_scale(fit)));
            }
        };
    // The lava lake (no collider: you sink into it).
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(pit_radius(), 0.2))),
        MeshMaterial3d(lava),
        Transform::from_translation(centre),
        DespawnOnExit(AppState::InGame),
    ));
    // Walls all round and a ceiling, so it's dark and closed.
    let sides = 28;
    let panel = 2.0 * pit_radius() * (std::f32::consts::PI / sides as f32).tan() + 0.4;
    let panel_fit = Vec3::new(panel / 4.3, (pit_height() + 2.0) / 28.0, 1.0);
    for i in 0..sides {
        let a = i as f32 / sides as f32 * std::f32::consts::TAU;
        let out = Vec3::new(a.cos(), 0.0, a.sin());
        spawn_solid(
            &mut commands,
            "hell_wall.gltf",
            panel_fit,
            Transform::from_translation(
                centre + out * (pit_radius() + 0.5) + Vec3::Y * (pit_height() / 2.0),
            )
            .with_rotation(Quat::from_rotation_y(-a + std::f32::consts::FRAC_PI_2)),
            Collider::cuboid(panel, pit_height() + 2.0, 1.0),
        );
    }
    let roof = 2.0 * pit_radius() + 4.0;
    spawn_solid(
        &mut commands,
        "hell_roof.gltf",
        Vec3::new(roof / 36.0, 1.0, roof / 36.0),
        Transform::from_translation(centre + Vec3::Y * (pit_height() + 0.5)),
        Collider::cuboid(roof, 1.0, roof),
    );
    // A basalt column standing up out of the lava: the collider is the
    // cylinder, the look a modeler-made column (prefabs `by-basalt-column`,
    // `by-basalt-cluster`: unit radius, 4 m tall, centred) stretched to fit.
    let spawn_basalt =
        |commands: &mut Commands, top: Vec3, radius: f32, turn: f32| {
            let height = top.y - lava_y() + 1.0;
            let mut rock = commands.spawn((
                Transform::from_translation(top - Vec3::Y * (height / 2.0)),
                Visibility::default(),
                RigidBody::Static,
                Collider::cylinder(radius, height),
                avian_rerecast::ExcludeColliderFromNavmesh,
                DespawnOnExit(AppState::InGame),
            ));
            if let Some(assets) = &assets {
                let file = if radius >= 1.5 {
                    "basalt_cluster.gltf"
                } else {
                    "basalt_column.gltf"
                };
                rock.with_children(|p| {
                    p.spawn((
                        model(assets, file),
                        Transform::from_rotation(Quat::from_rotation_y(turn))
                            .with_scale(Vec3::new(radius, height / 4.0, radius)),
                    ));
                });
            }
        };
    // The stepping stones, standing up out of the lava.
    for (i, stone) in hell_stones().into_iter().enumerate() {
        spawn_basalt(&mut commands, stone.top, stone.radius, i as f32 * 2.4);
    }
    // The white light's platform and its beam.
    let exit = hell_exit();
    spawn_basalt(&mut commands, exit, tuning().exit_radius, 0.0);
    // The way out, marked like every other way out.
    crate::features::doorway::spawn(
        &mut commands,
        exit,
        0.0,
        crate::features::doorway::Doorway::way_out("back to your spawn")
            .hatch(tuning().exit_reach * 1.4),
    );
    let beam = pit_height() - (exit.y - lava_y());
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(tuning().exit_reach * 0.7, beam))),
        MeshMaterial3d(light),
        Transform::from_translation(exit + Vec3::Y * (beam / 2.0)),
        ExitLight,
        bevy::light::NotShadowCaster,
        DespawnOnExit(AppState::InGame),
    ));
    // Red glow from the lava, and white from the way out.
    for i in 0..4 {
        let a = i as f32 / 4.0 * std::f32::consts::TAU + 0.4;
        commands.spawn((
            PointLight {
                color: Color::srgb(1.0, 0.35, 0.1),
                intensity: 400_000.0,
                range: 30.0,
                ..default()
            },
            Transform::from_translation(centre + Vec3::new(a.cos() * 8.0, 3.0, a.sin() * 8.0)),
            DespawnOnExit(AppState::InGame),
        ));
    }
    commands.spawn((
        PointLight {
            color: Color::WHITE,
            intensity: 300_000.0,
            range: 20.0,
            ..default()
        },
        Transform::from_translation(exit + Vec3::Y * 3.0),
        DespawnOnExit(AppState::InGame),
    ));
}

fn afterlife_help(mut commands: Commands) {
    commands.spawn((
        Text::new(
            "dying: you go to heaven. Friends click your grave to beg (out loud counts double), \
             or read the Tome of Uncancelling (lectern by the spawn) over it.",
        ),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

#[derive(Component)]
struct AfterlifeHud;

fn spawn_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextColor(crate::theme::PINK),
        AfterlifeHud,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
}

/// The HUD lines for `me`, given everyone who's dead and the tome's holder.
pub fn hud_lines(me: u64, dead: &[(u64, Dead)], tome_holder: Option<u64>) -> Vec<String> {
    let mut lines = Vec::new();
    let needed = tuning().pleas_needed;
    for (id, state) in dead {
        let line = match (*id == me, state) {
            (true, Dead::Heaven { secs_left, pleas }) => {
                format!("DEAD. Judged in {secs_left} s. Pleas for your innocence: {pleas}/{needed}")
            }
            (true, Dead::Hell) => {
                "IN HELL. Hop across the lava to the white light in the middle".to_string()
            }
            (false, Dead::Heaven { secs_left, pleas }) => {
                format!("player {id} is dead: beg at their grave ({pleas}/{needed}, {secs_left} s)")
            }
            (false, Dead::Hell) => format!("player {id} is in hell"),
        };
        lines.push(line);
    }
    if tome_holder == Some(me) {
        lines.push(crate::input::fill(
            "you hold the Tome of Uncancelling: {use} a grave to read it",
        ));
    }
    lines
}

fn update_hud(
    me: Option<Res<LocalPlayer>>,
    dead: Query<(&Player, &Dead)>,
    tomes: Query<&Tome>,
    mut hud: Query<&mut Text, With<AfterlifeHud>>,
) {
    let Some(me) = me else { return };
    let mut list: Vec<(u64, Dead)> = dead.iter().map(|(p, d)| (p.id, *d)).collect();
    list.sort_by_key(|(id, _)| *id);
    let holder = tomes.iter().find_map(|t| t.holder);
    let want = hud_lines(me.0, &list, holder).join("\n");
    for mut text in &mut hud {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn begging_out_loud_counts_double() {
        assert_eq!(plea_worth(false), 1);
        assert_eq!(plea_worth(true), 2);
    }

    #[test]
    fn heaven_waits_then_judges() {
        assert_eq!(verdict(0, 30.0), None);
        assert_eq!(verdict(tuning().pleas_needed, 30.0), Some(true));
        assert_eq!(
            verdict(tuning().pleas_needed, 0.0),
            Some(true),
            "just in time"
        );
        assert_eq!(verdict(tuning().pleas_needed - 1, 0.0), Some(false));
    }

    #[test]
    fn you_wake_up_in_heaven_and_cant_leave() {
        assert!(!left_heaven(heaven_arrival()));
        assert!(left_heaven(heaven_arrival() - Vec3::Y * 5.0));
        assert!(left_heaven(Vec3::ZERO));
    }

    #[test]
    fn the_hell_pit_can_be_crossed() {
        let stones = hell_stones();
        assert!(stones.len() > 10, "a proper climb");
        for pair in stones.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let gap = a.top.xz().distance(b.top.xz()) - a.radius - b.radius;
            assert!((0.5..=2.0).contains(&gap), "a jumpable gap, not {gap}");
            assert!(b.top.y - a.top.y <= 0.5, "no higher than a step");
        }
        let last = stones[stones.len() - 1];
        let exit = hell_exit();
        let gap = last.top.xz().distance(exit.xz()) - last.radius - tuning().exit_radius;
        assert!(
            (0.3..=2.0).contains(&gap),
            "the light is a jump away: {gap}"
        );
        for s in &stones {
            assert!(
                s.top.xz().distance(hell_at()) + s.radius < pit_radius(),
                "inside the walls"
            );
            assert!(!burnt(s.top), "standing on a stone is safe");
            assert!(!escaped(s.top), "only the light gets you out");
        }
        assert!(escaped(exit));
        assert!(burnt(hell_start().with_y(lava_y())), "the lava burns");
    }

    #[test]
    fn hell_is_far_under_everything() {
        assert!(lava_y() + pit_height() < super::super::underground::TUNNEL_Y - 50.0);
        assert!(hell_at().x.abs() + pit_radius() < crate::camera::FLOOR_HALF);
    }

    #[test]
    fn the_hud_tells_the_dead_and_the_living() {
        let dead = [(
            1,
            Dead::Heaven {
                secs_left: 42,
                pleas: 3,
            },
        )];
        let mine = hud_lines(1, &dead, None);
        assert!(mine[0].contains("42 s") && mine[0].contains("3/12"));
        let theirs = hud_lines(0, &dead, Some(0));
        assert!(theirs[0].contains("beg at their grave"));
        assert!(theirs[1].contains("Tome"));
        assert!(hud_lines(0, &[], None).is_empty());
    }

    #[test]
    fn every_fate_has_words() {
        assert!(fate_text(1, Fate::Died, Some(1)).contains("Heaven"));
        assert!(fate_text(1, Fate::Begged { by: 0, line: 200 }, Some(0)).starts_with("you beg"));
        assert!(fate_text(1, Fate::Uncancelled { by: 0 }, Some(1)).contains("you are back"));
        assert!(
            fate_text(1, Fate::Lava, Some(0)).is_empty(),
            "only you hear about your lava"
        );
    }
}
