//! Nonsensical RPG stats. From issue #7, reworked by #48.
//!
//! #7's Wombility, Rumpet and John were only examples, so #48 replaced them
//! with stats in the same spirit. Every player gets six replicated stats,
//! rolled 1-10 by the server when they join:
//! - **Spoonth**: everyone can read the stats from further away; the stats
//!   text grows with the room's best Spoonth.
//! - **Glimbo**: at 9+ a lone teacup slowly orbits the middle of the world.
//! - **Frumbly**: increases bread affinity in bakeries. There is no bread and
//!   there are no bakeries, so Frumbly does nothing yet.
//! - **Hrumph**: in a duel you move Hrumph x 4% faster, out of sheer huffiness.
//! - **Wobblesome**: the stats text wobbles side to side, harder with the
//!   room's best Wobblesome.
//! - **Pelf**: the stats text turns gold, more the higher the room's best Pelf.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::player::Player;

/// Glimbo at which the teacup appears.
pub const TEACUP_GLIMBO: u8 = 9;

/// A player's stats, each 1..=10.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stats {
    pub spoonth: u8,
    pub glimbo: u8,
    pub frumbly: u8,
    pub hrumph: u8,
    pub wobblesome: u8,
    pub pelf: u8,
}

impl Stats {
    /// Roll stats from numbers in `0.0..1.0`, one per stat in field order.
    pub fn roll(r: [f32; 6]) -> Self {
        let pick = |r: f32| 1 + (r.clamp(0.0, 0.999_999) * 10.0) as u8;
        Self {
            spoonth: pick(r[0]),
            glimbo: pick(r[1]),
            frumbly: pick(r[2]),
            hrumph: pick(r[3]),
            wobblesome: pick(r[4]),
            pelf: pick(r[5]),
        }
    }

    pub fn all(&self) -> [u8; 6] {
        [
            self.spoonth,
            self.glimbo,
            self.frumbly,
            self.hrumph,
            self.wobblesome,
            self.pelf,
        ]
    }
}

/// Stats text size from the room's best Spoonth: 13 at 0, up to 23 at 10.
pub fn spoonth_font_size(spoonth: u8) -> f32 {
    13.0 + f32::from(spoonth.min(10))
}

/// Duel speed multiplier from Hrumph: `1 + h * 0.04`.
pub fn hrumph_speed(hrumph: u8) -> f32 {
    1.0 + f32::from(hrumph.min(10)) * 0.04
}

/// Sideways wobble of the stats text (px) at time `t` seconds.
pub fn wobble_px(wobblesome: u8, t: f32) -> f32 {
    f32::from(wobblesome.min(10)) * 1.5 * (t * 2.0).sin()
}

/// How far (0..1) the stats text is tinted gold by Pelf.
pub fn pelf_gold(pelf: u8) -> f32 {
    f32::from(pelf.min(10)) / 10.0
}

pub fn protocol(app: &mut App) {
    app.replicate::<Stats>()
        .add_observer(reroll_stats)
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

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_stats_hud)
        .add_systems(
            Update,
            (
                update_stats_hud.run_if(crate::world::hud_tick()),
                summon_teacup,
                orbit_teacup,
            )
                .run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct StatsHud;

const HUD_LEFT: f32 = 12.0;

fn spawn_stats_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextFont::from_font_size(13.0),
        StatsHud,
        crate::hud::slot(crate::hud::HudSlot::Scoreboard),
        DespawnOnExit(AppState::InGame),
    ));
}

fn best(players: &Query<(&Player, &Stats)>, f: impl Fn(&Stats) -> u8) -> u8 {
    players.iter().map(|(_, s)| f(s)).max().unwrap_or(0)
}

fn update_stats_hud(
    time: Res<Time>,
    players: Query<(&Player, &Stats)>,
    mut hud: Query<(&mut Text, &mut TextFont, &mut TextColor, &mut Node), With<StatsHud>>,
) {
    let mut rows: Vec<_> = players.iter().collect();
    rows.sort_by_key(|(p, _)| p.id);
    let mut lines = vec!["stats".to_string()];
    for (p, s) in rows {
        lines.push(format!(
            "player {}: Spoonth {} / Glimbo {} / Frumbly {} / Hrumph {} / Wobblesome {} / Pelf {}",
            p.id, s.spoonth, s.glimbo, s.frumbly, s.hrumph, s.wobblesome, s.pelf
        ));
    }
    let want = lines.join("\n");
    let size = bevy::text::FontSize::Px(spoonth_font_size(best(&players, |s| s.spoonth)));
    let color = Color::WHITE.mix(
        &Color::srgb(1.0, 0.8, 0.2),
        pelf_gold(best(&players, |s| s.pelf)),
    );
    let left =
        px(HUD_LEFT + 15.0 + wobble_px(best(&players, |s| s.wobblesome), time.elapsed_secs()));
    for (mut text, mut font, mut tint, mut node) in &mut hud {
        if text.0 != want {
            text.0.clone_from(&want);
        }
        if font.font_size != size {
            font.font_size = size;
        }
        if tint.0 != color {
            tint.0 = color;
        }
        if node.left != left {
            node.left = left;
        }
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

/// New round: everyone's stats are rolled again.
fn reroll_stats(
    _reset: On<crate::features::objective::RoundReset>,
    mut dice: ResMut<StatDice>,
    mut players: Query<&mut Stats>,
) {
    for mut s in &mut players {
        *s = Stats::roll(dice.roll(), dice.roll(), dice.roll());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_are_one_to_ten() {
        for i in 0..100 {
            let r = i as f32 / 100.0;
            for v in Stats::roll([r; 6]).all() {
                assert!((1..=10).contains(&v));
            }
        }
        assert_eq!(Stats::roll([0.0; 6]).spoonth, 1);
        assert_eq!(Stats::roll([0.999, 0.0, 0.0, 0.0, 0.0, 0.0]).spoonth, 10);
        assert_eq!(Stats::roll([0.0, 0.0, 0.0, 0.0, 0.0, 0.999]).pelf, 10);
    }

    #[test]
    fn spoonth_makes_stats_bigger() {
        assert!(spoonth_font_size(10) > spoonth_font_size(1));
        assert_eq!(spoonth_font_size(0), 13.0);
    }

    #[test]
    fn hrumph_wobblesome_and_pelf_scale() {
        assert!(hrumph_speed(10) > hrumph_speed(1));
        assert!(hrumph_speed(1) > 1.0);
        assert_eq!(wobble_px(5, 0.0), 0.0);
        assert!(wobble_px(10, 0.8).abs() > wobble_px(1, 0.8).abs());
        assert_eq!(pelf_gold(10), 1.0);
    }
}
