//! Where every mechanic lives (#412, bben: "individual mechanics spread out
//! through the world"). A **site** is a named place with a reach; it is awake
//! only while somebody is there, and what lives at it (creatures, sounds, HUD
//! lines, key hints) follows that. The list is data: `data/base/sites.ron`.
//!
//! ```ignore
//! app.add_systems(Update, prowl.run_if(sites::awake("nyc")));       // anyone there
//! app.add_systems(Update, flap.run_if(sites::here("nyc")));         // you're there
//! app.add_systems(Update, spawn.run_if(sites::arrived("nyc")));     // you just got there
//! app.add_systems(Update, sites::put_away::<Pigeon>.run_if(sites::left("nyc")));
//! let home = sites::at("wreck");                                    // its middle (x, z)
//! ```
//!
//! "Anyone" is every player's feet as this peer knows them ([`People`]), so
//! the server (which runs the simulation) and the clients agree; "you" is
//! [`crate::place::Here`] (nobody, on a peer without one). Both are written
//! by `fps.rs`. Nothing
//! is replicated. An id the file doesn't have counts as awake, so a broken
//! file leaves the game running as before (`tests/it/sites.rs` checks ids).

use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::AppState;
use crate::place::Here;

/// Every site: `assets/data/base/sites.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Sites {
    /// A woken site stays awake until everyone is this far (m) outside its reach.
    pub linger: f32,
    pub sites: Vec<Site>,
}

/// One place a mechanic lives.
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
pub struct Site {
    /// What code calls it.
    pub id: String,
    /// What players call it (the journal).
    pub name: String,
    /// The part of the map it's in.
    pub district: String,
    /// Its middle.
    pub place: Place,
    /// How far from its middle it wakes.
    pub reach: Reach,
}

/// Where a site's middle is.
#[derive(serde::Deserialize, Debug, Clone, PartialEq)]
pub enum Place {
    /// A `Spot` marker on the commune map, and how far from it (x, z).
    Spot(String, (f32, f32)),
    /// A point (x, z), for places the commune map doesn't hold (New York).
    At(f32, f32),
}

/// How far a site reaches from its middle (m, on the ground: a site is as
/// tall as the world, so the floors over and under it are part of it).
#[derive(serde::Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum Reach {
    /// A circle's radius.
    Round(f32),
    /// A box's half sizes (x, z).
    Box(f32, f32),
}

impl Reach {
    /// How far outside the reach round `middle` is `p`? 0 inside.
    pub fn gap(self, middle: Vec2, p: Vec2) -> f32 {
        let d = p - middle;
        match self {
            Reach::Round(r) => (d.length() - r).max(0.0),
            Reach::Box(x, z) => (d.abs() - Vec2::new(x, z)).max(Vec2::ZERO).length(),
        }
    }
}

impl Site {
    /// Its middle (x, z). A spot the map doesn't have is the map's middle.
    pub fn middle(&self) -> Vec2 {
        match &self.place {
            Place::Spot(name, (x, z)) => {
                friendslop_world::places::spot(name).unwrap_or_default() + Vec2::new(*x, *z)
            }
            Place::At(x, z) => Vec2::new(*x, *z),
        }
    }

    /// Is it awake with people at `people`? A site that `was` awake lingers.
    pub fn wakes(&self, linger: f32, was: bool, people: impl IntoIterator<Item = Vec2>) -> bool {
        let slack = if was { linger } else { 0.0 };
        let middle = self.middle();
        people
            .into_iter()
            .any(|p| self.reach.gap(middle, p) <= slack)
    }
}

pub static SITES: friendslop_data::Data<Sites> = friendslop_data::Data::ron("data/base/sites.ron");

/// The current list of sites.
pub fn sites() -> std::sync::Arc<Sites> {
    SITES.get()
}

/// The middle (x, z) of the site called `id`; the map's middle without one.
pub fn at(id: &str) -> Vec2 {
    sites()
        .sites
        .iter()
        .find(|s| s.id == id)
        .map(Site::middle)
        .unwrap_or_default()
}

/// Where every player's feet are (x, z), written each frame (`fps.rs`).
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct People(pub Vec<Vec2>);

/// One site's state this frame and last.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct Woke {
    anyone: bool,
    me: bool,
    me_before: bool,
}

/// Which sites are awake (see the module docs).
#[derive(Resource, Default, Debug)]
pub struct Awake(HashMap<String, Woke>);

impl Awake {
    /// Is anyone at `id`? (An unknown site counts as awake.)
    pub fn anyone(&self, id: &str) -> bool {
        self.0.get(id).is_none_or(|w| w.anyone)
    }

    /// Are you at `id`? (An unknown site counts as awake.)
    pub fn me(&self, id: &str) -> bool {
        self.0.get(id).is_none_or(|w| w.me)
    }

    /// Did you get to `id` this frame?
    pub fn arrived(&self, id: &str) -> bool {
        self.0.get(id).is_some_and(|w| w.me && !w.me_before)
    }

    /// Did you leave `id` this frame?
    pub fn left(&self, id: &str) -> bool {
        self.0.get(id).is_some_and(|w| !w.me && w.me_before)
    }

    /// The ids of the sites with anyone at them, in the file's order.
    pub fn ids(&self) -> Vec<String> {
        sites()
            .sites
            .iter()
            .filter(|s| self.0.get(&s.id).is_some_and(|w| w.anyone))
            .map(|s| s.id.clone())
            .collect()
    }

    /// Work out this frame from where `you` (if you're anywhere) and all the
    /// `players` stand.
    fn track(&mut self, list: &Sites, you: Option<Vec2>, players: &[Vec2]) {
        self.0
            .retain(|id, _| list.sites.iter().any(|s| s.id == *id));
        for site in &list.sites {
            let was = self.0.get(&site.id).copied().unwrap_or_default();
            let me = site.wakes(list.linger, was.me, you);
            // You count as a player even before your body is there.
            let anyone = me || site.wakes(list.linger, was.anyone, players.iter().copied());
            self.0.insert(
                site.id.clone(),
                Woke {
                    anyone,
                    me,
                    me_before: was.me,
                },
            );
        }
    }
}

/// Run while anyone is at the site `id` (the simulation).
pub fn awake(id: &'static str) -> impl FnMut(Res<Awake>) -> bool + Clone {
    move |a: Res<Awake>| a.anyone(id)
}

/// Run while you are at the site `id` (what only you see and hear).
pub fn here(id: &'static str) -> impl FnMut(Res<Awake>) -> bool + Clone {
    move |a: Res<Awake>| a.me(id)
}

/// Run on the frame you get to the site `id` (bring out its dressing).
pub fn arrived(id: &'static str) -> impl FnMut(Res<Awake>) -> bool + Clone {
    move |a: Res<Awake>| a.arrived(id)
}

/// Run on the frame you leave the site `id` (put its dressing away).
pub fn left(id: &'static str) -> impl FnMut(Res<Awake>) -> bool + Clone {
    move |a: Res<Awake>| a.left(id)
}

/// Despawn everything with `C`: a sleeping site's local dressing.
pub fn put_away<C: Component>(mut commands: Commands, things: Query<Entity, With<C>>) {
    for e in &things {
        commands.entity(e).despawn();
    }
}

/// Added with the protocol (`ProtocolPlugin`), not the game's own plugins:
/// the server's simulation asks which sites are awake, headless too.
pub fn plugin(app: &mut App) {
    app.init_resource::<Awake>()
        .init_resource::<People>()
        // A new game starts with every site asleep, so `arrived` fires for
        // the ones you start in.
        .add_systems(OnEnter(AppState::InGame), |mut a: ResMut<Awake>| {
            a.0.clear()
        })
        .add_systems(PreUpdate, track);
}

fn track(here: Option<Res<Here>>, people: Res<People>, mut awake: ResMut<Awake>) {
    awake.track(&sites(), here.map(|h| h.0.xz()), &people.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(reach: Reach) -> Site {
        Site {
            id: "grove".into(),
            name: "The Still Grove".into(),
            district: "forest".into(),
            place: Place::At(10.0, 0.0),
            reach,
        }
    }

    fn list(reach: Reach) -> Sites {
        Sites {
            linger: 5.0,
            sites: vec![site(reach)],
        }
    }

    #[test]
    fn a_reach_measures_how_far_outside_you_are() {
        let middle = Vec2::new(10.0, 0.0);
        assert_eq!(Reach::Round(4.0).gap(middle, Vec2::new(12.0, 0.0)), 0.0);
        assert_eq!(Reach::Round(4.0).gap(middle, Vec2::new(20.0, 0.0)), 6.0);
        assert_eq!(Reach::Box(4.0, 2.0).gap(middle, Vec2::new(13.0, -2.0)), 0.0);
        assert_eq!(Reach::Box(4.0, 2.0).gap(middle, Vec2::new(17.0, 6.0)), 5.0);
    }

    #[test]
    fn a_site_wakes_when_someone_is_there_and_lingers() {
        let s = site(Reach::Round(4.0));
        assert!(!s.wakes(5.0, false, []));
        assert!(s.wakes(5.0, false, [Vec2::new(13.0, 0.0)]));
        // 7 m from the middle: outside the reach, inside the linger.
        let edge = [Vec2::new(17.0, 0.0)];
        assert!(!s.wakes(5.0, false, edge));
        assert!(s.wakes(5.0, true, edge));
        assert!(!s.wakes(5.0, true, [Vec2::new(30.0, 0.0)]));
    }

    #[test]
    fn you_arrive_and_leave_once() {
        let list = list(Reach::Round(4.0));
        let (there, away) = (Vec2::new(10.0, 1.0), Vec2::new(100.0, 0.0));
        let mut a = Awake::default();
        a.track(&list, Some(there), &[]);
        assert!(a.me("grove") && a.anyone("grove") && a.arrived("grove"));
        a.track(&list, Some(there), &[]);
        assert!(a.me("grove") && !a.arrived("grove") && !a.left("grove"));
        a.track(&list, Some(away), &[]);
        assert!(!a.me("grove") && a.left("grove"));
        a.track(&list, Some(away), &[]);
        assert!(!a.left("grove"));
    }

    #[test]
    fn a_friend_keeps_a_site_awake_for_the_simulation_only() {
        let list = list(Reach::Round(4.0));
        let mut a = Awake::default();
        a.track(&list, None, &[Vec2::new(10.0, 0.0)]);
        assert!(a.anyone("grove") && !a.me("grove"));
    }

    #[test]
    fn an_unknown_site_counts_as_awake_but_never_arrives() {
        let a = Awake::default();
        assert!(a.anyone("nowhere") && a.me("nowhere"));
        assert!(!a.arrived("nowhere") && !a.left("nowhere"));
    }
}
