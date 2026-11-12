//! The shipwreck. From issue #185 by Freyja Moth: "There was a ship that
//! crashed on the shore in the storm of '21, make sure there are gay pirate
//! aboard guarding a treasure that you have to battle with. Also one of the
//! pirates is a trans man".
//!
//! The *Rainbow Reckoning* lies beached on the east shore of the valley lake,
//! listing, mast snapped, its rainbow flag still flying (local scenery, the
//! same on every peer). Three replicated [`Pirate`]s guard the treasure chest
//! on the sand beside it. Click a pirate to battle: each hit takes one of his
//! [`PIRATE_HP`], and he swings back, sometimes knocking you
//! [`KNOCKBACK`] m away. A beaten pirate yields and sits down for
//! [`YIELD_SECS`]. While all three are down, the chest is open: click it to
//! take [`HAUL`] doubloons (replicated [`Doubloons`]), once each until the
//! crew gets back up.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Where the wreck lies (x, z): beached on the lake's east shore.
pub use friendslop_world::places::wreck_at;
/// Hits a pirate takes before he yields.
pub const PIRATE_HP: u8 = 3;
/// Tuning numbers: `assets/data/side/shipwreck.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// How long a beaten pirate stays down.
    pub yield_secs: f32,
    /// Chance a pirate's swing knocks you back.
    pub knockback_chance: f32,
    /// How far a knockback sends you.
    pub knockback: f32,
    /// Doubloons from one helping of treasure.
    pub haul: u32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/shipwreck.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The crew: name and the line he greets a challenger with.
pub static CREW: friendslop_data::Data<Vec<(String, String)>> =
    friendslop_data::Data::ron("data/text/shipwreck_crew.ron");

/// Crew member `id`: name and greeting.
pub fn crew_member(id: usize) -> (String, String) {
    let crew = CREW.get();
    crew.get(id % crew.len().max(1))
        .cloned()
        .unwrap_or_default()
}

/// A pirate guarding the treasure. `down` > 0 while he's yielded.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Pirate {
    pub id: u8,
    pub hp: u8,
    pub down: f32,
}

impl Pirate {
    pub fn up(&self) -> bool {
        self.down <= 0.0
    }

    pub fn name(&self) -> String {
        crew_member(self.id as usize).0
    }
}

/// The treasure chest (one, replicated so clients can click it).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Treasure;

/// How many doubloons this player has. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Doubloons(pub u32);

/// Client -> server: I attack pirate `id`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Attack {
    pub id: u8,
}

/// Client -> server: I take from the chest.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct Loot;

/// Server -> everyone: something happened at the wreck.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WreckNews {
    pub text: String,
}

/// Where pirate `id` stands, and the chest, on the sand (feet positions).
pub fn pirate_spot(id: u8) -> Vec3 {
    let x = wreck_at().x + 3.2;
    let z = wreck_at().y - 2.0 + id as f32 * 2.0;
    Vec3::new(x, crate::terrain::height_at(x, z), z)
}

pub fn chest_spot() -> Vec3 {
    let (x, z) = (wreck_at().x + 1.8, wreck_at().y);
    Vec3::new(x, crate::terrain::height_at(x, z), z)
}

/// Is the treasure open? Only while the whole crew is down.
pub fn open(pirates: &[Pirate]) -> bool {
    !pirates.is_empty() && pirates.iter().all(|p| !p.up())
}

/// One hit on `p`: returns whether he yielded with it.
pub fn hit(p: &mut Pirate) -> bool {
    if !p.up() {
        return false;
    }
    p.hp = p.hp.saturating_sub(1);
    if p.hp == 0 {
        p.down = tuning().yield_secs;
        true
    } else {
        false
    }
}

pub fn protocol(app: &mut App) {
    // Keep trees and bushes off the beach (see world::footprints).
    friendslop_world::footprints::claim(wreck_at() + Vec2::X * 1.5, 7.5);
    app.replicate::<Pirate>()
        .replicate::<Treasure>()
        .replicate::<Doubloons>()
        .add_client_event::<Attack>(Channel::Ordered)
        .add_client_event::<Loot>(Channel::Ordered)
        .add_server_event::<WreckNews>(Channel::Ordered)
        .init_resource::<Crew>()
        .add_observer(give_purse)
        .add_observer(attack)
        .add_observer(loot)
        .add_systems(OnEnter(ServerState::Running), man_the_wreck)
        .add_systems(Update, recover.run_if(in_state(ServerState::Running)));
}

/// Server only: dice, and who has looted since the chest opened.
#[derive(Resource)]
struct Crew {
    dice: u32,
    looted: Vec<u64>,
}

impl Default for Crew {
    fn default() -> Self {
        Self {
            dice: 0x5419_0185,
            looted: Vec::new(),
        }
    }
}

impl Crew {
    fn roll(&mut self) -> f32 {
        self.dice = self
            .dice
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.dice >> 8) as f32 / (1u32 << 24) as f32
    }
}

fn news(commands: &mut Commands, text: String) {
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: WreckNews { text },
    });
}

fn give_purse(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() == ServerState::Running {
        commands.entity(add.entity).insert(Doubloons::default());
    }
}

fn man_the_wreck(
    mut commands: Commands,
    mut crew: ResMut<Crew>,
    old: Query<Entity, Or<(With<Pirate>, With<Treasure>)>>,
) {
    for e in &old {
        commands.entity(e).despawn();
    }
    crew.looted.clear();
    for id in 0..CREW.get().len() as u8 {
        commands.spawn((
            Pirate {
                id,
                hp: PIRATE_HP,
                down: 0.0,
            },
            Replicated,
        ));
    }
    commands.spawn((Treasure, Replicated));
}

/// Server: beaten pirates get back up, and the chest closes again.
fn recover(
    time: Res<Time>,
    mut commands: Commands,
    mut crew: ResMut<Crew>,
    mut pirates: Query<&mut Pirate>,
) {
    let dt = time.delta_secs();
    for mut p in &mut pirates {
        if p.up() {
            continue;
        }
        p.down -= dt;
        if p.up() {
            p.hp = PIRATE_HP;
            crew.looted.clear();
            news(
                &mut commands,
                format!("{} is back on his feet, guarding the gold", p.name()),
            );
        }
    }
}

fn owner_of(client: ClientId) -> Option<Entity> {
    match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    }
}

#[allow(clippy::too_many_arguments)]
fn attack(
    ev: On<FromClient<Attack>>,
    mut commands: Commands,
    mut crew: ResMut<Crew>,
    mut pirates: Query<&mut Pirate>,
    mut bodies: Query<(&PlayerOwner, &mut Walker, Option<&mut crate::kcc::Kcc>)>,
    mut fate: super::fate::Fate,
) {
    let Some(mut p) = pirates.iter_mut().find(|p| p.id == ev.id) else {
        return;
    };
    if !p.up() {
        deny(
            &mut commands,
            ev.client_id,
            format!("{} has yielded", p.name()),
        );
        return;
    }
    let owner = owner_of(ev.client_id);
    let near = bodies
        .iter()
        .find(|(o, ..)| o.0 == owner)
        .is_some_and(|(_, w, _)| {
            super::interact::within(w.pos, pirate_spot(p.id), super::interact::SERVER_REACH)
        });
    if !near {
        deny(&mut commands, ev.client_id, "too far away: walk closer");
        return;
    }
    if fate.fails(ev.client_id) {
        return;
    }
    if p.hp == PIRATE_HP {
        news(&mut commands, crew_member(p.id as usize).1);
    }
    if hit(&mut p) {
        news(
            &mut commands,
            format!(
                "{} yields! \"fine, fine, you fight like a lesbian\"",
                p.name()
            ),
        );
    } else if crew.roll() < tuning().knockback_chance {
        // He swings back and sends you flying.
        if let Some((_, mut w, kcc)) = bodies.iter_mut().find(|(o, ..)| o.0 == owner) {
            let away = (w.pos.xz() - pirate_spot(p.id).xz()).normalize_or(Vec2::X);
            let to = w.pos.xz() + away * tuning().knockback;
            w.pos = Vec3::new(to.x, crate::terrain::height_at(to.x, to.y), to.y);
            if let Some(mut kcc) = kcc {
                kcc.velocity = Vec3::ZERO;
            }
        }
        deny(
            &mut commands,
            ev.client_id,
            format!("{} parries and knocks you back!", p.name()),
        );
    }
}

fn loot(
    ev: On<FromClient<Loot>>,
    mut commands: Commands,
    mut crew: ResMut<Crew>,
    pirates: Query<&Pirate>,
    walkers: Query<(&PlayerOwner, &Walker)>,
    mut purses: Query<(&Player, &PlayerOwner, &mut Doubloons)>,
) {
    let all: Vec<Pirate> = pirates.iter().copied().collect();
    if !open(&all) {
        deny(
            &mut commands,
            ev.client_id,
            "the pirates guard the chest: beat all three first",
        );
        return;
    }
    if !in_reach(&mut commands, ev.client_id, &walkers, chest_spot()) {
        return;
    }
    let owner = owner_of(ev.client_id);
    let Some((p, _, mut purse)) = purses.iter_mut().find(|(_, o, _)| o.0 == owner) else {
        return;
    };
    if crew.looted.contains(&p.id) {
        deny(
            &mut commands,
            ev.client_id,
            "you've had your share: wait for the crew to get up",
        );
        return;
    }
    crew.looted.push(p.id);
    purse.0 += tuning().haul;
    news(
        &mut commands,
        format!(
            "player {} plunders {} doubloons from the wreck!",
            p.id,
            tuning().haul
        ),
    );
}

pub fn plugin(app: &mut App) {
    app.add_observer(click)
        .add_observer(show_news)
        .add_systems(OnEnter(AppState::InGame), (build_wreck, spawn_ui))
        .add_systems(
            Update,
            (dress_pirates, pose_pirates, dress_chest, show_purse)
                .run_if(in_state(AppState::InGame)),
        );
}

fn click(
    ev: On<Clicked>,
    mut commands: Commands,
    pirates: Query<&Pirate>,
    chests: Query<(), With<Treasure>>,
) {
    if let Ok(p) = pirates.get(ev.entity) {
        commands.client_trigger(Attack { id: p.id });
    } else if chests.contains(ev.entity) {
        commands.client_trigger(Loot);
    }
}

fn show_news(ev: On<WreckNews>, toast: Option<ResMut<Toast>>) {
    if let Some(mut toast) = toast {
        toast.show(ev.text.clone());
    }
}

#[derive(Component)]
struct PurseText;

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        PurseText,
        crate::hud::slot(crate::hud::HudSlot::BottomRight),
        DespawnOnExit(AppState::InGame),
    ));
    commands.spawn((
        Text::new(
            "shipwreck (east shore of the valley lake): click the pirates to battle them, then the chest",
        ),
        crate::hud::slot(crate::hud::HudSlot::Help),
        DespawnOnExit(AppState::InGame),
    ));
}

fn show_purse(
    me: Option<Res<LocalPlayer>>,
    purses: Query<(&Player, &Doubloons)>,
    mut text: Query<&mut Text, With<PurseText>>,
) {
    let Some(me) = me else { return };
    let n = purses
        .iter()
        .find(|(p, _)| p.id == me.0)
        .map_or(0, |(_, d)| d.0);
    let want = if n == 0 {
        String::new()
    } else {
        format!("doubloons: {n}")
    };
    for mut t in &mut text {
        if t.0 != want {
            t.0.clone_from(&want);
        }
    }
}

#[derive(Component)]
struct Dressed;

/// Pirates wear the shared people rig (each his own look), facing the lake.
fn dress_pirates(mut commands: Commands, new: Query<(Entity, &Pirate), Without<Dressed>>) {
    for (e, p) in &new {
        commands.entity(e).insert((
            Dressed,
            crate::people::Gait::default(),
            Transform::from_translation(pirate_spot(p.id))
                .with_rotation(Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2)),
            Visibility::default(),
            Clickable::at_height(1.0)
                .radius(60.0)
                .labelled("fight the pirate"),
            crate::people::CharacterLook::random(0x0185_0000 + p.id as u64 * 7),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// Up and on guard, or sat down having yielded.
fn pose_pirates(
    pirates: Query<(&Pirate, &Children), With<Dressed>>,
    mut rigs: Query<&mut Transform, With<crate::people::RigRoot>>,
) {
    for (p, children) in &pirates {
        for c in children.iter() {
            if let Ok(mut tf) = rigs.get_mut(c) {
                let (y, sy) = if p.up() { (0.0, 1.0) } else { (-0.35, 0.7) };
                tf.translation.y += (y - tf.translation.y) * 0.2;
                tf.scale.y += (sy - tf.scale.y) * 0.2;
            }
        }
    }
}

fn dress_chest(
    mut commands: Commands,
    new: Query<Entity, Added<Treasure>>,
    assets: Res<AssetServer>,
) {
    for e in &new {
        commands
            .entity(e)
            .insert((
                Transform::from_translation(chest_spot()),
                Visibility::default(),
                Clickable::at_height(0.4)
                    .radius(50.0)
                    .labelled("take the treasure"),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|c| {
                // Modeler-made (sources in `.modeler/`, prefab `sp-treasure-chest`).
                c.spawn((model(&assets, "treasure_chest.gltf"), Transform::default()));
            });
    }
}

/// A modeler-made feature model in `assets/models/features`.
fn model(assets: &AssetServer, file: &str) -> WorldAssetRoot {
    WorldAssetRoot(
        assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
    )
}

/// The wreck: a listing hull of planks, a snapped mast, a rainbow flag.
fn build_wreck(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut exact: ResMut<super::cat_palette::ExactColors>,
) {
    let cube = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    let ground = crate::terrain::height_at(wreck_at().x, wreck_at().y);
    // Listing towards the lake, bow up the beach.
    let root = commands
        .spawn((
            Transform::from_translation(Vec3::new(wreck_at().x, ground - 0.3, wreck_at().y))
                .with_rotation(Quat::from_rotation_y(0.25) * Quat::from_rotation_x(0.18)),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ))
        .id();
    // Hull, decks with a hole stove in, the snapped mast and its broken top:
    // one modeler-made model in the root's frame (prefab `sp-shipwreck`).
    commands.spawn((
        model(&assets, "shipwreck.gltf"),
        Transform::default(),
        ChildOf(root),
    ));
    let mut part = |at: Vec3, size: Vec3, rot: Quat, mat: &Handle<StandardMaterial>| {
        commands.spawn((
            Mesh3d(cube.clone()),
            MeshMaterial3d(mat.clone()),
            Transform::from_translation(at)
                .with_rotation(rot)
                .with_scale(size),
            ChildOf(root),
        ));
    };
    // A rainbow flag: six stripes, red at the top.
    const STRIPES: [(f32, f32, f32); 6] = [
        (0.89, 0.01, 0.01),
        (1.0, 0.55, 0.0),
        (1.0, 0.93, 0.0),
        (0.0, 0.5, 0.15),
        (0.0, 0.3, 1.0),
        (0.46, 0.03, 0.53),
    ];
    for (i, (r, g, b)) in STRIPES.iter().enumerate() {
        // Exact colours: a pride flag shouldn't be cat-filtered.
        let m = super::cat_palette::exact_material(
            &mut materials,
            &mut exact,
            StandardMaterial {
                base_color: Color::srgb(*r, *g, *b),
                unlit: true,
                double_sided: true,
                cull_mode: None,
                ..default()
            },
        );
        part(
            Vec3::new(0.35 + 0.55, 4.5 - i as f32 * 0.14, -0.5),
            Vec3::new(1.1, 0.14, 0.02),
            Quat::IDENTITY,
            &m,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_hits_and_he_yields_and_the_chest_opens_when_all_are_down() {
        let mut crew = [0, 1, 2].map(|id| Pirate {
            id,
            hp: PIRATE_HP,
            down: 0.0,
        });
        assert!(!open(&crew));
        for p in crew.iter_mut() {
            assert!(!hit(p));
            assert!(!hit(p));
            assert!(hit(p), "the third hit beats him");
            assert!(!hit(p), "he's already down");
        }
        assert!(open(&crew));
    }

    #[test]
    fn the_wreck_is_on_the_lake_shore() {
        let lake = crate::terrain::lake_at();
        assert!(wreck_at().distance(lake) < 13.0);
        for id in 0..3 {
            let p = pirate_spot(id);
            assert!(
                !crate::terrain::is_lake(p.x, p.z),
                "pirate {id} in the water"
            );
        }
        let c = chest_spot();
        assert!(!crate::terrain::is_lake(c.x, c.z));
    }
}
