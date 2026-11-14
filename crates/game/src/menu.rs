//! Main menu (#25): a cute clickable menu to host, join with a code, and change
//! a few settings. `friendslop host` / `friendslop join <code>` still skip it.
//!
//! Hosting alone is fine: you're simply the only player until friends join.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;

use crate::net::IrohClient;
use bevy::picking::hover::Hovered;
use bevy_replicon::prelude::*;

use crate::AppState;
use crate::hud::Toast;
use crate::net::{Clipboard, NetCommand, NetStatus};
use crate::player::LocalPlayer;
use crate::scenery;
use crate::theme::{self, Pop, Squishy};

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
                    picker_esc,
                )
                    .run_if(in_state(AppState::Menu)),
            )
            .add_systems(Update, copy_code_again.run_if(in_state(AppState::InGame)));
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
        // Same rule as typing: codes are hex, drop anything else (a pasted
        // share link counts as its code).
        self.0 = code_in(text)
            .trim()
            .chars()
            .filter(char::is_ascii_hexdigit)
            .take(Self::MAX_LEN)
            .collect();
    }
}

/// The join code in `text`: the text itself, or the `join=` part of a share
/// link (`https://.../?join=<code>`).
pub fn code_in(text: &str) -> &str {
    match text.split_once("join=") {
        Some((_, rest)) => rest.split(['&', '#']).next().unwrap_or(rest),
        None => text,
    }
}

pub(crate) const CARD: Color = theme::CARD;
pub(crate) const PINK: Color = theme::PINK;
pub(crate) const LILAC: Color = theme::LILAC;
pub(crate) const MINT: Color = theme::MINT;
pub(crate) const INK: Color = theme::INK;
pub(crate) const SOFT_INK: Color = theme::SOFT_INK;

#[derive(Component, Clone, Copy)]
enum Label {
    Status,
    Code,
    /// The end of the code, in the gamepad's hex picker.
    CodeTail,
    JoinButton,
    /// The LAN discovery button (#410).
    LanButton,
}

/// The join-code field (the hex picker gives the pad's focus back to it).
#[derive(Component)]
struct CodeField;

/// The gamepad's join-code keypad (#361), opened by A on the code field. It
/// doesn't need Steam's on-screen keyboard.
#[derive(Component)]
struct HexPicker;

/// One key of the [`HexPicker`].
#[derive(Component, Clone, Copy, Debug, PartialEq)]
enum PickKey {
    Hex(char),
    Back,
    Paste,
    Join,
}

/// The picker's keys in rows: 0-f, then delete, paste and join.
const PICKER_ROWS: [&[PickKey]; 5] = [
    &[
        PickKey::Hex('0'),
        PickKey::Hex('1'),
        PickKey::Hex('2'),
        PickKey::Hex('3'),
    ],
    &[
        PickKey::Hex('4'),
        PickKey::Hex('5'),
        PickKey::Hex('6'),
        PickKey::Hex('7'),
    ],
    &[
        PickKey::Hex('8'),
        PickKey::Hex('9'),
        PickKey::Hex('a'),
        PickKey::Hex('b'),
    ],
    &[
        PickKey::Hex('c'),
        PickKey::Hex('d'),
        PickKey::Hex('e'),
        PickKey::Hex('f'),
    ],
    &[PickKey::Back, PickKey::Paste, PickKey::Join],
];

impl PickKey {
    fn label(self) -> String {
        match self {
            PickKey::Hex(c) => c.to_string(),
            PickKey::Back => "Del".into(),
            PickKey::Paste => "Paste".into(),
            PickKey::Join => "Join".into(),
        }
    }

    /// Apply a key to the code. Returns true for Join.
    fn press(self, code: &mut JoinCode, clipboard: Option<String>) -> bool {
        match self {
            PickKey::Hex(c) => {
                code.type_key(&Key::Character(c.to_string().into()));
            }
            PickKey::Back => {
                code.0.pop();
            }
            PickKey::Paste => {
                if let Some(text) = clipboard {
                    code.paste(&text);
                }
            }
            PickKey::Join => return true,
        }
        false
    }
}

/// The last `n` characters of a code, "…" in front if cut.
fn tail(code: &str, n: usize) -> String {
    let len = code.chars().count();
    if len <= n {
        code.to_string()
    } else {
        format!("…{}", code.chars().skip(len - n).collect::<String>())
    }
}

/// Spawn the hex picker beside the title card.
fn spawn_picker(world: &mut World) {
    use crate::theme::indigo::{BORDER, BUTTER, CREAM, INK as I_INK, PERIWINKLE, RAISED};
    use crate::ui::{at, body, head};
    let (key_w, key_h, gap, pad, b) = (64.0, 50.0, 10.0, 18.0, 2.0);
    let width = 4.0 * key_w + 3.0 * gap + 2.0 * pad;
    let top = 92.0;
    let height = top + 5.0 * key_h + 4.0 * gap + 44.0;
    world
        .spawn((
            HexPicker,
            Node {
                position_type: PositionType::Absolute,
                left: px(486),
                top: px(220),
                width: px(width),
                height: px(height),
                border: UiRect::all(px(b)),
                border_radius: BorderRadius::all(px(18)),
                ..default()
            },
            BackgroundColor(I_INK),
            BorderColor::all(BORDER),
            GlobalZIndex(40),
            DespawnOnExit(AppState::Menu),
        ))
        .with_children(|p| {
            p.spawn((Text::new("Join code"), head(22.0, CREAM), at(pad, 12.0, b)));
            p.spawn((
                Text::new(""),
                body(17.0, PERIWINKLE),
                TextLayout::no_wrap(),
                Label::CodeTail,
                at(pad, 50.0, b),
            ));
            for (row, keys) in PICKER_ROWS.iter().enumerate() {
                // The last row has three keys: Paste takes two columns.
                let mut x = pad;
                for &key in *keys {
                    let w = if key == PickKey::Paste {
                        2.0 * key_w + gap
                    } else {
                        key_w
                    };
                    let y = top + row as f32 * (key_h + gap);
                    let (fill, ink) = if key == PickKey::Join {
                        (BUTTER, I_INK)
                    } else {
                        (RAISED, CREAM)
                    };
                    p.spawn((
                        key,
                        Node {
                            width: px(w),
                            height: px(key_h),
                            border: UiRect::all(px(b)),
                            border_radius: BorderRadius::all(px(12)),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..at(x, y, b)
                        },
                        BackgroundColor(fill),
                        BorderColor::all(BORDER),
                        BaseColor(fill),
                        Hovered::default(),
                        crate::ui::pad_nav::PadFocusable,
                        children![(Text::new(key.label()), head(21.0, ink))],
                    ))
                    .observe(press_picker_key);
                    x += w + gap;
                }
            }
            p.spawn((
                Text::new(""),
                crate::input::Hint("{confirm} press  ·  {back} close".into()),
                body(14.0, PERIWINKLE),
                at(pad, height - 34.0, b),
            ));
        });
}

#[allow(clippy::too_many_arguments)]
fn press_picker_key(
    click: On<PointerClick>,
    keys: Query<&PickKey>,
    mut commands: Commands,
    clip: Res<Clipboard>,
    mut code: ResMut<JoinCode>,
    mut connecting: ResMut<Connecting>,
    mut status: ResMut<NetStatus>,
    client: Option<Res<IrohClient>>,
) {
    let Ok(&key) = keys.get(click.entity) else {
        return;
    };
    let pasted = (key == PickKey::Paste).then(|| clip.get()).flatten();
    if key.press(&mut code, pasted) {
        join(&code, &mut connecting, &mut status, client.is_some());
        commands.queue(close_picker);
    }
}

/// Close the hex picker and put the pad's focus back on the code field.
fn close_picker(world: &mut World) {
    let mut pickers = world.query_filtered::<Entity, With<HexPicker>>();
    let open: Vec<Entity> = pickers.iter(world).collect();
    if open.is_empty() {
        return;
    }
    for e in open {
        world.despawn(e);
    }
    let mut field = world.query_filtered::<Entity, With<CodeField>>();
    let field = field.iter(world).next();
    if let Some(mut focus) = world.get_resource_mut::<crate::ui::pad_nav::PadFocus>() {
        focus.entity = field;
    }
}

/// B (Esc) closes the hex picker.
fn picker_esc(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::Escape) {
        commands.queue(close_picker);
    }
}

/// A button's resting color, brightened while hovered.
#[derive(Component, Clone, Copy)]
pub(crate) struct BaseColor(pub(crate) Color);

/// The pill behind the status line (tinted by what the message says).
#[derive(Component)]
struct StatusPill;

/// The backdrop camera, orbiting the little meadow.
#[derive(Component)]
struct MenuCamera;

/// The title screen's "Indigo journal cover" (Figma 3:1836): an ink card on
/// the left over the meadow diorama, with the logo, Host / Join / Character
/// creator buttons, the join-code field and quiet text links.
fn spawn_menu(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    use crate::theme::indigo::{BORDER, BUTTER, CREAM, INK as I_INK, PERIWINKLE, RAISED};
    use crate::ui::{at, body, head};
    let logo: Handle<Image> = assets.map(|a| a.load("ui/logo.png")).unwrap_or_default();
    let b = 2.0;
    let cover_button = |label: &str, fill: Color, ink: Color, h: f32, y: f32| {
        (
            Node {
                width: px(362),
                height: px(h),
                border: UiRect::all(px(b)),
                border_radius: BorderRadius::all(px(18)),
                ..at(26.0, y, b)
            },
            BackgroundColor(fill),
            BorderColor::all(BORDER),
            // A hard ink shadow 5 px down, like the design's lip.
            BoxShadow::new(I_INK, px(0), px(5), px(0), px(0)),
            BaseColor(fill),
            Hovered::default(),
            crate::ui::pad_nav::PadFocusable,
            Squishy::default(),
            UiTransform::default(),
            children![(
                Text::new(label.to_string()),
                head(23.0, ink),
                at(16.0, 9.0, b)
            )],
        )
    };
    let link = |label: &str, color: Color, x: f32, y: f32| {
        (
            Text::new(label.to_string()),
            body(18.0, color),
            at(x, y, b),
            Hovered::default(),
            crate::ui::pad_nav::PadFocusable,
        )
    };
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(44),
                top: px(40),
                width: px(418),
                height: px(698),
                border: UiRect::all(px(b)),
                border_radius: BorderRadius::all(px(18)),
                ..default()
            },
            BackgroundColor(I_INK),
            BorderColor::all(BORDER),
            Pop::now(0.85),
            UiTransform::default(),
            DespawnOnExit(AppState::Menu),
        ))
        .with_children(|ui| {
            ui.spawn((
                Text::new("a commune of terrible ideas"),
                body(17.0, PERIWINKLE),
                at(26.0, 20.0, b),
            ));
            // The illustrated logo; the wordmark stands in if the file is missing.
            ui.spawn((
                Node {
                    width: px(362),
                    height: px(104.4),
                    ..at(26.0, 44.0, b)
                },
                ImageNode::new(logo),
            ));
            ui.spawn((
                Text::new("1–6 friends. One very bad landlord."),
                body(17.0, CREAM),
                at(26.0, 151.0, b),
            ));
            ui.spawn(cover_button("Host a commune", BUTTER, I_INK, 54.0, 179.0))
                .observe(
                    |_: On<PointerClick>,
                     mut net: MessageWriter<NetCommand>,
                     mut connecting: ResMut<Connecting>| {
                        connecting.pending = None;
                        net.write(NetCommand::Host);
                    },
                );
            ui.spawn((
                cover_button("Join with a code", RAISED, CREAM, 52.0, 252.0),
                Label::JoinButton,
            ))
            .observe(
                |_: On<PointerClick>,
                 code: Res<JoinCode>,
                 mut connecting: ResMut<Connecting>,
                 mut status: ResMut<NetStatus>,
                 client: Option<Res<IrohClient>>| {
                    join(&code, &mut connecting, &mut status, client.is_some())
                },
            );
            // The code field: type, Ctrl/Cmd+V, or click it to paste. A
            // gamepad's A opens the hex picker instead (#361).
            ui.spawn((
                CodeField,
                crate::ui::pad_nav::PadFocusable,
                Node {
                    width: px(362),
                    height: px(44),
                    border: UiRect::all(px(b)),
                    border_radius: BorderRadius::all(px(12)),
                    overflow: Overflow::clip(),
                    ..at(26.0, 317.0, b)
                },
                BackgroundColor(RAISED),
                BorderColor::all(BORDER),
                children![(
                    Text::new(""),
                    body(17.0, PERIWINKLE),
                    TextLayout::no_wrap(),
                    Label::Code,
                    at(12.0, 9.0, b),
                )],
            ))
            .observe(
                |_: On<PointerClick>,
                 mut commands: Commands,
                 focus: Res<crate::ui::pad_nav::PadFocus>,
                 pickers: Query<(), With<HexPicker>>,
                 clip: Res<Clipboard>,
                 mut code: ResMut<JoinCode>| {
                    if focus.pad_click {
                        if pickers.is_empty() {
                            commands.queue(spawn_picker);
                        }
                    } else if let Some(text) = clip.get() {
                        code.paste(&text);
                    }
                },
            );
            // LAN discovery (#410): a game hosted on the same network.
            ui.spawn((
                cover_button("", PERIWINKLE, I_INK, 46.0, 381.0),
                Label::LanButton,
            ))
            .observe(
                |_: On<PointerClick>,
                 games: Res<crate::lan::LanGames>,
                 mut code: ResMut<JoinCode>,
                 mut connecting: ResMut<Connecting>,
                 mut status: ResMut<NetStatus>,
                 client: Option<Res<IrohClient>>| {
                    let Some(game) = games.0.first() else {
                        status.message =
                            "No games on your network yet: ask a friend to host.".into();
                        return;
                    };
                    code.0.clone_from(&game.code);
                    join(&code, &mut connecting, &mut status, client.is_some())
                },
            );
            ui.spawn(cover_button(
                "Character creator",
                RAISED,
                CREAM,
                52.0,
                439.0,
            ))
            .observe(
                |_: On<PointerClick>, mut open: ResMut<crate::creator::CreatorOpen>| {
                    open.0 = !open.0;
                },
            );
            ui.spawn(link("Settings", PERIWINKLE, 26.0, 519.0)).observe(
                |_: On<PointerClick>, mut panel: Query<&mut Node, With<SettingsPanel>>| {
                    crate::ui::settings_panel::toggle(&mut panel);
                },
            );
            ui.spawn((Text::new("·"), body(18.0, PERIWINKLE), at(120.0, 519.0, b)));
            ui.spawn(link("Whose fault is it", PERIWINKLE, 151.0, 519.0))
                .observe(
                |_: On<PointerClick>,
                 mut panel: Query<&mut Node, With<crate::fault::FaultPanel>>| {
                    crate::fault::toggle(&mut panel);
                },
            );
            ui.spawn(link("Kori prizes", PERIWINKLE, 26.0, 560.0))
                .observe(
                |_: On<PointerClick>,
                 mut panel: Query<&mut Node, With<crate::kori_shelf::ShelfPanel>>| {
                    crate::kori_shelf::toggle(&mut panel);
                },
            );
            ui.spawn(link("Quit", CREAM, 151.0, 560.0)).observe(
                |_: On<PointerClick>, mut exit: MessageWriter<AppExit>| {
                    exit.write(AppExit::Success);
                },
            );
            // Connection news ("Connecting...", errors) above the footer.
            ui.spawn((
                Node {
                    padding: UiRect::axes(px(12), px(5)),
                    border_radius: BorderRadius::all(px(999)),
                    max_width: px(362),
                    display: Display::None,
                    ..at(26.0, 598.0, b)
                },
                BackgroundColor(RAISED),
                StatusPill,
                children![(Text::new(""), body(14.0, CREAM), Label::Status)],
            ));
            ui.spawn((
                Text::new("Made by your Discord. Blame accordingly."),
                body(14.0, PERIWINKLE),
                at(26.0, 638.0, b),
            ));
            ui.spawn((
                Text::new(""),
                crate::input::Hint("D-pad move  ·  {confirm} select  ·  {back} back".into()),
                body(14.0, CREAM),
                at(26.0, 664.0, b),
                Visibility::Hidden,
                crate::ui::pad_nav::PadHint,
            ));
        });
}

/// The settings notebook (`ui::settings_panel`).
pub(crate) use crate::ui::settings_panel::SettingsPanel;

fn toggle_settings(panel: &mut Query<&mut Node, With<SettingsPanel>>) {
    crate::ui::settings_panel::toggle(panel);
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
        // The character preview camera draws on top; the menu UI stays here.
        IsDefaultUiCamera,
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
    let pond = crate::terrain::pond_at();
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
    // The scenery log seat (r 0.2 x 1.5 m, lying along its Z, origin at its
    // centre), scaled down to the old 0.18 x 1.4 m seat.
    for yaw in [0.6f32, 2.9] {
        let p = Vec2::from_angle(yaw) * 2.0;
        commands.spawn((
            scenery::scenery_model(&assets, "log_seat"),
            Transform::from_xyz(p.x, 0.18, p.y)
                .with_rotation(Quat::from_rotation_y(-yaw))
                .with_scale(Vec3::splat(0.92)),
            out.clone(),
        ));
    }
    for (i, ang) in [0.2f32, 2.2, 4.3].into_iter().enumerate() {
        let p = Vec2::from_angle(ang) * 2.6;
        // Face the fire (bodies look along -Z).
        let facing = Transform::from_xyz(p.x, 0.0, p.y).looking_at(Vec3::ZERO, Vec3::Y);
        commands.spawn((
            crate::people::Gait::default(),
            crate::people::CharacterLook::random(4242 + i as u64 * 7),
            facing,
            Visibility::default(),
            out.clone(),
        ));
    }

    // Bunting between two poles behind the fire.
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
        // Modeler pole (`models/menu/`), base at its origin.
        commands.spawn((
            WorldAssetRoot(
                assets.load(GltfAssetLabel::Scene(0).from_asset("models/menu/bunting_pole.gltf")),
            ),
            Transform::from_xyz(p.x, 0.0, p.y),
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
            .with_scale(Vec3::splat(scale)),
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
    // Never red vs green (UI direction E): butter for trouble.
    let tint = if bad {
        crate::theme::indigo::BUTTER
    } else if good {
        crate::theme::indigo::PERIWINKLE
    } else {
        crate::theme::indigo::CREAM
    };
    for (mut node, mut bg) in &mut pill {
        node.display = if m.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        bg.0 = crate::theme::indigo::RAISED;
    }
    for (l, mut c) in &mut label {
        if matches!(l, Label::Status) {
            c.0 = tint;
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

/// The settings notebook's "Leave game" button (#317; Esc used to leave):
/// disconnect (or stop hosting) and go back to the menu.
pub(crate) fn leave_game(
    _: On<PointerClick>,
    mut commands: Commands,
    replicated: Query<Entity, With<Replicated>>,
    mut status: ResMut<NetStatus>,
    mut next: ResMut<NextState<AppState>>,
) {
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
            let share = crate::net::share_text(code);
            clip.set(&share);
            toast.show(if share == *code {
                "join code copied"
            } else {
                "join link copied"
            });
        }
        None => toast.show("only the host has a join code"),
    }
}

fn hover_buttons(
    mut buttons: Query<(&Hovered, &BaseColor, &mut BackgroundColor), Changed<Hovered>>,
) {
    for (hovered, base, mut bg) in &mut buttons {
        bg.0 = if hovered.get() {
            base.0.mix(&Color::WHITE, 0.2)
        } else {
            base.0
        };
    }
}

pub(crate) fn text(ui: &mut ChildSpawnerCommands, s: &str, size: f32, color: Color) {
    ui.spawn((
        Text::new(s),
        TextFont::from_font_size(size),
        TextColor(color),
    ));
}

pub(crate) fn row(ui: &mut ChildSpawnerCommands, f: impl FnOnce(&mut ChildSpawnerCommands)) {
    ui.spawn(Node {
        column_gap: px(12),
        align_items: AlignItems::Center,
        ..default()
    })
    .with_children(f);
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
    connecting: Res<Connecting>,
    client: Option<Res<IrohClient>>,
    time: Res<Time>,
    lan: Res<crate::lan::LanGames>,
    mut texts: Query<(&mut Text, &Label)>,
    buttons: Query<(&Label, &Children), Without<Text>>,
    mut child_texts: Query<&mut Text, Without<Label>>,
) {
    for (mut t, label) in &mut texts {
        let new = match label {
            Label::Status => status.message.clone(),
            Label::Code if code.0.is_empty() => "Paste code or share link…".into(),
            Label::Code => code.0.clone(),
            Label::CodeTail if code.0.is_empty() => "(empty: pick 0-f, or Paste)".into(),
            Label::CodeTail => format!("{}  ({})", tail(&code.0, 22), code.0.len()),
            Label::JoinButton | Label::LanButton => continue,
        };
        if t.0 != new {
            t.0 = new;
        }
    }
    let dots = &"..."[..(time.elapsed_secs() * 3.0) as usize % 4];
    let join = if connecting.pending.is_some() || client.is_some() {
        format!("Joining{dots}")
    } else {
        "Join with a code".to_string()
    };
    let lan = match lan.0.first() {
        Some(game) => crate::lan::button_text(game),
        None => "Looking for games on your Wi-Fi...".to_string(),
    };
    for (label, children) in &buttons {
        let join = match label {
            Label::JoinButton => &join,
            Label::LanButton => &lan,
            _ => continue,
        };
        for child in children.iter() {
            if let Ok(mut t) = child_texts.get_mut(child)
                && t.0 != *join
            {
                t.0.clone_from(join);
            }
        }
    }
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
        // A share link pastes as its code, not every hex letter in the URL.
        c.paste("https://cafe.example/friendSLOP/?join=0123abcd&x=1");
        assert_eq!(c.0, "0123abcd");
        assert_eq!(code_in("https://a.b/?join=beef#top"), "beef");
        assert_eq!(code_in("beef"), "beef");
    }

    #[test]
    fn hex_picker_types_deletes_pastes_and_joins() {
        let mut c = JoinCode::default();
        for k in [PickKey::Hex('a'), PickKey::Hex('0'), PickKey::Hex('f')] {
            assert!(!k.press(&mut c, None));
        }
        assert_eq!(c.0, "a0f");
        PickKey::Back.press(&mut c, None);
        assert_eq!(c.0, "a0");
        PickKey::Paste.press(&mut c, Some("https://x.y/?join=beef".into()));
        assert_eq!(c.0, "beef");
        // Nothing on the clipboard: the code stays.
        PickKey::Paste.press(&mut c, None);
        assert_eq!(c.0, "beef");
        assert!(PickKey::Join.press(&mut c, None));
        // Every hex digit has a key, once.
        let hex: String = PICKER_ROWS
            .iter()
            .flat_map(|r| r.iter())
            .filter_map(|k| match k {
                PickKey::Hex(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert_eq!(hex, "0123456789abcdef");
        assert_eq!(tail("0123456789", 4), "…6789");
        assert_eq!(tail("abc", 4), "abc");
    }
}
