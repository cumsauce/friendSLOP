//! Pointless achievements: the game congratulates you on things nobody asked
//! you to do. From issue #378 ("come up with your own signature addition"),
//! the AI's own pick.
//!
//! Each peer counts its own player's numbers (metres walked, seconds stood
//! still in one go, full turns spun on the spot, seconds played) and, when one
//! passes a line in `assets/data/side/achievements.ron`, shows a toast like
//! "Achievement unlocked: Legs. You walked 100 m. Nobody is impressed."
//! Local only: nothing is sent, saved or rewarded.

use std::f32::consts::TAU;

use bevy::prelude::*;
use serde::Deserialize;

use crate::AppState;
use crate::features::fps::Walker;
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player};

/// Which of your numbers an achievement watches.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stat {
    /// Metres walked (on the ground, any direction).
    Walked,
    /// Seconds stood still without a break.
    Still,
    /// Full turns spun (looking around counts).
    Spun,
    /// Seconds in the game.
    Played,
}

/// One achievement.
#[derive(Deserialize, Clone, Debug)]
pub struct Achievement {
    /// The number it watches.
    pub stat: Stat,
    /// Earned once that number reaches this.
    pub at: f32,
    /// Its name ("Legs").
    pub title: String,
    /// The line under it ("You walked 100 m. Nobody is impressed.").
    pub line: String,
}

/// Tuning and writing: `assets/data/side/achievements.ron` (read at run time, edit without a rebuild).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Moving less than this many metres a second counts as standing still.
    pub still_speed: f32,
    /// A jump bigger than this in one frame (a teleport, a respawn) isn't walking.
    pub max_step: f32,
    /// Every achievement, in any order.
    pub list: Vec<Achievement>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/achievements.ron");

/// Your numbers so far.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Stats {
    pub walked: f32,
    pub still: f32,
    /// Radians turned, either way.
    pub turned: f32,
    pub played: f32,
}

impl Stats {
    /// The value of one stat.
    pub fn get(&self, stat: Stat) -> f32 {
        match stat {
            Stat::Walked => self.walked,
            Stat::Still => self.still,
            Stat::Spun => self.turned / TAU,
            Stat::Played => self.played,
        }
    }

    /// The numbers after a frame of `dt` seconds in which the feet moved by
    /// `step` (on the ground) and the view turned by `turn` radians.
    pub fn after(self, dt: f32, step: f32, turn: f32, tuning: &Tuning) -> Stats {
        let walking = step <= tuning.max_step;
        let moving = dt > 0.0 && step > tuning.still_speed * dt;
        Stats {
            walked: self.walked + if walking { step } else { 0.0 },
            still: if moving { 0.0 } else { self.still + dt },
            turned: self.turned + turn.abs(),
            played: self.played + dt,
        }
    }
}

/// The smallest turn from yaw `a` to yaw `b`, in radians (-PI..PI).
pub fn turn_between(a: f32, b: f32) -> f32 {
    (b - a + std::f32::consts::PI).rem_euclid(TAU) - std::f32::consts::PI
}

/// Indices of the achievements in `list` that `stats` has reached and
/// `earned` doesn't have yet.
pub fn newly_earned(stats: &Stats, list: &[Achievement], earned: &[usize]) -> Vec<usize> {
    list.iter()
        .enumerate()
        .filter(|(i, a)| !earned.contains(i) && stats.get(a.stat) >= a.at)
        .map(|(i, _)| i)
        .collect()
}

/// The toast for one achievement.
pub fn announce(a: &Achievement) -> String {
    format!("Achievement unlocked: {}. {}", a.title, a.line)
}

/// This peer's counting: the numbers, what's earned, and last frame's feet.
#[derive(Resource, Default)]
struct Progress {
    stats: Stats,
    earned: Vec<usize>,
    last: Option<(Vec3, f32)>,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Progress>()
        .add_systems(Update, count.run_if(in_state(AppState::InGame)))
        .add_systems(OnExit(AppState::InGame), forget);
}

fn forget(mut progress: ResMut<Progress>) {
    *progress = Progress::default();
}

fn count(
    time: Res<Time>,
    mut progress: ResMut<Progress>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &Walker)>,
    toast: Option<ResMut<Toast>>,
) {
    let Some(me) = me else { return };
    let Some((_, w)) = walkers.iter().find(|(p, _)| p.id == me.0) else {
        return;
    };
    let tuning = TUNING.get();
    let (step, turn) = match progress.last {
        Some((pos, yaw)) => (pos.xz().distance(w.pos.xz()), turn_between(yaw, w.yaw)),
        None => (0.0, 0.0),
    };
    progress.last = Some((w.pos, w.yaw));
    progress.stats = progress.stats.after(time.delta_secs(), step, turn, &tuning);
    let new = newly_earned(&progress.stats, &tuning.list, &progress.earned);
    if new.is_empty() {
        return;
    }
    let Some(mut toast) = toast else { return };
    for i in new {
        progress.earned.push(i);
        if let Some(a) = tuning.list.get(i) {
            toast.show(announce(a));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuning() -> Tuning {
        Tuning {
            still_speed: 0.2,
            max_step: 5.0,
            list: vec![
                Achievement {
                    stat: Stat::Walked,
                    at: 10.0,
                    title: "Legs".into(),
                    line: "You walked.".into(),
                },
                Achievement {
                    stat: Stat::Spun,
                    at: 1.0,
                    title: "Dizzy".into(),
                    line: "You spun.".into(),
                },
            ],
        }
    }

    #[test]
    fn walking_counts_but_teleports_dont() {
        let t = tuning();
        let s = Stats::default().after(1.0, 3.0, 0.0, &t);
        assert_eq!(s.walked, 3.0);
        assert_eq!(s.still, 0.0);
        let s = s.after(1.0, 50.0, 0.0, &t);
        assert_eq!(s.walked, 3.0);
    }

    #[test]
    fn standing_still_adds_up_and_moving_resets_it() {
        let t = tuning();
        let s = Stats::default()
            .after(1.0, 0.0, 0.0, &t)
            .after(1.0, 0.1, 0.0, &t);
        assert_eq!(s.still, 2.0);
        assert_eq!(s.after(1.0, 1.0, 0.0, &t).still, 0.0);
    }

    #[test]
    fn turns_wrap_the_short_way() {
        let near_pi = std::f32::consts::PI - 0.1;
        assert!((turn_between(near_pi, -near_pi) - 0.2).abs() < 1e-4);
        assert!((turn_between(0.0, -0.5) + 0.5).abs() < 1e-4);
    }

    #[test]
    fn each_achievement_is_earned_once() {
        let t = tuning();
        let s = Stats {
            walked: 12.0,
            turned: TAU * 1.5,
            ..default()
        };
        assert_eq!(newly_earned(&s, &t.list, &[]), vec![0, 1]);
        assert_eq!(newly_earned(&s, &t.list, &[0]), vec![1]);
        assert!(newly_earned(&Stats::default(), &t.list, &[]).is_empty());
        assert_eq!(
            announce(&t.list[0]),
            "Achievement unlocked: Legs. You walked."
        );
    }
}
