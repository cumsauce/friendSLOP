//! The abandoned warehouse. From issue #130 by doomy: "at the edge of the
//! commune lies an abandoned warehouse. all players must agree to enter.
//! entering loads the new map which is generated via wfc. it has a monster
//! that will pursue or trick players. the goal in the warehouse is to find
//! valuable soul pieces and retrieve them to jegg who is right outside. make
//! this part of the game scary".
//!
//! - Just south of the commune square stands a dark, rusting warehouse with
//!   a loading door, and Jegg waits beside it.
//! - Click the door to agree to go in. When **every** player in the game has
//!   agreed, the server rolls a seed, everyone is taken inside, and every peer
//!   builds the same floor plan from it with wave function collapse
//!   ([`wfc`]). The inside is its own little map, deep under the meadow.
//! - Somewhere in the dark lie glowing **soul pieces**. Click one to carry it
//!   (one each), go back out through the loading door and click Jegg to hand
//!   it over. Jegg wants [`SOUL_PIECES`]; the count is shared by everyone.
//! - **The monster** prowls the aisles. If it sees or hears you it hunts you
//!   (faster than walking: run). Its trick: it sometimes sits still pretending
//!   to be a soul piece, and springs when you come close. If it reaches you,
//!   you're thrown out of the warehouse and drop what you carried.
//! - Once the expedition is on, anyone can go back in by clicking the door.
//!   It ends when everyone is out (and nobody holds a piece), or when Jegg is
//!   whole; the next one needs everyone to agree again and is a new building.
//! - Scary on purpose (local only): inside, the sun and sky are gone, there's
//!   a black fog, a flashlight, a few flickering lamps, a low drone instead
//!   of the band, the monster's breathing, and a scream when it gets someone.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::features::fps::{WalkInput, Walker};
use crate::features::interact::{SERVER_REACH, deny, within};
use crate::player::{Player, PlayerOwner};

pub mod bathroom;
pub mod sound;
pub(crate) mod visuals;
pub mod wfc;

pub use bathroom::{FartMonster, Gassed};
pub use visuals::{Spooked, spooked};

pub use crate::rules::depths::*;
use wfc::{CELL, ENTRANCE, H, Plan, Rng, W, cell_centre, cell_of};

/// Soul pieces Jegg needs to be whole.
pub const SOUL_PIECES: u8 = 5;
/// The middle of the warehouse's footprint, out in the meadow.
pub use friendslop_world::places::outside_at;
/// Its size (x, z): the warehouse spot's box on the map.
pub fn outside_size() -> Vec2 {
    friendslop_world::places::sizes("warehouse").size
}
/// Its wall height (the spot's `Feature` height).
pub fn outside_height() -> f32 {
    friendslop_world::places::sizes("warehouse").height
}
/// Wall height inside.
pub const INSIDE_WALL: f32 = 4.2;
/// Tuning numbers: `assets/data/places/warehouse.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Monster speeds: prowling, and hunting (walking is 5 m/s; run!).
    pub prowl_speed: f32,
    pub hunt_speed: f32,
    /// It sees you this far down a clear aisle, and hears you this close anyway.
    pub sight: f32,
    pub hearing: f32,
    /// Pretending to be a soul piece, it springs when you come this close.
    pub lure_spring: f32,
    /// It gets you this close.
    pub catch: f32,
    /// It gives up a hunt after not seeing you this long.
    pub lose_after: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/places/warehouse.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The loading door, on the north face (towards the commune).
pub fn door_at() -> Vec3 {
    let p = Vec2::new(outside_at().x, outside_at().y - outside_size().y / 2.0);
    Vec3::new(p.x, outside_floor(), p.y)
}

/// Where you come out: just in front of the door.
pub fn out_spot(i: usize) -> Vec3 {
    let d = door_at();
    let x = d.x + (i % 4) as f32 * 1.2 - 1.8;
    let z = d.z - 2.6 - (i / 4 % 3) as f32 * 1.2;
    Vec3::new(x, crate::terrain::height_at(x, z), z)
}

/// Jegg waits right outside, beside the door.
pub fn jegg_at() -> Vec3 {
    let d = door_at();
    let (x, z) = (d.x + 4.2, d.z - 1.6);
    Vec3::new(x, crate::terrain::height_at(x, z), z)
}

/// The floor height of the building: the highest ground under it, so it sits
/// on a plinth rather than floating.
pub fn outside_floor() -> f32 {
    let h = outside_size() / 2.0;
    let mut top = f32::MIN;
    for i in -2..=2 {
        for j in -2..=2 {
            let p = outside_at() + Vec2::new(h.x * i as f32 / 2.0, h.y * j as f32 / 2.0);
            top = top.max(crate::terrain::height_at(p.x, p.y));
        }
    }
    top
}

/// Does the warehouse (plus `margin`) cover this meadow spot? Scenery keeps out.
pub fn covers(p: Vec2, margin: f32) -> bool {
    let h = outside_size() / 2.0 + Vec2::splat(margin + 2.0);
    let d = (p - outside_at()).abs();
    let lot = d.x < h.x && d.y < h.y;
    // Keep the yard in front of the door clear too.
    let yard = (p - Vec2::new(outside_at().x, outside_at().y - h.y - 3.0)).length() < 6.0 + margin;
    lot || yard
}

/// World position of an interior cell's middle (on the floor).
pub fn cell_world(x: usize, z: usize) -> Vec3 {
    let c = cell_centre(x, z);
    Vec3::new(c.x, INSIDE_Y, c.y)
}

/// The inside of the loading door (where you click to leave).
pub fn exit_at() -> Vec3 {
    cell_world(ENTRANCE.0, ENTRANCE.1) - Vec3::Z * (CELL / 2.0)
}

/// Where you arrive inside.
pub fn in_spot(i: usize) -> Vec3 {
    let c = cell_world(ENTRANCE.0, ENTRANCE.1);
    c + Vec3::new(
        (i % 3) as f32 * 1.1 - 1.1,
        0.0,
        -0.6 + (i / 3 % 3) as f32 * 0.9,
    )
}

/// The warehouse's shared state. One entity, replicated.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Warehouse {
    /// Players who have agreed to go in.
    pub votes: Vec<u64>,
    /// The current expedition's floor-plan seed, while one is on.
    pub seed: Option<u32>,
    /// Soul pieces Jegg has (out of [`SOUL_PIECES`]).
    pub delivered: u8,
}

/// A soul piece, lying in the dark or carried. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SoulPiece {
    pub id: u8,
    pub pos: Vec3,
    pub carrier: Option<u64>,
}

/// What the monster is up to.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonsterMode {
    /// Wandering the aisles.
    Prowl,
    /// After this player.
    Hunt(u64),
    /// Sitting still, looking exactly like a soul piece.
    Lure,
    /// Just got someone; slinking off.
    Sated,
}

/// The thing in the warehouse. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Monster {
    pub pos: Vec3,
    pub yaw: f32,
    pub mode: MonsterMode,
}

/// Client -> server: (outside) I agree to go in / I take it back; or, once
/// it's on, let me in.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct KnockDoor;

/// Client -> server: (inside) let me out.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct LeaveWarehouse;

/// Client -> server: I pick up soul piece `id`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TakeSoul {
    pub id: u8,
}

/// Client -> server: Jegg, take this.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct GiveToJegg;

/// Server -> everyone: the monster got `player` at `at`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Caught {
    pub player: u64,
    pub at: Vec3,
}

/// Server -> everyone: `player` gave Jegg a piece; Jegg now has `total`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SoulDelivered {
    pub player: u64,
    pub total: u8,
}

/// Server only: where a piece was found, to put it back if its carrier leaves.
#[derive(Component, Debug)]
struct SoulHome(Vec3);

/// Server only: the monster's brain.
#[derive(Component, Debug)]
struct MonsterBrain {
    plan: Plan,
    rng: Rng,
    goal: (usize, usize),
    /// Seconds left in the current mood (lure, sated).
    timer: f32,
    /// Seconds since it last saw its prey.
    lost: f32,
    /// How fast it hunts (Marmalade's wheel moves it, #173).
    hunt_speed: f32,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Warehouse>()
        .replicate::<SoulPiece>()
        .replicate::<Monster>()
        .add_client_event::<KnockDoor>(Channel::Ordered)
        .add_client_event::<LeaveWarehouse>(Channel::Ordered)
        .add_client_event::<TakeSoul>(Channel::Ordered)
        .add_client_event::<GiveToJegg>(Channel::Ordered)
        .add_server_event::<Caught>(Channel::Ordered)
        .add_server_event::<SoulDelivered>(Channel::Ordered)
        .add_systems(OnEnter(ServerState::Running), spawn_warehouse)
        .add_systems(
            Update,
            (
                tend_votes,
                carry_pieces,
                hunt,
                bathroom::patrol,
                end_when_empty,
            )
                .chain()
                .run_if(in_state(ServerState::Running)),
        )
        .add_observer(knock)
        .add_observer(leave)
        .add_observer(take_soul)
        .add_observer(give_to_jegg)
        .add_observer(reset_round);
    bathroom::protocol(app);
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, music_signals.in_set(crate::music::MusicSignalSet));
    visuals::plugin(app);
}

fn spawn_warehouse(mut commands: Commands) {
    commands.spawn((Warehouse::default(), Replicated));
}

/// Both monsters (the warehouse's and the bathroom's fart monster).
type Monsters<'w, 's> = Query<'w, 's, Entity, Or<(With<Monster>, With<FartMonster>)>>;

type Bodies<'w, 's> = Query<
    'w,
    's,
    (
        &'static Player,
        &'static PlayerOwner,
        &'static mut Walker,
        Option<&'static mut WalkInput>,
        Option<&'static mut crate::kcc::Kcc>,
    ),
>;

/// Server: move a body somewhere else entirely (in or out of the warehouse).
fn put(
    walker: &mut Walker,
    input: Option<Mut<WalkInput>>,
    kcc: Option<Mut<crate::kcc::Kcc>>,
    to: Vec3,
) {
    walker.pos = to;
    if let Some(mut input) = input {
        input.stick = Vec2::ZERO;
        input.jump = false;
    }
    if let Some(mut kcc) = kcc {
        kcc.velocity = Vec3::ZERO;
    }
}

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    }
}

/// Server, tests and bug repros: when present, every expedition uses this
/// floor-plan seed instead of a fresh one.
#[derive(Resource, Clone, Copy, Debug)]
pub struct FixedPlan(pub u32);

/// Server: someone clicked the loading door from outside.
fn knock(
    ev: On<FromClient<KnockDoor>>,
    mut commands: Commands,
    time: Res<Time>,
    fixed: Option<Res<FixedPlan>>,
    still: Query<&super::stillness::Stillness>,
    mut warehouse: Query<&mut Warehouse>,
    mut bodies: Bodies,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let Ok(mut wh) = warehouse.single_mut() else {
        return;
    };
    let owner = owner_of(ev.client_id);
    let Some(me) = bodies
        .iter()
        .find(|b| b.1.0 == owner)
        .map(|b| (b.0.id, b.2.pos))
    else {
        return;
    };
    if inside(me.1) || !within(me.1, door_at(), SERVER_REACH + 1.5) {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    if wh.delivered >= SOUL_PIECES {
        deny(
            &mut commands,
            ev.client_id,
            "Jegg is whole again. Nothing calls from the warehouse now.",
        );
        return;
    }
    if wh.seed.is_some() {
        // They're already in there: follow them.
        if let Some((_, _, mut w, input, kcc)) = bodies.iter_mut().find(|b| b.1.0 == owner) {
            put(&mut w, input, kcc, in_spot(me.0 as usize));
        }
        return;
    }
    if let Some(i) = wh.votes.iter().position(|&v| v == me.0) {
        wh.votes.remove(i);
        return;
    }
    wh.votes.push(me.0);
    let everyone: Vec<u64> = bodies.iter().map(|b| b.0.id).collect();
    if !everyone.iter().all(|id| wh.votes.contains(id)) {
        return;
    }
    // Everyone agrees: in we go, into a building nobody has seen before.
    let seed = fixed.map_or(
        (time.elapsed_secs_f64() * 1000.0) as u32 ^ 0x5017_0130,
        |f| f.0,
    );
    start_expedition(&mut commands, &mut wh, seed);
    for (i, (_, _, mut w, input, kcc)) in bodies.iter_mut().enumerate() {
        put(&mut w, input, kcc, in_spot(i));
    }
}

/// Server: open the warehouse with floor plan `seed`: soul pieces in its
/// farthest corners and the monster in the very farthest.
pub fn start_expedition(commands: &mut Commands, wh: &mut Warehouse, seed: u32) {
    wh.votes.clear();
    wh.seed = Some(seed);
    let plan = Plan::generate(seed);
    let dist = plan.distances(ENTRANCE);
    let mut cells = plan.reachable();
    let mut rng = Rng::new(seed ^ 0x5011);
    // Farthest first, shuffled a little among similar distances.
    let mut keyed: Vec<(f32, (usize, usize))> = cells
        .drain(..)
        .map(|(x, z)| (dist[z][x] as f32 + rng.roll() * 2.5, (x, z)))
        .collect();
    keyed.sort_by(|a, b| b.0.total_cmp(&a.0));
    let far: Vec<(usize, usize)> = keyed.into_iter().map(|k| k.1).collect();
    let Some(&lair) = far.first() else { return };
    let wanted = SOUL_PIECES.saturating_sub(wh.delivered) as usize;
    for (i, &(x, z)) in far
        .iter()
        .skip(1)
        .filter(|&&c| c != ENTRANCE)
        .take(wanted)
        .enumerate()
    {
        let off = Vec3::new(rng.roll() - 0.5, 0.0, rng.roll() - 0.5) * 2.0;
        let pos = cell_world(x, z) + off + Vec3::Y * 0.8;
        commands.spawn((
            SoulPiece {
                id: i as u8,
                pos,
                carrier: None,
            },
            SoulHome(pos),
            Replicated,
        ));
    }
    bathroom::spawn(commands, seed, &plan);
    commands.spawn((
        Monster {
            pos: cell_world(lair.0, lair.1),
            yaw: 0.0,
            mode: MonsterMode::Prowl,
        },
        MonsterBrain {
            plan,
            rng,
            goal: lair,
            timer: 0.0,
            lost: 0.0,
            hunt_speed: tuning().hunt_speed,
        },
        Replicated,
    ));
}

/// Server: someone clicked the loading door from inside.
fn leave(
    ev: On<FromClient<LeaveWarehouse>>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    mut bodies: Bodies,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let owner = owner_of(ev.client_id);
    let Some((p, _, mut w, input, kcc)) = bodies.iter_mut().find(|b| b.1.0 == owner) else {
        return;
    };
    if !inside(w.pos) || !within(w.pos, exit_at(), SERVER_REACH + 1.0) {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    let to = out_spot(p.id as usize);
    put(&mut w, input, kcc, to);
}

/// Server: pick up a soul piece.
fn take_soul(
    ev: On<FromClient<TakeSoul>>,
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    bodies: Query<(&Player, &PlayerOwner, &Walker)>,
    mut pieces: Query<&mut SoulPiece>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let owner = owner_of(ev.client_id);
    let Some((p, _, w)) = bodies.iter().find(|b| b.1.0 == owner) else {
        return;
    };
    if pieces.iter().any(|s| s.carrier == Some(p.id)) {
        deny(
            &mut commands,
            ev.client_id,
            "you can only carry one soul piece",
        );
        return;
    }
    let Some(mut piece) = pieces.iter_mut().find(|s| s.id == ev.id) else {
        return;
    };
    if piece.carrier.is_some() {
        deny(&mut commands, ev.client_id, "someone already has that one");
        return;
    }
    if !inside(w.pos) || !within(w.pos, piece.pos, SERVER_REACH) {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    piece.carrier = Some(p.id);
}

/// What Jegg says when you come empty-handed.
pub const JEGG_PLEAS: [&str; 4] = [
    "Jegg: \"They're in there... the pieces of me. Please.\"",
    "Jegg: \"I can hear them glowing. Bring them out to me.\"",
    "Jegg: \"Don't trust everything that shines in there.\"",
    "Jegg: \"It's still in there. Be quick. Be quiet.\"",
];

/// Server: hand Jegg a piece.
#[allow(clippy::too_many_arguments)]
fn give_to_jegg(
    ev: On<FromClient<GiveToJegg>>,
    mut commands: Commands,
    time: Res<Time>,
    still: Query<&super::stillness::Stillness>,
    mut warehouse: Query<&mut Warehouse>,
    pieces: Query<(Entity, &SoulPiece)>,
    mut bodies: Bodies,
    monsters: Monsters,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let Ok(mut wh) = warehouse.single_mut() else {
        return;
    };
    let owner = owner_of(ev.client_id);
    let Some((id, pos)) = bodies
        .iter()
        .find(|b| b.1.0 == owner)
        .map(|b| (b.0.id, b.2.pos))
    else {
        return;
    };
    if inside(pos) || !within(pos, jegg_at(), SERVER_REACH) {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    let Some((piece, _)) = pieces.iter().find(|(_, s)| s.carrier == Some(id)) else {
        let line = (time.elapsed_secs() as usize / 4) % JEGG_PLEAS.len();
        deny(&mut commands, ev.client_id, JEGG_PLEAS[line]);
        return;
    };
    commands.entity(piece).despawn();
    wh.delivered = (wh.delivered + 1).min(SOUL_PIECES);
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: SoulDelivered {
            player: id,
            total: wh.delivered,
        },
    });
    if wh.delivered >= SOUL_PIECES {
        // Whole again: the warehouse lets everyone go.
        end_expedition(&mut commands, &mut wh, &pieces, &monsters, &mut bodies);
    }
}

/// Server: close the warehouse and send anyone still in there out.
fn end_expedition(
    commands: &mut Commands,
    wh: &mut Warehouse,
    pieces: &Query<(Entity, &SoulPiece)>,
    monsters: &Monsters,
    bodies: &mut Bodies,
) {
    wh.seed = None;
    wh.votes.clear();
    for (e, _) in pieces {
        commands.entity(e).despawn();
    }
    for e in monsters {
        commands.entity(e).despawn();
    }
    for (p, _, mut w, input, kcc) in bodies.iter_mut() {
        if inside(w.pos) {
            put(&mut w, input, kcc, out_spot(p.id as usize));
        }
    }
}

/// Server: a new round (#75) starts with Jegg in pieces and the doors shut.
fn reset_round(
    _reset: On<crate::rules::campaign::RoundReset>,
    mut commands: Commands,
    mut warehouse: Query<&mut Warehouse>,
    pieces: Query<(Entity, &SoulPiece)>,
    monsters: Monsters,
    mut bodies: Bodies,
) {
    let Ok(mut wh) = warehouse.single_mut() else {
        return;
    };
    end_expedition(&mut commands, &mut wh, &pieces, &monsters, &mut bodies);
    wh.delivered = 0;
}

/// Server: votes only count from players who are still here, at the door.
fn tend_votes(mut warehouse: Query<&mut Warehouse>, bodies: Query<(&Player, &Walker)>) {
    let Ok(mut wh) = warehouse.single_mut() else {
        return;
    };
    let keep = |id: &u64| {
        bodies
            .iter()
            .any(|(p, w)| p.id == *id && !inside(w.pos) && within(w.pos, door_at(), 20.0))
    };
    if !wh.votes.iter().all(keep) {
        wh.votes.retain(keep);
    }
}

/// Server: carried pieces go where their carrier goes; a carrier who left the
/// game leaves theirs back where it was found.
fn carry_pieces(mut pieces: Query<(&mut SoulPiece, &SoulHome)>, bodies: Query<(&Player, &Walker)>) {
    for (mut piece, home) in &mut pieces {
        let Some(carrier) = piece.carrier else {
            continue;
        };
        let at = bodies
            .iter()
            .find(|(p, _)| p.id == carrier)
            .map(|(_, w)| w.pos + Vec3::Y * 1.0);
        match at {
            Some(at) if piece.pos.distance(at) > 0.05 => piece.pos = at,
            Some(_) => {}
            None => {
                piece.carrier = None;
                piece.pos = home.0;
            }
        }
    }
}

/// Server: close up once nobody is inside and nobody holds a piece.
fn end_when_empty(
    mut commands: Commands,
    mut warehouse: Query<&mut Warehouse>,
    pieces: Query<(Entity, &SoulPiece)>,
    monsters: Monsters,
    mut bodies: Bodies,
) {
    let Ok(wh) = warehouse.single() else { return };
    if wh.seed.is_none()
        || bodies.iter().any(|b| inside(b.2.pos))
        || pieces.iter().any(|(_, s)| s.carrier.is_some())
    {
        return;
    }
    let Ok(mut wh) = warehouse.single_mut() else {
        return;
    };
    end_expedition(&mut commands, &mut wh, &pieces, &monsters, &mut bodies);
}

/// Floor-plane offset inside the grid.
fn local(p: Vec3) -> Vec2 {
    p.xz()
}

/// Server: the monster thinks and moves.
#[allow(clippy::type_complexity)]
fn hunt(
    mut commands: Commands,
    time: Res<Time>,
    still: Query<&super::stillness::Stillness>,
    mut monsters: Query<(&mut Monster, &mut MonsterBrain)>,
    mut pieces: Query<&mut SoulPiece>,
    mut bodies: Bodies,
    odds: Query<&crate::rules::house_odds::HouseOdds>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let hunt_speed = crate::rules::house_odds::odds(&odds).hunt_speed();
    let dt = time.delta_secs().min(0.1);
    let prey: Vec<(u64, Vec2)> = bodies
        .iter()
        .filter(|b| inside(b.2.pos))
        .map(|b| (b.0.id, local(b.2.pos)))
        .collect();
    for (mut monster, mut brain) in &mut monsters {
        let here = local(monster.pos);
        brain.hunt_speed = hunt_speed;
        let step = think(&mut brain, monster.mode, here, &prey, dt);
        if monster.mode != step.mode {
            monster.mode = step.mode;
        }
        if step.pos != here {
            monster.pos = Vec3::new(step.pos.x, INSIDE_Y, step.pos.y);
            let d = step.pos - here;
            monster.yaw = (-d.x).atan2(-d.y);
        }
        let Some(victim) = step.caught else { continue };
        let at = monster.pos;
        throw_out(victim, at, &mut pieces, &mut bodies);
        commands.server_trigger(ToClients {
            targets: SendTargets::All,
            message: Caught { player: victim, at },
        });
    }
}

/// Server: a monster got `victim` at `at`. Out they go, and what they carried
/// falls where they stood.
fn throw_out(victim: u64, at: Vec3, pieces: &mut Query<&mut SoulPiece>, bodies: &mut Bodies) {
    for mut piece in pieces.iter_mut() {
        if piece.carrier == Some(victim) {
            piece.carrier = None;
            piece.pos = at + Vec3::Y * 0.8;
        }
    }
    if let Some((p, _, mut w, input, kcc)) = bodies.iter_mut().find(|b| b.0.id == victim) {
        let to = out_spot(p.id as usize);
        put(&mut w, input, kcc, to);
    }
}

/// One tick of the monster's mind.
#[derive(Debug, PartialEq)]
struct Thought {
    mode: MonsterMode,
    pos: Vec2,
    caught: Option<u64>,
}

/// The monster's brain, pure: from its mode, where it is and where everyone
/// inside is, decide what it does this tick.
fn think(
    brain: &mut MonsterBrain,
    mode: MonsterMode,
    here: Vec2,
    prey: &[(u64, Vec2)],
    dt: f32,
) -> Thought {
    let nearest = prey
        .iter()
        .copied()
        .min_by(|a, b| a.1.distance(here).total_cmp(&b.1.distance(here)));
    let noticed = prey
        .iter()
        .copied()
        .filter(|(_, p)| {
            let d = p.distance(here);
            d < tuning().hearing || (d < tuning().sight && brain.plan.sees(here, *p))
        })
        .min_by(|a, b| a.1.distance(here).total_cmp(&b.1.distance(here)));
    let idle = |pos| Thought {
        mode,
        pos,
        caught: None,
    };
    match mode {
        MonsterMode::Lure => {
            brain.timer -= dt;
            if let Some((id, _)) = nearest.filter(|(_, p)| p.distance(here) < tuning().lure_spring)
            {
                brain.lost = 0.0;
                return Thought {
                    mode: MonsterMode::Hunt(id),
                    pos: here,
                    caught: None,
                };
            }
            if brain.timer <= 0.0 {
                return Thought {
                    mode: MonsterMode::Prowl,
                    pos: here,
                    caught: None,
                };
            }
            idle(here)
        }
        MonsterMode::Sated => {
            brain.timer -= dt;
            let pos = wander(brain, here, dt);
            if brain.timer <= 0.0 {
                return Thought {
                    mode: MonsterMode::Prowl,
                    pos,
                    caught: None,
                };
            }
            idle(pos)
        }
        MonsterMode::Prowl => {
            if let Some((id, _)) = noticed {
                brain.lost = 0.0;
                return Thought {
                    mode: MonsterMode::Hunt(id),
                    pos: here,
                    caught: None,
                };
            }
            let before = brain.goal;
            let pos = wander(brain, here, dt);
            // Arrived somewhere quiet: sometimes it sits and pretends.
            let quiet = nearest.is_none_or(|(_, p)| p.distance(here) > 10.0);
            if brain.goal != before && quiet && brain.rng.roll() < 0.3 {
                brain.timer = 18.0 + brain.rng.roll() * 12.0;
                return Thought {
                    mode: MonsterMode::Lure,
                    pos,
                    caught: None,
                };
            }
            idle(pos)
        }
        MonsterMode::Hunt(id) => {
            let Some(&(_, target)) = prey.iter().find(|(p, _)| *p == id) else {
                return Thought {
                    mode: MonsterMode::Prowl,
                    pos: here,
                    caught: None,
                };
            };
            let sees = brain.plan.sees(here, target) || target.distance(here) < tuning().hearing;
            brain.lost = if sees { 0.0 } else { brain.lost + dt };
            if brain.lost > tuning().lose_after {
                return Thought {
                    mode: MonsterMode::Prowl,
                    pos: here,
                    caught: None,
                };
            }
            let towards = if sees {
                target
            } else {
                next_waypoint(&brain.plan, here, target)
            };
            let pos = move_to(here, towards, brain.hunt_speed * dt);
            if pos.distance(target) < tuning().catch {
                brain.timer = 6.0;
                brain.goal = far_cell(brain, pos);
                return Thought {
                    mode: MonsterMode::Sated,
                    pos,
                    caught: Some(id),
                };
            }
            idle(pos)
        }
    }
}

fn move_to(from: Vec2, to: Vec2, max: f32) -> Vec2 {
    let d = to - from;
    let len = d.length();
    if len <= max || len < 1e-5 {
        to
    } else {
        from + d / len * max
    }
}

/// The middle of the next cell on the way from `here` to `target`.
fn next_waypoint(plan: &Plan, here: Vec2, target: Vec2) -> Vec2 {
    let (Some(a), Some(b)) = (cell_of(here), cell_of(target)) else {
        return target;
    };
    match plan.step_towards(a, b) {
        Some(c) if c != a => cell_centre(c.0, c.1),
        _ => target,
    }
}

/// A reachable cell well away from `here`.
fn far_cell(brain: &mut MonsterBrain, here: Vec2) -> (usize, usize) {
    let from = cell_of(here).unwrap_or(ENTRANCE);
    let d = brain.plan.distances(from);
    let mut best = from;
    let mut score = -1.0;
    for (z, row) in d.iter().enumerate() {
        for (x, &v) in row.iter().enumerate() {
            if v == u8::MAX {
                continue;
            }
            let s = v as f32 + brain.rng.roll() * 2.0;
            if s > score {
                score = s;
                best = (x, z);
            }
        }
    }
    best
}

/// Prowl from cell to cell towards the goal; pick a new goal on arrival.
fn wander(brain: &mut MonsterBrain, here: Vec2, dt: f32) -> Vec2 {
    let goal = cell_centre(brain.goal.0, brain.goal.1);
    if here.distance(goal) < 0.2 {
        let cells = brain.plan.distances(brain.goal);
        let options: Vec<(usize, usize)> = (0..H)
            .flat_map(|z| (0..W).map(move |x| (x, z)))
            .filter(|&(x, z)| cells[z][x] != u8::MAX && (x, z) != brain.goal)
            .collect();
        if !options.is_empty() {
            brain.goal = options[brain.rng.below(options.len())];
        }
        return here;
    }
    let towards = next_waypoint(&brain.plan, here, goal);
    move_to(here, towards, tuning().prowl_speed * dt)
}

/// For the music: I'm in the dark of the warehouse (the band goes quiet).
fn music_signals(
    spooked: Option<Res<visuals::Spooked>>,
    signals: Option<ResMut<crate::music::MusicSignals>>,
) {
    let Some(mut signals) = signals else { return };
    signals.flag("spooked", visuals::spooked(&spooked));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brain() -> MonsterBrain {
        MonsterBrain {
            plan: Plan::fallback(),
            rng: Rng::new(7),
            goal: (0, 0),
            timer: 0.0,
            lost: 0.0,
            hunt_speed: tuning().hunt_speed,
        }
    }

    #[test]
    fn it_hunts_whoever_it_sees_and_catches_them() {
        let mut b = brain();
        let here = cell_centre(3, 3);
        let you = here + Vec2::new(6.0, 0.0);
        let t = think(&mut b, MonsterMode::Prowl, here, &[(4, you)], 0.1);
        assert_eq!(t.mode, MonsterMode::Hunt(4));
        let mut pos = here;
        let mut caught = None;
        for _ in 0..40 {
            let t = think(&mut b, MonsterMode::Hunt(4), pos, &[(4, you)], 0.1);
            pos = t.pos;
            if t.caught.is_some() {
                caught = t.caught;
                assert_eq!(t.mode, MonsterMode::Sated);
                break;
            }
        }
        assert_eq!(caught, Some(4));
    }

    #[test]
    fn the_lure_springs_up_close() {
        let mut b = brain();
        b.timer = 20.0;
        let here = cell_centre(1, 1);
        let far = think(
            &mut b,
            MonsterMode::Lure,
            here,
            &[(2, here + Vec2::X * 8.0)],
            0.1,
        );
        assert_eq!(far.mode, MonsterMode::Lure);
        assert_eq!(far.pos, here, "a lure keeps still");
        let near = think(
            &mut b,
            MonsterMode::Lure,
            here,
            &[(2, here + Vec2::X * 2.0)],
            0.1,
        );
        assert_eq!(near.mode, MonsterMode::Hunt(2));
    }

    #[test]
    fn walls_hide_you() {
        let mut b = brain();
        // Wall off cell (3,3) from (4,3).
        b.plan.cells[3][3] &= !wfc::E;
        b.plan.cells[3][4] &= !wfc::WEST;
        let here = cell_centre(3, 3);
        let you = cell_centre(4, 3) + Vec2::X;
        let t = think(&mut b, MonsterMode::Prowl, here, &[(1, you)], 0.1);
        assert_eq!(t.mode, MonsterMode::Prowl);
    }

    #[test]
    fn landmarks_are_where_they_should_be() {
        assert!(!inside(door_at()));
        assert!(inside(in_spot(0)));
        assert!(inside(exit_at()));
        assert!(covers(door_at().xz(), 0.0));
        assert!(!covers(Vec2::ZERO, 0.0));
        assert!(jegg_at().xz().distance(out_spot(0).xz()) < 8.0);
        assert!(within(in_spot(0), exit_at(), SERVER_REACH + 1.0));
    }
}
