//! Letting your fat cat go (from a request to be able to kill the cat that
//! follows you all the time). The cat follows you as usual; this is only
//! ever something you choose.
//!
//! Press [`KILL_KEY`] with your own cat within reach and the server puts it
//! down: the cat's ghost floats slowly up into the sky and fades, with a
//! soft sad tune and a quiet line for everyone. Nothing follows you after
//! that. A small stray kitten ([`StrayCat`], replicated) waits where the cat
//! was: click it to adopt a cat again, a merely fat one.
//! Only your own cat can be killed and only your own stray adopted.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::fat_cat::FatCat;
use super::interact::{Clickable, Clicked};
use crate::AppState;
use crate::features::fps::Walker;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// The key that kills your cat.
pub const KILL_KEY: KeyCode = KeyCode::Delete;

/// Tuning numbers: `assets/data/afterlife/cat_grave.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Seconds the ghost cat takes to float away.
    pub ghost_secs: f32,
    /// How fast it rises (metres per second).
    pub ghost_rise: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/afterlife/cat_grave.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The kitten waiting for `owner` to adopt it, where their cat was. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct StrayCat {
    pub owner: u64,
    pub at: Vec3,
}

/// Client -> server: let my cat go.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct KillCat;

/// Client -> server: adopt my stray again.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct AdoptCat;

/// Server -> everyone: `owner`'s cat died at `at`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct CatKilled {
    pub owner: u64,
    pub at: Vec3,
}

/// Whether `owner` has let their cat go and not adopted again.
pub fn without_cat(owner: u64, strays: &Query<&StrayCat>) -> bool {
    strays.iter().any(|s| s.owner == owner)
}

/// The quiet line for a death, from `me`'s point of view.
pub fn killed_text(owner: u64, me: Option<u64>) -> String {
    if me == Some(owner) {
        "Your cat is gone. It floats up and up, and then it isn't there.".to_string()
    } else {
        format!("Player {owner}'s cat floats away.")
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<StrayCat>()
        .add_client_event::<KillCat>(Channel::Ordered)
        .add_client_event::<AdoptCat>(Channel::Ordered)
        .add_server_event::<CatKilled>(Channel::Ordered)
        .add_observer(kill_cat)
        .add_observer(adopt_cat);
}

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    }
}

/// Server: the owner's own cat dies, if it is close enough to reach.
fn kill_cat(
    ev: On<FromClient<KillCat>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Player, &Walker)>,
    cats: Query<(Entity, &FatCat)>,
) {
    let who = owner_of(ev.client_id);
    let Some((_, player, walker)) = walkers.iter().find(|(o, ..)| o.0 == who) else {
        return;
    };
    let Some((entity, cat)) = cats.iter().find(|(_, c)| c.owner == player.id) else {
        return;
    };
    let reach = super::interact::SERVER_REACH + 0.5 * cat.size;
    if !super::interact::within(walker.pos, cat.pos, reach) {
        super::interact::deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    let at = cat.pos;
    commands.entity(entity).despawn();
    commands.spawn((
        StrayCat {
            owner: player.id,
            at,
        },
        Replicated,
    ));
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: CatKilled {
            owner: player.id,
            at,
        },
    });
}

/// Server: the owner adopts their stray; `give_cats` spawns the cat.
fn adopt_cat(
    ev: On<FromClient<AdoptCat>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Player, &Walker)>,
    strays: Query<(Entity, &StrayCat)>,
) {
    let who = owner_of(ev.client_id);
    let Some((_, player, walker)) = walkers.iter().find(|(o, ..)| o.0 == who) else {
        return;
    };
    let Some((entity, stray)) = strays.iter().find(|(_, s)| s.owner == player.id) else {
        return;
    };
    if !super::interact::within(walker.pos, stray.at, super::interact::SERVER_REACH) {
        super::interact::deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    commands.entity(entity).despawn();
}

/// Server: a leaver's stray goes with them.
fn clear_leavers(
    mut commands: Commands,
    players: Query<&Player>,
    strays: Query<(Entity, &StrayCat)>,
) {
    for (entity, stray) in &strays {
        if !players.iter().any(|p| p.id == stray.owner) {
            commands.entity(entity).despawn();
        }
    }
}

/// A rising, fading ghost cat.
#[derive(Component)]
struct Ghost(f32);

pub fn plugin(app: &mut App) {
    app.add_observer(on_killed)
        .add_observer(click_stray)
        .add_systems(Update, clear_leavers.run_if(in_state(ServerState::Running)))
        .add_systems(
            Update,
            (send_kill, dress_strays, drift_ghosts).run_if(in_state(AppState::InGame)),
        );
}

/// Local: the kill key.
fn send_kill(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KILL_KEY) {
        commands.client_trigger(KillCat);
    }
}

/// Local: clicking your own stray adopts it.
fn click_stray(
    ev: On<Clicked>,
    mut commands: Commands,
    strays: Query<&StrayCat>,
    me: Option<Res<LocalPlayer>>,
) {
    let (Ok(stray), Some(me)) = (strays.get(ev.entity), me) else {
        return;
    };
    if stray.owner == me.0 {
        commands.client_trigger(AdoptCat);
    }
}

/// Local: the waiting kitten, a small copy of the fat cat model.
fn dress_strays(
    mut commands: Commands,
    new: Query<(Entity, &StrayCat), Added<StrayCat>>,
    assets: Option<Res<AssetServer>>,
    me: Option<Res<LocalPlayer>>,
) {
    let Some(assets) = assets else {
        return;
    };
    for (entity, stray) in &new {
        let mut click = Clickable::at_height(0.3);
        if me.as_ref().is_some_and(|m| m.0 == stray.owner) {
            click = click.labelled("adopt a cat");
        }
        commands
            .entity(entity)
            .try_insert((
                Transform::from_translation(stray.at).with_scale(Vec3::splat(0.3)),
                Visibility::default(),
                click,
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    WorldAssetRoot(
                        assets.load(GltfAssetLabel::Scene(0).from_asset(super::fat_cat::CAT_MODEL)),
                    ),
                    Transform::default(),
                ));
            });
    }
}

/// Local: a cat died. A soft tune, a ghost, a quiet line.
fn on_killed(
    ev: On<CatKilled>,
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
        toast.show(killed_text(ev.owner, me.map(|m| m.0)));
    }
    crate::spatial::play_wav_at(
        &mut commands,
        ev.at + Vec3::Y,
        0.5,
        crate::spatial::Carry::CALL,
        || {
            super::bear::wav_bytes(
                &farewell_samples(super::synth_audio::RATE),
                super::synth_audio::RATE,
            )
        },
    );
    let (Some(mut meshes), Some(mut materials)) = (meshes, materials) else {
        return;
    };
    let mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.9, 0.95, 1.0, 0.6),
        emissive: LinearRgba::rgb(0.5, 0.6, 0.9),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });
    commands.spawn((
        Ghost(0.0),
        Mesh3d(meshes.add(Sphere::new(0.9))),
        MeshMaterial3d(mat),
        Transform::from_translation(ev.at + Vec3::Y).with_scale(Vec3::new(1.3, 0.9, 1.0)),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Local: the ghost floats up and fades out.
fn drift_ghosts(
    mut commands: Commands,
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut ghosts: Query<(
        Entity,
        &mut Ghost,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let secs = tuning().ghost_secs.max(0.1);
    for (e, mut g, mut tf, mat) in &mut ghosts {
        g.0 += time.delta_secs();
        if g.0 >= secs {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation.y += tuning().ghost_rise * time.delta_secs();
        if let Some(mut m) = materials.get_mut(&mat.0) {
            m.base_color = m.base_color.with_alpha(0.6 * (1.0 - g.0 / secs));
        }
    }
}

/// A slow, soft falling phrase: three sine notes sinking and fading.
pub fn farewell_samples(rate: u32) -> Vec<f32> {
    use std::f32::consts::TAU;
    let notes = [(440.0, 0.9), (392.0, 0.9), (330.0, 1.8)];
    let mut out = Vec::new();
    let mut phase = 0.0f32;
    for (hz, secs) in notes {
        let n = (secs * rate as f32) as usize;
        for i in 0..n {
            let t = i as f32 / rate as f32;
            let p = t / secs;
            phase = (phase + hz / rate as f32).fract();
            let env = (t * 8.0).min(1.0) * (1.0 - p).powf(1.5);
            out.push(0.25 * env * ((phase * TAU).sin() + 0.2 * (phase * TAU * 2.0).sin()));
        }
    }
    super::synth_audio::master(&mut out, rate, 3000.0, 0.5);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn farewell_is_audible_and_finite() {
        let s = farewell_samples(super::super::synth_audio::RATE);
        assert!(s.len() > 1000);
        assert!(s.iter().all(|v| v.is_finite()));
        assert!(s.iter().any(|v| v.abs() > 0.05));
    }

    #[test]
    fn line_names_the_owner() {
        assert!(killed_text(1, Some(1)).contains("Your cat"));
        assert!(killed_text(2, Some(1)).contains("Player 2"));
    }
}
