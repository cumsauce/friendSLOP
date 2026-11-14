//! The lake can kill you (#238, "what else should be able to kill you?").
//!
//! Wade into the deep middle of the west valley's lake and you can't swim:
//! stay with your feet more than [`Tuning::depth`] m under the water for
//! [`Tuning::secs`] seconds and you drown (you die, see `afterlife.rs`).
//! Climb out, even for a moment, and you get your breath back. The clock
//! stops while the world is frozen (`stillness.rs`) and while you're dead.
//!
//! It lives at the lake site (`data/base/sites.ron`) and only runs while
//! someone is there. Server only, no new art, nothing new on the wire.

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::features::fps::Walker;
use crate::player::Player;
use crate::sites;

/// Tuning numbers: `assets/data/afterlife/drowning.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Feet this far (metres) below the water's surface count as out of your depth.
    pub depth: f32,
    /// How long you can stay out of your depth before you drown.
    pub secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/afterlife/drowning.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Server only: how long one player has been out of their depth.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct Lungs {
    pub under: f32,
}

impl Lungs {
    /// Advance by `dt`; `deep` says whether they're out of their depth now.
    /// Returns true on the tick they drown (then they start over).
    pub fn tick(&mut self, deep: bool, dt: f32) -> bool {
        if !deep {
            self.under = 0.0;
            return false;
        }
        self.under += dt;
        if self.under >= tuning().secs {
            self.under = 0.0;
            return true;
        }
        false
    }
}

/// Are `feet` out of their depth in the lake?
pub fn out_of_depth(feet: Vec3) -> bool {
    crate::terrain::is_lake(feet.x, feet.z)
        && feet.y < crate::terrain::lake_level() - tuning().depth
}

pub fn protocol(app: &mut App) {
    app.add_systems(
        Update,
        watch
            .run_if(in_state(ServerState::Running))
            .run_if(sites::awake("lake")),
    );
}

/// Server: breathe in or out for every player; drown the ones who can't.
fn watch(
    mut commands: Commands,
    time: Res<Time>,
    still: Query<&super::stillness::Stillness>,
    mut players: Query<(
        Entity,
        &Player,
        &Walker,
        Option<&mut Lungs>,
        Has<super::afterlife::Dead>,
    )>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    for (e, player, walker, lungs, dead) in &mut players {
        let Some(mut lungs) = lungs else {
            commands.entity(e).insert(Lungs::default());
            continue;
        };
        let deep = !dead && out_of_depth(walker.pos);
        if lungs.tick(deep, time.delta_secs()) {
            commands.trigger(super::afterlife::Die { player: player.id });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load() {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            TUNING.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(TUNING.set_bytes(&bytes), "{path} doesn't parse");
    }

    #[test]
    fn staying_under_drowns_and_surfacing_resets() {
        load();
        let mut lungs = Lungs::default();
        let secs = tuning().secs;
        assert!(!lungs.tick(true, secs - 1.0));
        assert!(!lungs.tick(false, 1.0));
        assert_eq!(lungs.under, 0.0);
        assert!(!lungs.tick(true, secs - 1.0));
        assert!(lungs.tick(true, 1.0));
        assert_eq!(lungs.under, 0.0);
    }

    #[test]
    fn only_the_deep_lake_is_deep() {
        load();
        let at = crate::terrain::lake_at();
        let floor = crate::terrain::height_at(at.x, at.y);
        assert!(out_of_depth(Vec3::new(at.x, floor, at.y)), "lake bed");
        let level = crate::terrain::lake_level();
        assert!(
            !out_of_depth(Vec3::new(at.x, level, at.y)),
            "at the surface"
        );
        assert!(
            !out_of_depth(Vec3::new(-40.0, level - 5.0, 0.0)),
            "not the lake"
        );
    }
}
