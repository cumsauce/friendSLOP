//! Commune villagers to hide among. From issue #61 by doomy ("environment
//! generated and npcs and then hide and seek with players").
//!
//! The server spawns [`COUNT`] replicated `Villager`s that look like player
//! bodies (same capsule, same colour scheme, googly eyes). Their brain runs on
//! the server at [`AI_HZ`]: wander between points of interest (bonfire, ruins,
//! pond, cover spots), pause or sit for a bit, never step into the duel arena.
//! Only the position (updated at the AI rate) is replicated; every peer glides
//! the body there.
//!
//! Clicking a villager outside hide and seek just gets a hello. A seeker who
//! clicks one during the seeking phase is told "that's just Kevin from the
//! commune" and is frozen for [`PENALTY_SECS`]. Now and then a villager near
//! you mutters something (HUD toast).

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::duel::{ARENA_CENTER, ARENA_HALF};
use crate::features::fps::{WalkSet, Walker};
use crate::features::hide_and_seek::{HideSeek, HideSeekNews, Phase, Role};
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

pub const COUNT: usize = 10;
/// AI decisions (and replicated moves) per second.
pub const AI_HZ: f32 = 5.0;
pub const PENALTY_SECS: f32 = 3.0;
const SPEED: f32 = 1.3;
/// How far outside the arena walls villagers stay.
const ARENA_MARGIN: f32 = 1.5;

pub const NAMES: [&str; COUNT] = [
    "Kevin", "Brenda", "Old Tom", "Moss", "Pip", "Gwen", "Barnaby", "Juniper", "Dot", "Hank",
];

/// Where villagers like to hang out (x, z).
const POINTS: [Vec2; 12] = [
    Vec2::new(-8.5, 0.5),   // by the bonfire
    Vec2::new(-6.5, -1.0),  // other side of the bonfire
    Vec2::new(-12.0, -1.0), // under the ruins
    Vec2::new(-16.5, -4.0), // ruins' west end
    Vec2::new(9.0, 9.0),    // the pond
    Vec2::new(13.0, 13.0),  // far side of the pond
    Vec2::new(0.0, 2.0),    // the middle
    Vec2::new(-4.0, 4.0),
    Vec2::new(3.0, 8.0),
    Vec2::new(-6.0, 11.0),
    Vec2::new(8.0, 3.0),
    Vec2::new(15.0, -3.0),
];

/// Server -> everyone: one villager. `pos` is the feet position on the floor.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Villager {
    pub id: u8,
    pub pos: Vec2,
    pub yaw: f32,
    pub sitting: bool,
}

impl Villager {
    pub fn name(&self) -> &'static str {
        NAMES[self.id as usize % COUNT]
    }
}

/// Server only: what a villager is up to.
#[derive(Component, Debug)]
struct Brain {
    target: Vec2,
    wait: f32,
}

/// Server only: a seeker who mistook a villager for a hider stands here until `until`.
#[derive(Component, Debug)]
pub struct Fooled {
    pub until: f32,
    pub at: Vec3,
}

/// Client -> server: I clicked villager `id`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct PokeVillager {
    pub id: u8,
}

/// Server only: villager dice.
#[derive(Resource)]
struct VillageRng(u64);

impl VillageRng {
    fn f(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<Villager>()
        .add_client_event::<PokeVillager>(Channel::Ordered)
        .insert_resource(VillageRng(0x5EED_CAFE_F00D_1234))
        .add_observer(on_poke)
        .add_systems(OnEnter(ServerState::Running), spawn_villagers)
        .add_systems(
            Update,
            (think, hold_fooled.after(WalkSet)).run_if(in_state(ServerState::Running)),
        );
}

/// Push a point out of the duel arena (plus margin), if it's inside.
pub fn outside_arena(p: Vec2) -> Vec2 {
    let c = ARENA_CENTER.xz();
    let half = ARENA_HALF + ARENA_MARGIN;
    let d = p - c;
    if d.x.abs() >= half || d.y.abs() >= half {
        return p;
    }
    // Out through the nearest wall.
    let (px, py) = (half - d.x.abs(), half - d.y.abs());
    if px < py {
        Vec2::new(c.x + half * d.x.signum(), p.y)
    } else {
        Vec2::new(p.x, c.y + half * d.y.signum())
    }
}

fn spawn_villagers(mut commands: Commands, mut rng: ResMut<VillageRng>) {
    for i in 0..COUNT {
        let home = POINTS[i % POINTS.len()];
        let pos = outside_arena(home + Vec2::new(rng.f() - 0.5, rng.f() - 0.5) * 2.0);
        commands.spawn((
            Villager {
                id: i as u8,
                pos,
                yaw: rng.f() * std::f32::consts::TAU,
                sitting: false,
            },
            Brain {
                target: pos,
                wait: rng.f() * 4.0,
            },
            Replicated,
        ));
    }
}

/// Server: villager brains, ticked at `AI_HZ`.
fn think(
    time: Res<Time>,
    mut acc: Local<f32>,
    mut rng: ResMut<VillageRng>,
    still: Query<&super::stillness::Stillness>,
    mut villagers: Query<(&mut Villager, &mut Brain)>,
) {
    *acc += time.delta_secs();
    let dt = 1.0 / AI_HZ;
    if *acc < dt {
        return;
    }
    *acc = (*acc - dt).min(dt);
    if super::stillness::is_frozen(&still) {
        return; // villagers respect the stillness too
    }
    for (mut v, mut brain) in &mut villagers {
        if brain.wait > 0.0 {
            brain.wait -= dt;
            if brain.wait <= 0.0 {
                // Up and off somewhere new.
                let p = POINTS[(rng.f() * POINTS.len() as f32) as usize % POINTS.len()];
                brain.target = outside_arena(p + Vec2::new(rng.f() - 0.5, rng.f() - 0.5) * 3.0);
                if v.sitting {
                    v.sitting = false;
                }
            }
            continue;
        }
        let to = brain.target - v.pos;
        let step = SPEED * dt;
        if to.length() <= step {
            v.pos = brain.target;
            brain.wait = 2.0 + rng.f() * 8.0;
            v.sitting = rng.f() < 0.35;
            continue;
        }
        let dir = to / to.length();
        v.pos = outside_arena(v.pos + dir * step);
        v.yaw = (-dir.x).atan2(-dir.y);
    }
}

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    }
}

fn on_poke(
    ev: On<FromClient<PokeVillager>>,
    mut commands: Commands,
    time: Res<Time>,
    state: Query<&HideSeek>,
    villagers: Query<&Villager>,
    walkers: Query<(&PlayerOwner, &Walker)>,
    players: Query<(
        Entity,
        &Player,
        &PlayerOwner,
        &Walker,
        Option<&Role>,
        Has<Fooled>,
    )>,
) {
    let Some(v) = villagers.iter().find(|v| v.id == ev.id) else {
        return;
    };
    let seeking = state.single().is_ok_and(|s| s.phase == Phase::Seeking);
    let owner = owner_of(ev.client_id);
    let Some((me, player, _, walker, role, fooled)) =
        players.iter().find(|(_, _, o, ..)| o.0 == owner)
    else {
        return;
    };
    if !seeking || !role.is_some_and(|r| r.seeks()) || fooled {
        return; // just a friendly poke; the client already said hello
    }
    let at = Vec3::new(v.pos.x, 0.0, v.pos.y);
    if !in_reach(&mut commands, ev.client_id, &walkers, at) {
        return;
    }
    commands.entity(me).insert(Fooled {
        until: time.elapsed_secs() + PENALTY_SECS,
        at: walker.pos,
    });
    deny(
        &mut commands,
        ev.client_id,
        format!(
            "that's just {} from the commune. you stand there, embarrassed ({}s)",
            v.name(),
            PENALTY_SECS as u32
        ),
    );
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: HideSeekNews {
            text: format!(
                "player {} tried to tag {}. {} is confused.",
                player.id,
                v.name(),
                v.name()
            ),
        },
    });
}

/// Server: fooled seekers can't move for a moment.
fn hold_fooled(
    mut commands: Commands,
    time: Res<Time>,
    mut fooled: Query<(Entity, &mut Walker, &Fooled)>,
) {
    let now = time.elapsed_secs();
    for (e, mut w, f) in &mut fooled {
        if now >= f.until {
            commands.entity(e).remove::<Fooled>();
        } else if w.pos != f.at {
            w.pos = f.at;
        }
    }
}

// ---------------------------------------------------------------- local side

pub fn plugin(app: &mut App) {
    app.add_observer(click_villager).add_systems(
        Update,
        (dress_villagers, move_villagers, mutter).run_if(in_state(AppState::InGame)),
    );
}

#[derive(Component)]
struct Dressed;

/// The body part that squashes when sitting and bobs when walking.
#[derive(Component)]
struct VillagerBody;

/// Same body as a player (fps.rs): capsule r 0.35, length 1.0, centre 0.85 up,
/// coloured like player ids so hiders blend in.
fn dress_villagers(
    mut commands: Commands,
    new: Query<(Entity, &Villager), Without<Dressed>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut shared: Local<Option<(Handle<Mesh>, Vec<Handle<StandardMaterial>>)>>,
) {
    if new.is_empty() {
        return;
    }
    let (capsule, skins) = shared.get_or_insert_with(|| {
        let capsule = meshes.add(Capsule3d::new(0.35, 1.0));
        let skins = (0..8)
            .map(|i| {
                let hue = (i as f32 * 137.5) % 360.0;
                materials.add(Color::hsl(hue, 0.65, 0.6))
            })
            .collect();
        (capsule, skins)
    });
    let (capsule, skins) = (capsule.clone(), skins.clone());
    for (entity, v) in &new {
        let skin = skins[v.id as usize * 3 % skins.len()].clone();
        commands
            .entity(entity)
            .insert((
                Dressed,
                Transform::from_xyz(v.pos.x, 0.0, v.pos.y)
                    .with_rotation(Quat::from_rotation_y(v.yaw)),
                Visibility::default(),
                Clickable::at_height(1.0).radius(60.0),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((VillagerBody, Transform::default(), Visibility::default()))
                    .with_children(|b| {
                        b.spawn((
                            Mesh3d(capsule.clone()),
                            MeshMaterial3d(skin),
                            Transform::from_xyz(0.0, 0.85, 0.0),
                        ));
                        crate::art::eyes(
                            b,
                            &mut meshes,
                            &mut materials,
                            Vec3::new(0.0, 1.45, -0.3),
                            0.26,
                            0.09,
                        );
                        crate::art::blush(
                            b,
                            &mut meshes,
                            &mut materials,
                            Vec3::new(0.0, 1.3, -0.31),
                            0.44,
                            0.06,
                        );
                    });
            });
    }
}

/// Glide to the replicated spot; sit by squashing down, bob while walking.
fn move_villagers(
    time: Res<Time>,
    mut villagers: Query<(&Villager, &mut Transform, &Children), With<Dressed>>,
    mut bodies: Query<&mut Transform, (With<VillagerBody>, Without<Dressed>)>,
) {
    let t = 1.0 - (-6.0 * time.delta_secs()).exp();
    let now = time.elapsed_secs();
    for (v, mut tf, children) in &mut villagers {
        let target = Vec3::new(
            v.pos.x,
            crate::terrain::height_at(v.pos.x, v.pos.y),
            v.pos.y,
        );
        let moving = tf.translation.xz().distance(v.pos) > 0.05;
        tf.translation = tf.translation.lerp(target, t);
        tf.rotation = tf.rotation.slerp(Quat::from_rotation_y(v.yaw), t);
        for child in children.iter() {
            let Ok(mut body) = bodies.get_mut(child) else {
                continue;
            };
            let (y, sy) = if v.sitting {
                (-0.3, 0.75)
            } else if moving {
                ((now * 9.0 + v.id as f32).sin().abs() * 0.06, 1.0)
            } else {
                (0.0, 1.0)
            };
            body.translation.y += (y - body.translation.y) * t;
            body.scale.y += (sy - body.scale.y) * t;
        }
    }
}

fn click_villager(
    ev: On<Clicked>,
    mut commands: Commands,
    villagers: Query<&Villager>,
    me: Option<Res<LocalPlayer>>,
    state: Query<&HideSeek>,
    roles: Query<(&Player, &Role)>,
    toast: Option<ResMut<Toast>>,
) {
    let Ok(v) = villagers.get(ev.entity) else {
        return;
    };
    commands.client_trigger(PokeVillager { id: v.id });
    let seeking = state.single().is_ok_and(|s| s.phase == Phase::Seeking)
        && me
            .as_deref()
            .is_some_and(|m| roles.iter().any(|(p, r)| p.id == m.0 && r.seeks()));
    if !seeking && let Some(mut toast) = toast {
        toast.show(format!("{}: {}", v.name(), greeting(v.id as usize)));
    }
}

fn greeting(n: usize) -> &'static str {
    const LINES: [&str; 5] = [
        "oh! hello. lovely day for it.",
        "have you seen my good spoon?",
        "don't tell Dave I'm out here.",
        "I'm on my way to the bonfire. probably.",
        "I am simply standing. like a person.",
    ];
    LINES[n % LINES.len()]
}

const MUTTERS: [&str; 6] = [
    "Kevin: is it soup night?",
    "Brenda: the goat's been looking at me again.",
    "Old Tom: back in my day the ruins were just ruins.",
    "Moss: I think I left the bonfire on.",
    "Kevin: nobody's hiding behind me. nope.",
    "Brenda: the pond has opinions, you know.",
];

/// Now and then, a nearby villager says something under their breath.
fn mutter(
    time: Res<Time>,
    mut next: Local<f32>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    villagers: Query<&Villager>,
    toast: Option<ResMut<Toast>>,
) {
    let now = time.elapsed_secs();
    if *next == 0.0 {
        *next = now + 20.0;
    }
    if now < *next {
        return;
    }
    *next = now + 25.0;
    let Some(me) = me else { return };
    let Some(here) = walkers
        .iter()
        .find(|(p, _)| p.id == me.0)
        .map(|(_, w)| w.pos)
    else {
        return;
    };
    let near = villagers
        .iter()
        .any(|v| v.id < 4 && v.pos.distance(here.xz()) < 8.0);
    if near && let Some(mut toast) = toast {
        let n = (now as usize / 25) % MUTTERS.len();
        toast.show(MUTTERS[n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_and_pushes_stay_out_of_the_arena() {
        let half = ARENA_HALF + ARENA_MARGIN;
        for p in POINTS {
            let q = outside_arena(p);
            assert_eq!(p, q, "{p} is inside the arena");
        }
        let q = outside_arena(ARENA_CENTER.xz() + Vec2::new(1.0, 0.5));
        let d = q - ARENA_CENTER.xz();
        assert!(d.x.abs() >= half - 1e-4 || d.y.abs() >= half - 1e-4);
    }
}
