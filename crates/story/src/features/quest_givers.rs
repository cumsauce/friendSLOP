//! The people who give quests: where they stand, the marker over their head
//! ("!" something to give, "?" one of theirs is on your list), the hours
//! some of them keep, and the talk you have with them (the game's
//! `ui::dialogue` draws it).
//!
//! Givers are local: every peer reads where they are from
//! `data/story/quest_givers.ron` and draws them while their area shows, and
//! the server checks you're beside one (the same data) when you say yes. A
//! giver with `hours` is only out then (the shared clock, `crate::clock`);
//! one without a `body` is a character who's already there (Abuela, Sewer
//! Paul, Jegg): they only get the marker, and clicking them opens the talk
//! too.
//!
//! The markers are modeler models (`models/features/quest_offer.gltf`, "!",
//! and `quest_busy.gltf`, "?"; sources `.modeler/prefabs/quest-offer.json`
//! and `quest-busy.json`) worn in a glowing colour. Without the files you
//! just don't see the markers.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_replicon::prelude::*;

use crate::AppState;
use crate::features::fps::HandsBusy;
use crate::features::interact::{Clickable, Clicked};
use crate::features::quests::TalkChoice;
use crate::player::{LocalPlayer, Player};
use crate::rules::quests::*;

/// A giver's body (local).
#[derive(Component, Debug, Clone)]
pub struct GiverBody {
    pub giver: String,
}

/// The marker over a giver (local).
#[derive(Component, Debug, Clone)]
pub struct QuestMarker {
    pub giver: String,
    pub base: f32,
}

/// Local: open the talk with `giver`.
#[derive(Event, Debug, Clone)]
pub struct OpenTalk {
    pub giver: String,
}

/// The talk you're having, if any (local).
#[derive(Resource, Default, Debug, Clone, PartialEq)]
pub struct Talking(pub Option<Conversation>);

/// One talk with a giver.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    pub giver: String,
    /// The quest it's about.
    pub quest: Option<String>,
    /// The scene of the offer you're in; none: a single line (busy, done, idle).
    pub scene: Option<String>,
    /// That single line.
    pub line: String,
}

/// What the dialogue card shows.
#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub name: String,
    pub line: String,
    pub choices: Vec<String>,
}

/// What a choice did.
#[derive(Debug, Clone, PartialEq)]
pub enum Said {
    /// Nothing (there's no such choice).
    Nothing,
    /// The talk goes on.
    Went,
    /// You took `quest` on (the `variant` way), and they've something to
    /// say about it: the talk goes on.
    Took(String, Option<String>),
    /// The talk is over; maybe you took `quest` on (the `variant` way).
    Ended(Option<(String, Option<String>)>),
}

impl Conversation {
    /// Start talking to `giver`, with the log as it is.
    pub fn open(book: &QuestBook, log: &QuestLog, words: &Dialogues, giver: &str) -> Self {
        let talk = words.givers.get(giver);
        let quest = topic(book, log, giver).map(|q| q.id.clone());
        let about = quest
            .as_deref()
            .and_then(|q| talk.and_then(|t| t.quests.get(q)));
        let mut c = Conversation {
            giver: giver.to_string(),
            quest: quest.clone(),
            scene: None,
            line: talk.map(|t| t.idle.clone()).unwrap_or_default(),
        };
        let (Some(q), Some(about)) = (quest.as_deref(), about) else {
            return c;
        };
        match log.state(q) {
            QuestState::Available => {
                c.line.clear();
                c.scene = Some(about.start.clone());
            }
            QuestState::Failed => {
                // A word about last time, then the offer again.
                c.line = about.failed.clone();
                c.scene = Some(about.start.clone());
            }
            QuestState::Active { .. } => c.line = about.busy.clone(),
            QuestState::Done => c.line = about.done.clone(),
            QuestState::Locked => {}
        }
        c
    }

    fn scene<'a>(&self, words: &'a Dialogues) -> Option<&'a crate::rules::quests::Scene> {
        let q = self.quest.as_deref()?;
        let s = self.scene.as_deref()?;
        words.givers.get(&self.giver)?.quests.get(q)?.scenes.get(s)
    }

    /// What to show.
    pub fn view(&self, words: &Dialogues) -> View {
        let name = words
            .givers
            .get(&self.giver)
            .map_or_else(|| self.giver.clone(), |g| g.name.clone());
        match self.scene(words) {
            Some(s) => {
                // A word about last time goes before the offer (until you answer).
                let line = if self.line.is_empty() {
                    s.line.clone()
                } else {
                    format!("{}\n\n{}", self.line, s.line)
                };
                let mut choices: Vec<String> = s.choices.iter().map(|c| c.text.clone()).collect();
                if choices.is_empty() {
                    choices.push("Bye.".into());
                }
                View {
                    name,
                    line,
                    choices,
                }
            }
            None => View {
                name,
                line: self.line.clone(),
                choices: vec!["Bye.".into()],
            },
        }
    }

    /// Say choice `i`.
    pub fn choose(&mut self, words: &Dialogues, i: usize) -> Said {
        let Some(s) = self.scene(words) else {
            return if i == 0 {
                Said::Ended(None)
            } else {
                Said::Nothing
            };
        };
        if s.choices.is_empty() {
            return if i == 0 {
                Said::Ended(None)
            } else {
                Said::Nothing
            };
        }
        let Some(c) = s.choices.get(i).cloned() else {
            return Said::Nothing;
        };
        // Once past the opening, the failed line is done with.
        self.line.clear();
        let took = match (&c.effect, &self.quest) {
            (Effect::Accept(v), Some(q)) => Some((q.clone(), v.clone())),
            _ => None,
        };
        match c.next {
            Some(n) if took.is_none() => {
                self.scene = Some(n);
                Said::Went
            }
            Some(n) => {
                self.scene = Some(n);
                let (q, v) = took.unwrap_or_default();
                Said::Took(q, v)
            }
            None => Said::Ended(took),
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Talking>()
        .add_observer(click_giver)
        .add_observer(open_talk)
        .add_systems(
            Update,
            (place_givers, mark_givers, walk_away).run_if(in_state(AppState::InGame)),
        )
        .add_systems(OnExit(AppState::InGame), stop_talking);
}

/// The local hour, from the replicated day start.
fn hour_here(day: &Query<&friendslop_base::clock::DayStart>) -> f32 {
    friendslop_base::clock::hour_now(day.iter().next())
}

/// What's drawn for each giver (local).
#[derive(Default)]
struct Placed(HashMap<String, Entity>);

/// Bring givers out while their area shows and they're out, and put them
/// away when they go in for the night (or the morning).
#[allow(clippy::too_many_arguments)]
fn place_givers(
    mut commands: Commands,
    time: Res<Time>,
    mut every: Local<f32>,
    mut placed: Local<Placed>,
    loaded: Res<friendslop_base::areas::Loaded>,
    day: Query<&friendslop_base::clock::DayStart>,
    alive: Query<()>,
) {
    *every -= time.delta_secs();
    if *every > 0.0 {
        return;
    }
    *every = 0.5;
    let givers = GIVERS.get();
    let hour = hour_here(&day);
    placed.0.retain(|_, e| alive.contains(*e));
    for g in &givers.givers {
        let want = loaded.is(&g.area, friendslop_base::areas::Layer::Show) && g.out_at(hour);
        match (want, placed.0.get(&g.id).copied()) {
            (true, None) => {
                let e = spawn_giver(&mut commands, g);
                placed.0.insert(g.id.clone(), e);
            }
            (false, Some(e)) => {
                commands.entity(e).despawn();
                placed.0.remove(&g.id);
            }
            _ => {}
        }
    }
    // Givers taken out of the file.
    placed.0.retain(|id, e| {
        let keep = givers.get(id).is_some();
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
}

fn spawn_giver(commands: &mut Commands, g: &GiverDef) -> Entity {
    let at = g.place.at();
    let mut e = commands.spawn((
        Transform::from_translation(at).with_rotation(Quat::from_rotation_y(g.yaw.to_radians())),
        Visibility::default(),
        friendslop_base::areas::show(&g.area),
    ));
    if g.body {
        e.insert((
            GiverBody {
                giver: g.id.clone(),
            },
            crate::people::Gait::default(),
            crate::people::CharacterLook::random(g.look),
            Clickable::at_height(1.0).radius(70.0).labelled("talk"),
        ));
    }
    e.with_child((
        QuestMarker {
            giver: g.id.clone(),
            base: g.marker_height,
        },
        Transform::from_xyz(0.0, g.marker_height, 0.0),
        Visibility::Hidden,
    ));
    e.id()
}

/// The two marker looks.
struct Marks {
    offer: (Handle<Mesh>, Handle<StandardMaterial>),
    busy: (Handle<Mesh>, Handle<StandardMaterial>),
}

fn marks(
    assets: &AssetServer,
    materials: &mut Assets<StandardMaterial>,
    exact: &mut crate::features::cat_palette::ExactColors,
) -> Marks {
    // Exact colours: the cat palette would pull them towards mustard. Bright
    // and glowing, so they read from across the commune (and bloom).
    let mut glow = |materials: &mut Assets<StandardMaterial>, c: Color, e: LinearRgba| {
        crate::features::cat_palette::exact_material(
            materials,
            exact,
            StandardMaterial {
                base_color: c,
                // Lit, so the emissive counts: past 1.0 it blooms instead of
                // the tonemapper turning a flat yellow into mustard.
                emissive: e,
                fog_enabled: false,
                ..default()
            },
        )
    };
    let mesh = |path: &'static str| {
        assets.load(
            GltfAssetLabel::Primitive {
                mesh: 0,
                primitive: 0,
            }
            .from_asset(path),
        )
    };
    Marks {
        offer: (
            mesh("models/features/quest_offer.gltf"),
            glow(
                materials,
                Color::srgb(1.0, 0.9, 0.1),
                LinearRgba::rgb(2.6, 1.5, 0.0),
            ),
        ),
        busy: (
            mesh("models/features/quest_busy.gltf"),
            glow(
                materials,
                Color::srgb(0.35, 0.85, 1.0),
                LinearRgba::rgb(0.1, 1.2, 3.0),
            ),
        ),
    }
}

/// Each marker shows what its giver has for you, bobbing and turning.
#[allow(clippy::too_many_arguments)]
fn mark_givers(
    mut commands: Commands,
    time: Res<Time>,
    assets: Option<Res<AssetServer>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut exact: ResMut<crate::features::cat_palette::ExactColors>,
    mut looks: Local<Option<Marks>>,
    log: Query<&QuestLog>,
    mut markers: Query<(
        Entity,
        &QuestMarker,
        &mut Transform,
        &mut Visibility,
        Option<&Mesh3d>,
    )>,
) {
    let Some(assets) = assets else { return };
    let looks = looks.get_or_insert_with(|| marks(&assets, &mut materials, &mut exact));
    let book = QUESTS.get();
    let log = log.iter().next();
    let t = time.elapsed_secs();
    for (e, m, mut tf, mut vis, mesh) in &mut markers {
        let shown = log.map_or(Mark::None, |l| mark(&book, l, &m.giver));
        let pick = match shown {
            Mark::Offer => Some(&looks.offer),
            Mark::Busy => Some(&looks.busy),
            Mark::None => None,
        };
        let want = if pick.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        vis.set_if_neq(want);
        if let Some((mesh_h, mat)) = pick {
            if mesh.is_none_or(|have| have.0 != *mesh_h) {
                commands
                    .entity(e)
                    .insert((Mesh3d(mesh_h.clone()), MeshMaterial3d(mat.clone())));
            }
            tf.translation.y = m.base + (t * 2.2).sin() * 0.08;
            tf.rotation = Quat::from_rotation_y(t * 1.4);
            tf.scale = Vec3::splat(1.5);
        }
    }
}

/// Clicking a giver (or the character a body-less giver stands on) opens
/// the talk.
fn click_giver(
    ev: On<Clicked>,
    mut commands: Commands,
    bodies: Query<&GiverBody>,
    places: Query<&GlobalTransform>,
    day: Query<&friendslop_base::clock::DayStart>,
    log: Query<&QuestLog>,
) {
    if let Ok(b) = bodies.get(ev.entity) {
        commands.trigger(OpenTalk {
            giver: b.giver.clone(),
        });
        return;
    }
    // Characters who're already there: only when they've a quest to talk
    // about, so clicking them otherwise does what it always did.
    let Ok(at) = places.get(ev.entity).map(|t| t.translation()) else {
        return;
    };
    let Some(l) = log.iter().next() else { return };
    let book = QUESTS.get();
    let hour = hour_here(&day);
    let givers = GIVERS.get();
    let near = givers.givers.iter().find(|g| {
        !g.body && g.out_at(hour) && g.place.at().distance(at) < 2.5 && {
            mark(&book, l, &g.id) != Mark::None
        }
    });
    if let Some(g) = near {
        commands.trigger(OpenTalk {
            giver: g.id.clone(),
        });
    }
}

fn open_talk(
    ev: On<OpenTalk>,
    mut talking: ResMut<Talking>,
    mut busy: ResMut<HandsBusy>,
    log: Query<&QuestLog>,
) {
    if talking.0.is_some() || busy.0 {
        return;
    }
    let Some(l) = log.iter().next() else { return };
    talking.0 = Some(Conversation::open(
        &QUESTS.get(),
        l,
        &DIALOGUE.get(),
        &ev.giver,
    ));
    busy.0 = true;
}

/// Say choice `i` in the talk (the dialogue card calls this): sends the
/// yes to the server and ends the talk when it's over.
pub fn say(commands: &mut Commands, talking: &mut Talking, busy: &mut HandsBusy, i: usize) {
    let Some(c) = talking.0.as_mut() else {
        return;
    };
    let words = DIALOGUE.get();
    match c.choose(&words, i) {
        Said::Nothing | Said::Went => {}
        Said::Took(quest, variant) => {
            commands.client_trigger(TalkChoice {
                giver: c.giver.clone(),
                quest,
                variant,
            });
        }
        Said::Ended(took) => {
            if let Some((quest, variant)) = took {
                commands.client_trigger(TalkChoice {
                    giver: c.giver.clone(),
                    quest,
                    variant,
                });
            }
            end_talk(talking, busy);
        }
    }
}

/// Leave the talk.
pub fn end_talk(talking: &mut Talking, busy: &mut HandsBusy) {
    if talking.0.take().is_some() {
        busy.0 = false;
    }
}

/// Walking (or being carried) away from a giver ends the talk.
fn walk_away(
    mut talking: ResMut<Talking>,
    mut busy: ResMut<HandsBusy>,
    me: Option<Res<LocalPlayer>>,
    walkers: Query<(&Player, &GlobalTransform), With<crate::features::fps::Walker>>,
) {
    let Some(c) = talking.0.as_ref() else { return };
    let Some(g) = GIVERS.get().get(&c.giver).cloned() else {
        end_talk(&mut talking, &mut busy);
        return;
    };
    let Some(me) = me else { return };
    let mine = walkers
        .iter()
        .find(|(p, _)| p.id == me.0)
        .map(|(_, t)| t.translation());
    if mine.is_some_and(|at| at.distance(g.place.at()) > 7.0) {
        end_talk(&mut talking, &mut busy);
    }
}

fn stop_talking(mut talking: ResMut<Talking>, mut busy: ResMut<HandsBusy>) {
    end_talk(&mut talking, &mut busy);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn talking_offers_takes_on_and_then_talks_about_it() {
        let book = QUESTS.get();
        let words = DIALOGUE.get();
        let mut log = QuestLog::fresh(&book);
        let q = book
            .quests
            .iter()
            .find(|q| log.state(&q.id) == QuestState::Available)
            .cloned()
            .unwrap_or_default();
        let mut c = Conversation::open(&book, &log, &words, &q.giver);
        assert_eq!(c.quest.as_deref(), Some(q.id.as_str()));
        assert!(!c.view(&words).choices.is_empty());
        // Say the first thing every time until it's over.
        let mut took = None;
        for _ in 0..20 {
            let n = c.view(&words).choices.len();
            // Prefer a yes when there is one.
            let yes = (0..n).find(|&i| {
                c.scene(&words)
                    .and_then(|s| s.choices.get(i))
                    .is_some_and(|ch| matches!(ch.effect, Effect::Accept(_)))
            });
            match c.choose(&words, yes.unwrap_or(0)) {
                Said::Took(q, v) => {
                    took = Some((q, v));
                    break;
                }
                Said::Ended(t) => {
                    took = t;
                    break;
                }
                Said::Went => {}
                Said::Nothing => panic!("a choice that isn't there"),
            }
        }
        let (id, variant) = took.unwrap_or_default();
        assert_eq!(id, q.id);
        assert!(log.accept(&book, &id, variant.as_deref()));
        let c = Conversation::open(&book, &log, &words, &q.giver);
        assert_eq!(c.scene, None, "busy: one line");
        assert_eq!(c.view(&words).choices, vec!["Bye.".to_string()]);
    }
}
