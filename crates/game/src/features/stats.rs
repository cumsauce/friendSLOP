//! Nonsensical RPG stats. From issue #7.
//!
//! Every player gets three replicated stats, rolled 1-10 by the server when
//! they join, straight from the issue:
//! - **Wombility**: in a duel (combat) your speed follows `sin(x)` of time,
//!   swinging harder the more Wombility you have. It should also make
//!   purple-haired characters like you and green-haired ones despise you,
//!   but nobody has hair yet.
//! - **Rumpet**: tints walls (the duel arena's) yellow, more the higher the
//!   room's best Rumpet is. At 8+ rabbits appear on the floor.
//! - **John**: a slight increase to jump height in meadows. There is no
//!   jumping and there are no meadows, so John does nothing yet.
//!
//! More stats in the same spirit, from #48 (the three above were only
//! examples, but they stay so nothing is lost):
//! - **Spoonth**: everyone can read the stats from further away; the stats
//!   text grows with the room's best Spoonth.
//! - **Glimbo**: at 9+ a lone teacup slowly orbits the middle of the world.
//! - **Frumbly**: increases bread affinity in bakeries. There is no bread and
//!   there are no bakeries, so Frumbly does nothing yet.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::player::Player;

/// Rumpet at which rabbits appear.
pub const RABBIT_RUMPET: u8 = 8;
/// Glimbo at which the teacup appears.
pub const TEACUP_GLIMBO: u8 = 9;

/// A player's stats, each 1..=10.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    pub wombility: u8,
    pub rumpet: u8,
    pub john: u8,
    pub spoonth: u8,
    pub glimbo: u8,
    pub frumbly: u8,
}

impl Stats {
    /// Roll stats from numbers in `0.0..1.0`, one per stat in field order.
    pub fn roll(r: [f32; 6]) -> Self {
        let pick = |r: f32| 1 + (r.clamp(0.0, 0.999_999) * 10.0) as u8;
        Self {
            wombility: pick(r[0]),
            rumpet: pick(r[1]),
            john: pick(r[2]),
            spoonth: pick(r[3]),
            glimbo: pick(r[4]),
            frumbly: pick(r[5]),
        }
    }

    pub fn all(&self) -> [u8; 6] {
        [
            self.wombility,
            self.rumpet,
            self.john,
            self.spoonth,
            self.glimbo,
            self.frumbly,
        ]
    }
}

/// Duel speed multiplier from Wombility at time `t` seconds: `1 + w/12 * sin(t)`.
/// Always stays positive (at most ±83%).
pub fn wombility_speed(wombility: u8, t: f32) -> f32 {
    1.0 + f32::from(wombility.min(10)) / 12.0 * t.sin()
}

/// How far (0..1) walls are tinted yellow by a Rumpet.
pub fn rumpet_tint(rumpet: u8) -> f32 {
    f32::from(rumpet.min(10)) / 10.0
}

/// Stats text size from the room's best Spoonth: 13 at 0, up to 23 at 10.
pub fn spoonth_font_size(spoonth: u8) -> f32 {
    13.0 + f32::from(spoonth.min(10))
}

pub fn protocol(app: &mut App) {
    app.replicate::<Stats>()
        .init_resource::<StatDice>()
        .add_observer(give_stats);
}

/// Server only: tiny deterministic random source.
#[derive(Resource)]
struct StatDice(u32);

impl Default for StatDice {
    fn default() -> Self {
        Self(0x0057_a757)
    }
}

impl StatDice {
    fn roll(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32
    }
}

fn give_stats(
    add: On<Add<Player>>,
    server: Res<State<ServerState>>,
    mut dice: ResMut<StatDice>,
    mut commands: Commands,
) {
    if *server.get() != ServerState::Running {
        return;
    }
    let stats = Stats::roll(std::array::from_fn(|_| dice.roll()));
    commands.entity(add.entity).insert(stats);
}

/// Marks a wall Rumpet can tint. Remembers its own colour the first time.
#[derive(Component, Default)]
pub struct RumpetWall {
    base: Option<Color>,
}

#[derive(Component)]
struct Rabbit {
    home: Vec3,
    phase: f32,
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_stats_hud)
        .add_systems(
            Update,
            (
                update_stats_hud,
                tint_walls,
                summon_rabbits,
                hop_rabbits,
                summon_teacup,
                orbit_teacup,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct StatsHud;

fn spawn_stats_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(13.0),
        StatsHud,
        Node {
            position_type: PositionType::Absolute,
            top: percent(35),
            left: px(12),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_stats_hud(
    players: Query<(&Player, &Stats)>,
    mut hud: Query<(&mut Text, &mut TextFont), With<StatsHud>>,
) {
    let mut rows: Vec<_> = players.iter().collect();
    rows.sort_by_key(|(p, _)| p.id);
    let mut lines = vec!["stats".to_string()];
    for (p, s) in rows {
        lines.push(format!(
            "player {}: Wombility {} / Rumpet {} / John {} / Spoonth {} / Glimbo {} / Frumbly {}",
            p.id, s.wombility, s.rumpet, s.john, s.spoonth, s.glimbo, s.frumbly
        ));
    }
    let want = lines.join("\n");
    let size = spoonth_font_size(players.iter().map(|(_, s)| s.spoonth).max().unwrap_or(0));
    for (mut text, mut font) in &mut hud {
        if text.0 != want {
            text.0.clone_from(&want);
        }
        let size = bevy::text::FontSize::Px(size);
        if font.font_size != size {
            font.font_size = size;
        }
    }
}

fn best_rumpet(players: &Query<&Stats>) -> u8 {
    players.iter().map(|s| s.rumpet).max().unwrap_or(0)
}

fn tint_walls(
    players: Query<&Stats>,
    mut walls: Query<(&mut RumpetWall, &MeshMaterial3d<StandardMaterial>)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let t = rumpet_tint(best_rumpet(&players));
    let yellow = Color::srgb(1.0, 0.9, 0.1);
    for (mut wall, mat) in &mut walls {
        let Some(current) = materials.get(&mat.0).map(|m| m.base_color) else {
            continue;
        };
        let base = *wall.base.get_or_insert(current);
        let want = base.mix(&yellow, t);
        if current != want
            && let Some(mut m) = materials.get_mut(&mat.0)
        {
            m.base_color = want;
        }
    }
}

fn summon_rabbits(
    mut commands: Commands,
    players: Query<&Stats>,
    rabbits: Query<Entity, With<Rabbit>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let want = best_rumpet(&players) >= RABBIT_RUMPET;
    if !want {
        for r in &rabbits {
            commands.entity(r).despawn();
        }
        return;
    }
    if !rabbits.is_empty() {
        return;
    }
    let body = meshes.add(Sphere::new(0.18));
    let ear = meshes.add(Capsule3d::new(0.03, 0.18));
    let white = materials.add(Color::srgb(0.97, 0.95, 0.92));
    for i in 0..4 {
        let a = i as f32 * 1.7;
        let home = Vec3::new(a.cos() * 4.0 - 2.0, 0.18, a.sin() * 4.0 + 3.0);
        commands
            .spawn((
                Mesh3d(body.clone()),
                MeshMaterial3d(white.clone()),
                Transform::from_translation(home),
                Rabbit {
                    home,
                    phase: i as f32,
                },
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|r| {
                for x in [-0.06, 0.06] {
                    r.spawn((
                        Mesh3d(ear.clone()),
                        MeshMaterial3d(white.clone()),
                        Transform::from_xyz(x, 0.25, 0.0),
                    ));
                }
            });
    }
}

fn hop_rabbits(time: Res<Time>, mut rabbits: Query<(&Rabbit, &mut Transform)>) {
    let t = time.elapsed_secs();
    for (r, mut tf) in &mut rabbits {
        let p = t * 3.0 + r.phase;
        tf.translation = r.home
            + Vec3::new(
                (p * 0.2).sin() * 0.8,
                (p.sin().abs()) * 0.3,
                (p * 0.2).cos() * 0.8,
            );
    }
}

#[derive(Component)]
struct Teacup;

fn summon_teacup(
    mut commands: Commands,
    players: Query<&Stats>,
    cups: Query<Entity, With<Teacup>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let want = players.iter().map(|s| s.glimbo).max().unwrap_or(0) >= TEACUP_GLIMBO;
    if !want {
        for c in &cups {
            commands.entity(c).despawn();
        }
        return;
    }
    if !cups.is_empty() {
        return;
    }
    let china = materials.add(Color::srgb(0.95, 0.95, 1.0));
    commands
        .spawn((
            Mesh3d(meshes.add(Cylinder::new(0.2, 0.25))),
            MeshMaterial3d(china.clone()),
            Transform::from_xyz(6.0, 2.5, 0.0),
            Teacup,
            DespawnOnExit(AppState::InGame),
        ))
        .with_child((
            Mesh3d(meshes.add(Torus::new(0.04, 0.09))),
            MeshMaterial3d(china),
            Transform::from_xyz(0.24, 0.0, 0.0)
                .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
        ));
}

fn orbit_teacup(time: Res<Time>, mut cups: Query<&mut Transform, With<Teacup>>) {
    let t = time.elapsed_secs() * 0.3;
    for mut tf in &mut cups {
        tf.translation = Vec3::new(t.cos() * 6.0, 2.5 + (t * 3.0).sin() * 0.2, t.sin() * 6.0);
        tf.rotation = Quat::from_rotation_y(-t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_are_one_to_ten() {
        for i in 0..100 {
            let r = i as f32 / 100.0;
            let s = Stats::roll([r; 6]);
            for v in s.all() {
                assert!((1..=10).contains(&v));
            }
        }
        assert_eq!(Stats::roll([0.0; 6]).wombility, 1);
        assert_eq!(Stats::roll([0.999, 0.0, 0.0, 0.0, 0.0, 0.0]).wombility, 10);
        assert_eq!(Stats::roll([0.0, 0.0, 0.0, 0.0, 0.0, 0.999]).frumbly, 10);
    }

    #[test]
    fn spoonth_makes_stats_bigger() {
        assert!(spoonth_font_size(10) > spoonth_font_size(1));
        assert_eq!(spoonth_font_size(0), 13.0);
    }

    #[test]
    fn wombility_speed_follows_sin_and_stays_positive() {
        assert_eq!(wombility_speed(10, 0.0), 1.0);
        assert!(wombility_speed(10, std::f32::consts::FRAC_PI_2) > 1.8);
        assert!(wombility_speed(10, -std::f32::consts::FRAC_PI_2) > 0.0);
        assert!(wombility_speed(1, 1.0) < wombility_speed(10, 1.0));
    }
}
