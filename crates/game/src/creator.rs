//! Character creator (#100 by doomy): the main menu's "Character" button opens
//! a card of cyclers and sliders for your [`CharacterLook`] next to a live,
//! slowly turning 3D preview. The look is saved to `character.txt` in the
//! config folder (see [`crate::settings::config_path`]) and sent to the server
//! whenever you're in a game, which clamps it and replicates it to everyone.

use bevy::camera::Viewport;
use bevy::picking::hover::Hovered;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;

use crate::AppState;
use crate::menu::{self, BaseColor, SettingsPanel};
use crate::people::{CharacterLook, Field, Gait, SetLook};
use crate::player::{LocalPlayer, Player};

const FILE: &str = "character.txt";
/// The preview stands far above the menu meadow, out of the orbit camera's view.
const PREVIEW_AT: Vec3 = Vec3::new(0.0, 300.0, 0.0);

/// The look I made (or rolled). Loaded at startup, saved when edited.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct MyLook(pub CharacterLook);

impl FromWorld for MyLook {
    fn from_world(_: &mut World) -> Self {
        let path = crate::settings::config_path(FILE);
        if let Some(text) = path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
            return Self(CharacterLook::from_text(&text));
        }
        // First launch: roll one from the shared pool and keep it.
        let look = CharacterLook::random(clock_seed());
        save(&look);
        Self(look)
    }
}

/// Whether the creator card is open (the menu's Character button flips it).
#[derive(Resource, Default)]
pub struct CreatorOpen(pub bool);

#[derive(Component)]
struct CreatorPanel;

#[derive(Component)]
struct CreatorValue(Field);

#[derive(Component)]
struct PreviewCamera;

#[derive(Component)]
struct Preview;

pub fn plugin(app: &mut App) {
    app.init_resource::<MyLook>()
        .init_resource::<CreatorOpen>()
        .add_systems(OnEnter(AppState::Menu), spawn_creator)
        .add_systems(OnExit(AppState::Menu), |mut open: ResMut<CreatorOpen>| {
            open.0 = false;
        })
        .add_systems(
            Update,
            (show_creator, place_preview, sync_preview, refresh_values)
                .chain()
                .run_if(in_state(AppState::Menu)),
        )
        .add_systems(Update, save_on_edit)
        .add_systems(Update, send_my_look.run_if(in_state(AppState::InGame)));
}

fn clock_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(1, |d| d.as_nanos() as u64)
}

fn save(look: &CharacterLook) {
    let Some(path) = crate::settings::config_path(FILE) else {
        return;
    };
    if let Some(dir) = path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        warn!("can't create {}: {e}", dir.display());
        return;
    }
    if let Err(e) = std::fs::write(&path, look.to_text()) {
        warn!("can't save {}: {e}", path.display());
    }
}

fn save_on_edit(look: Res<MyLook>) {
    if look.is_changed() && !look.is_added() {
        save(&look.0);
    }
}

/// While in a game, make sure the server has my look (host included: its
/// trigger arrives as `ClientId::Server`). Retries once a second until the
/// replicated look on my player matches.
fn send_my_look(
    mut commands: Commands,
    time: Res<Time>,
    mine: Res<MyLook>,
    me: Option<Res<LocalPlayer>>,
    players: Query<(&Player, Option<&CharacterLook>)>,
    mut cooldown: Local<f32>,
) {
    *cooldown -= time.delta_secs();
    let Some(me) = me else {
        return;
    };
    let want = mine.0.clamped();
    let has = players
        .iter()
        .find(|(p, _)| p.id == me.0)
        .and_then(|(_, l)| l.copied());
    if has == Some(want) || *cooldown > 0.0 {
        return;
    }
    *cooldown = 1.0;
    commands.client_trigger(SetLook(want));
}

fn small_button<'a>(
    ui: &'a mut ChildSpawnerCommands,
    label: &str,
    color: Color,
    width: f32,
) -> EntityCommands<'a> {
    let mut e = ui.spawn((
        Node {
            width: px(width),
            padding: UiRect::new(px(8), px(8), px(3), px(2)),
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(px(999)),
            border: UiRect::new(px(1.0), px(1.0), px(1.0), px(3.0)),
            ..default()
        },
        BackgroundColor(color),
        BorderColor::all(crate::theme::shade(color, 0.28)),
        crate::theme::shadow(2.0, 6.0),
        BaseColor(color),
        Hovered::default(),
        crate::theme::Squishy::default(),
        UiTransform::default(),
    ));
    e.with_children(|b| {
        b.spawn((
            Text::new(label),
            TextFont::from_font_size(15.0),
            TextColor(Color::WHITE),
        ));
    });
    e
}

fn spawn_creator(
    mut commands: Commands,
    look: Res<MyLook>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let out = DespawnOnExit(AppState::Menu);
    // The preview: a little stage high above the meadow with its own camera.
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 1,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.93, 0.88, 0.98)),
            ..default()
        },
        Transform::from_translation(PREVIEW_AT + Vec3::new(0.0, 1.3, 4.3))
            .looking_at(PREVIEW_AT + Vec3::Y * 0.85, Vec3::Y),
        PreviewCamera,
        out.clone(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.9, 0.1))),
        MeshMaterial3d(crate::art::soft(
            &mut materials,
            Color::srgb(0.62, 0.8, 0.55),
        )),
        Transform::from_translation(PREVIEW_AT - Vec3::Y * 0.05),
        out.clone(),
    ));
    commands.spawn((
        Preview,
        Gait::default(),
        look.0,
        Transform::from_translation(PREVIEW_AT),
        Visibility::default(),
        out.clone(),
    ));

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: px(24),
                top: px(24),
                bottom: px(24),
                width: px(380),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(px(20), px(16)),
                row_gap: px(5),
                border_radius: BorderRadius::all(px(28)),
                border: UiRect::all(px(2)),
                display: Display::None,
                overflow: Overflow::clip_y(),
                ..default()
            },
            BackgroundColor(menu::CARD),
            BorderColor::all(crate::theme::LINE),
            crate::theme::shadow(10.0, 32.0),
            crate::theme::Pop::default(),
            crate::theme::PopOnShow::default(),
            UiTransform::default(),
            CreatorPanel,
            out,
        ))
        .with_children(|ui| {
            ui.spawn((
                Text::new("Your character"),
                TextFont::from_font_size(28.0),
                TextColor(menu::INK),
                crate::assets::DisplayText,
            ));
            menu::text(
                ui,
                "everyone sees this; villagers roll from the same pool",
                12.0,
                menu::SOFT_INK,
            );
            for field in Field::ALL {
                menu::row(ui, |r| {
                    r.spawn((
                        Text::new(field.label()),
                        TextFont::from_font_size(15.0),
                        TextColor(menu::INK),
                        Node {
                            width: px(105),
                            ..default()
                        },
                    ));
                    small_button(r, "<", menu::LILAC, 34.0).observe(
                        move |_: On<PointerClick>, mut look: ResMut<MyLook>| {
                            look.0.nudge(field, -1);
                        },
                    );
                    r.spawn((
                        Text::new(""),
                        TextFont::from_font_size(15.0),
                        TextColor(menu::INK),
                        TextLayout::justify(Justify::Center),
                        Node {
                            width: px(120),
                            ..default()
                        },
                        CreatorValue(field),
                    ));
                    small_button(r, ">", menu::LILAC, 34.0).observe(
                        move |_: On<PointerClick>, mut look: ResMut<MyLook>| {
                            look.0.nudge(field, 1);
                        },
                    );
                });
            }
            menu::row(ui, |r| {
                small_button(r, "Randomize", menu::PINK, 150.0).observe(
                    |_: On<PointerClick>, mut look: ResMut<MyLook>| {
                        look.0 = CharacterLook::random(clock_seed());
                    },
                );
                small_button(r, "Done", menu::MINT, 150.0).observe(
                    |_: On<PointerClick>, mut open: ResMut<CreatorOpen>| {
                        open.0 = false;
                    },
                );
            });
        });
}

/// Open/close the card; it and the settings card take turns.
fn show_creator(
    mut open: ResMut<CreatorOpen>,
    mut panels: Query<&mut Node, (With<CreatorPanel>, Without<SettingsPanel>)>,
    mut settings: Query<&mut Node, (With<SettingsPanel>, Without<CreatorPanel>)>,
) {
    let settings_opened = settings
        .iter_mut()
        .any(|n| n.is_changed() && !n.is_added() && n.display != Display::None);
    if settings_opened && open.0 && !open.is_changed() {
        open.0 = false;
    }
    if !open.is_changed() {
        return;
    }
    let want = if open.0 { Display::Flex } else { Display::None };
    for mut n in &mut panels {
        n.display = want;
    }
    if open.0 {
        for mut n in &mut settings {
            n.display = Display::None;
        }
    }
}

/// Fit the preview camera between the menu card and the creator card.
fn place_preview(
    open: Res<CreatorOpen>,
    window: Query<&Window, With<PrimaryWindow>>,
    mut cams: Query<&mut Camera, With<PreviewCamera>>,
) {
    let Ok(window) = window.single() else {
        return;
    };
    let scale = window.scale_factor();
    let (w, h) = (window.width(), window.height());
    let x0 = 520.0f32.min(w * 0.4);
    let x1 = (w - 420.0).max(x0 + 120.0).min(w);
    let (y0, y1) = (60.0f32, (h - 60.0).max(180.0).min(h));
    let viewport = Viewport {
        physical_position: UVec2::new((x0 * scale) as u32, (y0 * scale) as u32),
        physical_size: UVec2::new(
            (((x1 - x0) * scale) as u32).max(1),
            (((y1 - y0) * scale) as u32).max(1),
        ),
        ..default()
    };
    for mut cam in &mut cams {
        let active = open.0 && x1 - x0 >= 120.0;
        if cam.is_active != active {
            cam.is_active = active;
        }
        let same = cam.viewport.as_ref().is_some_and(|v| {
            v.physical_position == viewport.physical_position
                && v.physical_size == viewport.physical_size
        });
        if !same {
            cam.viewport = Some(viewport.clone());
        }
    }
}

/// Copy edits onto the preview body and turn it slowly.
fn sync_preview(
    time: Res<Time>,
    look: Res<MyLook>,
    mut preview: Query<(&mut CharacterLook, &mut Transform), With<Preview>>,
) {
    for (mut l, mut tf) in &mut preview {
        l.set_if_neq(look.0);
        tf.rotation = Quat::from_rotation_y(time.elapsed_secs() * 0.6);
    }
}

fn refresh_values(look: Res<MyLook>, mut values: Query<(&mut Text, &CreatorValue)>) {
    let mut l = look.0;
    for (mut t, v) in &mut values {
        let want = l.describe(v.0);
        if t.0 != want {
            t.0 = want;
        }
    }
}
