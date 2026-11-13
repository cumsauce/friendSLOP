//! The Anarcho-Syndicalist Commune of Dave. From issue #3.
//!
//! Parliament has burned down and Dave's militia runs the place. Pure scenery,
//! spawned locally on every peer (nothing to replicate): a smouldering
//! parliament ruin, the commune's flag, Dave himself, and a banner.
//!
//! The flag is themed on the flag of the UK, but with the Welsh dragon in the
//! middle, because Dave isn't a coward (Freyja Moth's correction in #43).
//! It was then made more anarchist (#57): the field is split diagonally red
//! over black like an anarcho-syndicalist flag, the crosses are black, and a
//! circled A is sprayed on the top hoist corner.
//!
//! Everything is a modeler model (sources in `.modeler/`): the ruin, tower
//! and pole, the flag (`fa-commune-flag`, built from [`commune_flag`]'s
//! design, fluttering with its `flutter` clip) and Dave himself
//! (`fa-dave`, a rigged plush lad playing his `idle` clip). A missing file
//! just draws nothing.

use bevy::prelude::*;

use crate::AppState;
use crate::rigged::{RigClip, Rigged};

pub fn protocol(_app: &mut App) {}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_commune);
}

/// Where the burnt-out parliament stands (x, z).
pub use friendslop_world::places::parliament_at;

/// Where Dave himself stands, 7 m from the parliament; the fart-shame check
/// looks for him here.
pub use friendslop_world::places::dave_at;

/// Flag size (width, height), in metres.
#[cfg(test)]
const FLAG_W: f32 = 2.4;
const FLAG_H: f32 = 1.2;

// The flag's design, kept as the record the model was built from (the
// `fa-commune-flag` material paints exactly these pieces) and checked by
// the test below.
#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FlagColour {
    Black,
    White,
    Red,
    Green,
}

/// The outline of a flag piece.
#[cfg(test)]
#[allow(dead_code)] // the outlines are painted by the model, not read here
#[derive(Clone, Copy, Debug)]
enum FlagShape {
    /// A rectangle of this size.
    Rect(Vec2),
    /// A triangle with these corners, relative to the piece's `at`.
    Tri([Vec2; 3]),
}

/// One flat piece of the flag, centred at `at` and rotated by `angle`
/// radians. Later pieces are thicker so they sit on top (both sides).
#[cfg(test)]
#[allow(dead_code)] // the angles are painted by the model, not read here
#[derive(Clone, Copy, Debug)]
struct FlagPart {
    colour: FlagColour,
    at: Vec2,
    shape: FlagShape,
    angle: f32,
    depth: f32,
}

/// The commune flag: the Union flag's saltires and cross on an
/// anarcho-syndicalist red-and-black field, with black crosses, a circled A,
/// and a little Welsh flag (white over green) in the middle carrying a red
/// dragon.
#[cfg(test)]
fn commune_flag() -> Vec<FlagPart> {
    use FlagColour::*;
    let mut parts = Vec::new();
    let mut layer = 0.04;
    let mut add_shape = |colour, at: Vec2, shape, deg: f32| {
        parts.push(FlagPart {
            colour,
            at,
            shape,
            angle: deg.to_radians(),
            depth: layer,
        });
        layer += 0.004;
    };
    let diag = FLAG_W.hypot(FLAG_H) - 0.25;
    let tilt = FLAG_H.atan2(FLAG_W).to_degrees();
    // Field: red above, black below, split corner to corner (hidden under a
    // saltire, like the Union flag's own diagonals).
    let (hw, hh) = (FLAG_W / 2.0, FLAG_H / 2.0);
    add_shape(
        Red,
        Vec2::ZERO,
        FlagShape::Rect(Vec2::new(FLAG_W, FLAG_H)),
        0.0,
    );
    let corners = [Vec2::new(-hw, -hh), Vec2::new(hw, -hh), Vec2::new(hw, hh)];
    let centroid = (corners[0] + corners[1] + corners[2]) / 3.0;
    add_shape(
        Black,
        centroid,
        FlagShape::Tri(corners.map(|c| c - centroid)),
        0.0,
    );
    let mut add = |colour, x: f32, y: f32, w: f32, h: f32, deg: f32| {
        add_shape(
            colour,
            Vec2::new(x, y),
            FlagShape::Rect(Vec2::new(w, h)),
            deg,
        );
    };
    // Saltires: white, then thinner black on top.
    add(White, 0.0, 0.0, diag, 0.24, tilt);
    add(White, 0.0, 0.0, diag, 0.24, -tilt);
    add(Black, 0.0, 0.0, diag, 0.08, tilt);
    add(Black, 0.0, 0.0, diag, 0.08, -tilt);
    // Cross: white, then black.
    add(White, 0.0, 0.0, FLAG_W, 0.36, 0.0);
    add(White, 0.0, 0.0, 0.36, FLAG_H, 0.0);
    add(Black, 0.0, 0.0, FLAG_W, 0.22, 0.0);
    add(Black, 0.0, 0.0, 0.22, FLAG_H, 0.0);
    // A circled A, sprayed in black on the red top hoist corner.
    let (ax, ay, r) = (-0.4, 0.46, 0.11);
    for i in 0..12 {
        let a = i as f32 * std::f32::consts::TAU / 12.0;
        add(
            Black,
            ax + a.cos() * r,
            ay + a.sin() * r,
            0.025,
            0.062,
            a.to_degrees(),
        );
    }
    add(Black, ax - 0.035, ay, 0.022, 0.21, -18.0);
    add(Black, ax + 0.035, ay, 0.022, 0.21, 18.0);
    add(Black, ax, ay - 0.03, 0.13, 0.022, 0.0);
    // Wales, not cowardly left out: white over green...
    add(White, 0.0, 0.125, 0.7, 0.25, 0.0);
    add(Green, 0.0, -0.125, 0.7, 0.25, 0.0);
    // ...and the red dragon, facing the pole, one claw raised.
    for (x, y, w, h, deg) in [
        (0.0, -0.02, 0.28, 0.1, 0.0),     // body
        (-0.15, 0.05, 0.06, 0.15, -30.0), // neck
        (-0.21, 0.12, 0.1, 0.06, 0.0),    // head
        (0.0, 0.1, 0.06, 0.18, -20.0),    // wing
        (0.07, 0.1, 0.05, 0.15, -35.0),   // other wing
        (0.18, 0.02, 0.16, 0.04, 40.0),   // tail
        (0.25, 0.1, 0.04, 0.08, 0.0),     // tail tip
        (-0.08, -0.12, 0.04, 0.1, 0.0),   // back leg
        (0.08, -0.12, 0.04, 0.1, 0.0),    // back leg
        (-0.17, -0.03, 0.1, 0.035, 20.0), // raised claw
    ] {
        add(Red, x, y, w, h, deg);
    }
    parts
}

fn spawn_commune(mut commands: Commands, assets: Res<AssetServer>) {
    let at = |x: f32, y: f32, z: f32| {
        Transform::from_xyz(parliament_at().x + x, y, parliament_at().y + z)
    };
    // Modeler-made (sources in `.modeler/`, prefabs `sp-parliament-ruin`,
    // `sp-clock-tower`, `sp-flag-pole`).
    let model = |file: &str| {
        WorldAssetRoot(
            assets.load(GltfAssetLabel::Scene(0).from_asset(format!("models/features/{file}"))),
        )
    };

    // Charred parliament: a stepped plinth, a broken colonnade with capitals,
    // a cracked arch, scorched walls with jagged tops, scorch marks, embers
    // and rubble.
    commands.spawn((
        model("parliament_ruin.gltf"),
        at(0.0, 0.0, 0.0),
        DespawnOnExit(AppState::InGame),
    ));
    // The toppled tower, its pointed roof knocked off beside it, clock face up.
    commands.spawn((
        model("clock_tower.gltf"),
        at(6.0, 0.0, 0.0),
        DespawnOnExit(AppState::InGame),
    ));

    // Flag on a pole planted in the ruins.
    commands.spawn((
        model("flag_pole.gltf"),
        at(1.0, 2.5, 6.0),
        DespawnOnExit(AppState::InGame),
    ));
    // The flag hangs from a hinge at the pole and swings on it in the breeze
    // (cheap waving that can't come off the pole); the cloth ripples with
    // its own `flutter` clip. The model's origin is the hinge, the cloth
    // running out along +X.
    commands
        .spawn((
            at(1.0 + 0.06, 5.0 - FLAG_H / 2.0, 6.0),
            Visibility::default(),
            crate::art::Wiggle::sway(Quat::IDENTITY, Vec3::Y, 0.28, 1.3, 0.0),
            DespawnOnExit(AppState::InGame),
        ))
        .with_child((
            Transform::default(),
            Visibility::default(),
            Rigged {
                path: "models/features/commune_flag.gltf",
            },
            RigClip::looping("flutter"),
        ));
    // Dave: a little lad in a jumper and beanie, fist raised for the commune
    // (faces -Z, origin at his feet). His `idle` breathes, pumps the fist,
    // swings the other arm, tilts his head, swings the beanie's bobble and
    // blinks.
    commands.spawn((
        at(-1.0, 0.0, 7.0),
        Visibility::default(),
        Rigged {
            path: "models/creatures/dave.gltf",
        },
        RigClip::looping("idle"),
        DespawnOnExit(AppState::InGame),
    ));

    commands.spawn((
        Text::new("Commune of Dave: power generates parasites"),
        crate::hud::title(),
        crate::hud::slot(crate::hud::HudSlot::TopCenter),
        // Quiet: shows when you arrive near the parliament, then lives in the journal.
        crate::hud::Calm::new("Commune of Dave").near(Vec3::new(
            parliament_at().x,
            0.0,
            parliament_at().y,
        )),
        DespawnOnExit(AppState::InGame),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_is_anarchist_union_flag_with_a_welsh_dragon() {
        let parts = commune_flag();
        assert_eq!(parts[0].colour, FlagColour::Red, "red field...");
        assert_eq!(parts[1].colour, FlagColour::Black, "...split with black");
        assert!(matches!(parts[1].shape, FlagShape::Tri(_)));
        assert!(parts.iter().any(|p| p.colour == FlagColour::Green), "wales");
        // The dragon (red, on top of the Welsh patch) is there.
        let wales = parts
            .iter()
            .position(|p| p.colour == FlagColour::Green)
            .unwrap_or(usize::MAX);
        assert!(
            parts[wales + 1..]
                .iter()
                .all(|p| p.colour == FlagColour::Red)
        );
        assert!(parts.len() > wales + 5);
        // Each piece is layered above the last so nothing z-fights.
        assert!(parts.windows(2).all(|w| w[1].depth > w[0].depth));
        // No blue left: the crosses are black now.
        assert!(
            parts[2..]
                .iter()
                .filter(|p| p.colour == FlagColour::Black)
                .count()
                > 15,
            "black crosses and a circled A"
        );
        // Everything stays roughly on the flag.
        assert!(
            parts
                .iter()
                .all(|p| p.at.x.abs() <= FLAG_W / 2.0 && p.at.y.abs() <= FLAG_H / 2.0)
        );
    }
}
