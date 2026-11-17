//! How loud you are. From the user: "add (in general) a 'noise' radius to the
//! player. it is very small/almost nothing crouched, very small walking,
//! large running, large jumping. i think it should be a generic system".
//!
//! Every frame on the server, each walker is put in one of the noise levels
//! below (idle, crouch, walk, run, air) and the result lands in the
//! [`Noises`] resource: every sound being made this frame, and how far it
//! carries. Anything that listens — today the Fulton St hunter's brain, later
//! villagers, rats, whatever — reads [`Noises`] and does its own hearing:
//! you're heard when you stand within a noise's radius of the listener.
//!
//! There's no crouch input yet, so nothing lands in [`Level::Crouch`] today;
//! the level and its radius are here for when one arrives. Jumping and
//! falling are `air`. Tuning: `assets/data/play/noise.ron`.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::Deserialize;

use super::fps::{WalkInput, Walker};
use crate::kcc::Kcc;

/// Tuning numbers: `assets/data/play/noise.ron` (read at run time).
#[derive(Deserialize, Debug, Clone)]
pub struct Tuning {
    /// How far each level carries (m). Idle carries nothing.
    pub crouch: f32,
    pub walk: f32,
    pub run: f32,
    /// Jumping and falling.
    pub air: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            crouch: 1.5,
            walk: 5.0,
            run: 12.0,
            air: 10.0,
        }
    }
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/play/noise.ron");

/// How loud a body is being right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Idle,
    Crouch,
    Walk,
    Run,
    /// Off the ground: jumping or falling.
    Air,
}

/// One body's noise this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Noise {
    /// Feet of the body making it.
    pub at: Vec3,
    pub level: Level,
    /// How far it carries (m): heard within this of `at`.
    pub radius: f32,
}

impl Level {
    /// How far this level carries.
    pub fn radius(self, t: &Tuning) -> f32 {
        match self {
            Level::Idle => 0.0,
            Level::Crouch => t.crouch,
            Level::Walk => t.walk,
            Level::Run => t.run,
            Level::Air => t.air,
        }
    }
}

/// Which level a walker in this state is making, if any. Pure: the decisions
/// here, a thin system as the wiring.
pub fn made(t: &Tuning, stick: Vec2, run: bool, airborne: bool) -> Option<Noise> {
    let level = if airborne {
        Level::Air
    } else if stick == Vec2::ZERO {
        Level::Idle
    } else if run {
        Level::Run
    } else {
        Level::Walk
    };
    let radius = level.radius(t);
    (radius > 0.0).then_some(Noise {
        at: Vec3::ZERO,
        level,
        radius,
    })
}

/// Is `ear` within hearing of any of `noises`? The loudest one that reaches
/// it, if so.
pub fn heard(noises: &[Noise], ear: Vec3) -> Option<Noise> {
    noises
        .iter()
        .copied()
        .filter(|n| n.at.distance(ear) <= n.radius)
        .max_by(|a, b| a.radius.total_cmp(&b.radius))
}

/// Everything being made this frame (server only).
#[derive(Resource, Default, Debug)]
pub struct Noises(pub Vec<Noise>);

/// Ordering hook: this runs after movement (it reads where bodies ended up),
/// before anything that listens.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct NoiseSet;

pub fn plugin(app: &mut App) {
    app.init_resource::<Noises>().add_systems(
        Update,
        listen
            .in_set(NoiseSet)
            .run_if(in_state(ServerState::Running)),
    );
}

/// Server: work out every walker's noise for this frame.
fn listen(mut noises: ResMut<Noises>, walkers: Query<(&Walker, &WalkInput, Option<&Kcc>)>) {
    let t = TUNING.get();
    noises.0 = walkers
        .iter()
        .filter_map(|(w, input, kcc)| {
            // Without a controller (headless tests) a body on the floor is
            // judged by its stick alone.
            let airborne = kcc
                .map(|k| k.grounded.is_none() && k.velocity != Vec3::ZERO)
                .unwrap_or(false);
            made(&t, input.stick, input.is_running(), airborne).map(|mut n| {
                n.at = w.pos;
                n
            })
        })
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_loud_in_the_right_order() {
        let t = Tuning::default();
        assert!(
            t.crouch < t.walk && t.walk < t.run,
            "crouch < walk < run: {} {} {}",
            t.crouch,
            t.walk,
            t.run
        );
        // Idle makes nothing at all.
        assert_eq!(made(&t, Vec2::ZERO, false, false), None);
        // Crouch is quiet but there.
        let crouch = Level::Crouch.radius(&t);
        assert!(crouch > 0.0 && crouch < t.walk);
        assert_eq!(
            made(&t, Vec2::ONE, true, false).map(|n| n.radius),
            Some(t.run)
        );
        assert_eq!(
            made(&t, Vec2::ONE, false, false).map(|n| n.radius),
            Some(t.walk)
        );
        // Jumping is loud even standing still.
        assert!(made(&t, Vec2::ZERO, false, true).is_some());
    }

    #[test]
    fn hearing_is_distance_and_prefers_the_loudest() {
        let loud = Noise {
            at: Vec3::new(30.0, 0.0, 0.0),
            level: Level::Run,
            radius: 12.0,
        };
        let quiet = Noise {
            at: Vec3::new(1.0, 0.0, 0.0),
            level: Level::Walk,
            radius: 5.0,
        };
        // Near the quiet one and far from the loud one: only it reaches.
        assert_eq!(heard(&[loud, quiet], Vec3::ZERO), Some(quiet));
        // Within reach of both, the louder wins.
        assert_eq!(heard(&[loud, quiet], Vec3::new(26.0, 0.0, 0.0)), Some(loud));
        // Out of earshot of both: nothing.
        assert_eq!(heard(&[loud, quiet], Vec3::new(15.0, 0.0, 0.0)), None);
    }
}
