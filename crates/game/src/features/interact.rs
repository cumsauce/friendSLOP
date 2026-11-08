//! One click, one thing (polish pass).
//!
//! Every clickable thing carries a `Clickable`. On a left click this module
//! finds the single closest `Clickable` under the aim point (the crosshair in
//! first person, the cursor in the overview camera), checks that it's within
//! `REACH` of your body (`fps::Walker`), and triggers `Clicked` on that one
//! entity. Features observe `Clicked` and send their own request; the server
//! re-checks the distance and answers with `Denied` when it says no, shown in
//! the HUD toast.
//!
//! In a duel the duel owns the mouse (shooting), so picking is off there. In
//! first person, a click while the cursor is free (after Esc) only grabs it
//! again.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::duel::Duelist;
use crate::features::fps::{FpsView, Walker, aim_point};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};
use crate::world::MainCamera;

/// Default pick radius on screen, in pixels.
pub const CLICK_RADIUS: f32 = 40.0;
/// How far from your feet you can reach things (client-side check).
pub const REACH: f32 = 3.5;
/// The server allows a little extra for lag.
pub const SERVER_REACH: f32 = REACH + 1.0;

/// Something you can click. The pick point is `height` above the entity origin.
#[derive(Component, Clone, Copy, Debug)]
pub struct Clickable {
    pub height: f32,
    pub radius_px: f32,
}

impl Clickable {
    pub fn at_height(height: f32) -> Self {
        Self {
            height,
            radius_px: CLICK_RADIUS,
        }
    }

    pub fn radius(mut self, px: f32) -> Self {
        self.radius_px = px;
        self
    }
}

/// Local: the local player clicked this entity (in reach). Exactly one per click.
#[derive(EntityEvent, Clone, Copy, Debug)]
pub struct Clicked {
    pub entity: Entity,
}

/// Server -> one client: your request was refused, and why.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Denied {
    pub text: String,
}

pub fn protocol(app: &mut App) {
    app.add_server_event::<Denied>(Channel::Ordered);
}

/// Is `target` within `reach` of feet at `from` (on the floor plane)?
pub fn within(from: Vec3, target: Vec3, reach: f32) -> bool {
    from.xz().distance(target.xz()) <= reach
}

/// Server: the body position of whoever sent a client event.
pub fn body_of(client: ClientId, walkers: &Query<(&PlayerOwner, &Walker)>) -> Option<Vec3> {
    let owner = match client {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    walkers
        .iter()
        .find(|(o, _)| o.0 == owner)
        .map(|(_, w)| w.pos)
}

/// Server: tell `client` no.
pub fn deny(commands: &mut Commands, client: ClientId, text: impl Into<String>) {
    commands.server_trigger(ToClients {
        targets: SendTargets::Single(client),
        message: Denied { text: text.into() },
    });
}

/// Server: is `client`'s body close enough to `target`? Denies if not.
pub fn in_reach(
    commands: &mut Commands,
    client: ClientId,
    walkers: &Query<(&PlayerOwner, &Walker)>,
    target: Vec3,
) -> bool {
    let ok = body_of(client, walkers).is_some_and(|me| within(me, target, SERVER_REACH));
    if !ok {
        deny(commands, client, "too far away: walk closer");
    }
    ok
}

/// Of `candidates` (entity, screen point, radius), the one closest to `cursor`
/// that the cursor is actually on. At most one.
pub fn pick(
    cursor: Vec2,
    candidates: impl IntoIterator<Item = (Entity, Vec2, f32)>,
) -> Option<Entity> {
    candidates
        .into_iter()
        .map(|(e, p, r)| (e, p.distance(cursor), r))
        .filter(|(_, d, r)| d < r)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(e, _, _)| e)
}

pub fn plugin(app: &mut App) {
    app.add_observer(show_denied)
        .add_systems(Update, click.run_if(in_state(AppState::InGame)));
}

fn show_denied(ev: On<Denied>, toast: Option<ResMut<Toast>>) {
    if let Some(mut toast) = toast {
        toast.show(ev.text.clone());
    }
}

/// Client-side helper for feature observers: show a refusal.
pub fn refuse(toast: Option<ResMut<Toast>>, text: &str) {
    if let Some(mut toast) = toast {
        toast.show(text);
    }
}

#[allow(clippy::too_many_arguments)]
fn click(
    mut commands: Commands,
    mouse: Res<ButtonInput<MouseButton>>,
    me: Option<Res<LocalPlayer>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    clickables: Query<(Entity, &Clickable, &GlobalTransform, &InheritedVisibility)>,
    walkers: Query<(&Player, &Walker)>,
    duelists: Query<&Duelist>,
    still: Query<&super::stillness::Stillness>,
    mut view: ResMut<FpsView>,
    mut toast: ResMut<Toast>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Some(me) = me else { return };
    if duelists.iter().any(|d| d.player == me.0) {
        return; // the duel owns the mouse
    }
    if view.first_person && view.released {
        view.released = false; // this click just grabs the cursor again
        return;
    }
    let Ok(window) = window.single() else { return };
    let Some(cursor) = aim_point(window, &view) else {
        return;
    };
    let Ok((camera, cam_tf)) = camera.single() else {
        return;
    };
    let candidates = clickables.iter().filter_map(|(e, c, tf, vis)| {
        if !vis.get() {
            return None;
        }
        let at = tf.translation() + Vec3::Y * c.height;
        let p = camera.world_to_viewport(cam_tf, at).ok()?;
        Some((e, p, c.radius_px))
    });
    let Some(target) = pick(cursor, candidates) else {
        return;
    };
    let Ok((_, _, target_tf, _)) = clickables.get(target) else {
        return;
    };
    let Some(here) = walkers
        .iter()
        .find(|(p, _)| p.id == me.0)
        .map(|(_, w)| w.pos)
    else {
        return;
    };
    if !within(here, target_tf.translation(), REACH) {
        toast.show("too far away: walk closer");
        return;
    }
    if super::stillness::is_frozen(&still) {
        toast.show("the world stands still: nothing happens");
        return;
    }
    commands.trigger(Clicked { entity: target });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_targets_pick_only_the_closest() {
        let mut world = World::new();
        let (a, b) = (world.spawn_empty().id(), world.spawn_empty().id());
        let got = pick(
            Vec2::new(100.0, 100.0),
            [
                (a, Vec2::new(120.0, 100.0), 40.0),
                (b, Vec2::new(105.0, 100.0), 40.0),
            ],
        );
        assert_eq!(got, Some(b));
        assert_eq!(pick(Vec2::ZERO, [(a, Vec2::new(100.0, 0.0), 40.0)]), None);
    }

    #[test]
    fn reach_ignores_height() {
        assert!(within(Vec3::ZERO, Vec3::new(3.0, 5.0, 0.0), REACH));
        assert!(!within(Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0), REACH));
    }
}
