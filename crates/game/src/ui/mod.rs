//! The "E · Indigo journal" HUD (Figma spec node 3:2918; UI polish, not a
//! friend idea). Quiet corners: the quest card top-left, the team strip
//! top-right with the danger lane under it, the event feed on the left, the
//! gas / mic / cat stack bottom-right, the journal hint bottom-left and the
//! context prompt under the aim point.
//!
//! Everything is authored at 1280x720 logical pixels and scaled uniformly
//! with the window ([`UiScale`]). Widgets only display state the features
//! own; they never change it. Lines features still drop into
//! [`crate::hud::slot`]s keep working around these widgets.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::AppState;
use crate::theme::indigo;

pub mod danger;
pub mod feed;
pub mod prompt;
pub mod quest;
pub mod status;
pub mod team;

/// The HUD's small white glyphs (`assets/ui/*.png`, exported from the Figma
/// kit), tinted per use. A missing file just draws nothing: every glyph has
/// text beside it that says the same thing.
#[derive(Resource, Clone, Default)]
pub struct Icons {
    pub mic: Handle<Image>,
    pub mic_slash: Handle<Image>,
    pub tear: Handle<Image>,
    pub gas: Handle<Image>,
    pub cat: Handle<Image>,
    pub warn: Handle<Image>,
    pub target: Handle<Image>,
    pub hand: Handle<Image>,
}

/// Text in the UI face (Nunito Bold) once it has loaded.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Body;

/// Text in the display face (Fredoka SemiBold, or Nunito ExtraBold when the
/// Fredoka file is missing) once it has loaded.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct Head;

/// UI-face text: `size` logical px, bold, in `color`.
pub fn body(size: f32, color: Color) -> impl Bundle {
    (
        TextFont {
            font_size: FontSize::Px(size),
            weight: FontWeight(700),
            ..default()
        },
        TextColor(color),
        Body,
    )
}

/// Display-face text: `size` logical px, semibold, in `color`.
pub fn head(size: f32, color: Color) -> impl Bundle {
    (
        TextFont {
            font_size: FontSize::Px(size),
            weight: FontWeight(600),
            ..default()
        },
        TextColor(color),
        Head,
    )
}

/// A node placed at Figma's `x, y` inside its card (minus the card's border,
/// since Bevy places absolute children inside it).
pub fn at(x: f32, y: f32, border: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: px(x - border),
        top: px(y - border),
        ..default()
    }
}

/// A `size` px glyph at `x, y` inside a card with a `border` px outline.
pub fn glyph(
    image: &Handle<Image>,
    size: f32,
    x: f32,
    y: f32,
    border: f32,
    color: Color,
) -> impl Bundle {
    (
        Node {
            width: px(size),
            height: px(size),
            ..at(x, y, border)
        },
        ImageNode::new(image.clone()).with_color(color),
    )
}

/// The common HUD card: solid fill, thin outline, rounded corners.
pub fn card(w: f32, h: f32, radius: f32, border: f32, fill: Color, line: Color) -> impl Bundle {
    (
        Node {
            width: px(w),
            height: px(h),
            border: UiRect::all(px(border)),
            border_radius: BorderRadius::all(px(radius)),
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(line),
    )
}

/// The HUD's root: fills the screen, passes clicks through.
#[derive(Component)]
pub struct HudRoot;

/// The display font (Fredoka). Missing file: [`Head`] text falls back to Nunito.
#[derive(Resource, Clone)]
pub struct HeadFont(pub Handle<Font>);

pub fn plugin(app: &mut App) {
    app.init_resource::<Icons>()
        .add_systems(Startup, load)
        .add_systems(Update, (scale_ui, apply_fonts))
        .add_systems(OnEnter(AppState::InGame), spawn_root)
        .add_plugins((
            quest::plugin,
            team::plugin,
            danger::plugin,
            feed::plugin,
            status::plugin,
            prompt::plugin,
        ));
}

fn load(mut commands: Commands, assets: Option<Res<AssetServer>>) {
    let Some(assets) = assets else { return };
    let png = |name: &str| assets.load(format!("ui/{name}.png"));
    commands.insert_resource(Icons {
        mic: png("mic"),
        mic_slash: png("mic_slash"),
        tear: png("tear"),
        gas: png("gas"),
        cat: png("cat"),
        warn: png("warn"),
        target: png("target"),
        hand: png("hand"),
    });
    commands.insert_resource(HeadFont(assets.load("fonts/Fredoka.ttf")));
}

/// Author at 1280x720, scale uniformly with the window. Never below 1:
/// screens under 960x600 (phones) are already shrunk to fit by
/// `graphics::ui_fit`, and shrinking twice would make the UI tiny again (#191).
fn scale_ui(window: Query<&Window, With<PrimaryWindow>>, mut scale: ResMut<UiScale>) {
    let Ok(window) = window.single() else { return };
    let want = indigo::ui_scale(window.width(), window.height()).max(1.0);
    if (scale.0 - want).abs() > 1e-3 {
        scale.0 = want;
    }
}

/// Give [`Body`] / [`Head`] text their faces once loaded. A missing Fredoka
/// falls back to Nunito at a heavier weight; a missing Nunito keeps Bevy's font.
fn apply_fonts(
    assets: Option<Res<AssetServer>>,
    ui: Option<Res<crate::assets::UiFont>>,
    display: Option<Res<HeadFont>>,
    mut texts: Query<(&mut TextFont, Has<Head>), Or<(With<Body>, With<Head>)>>,
    mut known: Local<(bool, bool)>,
) {
    let (Some(assets), Some(ui)) = (assets, ui) else {
        return;
    };
    let ui_ok = ui.ready(&assets);
    let head = display
        .as_ref()
        .filter(|h| assets.is_loaded_with_dependencies(&h.0));
    let head_settled = head.is_some()
        || display
            .as_ref()
            .is_none_or(|h| assets.load_state(&h.0).is_failed());
    // When a face finishes loading (or fails), revisit every text once.
    let full = (ui_ok, head_settled) != *known;
    *known = (ui_ok, head_settled);
    for (mut tf, is_head) in &mut texts {
        if !full && !tf.is_added() {
            continue;
        }
        let want = match (is_head, head) {
            (true, Some(h)) => Some((h.0.clone(), 600)),
            _ if ui_ok => Some((ui.0.clone(), if is_head { 800 } else { 700 })),
            _ => None,
        };
        let Some((font, weight)) = want else { continue };
        let font = FontSource::Handle(font);
        if tf.font != font || tf.weight.0 != weight {
            tf.font = font;
            tf.weight = FontWeight(weight);
        }
    }
}

fn spawn_root(mut commands: Commands) {
    commands.spawn((
        HudRoot,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        Pickable::IGNORE,
        DespawnOnExit(AppState::InGame),
    ));
}

/// `MM:SS`, clamped at zero (never negative).
pub fn mmss(secs: f32) -> String {
    let s = if secs.is_finite() {
        secs.max(0.0).ceil() as u32
    } else {
        0
    };
    format!("{:02}:{:02}", s / 60, s % 60)
}

/// A count for the HUD: `×N`, capped at `×99+`.
pub fn times(n: u32) -> String {
    if n > 99 {
        "×99+".to_string()
    } else {
        format!("×{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn countdowns_clamp_and_counts_cap() {
        assert_eq!(mmss(8.0), "00:08");
        assert_eq!(mmss(52.2), "00:53");
        assert_eq!(mmss(61.0), "01:01");
        assert_eq!(mmss(-3.0), "00:00");
        assert_eq!(mmss(f32::NAN), "00:00");
        assert_eq!(times(0), "×0");
        assert_eq!(times(99), "×99");
        assert_eq!(times(100), "×99+");
    }
}
