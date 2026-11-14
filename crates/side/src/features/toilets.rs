//! Toilets full of feces (#379, Kori.exe): "randomly spawning toilets filled
//! with feces. You can grab the feces and eat it for massive speed boost up
//! to 20 seconds".
//!
//! The server drops a toilet (`models/props/toilet.gltf`, with
//! `toilet_feces.gltf` in the bowl) on the ground somewhere near a random
//! player every so often, up to a cap. Click a full toilet to eat: it adds a
//! [`Stack`] to your boba [`Buzz`] (the same walk-speed effect, tint and HUD
//! line), so it stacks with drinks. An eaten toilet empties, then vanishes.
//! Numbers and lines: `assets/data/side/toilets.ron`.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::boba::{Buzz, Stack};
use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Tuning and writing: `assets/data/side/toilets.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Most toilets at once.
    pub max_toilets: usize,
    /// Seconds between new toilets.
    pub spawn_secs: f32,
    /// Distance (min, max) from a random player a new toilet lands.
    pub ring: (f32, f32),
    /// Seconds an empty toilet stays.
    pub empty_secs: f32,
    /// Seconds the boost lasts.
    pub boost_secs: f32,
    /// Walking speed it adds (1.0 = +100%).
    pub boost_speed: f32,
    /// The stack's name on the HUD.
    pub boost_name: String,
    /// Screen tint (r, g, b).
    pub tint: (f32, f32, f32),
    /// Shown when you eat.
    pub ate_line: String,
    /// Shown when it's already empty.
    pub empty_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/toilets.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

pub const TOILET_MODEL: &str = "models/props/toilet.gltf";
pub const FECES_MODEL: &str = "models/props/toilet_feces.gltf";
/// Where the pile sits in the bowl (model-local).
const PILE_AT: Vec3 = Vec3::new(0.0, 0.33, 0.02);
/// Most boost stacks (shared with boba's cap so the two mix).
const MAX_STACKS: usize = 6;

/// A toilet. Replicated; `full` until someone eats.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Toilet {
    pub at: Vec3,
    pub yaw: f32,
    pub full: bool,
}

/// Server only: seconds an empty toilet has been empty.
#[derive(Component, Default)]
struct EmptyFor(f32);

/// Client → server: eat from this toilet.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct EatFeces {
    /// The toilet's spot (the nearest toilet within a metre of it).
    pub at: Vec3,
}

/// Server → clients: a toast for player `to`.
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct ToiletNews {
    pub to: u64,
    pub text: String,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Toilet>()
        .add_client_event::<EatFeces>(Channel::Ordered)
        .add_server_event::<ToiletNews>(Channel::Ordered)
        .add_observer(eat)
        .add_systems(
            Update,
            (spawn_toilets, clear_empty).run_if(in_state(ServerState::Running)),
        );
}

/// Pure: a cheap hash to a number in 0..1.
fn unit(seed: u64) -> f32 {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32_D192_ED03;
    x ^= x >> 31;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 29;
    (x >> 40) as f32 / (1u64 << 24) as f32
}

/// Pure: where toilet number `n` lands near `around` (x, z), and its yaw.
pub fn spot(around: Vec2, n: u64, ring: (f32, f32)) -> (Vec2, f32) {
    let a = unit(n * 3) * std::f32::consts::TAU;
    let r = ring.0 + unit(n * 3 + 1) * (ring.1 - ring.0).max(0.0);
    let yaw = unit(n * 3 + 2) * std::f32::consts::TAU;
    (around + Vec2::new(a.cos(), a.sin()) * r, yaw)
}

fn spawn_toilets(
    mut commands: Commands,
    time: Res<Time>,
    mut clock: Local<f32>,
    mut n: Local<u64>,
    toilets: Query<(), With<Toilet>>,
    walkers: Query<&Walker>,
) {
    let t = tuning();
    *clock += time.delta_secs();
    if *clock < t.spawn_secs || toilets.iter().count() >= t.max_toilets {
        return;
    }
    *clock = 0.0;
    *n += 1;
    let bodies: Vec<&Walker> = walkers.iter().collect();
    if bodies.is_empty() {
        return;
    }
    let who = bodies[(unit(*n * 7 + 5) * bodies.len() as f32) as usize % bodies.len()];
    // Only out on the commune's ground (not up a tower or in another place).
    let ground = crate::terrain::height_at(who.pos.x, who.pos.z);
    if (who.pos.y - ground).abs() > 3.0 {
        return;
    }
    let (p, yaw) = spot(who.pos.xz(), *n + time.elapsed_secs() as u64, t.ring);
    commands.spawn((
        Toilet {
            at: Vec3::new(p.x, crate::terrain::height_at(p.x, p.y), p.y),
            yaw,
            full: true,
        },
        Replicated,
    ));
}

fn clear_empty(
    mut commands: Commands,
    time: Res<Time>,
    mut toilets: Query<(Entity, &Toilet, Option<&mut EmptyFor>)>,
) {
    let limit = tuning().empty_secs;
    for (e, toilet, empty) in &mut toilets {
        if toilet.full {
            continue;
        }
        match empty {
            Some(mut f) => {
                f.0 += time.delta_secs();
                if f.0 > limit {
                    commands.entity(e).despawn();
                }
            }
            None => {
                commands.entity(e).insert(EmptyFor::default());
            }
        }
    }
}

fn eat(
    ev: On<FromClient<EatFeces>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    mut toilets: Query<&mut Toilet>,
    mut eaters: Query<(&Player, &PlayerOwner, &mut Buzz)>,
) {
    let t = tuning();
    let Some(mut toilet) = toilets
        .iter_mut()
        .filter(|t| t.at.distance(ev.at) < 1.0)
        .min_by(|a, b| a.at.distance(ev.at).total_cmp(&b.at.distance(ev.at)))
    else {
        return;
    };
    if !in_reach(&mut commands, ev.client_id, &walkers, toilet.at) {
        return;
    }
    if !toilet.full {
        deny(&mut commands, ev.client_id, t.empty_line.clone());
        return;
    }
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((player, _, mut buzz)) = eaters.iter_mut().find(|b| b.1.0 == owner) else {
        return;
    };
    toilet.full = false;
    buzz.add(
        Stack {
            drink: t.boost_name.clone(),
            speed: t.boost_speed,
            secs: t.boost_secs,
            tint: t.tint,
        },
        MAX_STACKS,
    );
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: ToiletNews {
            to: player.id,
            text: t.ate_line.clone(),
        },
    });
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_observer(click).add_observer(news).add_systems(
        Update,
        (dress, empty_out).run_if(in_state(AppState::InGame)),
    );
}

/// The pile in a toilet's bowl (child entity).
#[derive(Component)]
struct Pile;

fn dress(
    mut commands: Commands,
    assets: Res<AssetServer>,
    new: Query<(Entity, &Toilet), Added<Toilet>>,
) {
    for (e, toilet) in &new {
        let scene =
            |p: &'static str| WorldAssetRoot(assets.load(GltfAssetLabel::Scene(0).from_asset(p)));
        commands
            .entity(e)
            .insert((
                Transform::from_translation(toilet.at)
                    .with_rotation(Quat::from_rotation_y(toilet.yaw)),
                Visibility::default(),
                scene(TOILET_MODEL),
                Clickable::at_height(0.5).labelled("eat the feces"),
                avian3d::prelude::RigidBody::Static,
                avian3d::prelude::Collider::cuboid(0.45, 0.8, 0.7),
                DespawnOnExit(AppState::InGame),
            ))
            .with_child((
                Pile,
                Transform::from_translation(PILE_AT),
                if toilet.full {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                scene(FECES_MODEL),
            ));
    }
}

fn empty_out(
    toilets: Query<(&Toilet, &Children), Changed<Toilet>>,
    mut piles: Query<&mut Visibility, With<Pile>>,
) {
    for (toilet, children) in &toilets {
        for c in children.iter() {
            if let Ok(mut v) = piles.get_mut(c) {
                *v = if toilet.full {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}

fn click(c: On<Clicked>, mut commands: Commands, toilets: Query<&Toilet>) {
    if let Ok(t) = toilets.get(c.entity) {
        commands.client_trigger(EatFeces { at: t.at });
    }
}

fn news(
    ev: On<ToiletNews>,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    if ev.to == me.0 {
        toast.show(ev.text.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        let t = TUNING.get();
        assert!(t.max_toilets > 0 && t.spawn_secs > 0.0 && t.empty_secs > 0.0);
        assert!(t.ring.1 >= t.ring.0 && t.ring.0 > 0.0);
        assert!(t.boost_secs > 0.0 && t.boost_secs <= 20.0 && t.boost_speed > 0.0);
        assert!(!t.boost_name.is_empty() && !t.ate_line.is_empty() && !t.empty_line.is_empty());
    }

    #[test]
    fn toilets_land_in_the_ring() {
        for n in 0..200 {
            let (p, _) = spot(Vec2::new(10.0, -5.0), n, (12.0, 35.0));
            let d = p.distance(Vec2::new(10.0, -5.0));
            assert!((11.99..=35.01).contains(&d), "{d}");
        }
    }
}
