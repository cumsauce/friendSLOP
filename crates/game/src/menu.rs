//! Main menu (#25): a cute clickable menu to host, join with a code, and change
//! a few settings. `friendslop host` / `friendslop join <code>` still skip it.
//!
//! Hosting alone is fine: you're simply the only player until friends join.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use bevy::picking::hover::Hovered;
use bevy_replicon::prelude::*;
use bevy_replicon_iroh::{IrohClient, IrohServer};

use crate::AppState;
use crate::hud::Toast;
use crate::net::{Clipboard, NetCommand, NetStatus};
use crate::player::LocalPlayer;
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
                for ch in c.chars().filter(|c| !c.is_control() && !c.is_whitespace()) {
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
        self.0 = text.trim().chars().take(Self::MAX_LEN).collect();
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
                    |_: On<PointerClick>, mut net: MessageWriter<NetCommand>| {
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
                             connecting: ResMut<Connecting>,
                             status: ResMut<NetStatus>| {
                                join(&code, connecting, status)
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
/// meshes, the pond, a ring of primitive trees and a glowing campfire, seen by a
/// slowly orbiting camera. Shares its pipelines with the game, so it doubles as
/// a shader warm-up.
fn spawn_backdrop(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
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

    // Lollipop trees in a loose ring, deterministic.
    let trunk = meshes.add(Cylinder::new(0.25, 2.0));
    let crown = meshes.add(Sphere::new(1.4));
    let bark = materials.add(Color::srgb(0.5, 0.36, 0.26));
    let leaves = [
        materials.add(Color::srgb(0.45, 0.72, 0.4)),
        materials.add(Color::srgb(0.6, 0.78, 0.42)),
        materials.add(Color::srgb(1.0, 0.72, 0.82)),
    ];
    for i in 0..18u32 {
        let a = i as f32 * 2.399;
        let r = 7.0 + (i * 7 % 8) as f32;
        let (x, z) = (a.cos() * r, a.sin() * r);
        let s = 0.8 + (i % 4) as f32 * 0.2;
        let y = crate::terrain::height_at(x, z);
        commands.spawn((
            Mesh3d(trunk.clone()),
            MeshMaterial3d(bark.clone()),
            Transform::from_xyz(x, y + s, z).with_scale(Vec3::splat(s)),
            out.clone(),
        ));
        commands.spawn((
            Mesh3d(crown.clone()),
            MeshMaterial3d(leaves[i as usize % leaves.len()].clone()),
            Transform::from_xyz(x, y + 2.6 * s, z).with_scale(Vec3::splat(s)),
            out.clone(),
        ));
    }

    // A little campfire in the middle.
    commands.spawn((
        Mesh3d(meshes.add(Cone::new(0.5, 1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.6, 0.2),
            emissive: LinearRgba::rgb(6.0, 2.4, 0.5),
            ..default()
        })),
        Transform::from_xyz(0.0, 0.5, 0.0),
        out.clone(),
    ));
    commands.spawn((
        PointLight {
            color: Color::srgb(1.0, 0.65, 0.3),
            intensity: 60_000.0,
            range: 12.0,
            ..default()
        },
        Transform::from_xyz(0.0, 1.5, 0.0),
        out,
    ));
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

fn join(code: &JoinCode, mut connecting: ResMut<Connecting>, mut status: ResMut<NetStatus>) {
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
        commands.remove_resource::<IrohClient>();
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
) {
    if !keys.just_pressed(KeyCode::Escape) {
        return;
    }
    // First Esc in first person just frees the mouse; the next one leaves.
    if view.first_person && !view.released {
        view.released = true;
        return;
    }
    commands.remove_resource::<IrohClient>();
    commands.remove_resource::<IrohServer>();
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
    connecting: ResMut<Connecting>,
    status: ResMut<NetStatus>,
) {
    let mut enter = false;
    for key in keys.read() {
        if key.state == ButtonState::Pressed {
            enter |= code.type_key(&key.logical_key);
        }
    }
    if enter {
        join(&code, connecting, status);
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
