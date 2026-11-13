//! What the Watcher leaves behind (#278, Horror ARG 2/3, catherine's #182).
//!
//! Where the Watcher vanishes, the server leaves a replicated [`Scrap`] of
//! paper on the ground, carrying the next piece of a coded message nobody in
//! the commune has read yet. Click it to read it: the piece goes into the
//! commune's shared [`Clues`] (everyone reads it on screen) and the scrap is
//! gone. Once every piece is found, the whole message shows, and it points
//! somewhere on the map (#279 makes something answer there).
//!
//! The pieces are writing: `assets/data/text/clues.ron`.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use super::watcher::WatcherVanished;
use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, in_reach};
use crate::player::PlayerOwner;

/// Tuning numbers: `assets/data/side/clues.ron` (read at run time, edit without a rebuild).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// How long a piece you've just read stays on screen, in seconds.
    pub show_secs: f32,
    /// How long the whole message stays on screen once it's complete.
    pub complete_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/clues.ron");

/// The message: `assets/data/text/clues.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Writing {
    /// The pieces, in order. One scrap carries one piece.
    pub pieces: Vec<String>,
    /// Shown once every piece is found, under the whole message.
    pub complete: String,
}

pub static WRITING: friendslop_data::Data<Writing> =
    friendslop_data::Data::ron("data/text/clues.ron");

/// A scrap of paper lying where the Watcher vanished, carrying piece `piece`.
#[derive(Component, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Scrap {
    pub at: Vec2,
    pub piece: u8,
}

/// The commune's pieces so far (one replicated entity).
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Clues {
    pub found: Vec<u8>,
}

/// Client -> server: I picked up the scrap carrying `piece`.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct TakeScrap {
    pub piece: u8,
}

pub fn protocol(app: &mut App) {
    app.replicate::<Scrap>()
        .replicate::<Clues>()
        .add_client_event::<TakeScrap>(Channel::Ordered)
        .add_observer(drop_scrap)
        .add_observer(take_scrap)
        .add_systems(OnEnter(ServerState::Running), spawn_clues);
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_text)
        .add_systems(Update, (dress, read).run_if(in_state(AppState::InGame)))
        .add_observer(click_scrap);
}

/// The next piece to leave: the first one that is neither found nor already
/// lying somewhere. `None` once every piece is out.
pub fn next_piece(total: usize, found: &[u8], lying: &[u8]) -> Option<u8> {
    (0..total.min(u8::MAX as usize) as u8).find(|p| !found.contains(p) && !lying.contains(p))
}

/// What reading the commune's pieces shows: the newest piece with a count,
/// or, once all are found, the whole message in order.
pub fn reading(w: &Writing, found: &[u8]) -> String {
    let total = w.pieces.len();
    let Some(&newest) = found.last() else {
        return String::new();
    };
    let piece = |p: u8| w.pieces.get(p as usize).cloned().unwrap_or_default();
    let complete = total > 0 && (0..total as u8).all(|p| found.contains(&p));
    if complete {
        let all: Vec<String> = (0..total as u8).map(piece).collect();
        format!("{}\n{}", all.join(" "), w.complete)
    } else {
        format!(
            "A scrap of paper ({}/{}): \"{}\"",
            found.len(),
            total,
            piece(newest)
        )
    }
}

fn spawn_clues(mut commands: Commands, existing: Query<(), With<Clues>>) {
    if existing.is_empty() {
        commands.spawn((Clues::default(), Replicated));
    }
}

/// Server: the Watcher vanished, so leave the next scrap where it stood.
fn drop_scrap(
    ev: On<WatcherVanished>,
    mut commands: Commands,
    server: Option<Res<State<ServerState>>>,
    clues: Query<&Clues>,
    scraps: Query<&Scrap>,
) {
    if server.is_none_or(|s| *s.get() != ServerState::Running) {
        return;
    }
    let found = clues
        .iter()
        .next()
        .map(|c| c.found.clone())
        .unwrap_or_default();
    let lying: Vec<u8> = scraps.iter().map(|s| s.piece).collect();
    let Some(piece) = next_piece(WRITING.get().pieces.len(), &found, &lying) else {
        return;
    };
    commands.spawn((Scrap { at: ev.at, piece }, Replicated));
}

/// Server: someone in reach picks up a scrap; its piece joins the clues.
fn take_scrap(
    ev: On<FromClient<TakeScrap>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    scraps: Query<(Entity, &Scrap)>,
    mut clues: Query<&mut Clues>,
) {
    let Some((entity, scrap)) = scraps.iter().find(|(_, s)| s.piece == ev.piece) else {
        return;
    };
    let at = Vec3::new(scrap.at.x, 0.0, scrap.at.y);
    if !in_reach(&mut commands, ev.client_id, &walkers, at) {
        return;
    }
    commands.entity(entity).despawn();
    let Some(mut clues) = clues.iter_mut().next() else {
        return;
    };
    if !clues.found.contains(&scrap.piece) {
        clues.found.push(scrap.piece);
    }
}

/// Every peer: a small pale sheet lying flat on the ground, clickable.
fn dress(
    mut commands: Commands,
    new: Query<(Entity, &Scrap), Added<Scrap>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (entity, scrap) in &new {
        let ground = crate::terrain::height_at(scrap.at.x, scrap.at.y);
        let paper = materials.add(StandardMaterial {
            base_color: Color::srgb(0.86, 0.83, 0.72),
            emissive: LinearRgba::rgb(0.08, 0.08, 0.07),
            perceptual_roughness: 0.95,
            ..default()
        });
        let ink = materials.add(StandardMaterial {
            base_color: Color::srgb(0.12, 0.03, 0.03),
            perceptual_roughness: 1.0,
            ..default()
        });
        let yaw = scrap.piece as f32 * 1.9;
        commands
            .entity(entity)
            .insert((
                Transform::from_xyz(scrap.at.x, ground + 0.02, scrap.at.y)
                    .with_rotation(Quat::from_rotation_y(yaw)),
                Visibility::default(),
                Clickable::at_height(0.05).labelled("read the scrap"),
                DespawnOnExit(AppState::InGame),
            ))
            .with_children(|c| {
                c.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.3, 0.01, 0.4))),
                    MeshMaterial3d(paper),
                ));
                // A scrawled line of ink across it.
                c.spawn((
                    Mesh3d(meshes.add(Cuboid::new(0.2, 0.012, 0.03))),
                    MeshMaterial3d(ink),
                    Transform::from_xyz(0.0, 0.002, 0.05).with_rotation(Quat::from_rotation_y(0.2)),
                ));
            });
    }
}

/// The unified click landed on a scrap: ask the server for it.
fn click_scrap(click: On<Clicked>, mut commands: Commands, scraps: Query<&Scrap>) {
    if let Ok(scrap) = scraps.get(click.entity) {
        commands.client_trigger(TakeScrap { piece: scrap.piece });
    }
}

#[derive(Component)]
struct ClueText;

fn spawn_text(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        TextColor(Color::srgb(0.9, 0.82, 0.78)),
        ClueText,
        crate::hud::slot(crate::hud::HudSlot::Subtitle),
        DespawnOnExit(AppState::InGame),
    ));
}

/// Every peer: when the commune finds a piece, show it to everyone for a
/// while (or the whole message, once complete).
fn read(
    time: Res<Time>,
    clues: Query<&Clues, Changed<Clues>>,
    mut text: Query<&mut Text, With<ClueText>>,
    mut left: Local<f32>,
) {
    let Ok(mut text) = text.single_mut() else {
        return;
    };
    if let Some(clues) = clues.iter().next()
        && !clues.found.is_empty()
    {
        let w = WRITING.get();
        let t = TUNING.get();
        text.0 = reading(&w, &clues.found);
        *left = if clues.found.len() >= w.pieces.len() {
            t.complete_secs
        } else {
            t.show_secs
        };
        return;
    }
    if *left > 0.0 {
        *left -= time.delta_secs();
        if *left <= 0.0 {
            text.0.clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn writing() -> Writing {
        Writing {
            pieces: vec!["A".into(), "B".into(), "C".into()],
            complete: "done".into(),
        }
    }

    #[test]
    fn scraps_leave_in_order_and_run_out() {
        assert_eq!(next_piece(3, &[], &[]), Some(0));
        assert_eq!(next_piece(3, &[0], &[1]), Some(2));
        assert_eq!(next_piece(3, &[0, 2], &[]), Some(1));
        assert_eq!(next_piece(3, &[0, 1], &[2]), None);
    }

    #[test]
    fn reading_shows_the_newest_then_the_whole_message() {
        let w = writing();
        assert_eq!(reading(&w, &[]), "");
        assert_eq!(reading(&w, &[0, 2]), "A scrap of paper (2/3): \"C\"");
        assert_eq!(reading(&w, &[2, 0, 1]), "A B C\ndone");
    }

    #[test]
    fn the_message_has_pieces() {
        let w = WRITING.get();
        assert!(w.pieces.len() >= 3, "{:?}", w.pieces);
        assert!(!w.complete.is_empty());
    }
}
