//! Areas (docs/design/performance-and-areas.md, Part 2): **everything spawned
//! into the world belongs to exactly one area, or is on the short named list
//! of [`GLOBALS`].** An area is loaded while someone needs it and despawned
//! when nobody does. The list is data: `data/base/areas.ron`; every site in
//! `sites.ron` names the area it is in.
//!
//! An area has three layers, because the host simulates for everyone:
//!
//! | Layer | Loaded when | Ask |
//! |---|---|---|
//! | [`Layer::Sim`]: colliders, simulation | anyone is near (host); you are near (client) | `areas::live("nyc")` |
//! | replicated gameplay state | always: spawn it as before, never under an area | (none) |
//! | [`Layer::Show`]: meshes, lights, sound | you are near | `areas::shown("nyc")` |
//!
//! ```ignore
//! // Spawn its colliders when the layer loads, under the area's root:
//! app.add_systems(Update, walls.run_if(areas::loaded(Layer::Sim, "nyc")));
//! fn walls(mut commands: Commands, roots: Res<areas::Roots>) {
//!     let Some(root) = roots.get("nyc", Layer::Sim) else { return };
//!     commands.spawn((Collider::cuboid(1.0, 1.0, 1.0), ChildOf(root)));
//!     // or, when it can't be a child: commands.spawn((InArea::sim("nyc"), ..))
//! }
//! app.add_systems(Update, spawn_meshes.run_if(areas::loaded(Layer::Show, "nyc")));
//! app.add_systems(Update, honk.run_if(areas::shown("nyc")));
//! app.add_systems(Update, traffic.run_if(areas::live("nyc")));
//! ```
//!
//! Unloading needs no code: a layer that unloads despawns its root (and so
//! everything under it) and every entity carrying its [`InArea`]. A layer
//! loads within `load` m of the area's bounds and unloads only once nobody is
//! within `load + unload` m (hysteresis). An id the file doesn't have counts
//! as loaded, like `sites`, so a broken file leaves the game running.
//!
//! Teleports (helicopter, train, a portal) send [`Cover`] to put the loading
//! curtain up while the place they land in loads (`crates/game/src/loading.rs`).

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_replicon::prelude::ClientState;

use crate::AppState;
use crate::place::Here;
use crate::sites::People;

/// Every area: `assets/data/base/areas.ron` (read at run time).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Areas {
    pub areas: Vec<Area>,
}

/// One area: a part of the world loaded and unloaded as a whole.
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Area {
    /// What code and `sites.ron` call it.
    pub id: String,
    /// What players call it.
    pub name: String,
    /// Its box on the ground: (min x, min z), (max x, max z). As tall as the
    /// world, so places stacked over and under it are part of it.
    pub bounds: ((f32, f32), (f32, f32)),
    /// The modeler map its layout comes from (`maps/<area>.bsn`), if any.
    pub map: Option<String>,
    /// How far (m) outside its bounds it starts loading.
    pub load: f32,
    /// How much further (m) everyone must be before it unloads again.
    pub unload: f32,
    /// Its floor and ceiling (min y, max y), for a place stacked over or
    /// under another (the warehouse's inside, the tunnels, hell, the sky
    /// courses); `None` (left out) is as tall as the world.
    #[serde(default)]
    pub heights: Option<(f32, f32)>,
}

impl Area {
    /// How far outside the bounds is `p`? 0 inside.
    pub fn gap(&self, p: Vec3) -> f32 {
        let ((x0, z0), (x1, z1)) = self.bounds;
        let (y0, y1) = self.heights.unwrap_or((f32::MIN, f32::MAX));
        let below = Vec3::new(x0, y0, z0) - p;
        let above = p - Vec3::new(x1, y1, z1);
        below.max(above).max(Vec3::ZERO).length()
    }

    /// Should it be loaded for people at `people`? One that `was` loaded
    /// stays until they're all past `load + unload`.
    pub fn wanted(&self, was: bool, people: impl IntoIterator<Item = Vec3>) -> bool {
        let reach = self.load + if was { self.unload } else { 0.0 };
        people.into_iter().any(|p| self.gap(p) <= reach)
    }

    /// Does it hold `p` (inside its bounds)?
    pub fn holds(&self, p: Vec3) -> bool {
        self.gap(p) == 0.0
    }
}

pub static AREAS: friendslop_data::Data<Areas> = friendslop_data::Data::ron("data/base/areas.ron");

/// The current list of areas.
pub fn areas() -> std::sync::Arc<Areas> {
    AREAS.get()
}

/// The id of the area holding `p`, if any: one with [`Area::heights`]
/// (stacked over or under another) before one as tall as the world.
pub fn area_at(p: Vec3) -> Option<String> {
    let list = areas();
    let mut holding: Vec<&Area> = list.areas.iter().filter(|a| a.holds(p)).collect();
    holding.sort_by_key(|a| a.heights.is_none());
    holding.first().map(|a| a.id.clone())
}

/// The area nearest `p`: the one holding it (a stacked one first, like
/// [`area_at`]), else the closest by [`Area::gap`]. For things that move
/// between areas (a walker on a route, a drone): they belong to wherever
/// they are now.
pub fn nearest_area(p: Vec3) -> Option<String> {
    areas()
        .areas
        .iter()
        .min_by(|a, b| {
            a.gap(p)
                .total_cmp(&b.gap(p))
                .then(a.heights.is_none().cmp(&b.heights.is_none()))
        })
        .map(|a| a.id.clone())
}

/// In `area`'s colliders and simulation layer (gone when it unloads, and
/// when the game ends).
pub fn sim(area: &str) -> impl Bundle {
    (DespawnOnExit(AppState::InGame), InArea::sim(area))
}

/// In `area`'s visuals layer (gone when you leave it, and when the game ends).
pub fn show(area: &str) -> impl Bundle {
    (DespawnOnExit(AppState::InGame), InArea::show(area))
}

/// Run `systems` on the frame `area`'s colliders load (in `PreUpdate`,
/// after [`AreaSet`], so they're there before the frame's physics: a
/// teleport never lands on missing ground).
pub fn on_sim<M>(
    app: &mut App,
    area: &'static str,
    systems: impl IntoScheduleConfigs<bevy::ecs::system::ScheduleSystem, M>,
) {
    app.add_systems(PreUpdate, at_load(systems, Layer::Sim, area));
}

/// Run `systems` on the frame `area`'s visuals load (see [`on_sim`]).
pub fn on_show<M>(
    app: &mut App,
    area: &'static str,
    systems: impl IntoScheduleConfigs<bevy::ecs::system::ScheduleSystem, M>,
) {
    app.add_systems(PreUpdate, at_load(systems, Layer::Show, area));
}

/// Run `systems` (spawners written for the whole world) whenever either of
/// `area`'s layers loads, and file what they spawn: each new tree (an entity
/// and its children) that holds a collider, or nothing drawn at all, goes in
/// the Sim layer; one that's only drawn (meshes, scenes, lights, sound) in
/// the Show layer. A tree is kept only if its layer loaded this frame (the
/// other one is already there), so each runs at most once per load. Replicated
/// entities are left alone (gameplay state is always kept: don't spawn it here).
///
/// A tree with both colliders and meshes stays as long as the colliders do:
/// split it into two spawners when its looks are heavy.
pub fn spawn_into<M>(
    app: &mut App,
    area: &'static str,
    systems: impl IntoScheduleConfigs<bevy::ecs::system::ScheduleSystem, M>,
) {
    spawn_into_refreshed(app, area, "", systems);
}

/// [`spawn_into`], also run again (into whichever layers are loaded) on the
/// frame [`Refresh`] holds `key`: for spawners that rebuild what they made
/// when something changes (the village on a new world seed: it despawns its
/// old parts itself).
pub fn spawn_into_refreshed<M>(
    app: &mut App,
    area: &'static str,
    key: &'static str,
    systems: impl IntoScheduleConfigs<bevy::ecs::system::ScheduleSystem, M>,
) {
    let label = SpawnInto(NEXT_SPAWN.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    app.init_resource::<Refresh>();
    app.add_systems(label, systems);
    app.add_systems(
        PreUpdate,
        (
            (move |world: &mut World| spawn_and_file(world, label, area, key)).run_if(
                move |l: Res<Loaded>, r: Res<Refresh>| {
                    l.loaded(area, Layer::Sim)
                        || l.loaded(area, Layer::Show)
                        || (!key.is_empty() && r.0.contains(key) && l.is(area, Layer::Sim))
                },
            ),
            (move |mut r: ResMut<Refresh>| {
                r.0.remove(key);
            })
            .run_if(move |r: Res<Refresh>| r.0.contains(key)),
        )
            .chain()
            .after(AreaSet)
            .run_if(in_state(AppState::InGame)),
    );
}

/// Spawners ([`spawn_into_refreshed`]) to run again this frame, by key.
/// Insert in `PreUpdate` before [`AreaSet`] (or any time the frame before).
#[derive(Resource, Default, Debug)]
pub struct Refresh(pub bevy::platform::collections::HashSet<&'static str>);

/// [`spawn_into`] and the HUD's spawner as `App` methods, so plugins can
/// chain them: `app.spawn_into("commune", build).add_hud(spawn_text)`.
pub trait AreaApp {
    /// [`spawn_into`].
    fn spawn_into<M>(
        &mut self,
        area: &'static str,
        systems: impl IntoScheduleConfigs<bevy::ecs::system::ScheduleSystem, M>,
    ) -> &mut Self;
    /// [`crate::hud::on_enter_game`]: HUD and UI only, never world things.
    fn add_hud<M>(
        &mut self,
        systems: impl IntoScheduleConfigs<bevy::ecs::system::ScheduleSystem, M>,
    ) -> &mut Self;
    /// [`dress_in_area`].
    fn dress_in_area<C: Component>(&mut self) -> &mut Self;
}

impl AreaApp for App {
    fn spawn_into<M>(
        &mut self,
        area: &'static str,
        systems: impl IntoScheduleConfigs<bevy::ecs::system::ScheduleSystem, M>,
    ) -> &mut Self {
        spawn_into(self, area, systems);
        self
    }
    fn add_hud<M>(
        &mut self,
        systems: impl IntoScheduleConfigs<bevy::ecs::system::ScheduleSystem, M>,
    ) -> &mut Self {
        crate::hud::on_enter_game(self, systems);
        self
    }
    fn dress_in_area<C: Component>(&mut self) -> &mut Self {
        dress_in_area::<C>(self);
        self
    }
}

/// Replicated things with a `C` are drawn only while the area they're in
/// shows (gameplay state is always kept; its looks are local). Each frame
/// they get [`Shown`] while the visuals of the area they stand in are loaded
/// (always, outside every area) and lose it when they aren't; each child
/// tree under them that holds no collider and isn't replicated (the
/// dressing) is filed in that area's visuals ([`Dress`] + [`InArea`]), so
/// it goes when the area unloads and follows the thing between areas.
///
/// The feature dresses on `Added<Shown>` (it fires again each time the area
/// shows again): `Query<(Entity, &Villager), Added<Shown>>`. Keep colliders
/// out of the dressing: a tree with one is never filed or put away.
pub fn dress_in_area<C: Component>(app: &mut App) {
    app.add_systems(
        PreUpdate,
        follow::<C>
            .in_set(AreaSet)
            .after(own)
            .run_if(in_state(AppState::InGame)),
    );
}

/// On a replicated thing ([`dress_in_area`]): its area's visuals are
/// loaded, so it's dressed. Dress on `Added<Shown>`.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Shown;

/// A local child that draws a replicated thing, filed in the visuals of the
/// area the thing is in.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Dress;

/// Has `children` a [`Dress`] among them?
pub fn dressed(children: Option<&Children>, dresses: &Query<(), With<Dress>>) -> bool {
    children.is_some_and(|c| c.iter().any(|e| dresses.contains(e)))
}

/// Put a thing's dressing away: its filed children, and the drawing parts
/// on itself (a rig, a character rig, a mesh, a model), so dressing it again
/// builds them afresh.
fn undress(
    commands: &mut Commands,
    e: Entity,
    children: Option<&Children>,
    filed: &Query<(), With<Dress>>,
) {
    for c in children.into_iter().flat_map(|c| c.iter()) {
        if filed.contains(c) {
            commands.entity(c).try_despawn();
        }
    }
    let mut thing = commands.entity(e);
    thing.remove::<(
        Shown,
        Dwells,
        crate::people::Gait,
        crate::people::CharacterRig,
        Mesh3d,
        MeshMaterial3d<StandardMaterial>,
        bevy::world_serialization::WorldAssetRoot,
    )>();
    crate::rigged::forget(&mut thing);
}

/// Where a [`dress_in_area`] thing was last frame (`None`: outside every area).
#[derive(Component, Debug, Default, Clone, PartialEq)]
struct Dwells(Option<String>);

#[allow(clippy::type_complexity)]
fn follow<C: Component>(
    mut commands: Commands,
    loaded: Res<Loaded>,
    things: Query<
        (
            Entity,
            Option<&GlobalTransform>,
            Option<&Children>,
            Has<Shown>,
            Option<&Dwells>,
        ),
        (
            With<C>,
            Or<(
                With<bevy_replicon::prelude::Replicated>,
                With<bevy_replicon::client::Remote>,
            )>,
        ),
    >,
    filed: Query<(), With<Dress>>,
    kids: Query<&Children>,
    solid: Query<
        (),
        Or<(
            With<avian3d::prelude::Collider>,
            With<bevy_replicon::prelude::Replicated>,
            With<bevy_replicon::client::Remote>,
        )>,
    >,
) {
    for (e, at, children, shown, dwells) in &things {
        // Not placed yet (dressing places it): dress it, then look again.
        let area = at.and_then(|t| area_at(t.translation()));
        let want = area.as_deref().is_none_or(|a| loaded.is(a, Layer::Show));
        if want && !shown {
            commands.entity(e).insert(Shown);
        } else if !want && shown {
            undress(&mut commands, e, children, &filed);
            continue;
        }
        let moved = dwells.is_none_or(|d| d.0 != area);
        if moved {
            commands.entity(e).insert(Dwells(area.clone()));
        }
        let Some(children) = children else { continue };
        for c in children.iter() {
            let is_filed = filed.contains(c);
            if is_filed && !moved {
                continue;
            }
            if !is_filed
                && (solid.contains(c) || kids.iter_descendants(c).any(|d| solid.contains(d)))
            {
                continue;
            }
            let mut child = commands.entity(c);
            child.insert(Dress);
            match &area {
                Some(a) => {
                    child.remove::<Global>().insert(InArea::show(a));
                }
                None => {
                    child.remove::<InArea>().insert(Global);
                }
            }
        }
    }
}

static NEXT_SPAWN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[derive(bevy::ecs::schedule::ScheduleLabel, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SpawnInto(usize);

fn spawn_and_file(world: &mut World, label: SpawnInto, area: &'static str, key: &'static str) {
    let again = !key.is_empty() && world.resource::<Refresh>().0.contains(key);
    let (sim, show) = {
        let l = world.resource::<Loaded>();
        if again {
            (l.is(area, Layer::Sim), l.is(area, Layer::Show))
        } else {
            (l.loaded(area, Layer::Sim), l.loaded(area, Layer::Show))
        }
    };
    let before: bevy::ecs::entity::EntityHashSet = world.query::<Entity>().iter(world).collect();
    world.run_schedule(label);
    let mut new: Vec<Entity> = world
        .query::<Entity>()
        .iter(world)
        .filter(|e| !before.contains(e))
        .collect();
    new.sort();
    let is_new = |e: Entity| !before.contains(&e);
    for e in new {
        let parent = world.get::<ChildOf>(e).map(ChildOf::parent);
        if parent.is_some_and(is_new) {
            continue; // filed with its tree
        }
        if world.get::<bevy_replicon::prelude::Replicated>(e).is_some() {
            continue;
        }
        let (collider, drawn) = contents(world, e);
        let layer = if collider || !drawn {
            Layer::Sim
        } else {
            Layer::Show
        };
        let keep = match layer {
            Layer::Sim => sim,
            Layer::Show => show,
        };
        let Ok(mut entity) = world.get_entity_mut(e) else {
            continue;
        };
        if keep {
            entity.insert(InArea {
                area: area.into(),
                layer,
            });
        } else {
            entity.despawn();
        }
    }
}

/// Does the tree under `e` hold a collider? Anything drawn or heard?
fn contents(world: &World, e: Entity) -> (bool, bool) {
    let mut collider = false;
    let mut drawn = false;
    let mut stack = vec![e];
    while let Some(at) = stack.pop() {
        let Ok(r) = world.get_entity(at) else {
            continue;
        };
        collider |= r.contains::<avian3d::prelude::Collider>();
        drawn |= r.contains::<Mesh3d>()
            || r.contains::<WorldAssetRoot>()
            || r.contains::<PointLight>()
            || r.contains::<SpotLight>()
            || r.contains::<AudioPlayer>()
            || r.contains::<crate::spatial::Emitter>()
            || r.contains::<friendslop_world::map::Map>();
        if let Some(children) = r.get::<Children>() {
            stack.extend(children.iter());
        }
    }
    (collider, drawn)
}

fn at_load<M>(
    systems: impl IntoScheduleConfigs<bevy::ecs::system::ScheduleSystem, M>,
    layer: Layer,
    area: &'static str,
) -> bevy::ecs::schedule::ScheduleConfigs<bevy::ecs::system::ScheduleSystem> {
    systems
        .into_configs()
        .after(AreaSet)
        .run_if(in_state(AppState::InGame))
        .run_if(loaded(layer, area))
}

/// The globals: the only files that may spawn into the world on entering
/// the game and own nothing in any area. Sky, camera, HUD, input, players
/// and replicated gameplay state. `tests/it/areas.rs` holds every other
/// file that spawns at game start to the ratchet list. Add a line here only
/// for something that is truly everywhere for everyone.
pub const GLOBALS: &[(&str, &str)] = &[
    ("crates/base/src/hud.rs", "HUD"),
    ("crates/base/src/sites.rs", "which sites are awake"),
    ("crates/base/src/features/voice_lines.rs", "voice lines"),
    ("crates/game/src/ui/mod.rs", "UI"),
    ("crates/game/src/ui/settings_panel.rs", "settings panel"),
    ("crates/game/src/ui/pause.rs", "pause screen"),
    ("crates/game/src/camera.rs", "camera"),
    ("crates/game/src/controls.rs", "input"),
    ("crates/game/src/loading.rs", "loading curtain"),
    ("crates/game/src/atmosphere.rs", "sky, sun and fog"),
    ("crates/game/src/ambience.rs", "ambient sound bed"),
    ("crates/play/src/features/fps.rs", "players' bodies"),
    ("crates/play/src/features/stats.rs", "replicated stats"),
    ("crates/play/src/features/fate.rs", "replicated fate"),
    (
        "crates/story/src/features/campaign.rs",
        "replicated campaign",
    ),
    (
        "crates/story/src/features/objective.rs",
        "replicated objective",
    ),
    (
        "crates/village/src/features/morality.rs",
        "replicated morality",
    ),
    (
        "crates/game/src/world.rs",
        "the main camera and the scoreboard",
    ),
    (
        "crates/side/src/features/sesbian_lex.rs",
        "the hearts and glow that follow whoever's lexing, anywhere",
    ),
];

/// An area's layer that loads on its own (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Colliders and simulation: anyone near (host), you near (client).
    Sim,
    /// Meshes, lights and sound: you near.
    Show,
}

/// Belongs to `area`'s `layer`: despawned when that layer unloads. Use it
/// for what can't sit under the area's root ([`Roots`]).
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct InArea {
    pub area: String,
    pub layer: Layer,
}

impl InArea {
    /// In `area`'s colliders/simulation layer.
    pub fn sim(area: &str) -> Self {
        Self {
            area: area.into(),
            layer: Layer::Sim,
        }
    }

    /// In `area`'s visuals layer.
    pub fn show(area: &str) -> Self {
        Self {
            area: area.into(),
            layer: Layer::Show,
        }
    }
}

/// The root entity of a loaded layer: spawn its things as children.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct AreaRoot {
    pub area: String,
    pub layer: Layer,
}

/// Put the loading curtain up (a teleport into a place that has to load).
/// It lifts once the models have loaded and frames are smooth again.
#[derive(Message, Debug, Clone, Default)]
pub struct Cover;

/// Every loaded layer's root entity.
#[derive(Resource, Default, Debug)]
pub struct Roots(HashMap<(String, Layer), Entity>);

impl Roots {
    /// The root of `area`'s `layer`, while it's loaded.
    pub fn get(&self, area: &str, layer: Layer) -> Option<Entity> {
        self.0.get(&(area.to_owned(), layer)).copied()
    }

    /// Every loaded root, as (area, layer, entity).
    pub fn iter(&self) -> impl Iterator<Item = (&str, Layer, Entity)> {
        self.0.iter().map(|((a, l), e)| (a.as_str(), *l, *e))
    }
}

/// Force every area loaded (`Some(true)`) or unloaded (`Some(false)`), for
/// tests and debugging; `None` follows the players.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct Force(pub Option<bool>);

/// One layer this frame and last.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct State2 {
    now: bool,
    before: bool,
}

/// Which areas' layers are loaded.
#[derive(Resource, Default, Debug)]
pub struct Loaded(HashMap<(String, Layer), State2>);

impl Loaded {
    /// Is `area`'s `layer` loaded? (An unknown area counts as loaded.)
    pub fn is(&self, area: &str, layer: Layer) -> bool {
        self.0.get(&(area.to_owned(), layer)).is_none_or(|s| s.now)
    }

    /// Did `area`'s `layer` load this frame?
    pub fn loaded(&self, area: &str, layer: Layer) -> bool {
        self.0
            .get(&(area.to_owned(), layer))
            .is_some_and(|s| s.now && !s.before)
    }

    /// Did `area`'s `layer` unload this frame?
    pub fn unloaded(&self, area: &str, layer: Layer) -> bool {
        self.0
            .get(&(area.to_owned(), layer))
            .is_some_and(|s| !s.now && s.before)
    }

    /// Work out this frame. `client`: this peer only simulates where you are.
    fn track(
        &mut self,
        list: &Areas,
        force: Option<bool>,
        client: bool,
        you: Option<Vec3>,
        players: &[Vec3],
    ) {
        self.0
            .retain(|(id, _), _| list.areas.iter().any(|a| a.id == *id));
        for area in &list.areas {
            let get = |l: &HashMap<_, State2>, layer| {
                l.get(&(area.id.clone(), layer))
                    .copied()
                    .unwrap_or_default()
            };
            let (show, sim) = (get(&self.0, Layer::Show), get(&self.0, Layer::Sim));
            // Someone in a place stacked over or under this one (the
            // tunnels under the commune) is there, not here: they got there
            // by teleport and come back the same way.
            let counts = |p: &Vec3| {
                !list
                    .areas
                    .iter()
                    .any(|o| o.id != area.id && o.heights.is_some() && o.holds(*p))
            };
            let me = force.unwrap_or_else(|| area.wanted(show.now, you.filter(counts)));
            let anyone = force.unwrap_or_else(|| {
                if client {
                    area.wanted(sim.now, you.filter(counts))
                } else {
                    area.wanted(
                        sim.now,
                        you.into_iter()
                            .chain(players.iter().copied())
                            .filter(counts),
                    )
                }
            });
            for (layer, was, now) in [(Layer::Show, show, me), (Layer::Sim, sim, anyone)] {
                self.0.insert(
                    (area.id.clone(), layer),
                    State2 {
                        now,
                        before: was.now,
                    },
                );
            }
        }
    }
}

/// Run while `area`'s colliders and simulation are loaded.
pub fn live(area: &'static str) -> impl FnMut(Res<Loaded>) -> bool + Clone {
    move |l: Res<Loaded>| l.is(area, Layer::Sim)
}

/// Run while `area`'s visuals are loaded (you are near).
pub fn shown(area: &'static str) -> impl FnMut(Res<Loaded>) -> bool + Clone {
    move |l: Res<Loaded>| l.is(area, Layer::Show)
}

/// Run on the frame `area`'s `layer` loads: spawn under [`Roots::get`].
pub fn loaded(layer: Layer, area: &'static str) -> impl FnMut(Res<Loaded>) -> bool + Clone {
    move |l: Res<Loaded>| l.loaded(area, layer)
}

/// Run on the frame `area`'s `layer` unloads (its things are already gone;
/// for resetting resources).
pub fn unloaded(layer: Layer, area: &'static str) -> impl FnMut(Res<Loaded>) -> bool + Clone {
    move |l: Res<Loaded>| l.unloaded(area, layer)
}

/// Updates [`Loaded`] and [`Roots`] in `PreUpdate`, after the sites.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AreaSet;

/// Added with the protocol next to `sites::plugin` (headless too).
pub fn plugin(app: &mut App) {
    app.init_resource::<Loaded>()
        .init_resource::<Roots>()
        .init_resource::<Force>()
        .add_message::<Cover>()
        // Leaving the game forgets every layer, so the next game's first
        // frame loads (and `loaded` fires for) the ones you start in.
        .add_systems(OnExit(AppState::InGame), forget)
        .add_systems(
            PreUpdate,
            (track, own, cover_arrivals)
                .chain()
                .in_set(AreaSet)
                .run_if(in_state(AppState::InGame)),
        );
}

fn forget(mut loaded: ResMut<Loaded>, mut roots: ResMut<Roots>) {
    loaded.0.clear();
    roots.0.clear();
}

fn track(
    here: Option<Res<Here>>,
    people: Res<People>,
    force: Res<Force>,
    client: Option<Res<State<ClientState>>>,
    mut loaded: ResMut<Loaded>,
) {
    let client = client.is_some_and(|c| *c.get() == ClientState::Connected);
    loaded.track(&areas(), force.0, client, here.map(|h| h.0), &people.0);
}

/// Spawn the roots of layers that loaded; despawn what unloaded.
fn own(
    mut commands: Commands,
    loaded: Res<Loaded>,
    mut roots: ResMut<Roots>,
    tagged: Query<(Entity, &InArea)>,
) {
    for ((area, layer), s) in &loaded.0 {
        let key = (area.clone(), *layer);
        if s.now && !roots.0.contains_key(&key) {
            let root = commands
                .spawn((
                    Name::new(format!("area {area} {layer:?}")),
                    AreaRoot {
                        area: area.clone(),
                        layer: *layer,
                    },
                    Transform::default(),
                    Visibility::default(),
                    DespawnOnExit(AppState::InGame),
                ))
                .id();
            roots.0.insert(key, root);
        } else if !s.now
            && let Some(root) = roots.0.remove(&key)
        {
            commands.entity(root).despawn();
        }
    }
    for (e, tag) in &tagged {
        if !loaded.is(&tag.area, tag.layer) {
            commands.entity(e).despawn();
        }
    }
}

/// A teleport landed you inside an area whose visuals weren't loaded (you
/// didn't walk in: walking loads it `load` m out): put the curtain up while
/// it loads. Covers every teleport (trains, start points, respawns) without
/// each one asking.
fn cover_arrivals(here: Option<Res<Here>>, loaded: Res<Loaded>, mut cover: MessageWriter<Cover>) {
    let Some(here) = here else { return };
    if areas()
        .areas
        .iter()
        .any(|a| loaded.loaded(&a.id, Layer::Show) && a.holds(here.0))
    {
        cover.write(Cover);
    }
}

/// One area's line in the census.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Count {
    pub entities: usize,
    pub meshes: usize,
    pub colliders: usize,
}

/// Count `things` (their owning area, or `None`; has a mesh; has a
/// collider) per area, `None` last ("no area": the progress bar).
pub fn census<'a>(
    things: impl IntoIterator<Item = (Option<&'a str>, bool, bool)>,
) -> Vec<(Option<&'a str>, Count)> {
    let mut out: Vec<(Option<&str>, Count)> = Vec::new();
    for (area, mesh, collider) in things {
        let i = match out.iter().position(|(a, _)| *a == area) {
            Some(i) => i,
            None => {
                out.push((area, Count::default()));
                out.len() - 1
            }
        };
        let c = &mut out[i].1;
        c.entities += 1;
        c.meshes += usize::from(mesh);
        c.colliders += usize::from(collider);
    }
    out.sort_by_key(|(a, _)| (a.is_none(), a.map(str::to_owned)));
    out
}

/// Not in any area on purpose: a global thing that is everywhere for
/// whoever it's for (the camera, the sun and clouds, the ambience that
/// follows you, UI). Put it on the top entity; its children count with it.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct Global;

/// What the census looks at on each entity, to say who owns it.
#[derive(bevy::ecs::query::QueryData)]
pub struct Tags {
    tag: Option<&'static InArea>,
    root: Option<&'static AreaRoot>,
    global: Has<Global>,
    replicated: Has<bevy_replicon::prelude::Replicated>,
    remote: Has<bevy_replicon::client::Remote>,
    waste: Has<friendslop_world::wastes::WasteChunk>,
    placed: Has<Transform>,
    ui: Has<Node>,
}

/// The census line for things kept on purpose outside areas ([`Global`],
/// and UI).
pub const GLOBAL: &str = "globals";
/// The census line for replicated gameplay state (always kept: an area's
/// middle layer) and the local dressing under it, which moves with it.
pub const REPLICATED: &str = "replicated";
/// The census line for the wasteland's streamed chunks.
pub const WASTES: &str = "wastes";

/// Who owns `e`, for the census: its own or nearest ancestor's [`InArea`]
/// or [`AreaRoot`] (that area), [`Global`] or UI ([`GLOBAL`]), `Replicated`
/// or `Remote` on a client ([`REPLICATED`]) or a wastes chunk ([`WASTES`]). `Some(None)`: no area,
/// the progress bar. `None`: not in the world at all (no `Transform`:
/// observers, resources, systems), not counted.
pub fn owner<'a>(
    e: Entity,
    parents: &Query<&ChildOf>,
    tags: &'a Query<Tags>,
) -> Option<Option<&'a str>> {
    let mut at = e;
    let mut placed = false;
    loop {
        if let Ok(t) = tags.get(at) {
            placed |= t.placed || t.ui;
            if let Some(t) = t.tag {
                return Some(Some(&t.area));
            }
            if let Some(r) = t.root {
                return Some(Some(&r.area));
            }
            if t.global || t.ui {
                return Some(Some(GLOBAL));
            }
            if t.replicated || t.remote {
                return Some(Some(REPLICATED));
            }
            if t.waste {
                return Some(Some(WASTES));
            }
        }
        match parents.get(at) {
            Ok(p) => at = p.parent(),
            Err(_) => return placed.then_some(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area() -> Area {
        Area {
            id: "nyc".into(),
            name: "New York".into(),
            bounds: ((0.0, 0.0), (10.0, 10.0)),
            map: None,
            load: 20.0,
            unload: 10.0,
            heights: None,
        }
    }

    fn list() -> Areas {
        Areas {
            areas: vec![area()],
        }
    }

    #[test]
    fn the_file_parses_with_every_field() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../game/assets")
            .join(AREAS.path());
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("missing {path:?}");
        };
        assert!(
            friendslop_data::DataFile::set_bytes(&AREAS, &bytes),
            "areas.ron doesn't parse (or misses a field)"
        );
        let areas = areas();
        assert!(!areas.areas.is_empty());
        for (i, a) in areas.areas.iter().enumerate() {
            assert!(!areas.areas[..i].iter().any(|b| b.id == a.id), "{}", a.id);
            let ((x0, z0), (x1, z1)) = a.bounds;
            assert!(x0 < x1 && z0 < z1, "{}'s bounds are inside out", a.id);
            assert!(a.load >= 0.0 && a.unload >= 0.0, "{}", a.id);
        }
    }

    #[test]
    fn a_stacked_area_has_a_floor_and_a_ceiling_and_wins_where_it_is() {
        let mut under = area();
        under.id = "cellar".into();
        under.heights = Some((-40.0, -20.0));
        assert_eq!(under.gap(Vec3::new(5.0, -30.0, 5.0)), 0.0);
        assert_eq!(under.gap(Vec3::new(5.0, 0.0, 5.0)), 20.0);
        assert!(!under.holds(Vec3::new(5.0, 0.0, 5.0)));
        // As tall as the world, the other holds both; the cellar wins below.
        assert!(area().holds(Vec3::new(5.0, -30.0, 5.0)));
    }

    #[test]
    fn an_area_loads_near_and_unloads_further_out() {
        let a = area();
        assert_eq!(a.gap(Vec3::new(5.0, 0.0, 5.0)), 0.0);
        assert_eq!(a.gap(Vec3::new(25.0, 0.0, 5.0)), 15.0);
        assert!(a.wanted(false, [Vec3::new(25.0, 0.0, 5.0)]));
        // 25 m out: past `load`, inside `load + unload`.
        let edge = [Vec3::new(35.0, 0.0, 5.0)];
        assert!(!a.wanted(false, edge));
        assert!(a.wanted(true, edge));
        assert!(!a.wanted(true, [Vec3::new(45.0, 0.0, 5.0)]));
    }

    #[test]
    fn the_host_simulates_for_a_friend_but_only_shows_you() {
        let mut l = Loaded::default();
        let far = Some(Vec3::new(500.0, 0.0, 0.0));
        l.track(&list(), None, false, far, &[Vec3::new(5.0, 0.0, 5.0)]);
        assert!(l.is("nyc", Layer::Sim) && !l.is("nyc", Layer::Show));
        assert!(l.loaded("nyc", Layer::Sim));
        // A client away from it doesn't simulate it.
        let mut c = Loaded::default();
        c.track(&list(), None, true, far, &[Vec3::new(5.0, 0.0, 5.0)]);
        assert!(!c.is("nyc", Layer::Sim));
    }

    #[test]
    fn layers_load_and_unload_once() {
        let mut l = Loaded::default();
        let (there, away) = (
            Some(Vec3::new(5.0, 0.0, 5.0)),
            Some(Vec3::new(500.0, 0.0, 0.0)),
        );
        l.track(&list(), None, false, there, &[]);
        assert!(l.loaded("nyc", Layer::Show));
        l.track(&list(), None, false, there, &[]);
        assert!(!l.loaded("nyc", Layer::Show) && l.is("nyc", Layer::Show));
        l.track(&list(), None, false, away, &[]);
        assert!(l.unloaded("nyc", Layer::Show) && l.unloaded("nyc", Layer::Sim));
        l.track(&list(), Some(true), false, away, &[]);
        assert!(l.loaded("nyc", Layer::Show));
    }

    #[test]
    fn an_unknown_area_counts_as_loaded() {
        let l = Loaded::default();
        assert!(l.is("nowhere", Layer::Sim) && !l.loaded("nowhere", Layer::Sim));
    }

    #[test]
    fn the_census_counts_per_area_with_no_area_last() {
        let c = census([
            (None, true, false),
            (Some("nyc"), true, true),
            (Some("commune"), false, true),
            (Some("nyc"), false, false),
        ]);
        let ids: Vec<_> = c.iter().map(|(a, _)| *a).collect();
        assert_eq!(ids, [Some("commune"), Some("nyc"), None]);
        assert_eq!(
            c[1].1,
            Count {
                entities: 2,
                meshes: 1,
                colliders: 1
            }
        );
    }
}
