//! The wastes past the map's north edge (#391): the ground itself is the
//! world crate's (`friendslop_world::wastes`); this only tells it who is
//! near, the same way areas ask (`friendslop_base::areas`): colliders for
//! anyone near on the host (only you on a client), meshes for you.

use bevy::prelude::*;
use bevy_replicon::prelude::ClientState;
use friendslop_world::wastes::{Viewers, WastesPlugin};

use crate::AppState;
use crate::place::Here;
use crate::sites::People;

pub fn plugin(app: &mut App) {
    // `FRIENDSLOP_START=wastes`: 300 m out past the north edge.
    crate::features::start::provide(|| {
        let (x, z) = (0.0, -410.0);
        let y = friendslop_world::wastes::height(friendslop_world::wastes::tuning().seed, x, z);
        vec![("wastes".into(), Vec3::new(x, y + 1.0, z))]
    });
    app.add_plugins(WastesPlugin).add_systems(
        Update,
        watch
            .before(friendslop_world::wastes::StreamSet)
            .run_if(in_state(AppState::InGame)),
    );
}

/// Who the chunks stream around, as (sim, show) points. A client only
/// simulates where it is, like an area's Sim layer.
pub fn viewers(you: Option<Vec3>, people: &[Vec3], client: bool) -> (Vec<Vec2>, Vec<Vec2>) {
    let show: Vec<Vec2> = you.map(|p| p.xz()).into_iter().collect();
    let mut sim = show.clone();
    if !client {
        sim.extend(people.iter().map(|p| p.xz()));
    }
    (sim, show)
}

/// Fill the wastes' [`Viewers`] (pub for the headless tests).
pub fn watch(
    here: Option<Res<Here>>,
    people: Res<People>,
    client: Option<Res<State<ClientState>>>,
    mut out: ResMut<Viewers>,
) {
    let client = client.is_some_and(|c| *c.get() == ClientState::Connected);
    let (sim, show) = viewers(here.map(|h| h.0), &people.0, client);
    out.sim = sim;
    out.show = show;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_host_simulates_for_a_friend_but_only_shows_you() {
        let you = Vec3::new(0.0, 0.0, -300.0);
        let friend = Vec3::new(50.0, 0.0, -900.0);
        let (sim, show) = viewers(Some(you), &[you, friend], false);
        assert!(sim.contains(&friend.xz()));
        assert_eq!(show, vec![you.xz()]);
        let (sim, _) = viewers(Some(you), &[you, friend], true);
        assert_eq!(sim, vec![you.xz()], "a client simulates only where it is");
    }
}
