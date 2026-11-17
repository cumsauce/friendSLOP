//! The map (M; not a friend idea, asked for directly): a live, top-down
//! picture of the area you're in, painted like a paper map.
//!
//! A second 3D camera ([`MapCamera`]) looks straight down over the area
//! (`areas.ron`'s box for where you stand) into an image, re-shot every few
//! frames while the map is open, so friends, cats and doors move on it. A UI
//! material ([`PaperMap`], `shaders/paper_map.wesl`) flattens that picture
//! into a few paper colours with ink lines, grain and a burnt edge. Every
//! player gets a bobbing cut-paper pin over the picture; the
//! names of the area's sites (`sites.ron`) and players are UI text on top.
//!
//! Indoors (a stacked place like the warehouse, or a walk-in interior) the
//! camera sits just over your head, so its near plane cuts the ceiling away.
//! It's local: nothing here is replicated. Tuning: `data/game/map.ron`.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{Exposure, RenderTarget, ScalingMode};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType, TextureFormat};
use bevy::shader::ShaderRef;
use serde::Deserialize;

use super::{Overlay, at, body, head};
use crate::AppState;
use crate::features::fps::Walker;
use crate::player::{LocalPlayer, Player};
use crate::theme::kit::{INK, size};
use crate::world::MapCamera;

/// The render layer only the map camera draws (pins). 31 is the HUD's.
pub const MAP_LAYER: usize = 30;

/// The map's open/closed key.
pub const MAP: crate::input::Act = crate::input::Act("map");

/// Tuning: `assets/data/game/map.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// The picture's size (px, square), and on the Low preset.
    pub image: u32,
    pub image_low: u32,
    /// Re-shoot the picture every this many frames while open.
    pub every: u32,
    /// The most and least ground (m) the map shows across.
    pub view_max: f32,
    pub view_min: f32,
    /// Camera height over your feet outdoors, and indoors (m).
    pub sky: f32,
    pub ceiling_clear: f32,
    /// How far (m) under your feet it sees, outdoors.
    pub depth: f32,
    /// Exposure (ev100), stops brighter at night, and the daytime ambient.
    pub ev100: f32,
    pub night_lift: f32,
    pub day_ambient: f32,
    /// Pin width (share of the map), yours bigger by, bob (share, per s).
    pub pin: f32,
    pub pin_mine: f32,
    pub bob: f32,
    pub bob_speed: f32,
    /// Pin colours by player id (wraps).
    pub pins: Vec<(f32, f32, f32)>,
    /// Paper, ink and the six palette colours the world is pulled toward.
    pub paper: (f32, f32, f32),
    pub ink: (f32, f32, f32),
    pub palette: Vec<(f32, f32, f32)>,
    /// Ink darkness, ink sensitivity, grain, bands, palette mix, edge rag.
    pub ink_strength: f32,
    pub ink_edge: f32,
    pub grain: f32,
    pub bands: f32,
    pub palette_mix: f32,
    pub edge: f32,
    /// At most this many site names at once.
    pub labels: usize,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/game/map.ron");

/// The map's systems (a screenshot scene opens it before them).
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct MapSet;

/// Is the map open, and what is it showing.
#[derive(Resource, Default, Debug)]
pub struct MapView {
    pub open: bool,
    /// The area it's framed on (labels are rebuilt when this changes).
    area: Option<String>,
    /// The ground it shows (x, z).
    view: Rect,
}

/// What M / Esc do to the map this frame. Opening the journal closes it.
pub fn keys_to_map(open: bool, m: bool, esc: bool, journal: bool) -> bool {
    if journal || (open && esc) {
        return false;
    }
    if m { !open } else { open }
}

/// The square of ground the map shows: the whole area when it fits in
/// `view_max` (centred on it), else a `view_max` window round `feet`, kept
/// inside the area. Never smaller than `view_min`.
pub fn frame(bounds: ((f32, f32), (f32, f32)), feet: Vec2, view_max: f32, view_min: f32) -> Rect {
    let min = Vec2::new(bounds.0.0, bounds.0.1);
    let max = Vec2::new(bounds.1.0, bounds.1.1);
    let span = max - min;
    let size = span.max_element().min(view_max).max(view_min);
    let half = size / 2.0;
    let axis = |lo: f32, hi: f32, p: f32| {
        if hi - lo <= size {
            (lo + hi) / 2.0
        } else {
            p.clamp(lo + half, hi - half)
        }
    };
    let c = Vec2::new(axis(min.x, max.x, feet.x), axis(min.y, max.y, feet.y));
    Rect::from_center_size(c, Vec2::splat(size))
}

/// Where `p` (x, z) falls on the map picture: (0, 0) top-left (north-west),
/// (1, 1) bottom-right. North (-z) is up.
pub fn on_map(view: Rect, p: Vec2) -> Vec2 {
    (p - view.min) / view.size().max(Vec2::splat(1e-3))
}

/// The camera's height band (top, bottom) over `feet`: indoors it sits just
/// over your head; a stacked area sees down to its floor.
pub fn band(t: &Tuning, feet_y: f32, indoors: bool, heights: Option<(f32, f32)>) -> (f32, f32) {
    let stacked = heights.is_some();
    let top = feet_y
        + if indoors || stacked {
            t.ceiling_clear
        } else {
            t.sky
        };
    let bottom = heights.map_or(feet_y - t.depth, |(lo, _)| lo.min(feet_y - 1.0));
    (top, bottom)
}

/// The paper-map look (`shaders/paper_map.wesl`).
#[derive(AsBindGroup, Asset, TypePath, Debug, Clone)]
pub struct PaperMap {
    #[uniform(0)]
    pub look: PaperLook,
    #[texture(1)]
    #[sampler(2)]
    pub view: Handle<Image>,
}

#[derive(ShaderType, Debug, Clone, Default)]
pub struct PaperLook {
    pub paper: Vec4,
    pub ink: Vec4,
    pub look: Vec4,
    pub more: Vec4,
    pub palette: [Vec4; 6],
}

impl PaperLook {
    fn from(t: &Tuning, px: u32) -> Self {
        let c = |(r, g, b): (f32, f32, f32)| Color::srgb(r, g, b).to_linear().to_vec4();
        let mut palette = [Vec4::ONE; 6];
        for (slot, p) in palette.iter_mut().zip(&t.palette) {
            *slot = c(*p);
        }
        Self {
            paper: c(t.paper),
            ink: c(t.ink),
            look: Vec4::new(t.ink_strength, t.ink_edge, t.grain, t.bands),
            more: Vec4::new(t.palette_mix, t.edge, 1.0 / px.max(1) as f32, 0.0),
            palette,
        }
    }
}

impl UiMaterial for PaperMap {
    fn fragment_shader() -> ShaderRef {
        "shaders/paper_map.wesl".into()
    }
}

#[derive(Component)]
struct Root;
#[derive(Component)]
struct Sheet;
#[derive(Component)]
struct Title;
/// A site's name on the map: where it is (x, z), its rank (nearer the
/// middle goes first when two would overlap) and its tag's width (px).
#[derive(Component)]
struct SiteLabel {
    at: Vec2,
    rank: usize,
    width: f32,
}

/// A name tag's height (px) and the width of `name` on it, roughly: the tag
/// is a fixed size so overlaps can be worked out before layout.
const TAG_H: f32 = 24.0;
fn tag_width(name: &str) -> f32 {
    name.chars().count() as f32 * 6.9 + 18.0
}

/// A cut-paper name tag: cream card, white rim, a little shadow.
fn tag(name: &str, width: f32) -> impl Bundle {
    (
        Node {
            width: px_val(width),
            height: px_val(TAG_H),
            border: UiRect::all(px_val(2.0)),
            border_radius: BorderRadius::all(px_val(7.0)),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(Color::srgb(0.99, 0.96, 0.88)),
        BorderColor::all(Color::WHITE),
        BoxShadow::new(
            INK.with_alpha(0.35),
            px_val(1.5),
            px_val(2.5),
            px_val(0.0),
            px_val(1.0),
        ),
        children![(Text::new(name.to_string()), body(13.0, INK))],
    )
}

/// Where each tag goes: in rank order, at its place if that's clear of the
/// tags already placed, else the nearest clear spot round it (below, above,
/// right, left, then further out), kept inside `bounds`. Nothing is hidden:
/// a tag with no clear spot stays at its place. Pins don't count; names may
/// sit over them.
pub fn declutter(tags: &[Rect], bounds: Rect) -> Vec<Rect> {
    let mut taken: Vec<Rect> = Vec::new();
    tags.iter()
        .map(|r| {
            let fit = |o: Vec2| {
                let mut c = r.center() + o;
                let h = r.half_size();
                c = c.clamp(bounds.min + h, bounds.max - h);
                Rect::from_center_half_size(c, h)
            };
            let h = r.height() + 3.0;
            let w = r.width() / 2.0 + 6.0;
            let mut tries = vec![Vec2::ZERO];
            for ring in 1..=4 {
                let k = ring as f32;
                tries.extend([
                    Vec2::new(0.0, h * k),
                    Vec2::new(0.0, -h * k),
                    Vec2::new(w * k, 0.0),
                    Vec2::new(-w * k, 0.0),
                    Vec2::new(w * k, h * k),
                    Vec2::new(-w * k, h * k),
                    Vec2::new(w * k, -h * k),
                    Vec2::new(-w * k, -h * k),
                ]);
            }
            let spot = tries
                .into_iter()
                .map(fit)
                .find(|c| taken.iter().all(|o| o.intersect(*c).is_empty()))
                .unwrap_or_else(|| fit(Vec2::ZERO));
            taken.push(spot.inflate(2.0));
            spot
        })
        .collect()
}
/// A player's pin and name on the map, following their body.
#[derive(Component)]
struct PlayerLabel(Entity);
pub fn plugin(app: &mut App) {
    app.add_plugins(UiMaterialPlugin::<PaperMap>::default())
        .init_resource::<MapView>()
        .add_systems(OnExit(AppState::InGame), |mut m: ResMut<MapView>| {
            *m = MapView::default();
        })
        .add_systems(
            Update,
            (
                spawn.run_if(not(any_with_component::<Root>)),
                keys,
                show,
                aim.after(crate::place::PlaceSet),
                pins,
                labels,
            )
                .chain()
                .in_set(MapSet)
                .run_if(in_state(AppState::InGame)),
        )
        .add_systems(PostUpdate, |m: Res<MapView>, mut o: ResMut<Overlay>| {
            if o.map != m.open {
                o.map = m.open;
            }
        });
}

fn spawn(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut papers: ResMut<Assets<PaperMap>>,
    quality: Option<Res<crate::graphics::GraphicsQuality>>,
) {
    let t = TUNING.get();
    let low = quality.is_some_and(|q| q.preset() == Some(crate::graphics::Preset::Low));
    let px = if low { t.image_low } else { t.image }.clamp(64, 2048);
    let image = images.add(Image::new_target_texture(
        px,
        px,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    let (r, g, b) = t.paper;
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: -10,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::srgb(r, g, b)),
            ..default()
        },
        RenderTarget::Image(image.clone().into()),
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::Fixed {
                width: 100.0,
                height: 100.0,
            },
            ..OrthographicProjection::default_3d()
        }),
        Exposure { ev100: t.ev100 },
        Msaa::Off,
        RenderLayers::from_layers(&[0, MAP_LAYER]),
        Transform::from_xyz(0.0, 100.0, 0.0).looking_to(Vec3::NEG_Y, Vec3::NEG_Z),
        MapCamera,
        crate::areas::Global,
        DespawnOnExit(AppState::InGame),
    ));
    let paper = papers.add(PaperMap {
        look: PaperLook::from(&t, px),
        view: image,
    });

    // The card: a parchment sheet in the middle of the 1280 x 720 HUD.
    let card = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px_val(330.0),
                top: px_val(12.0),
                width: px_val(620.0),
                height: px_val(696.0),
                border: UiRect::all(px_val(3.0)),
                border_radius: BorderRadius::all(px_val(22.0)),
                ..default()
            },
            BackgroundColor(Color::srgb(r * 0.96, g * 0.94, b * 0.9)),
            BorderColor::all(INK),
            children![
                (
                    Title,
                    Text::new(""),
                    head(size::H1, INK),
                    at(26.0, 12.0, 3.0)
                ),
                (
                    Node {
                        align_items: AlignItems::Center,
                        column_gap: px_val(8.0),
                        ..at(400.0, 22.0, 3.0)
                    },
                    crate::input::KeyRow::new(
                        crate::input::Hint("{map}   close".into()),
                        crate::input::KeyStyle::Card,
                    ),
                ),
            ],
        ))
        .id();
    let sheet = commands
        .spawn((
            Sheet,
            Node {
                width: px_val(580.0),
                height: px_val(580.0),
                overflow: Overflow::clip(),
                ..at(20.0, 92.0, 3.0)
            },
            MaterialNode(paper),
        ))
        .id();
    commands.entity(card).add_child(sheet);
    commands
        .spawn((
            Root,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                display: Display::None,
                ..default()
            },
            BackgroundColor(INK.with_alpha(0.35)),
            GlobalZIndex(29),
            DespawnOnExit(AppState::InGame),
        ))
        .add_child(card);
}

fn px_val(v: f32) -> Val {
    Val::Px(v)
}

fn keys(
    actions: crate::input::Actions,
    keys: Res<ButtonInput<KeyCode>>,
    overlay: Res<Overlay>,
    journal: Res<super::journal::Journal>,
    mut map: ResMut<MapView>,
) {
    let journal_up = journal.kept || keys.pressed(KeyCode::Tab);
    let esc = keys.just_pressed(KeyCode::Escape) && !overlay.intro && !overlay.settings;
    let open = keys_to_map(map.open, actions.just_pressed(MAP), esc, journal_up);
    if map.open != open {
        map.open = open;
    }
}

fn show(map: Res<MapView>, mut root: Query<&mut Node, With<Root>>) {
    let want = if map.open {
        Display::Flex
    } else {
        Display::None
    };
    for mut n in &mut root {
        if n.display != want {
            n.display = want;
        }
    }
}

/// Frame the camera on your area and re-shoot every few frames.
#[allow(clippy::too_many_arguments)]
fn aim(
    mut map: ResMut<MapView>,
    here: Res<crate::place::Here>,
    looks: Res<crate::place::PlaceLooks>,
    ambient: Option<Res<GlobalAmbientLight>>,
    mut frames: Local<u32>,
    mut cams: Query<(&mut Camera, &mut Transform, &mut Projection, &mut Exposure), With<MapCamera>>,
    mut title: Query<&mut Text, With<Title>>,
) {
    let Ok((mut cam, mut tf, mut proj, mut exposure)) = cams.single_mut() else {
        return;
    };
    if !map.open {
        if cam.is_active {
            cam.is_active = false;
        }
        return;
    }
    let t = TUNING.get();
    let feet = here.0;
    let list = crate::areas::areas();
    let id = crate::areas::area_at(feet).or_else(|| crate::areas::nearest_area(feet));
    let Some(area) = id
        .as_deref()
        .and_then(|id| list.areas.iter().find(|a| a.id == id))
    else {
        return;
    };
    if map.area.as_deref() != Some(area.id.as_str()) {
        map.area = Some(area.id.clone());
        for mut text in &mut title {
            text.0.clone_from(&area.name);
        }
    }
    let view = frame(area.bounds, feet.xz(), t.view_max, t.view_min);
    map.view = view;
    let indoors = looks.ids().contains(&crate::place::INDOORS);
    let (top, bottom) = band(&t, feet.y, indoors, area.heights);
    let c = view.center();
    let want = Transform::from_xyz(c.x, top, c.y).looking_to(Vec3::NEG_Y, Vec3::NEG_Z);
    if *tf != want {
        *tf = want;
    }
    if let Projection::Orthographic(o) = proj.as_mut() {
        let s = view.width();
        o.scaling_mode = ScalingMode::Fixed {
            width: s,
            height: s,
        };
        o.near = 0.0;
        o.far = (top - bottom).max(1.0);
    }
    // Night: a few stops brighter, so the map still reads.
    let amb = ambient.map_or(t.day_ambient, |a| a.brightness);
    let lift = (t.day_ambient / amb.max(1.0))
        .log2()
        .clamp(0.0, t.night_lift);
    let ev = t.ev100 - lift;
    if (exposure.ev100 - ev).abs() > 0.01 {
        exposure.ev100 = ev;
    }
    *frames = frames.wrapping_add(1);
    let shoot = (*frames).is_multiple_of(t.every.max(1)) || *frames == 1;
    if cam.is_active != shoot {
        cam.is_active = shoot;
    }
}

/// One cut-paper pin per player while the map is open: a disc in their
/// colour with a white rim and their name, drawn over the painted picture
/// (so the paint never swallows it). [`labels`] moves them.
fn pins(
    mut commands: Commands,
    map: Res<MapView>,
    me: Option<Res<LocalPlayer>>,
    players: Query<(Entity, &Player), With<Walker>>,
    sheet: Query<Entity, With<Sheet>>,
    pins: Query<(Entity, &PlayerLabel)>,
) {
    if !map.open {
        for (e, _) in &pins {
            commands.entity(e).despawn();
        }
        return;
    }
    let t = TUNING.get();
    let Ok(s) = sheet.single() else { return };
    let mine = me.map(|m| m.0);
    for (e, p) in &players {
        if pins.iter().any(|(_, l)| l.0 == e) {
            continue;
        }
        let (r, g, b) = t
            .pins
            .get(p.id as usize % t.pins.len().max(1))
            .copied()
            .unwrap_or((1.0, 0.3, 0.4));
        let is_mine = Some(p.id) == mine;
        let d = pin_px(&t, is_mine);
        commands.entity(s).with_child((
            PlayerLabel(e),
            Node {
                position_type: PositionType::Absolute,
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                width: px_val(120.0),
                ..default()
            },
            children![
                (
                    Node {
                        width: px_val(d),
                        height: px_val(d),
                        border: UiRect::all(px_val(3.0)),
                        border_radius: BorderRadius::all(px_val(d)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(r, g, b)),
                    BorderColor::all(Color::WHITE),
                    BoxShadow::new(
                        INK.with_alpha(0.45),
                        px_val(2.0),
                        px_val(3.0),
                        px_val(0.0),
                        px_val(1.0),
                    ),
                ),
                tag(
                    &super::team::name_for(p.id, is_mine),
                    tag_width(&super::team::name_for(p.id, is_mine)),
                ),
            ],
        ));
    }
    for (e, l) in &pins {
        if players.get(l.0).is_err() {
            commands.entity(e).despawn();
        }
    }
}

/// A pin's width on screen (px of the 1280 x 720 HUD).
fn pin_px(t: &Tuning, mine: bool) -> f32 {
    580.0 * t.pin * if mine { t.pin_mine } else { 1.0 }
}

/// Site names (rebuilt when the area changes) and player names, placed
/// where they are on the picture.
#[allow(clippy::type_complexity)]
fn labels(
    mut commands: Commands,
    map: Res<MapView>,
    mut built: Local<Option<String>>,
    sheet: Query<Entity, With<Sheet>>,
    sites: Query<(Entity, &SiteLabel)>,
    players: Query<&Walker>,
    time: Res<Time>,
    me: Option<Res<LocalPlayer>>,
    owners: Query<(Entity, &Player)>,
    mut nodes: ParamSet<(
        Query<(&SiteLabel, &mut Node)>,
        Query<(&PlayerLabel, &mut Node)>,
    )>,
) {
    if !map.open {
        return;
    }
    let Ok(s) = sheet.single() else { return };
    let mine = me
        .and_then(|m| owners.iter().find(|(_, p)| p.id == m.0))
        .map(|(e, _)| e);
    if *built != map.area {
        built.clone_from(&map.area);
        for (e, _) in &sites {
            commands.entity(e).despawn();
        }
        let all = crate::sites::sites();
        let mut here: Vec<_> = all
            .sites
            .iter()
            .filter(|site| map.area.as_deref() == Some(site.area.as_str()))
            .map(|site| (site.name.clone(), site.middle()))
            .collect();
        let mid = map.view.center();
        here.sort_by(|a, b| a.1.distance(mid).total_cmp(&b.1.distance(mid)));
        for (rank, (name, at)) in here.into_iter().take(TUNING.get().labels).enumerate() {
            let width = tag_width(&name);
            commands.entity(s).with_child((
                SiteLabel { at, rank, width },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                children![tag(&name, width)],
            ));
        }
    }
    let side = 580.0;
    let place = |p: Vec2, n: &mut Node, dy: f32| {
        let uv = on_map(map.view, p);
        let inside = (0.02..0.92).contains(&uv.x) && (0.02..0.97).contains(&uv.y);
        let want = if inside { Display::Flex } else { Display::None };
        if n.display != want {
            n.display = want;
        }
        let (l, t) = (Val::Px(uv.x * side), Val::Px(uv.y * side + dy));
        if n.left != l || n.top != t {
            n.left = l;
            n.top = t;
        }
    };
    let t = TUNING.get();
    let bob = (time.elapsed_secs() * t.bob_speed).sin() * t.bob * 10.0;
    for (label, mut n) in &mut nodes.p1() {
        if let Ok(w) = players.get(label.0) {
            let d = pin_px(&t, Some(label.0) == mine);
            place(w.pos.xz(), &mut n, -d / 2.0 + bob);
            n.margin.left = px_val(-60.0);
        }
    }
    // Site names: centred on their place, shown nearest-first where they fit.
    let mut q = nodes.p0();
    let mut tags: Vec<_> = q.iter_mut().collect();
    tags.sort_by_key(|(l, _)| l.rank);
    let rects: Vec<Rect> = tags
        .iter()
        .map(|(l, _)| {
            let c = on_map(map.view, l.at) * side;
            Rect::from_center_size(c, Vec2::new(l.width, TAG_H))
        })
        .collect();
    let spots = declutter(&rects, Rect::new(4.0, 4.0, side - 4.0, side - 4.0));
    for ((_, n), r) in tags.iter_mut().zip(&spots) {
        let (l, top) = (Val::Px(r.min.x), Val::Px(r.min.y));
        if n.left != l || n.top != top {
            n.left = l;
            n.top = top;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn m_toggles_esc_and_the_journal_close() {
        assert!(keys_to_map(false, true, false, false));
        assert!(!keys_to_map(true, true, false, false));
        assert!(!keys_to_map(true, false, true, false));
        assert!(keys_to_map(true, false, false, false));
        assert!(!keys_to_map(true, false, false, true), "the journal wins");
        assert!(!keys_to_map(false, true, false, true));
    }

    #[test]
    fn small_areas_fit_whole_big_ones_scroll_and_clamp() {
        let commune = ((-110.0, -110.0), (110.0, 110.0));
        let v = frame(commune, Vec2::new(90.0, 0.0), 240.0, 30.0);
        assert_eq!(v.center(), Vec2::ZERO);
        assert_eq!(v.width(), 220.0);
        // The surf course is far bigger than the window: it follows you...
        let surf = ((3840.0, -2250.0), (5950.0, 145.0));
        let v = frame(surf, Vec2::new(5000.0, -1000.0), 240.0, 30.0);
        assert_eq!(v.width(), 240.0);
        assert_eq!(v.center(), Vec2::new(5000.0, -1000.0));
        // ...but never past its edge.
        let v = frame(surf, Vec2::new(3841.0, 140.0), 240.0, 30.0);
        assert_eq!(v.min.x, 3840.0);
        assert_eq!(v.max.y, 145.0);
        // A tiny place is never drawn smaller than view_min.
        let v = frame(((0.0, 0.0), (5.0, 5.0)), Vec2::ZERO, 240.0, 30.0);
        assert_eq!(v.width(), 30.0);
    }

    #[test]
    fn tags_move_aside_instead_of_hiding() {
        let b = Rect::new(0.0, 0.0, 580.0, 580.0);
        let near = Rect::new(100.0, 100.0, 200.0, 124.0);
        let clash = Rect::new(150.0, 110.0, 250.0, 134.0);
        let off = Rect::new(560.0, 10.0, 640.0, 34.0);
        let out = declutter(&[near, clash, off], b);
        assert_eq!(out[0], near, "the nearest stays put");
        assert!(out[0].intersect(out[1]).is_empty(), "the next moves aside");
        assert!(
            b.contains(out[2].min) && b.contains(out[2].max),
            "kept on the sheet"
        );
    }

    #[test]
    fn north_is_up() {
        let view = Rect::new(-10.0, -10.0, 10.0, 10.0);
        assert_eq!(on_map(view, Vec2::new(-10.0, -10.0)), Vec2::ZERO);
        assert_eq!(on_map(view, Vec2::new(10.0, 10.0)), Vec2::ONE);
    }

    #[test]
    fn indoors_cuts_the_ceiling_and_stacked_sees_its_floor() {
        let t = Tuning {
            sky: 80.0,
            ceiling_clear: 2.6,
            depth: 80.0,
            ..default()
        };
        assert_eq!(band(&t, 5.0, false, None), (85.0, -75.0));
        assert_eq!(band(&t, 5.0, true, None), (7.6, -75.0));
        assert_eq!(band(&t, -50.0, false, Some((-64.0, -46.0))), (-47.4, -64.0));
    }

    #[test]
    fn tuning_parses_with_every_field() {
        let path = format!("{}/assets/{}", env!("CARGO_MANIFEST_DIR"), TUNING.path());
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        let t = TUNING.get();
        assert!(t.image > 0 && t.image_low > 0 && t.every > 0);
        assert_eq!(t.palette.len(), 6, "the shader takes six swatches");
        assert!(!t.pins.is_empty());
    }
}
