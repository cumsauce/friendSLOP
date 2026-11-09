//! Main menu (#25): a cute clickable menu to host, join with a code, and change
//! a few settings. `friendslop host` / `friendslop join <code>` still skip it.
//!
//! Hosting alone is fine: you're simply the only player until friends join.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use bevy::picking::hover::Hovered;
use bevy_replicon::prelude::*;
use bevy_replicon_iroh::IrohClient;

use crate::AppState;
use crate::hud::Toast;
use crate::net::{Clipboard, NetCommand, NetStatus};
use crate::player::LocalPlayer;
use crate::scenery;
use crate::settings::Settings;
use crate::voice::VoiceMuted;

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<JoinCode>()
            .init_resource::<Connecting>()
            .add_systems(OnEnter(AppState::Menu), (spawn_menu, spawn_backdrop))
            .add_systems(
                Update,
                (
                    type_join_code,
                    start_pending_join,
                    connect_timeout,
                    refresh_labels,
                    hover_buttons,
                    orbit_camera,
                    backdrop_fallbacks,
                    menu_font,
                    menu_shortcuts,
                    style_status,
                )
                    .run_if(in_state(AppState::Menu)),
            )
            .add_systems(
                Update,
                (leave_on_esc, copy_code_again).run_if(in_state(AppState::InGame)),
            );
    }
}

/// Joining in progress. The join is sent a couple of frames after the click so
/// "Connecting..." is drawn before the (blocking) connect starts, then we give
/// the host [`Connecting::TIMEOUT_SECS`] to answer.
#[derive(Resource, Default, Debug)]
pub struct Connecting {
    pending: Option<(String, u8)>,
    waited: f32,
}

impl Connecting {
    pub const TIMEOUT_SECS: f32 = 20.0;
}

/// What's typed (or pasted) into the join box.
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct JoinCode(pub String);

impl JoinCode {
    /// Longest code we accept in the box (iroh ids are 64 hex chars).
    pub const MAX_LEN: usize = 128;

    /// Apply one key press. Returns true if Enter was pressed.
    pub fn type_key(&mut self, key: &Key) -> bool {
        match key {
            Key::Character(c) => {
                // Codes are hex: anything else (a stray 'm' from the mute
                // key, say) would only make the code invalid.
                for ch in c.chars().filter(char::is_ascii_hexdigit) {
                    if self.0.len() < Self::MAX_LEN {
                        self.0.push(ch);
                    }
                }
            }
            Key::Backspace => {
                self.0.pop();
            }
            Key::Enter => return true,
            _ => {}
        }
        false
    }

    /// Replace the box with pasted text, trimmed.
    pub fn paste(&mut self, text: &str) {
        // Same rule as typing: codes are hex, drop anything else.
        self.0 = text
            .trim()
            .chars()
            .filter(char::is_ascii_hexdigit)
            .take(Self::MAX_LEN)
            .collect();
    }
}

const CARD: Color = Color::srgba(1.0, 0.98, 1.0, 0.88);
const FIELD: Color = Color::srgb(1.0, 0.92, 0.96);
const PINK: Color = Color::srgb(1.0, 0.55, 0.76);
const LILAC: Color = Color::srgb(0.7, 0.6, 0.98);
const MINT: Color = Color::srgb(0.42, 0.8, 0.68);
const GREY: Color = Color::srgb(0.6, 0.55, 0.62);
const INK: Color = Color::srgb(0.42, 0.24, 0.4);
const SOFT_INK: Color = Color::srgb(0.6, 0.45, 0.58);
const OK: Color = Color::srgb(0.3, 0.62, 0.45);
const ERR: Color = Color::srgb(0.85, 0.3, 0.38);

#[derive(Component, Clone, Copy)]
enum Label {
    Status,
    Code,
    Master,
    Voice,
    Mic,
    HaltName,
    JoinButton,
}

/// A button's resting color, brightened while hovered.
#[derive(Component, Clone, Copy)]
struct BaseColor(Color);

/// The settings card, toggled by the Settings button (or Tab).
#[derive(Component)]
struct SettingsPanel;

/// The pill behind the status line (tinted by what the message says).
#[derive(Component)]
struct StatusPill;

/// The backdrop camera, orbiting the little meadow.
#[derive(Component)]
struct MenuCamera;

fn spawn_menu(mut commands: Commands) {
    commands
        .spawn((
            Node {
                width: percent(100),
                height: percent(100),
                padding: UiRect::axes(px(56), px(40)),
                column_gap: px(24),
                align_items: AlignItems::Center,
                ..default()
            },
            DespawnOnExit(AppState::Menu),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(px(36), px(28)),
                    row_gap: px(12),
                    border_radius: BorderRadius::all(px(32)),
                    width: px(440),
                    ..default()
                },
                BackgroundColor(CARD),
                BoxShadow::new(
                    Color::srgba(0.4, 0.2, 0.4, 0.25),
                    px(0),
                    px(8),
                    px(0),
                    px(28),
                ),
            ))
            .with_children(|ui| {
                ui.spawn((
                    Text::new("friendSLOP"),
                    TextFont::from_font_size(72.0),
                    TextColor(PINK),
                    TextShadow {
                        offset: Vec2::new(3.0, 4.0),
                        color: Color::srgba(0.45, 0.25, 0.5, 0.35),
                    },
                ));
                text(
                    ui,
                    "a game made by friends, built by robots",
                    16.0,
                    SOFT_INK,
                );
                spacer(ui, 6.0);

                button(ui, "Host a game", PINK, 320.0).observe(
                    |_: On<PointerClick>,
                     mut net: MessageWriter<NetCommand>,
                     mut connecting: ResMut<Connecting>| {
                        connecting.pending = None;
                        net.write(NetCommand::Host);
                    },
                );
                text(
                    ui,
                    "works solo too; the join code gets copied for friends",
                    13.0,
                    SOFT_INK,
                );
                spacer(ui, 6.0);

                text(ui, "Join a friend", 20.0, INK);
                ui.spawn((
                    Node {
                        width: px(360),
                        min_height: px(44),
                        padding: UiRect::axes(px(14), px(8)),
                        border_radius: BorderRadius::all(px(14)),
                        border: UiRect::all(px(2)),
                        justify_content: JustifyContent::Center,
                        align_items: AlignItems::Center,
                        ..default()
                    },
                    BackgroundColor(FIELD),
                    BorderColor::all(LILAC),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new(""),
                        TextFont::from_font_size(13.0),
                        TextColor(INK),
                        TextLayout::linebreak(LineBreak::AnyCharacter),
                        Label::Code,
                    ));
                });
                row(ui, |r| {
                    button(r, "Paste", LILAC, 100.0).observe(
                        |_: On<PointerClick>, clip: Res<Clipboard>, mut code: ResMut<JoinCode>| {
                            if let Some(text) = clip.get() {
                                code.paste(&text);
                            }
                        },
                    );
                    button(r, "Clear", LILAC, 100.0)
                        .observe(|_: On<PointerClick>, mut code: ResMut<JoinCode>| code.0.clear());
                    button(r, "Join!", PINK, 140.0)
                        .insert(Label::JoinButton)
                        .observe(
                            |_: On<PointerClick>,
                             code: Res<JoinCode>,
                             mut connecting: ResMut<Connecting>,
                             mut status: ResMut<NetStatus>,
                             client: Option<Res<IrohClient>>| {
                                join(&code, &mut connecting, &mut status, client.is_some())
                            },
                        );
                });
                text(
                    ui,
                    "type the code, Ctrl/Cmd+V pastes, Enter joins",
                    13.0,
                    SOFT_INK,
                );

                ui.spawn((
                    Node {
                        padding: UiRect::axes(px(14), px(6)),
                        border_radius: BorderRadius::all(px(999)),
                        max_width: px(380),
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(FIELD),
                    StatusPill,
                ))
                .with_children(|p| {
                    p.spawn((
                        Text::new(""),
                        TextFont::from_font_size(15.0),
                        TextColor(INK),
                        Label::Status,
                    ));
                });
                spacer(ui, 4.0);

                row(ui, |r| {
                    button(r, "Settings", MINT, 160.0).observe(
                        |_: On<PointerClick>, mut panel: Query<&mut Node, With<SettingsPanel>>| {
                            toggle_settings(&mut panel);
                        },
                    );
                    button(r, "Quit", GREY, 160.0).observe(
                        |_: On<PointerClick>, mut exit: MessageWriter<AppExit>| {
                            exit.write(AppExit::Success);
                        },
                    );
                });
            });

            root.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::FlexStart,
                    padding: UiRect::axes(px(28), px(22)),
                    row_gap: px(10),
                    border_radius: BorderRadius::all(px(28)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(CARD),
                SettingsPanel,
            ))
            .with_children(|ui| {
                text(ui, "Settings", 26.0, INK);
                volume_row(ui, "Master volume", Label::Master, |s| &mut s.master_volume);
                volume_row(ui, "Voice volume", Label::Voice, |s| &mut s.voice_volume);
                row(ui, |r| {
                    text(r, "Microphone (or press M)", 17.0, INK);
                    button(r, "", MINT, 110.0).insert(Label::Mic).observe(
                        |_: On<PointerClick>, mut muted: ResMut<VoiceMuted>| {
                            muted.0 = !muted.0;
                        },
                    );
                });
                row(ui, |r| {
                    text(r, "Let HALT use my username", 17.0, INK);
                    button(r, "", MINT, 110.0).insert(Label::HaltName).observe(
                        |_: On<PointerClick>, mut s: ResMut<Settings>| {
                            s.halt_real_name = !s.halt_real_name;
                        },
                    );
                });
            });
        });
}

fn toggle_settings(panel: &mut Query<&mut Node, With<SettingsPanel>>) {
    for mut node in panel.iter_mut() {
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// A tiny diorama of the meadow behind the menu: the real ground and grass
/// meshes, the pond with lily pads, a ring of the real CC0 trees, bushes and
/// rocks, the campfire with a few commune people, and bunting, seen by a
/// slowly orbiting camera. Shares its pipelines with the game, so it doubles as
/// a shader warm-up.
fn spawn_backdrop(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    assets: Res<AssetServer>,
    mut kit: ResMut<crate::people::PeopleKit>,
) {
    let out = DespawnOnExit(AppState::Menu);
    let horizon = crate::atmosphere::horizon_colour();
    commands.spawn((
        Camera3d::default(),
        Camera {
            clear_color: ClearColorConfig::Custom(horizon),
            ..default()
        },
        bevy::core_pipeline::tonemapping::Tonemapping::TonyMcMapface,
        DistanceFog {
            color: horizon,
            falloff: FogFalloff::Linear {
                start: 30.0,
                end: 90.0,
            },
            ..default()
        },
        orbit(0.0),
        MenuCamera,
        out.clone(),
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.78, 0.86, 1.0),
        brightness: 500.0,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 5_000.0,
            color: Color::srgb(1.0, 0.93, 0.82),
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(20.0, 30.0, 10.0).looking_at(Vec3::ZERO, Vec3::Y),
        out.clone(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(crate::terrain::build_mesh())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(crate::art::grass_texture(&mut images)),
            perceptual_roughness: 0.95,
            reflectance: 0.12,
            ..default()
        })),
        out.clone(),
    ));
    commands.spawn((
        Mesh3d(meshes.add(crate::terrain::build_tufts())),
        MeshMaterial3d(materials.add(StandardMaterial {
            perceptual_roughness: 0.9,
            double_sided: true,
            cull_mode: None,
            ..default()
        })),
        bevy::light::NotShadowCaster,
        out.clone(),
    ));
    let pond = crate::terrain::POND_AT;
    commands.spawn((
        Mesh3d(meshes.add(Circle::new(3.4).mesh().resolution(40))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgba(0.3, 0.55, 0.65, 0.7),
            alpha_mode: AlphaMode::Blend,
            perceptual_roughness: 0.08,
            reflectance: 0.6,
            ..default()
        })),
        Transform::from_xyz(pond.x, 0.03, pond.y)
            .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)),
        out.clone(),
    ));

    // Real CC0 trees, bushes and rocks in a loose ring (deterministic), with a
    // primitive stand-in only if a model fails to load (LFS data missing).
    for i in 0..18u32 {
        let a = i as f32 * 2.399;
        let r = 8.0 + (i * 7 % 8) as f32;
        let at = Vec2::new(a.cos() * r, a.sin() * r);
        if at.distance(pond) < 5.0 {
            continue;
        }
        let (file, scale, kind) = match i % 3 {
            0 | 1 => (
                scenery::TREES[i as usize % scenery::TREES.len()],
                0.6 + (i % 4) as f32 * 0.12,
                Stand::Tree,
            ),
            _ => (
                scenery::BUSHES[i as usize % scenery::BUSHES.len()],
                0.9,
                Stand::Bush,
            ),
        };
        backdrop_model(&mut commands, &assets, file, at, a * 3.0, scale, kind);
    }
    for (i, at) in [Vec2::new(-9.0, 6.0), Vec2::new(5.0, -11.0)]
        .into_iter()
        .enumerate()
    {
        let file = scenery::ROCKS[i % scenery::ROCKS.len()];
        backdrop_model(
            &mut commands,
            &assets,
            file,
            at,
            i as f32,
            0.55,
            Stand::Rock,
        );
    }

    // The campfire with its glow, two log seats and three commune people idling.
    scenery::spawn_bonfire(&mut commands, &assets, Vec3::ZERO, AppState::Menu);
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.6, 0.25),
            intensity: 25_000.0,
            range: 9.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(0.0, 0.8, 0.0),
        out.clone(),
    ));
    let log = meshes.add(Cylinder::new(0.18, 1.4));
    let bark = crate::art::soft(&mut materials, Color::srgb(0.45, 0.32, 0.22));
    for yaw in [0.6f32, 2.9] {
        let p = Vec2::from_angle(yaw) * 2.0;
        commands.spawn((
            Mesh3d(log.clone()),
            MeshMaterial3d(bark.clone()),
            Transform::from_xyz(p.x, 0.18, p.y).with_rotation(
                Quat::from_rotation_y(-yaw) * Quat::from_rotation_x(std::f32::consts::FRAC_PI_2),
            ),
            out.clone(),
        ));
    }
    for (i, ang) in [0.2f32, 2.2, 4.3].into_iter().enumerate() {
        let p = Vec2::from_angle(ang) * 2.6;
        // Face the fire (bodies look along -Z).
        let facing = Transform::from_xyz(p.x, 0.0, p.y).looking_at(Vec3::ZERO, Vec3::Y);
        let root = commands
            .spawn((
                crate::people::Gait::default(),
                facing,
                Visibility::default(),
                out.clone(),
            ))
            .id();
        commands.entity(root).with_children(|b| {
            crate::people::spawn_person(
                b,
                root,
                &mut kit,
                &mut meshes,
                &mut materials,
                crate::people::Look::for_index(i as u64 * 2 + 1, 4242 + i as u64 * 7),
            );
        });
    }

    // Bunting between two poles behind the fire.
    let pole = meshes.add(Cylinder::new(0.05, 2.6));
    let pole_mat = crate::art::soft(&mut materials, Color::srgb(0.36, 0.25, 0.15));
    let flag = meshes.add(Triangle3d::new(
        Vec3::new(-0.16, 0.0, 0.0),
        Vec3::new(0.16, 0.0, 0.0),
        Vec3::new(0.0, -0.32, 0.0),
    ));
    let flags = [
        Color::srgb(0.95, 0.45, 0.45),
        Color::srgb(0.98, 0.8, 0.35),
        Color::srgb(0.5, 0.75, 0.95),
        Color::srgb(0.6, 0.85, 0.5),
        Color::srgb(0.95, 0.6, 0.85),
    ]
    .map(|c| {
        materials.add(StandardMaterial {
            base_color: c,
            double_sided: true,
            cull_mode: None,
            perceptual_roughness: 0.9,
            ..default()
        })
    });
    let (l, r) = (Vec2::new(-4.0, -3.0), Vec2::new(3.5, -4.5));
    for p in [l, r] {
        commands.spawn((
            Mesh3d(pole.clone()),
            MeshMaterial3d(pole_mat.clone()),
            Transform::from_xyz(p.x, 1.3, p.y),
            out.clone(),
        ));
    }
    let along = (r - l).normalize_or(Vec2::X);
    let yaw = Quat::from_rotation_y(-along.to_angle());
    const FLAGS: usize = 14;
    for k in 0..FLAGS {
        let t = (k as f32 + 0.5) / FLAGS as f32;
        let xz = l.lerp(r, t);
        let y = 2.5 - 4.0 * t * (1.0 - t) * 0.5;
        commands.spawn((
            Mesh3d(flag.clone()),
            MeshMaterial3d(flags[k % flags.len()].clone()),
            Transform::from_xyz(xz.x, y, xz.y).with_rotation(yaw),
            bevy::light::NotShadowCaster,
            out.clone(),
        ));
    }

    // Lily pads with a pink bloom on the pond.
    let pad = meshes.add(CircularSector::new(0.35, std::f32::consts::PI * 0.9));
    let pad_mat = crate::art::soft(&mut materials, Color::srgb(0.3, 0.6, 0.25));
    let bloom = meshes.add(Sphere::new(0.09));
    let pink = crate::art::soft(&mut materials, Color::srgb(1.0, 0.6, 0.78));
    for (k, (a, r)) in [(2.4f32, 1.8f32), (3.6, 2.3), (5.2, 1.3)]
        .into_iter()
        .enumerate()
    {
        let p = pond + Vec2::from_angle(a) * r;
        commands.spawn((
            Mesh3d(pad.clone()),
            MeshMaterial3d(pad_mat.clone()),
            Transform::from_xyz(p.x, 0.045, p.y).with_rotation(
                Quat::from_rotation_y(a) * Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            ),
            bevy::light::NotShadowCaster,
            out.clone(),
        ));
        if k != 1 {
            commands.spawn((
                Mesh3d(bloom.clone()),
                MeshMaterial3d(pink.clone()),
                Transform::from_xyz(p.x, 0.1, p.y),
                bevy::light::NotShadowCaster,
                out.clone(),
            ));
        }
    }
}

/// What primitive stands in for a backdrop model whose file failed to load.
#[derive(Clone, Copy)]
enum Stand {
    Tree,
    Bush,
    Rock,
}

/// A backdrop model still waiting to see whether its file loads.
#[derive(Component)]
struct PendingModel(Stand);

fn backdrop_model(
    commands: &mut Commands,
    assets: &AssetServer,
    file: &str,
    at: Vec2,
    yaw: f32,
    scale: f32,
    kind: Stand,
) {
    let y = crate::terrain::height_at(at.x, at.y);
    commands.spawn((
        scenery::nature_model(assets, file),
        Transform::from_xyz(at.x, y, at.y)
            .with_rotation(Quat::from_rotation_y(yaw))
            .with_scale(Vec3::splat(scale * scenery::model_size_fix(file))),
        crate::ambience::Sway::default(),
        PendingModel(kind),
        DespawnOnExit(AppState::Menu),
    ));
}

/// Swap failed backdrop models for primitives so the meadow is never empty.
fn backdrop_fallbacks(
    mut commands: Commands,
    assets: Res<AssetServer>,
    pending: Query<(Entity, &WorldAssetRoot, &PendingModel)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (e, root, kind) in &pending {
        let state = assets.load_state(root.0.id());
        if state.is_loaded() {
            commands.entity(e).remove::<PendingModel>();
            continue;
        }
        if !state.is_failed() {
            continue;
        }
        let (mesh, colour, lift): (Mesh, Color, f32) = match kind.0 {
            Stand::Tree => (Sphere::new(1.4).into(), Color::srgb(0.45, 0.72, 0.4), 2.6),
            Stand::Bush => (Sphere::new(0.8).into(), Color::srgb(0.5, 0.7, 0.4), 0.5),
            Stand::Rock => (
                Cuboid::new(2.4, 1.6, 2.0).into(),
                Color::srgb(0.6, 0.6, 0.62),
                0.6,
            ),
        };
        commands
            .entity(e)
            .remove::<(WorldAssetRoot, PendingModel)>()
            .with_children(|p| {
                p.spawn((
                    Mesh3d(meshes.add(mesh)),
                    MeshMaterial3d(materials.add(colour)),
                    Transform::from_xyz(0.0, lift, 0.0),
                ));
                if matches!(kind.0, Stand::Tree) {
                    p.spawn((
                        Mesh3d(meshes.add(Cylinder::new(0.25, 2.0))),
                        MeshMaterial3d(materials.add(Color::srgb(0.5, 0.36, 0.26))),
                        Transform::from_xyz(0.0, 1.0, 0.0),
                    ));
                }
            });
    }
}

fn orbit(t: f32) -> Transform {
    let a = t * 0.06;
    Transform::from_xyz(a.cos() * 26.0, 10.0, a.sin() * 26.0)
        .looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y)
}

fn orbit_camera(time: Res<Time>, mut cams: Query<&mut Transform, With<MenuCamera>>) {
    for mut t in &mut cams {
        *t = orbit(time.elapsed_secs());
    }
}

/// Menu text uses the rounded Nunito font once it has loaded (missing file:
/// Bevy's default font stays).
fn menu_font(
    font: Option<Res<crate::assets::UiFont>>,
    assets: Option<Res<AssetServer>>,
    mut texts: Query<&mut TextFont>,
) {
    let (Some(font), Some(assets)) = (font, assets) else {
        return;
    };
    if !font.ready(&assets) {
        return;
    }
    let default = FontSource::Handle(Handle::default());
    for mut tf in &mut texts {
        if tf.font == default {
            tf.font = FontSource::Handle(font.0.clone());
        }
    }
}

/// Tab toggles settings; Ctrl/Cmd+V pastes a join code.
fn menu_shortcuts(
    keys: Res<ButtonInput<KeyCode>>,
    clip: Res<Clipboard>,
    mut code: ResMut<JoinCode>,
    mut panel: Query<&mut Node, With<SettingsPanel>>,
) {
    if keys.just_pressed(KeyCode::Tab) {
        toggle_settings(&mut panel);
    }
    let modifier = keys.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    if modifier
        && keys.just_pressed(KeyCode::KeyV)
        && let Some(text) = clip.get()
    {
        code.paste(&text);
    }
}

/// Tint the status pill: green for good news, red for trouble.
fn style_status(
    status: Res<NetStatus>,
    mut pill: Query<(&mut Node, &mut BackgroundColor), With<StatusPill>>,
    mut label: Query<(&Label, &mut TextColor)>,
) {
    if !status.is_changed() {
        return;
    }
    let m = status.message.to_lowercase();
    let bad = m.contains("could not") || m.contains("doesn't") || m.contains("first");
    let good = m.contains("hosting") || m.contains("connected!");
    let tint = if bad {
        ERR
    } else if good {
        OK
    } else {
        LILAC
    };
    for (mut node, mut bg) in &mut pill {
        node.display = if m.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        bg.0 = tint.with_alpha(0.18);
    }
    for (l, mut c) in &mut label {
        if matches!(l, Label::Status) {
            c.0 = if bad || good { tint } else { INK };
        }
    }
}

fn join(code: &JoinCode, connecting: &mut Connecting, status: &mut NetStatus, busy: bool) {
    if busy {
        return;
    }
    if code.0.is_empty() {
        status.message = "Type or paste a join code first.".into();
    } else if connecting.pending.is_none() {
        status.message = "Connecting... (this can take a few seconds)".into();
        connecting.pending = Some((code.0.clone(), 2));
        connecting.waited = 0.0;
    }
}

fn start_pending_join(mut connecting: ResMut<Connecting>, mut net: MessageWriter<NetCommand>) {
    let Some((_, frames)) = connecting.pending.as_mut() else {
        return;
    };
    if *frames > 0 {
        *frames -= 1;
        return;
    }
    if let Some((code, _)) = connecting.pending.take() {
        net.write(NetCommand::Join(code));
    }
}

/// Give up if the host never finishes the handshake.
fn connect_timeout(
    mut commands: Commands,
    time: Res<Time>,
    client: Option<Res<IrohClient>>,
    state: Option<Res<State<ClientState>>>,
    mut connecting: ResMut<Connecting>,
    mut status: ResMut<NetStatus>,
) {
    let waiting = client.is_some() && state.is_some_and(|s| *s.get() == ClientState::Connecting);
    if !waiting {
        connecting.waited = 0.0;
        return;
    }
    connecting.waited += time.delta_secs();
    if connecting.waited >= Connecting::TIMEOUT_SECS {
        commands.queue(crate::net::teardown_transport);
        connecting.waited = 0.0;
        status.message =
            "Could not reach that host (timed out). Check the code and try again.".into();
    }
}

/// Esc in game: disconnect (or stop hosting) and go back to the menu.
fn leave_on_esc(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    replicated: Query<Entity, With<Replicated>>,
    mut status: ResMut<NetStatus>,
    mut next: ResMut<NextState<AppState>>,
    mut view: ResMut<crate::features::fps::FpsView>,
    me: Option<Res<LocalPlayer>>,
    duelists: Query<&crate::features::duel::Duelist>,
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    // Duelists have their cursor locked regardless of `released`, so there is
    // nothing for a first Esc to free: leave straight away.
    let duelling = me.is_some_and(|m| duelists.iter().any(|d| d.player == m.0));
    // First Esc in first person just frees the mouse; the next one leaves.
    if view.first_person && !view.released && !duelling {
        view.released = true;
        return;
    }
    commands.queue(crate::net::teardown_transport);
    commands.remove_resource::<LocalPlayer>();
    for e in &replicated {
        commands.entity(e).despawn();
    }
    status.join_code = None;
    status.message = "Left the game.".into();
    next.set(AppState::Menu);
}

fn copy_code_again(
    keys: Res<ButtonInput<KeyCode>>,
    status: Res<NetStatus>,
    clip: Res<Clipboard>,
    mut toast: ResMut<Toast>,
) {
    if !keys.just_pressed(KeyCode::KeyC) {
        return;
    }
    match &status.join_code {
        Some(code) => {
            clip.set(code);
            toast.show("join code copied");
        }
        None => toast.show("only the host has a join code"),
    }
}

fn hover_buttons(
    mut buttons: Query<(&Hovered, &BaseColor, &mut BackgroundColor), Changed<Hovered>>,
) {
    for (hovered, base, mut bg) in &mut buttons {
        bg.0 = if hovered.get() {
            base.0.mix(&Color::WHITE, 0.25)
        } else {
            base.0
        };
    }
}

fn text(ui: &mut ChildSpawnerCommands, s: &str, size: f32, color: Color) {
    ui.spawn((
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(color),
    ));
}

fn spacer(ui: &mut ChildSpawnerCommands, h: f32) {
    ui.spawn(Node {
        height: px(h),
        ..default()
    });
}

fn row(ui: &mut ChildSpawnerCommands, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    ui.spawn(Node {
        column_gap: px(12),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(f);
}

/// A rounded button with a text child (the mic button's text is filled live).
fn button<'a>(
    ui: &'a mut ChildSpawnerCommands,
    label: &str,
    color: Color,
    width: f32,
) -> EntityCommands<'a> {
    let mut e = ui.spawn((
        Node {
            width: px(width),
            padding: UiRect::axes(px(16), px(11)),
            justify_content: JustifyContent::Center,
            border_radius: BorderRadius::all(px(999)),
            ..default()
        },
        BackgroundColor(color),
        BaseColor(color),
        Hovered::default(),
    ));
    e.with_children(|b| {
        b.spawn((
            Text::new(label),
            TextFont::from_font_size(20.0),
            TextColor(Color::WHITE),
        ));
    });
    e
}

fn volume_row(
    ui: &mut ChildSpawnerCommands,
    name: &str,
    label: Label,
    field: fn(&mut Settings) -> &mut f32,
) {
    row(ui, |r| {
        text(r, name, 17.0, INK);
        button(r, "-", LILAC, 50.0).observe(move |_: On<PointerClick>, mut s: ResMut<Settings>| {
            let v = field(&mut s);
            *v = Settings::nudge(*v, -1);
        });
        r.spawn((
            Text::new(""),
            TextFont::from_font_size(18.0),
            TextColor(INK),
            Node {
                width: px(60),
                ..default()
            },
            label,
        ));
        button(r, "+", LILAC, 50.0).observe(move |_: On<PointerClick>, mut s: ResMut<Settings>| {
            let v = field(&mut s);
            *v = Settings::nudge(*v, 1);
        });
    });
}

fn type_join_code(
    mut keys: MessageReader<KeyboardInput>,
    mut code: ResMut<JoinCode>,
    mut connecting: ResMut<Connecting>,
    mut status: ResMut<NetStatus>,
    client: Option<Res<IrohClient>>,
) {
    let mut enter = false;
    for key in keys.read() {
        if key.state == ButtonState::Pressed {
            enter |= code.type_key(&key.logical_key);
        }
    }
    if enter {
        join(&code, &mut connecting, &mut status, client.is_some());
    }
}

fn refresh_labels(
    status: Res<NetStatus>,
    code: Res<JoinCode>,
    settings: Res<Settings>,
    muted: Res<VoiceMuted>,
    connecting: Res<Connecting>,
    client: Option<Res<IrohClient>>,
    time: Res<Time>,
    mut texts: Query<(&mut Text, &Label)>,
    buttons: Query<(&Label, &Children), Without<Text>>,
    mut child_texts: Query<&mut Text, Without<Label>>,
) {
    for (mut t, label) in &mut texts {
        let new = match label {
            Label::Status => status.message.clone(),
            Label::Code if code.0.is_empty() => "(no code yet)".into(),
            Label::Code => code.0.clone(),
            Label::Master => pct(settings.master_volume),
            Label::Voice => pct(settings.voice_volume),
            Label::Mic | Label::HaltName | Label::JoinButton => continue,
        };
        if t.0 != new {
            t.0 = new;
        }
    }
    let mic = if muted.0 { "Muted" } else { "On" };
    let halt = if settings.halt_real_name { "Yes" } else { "No" };
    let dots = &"..."[..(time.elapsed_secs() * 3.0) as usize % 4];
    let join = if connecting.pending.is_some() || client.is_some() {
        format!("Wait{dots}")
    } else {
        "Join!".to_string()
    };
    for (label, children) in &buttons {
        let mic = match label {
            Label::Mic => mic,
            Label::HaltName => halt,
            Label::JoinButton => join.as_str(),
            _ => continue,
        };
        for child in children.iter() {
            if let Ok(mut t) = child_texts.get_mut(child)
                && t.0 != mic
            {
                t.0 = mic.into();
            }
        }
    }
}

fn pct(v: f32) -> String {
    format!("{}%", (v * 100.0).round() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_paste() {
        let mut c = JoinCode::default();
        assert!(!c.type_key(&Key::Character("ab".into())));
        c.type_key(&Key::Character(" ".into()));
        c.type_key(&Key::Backspace);
        assert_eq!(c.0, "a");
        assert!(c.type_key(&Key::Enter));
        c.paste("  deadbeef\n");
        assert_eq!(c.0, "deadbeef");
    }
}
