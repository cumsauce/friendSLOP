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
use crate::assets::DisplayText;
use crate::theme::{self, Bob};

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
            (lift_curtain, walk_critter)
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
            BackgroundColor(Color::srgb(0.99, 0.9, 0.93)),
            GlobalZIndex(1000),
            DespawnOnExit(AppState::InGame),
        ))
        .with_children(|bg| {
            // Soft pastel blobs drifting in the background.
            for (i, (x, y, size, col)) in [
                (8.0, 12.0, 260.0, theme::LILAC),
                (70.0, 58.0, 340.0, theme::MINT),
                (40.0, -10.0, 200.0, theme::BUTTER),
                (-6.0, 64.0, 220.0, theme::PEACH),
                (82.0, 6.0, 160.0, theme::PINK),
            ]
            .into_iter()
            .enumerate()
            {
                bg.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: percent(x),
                        top: percent(y),
                        width: px(size),
                        height: px(size),
                        border_radius: BorderRadius::all(px(size)),
                        ..default()
                    },
                    BackgroundColor(col.with_alpha(0.35)),
                    UiTransform::default(),
                    Bob {
                        phase: i as f32 * 1.3,
                        height: 18.0,
                        speed: 0.6,
                    },
                ));
            }
            bg.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    align_items: AlignItems::Center,
                    padding: UiRect::axes(px(48), px(30)),
                    row_gap: px(theme::S3),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::all(px(32)),
                    ..default()
                },
                BackgroundColor(theme::CARD),
                BorderColor::all(theme::LINE),
                theme::shadow(10.0, 36.0),
            ))
            .with_children(|card| {
                critter(card);
                card.spawn(Node {
                    align_items: AlignItems::FlexEnd,
                    column_gap: px(3),
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("Loading the commune"),
                        TextFont::from_font_size(30.0),
                        TextColor(theme::PINK.mix(&theme::INK, 0.15)),
                        TextShadow {
                            offset: Vec2::new(2.0, 3.0),
                            color: theme::SHADOW,
                        },
                        DisplayText,
                    ));
                    for (i, col) in [theme::PINK, theme::LILAC, theme::MINT]
                        .into_iter()
                        .enumerate()
                    {
                        row.spawn((
                            Node {
                                width: px(8),
                                height: px(8),
                                margin: UiRect::bottom(px(8)),
                                border_radius: BorderRadius::all(px(4)),
                                ..default()
                            },
                            BackgroundColor(col),
                            UiTransform::default(),
                            Bob {
                                phase: -(i as f32) * 0.9,
                                height: 9.0,
                                speed: 6.0,
                            },
                        ));
                    }
                });
                card.spawn((
                    Text::new("warming up the campfire, fluffing the grass"),
                    TextFont::from_font_size(15.0),
                    TextColor(theme::SOFT_INK),
                    DisplayText,
                ));
            });
        });
}

/// The little walking critter on the loading card.
#[derive(Component)]
struct Critter;

/// A round pink blob with eyes, blush and feet, built from UI nodes.
fn critter(card: &mut ChildSpawnerCommands) {
    let dot = |size: f32, col: Color| {
        (
            Node {
                width: px(size),
                height: px(size),
                border_radius: BorderRadius::all(px(size)),
                ..default()
            },
            BackgroundColor(col),
        )
    };
    card.spawn((
        Critter,
        Node {
            width: px(240),
            height: px(64),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexEnd,
            ..default()
        },
    ))
    .with_children(|lane| {
        lane.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                ..default()
            },
            UiTransform::default(),
            CritterWalk,
        ))
        .with_children(|c| {
            c.spawn((
                Node {
                    width: px(50),
                    height: px(42),
                    border: UiRect::all(px(2)),
                    border_radius: BorderRadius::new(px(26), px(26), px(16), px(16)),
                    padding: UiRect::top(px(13)),
                    column_gap: px(12),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                BackgroundColor(theme::PINK),
                BorderColor::all(theme::shade(theme::PINK, 0.35)),
                UiTransform::default(),
                Bob {
                    phase: 0.0,
                    height: 7.0,
                    speed: 9.0,
                },
            ))
            .with_children(|face| {
                face.spawn(dot(7.0, theme::INK));
                face.spawn(dot(7.0, theme::INK));
            });
            c.spawn(Node {
                column_gap: px(14),
                ..default()
            })
            .with_children(|feet| {
                feet.spawn(dot(10.0, theme::shade(theme::PINK, 0.35)));
                feet.spawn(dot(10.0, theme::shade(theme::PINK, 0.35)));
            });
        });
    });
}

#[derive(Component)]
struct CritterWalk;

/// Pace the critter back and forth along its lane, facing where it walks.
fn walk_critter(time: Res<Time<Real>>, mut q: Query<&mut UiTransform, With<CritterWalk>>) {
    let t = time.elapsed_secs() * 0.9;
    let x = t.sin() * 80.0;
    let facing = if t.cos() >= 0.0 { 1.0 } else { -1.0 };
    for mut tf in &mut q {
        tf.translation = Val2::px(x, 0.0);
        tf.scale = Vec2::new(facing, 1.0);
    }
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
