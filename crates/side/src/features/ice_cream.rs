//! High Places' ice cream machine (#350, by bben): "the ice cream dispenser
//! is down. surprise me with a quest to fix it too".
//!
//! A soft-serve machine stands on the High Places deck ([`super::high_places`])
//! with an OUT OF ORDER card on it. Click it and it tells you why: the city's
//! pigeons ([`super::pigeons`]) nested in it, and it needs parts that have
//! wandered off into the city's shops ([`Tuning::parts`], each in one of
//! [`super::nyc::SHOPS`]). Find each one (click it), bring them back, click
//! the machine and it's fixed for everyone: the quest is shared, so any
//! player's find counts. Then it hands out free soft serve.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, in_reach};
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// One part the machine needs.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Part {
    pub name: String,
    /// The shop it's in (one of nyc::SHOPS).
    pub shop: String,
    /// Shown when you find it ({who} found it).
    pub found_line: String,
}

/// Tuning and writing: `assets/data/side/ice_cream.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Where the machine stands on the deck (x, z, from the deck's middle) and its turn (degrees).
    pub machine: (f32, f32),
    pub machine_yaw: f32,
    pub parts: Vec<Part>,
    /// Clicking it while broken ({missing}: the parts still out there).
    pub broken_line: String,
    /// Clicking it with every part found: fixed ({who} fixed it).
    pub fixed_line: String,
    /// Clicking it once it works ({flavour}).
    pub cone_line: String,
    pub flavours: Vec<String>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/ice_cream.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// The quest, shared by everyone: which parts are found (bits) and whether
/// the machine works. One server entity, replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Machine {
    pub found: u32,
    pub fixed: bool,
    /// Cones served (picks the flavour).
    pub cones: u32,
}

impl Machine {
    pub fn has(&self, i: usize) -> bool {
        self.found & (1 << i) != 0
    }
    /// Every one of `n` parts found?
    pub fn all(&self, n: usize) -> bool {
        (0..n).all(|i| self.has(i))
    }
}

/// What clicking the machine does.
#[derive(Debug, PartialEq, Eq)]
pub enum Click {
    /// Still broken; these parts are missing.
    Missing(Vec<usize>),
    /// Every part's here: fix it.
    Fix,
    /// It works: a cone.
    Cone,
}

/// Pure: the machine's answer.
pub fn click_machine(m: &Machine, n: usize) -> Click {
    if m.fixed {
        Click::Cone
    } else if m.all(n) {
        Click::Fix
    } else {
        Click::Missing((0..n).filter(|i| !m.has(*i)).collect())
    }
}

/// Pure: the machine on the deck, for the WTC's foot.
pub fn machine_at(foot: Vec3, t: &Tuning) -> Vec3 {
    super::high_places::deck(foot) + Vec3::new(t.machine.0, 0.0, t.machine.1)
}

/// Pure: where part `i` lies: in the middle of its shop, at counter height
/// (a little to one side per part, so two in one shop don't overlap).
pub fn part_at(room: &super::nyc::Room, i: usize) -> Vec3 {
    let mid = (room.min + room.max) / 2.0;
    Vec3::new(
        mid.x,
        crate::rules::nyc::CITY_Y + 0.9,
        mid.y + (i as f32 - 1.0) * 0.7,
    )
}

fn room_of(name: &str) -> Option<super::nyc::Room> {
    let i = super::nyc::SHOPS.iter().position(|s| *s == name)?;
    let c = super::nyc::city(&super::nyc::MAP.get());
    c.rooms.get(i).copied().filter(|r| r.min.x < r.max.x)
}

// ------------------------------------------------------------------ server

/// Client → server: picked up part `0`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TakePart(pub u8);

/// Client → server: clicked the machine.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct UseMachine;

/// Server → clients: a toast for player `to`, or everyone (`to: None`).
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct MachineNews {
    pub to: Option<u64>,
    pub text: String,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Machine>()
        .add_client_event::<TakePart>(Channel::Ordered)
        .add_client_event::<UseMachine>(Channel::Ordered)
        .add_server_event::<MachineNews>(Channel::Ordered)
        .add_observer(take)
        .add_observer(use_machine)
        .add_systems(OnEnter(ServerState::Running), spawn_machine);
}

fn spawn_machine(mut commands: Commands) {
    commands.spawn((Machine::default(), Replicated));
}

fn who(client: ClientId, players: &Query<(&Player, &PlayerOwner)>) -> Option<u64> {
    let owner = match client {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    players.iter().find(|p| p.1.0 == owner).map(|p| p.0.id)
}

fn tell(commands: &mut Commands, to: Option<u64>, text: String) {
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: MachineNews { to, text },
    });
}

fn take(
    ev: On<FromClient<TakePart>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    players: Query<(&Player, &PlayerOwner)>,
    mut machine: Query<&mut Machine>,
) {
    let t = tuning();
    let i = ev.0 as usize;
    let Some(part) = t.parts.get(i) else { return };
    let Some(room) = room_of(&part.shop) else {
        return;
    };
    if i >= 32 || !in_reach(&mut commands, ev.client_id, &walkers, part_at(&room, i)) {
        return;
    }
    let Ok(mut m) = machine.single_mut() else {
        return;
    };
    if m.has(i) {
        return;
    }
    m.found |= 1 << i;
    let name =
        who(ev.client_id, &players).map_or("someone".to_string(), |id| format!("player {id}"));
    tell(&mut commands, None, part.found_line.replace("{who}", &name));
}

fn use_machine(
    ev: On<FromClient<UseMachine>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    players: Query<(&Player, &PlayerOwner)>,
    mut machine: Query<&mut Machine>,
) {
    let t = tuning();
    let Some(foot) = super::high_places::wtc(&super::nyc::MAP.get()) else {
        return;
    };
    if !in_reach(&mut commands, ev.client_id, &walkers, machine_at(foot, &t)) {
        return;
    }
    let Ok(mut m) = machine.single_mut() else {
        return;
    };
    let me = who(ev.client_id, &players);
    match click_machine(&m, t.parts.len()) {
        Click::Missing(missing) => {
            let list: Vec<String> = missing
                .iter()
                .filter_map(|i| t.parts.get(*i))
                .map(|p| format!("{} (the {})", p.name, p.shop))
                .collect();
            tell(
                &mut commands,
                me,
                t.broken_line.replace("{missing}", &list.join(", ")),
            );
        }
        Click::Fix => {
            m.fixed = true;
            let name = me.map_or("someone".to_string(), |id| format!("player {id}"));
            tell(&mut commands, None, t.fixed_line.replace("{who}", &name));
        }
        Click::Cone => {
            let n = t.flavours.len().max(1);
            let flavour = t
                .flavours
                .get(m.cones as usize % n)
                .cloned()
                .unwrap_or_default();
            m.cones += 1;
            tell(
                &mut commands,
                me,
                t.cone_line.replace("{flavour}", &flavour),
            );
        }
    }
}

// ------------------------------------------------------------------ every peer

pub fn plugin(app: &mut App) {
    app.add_observer(show_news)
        .add_observer(click)
        .add_systems(OnEnter(AppState::InGame), build)
        .add_systems(Update, show_state.run_if(in_state(AppState::InGame)));
}

#[derive(Component)]
enum Thing {
    Machine,
    Part(u8),
}

/// The OUT OF ORDER card (hidden once fixed).
#[derive(Component)]
struct Card;

fn build(mut commands: Commands, assets: Res<AssetServer>) {
    let t = tuning();
    if let Some(foot) = super::high_places::wtc(&super::nyc::MAP.get()) {
        let at = machine_at(foot, &t);
        let turn = Quat::from_rotation_y(t.machine_yaw.to_radians());
        commands.spawn((
            WorldAssetRoot(
                assets.load(GltfAssetLabel::Scene(0).from_asset("models/ice-cream.gltf")),
            ),
            Transform::from_translation(at).with_rotation(turn),
            DespawnOnExit(AppState::InGame),
        ));
        commands.spawn((
            Card,
            WorldAssetRoot(
                assets.load(GltfAssetLabel::Scene(0).from_asset("models/out-of-order.gltf")),
            ),
            Transform::from_translation(at).with_rotation(turn),
            DespawnOnExit(AppState::InGame),
        ));
        commands.spawn((
            Transform::from_translation(at + Vec3::Y * 0.9).with_rotation(turn),
            avian3d::prelude::RigidBody::Static,
            avian3d::prelude::Collider::cuboid(0.95, 1.8, 0.85),
            DespawnOnExit(AppState::InGame),
        ));
        commands.spawn((
            Thing::Machine,
            Transform::from_translation(at),
            Visibility::default(),
            Clickable::at_height(1.2).labelled("soft serve machine"),
            DespawnOnExit(AppState::InGame),
        ));
    }
    let mesh = Cuboid::new(0.25, 0.25, 0.25);
    for (i, part) in t.parts.iter().enumerate() {
        let Some(room) = room_of(&part.shop) else {
            continue;
        };
        commands.spawn((
            Thing::Part(i as u8),
            Transform::from_translation(part_at(&room, i)),
            Visibility::default(),
            Clickable::at_height(0.0).labelled(format!("take the {}", part.name).leak()),
            PartMesh(mesh),
            DespawnOnExit(AppState::InGame),
        ));
    }
}

/// A placeholder box for a part (drawn once the mesh assets are around).
#[derive(Component)]
struct PartMesh(Cuboid);

fn click(click: On<Clicked>, mut commands: Commands, things: Query<&Thing>) {
    match things.get(click.entity) {
        Ok(Thing::Machine) => commands.client_trigger(UseMachine),
        Ok(Thing::Part(i)) => commands.client_trigger(TakePart(*i)),
        Err(_) => {}
    }
}

fn show_news(
    ev: On<MachineNews>,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    if ev.to.is_none_or(|to| to == me.0) {
        toast.show(ev.text.clone());
    }
}

/// Give parts their boxes, hide found parts and the card once it's fixed.
#[allow(clippy::type_complexity)]
fn show_state(
    mut commands: Commands,
    meshes: Option<ResMut<Assets<Mesh>>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
    new: Query<(Entity, &PartMesh)>,
    machine: Query<&Machine>,
    mut parts: Query<(&Thing, &mut Visibility), Without<Card>>,
    mut card: Query<&mut Visibility, With<Card>>,
) {
    if let (Some(mut meshes), Some(mut materials)) = (meshes, materials) {
        for (e, m) in &new {
            let mat = crate::art::soft(&mut materials, Color::srgb(0.95, 0.6, 0.75));
            commands
                .entity(e)
                .remove::<PartMesh>()
                .insert((Mesh3d(meshes.add(m.0)), MeshMaterial3d(mat)));
        }
    }
    let m = machine.iter().next().copied().unwrap_or_default();
    for (thing, mut vis) in &mut parts {
        if let Thing::Part(i) = thing {
            let want = if m.has(*i as usize) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
            vis.set_if_neq(want);
        }
    }
    let want = if m.fixed {
        Visibility::Hidden
    } else {
        Visibility::Inherited
    };
    for mut v in &mut card {
        v.set_if_neq(want);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> std::sync::Arc<Tuning> {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        TUNING.get()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(!t.parts.is_empty() && t.parts.len() <= 32);
        for p in &t.parts {
            assert!(
                super::super::nyc::SHOPS.contains(&p.shop.as_str()),
                "{}",
                p.shop
            );
            assert!(!p.name.is_empty() && p.found_line.contains("{who}"));
        }
        assert!(t.broken_line.contains("{missing}") && t.fixed_line.contains("{who}"));
        assert!(t.cone_line.contains("{flavour}") && !t.flavours.is_empty());
        // On the deck, clear of the kiosk and the elevator hut.
        let (x, z) = t.machine;
        assert!(x.abs() < 14.0 && z.abs() < 14.0);
        assert!(!(x > 3.0 && z < -9.0) && x > -9.5);
    }

    #[test]
    fn find_every_part_then_fix_it_then_eat() {
        let mut m = Machine::default();
        assert_eq!(click_machine(&m, 3), Click::Missing(vec![0, 1, 2]));
        m.found |= 1 << 1;
        assert_eq!(click_machine(&m, 3), Click::Missing(vec![0, 2]));
        m.found |= 1 | (1 << 2);
        assert_eq!(click_machine(&m, 3), Click::Fix);
        m.fixed = true;
        assert_eq!(click_machine(&m, 3), Click::Cone);
    }
}
