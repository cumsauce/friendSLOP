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
}

impl Area {
    /// How far outside the bounds is `p`? 0 inside.
    pub fn gap(&self, p: Vec2) -> f32 {
        let ((x0, z0), (x1, z1)) = self.bounds;
        let below = Vec2::new(x0, z0) - p;
        let above = p - Vec2::new(x1, z1);
        below.max(above).max(Vec2::ZERO).length()
    }

    /// Should it be loaded for people at `people`? One that `was` loaded
    /// stays until they're all past `load + unload`.
    pub fn wanted(&self, was: bool, people: impl IntoIterator<Item = Vec2>) -> bool {
        let reach = self.load + if was { self.unload } else { 0.0 };
        people.into_iter().any(|p| self.gap(p) <= reach)
    }

    /// Does it hold `p` (inside its bounds)?
    pub fn holds(&self, p: Vec2) -> bool {
        self.gap(p) == 0.0
    }
}

pub static AREAS: friendslop_data::Data<Areas> = friendslop_data::Data::ron("data/base/areas.ron");

/// The current list of areas.
pub fn areas() -> std::sync::Arc<Areas> {
    AREAS.get()
}

/// The id of the first area holding `p` (x, z), if any.
pub fn area_at(p: Vec2) -> Option<String> {
    areas()
        .areas
        .iter()
        .find(|a| a.holds(p))
        .map(|a| a.id.clone())
}

/// The area nearest `p` (x, z): the one holding it, else the closest by
/// [`Area::gap`]. For things that move between areas (a walker on a route,
/// a drone): they belong to wherever they are now.
pub fn nearest_area(p: Vec2) -> Option<String> {
    areas()
        .areas
        .iter()
        .min_by(|a, b| a.gap(p).total_cmp(&b.gap(p)))
        .map(|a| a.id.clone())
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
        you: Option<Vec2>,
        players: &[Vec2],
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
            let me = force.unwrap_or_else(|| area.wanted(show.now, you));
            let anyone = force.unwrap_or_else(|| {
                if client {
                    area.wanted(sim.now, you)
                } else {
                    area.wanted(sim.now, you.into_iter().chain(players.iter().copied()))
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
    loaded.track(&areas(), force.0, client, here.map(|h| h.0.xz()), &people.0);
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
        .any(|a| loaded.loaded(&a.id, Layer::Show) && a.holds(here.0.xz()))
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

/// The area `e` belongs to: its own [`InArea`] or [`AreaRoot`], or its
/// nearest ancestor's.
pub fn owner<'a>(
    e: Entity,
    parents: &Query<&ChildOf>,
    tags: &'a Query<(Option<&InArea>, Option<&AreaRoot>)>,
) -> Option<&'a str> {
    let mut at = e;
    loop {
        if let Ok((tag, root)) = tags.get(at) {
            if let Some(t) = tag {
                return Some(&t.area);
            }
            if let Some(r) = root {
                return Some(&r.area);
            }
        }
        at = parents.get(at).ok()?.parent();
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
    fn an_area_loads_near_and_unloads_further_out() {
        let a = area();
        assert_eq!(a.gap(Vec2::new(5.0, 5.0)), 0.0);
        assert_eq!(a.gap(Vec2::new(25.0, 5.0)), 15.0);
        assert!(a.wanted(false, [Vec2::new(25.0, 5.0)]));
        // 25 m out: past `load`, inside `load + unload`.
        let edge = [Vec2::new(35.0, 5.0)];
        assert!(!a.wanted(false, edge));
        assert!(a.wanted(true, edge));
        assert!(!a.wanted(true, [Vec2::new(45.0, 5.0)]));
    }

    #[test]
    fn the_host_simulates_for_a_friend_but_only_shows_you() {
        let mut l = Loaded::default();
        let far = Some(Vec2::new(500.0, 0.0));
        l.track(&list(), None, false, far, &[Vec2::new(5.0, 5.0)]);
        assert!(l.is("nyc", Layer::Sim) && !l.is("nyc", Layer::Show));
        assert!(l.loaded("nyc", Layer::Sim));
        // A client away from it doesn't simulate it.
        let mut c = Loaded::default();
        c.track(&list(), None, true, far, &[Vec2::new(5.0, 5.0)]);
        assert!(!c.is("nyc", Layer::Sim));
    }

    #[test]
    fn layers_load_and_unload_once() {
        let mut l = Loaded::default();
        let (there, away) = (Some(Vec2::new(5.0, 5.0)), Some(Vec2::new(500.0, 0.0)));
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
