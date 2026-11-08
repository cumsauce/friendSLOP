//! Landlords. From issue #11 by Freyja Moth.
//!
//! The inciting incident: your landlord raises your rent. The story runs in
//! three acts, tracked on one replicated `Tenancy` entity the server owns:
//!
//! 1. **Legal**: click the landlord to pay rent with every organ you carry
//!    (organs are legal tender, #22). He raises the rent again every
//!    `RAISE_SECS`, so you can never quite keep up.
//! 2. **Less than legal**: after `RAISES_BEFORE_CRIME` raises you give up on the
//!    law. Clicking him now pickpockets an organ back from his stash, which
//!    counts as harming a landlord for morality (#21, corrected in #43): always virtuous.
//! 3. **Boss fight**: after `HEISTS_BEFORE_BOSS` heists he grows huge and
//!    fights back with eviction notices. Clicking hits him; at 0 health the
//!    game is over and you've won.
//!
//! Everything pauses during the Stillness (#17).

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::morality::{Act, MoralAct, Victim};
use crate::features::organ_currency::{Organ, OrganPurse};
use crate::features::stillness::{Stillness, is_frozen};
use crate::player::PlayerOwner;
use crate::world::MainCamera;

/// Where the landlord stands.
pub const LANDLORD_AT: Vec3 = Vec3::new(13.0, 0.0, -4.0);
/// Rent before the inciting raise.
pub const OLD_RENT: u32 = 10;
/// How much each raise adds.
pub const RAISE_BY: u32 = 15;
/// Seconds between rent raises.
pub const RAISE_SECS: f32 = 60.0;
/// Raises (after the first) until the tenants turn to crime.
pub const RAISES_BEFORE_CRIME: u32 = 3;
/// Heists until the landlord turns into a boss.
pub const HEISTS_BEFORE_BOSS: u32 = 3;
/// The boss landlord's health.
pub const BOSS_HP: u32 = 12;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Act3 {
    #[default]
    Legal,
    Crime,
    Boss,
    Won,
}

/// The whole tenancy story. One per session, server-authoritative.
#[derive(Component, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Tenancy {
    pub act: Act3,
    /// Rent currently demanded (in organ value).
    pub rent: u32,
    /// Paid towards the current rent.
    pub paid: u32,
    /// How many times he's raised it since the inciting incident.
    pub raises: u32,
    pub heists: u32,
    pub hp: u32,
    /// Organs he's collected; heists take from here.
    pub stash: Vec<Organ>,
    /// Seconds until the next raise.
    pub next_raise: f32,
}

impl Default for Tenancy {
    /// Right after the inciting incident: the rent has just gone up.
    fn default() -> Self {
        Self {
            act: Act3::Legal,
            rent: OLD_RENT + RAISE_BY,
            paid: 0,
            raises: 0,
            heists: 0,
            hp: BOSS_HP,
            stash: Vec::new(),
            next_raise: RAISE_SECS,
        }
    }
}

impl Tenancy {
    /// Time passes: in the legal act, he keeps raising the rent.
    pub fn tick(&mut self, dt: f32) {
        if self.act != Act3::Legal {
            return;
        }
        self.next_raise -= dt;
        if self.next_raise > 0.0 {
            return;
        }
        self.next_raise = RAISE_SECS;
        self.raises += 1;
        self.rent += RAISE_BY;
        if self.raises >= RAISES_BEFORE_CRIME {
            self.act = Act3::Crime;
        }
    }

    /// A tenant bothers the landlord. Returns organs they get back and whether
    /// it harmed him.
    pub fn bother(&mut self, purse: &mut OrganPurse) -> bool {
        match self.act {
            Act3::Legal => {
                for organ in purse.0.drain(..) {
                    self.paid += organ.value();
                    self.stash.push(organ);
                }
                if self.paid >= self.rent {
                    // Paid up, so naturally he raises it.
                    self.paid -= self.rent;
                    self.rent += RAISE_BY;
                }
                false
            }
            Act3::Crime => {
                if let Some(organ) = self.stash.pop() {
                    purse.0.push(organ);
                }
                self.heists += 1;
                if self.heists >= HEISTS_BEFORE_BOSS {
                    self.act = Act3::Boss;
                }
                true
            }
            Act3::Boss => {
                self.hp = self.hp.saturating_sub(1);
                if self.hp == 0 {
                    self.act = Act3::Won;
                    purse.0.append(&mut self.stash);
                }
                true
            }
            Act3::Won => false,
        }
    }
}

/// Client -> server: "I'm dealing with the landlord" (pay, steal or hit,
/// depending on the act).
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct BotherLandlord;

pub fn protocol(app: &mut App) {
    app.replicate::<Tenancy>()
        .add_observer(reset_tenancy)
        .add_client_event::<BotherLandlord>(Channel::Ordered)
        .add_observer(bother_landlord)
        .add_systems(OnEnter(ServerState::Running), raise_the_rent)
        .add_systems(Update, tick_tenancy.run_if(in_state(ServerState::Running)));
}

/// The inciting incident.
fn raise_the_rent(mut commands: Commands) {
    commands.spawn((Tenancy::default(), Replicated));
}

fn tick_tenancy(time: Res<Time>, still: Query<&Stillness>, mut tenancy: Query<&mut Tenancy>) {
    if is_frozen(&still) {
        return;
    }
    for mut t in &mut tenancy {
        t.tick(time.delta_secs());
    }
}

fn bother_landlord(
    ev: On<FromClient<BotherLandlord>>,
    mut commands: Commands,
    still: Query<&Stillness>,
    mut tenancy: Query<&mut Tenancy>,
    players: Query<(Entity, &PlayerOwner)>,
    mut purses: Query<&mut OrganPurse>,
) {
    if is_frozen(&still) {
        return;
    }
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((player, _)) = players.iter().find(|(_, o)| o.0 == owner) else {
        return;
    };
    let Ok(mut purse) = purses.get_mut(player) else {
        return;
    };
    let Ok(mut t) = tenancy.single_mut() else {
        return;
    };
    if t.bother(&mut purse) {
        commands.trigger(MoralAct {
            player,
            act: Act::Harm(Victim::Landlord),
        });
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_landlord)
        .add_systems(
            Update,
            (show_tenancy, click_landlord).run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct LandlordBody;

#[derive(Component)]
struct TenancyText;

fn spawn_landlord(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let suit = materials.add(Color::srgb(0.15, 0.15, 0.2));
    let skin = materials.add(Color::srgb(0.95, 0.75, 0.65));
    let hat = crate::art::glossy(&mut materials, Color::srgb(0.05, 0.05, 0.05));
    let shirt = crate::art::soft(&mut materials, Color::srgb(0.95, 0.95, 0.95));
    let band = crate::art::soft(&mut materials, Color::srgb(0.5, 0.1, 0.4));
    let gold = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.8, 0.3),
        metallic: 0.9,
        perceptual_roughness: 0.3,
        ..default()
    });
    commands
        .spawn((
            LandlordBody,
            Transform::from_translation(LANDLORD_AT),
            Visibility::default(),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|p| {
            p.spawn((
                Mesh3d(meshes.add(Capsule3d::new(0.4, 0.9))),
                MeshMaterial3d(suit.clone()),
                Transform::from_xyz(0.0, 0.85, 0.0),
            ));
            p.spawn((
                Mesh3d(meshes.add(Sphere::new(0.3))),
                MeshMaterial3d(skin.clone()),
                Transform::from_xyz(0.0, 1.75, 0.0),
            ))
            .with_children(|h| {
                crate::art::eyes(
                    h,
                    &mut meshes,
                    &mut materials,
                    Vec3::new(0.0, 0.05, -0.26),
                    0.2,
                    0.05,
                );
                // A smug curly moustache and a big nose.
                let tache = meshes.add(Capsule3d::new(0.03, 0.12));
                for x in [-0.07, 0.07] {
                    h.spawn((
                        Mesh3d(tache.clone()),
                        MeshMaterial3d(hat.clone()),
                        Transform::from_xyz(x, -0.1, -0.27)
                            .with_rotation(Quat::from_rotation_z(1.2 * x.signum())),
                    ));
                }
                h.spawn((
                    Mesh3d(meshes.add(Sphere::new(0.06))),
                    MeshMaterial3d(skin.clone()),
                    Transform::from_xyz(0.0, -0.03, -0.3),
                ));
            });
            // Arms, white shirt front, gold watch chain.
            let arm = meshes.add(Capsule3d::new(0.11, 0.55));
            for x in [-0.48, 0.48] {
                p.spawn((
                    Mesh3d(arm.clone()),
                    MeshMaterial3d(suit.clone()),
                    Transform::from_xyz(x, 1.0, 0.0)
                        .with_rotation(Quat::from_rotation_z(0.2 * x.signum())),
                ));
            }
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.25, 0.5, 0.05))),
                MeshMaterial3d(shirt),
                Transform::from_xyz(0.0, 1.15, -0.38),
            ));
            p.spawn((
                Mesh3d(meshes.add(Torus::new(0.01, 0.025))),
                MeshMaterial3d(gold.clone()),
                Transform::from_xyz(0.12, 0.85, -0.4)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ));
            // Hat band.
            p.spawn((
                Mesh3d(meshes.add(Cylinder::new(0.225, 0.08))),
                MeshMaterial3d(band),
                Transform::from_xyz(0.0, 2.06, 0.0),
            ));
            // A top hat, obviously.
            p.spawn((
                Mesh3d(meshes.add(Cylinder::new(0.22, 0.4))),
                MeshMaterial3d(hat.clone()),
                Transform::from_xyz(0.0, 2.2, 0.0),
            ));
            p.spawn((
                Mesh3d(meshes.add(Cylinder::new(0.38, 0.03))),
                MeshMaterial3d(hat),
                Transform::from_xyz(0.0, 2.0, 0.0),
            ));
        });
    commands.spawn((
        Text::new(""),
        TenancyText,
        crate::hud::slot(crate::hud::HudSlot::TopRight),
        DespawnOnExit(AppState::InGame),
    ));
}

/// What the HUD says about the tenancy.
pub fn describe(t: &Tenancy) -> String {
    match t.act {
        Act3::Legal => format!(
            "RENT {}  paid {}  raise in {:.0}s\nclick the landlord to pay",
            t.rent, t.paid, t.next_raise
        ),
        Act3::Crime => format!(
            "the law has failed you  |  rent {}\nclick him to pickpocket an organ  {}/{}",
            t.rent, t.heists, HEISTS_BEFORE_BOSS
        ),
        Act3::Boss => format!(
            "BOSS  |  THE LANDLORD  {}/{}\nclick him to fight back",
            t.hp, BOSS_HP
        ),
        Act3::Won => "the landlord is defeated  |  the rent is free".to_string(),
    }
}

fn show_tenancy(
    time: Res<Time>,
    tenancy: Query<&Tenancy>,
    mut text: Query<&mut Text, With<TenancyText>>,
    mut body: Query<&mut Transform, With<LandlordBody>>,
) {
    let Ok(t) = tenancy.single() else { return };
    let want = describe(t);
    for mut text in &mut text {
        if text.0 != want {
            text.0.clone_from(&want);
        }
    }
    let (scale, bob) = match t.act {
        Act3::Boss => (2.5, (time.elapsed_secs() * 6.0).sin().abs() * 0.4),
        Act3::Won => (0.6, 0.0),
        _ => (1.0, 0.0),
    };
    for mut tf in &mut body {
        tf.scale = Vec3::splat(scale);
        tf.translation = LANDLORD_AT + Vec3::Y * bob;
        if t.act == Act3::Won {
            tf.rotation = Quat::from_rotation_z(std::f32::consts::FRAC_PI_2);
        }
    }
}

const CLICK_RADIUS: f32 = 60.0;

fn click_landlord(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    body: Query<&GlobalTransform, With<LandlordBody>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(window) = window.single() else { return };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };
    let hit = body.iter().any(|b| {
        camera
            .world_to_viewport(cam_tf, b.translation() + Vec3::Y * 1.2)
            .is_ok_and(|p| p.distance(cursor) < CLICK_RADIUS)
    });
    if hit {
        commands.client_trigger(BotherLandlord);
    }
}

/// New round: back to right after the inciting incident.
fn reset_tenancy(
    _reset: On<crate::features::objective::RoundReset>,
    mut tenancy: Query<&mut Tenancy>,
) {
    for mut t in &mut tenancy {
        *t = Tenancy::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::organ_currency::starting_purse;

    #[test]
    fn the_whole_story() {
        let mut t = Tenancy::default();
        assert!(t.rent > OLD_RENT, "the inciting incident raises the rent");
        let mut purse = starting_purse();
        assert!(!t.bother(&mut purse), "paying rent is legal");
        assert!(purse.0.is_empty());
        assert_eq!(t.paid, 21);
        for _ in 0..RAISES_BEFORE_CRIME {
            t.tick(RAISE_SECS + 1.0);
        }
        assert_eq!(t.act, Act3::Crime);
        assert!(t.bother(&mut purse), "pickpocketing harms him");
        assert_eq!(purse.0.len(), 1);
        for _ in 1..HEISTS_BEFORE_BOSS {
            t.bother(&mut purse);
        }
        assert_eq!(t.act, Act3::Boss);
        for _ in 0..BOSS_HP {
            t.bother(&mut purse);
        }
        assert_eq!(t.act, Act3::Won);
        t.tick(RAISE_SECS * 10.0);
        assert_eq!(t.act, Act3::Won);
    }
}
