//! Tigers (#472). "And by tigers I mean": every time you adjust the mouse
//! look speed and go back to the game, it starts slowly drifting up or down
//! (picked at random) and keeps going until you adjust it again. No actual
//! tigers. It follows your mouse everywhere, so it isn't tied to a site.

use bevy::prelude::*;
use serde::Deserialize;

use crate::controls::PadSettings;
use crate::ui::Overlay;
use friendslop_base::shared::controls::MouseLook;

/// How the drift goes (`data/game/tigers.ron`).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Speed is multiplied (or divided) by e^(rate × minutes).
    pub rate_per_minute: f32,
    /// The drift stops at these multiples of the speed you set.
    pub min: f32,
    pub max: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/game/tigers.ron");

/// The drift on top of your setting.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct Tigers {
    /// Multiplies the "Mouse look" setting.
    pub drift: f32,
    /// -1 down, 1 up, 0 still.
    pub dir: f32,
    /// You changed the setting since the last pick.
    pub adjusted: bool,
    /// The setting as last seen, to notice changes.
    seen: f32,
    was_open: bool,
}

impl Default for Tigers {
    fn default() -> Self {
        Self {
            drift: 1.0,
            dir: 0.0,
            adjusted: false,
            seen: f32::NAN,
            was_open: false,
        }
    }
}

/// One step of the drift: `drift` after `secs` sliding in `dir`.
pub fn drift_step(drift: f32, dir: f32, secs: f32, t: &Tuning) -> f32 {
    let next = drift * (dir * t.rate_per_minute * secs / 60.0).exp();
    let (lo, hi) = (t.min.min(t.max), t.max.max(t.min));
    next.clamp(lo, hi)
}

/// What happens to the tigers this frame, given the setting and whether the
/// settings are open; `coin` picks up (true) or down.
pub fn update(tig: &mut Tigers, setting: f32, open: bool, coin: bool, secs: f32, t: &Tuning) {
    if tig.seen.is_nan() {
        tig.seen = setting;
    }
    if (setting - tig.seen).abs() > 1e-4 {
        // Adjusted: back to what you asked for, and hold still.
        tig.seen = setting;
        tig.drift = 1.0;
        tig.dir = 0.0;
        tig.adjusted = true;
    }
    if tig.was_open && !open && tig.adjusted {
        tig.dir = if coin { 1.0 } else { -1.0 };
        tig.adjusted = false;
    }
    tig.was_open = open;
    if !open {
        tig.drift = drift_step(tig.drift, tig.dir, secs, t);
    }
}

fn run(
    time: Res<Time<Real>>,
    pad: Res<PadSettings>,
    overlay: Option<Res<Overlay>>,
    mut tig: ResMut<Tigers>,
    mut look: ResMut<MouseLook>,
) {
    let open = overlay.is_some_and(|o| o.settings);
    let coin = time.elapsed().subsec_nanos() & 1024 == 0;
    update(
        &mut tig,
        pad.mouse,
        open,
        coin,
        time.delta_secs(),
        &TUNING.get(),
    );
    look.set_if_neq(MouseLook(pad.mouse * tig.drift));
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Tigers>()
        .init_resource::<MouseLook>()
        .add_systems(Update, run.run_if(in_state(crate::AppState::InGame)));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t() -> Tuning {
        Tuning {
            rate_per_minute: 0.1,
            min: 0.2,
            max: 5.0,
        }
    }

    #[test]
    fn still_until_you_adjust_it() {
        let mut g = Tigers::default();
        update(&mut g, 1.0, false, true, 60.0, &t());
        assert_eq!(g.drift, 1.0);
    }

    #[test]
    fn drifts_after_adjusting_and_stops_on_the_next_adjust() {
        let mut g = Tigers::default();
        update(&mut g, 1.0, true, true, 0.1, &t());
        update(&mut g, 1.5, true, true, 0.1, &t());
        update(&mut g, 1.5, false, true, 60.0, &t());
        assert!(g.drift > 1.05, "{}", g.drift);
        update(&mut g, 1.5, false, true, 600.0, &t());
        let up = g.drift;
        // Reopen and adjust: reset and still while open.
        update(&mut g, 1.5, true, false, 1.0, &t());
        update(&mut g, 1.25, true, false, 1.0, &t());
        assert_eq!(g.drift, 1.0);
        update(&mut g, 1.25, false, false, 60.0, &t());
        assert!(g.drift < 0.95 && up > 1.5);
    }

    #[test]
    fn opening_without_adjusting_keeps_the_slide() {
        let mut g = Tigers::default();
        update(&mut g, 1.0, true, true, 0.1, &t());
        update(&mut g, 2.0, true, true, 0.1, &t());
        update(&mut g, 2.0, false, true, 60.0, &t());
        update(&mut g, 2.0, true, false, 1.0, &t());
        update(&mut g, 2.0, false, false, 60.0, &t());
        assert_eq!(g.dir, 1.0);
    }

    #[test]
    fn clamped() {
        assert_eq!(drift_step(1.0, 1.0, 1e6, &t()), 5.0);
        assert_eq!(drift_step(1.0, -1.0, 1e6, &t()), 0.2);
    }
}
