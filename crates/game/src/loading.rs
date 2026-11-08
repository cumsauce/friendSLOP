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

fn spawn_curtain(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut ready: ResMut<WorldReady>,
    mut progress: ResMut<Progress>,
) {
    *ready = WorldReady::default();
    *progress = Progress {
        started: time.elapsed_secs(),
        smooth: 0,
    };
    commands.insert_resource(WorldReadyPending);
    commands
        .spawn((
            Curtain,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.12, 0.16, 0.22)),
            GlobalZIndex(1000),
            DespawnOnExit(AppState::InGame),
        ))
        .with_child((
            Text::new("Loading the commune..."),
            TextFont::from_font_size(28.0),
            TextColor(Color::srgb(0.9, 0.92, 1.0)),
        ));
}

fn lift_curtain(
    mut commands: Commands,
    time: Res<Time<Real>>,
    assets: Option<Res<AssetServer>>,
    roots: Query<&WorldAssetRoot>,
    curtains: Query<Entity, With<Curtain>>,
    mut ready: ResMut<WorldReady>,
    mut progress: ResMut<Progress>,
) {
    let elapsed = time.elapsed_secs() - progress.started;
    let loading = assets.is_some_and(|assets| {
        roots.iter().any(|root| {
            matches!(
                assets.get_recursive_dependency_load_state(root.0.id()),
                Some(RecursiveDependencyLoadState::Loading)
            )
        })
    });
    if loading || time.delta_secs() * 1000.0 > SMOOTH_MS {
        progress.smooth = 0;
    } else {
        progress.smooth += 1;
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
