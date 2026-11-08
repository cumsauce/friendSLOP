//! Gods tears: teardrop gems of great power that transform whoever uses them.
//! From issue #16 by Freyja Moth.
//!
//! The tears rained down only once, when humans came into existence: the
//! server drops a fixed, limited supply of replicated `GodsTear` gems when the
//! world begins, and no more ever fall (corrected in #43). Click one to use it: the server bumps your replicated
//! `Transformation`. The more you use, the further you change, until you
//! become a fetid, ever-hungry Abduction. What happens when an Abduction
//! succeeds is not known.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::player::{Player, PlayerOwner};
use crate::world::MainCamera;

/// The whole, limited supply of tears. They all fall once, at the start.
pub const TOTAL_TEARS: u32 = 12;
/// Uses after which you are an Abduction.
pub const ABDUCTION_AT: u32 = 7;
/// Height a new tear falls from.
const FALL_FROM: f32 = 12.0;
/// How fast it falls (units per second).
const FALL_SPEED: f32 = 6.0;

/// A tear on the floor at (x, z).
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct GodsTear {
    pub id: u32,
    pub at: Vec2,
}

/// How many tears this player has used. Server-authoritative, replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Transformation(pub u32);

impl Transformation {
    pub fn is_abduction(self) -> bool {
        self.0 >= ABDUCTION_AT
    }

    pub fn label(self) -> &'static str {
        match self.0 {
            0 => "untouched",
            1..=2 => "tear-touched",
            3..=4 => "changing",
            n if n < ABDUCTION_AT => "barely human",
            _ => "ABDUCTION (fetid, starving for tears)",
        }
    }
}

/// Client -> server: "I'm using tear `id`."
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct UseGodsTear {
    pub id: u32,
}

pub fn protocol(app: &mut App) {
    app.replicate::<GodsTear>()
        .replicate::<Transformation>()
        .add_observer(reset_tears)
        .add_client_event::<UseGodsTear>(Channel::Ordered)
        .init_resource::<TearSky>()
        .add_observer(give_transformation)
        .add_observer(use_tear)
        .add_systems(OnEnter(ServerState::Running), the_one_rain);
}

/// Server only: deterministic random source and next id.
#[derive(Resource, Default)]
struct TearSky {
    seed: u32,
    next_id: u32,
}

impl TearSky {
    fn next(&mut self) -> f32 {
        self.seed = self
            .seed
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    fn tear(&mut self) -> GodsTear {
        let angle = self.next() * std::f32::consts::TAU;
        let dist = 2.0 + self.next() * 10.0;
        self.next_id += 1;
        GodsTear {
            id: self.next_id,
            at: Vec2::from_angle(angle) * dist,
        }
    }
}

/// When humans came into existence, the tears rained down. Once.
fn the_one_rain(mut commands: Commands, mut sky: ResMut<TearSky>) {
    sky.seed = 0x7ea2_0016;
    for _ in 0..TOTAL_TEARS {
        let tear = sky.tear();
        commands.spawn((tear, Replicated));
    }
}

fn give_transformation(
    add: On<Add<Player>>,
    server: Res<State<ServerState>>,
    mut commands: Commands,
) {
    if *server.get() != ServerState::Running {
        return;
    }
    commands
        .entity(add.entity)
        .insert(Transformation::default());
}

/// Server: whoever asks first gets the tear, and changes a little more.
fn use_tear(
    ev: On<FromClient<UseGodsTear>>,
    still: Query<&crate::features::stillness::Stillness>,
    mut commands: Commands,
    tears: Query<(Entity, &GodsTear)>,
    mut players: Query<(&PlayerOwner, &mut Transformation)>,
    mut fate: super::fate::Fate,
) {
    if crate::features::stillness::is_frozen(&still) {
        return; // the world stands still: ignore requests
    }
    let Some((tear, _)) = tears.iter().find(|(_, t)| t.id == ev.id) else {
        return;
    };
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((_, mut change)) = players.iter_mut().find(|(o, _)| o.0 == owner) else {
        return;
    };
    if fate.fails(ev.client_id) {
        return;
    }
    commands.entity(tear).despawn();
    change.0 += 1;
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_tears_hud)
        .add_systems(
            Update,
            (
                dress_tears,
                fall_and_glint,
                click_tear,
                update_tears_hud.run_if(crate::world::hud_tick()),
            )
                .run_if(in_state(AppState::InGame)),
        );
}

/// Give every tear (on every peer) a glowing blue teardrop body.
fn dress_tears(
    mut commands: Commands,
    new: Query<(Entity, &GodsTear), Added<GodsTear>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, tear) in &new {
        let gem = materials.add(StandardMaterial {
            base_color: Color::srgb(0.45, 0.75, 1.0),
            emissive: LinearRgba::rgb(0.3, 0.6, 1.2),
            ..default()
        });
        commands
            .entity(entity)
            .insert((
                Transform::from_xyz(tear.at.x, FALL_FROM, tear.at.y),
                Visibility::default(),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|p| {
                p.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.18))),
                    MeshMaterial3d(gem.clone()),
                    Transform::from_xyz(0.0, 0.18, 0.0),
                ));
                p.spawn((
                    Mesh3d(meshes.add(Cone::new(0.17, 0.35))),
                    MeshMaterial3d(gem),
                    Transform::from_xyz(0.0, 0.42, 0.0),
                ));
            });
    }
}

/// Tears fall from the sky, then bob and spin gently on the floor.
fn fall_and_glint(time: Res<Time>, mut tears: Query<(&GodsTear, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (tear, mut transform) in &mut tears {
        let rest = 0.15 + 0.08 * (t * 2.0 + tear.id as f32).sin();
        let y = transform.translation.y;
        transform.translation.y = if y > rest + 0.2 {
            (y - FALL_SPEED * time.delta_secs()).max(rest)
        } else {
            rest
        };
        transform.rotation = Quat::from_rotation_y(t + tear.id as f32);
    }
}

/// How close (in pixels) a click must land to a tear on screen.
const CLICK_RADIUS: f32 = 40.0;

fn click_tear(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Query<&Window, With<PrimaryWindow>>,
    view: Res<super::fps::FpsView>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    tears: Query<(&GodsTear, &Transform)>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = window.single() else { return };
    let Some(cursor) = super::fps::aim_point(window, &view) else {
        return;
    };
    let Ok((camera, cam_transform)) = camera.single() else {
        return;
    };
    let hit = tears.iter().find(|(_, tf)| {
        camera
            .world_to_viewport(cam_transform, tf.translation + Vec3::Y * 0.3)
            .is_ok_and(|p| p.distance(cursor) < CLICK_RADIUS)
    });
    if let Some((tear, _)) = hit {
        commands.client_trigger(UseGodsTear { id: tear.id });
    }
}

#[derive(Component)]
struct TearsHud;

fn spawn_tears_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TearsHud,
        crate::hud::slot(crate::hud::HudSlot::Scoreboard),
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_tears_hud(
    players: Query<(&Player, &Transformation)>,
    tears: Query<(), With<GodsTear>>,
    mut hud: Query<&mut Text, With<TearsHud>>,
) {
    let mut rows: Vec<_> = players.iter().collect();
    rows.sort_by_key(|(p, _)| p.id);
    let left = tears.iter().count();
    let mut lines = vec![format!(
        "gods tears: {left} of {TOTAL_TEARS} left, no more will fall (click one to use it)"
    )];
    for (player, t) in rows {
        lines.push(format!(
            "player {}: {} ({} used)",
            player.id,
            t.label(),
            t.0
        ));
    }
    let want = lines.join("\n");
    for mut text in &mut hud {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
}

/// New round: the one rain falls again, and nobody has been transformed yet.
fn reset_tears(
    _reset: On<crate::features::objective::RoundReset>,
    mut commands: Commands,
    mut sky: ResMut<TearSky>,
    tears: Query<Entity, With<GodsTear>>,
    mut players: Query<&mut Transformation>,
) {
    for tear in &tears {
        commands.entity(tear).despawn();
    }
    *sky = TearSky::default();
    the_one_rain(commands, sky);
    for mut t in &mut players {
        *t = Transformation::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enough_tears_make_an_abduction() {
        assert!(!Transformation(0).is_abduction());
        assert!(!Transformation(ABDUCTION_AT - 1).is_abduction());
        assert!(Transformation(ABDUCTION_AT).is_abduction());
        assert!(
            Transformation(ABDUCTION_AT)
                .label()
                .starts_with("ABDUCTION")
        );
    }

    #[test]
    fn tears_land_on_the_floor_with_unique_ids() {
        let mut sky = TearSky::default();
        let a = sky.tear();
        let b = sky.tear();
        assert_ne!(a.id, b.id);
        assert!(a.at.length() <= 12.0 && a.at.length() >= 2.0);
    }
}
