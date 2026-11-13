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
//! Whatever the aim point is on gets a hint under it: the thing's verb
//! ([`Clickable::labelled`], or just "click"), and "too far" when it's out of
//! reach, so you can tell what's clickable before you click.
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

/// Tuning numbers: `assets/data/play/interact.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Default pick radius on screen, in pixels.
    pub click_radius: f32,
    /// How far (metres) your aim line may pass beside a thing and still use it.
    pub aim_slack: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/play/interact.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}
/// How far from your feet you can reach things (client-side check).
pub const REACH: f32 = 3.5;
/// The server allows a little extra for lag.
pub const SERVER_REACH: f32 = REACH + 1.0;

/// Something you can click. The pick point is `height` above the entity origin.
#[derive(Component, Clone, Copy, Debug)]
pub struct Clickable {
    pub height: f32,
    pub radius_px: f32,
    /// What clicking does, shown in the hover hint ("pet the cat").
    pub label: Option<&'static str>,
}

impl Clickable {
    pub fn at_height(height: f32) -> Self {
        Self {
            height,
            radius_px: tuning().click_radius,
            label: None,
        }
    }

    /// The verb shown in the hover hint.
    pub fn labelled(mut self, label: &'static str) -> Self {
        self.label = Some(label);
        self
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

/// One clickable as seen from the camera, for [`pick_forgiving`].
#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub entity: Entity,
    /// Its pick point on screen and the pick radius there, in pixels.
    pub screen: Vec2,
    pub radius_px: f32,
    /// How close the aim line passes to it (anywhere from its feet to its pick point), metres.
    pub gap: f32,
    pub in_reach: bool,
}

/// The forgiving pick: a thing counts as aimed at if the cursor is within its screen
/// radius *or* the aim line passes within `slack` metres of its body. Things in reach win
/// over things out of reach, then the best-aimed one wins.
pub fn pick_forgiving(
    cursor: Vec2,
    slack: f32,
    candidates: impl IntoIterator<Item = Candidate>,
) -> Option<Entity> {
    let slack = slack.max(0.01);
    candidates
        .into_iter()
        .filter_map(|c| {
            let score = (c.screen.distance(cursor) / c.radius_px.max(1.0)).min(c.gap / slack);
            (score < 1.0).then_some((c.entity, !c.in_reach, score))
        })
        .min_by(|a, b| a.1.cmp(&b.1).then(a.2.total_cmp(&b.2)))
        .map(|(e, _, _)| e)
}

/// Shortest distance from the ray (`origin`, unit `dir`, forwards only) to the upright
/// segment from `foot` up `height` metres.
pub fn ray_gap(origin: Vec3, dir: Vec3, foot: Vec3, height: f32) -> f32 {
    const STEPS: usize = 12;
    (0..=STEPS)
        .map(|i| {
            let p = foot + Vec3::Y * (height * i as f32 / STEPS as f32);
            let t = (p - origin).dot(dir).max(0.0);
            p.distance(origin + dir * t)
        })
        .fold(f32::INFINITY, f32::min)
}

/// Every visible clickable seen from the camera through `cursor`.
fn candidates(
    camera: &Camera,
    cam_tf: &GlobalTransform,
    cursor: Vec2,
    here: Option<Vec3>,
    clickables: &Query<(Entity, &Clickable, &GlobalTransform, &InheritedVisibility)>,
) -> Vec<Candidate> {
    let Ok(ray) = camera.viewport_to_world(cam_tf, cursor) else {
        return Vec::new();
    };
    clickables
        .iter()
        .filter(|(_, _, _, vis)| vis.get())
        .filter_map(|(e, c, tf, _)| {
            let foot = tf.translation();
            let screen = camera
                .world_to_viewport(cam_tf, foot + Vec3::Y * c.height)
                .ok()?;
            Some(Candidate {
                entity: e,
                screen,
                radius_px: c.radius_px,
                gap: ray_gap(ray.origin, *ray.direction, foot, c.height + 0.25),
                in_reach: here.is_some_and(|h| within(h, foot, REACH)),
            })
        })
        .collect()
}

pub fn plugin(app: &mut App) {
    app.add_observer(show_denied)
        .init_resource::<AimHover>()
        .add_systems(OnExit(AppState::InGame), |mut h: ResMut<AimHover>| {
            h.0 = None
        })
        .add_systems(
            Update,
            (click, hover_hint).run_if(in_state(AppState::InGame)),
        );
}

/// The context prompt's verb for a clickable's label: "pet the cat" ->
/// "Pet the cat"; unlabelled things are just "Use".
pub fn verb(label: Option<&str>) -> String {
    let Some(label) = label.map(str::trim).filter(|l| !l.is_empty()) else {
        return "Use".to_string();
    };
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => "Use".to_string(),
    }
}

/// What the aim point (crosshair, or cursor in the overview) is on this
/// frame, for the HUD's context prompt and crosshair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    /// Where the aim point is, in window logical pixels.
    pub at: Vec2,
    pub label: Option<&'static str>,
    pub in_reach: bool,
}

/// The current [`Aim`], or `None` when it's on nothing clickable.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct AimHover(pub Option<Aim>);

/// Work out what the aim point is on (and whether it's in reach), every frame.
#[allow(clippy::too_many_arguments)]
fn hover_hint(
    me: Option<Res<LocalPlayer>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<MainCamera>>,
    clickables: Query<(Entity, &Clickable, &GlobalTransform, &InheritedVisibility)>,
    walkers: Query<(&Player, &Walker)>,
    duelists: Query<&Duelist>,
    view: Res<FpsView>,
    mut hover: ResMut<AimHover>,
) {
    let target = (|| {
        let me = me.as_ref()?;
        if duelists.iter().any(|d| d.player == me.0) || (view.first_person && view.released) {
            return None;
        }
        let window = window.single().ok()?;
        let cursor = aim_point(window, &view)?;
        let (camera, cam_tf) = camera.single().ok()?;
        let here = walkers.iter().find(|(p, _)| p.id == me.0)?.1.pos;
        let seen = candidates(camera, cam_tf, cursor, Some(here), &clickables);
        let target = pick_forgiving(cursor, tuning().aim_slack, seen)?;
        let (_, c, tf, _) = clickables.get(target).ok()?;
        Some(Aim {
            at: cursor,
            label: c.label,
            in_reach: within(here, tf.translation(), REACH),
        })
    })();
    if hover.0 != target {
        hover.0 = target;
    }
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
    overlay: Option<Res<crate::ui::Overlay>>,
) {
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    // Clicks on the open journal are for its tabs, not the world.
    if overlay.is_some_and(|o| o.journal) {
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
    let Some(here) = walkers
        .iter()
        .find(|(p, _)| p.id == me.0)
        .map(|(_, w)| w.pos)
    else {
        return;
    };
    let seen = candidates(camera, cam_tf, cursor, Some(here), &clickables);
    let Some(target) = pick_forgiving(cursor, tuning().aim_slack, seen) else {
        return;
    };
    let Ok((_, _, target_tf, _)) = clickables.get(target) else {
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
    fn forgiving_pick_takes_near_misses_and_prefers_reach() {
        let mut world = World::new();
        let (near, far) = (world.spawn_empty().id(), world.spawn_empty().id());
        let c = |entity, screen, gap, in_reach| Candidate {
            entity,
            screen,
            radius_px: 40.0,
            gap,
            in_reach,
        };
        // Off by 100 px on screen but the aim line grazes its body: still a hit.
        let got = pick_forgiving(Vec2::ZERO, 0.5, [c(near, Vec2::new(100.0, 0.0), 0.3, true)]);
        assert_eq!(got, Some(near));
        // A better-aimed thing out of reach loses to one you can reach.
        let got = pick_forgiving(
            Vec2::ZERO,
            0.5,
            [
                c(far, Vec2::new(1.0, 0.0), 0.0, false),
                c(near, Vec2::new(30.0, 0.0), 0.4, true),
            ],
        );
        assert_eq!(got, Some(near));
        // Clear misses on both counts pick nothing.
        let got = pick_forgiving(Vec2::ZERO, 0.5, [c(near, Vec2::new(100.0, 0.0), 2.0, true)]);
        assert_eq!(got, None);
    }

    #[test]
    fn ray_gap_measures_to_the_whole_body() {
        // Looking along +x at head height passes 1 m beside a 2 m post at x=5, z=1.
        let g = ray_gap(
            Vec3::new(0.0, 1.6, 0.0),
            Vec3::X,
            Vec3::new(5.0, 0.0, 1.0),
            2.0,
        );
        assert!((g - 1.0).abs() < 0.05);
        // Things behind you are measured from your eye, not hit.
        let g = ray_gap(Vec3::ZERO, Vec3::X, Vec3::new(-5.0, 0.0, 0.0), 1.0);
        assert!(g > 4.0);
    }

    #[test]
    fn prompts_name_the_verb() {
        assert_eq!(verb(Some("pet the cat")), "Pet the cat");
        assert_eq!(verb(Some("  ")), "Use");
        assert_eq!(verb(None), "Use");
    }

    #[test]
    fn reach_ignores_height() {
        assert!(within(Vec3::ZERO, Vec3::new(3.0, 5.0, 0.0), REACH));
        assert!(!within(Vec3::ZERO, Vec3::new(4.0, 0.0, 0.0), REACH));
    }
}
