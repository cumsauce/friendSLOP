//! The main menu's "Kori prizes" page (#382): every claw machine figure, how
//! many of each you've won (your `side::kori::Shelf`), and the one you click
//! spinning on a little stage to inspect.

use bevy::camera::RenderTarget;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use crate::AppState;
use crate::features::kori::{Shelf, tuning};
use crate::menu::{self, INK, SOFT_INK};

/// Where the inspect stage stands, far under the menu's backdrop.
const STAGE: Vec3 = Vec3::new(0.0, -800.0, 0.0);
const VIEW_PX: u32 = 256;

#[derive(Component)]
pub struct ShelfPanel;

#[derive(Component)]
struct Blurb;

/// The figure on the stage.
#[derive(Component)]
struct OnStage;

#[derive(Component)]
struct StageCamera;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::Menu), spawn_panel)
        .add_systems(Update, spin.run_if(in_state(AppState::Menu)));
}

/// Show or hide the page.
pub fn toggle(panel: &mut Query<&mut Node, With<ShelfPanel>>) {
    for mut node in panel.iter_mut() {
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// Pure: a prize's row: its name and count, or "???" until you have one.
pub fn row_text(name: &str, count: u32) -> String {
    if count == 0 {
        "???  (not won yet)".to_string()
    } else {
        format!("{name}  ×{count}")
    }
}

fn spawn_panel(mut commands: Commands, mut images: ResMut<Assets<Image>>, shelf: Res<Shelf>) {
    let t = tuning();
    let view = images.add(Image::new_target_texture(
        VIEW_PX,
        VIEW_PX,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    commands.spawn((
        Camera3d::default(),
        Camera {
            order: -5,
            is_active: false,
            clear_color: ClearColorConfig::Custom(crate::theme::CARD),
            ..default()
        },
        RenderTarget::Image(view.clone().into()),
        Transform::from_translation(STAGE + Vec3::new(0.0, 0.1, 0.32))
            .looking_at(STAGE + Vec3::Y * 0.07, Vec3::Y),
        StageCamera,
        DespawnOnExit(AppState::Menu),
    ));
    commands.spawn((
        PointLight {
            intensity: 40_000.0,
            range: 5.0,
            ..default()
        },
        Transform::from_translation(STAGE + Vec3::new(0.3, 0.5, 0.5)),
        DespawnOnExit(AppState::Menu),
    ));
    let owned: u32 = t.prizes.iter().map(|p| shelf.count(&p.name).min(1)).sum();
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                top: percent(50),
                width: px(620),
                max_width: percent(94),
                margin: UiRect::new(px(-310), px(0), px(-260), px(0)),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(22)),
                row_gap: px(8),
                border_radius: BorderRadius::all(px(24)),
                border: UiRect::all(px(2)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(crate::theme::CARD),
            BorderColor::all(crate::theme::LINE),
            GlobalZIndex(20),
            ShelfPanel,
            DespawnOnExit(AppState::Menu),
        ))
        .with_children(|ui| {
            ui.spawn((
                Text::new("Kori prizes"),
                TextFont::from_font_size(28.0),
                TextColor(INK),
                crate::assets::DisplayText,
            ));
            menu::text(
                ui,
                &format!(
                    "{owned} of {} found. Win them at the claw machine by the gambling table; the . key holds one in game.",
                    t.prizes.len()
                ),
                15.0,
                SOFT_INK,
            );
            ui.spawn(Node {
                column_gap: px(16),
                ..default()
            })
            .with_children(|row| {
                row.spawn(Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(6),
                    flex_grow: 1.0,
                    ..default()
                })
                .with_children(|list| {
                    for (i, p) in t.prizes.iter().enumerate() {
                        let n = shelf.count(&p.name);
                        list.spawn((
                            Text::new(row_text(&p.name, n)),
                            TextFont::from_font_size(18.0),
                            TextColor(if n > 0 { INK } else { SOFT_INK }),
                        ))
                        .observe(move |_: On<PointerClick>, mut commands: Commands, shelf: Res<Shelf>,
                                       assets: Res<AssetServer>,
                                       staged: Query<Entity, With<OnStage>>,
                                       mut cams: Query<&mut Camera, With<StageCamera>>,
                                       mut blurb: Query<&mut Text, With<Blurb>>| {
                            let t = tuning();
                            let Some(p) = t.prizes.get(i) else { return };
                            if shelf.count(&p.name) == 0 {
                                return;
                            }
                            for e in &staged {
                                commands.entity(e).despawn();
                            }
                            commands.spawn((
                                OnStage,
                                Transform::from_translation(STAGE),
                                Visibility::default(),
                                WorldAssetRoot(assets.load(
                                    GltfAssetLabel::Scene(0).from_asset(p.model.clone()),
                                )),
                                DespawnOnExit(AppState::Menu),
                            ));
                            for mut c in &mut cams {
                                c.is_active = true;
                            }
                            for mut b in &mut blurb {
                                b.0 = format!("{}: {}", p.name, p.blurb);
                            }
                        });
                    }
                });
                row.spawn((
                    Node {
                        width: px(VIEW_PX as f32),
                        height: px(VIEW_PX as f32),
                        border_radius: BorderRadius::all(px(16)),
                        ..default()
                    },
                    ImageNode::new(view),
                ));
            });
            ui.spawn((
                Text::new("Click a prize you've won to inspect it."),
                TextFont::from_font_size(15.0),
                TextColor(SOFT_INK),
                Blurb,
            ));
        });
}

fn spin(time: Res<Time>, mut staged: Query<&mut Transform, With<OnStage>>) {
    for mut tf in &mut staged {
        tf.rotation = Quat::from_rotation_y(time.elapsed_secs() * 0.8);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unwon_prizes_stay_secret() {
        assert!(!super::row_text("Golden Log", 0).contains("Golden"));
        assert_eq!(super::row_text("Golden Log", 2), "Golden Log  ×2");
    }
}
