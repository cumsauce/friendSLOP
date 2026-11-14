//! Where you are, and how the place you're in looks.
//!
//! Places never write the sun, the ambient light or the fog themselves. Each
//! frame a place you're in pushes its id into [`PlaceLooks`] (in
//! [`PlaceSet`]), and the day/night cycle (`atmosphere.rs`, the only writer
//! of those globals) works out the outdoor values, lays the winning place's
//! look over them and writes the result. Nothing is saved and put back, so
//! overlapping places, or leaving them in any order, always come back to the
//! right sky.
//!
//! What each place looks like is data: `data/base/place_looks.ron`, keyed by
//! the ids places push. Code only answers "am I inside?", from [`Here`].
//!
//! ```ignore
//! pub const LOOK: &str = "warehouse";
//! fn look(here: Res<Here>, mut looks: ResMut<PlaceLooks>) {
//!     if inside(here.0) { looks.push(LOOK); }
//! }
//! app.add_systems(Update, look.in_set(PlaceSet));
//! ```
//!
//! Local only: every peer works out its own.

use std::collections::BTreeMap;

use bevy::prelude::*;

pub fn plugin(app: &mut App) {
    app.init_resource::<Here>()
        .init_resource::<PlaceLooks>()
        .add_systems(First, |mut looks: ResMut<PlaceLooks>| looks.0.clear())
        .add_systems(PostUpdate, apply_hidden);
}

/// Hidden by places (each by its own name) instead of each place setting
/// `Visibility` and putting back what it saw: an entity stays hidden while
/// any place still hides it, and gets its own visibility back when the last
/// one lets go. Add it, or call [`HiddenBy::add`] / [`HiddenBy::remove`].
#[derive(Component, Debug, Default, Clone, PartialEq)]
pub struct HiddenBy {
    by: Vec<&'static str>,
    was: Option<Visibility>,
}

impl HiddenBy {
    pub fn new(source: &'static str) -> Self {
        Self {
            by: vec![source],
            was: None,
        }
    }

    pub fn add(&mut self, source: &'static str) {
        if !self.by.contains(&source) {
            self.by.push(source);
        }
    }

    pub fn remove(&mut self, source: &'static str) {
        self.by.retain(|s| *s != source);
    }

    pub fn has(&self, source: &str) -> bool {
        self.by.contains(&source)
    }
}

/// Hides what places hide and gives it back its own visibility when no place does.
fn apply_hidden(
    mut commands: Commands,
    mut hidden: Query<(Entity, &mut HiddenBy, &mut Visibility), Changed<HiddenBy>>,
) {
    for (e, mut h, mut v) in &mut hidden {
        if h.by.is_empty() {
            if let Some(was) = h.was {
                v.set_if_neq(was);
            }
            commands.entity(e).remove::<HiddenBy>();
        } else {
            if h.was.is_none() {
                h.was = Some(*v);
            }
            v.set_if_neq(Visibility::Hidden);
        }
    }
}

/// Where "you" are for every place check: the local player's feet, or the
/// camera when there's no player (the menu, screenshots). Written early each
/// frame (`fps.rs`), so every place agrees.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq)]
pub struct Here(pub Vec3);

/// The ids of the places you're in this frame (cleared every frame).
#[derive(Resource, Default, Debug)]
pub struct PlaceLooks(Vec<&'static str>);

impl PlaceLooks {
    pub fn push(&mut self, id: &'static str) {
        if !self.0.contains(&id) {
            self.0.push(id);
        }
    }

    pub fn ids(&self) -> &[&'static str] {
        &self.0
    }
}

/// Places push their look in this set; the world's lighting is written after.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PlaceSet;

/// `data/base/place_looks.ron`: every place's look, by id.
#[derive(serde::Deserialize, Default, Debug)]
pub struct Looks {
    pub looks: BTreeMap<String, Look>,
}

/// How a place changes the world's light. A missing field leaves it to the outdoors.
#[derive(serde::Deserialize, Default, Debug, Clone, PartialEq)]
pub struct Look {
    /// When you're in several places, the highest wins.
    pub priority: i32,
    /// The sun's brightness (lux): 0 puts the sky's light out.
    #[serde(default)]
    pub sun: Option<f32>,
    /// The ambient light's brightness.
    #[serde(default)]
    pub ambient: Option<f32>,
    /// The haze: its colour (sRGB) and where it starts and ends (m).
    #[serde(default)]
    pub fog: Option<Fog>,
}

#[derive(serde::Deserialize, Default, Debug, Clone, Copy, PartialEq)]
pub struct Fog {
    pub color: (f32, f32, f32),
    pub start: f32,
    pub end: f32,
}

pub static LOOKS: friendslop_data::Data<Looks> =
    friendslop_data::Data::ron("data/base/place_looks.ron");

/// The world's light, outdoors or in a place.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    pub sun: f32,
    pub ambient: f32,
    pub fog_color: Color,
    pub fog_start: f32,
    pub fog_end: f32,
    /// True when a place's haze replaced the sky's (no sun glow in it).
    pub indoor_fog: bool,
}

/// The winning look among the places you're in: highest priority, ties to
/// the id that sorts first, so the order places push in never matters.
pub fn pick<'a>(ids: &[&str], looks: &'a Looks) -> Option<(&'a str, &'a Look)> {
    let mut best: Option<(&str, &Look)> = None;
    for id in ids {
        let Some((key, look)) = looks.looks.get_key_value(*id) else {
            continue;
        };
        let better = match best {
            None => true,
            Some((bk, bl)) => {
                look.priority > bl.priority || (look.priority == bl.priority && key.as_str() < bk)
            }
        };
        if better {
            best = Some((key.as_str(), look));
        }
    }
    best
}

/// The outdoor light with a place's look laid over it.
pub fn blend(outdoor: Light, look: Option<&Look>) -> Light {
    let Some(look) = look else {
        return outdoor;
    };
    let mut l = outdoor;
    if let Some(sun) = look.sun {
        l.sun = sun;
    }
    if let Some(ambient) = look.ambient {
        l.ambient = ambient;
    }
    if let Some(f) = look.fog {
        l.fog_color = Color::srgb(f.color.0, f.color.1, f.color.2);
        l.fog_start = f.start;
        l.fog_end = f.end;
        l.indoor_fog = true;
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    fn looks() -> Looks {
        let mut looks = BTreeMap::new();
        let dark = |priority, ambient| Look {
            priority,
            sun: Some(0.0),
            ambient: Some(ambient),
            fog: Some(Fog {
                color: (0.0, 0.0, 0.0),
                start: 2.0,
                end: 20.0,
            }),
        };
        looks.insert("hall".to_owned(), dark(1, 30.0));
        looks.insert("station".to_owned(), dark(2, 45.0));
        looks.insert("tie".to_owned(), dark(2, 99.0));
        Looks { looks }
    }

    fn outdoor() -> Light {
        Light {
            sun: 15_000.0,
            ambient: 1_100.0,
            fog_color: Color::WHITE,
            fog_start: 60.0,
            fog_end: 230.0,
            indoor_fog: false,
        }
    }

    #[test]
    fn the_highest_place_wins_whatever_the_order() {
        let l = looks();
        assert_eq!(pick(&["hall", "station"], &l).map(|p| p.0), Some("station"));
        assert_eq!(pick(&["station", "hall"], &l).map(|p| p.0), Some("station"));
        assert_eq!(pick(&["tie", "station"], &l).map(|p| p.0), Some("station"));
        assert_eq!(pick(&["nowhere"], &l), None);
        assert_eq!(pick(&[], &l), None);
    }

    #[test]
    fn leaving_every_place_is_the_outdoors_again() {
        let l = looks();
        let inside = blend(outdoor(), pick(&["hall", "station"], &l).map(|p| p.1));
        assert_eq!(inside.sun, 0.0);
        assert_eq!(inside.ambient, 45.0);
        assert!(inside.indoor_fog);
        assert_eq!(blend(outdoor(), None), outdoor());
    }

    #[test]
    fn hidden_until_the_last_place_lets_go() {
        let mut app = App::new();
        app.add_systems(Update, apply_hidden);
        let e = app.world_mut().spawn(Visibility::Inherited).id();
        app.world_mut()
            .entity_mut(e)
            .insert(HiddenBy::new("station"));
        app.update();
        let vis = |app: &App| *app.world().get::<Visibility>(e).expect("vis");
        assert_eq!(vis(&app), Visibility::Hidden);
        // A second place hides it too; the first letting go keeps it hidden.
        let mut h = app.world_mut().get_mut::<HiddenBy>(e).expect("hidden");
        h.add("city");
        h.remove("station");
        app.update();
        assert_eq!(vis(&app), Visibility::Hidden);
        app.world_mut()
            .get_mut::<HiddenBy>(e)
            .expect("hidden")
            .remove("city");
        app.update();
        assert_eq!(vis(&app), Visibility::Inherited);
        assert!(app.world().get::<HiddenBy>(e).is_none());
    }

    #[test]
    fn missing_fields_keep_the_outdoors() {
        let look = Look {
            priority: 0,
            ambient: Some(10.0),
            ..default()
        };
        let l = blend(outdoor(), Some(&look));
        assert_eq!(l.sun, 15_000.0);
        assert_eq!(l.ambient, 10.0);
        assert_eq!(l.fog_end, 230.0);
    }
}
