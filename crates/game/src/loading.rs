//! Brief "loading" curtain shown on entering the game (not a friend idea; perf).
//!
//! On macOS wgpu/Bevy compile every render pipeline synchronously, and the
//! scenery glTFs stream in over the first seconds, each needing new pipelines
//! and GPU uploads. That made the first seconds in game a slideshow of
//! multi-hundred-ms (up to multi-second) frames. Instead of showing a frozen
//! world, an opaque overlay covers the screen while the world keeps rendering
//! behind it (so the pipelines warm up), and lifts once every scenery model has
//! loaded (or failed: missing LFS data never blocks) and frames are smooth again.
//! Purely local; a hard timeout guarantees it always lifts.

use bevy::asset::RecursiveDependencyLoadState;
use bevy::prelude::*;

use crate::AppState;

/// Frames this long or shorter count as "smooth".
const SMOOTH_MS: f32 = 50.0;
/// Consecutive smooth frames needed before the curtain lifts.
const SMOOTH_FRAMES: u32 = 20;
/// Never keep the curtain up longer than this (seconds, real time).
const MAX_SECS: f32 = 15.0;

/// `true` once the loading curtain has lifted for this session.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct WorldReady {
    pub ready: bool,
    /// Real seconds from entering the game until the curtain lifted.
    pub took_secs: f32,
}

#[derive(Component)]
struct Curtain;

#[derive(Resource, Default)]
struct Progress {
    started: f32,
    smooth: u32,
}

pub fn plugin(app: &mut App) {
    app.init_resource::<WorldReady>()
        .init_resource::<Progress>()
        .add_systems(OnEnter(AppState::InGame), spawn_curtain)
        .add_systems(
            Update,
            lift_curtain
                .run_if(in_state(AppState::InGame))
                .run_if(resource_exists::<WorldReadyPending>),
        );
}

/// Present while the curtain is up.
#[derive(Resource)]
struct WorldReadyPending;

/// Tips under the progress bar, one per load (by the clock).
pub const TIPS: [&str; 4] = [
    "Pet your cat before it becomes everyone’s problem.",
    "Hold Tab (or press J) for the commune journal.",
    "F farts once the gas meter says READY.",
    "M mutes your mic. Your friends can still hear the cat.",
];

/// How far along loading is, `0..=1`: models loaded, then frames settling.
pub fn fraction(loaded: usize, total: usize, smooth: u32) -> f32 {
    let models = if total == 0 {
        1.0
    } else {
        loaded as f32 / total as f32
    };
    (0.85 * models + 0.15 * (smooth as f32 / SMOOTH_FRAMES as f32).min(1.0)).clamp(0.0, 1.0)
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Part {
    Detail,
    Percent,
    Bar,
}

const BAR_W: f32 = 1104.0;

/// The curtain (Figma 3:2198): ink, the illustrated logo, a progress bar
/// with how many models have loaded, and a tip.
fn spawn_curtain(
    mut commands: Commands,
    time: Res<Time<Real>>,
    assets: Option<Res<AssetServer>>,
    mut ready: ResMut<WorldReady>,
    mut progress: ResMut<Progress>,
) {
    use crate::theme::indigo::{CREAM, INK, PERIWINKLE};
    use crate::ui::{at, body, head};
    *ready = WorldReady::default();
    *progress = Progress {
        started: time.elapsed_secs(),
        smooth: 0,
    };
    commands.insert_resource(WorldReadyPending);
    let logo: Handle<Image> = assets.map(|a| a.load("ui/logo.png")).unwrap_or_default();
    let tip = TIPS[(crate::settings::since_epoch().as_secs() as usize) % TIPS.len()];
    commands.spawn((
        Curtain,
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BackgroundColor(INK),
        GlobalZIndex(1000),
        DespawnOnExit(AppState::InGame),
        children![(
            // The 1280x720 artboard, centred.
            Node {
                width: px(1280),
                height: px(720),
                flex_shrink: 0.0,
                ..default()
            },
            children![
                (
                    Text::new("Putting the kettle on…"),
                    head(24.0, PERIWINKLE),
                    at(88.0, 72.0, 0.0)
                ),
                (
                    Node {
                        width: px(600),
                        height: px(173),
                        ..at(88.0, 144.0, 0.0)
                    },
                    ImageNode::new(logo),
                ),
                (
                    Text::new("Negotiating with the landlord…"),
                    head(34.0, CREAM),
                    at(88.0, 386.0, 0.0)
                ),
                (
                    Part::Detail,
                    Text::new(""),
                    body(20.0, CREAM),
                    at(88.0, 446.0, 0.0)
                ),
                (
                    Part::Percent,
                    Text::new(""),
                    head(36.0, CREAM),
                    at(1100.0, 440.0, 0.0)
                ),
                (
                    Node {
                        width: px(BAR_W),
                        height: px(28),
                        border_radius: BorderRadius::all(px(14)),
                        overflow: Overflow::clip(),
                        ..at(88.0, 494.0, 0.0)
                    },
                    BackgroundColor(Color::srgb_u8(0xE5, 0xD7, 0xE8)),
                    children![(
                        Part::Bar,
                        Node {
                            width: px(0),
                            height: percent(100),
                            border_radius: BorderRadius::all(px(14)),
                            ..default()
                        },
                        BackgroundColor(PERIWINKLE),
                    )],
                ),
                (
                    Text::new(format!("TIP / {tip}")),
                    body(22.0, CREAM),
                    at(88.0, 566.0, 0.0)
                ),
                (
                    Text::new("Your friends are worth the wait."),
                    body(17.0, PERIWINKLE),
                    at(88.0, 636.0, 0.0)
                ),
            ],
        )],
    ));
}

fn lift_curtain(
    mut commands: Commands,
    time: Res<Time<Real>>,
    assets: Option<Res<AssetServer>>,
    roots: Query<&WorldAssetRoot>,
    curtains: Query<Entity, With<Curtain>>,
    mut parts: Query<(&Part, Option<&mut Text>, &mut Node)>,
    mut ready: ResMut<WorldReady>,
    mut progress: ResMut<Progress>,
) {
    let elapsed = time.elapsed_secs() - progress.started;
    let total = roots.iter().count();
    let still = assets.map_or(0, |assets| {
        roots
            .iter()
            .filter(|root| {
                matches!(
                    assets.get_recursive_dependency_load_state(root.0.id()),
                    Some(RecursiveDependencyLoadState::Loading)
                )
            })
            .count()
    });
    let loading = still > 0;
    if loading || time.delta_secs() * 1000.0 > SMOOTH_MS {
        progress.smooth = 0;
    } else {
        progress.smooth += 1;
    }
    let frac = fraction(total - still, total, progress.smooth);
    for (part, text, mut node) in &mut parts {
        let want = match part {
            Part::Bar => {
                let w = px(BAR_W * frac);
                if node.width != w {
                    node.width = w;
                }
                continue;
            }
            Part::Percent => format!("{}%", (frac * 100.0).round() as u32),
            Part::Detail if loading => {
                format!("Unpacking the commune · {} / {total} models", total - still)
            }
            Part::Detail => "Warming up the campfire".to_string(),
        };
        if let Some(mut t) = text
            && t.0 != want
        {
            t.0 = want;
        }
    }
    if progress.smooth < SMOOTH_FRAMES && elapsed < MAX_SECS {
        return;
    }
    for e in &curtains {
        commands.entity(e).despawn();
    }
    commands.remove_resource::<WorldReadyPending>();
    *ready = WorldReady {
        ready: true,
        took_secs: elapsed,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_counts_models_then_settling() {
        assert_eq!(fraction(0, 10, 0), 0.0);
        assert!((fraction(5, 10, 0) - 0.425).abs() < 1e-6);
        assert_eq!(fraction(10, 10, SMOOTH_FRAMES * 3), 1.0);
        assert_eq!(fraction(0, 0, SMOOTH_FRAMES), 1.0);
    }
}
