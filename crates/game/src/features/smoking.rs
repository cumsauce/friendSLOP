//! Smoking, and Sewer Paul. From issue #129 by doomy: "you can get cigs from
//! sewer paul if you get him his favorite crow eggs. and if you romance him
//! he'll give you a joint. you can smoke both".
//!
//! - Crow eggs ([`CrowEgg`]) sit in the grass by each crow's home. Click one
//!   to pocket it; a new one is laid [`EGG_SECS`] later.
//! - Sewer Paul lurks in the sewer east of the junction under the commune
//!   ([`paul_spot`]). Click him while carrying crow eggs: he takes one and
//!   hands you a cig. Click him with no eggs and you flirt instead; every
//!   [`LOVE_FOR_JOINT`]th flirt he blushes and gives you a joint.
//! - Press P to smoke: a cig if you have one, otherwise a joint. Everyone
//!   sees smoke curl up from you (greener for a joint) for [`SMOKE_SECS`].
//!
//! The server owns the pockets ([`Stash`], replicated) and checks reach.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::crows::CROW_COUNT;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, in_reach};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Seconds until a taken egg is laid again.
pub const EGG_SECS: f32 = 20.0;
/// Flirts with Paul per joint.
pub const LOVE_FOR_JOINT: u32 = 3;
/// How long one smoke lasts.
pub const SMOKE_SECS: f32 = 8.0;

/// What a player carries for (and from) Sewer Paul.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stash {
    pub eggs: u32,
    pub cigs: u32,
    pub joints: u32,
    /// How far Paul has fallen for you towards the next joint.
    pub love: u32,
}

/// On a player while they smoke: seconds left, and whether it's a joint.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Smoking {
    pub left: f32,
    pub joint: bool,
}

/// A crow egg in the grass by crow `nest`'s home.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrowEgg {
    pub nest: u32,
}

/// Client -> server: I take the egg at this nest.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TakeEgg {
    pub nest: u32,
}

/// Client -> server: I talk to Sewer Paul.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct VisitPaul;

/// Client -> server: I light up.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Smoke;

/// Server -> one client: what Paul (or your pockets) said.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct PaulSays {
    pub text: String,
}

/// What happens when you visit Paul with this stash. Pure, so it's tested.
pub fn visit(stash: &mut Stash) -> &'static str {
    if stash.eggs > 0 {
        stash.eggs -= 1;
        stash.cigs += 1;
        return "Sewer Paul: \"my favourite! crow eggs!\" he slurps it and gives you a cig";
    }
    stash.love += 1;
    if stash.love >= LOVE_FOR_JOINT {
        stash.love = 0;
        stash.joints += 1;
        "Sewer Paul blushes under the grime: \"for you, sweetheart\" (he gives you a joint)"
    } else {
        "you flirt with Sewer Paul. he giggles. (bring crow eggs for cigs)"
    }
}

/// Light up: a cig first, else a joint. `None` if your pockets are empty.
pub fn light(stash: &mut Stash) -> Option<bool> {
    if stash.cigs > 0 {
        stash.cigs -= 1;
        Some(false)
    } else if stash.joints > 0 {
        stash.joints -= 1;
        Some(true)
    } else {
        None
    }
}

/// Where Sewer Paul stands: in his chamber halfway between the junction and
/// the first manhole (pinned whatever the world seed), off to one side of
/// the water.
pub fn paul_spot() -> Vec3 {
    use super::underground::{TUNNEL_Y, cell_centre, paul_cell};
    let at = cell_centre(paul_cell());
    Vec3::new(at.x, TUNNEL_Y, at.y + 0.9)
}

/// Where the egg by crow `nest`'s home lies.
pub fn egg_spot(nest: u32) -> Vec3 {
    let angle = nest as f32 / CROW_COUNT as f32 * std::f32::consts::TAU;
    let at = Vec2::from_angle(angle) * 4.0 + Vec2::from_angle(angle + 1.0) * 1.2;
    Vec3::new(at.x, crate::terrain::height_at(at.x, at.y) + 0.08, at.y)
}

pub fn protocol(app: &mut App) {
    app.replicate::<Stash>()
        .replicate::<Smoking>()
        .replicate::<CrowEgg>()
        .add_client_event::<TakeEgg>(Channel::Ordered)
        .add_client_event::<VisitPaul>(Channel::Ordered)
        .add_client_event::<Smoke>(Channel::Ordered)
        .add_server_event::<PaulSays>(Channel::Ordered)
        .init_resource::<Nests>()
        .add_observer(give_stash)
        .add_observer(take_egg)
        .add_observer(visit_paul)
        .add_observer(smoke)
        .add_systems(OnEnter(ServerState::Running), lay_eggs)
        .add_systems(OnExit(ServerState::Running), clear_eggs)
        .add_systems(
            Update,
            (relay, burn_down).run_if(in_state(ServerState::Running)),
        );
}

/// Server only: per nest, seconds until the egg is laid again.
#[derive(Resource, Default)]
struct Nests(Vec<Option<f32>>);

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    }
}

fn say(commands: &mut Commands, client: ClientId, text: &str) {
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(client),
        message: PaulSays { text: text.into() },
    });
}

fn give_stash(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() == ServerState::Running {
        commands.entity(add.entity).insert(Stash::default());
    }
}

fn lay_eggs(mut commands: Commands, mut nests: ResMut<Nests>) {
    nests.0 = vec![None; CROW_COUNT as usize];
    for nest in 0..CROW_COUNT {
        commands.spawn((CrowEgg { nest }, Replicated));
    }
}

fn clear_eggs(mut commands: Commands, eggs: Query<Entity, With<CrowEgg>>) {
    for e in &eggs {
        commands.entity(e).despawn();
    }
}

/// Server: crows lay again.
fn relay(time: Res<Time>, mut commands: Commands, mut nests: ResMut<Nests>) {
    for (nest, timer) in nests.0.iter_mut().enumerate() {
        let Some(left) = timer.as_mut() else { continue };
        *left -= time.delta_secs();
        if *left <= 0.0 {
            *timer = None;
            commands.spawn((CrowEgg { nest: nest as u32 }, Replicated));
        }
    }
}

/// Server: smokes burn down and go out.
fn burn_down(time: Res<Time>, mut commands: Commands, mut smokers: Query<(Entity, &mut Smoking)>) {
    for (e, mut s) in &mut smokers {
        s.left -= time.delta_secs();
        if s.left <= 0.0 {
            commands.entity(e).remove::<Smoking>();
        }
    }
}

fn take_egg(
    take: On<FromClient<TakeEgg>>,
    mut commands: Commands,
    mut nests: ResMut<Nests>,
    eggs: Query<(Entity, &CrowEgg)>,
    mut stashes: Query<(&PlayerOwner, &mut Stash)>,
    walkers: Query<(&PlayerOwner, &Walker)>,
) {
    let nest = take.nest;
    let Some(timer) = nests.0.get_mut(nest as usize) else {
        return;
    };
    if timer.is_some() {
        return; // already taken this tick
    }
    let Some((egg, _)) = eggs.iter().find(|(_, e)| e.nest == nest) else {
        return;
    };
    if !in_reach(&mut commands, take.client_id, &walkers, egg_spot(nest)) {
        return;
    }
    let owner = owner_of(take.client_id);
    let Some((_, mut stash)) = stashes.iter_mut().find(|(o, _)| o.0 == owner) else {
        return;
    };
    stash.eggs += 1;
    commands.entity(egg).despawn();
    *timer = Some(EGG_SECS);
    say(&mut commands, take.client_id, "you pocket a crow egg");
}

fn visit_paul(
    ev: On<FromClient<VisitPaul>>,
    mut commands: Commands,
    mut stashes: Query<(&PlayerOwner, &mut Stash)>,
    walkers: Query<(&PlayerOwner, &Walker)>,
) {
    if !in_reach(&mut commands, ev.client_id, &walkers, paul_spot()) {
        return;
    }
    let owner = owner_of(ev.client_id);
    let Some((_, mut stash)) = stashes.iter_mut().find(|(o, _)| o.0 == owner) else {
        return;
    };
    let text = visit(&mut stash);
    say(&mut commands, ev.client_id, text);
}

fn smoke(
    ev: On<FromClient<Smoke>>,
    mut commands: Commands,
    mut stashes: Query<(Entity, &PlayerOwner, &mut Stash, Has<Smoking>)>,
) {
    let owner = owner_of(ev.client_id);
    let Some((player, _, mut stash, busy)) = stashes.iter_mut().find(|(_, o, ..)| o.0 == owner)
    else {
        return;
    };
    if busy {
        return;
    }
    let Some(joint) = light(&mut stash) else {
        say(
            &mut commands,
            ev.client_id,
            "nothing to smoke: Sewer Paul has cigs (and joints, for lovers)",
        );
        return;
    };
    commands.entity(player).insert(Smoking {
        left: SMOKE_SECS,
        joint,
    });
    let text = if joint {
        "you spark the joint. everything is very funny now"
    } else {
        "you light a cig. you look so cool"
    };
    say(&mut commands, ev.client_id, text);
}

pub fn plugin(app: &mut App) {
    app.add_observer(click)
        .add_observer(show_says)
        .add_systems(OnEnter(AppState::InGame), (spawn_paul, spawn_ui))
        .add_systems(
            Update,
            (
                dress_eggs,
                press_smoke,
                show_stash.run_if(crate::world::hud_tick()),
                puff,
                drift,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// Sewer Paul's body (local, the same on every peer).
#[derive(Component)]
struct Paul;

fn spawn_paul(mut commands: Commands) {
    commands.spawn((
        Paul,
        crate::people::Gait::default(),
        Transform::from_translation(paul_spot()).with_rotation(Quat::from_rotation_y(1.2)),
        Visibility::default(),
        Clickable::at_height(1.0)
            .radius(60.0)
            .labelled("talk to Sewer Paul"),
        crate::people::CharacterLook::random(0x5e3e_9a01),
        DespawnOnExit(AppState::InGame),
    ));
}

fn click(
    click: On<Clicked>,
    mut commands: Commands,
    eggs: Query<&CrowEgg>,
    paul: Query<(), With<Paul>>,
) {
    if let Ok(egg) = eggs.get(click.entity) {
        commands.client_trigger(TakeEgg { nest: egg.nest });
    } else if paul.contains(click.entity) {
        commands.client_trigger(VisitPaul);
    }
}

fn show_says(ev: On<PaulSays>, toast: Option<ResMut<Toast>>) {
    if let Some(mut toast) = toast {
        toast.show(ev.text.clone());
    }
}

fn press_smoke(mut commands: Commands, keys: Res<ButtonInput<KeyCode>>) {
    if keys.just_pressed(KeyCode::KeyP) {
        commands.client_trigger(Smoke);
    }
}

#[derive(Component)]
struct StashText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        StashText,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(
            "Sewer Paul (in the sewers): bring him crow eggs (by the crows) for cigs, flirt for a joint. P smokes",
        ),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

fn show_stash(
    me: Option<Res<LocalPlayer>>,
    stashes: Query<(&Player, &Stash)>,
    mut text: Query<&mut Text, With<StashText>>,
) {
    let Some(me) = me else { return };
    let s = stashes
        .iter()
        .find(|(p, _)| p.id == me.0)
        .map_or(Stash::default(), |(_, s)| *s);
    let mut parts = Vec::new();
    for (n, what) in [
        (s.eggs, "crow eggs"),
        (s.cigs, "cigs"),
        (s.joints, "joints"),
    ] {
        if n > 0 {
            parts.push(format!("{what}: {n}"));
        }
    }
    let want = parts.join("  ");
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

/// Speckled blue-green eggs in the grass.
fn dress_eggs(
    mut commands: Commands,
    new: Query<(Entity, &CrowEgg), Added<CrowEgg>>,
    assets: Res<AssetServer>,
) {
    if new.is_empty() {
        return;
    }
    // Modeler-made, already egg-shaped (16 x 21 cm, origin at its centre).
    let egg_model =
        assets.load(GltfAssetLabel::Scene(0).from_asset("models/features/crow_egg.gltf"));
    for (e, egg) in &new {
        commands.entity(e).insert((
            WorldAssetRoot(egg_model.clone()),
            Transform::from_translation(egg_spot(egg.nest)),
            Clickable::at_height(0.0)
                .radius(30.0)
                .labelled("take the crow egg"),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// A rising, fading puff of smoke.
#[derive(Component)]
struct Puff {
    age: f32,
}

/// Smokers breathe out a puff now and then.
fn puff(
    mut commands: Commands,
    time: Res<Time>,
    smokers: Query<(&Smoking, &GlobalTransform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut since: Local<f32>,
    mut kit: Local<Option<(Handle<Mesh>, [Handle<StandardMaterial>; 2])>>,
) {
    *since += time.delta_secs();
    if *since < 0.4 || smokers.is_empty() {
        return;
    }
    *since = 0.0;
    // One mesh and two materials shared by every puff, made on the first one
    // (#151: no new GPU assets every 0.4 s).
    let (ball, [cig, joint]) = kit.get_or_insert_with(|| {
        let mut smoke = |color: Color| {
            materials.add(StandardMaterial {
                base_color: color,
                alpha_mode: AlphaMode::Blend,
                unlit: true,
                ..default()
            })
        };
        (
            meshes.add(Sphere::new(0.12)),
            [
                smoke(Color::srgba(0.85, 0.85, 0.85, 0.5)),
                smoke(Color::srgba(0.75, 0.9, 0.7, 0.5)),
            ],
        )
    });
    for (s, tf) in &smokers {
        let mat = if s.joint { joint.clone() } else { cig.clone() };
        commands.spawn((
            Puff { age: 0.0 },
            Mesh3d(ball.clone()),
            MeshMaterial3d(mat),
            Transform::from_translation(tf.translation() + Vec3::new(0.0, 1.7, -0.3)),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

fn drift(
    mut commands: Commands,
    time: Res<Time>,
    mut puffs: Query<(Entity, &mut Puff, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (e, mut p, mut tf) in &mut puffs {
        p.age += dt;
        if p.age > 2.5 {
            commands.entity(e).despawn();
            continue;
        }
        tf.translation += Vec3::new((p.age * 3.0).sin() * 0.1, 0.5, 0.0) * dt;
        tf.scale = Vec3::splat(1.0 + p.age);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eggs_buy_cigs_and_flirting_buys_joints() {
        let mut s = Stash {
            eggs: 1,
            ..default()
        };
        visit(&mut s);
        assert_eq!((s.eggs, s.cigs, s.joints), (0, 1, 0));
        for _ in 0..LOVE_FOR_JOINT {
            visit(&mut s);
        }
        assert_eq!((s.cigs, s.joints, s.love), (1, 1, 0));
    }

    #[test]
    fn cigs_are_smoked_before_joints() {
        let mut s = Stash {
            cigs: 1,
            joints: 1,
            ..default()
        };
        assert_eq!(light(&mut s), Some(false));
        assert_eq!(light(&mut s), Some(true));
        assert_eq!(light(&mut s), None);
    }

    #[test]
    fn paul_lives_in_the_sewer() {
        use super::super::underground::{Part, SEED_SALT, WorldSeed, cell_of, layout};
        let p = paul_spot();
        let cell = cell_of(Vec2::new(p.x, p.z - 0.9));
        for seed in 0..8 {
            let l = layout(WorldSeed(seed).for_(SEED_SALT));
            assert_eq!(l.part(cell), Some(Part::Sewer));
        }
    }
}
