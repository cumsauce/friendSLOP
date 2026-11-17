//! Stamina: sprinting and climbing run on a budget. From a friend idea
//! (asked for directly, no issue number): Breath of the Wild / Genshin-like,
//! so sprinting drains it gently and climbing drains it hard.
//!
//! Every player carries a replicated [`Stamina`] (the host keeps it and every
//! peer sees it). Holding Shift on the ground spends it; climbing a face
//! spends it faster. When it hits zero the walker is exhausted: the
//! kinematic controller won't run (`crate::kcc`) and [`super::climb`] lets go
//! of the wall, so a spent climber falls to the ground. Resting on the ground
//! brings it back; at [`Tuning::again`] of full they can sprint and climb
//! again.
//!
//! The bar is a card in the bottom-right status stack (`ui::status`), drawn
//! with the gas and stoned meters: it appears while stamina is being spent or
//! is still coming back, and goes once it's full.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::features::fps::{WalkInput, Walker};
use crate::player::Player;

/// Tuning numbers: `assets/data/play/stamina.ron` (read at run time, edit
/// without a rebuild).
#[derive(serde::Deserialize, Debug, Clone)]
pub struct Tuning {
    /// Most stamina a walker has.
    pub max: f32,
    /// What a second of sprinting costs.
    pub sprint_drain: f32,
    /// What a second of climbing costs (climbing is harder work).
    pub climb_drain: f32,
    /// How fast it comes back while resting on the ground.
    pub recover: f32,
    /// Share of `max` you need back before you can sprint or climb again.
    pub again: f32,
}

impl Default for Tuning {
    fn default() -> Self {
        Self {
            max: 100.0,
            sprint_drain: 8.0,
            climb_drain: 14.0,
            recover: 22.0,
            again: 0.3,
        }
    }
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/play/stamina.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// A player's stamina. Lives on the `Player` entity. Replicated.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Stamina {
    pub value: f32,
    pub max: f32,
    /// Ran out: no sprinting or climbing until it's back to [`Tuning::again`].
    pub exhausted: bool,
}

impl Stamina {
    /// Full breath.
    pub fn full(max: f32) -> Self {
        Self {
            value: max,
            max,
            exhausted: false,
        }
    }

    /// How full the bar is, 0..=1.
    pub fn fraction(&self) -> f32 {
        if self.max <= 0.0 {
            return 0.0;
        }
        (self.value / self.max).clamp(0.0, 1.0)
    }

    /// Can this walker sprint?
    pub fn can_sprint(&self) -> bool {
        !self.exhausted
    }

    /// Can this walker hold on to a face?
    pub fn can_climb(&self) -> bool {
        !self.exhausted
    }
}

impl Default for Stamina {
    fn default() -> Self {
        Self::full(Tuning::default().max)
    }
}

/// What a walker is spending stamina on this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Use {
    Sprint,
    Climb,
}

/// What this frame's state costs, if anything. Pure: the decisions here, a
/// thin system as the wiring.
pub fn spending(
    stamina: &Stamina,
    climbing: bool,
    running: bool,
    moving: bool,
    grounded: bool,
) -> Option<Use> {
    if climbing {
        Some(Use::Climb)
    } else if running && moving && grounded && stamina.can_sprint() {
        Some(Use::Sprint)
    } else {
        None
    }
}

/// One frame of stamina: spend what this frame is doing, or recover on the
/// ground, and flag exhaustion. Pure.
pub fn step(
    t: &Tuning,
    stamina: Stamina,
    using: Option<Use>,
    recovering: bool,
    dt: f32,
) -> Stamina {
    let value = match using {
        Some(Use::Sprint) => stamina.value - t.sprint_drain * dt,
        Some(Use::Climb) => stamina.value - t.climb_drain * dt,
        None if recovering => stamina.value + t.recover * dt,
        None => stamina.value,
    };
    let value = value.clamp(0.0, t.max);
    let mut exhausted = stamina.exhausted;
    if using.is_some() {
        exhausted |= value <= 0.0;
    } else if exhausted && value >= t.max * t.again {
        exhausted = false;
    }
    Stamina {
        value,
        max: t.max,
        exhausted,
    }
}

pub fn protocol(app: &mut App) {
    app.replicate::<Stamina>().add_observer(give_stamina);
}

/// Server: everyone gets a full breath when they join.
fn give_stamina(add: On<Add<Player>>, server: Res<State<ServerState>>, mut commands: Commands) {
    if *server.get() != ServerState::Running {
        return;
    }
    commands
        .entity(add.entity)
        .insert(Stamina::full(TUNING.get().max));
}

pub fn plugin(app: &mut App) {
    // After `climb`, whose decision is what this frame is spending. The bar
    // itself is a status card in the game's HUD (`ui::status`).
    app.add_systems(
        Update,
        drain
            .after(super::climb::climb)
            .run_if(in_state(ServerState::Running)),
    );
}

/// Server: every walker's stamina this frame.
fn drain(time: Res<Time>, mut walkers: Query<(&Walker, &WalkInput, &mut Stamina)>) {
    let t = TUNING.get();
    let dt = time.delta_secs().min(0.1);
    for (walker, input, mut stamina) in &mut walkers {
        let using = spending(
            &stamina,
            input.climbing,
            input.is_running(),
            input.stick != Vec2::ZERO,
            walker.grounded,
        );
        // Only the ground gives it back (a spent climber falls first).
        let resting = walker.grounded && !input.climbing;
        let next = step(&t, *stamina, using, resting, dt);
        if *stamina != next {
            *stamina = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}");
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
        (*TUNING.get()).clone()
    }

    #[test]
    fn the_data_file_parses_and_has_everything() {
        let t = t();
        assert!(t.max > 0.0 && t.sprint_drain > 0.0 && t.climb_drain > 0.0);
        assert!(t.climb_drain > t.sprint_drain, "climbing costs more");
        assert!(t.recover > 0.0);
        assert!(t.again > 0.0 && t.again < 1.0);
    }

    #[test]
    fn sprinting_drains_and_climbing_drains_faster() {
        let t = t();
        let full = Stamina::full(t.max);
        let walked = step(&t, full, None, true, 1.0);
        assert_eq!(walked, full, "walking is free");
        let ran = step(&t, full, Some(Use::Sprint), false, 1.0);
        let climbed = step(&t, full, Some(Use::Climb), false, 1.0);
        assert!(ran.value < full.value, "sprinting costs");
        assert!(
            climbed.value < ran.value,
            "a second of climbing costs more than a second of sprinting"
        );
    }

    #[test]
    fn a_spent_walker_is_exhausted_until_they_get_some_back() {
        let t = t();
        let nearly = Stamina {
            value: 1.0,
            max: t.max,
            exhausted: false,
        };
        let spent = step(&t, nearly, Some(Use::Sprint), false, 1.0);
        assert_eq!(spent.value, 0.0);
        assert!(spent.exhausted && !spent.can_sprint() && !spent.can_climb());

        // Nothing while they're not on the ground...
        let falling = step(&t, spent, None, false, 1.0);
        assert_eq!(falling, spent, "no recovery off the ground");
        // ...then it comes back, but only past the line does it count.
        let mut now = spent;
        for _ in 0..60 {
            now = step(&t, now, None, true, 0.05);
        }
        assert!(now.value >= t.max * t.again && now.can_sprint());
    }

    #[test]
    fn recovery_stops_at_full_and_exhaustion_latches() {
        let t = t();
        let full = Stamina::full(t.max);
        assert_eq!(step(&t, full, None, true, 10.0), full);

        let spent = Stamina {
            value: 0.0,
            max: t.max,
            exhausted: true,
        };
        // A tiny sip is below the line: still exhausted.
        let sip = Stamina {
            value: t.max * t.again * 0.5,
            ..spent
        };
        let still = step(&t, sip, None, false, 0.0);
        assert!(still.exhausted, "under the line you still can't act");
        assert!(!still.can_sprint() && !still.can_climb());
    }

    #[test]
    fn spending_needs_the_ground_and_a_trying_walker() {
        let t = t();
        let full = Stamina::full(t.max);
        assert_eq!(spending(&full, false, false, true, true), None, "no shift");
        assert_eq!(
            spending(&full, false, true, false, true),
            None,
            "standing still"
        );
        assert_eq!(
            spending(&full, false, true, true, false),
            None,
            "in the air"
        );
        assert_eq!(spending(&full, false, true, true, true), Some(Use::Sprint));
        assert_eq!(
            spending(&full, true, false, false, false),
            Some(Use::Climb),
            "climbing is climbing wherever the body is"
        );
        let spent = Stamina {
            exhausted: true,
            ..full
        };
        assert_eq!(
            spending(&spent, false, true, true, true),
            None,
            "an exhausted walker can't sprint"
        );
    }
}
