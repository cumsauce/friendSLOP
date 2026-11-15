//! The title screen's layers (settings, creator, "Whose fault is it", Kori
//! prizes) take turns and B / Esc closes one at a time, top-most first. Each
//! panel also has a Back button the pad can focus.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::AppState;
use crate::creator::CreatorOpen;
use crate::fault::{self, FaultPanel};
use crate::kori_shelf::{self, ShelfPanel};
use crate::menu::JoinPanel;
use crate::ui::settings_panel::SettingsPanel;

/// A panel on top of the title card.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Settings,
    Creator,
    Fault,
    Kori,
    /// The join card.
    Join,
}

/// What B closes: the top-most open layer (settings float over the rest).
pub fn back_target(open: &[Layer]) -> Option<Layer> {
    [
        Layer::Settings,
        Layer::Creator,
        Layer::Fault,
        Layer::Kori,
        Layer::Join,
    ]
    .into_iter()
    .find(|l| open.contains(l))
}

/// The layers to close so that only the one that opened last stays up.
pub fn crowded_out(open: &[Layer], before: &[Layer]) -> Vec<Layer> {
    let Some(fresh) = open.iter().find(|l| !before.contains(l)) else {
        return Vec::new();
    };
    open.iter().copied().filter(|l| l != fresh).collect()
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (exclusive, back).chain().run_if(in_state(AppState::Menu)),
    );
}

#[derive(SystemParam)]
pub(crate) struct Layers<'w, 's> {
    settings: Query<'w, 's, &'static mut Node, With<SettingsPanel>>,
    fault: Query<'w, 's, &'static mut Node, (With<FaultPanel>, Without<SettingsPanel>)>,
    kori: Query<
        'w,
        's,
        &'static mut Node,
        (
            With<ShelfPanel>,
            Without<SettingsPanel>,
            Without<FaultPanel>,
        ),
    >,
    join: Query<
        'w,
        's,
        &'static mut Node,
        (
            With<JoinPanel>,
            Without<SettingsPanel>,
            Without<FaultPanel>,
            Without<ShelfPanel>,
        ),
    >,
    creator: ResMut<'w, CreatorOpen>,
}

impl Layers<'_, '_> {
    fn open(&self) -> Vec<Layer> {
        let shown = |n: &Node| n.display != Display::None;
        let mut open = Vec::new();
        if self.settings.iter().any(shown) {
            open.push(Layer::Settings);
        }
        if self.creator.0 {
            open.push(Layer::Creator);
        }
        if self.fault.iter().any(shown) {
            open.push(Layer::Fault);
        }
        if self.kori.iter().any(shown) {
            open.push(Layer::Kori);
        }
        if self.join.iter().any(shown) {
            open.push(Layer::Join);
        }
        open
    }

    fn close(&mut self, layer: Layer) {
        match layer {
            Layer::Settings => self
                .settings
                .iter_mut()
                .for_each(|mut n| n.display = Display::None),
            Layer::Creator => self.creator.0 = false,
            Layer::Fault => fault::close(self.fault.iter_mut()),
            Layer::Kori => kori_shelf::close(self.kori.iter_mut()),
            Layer::Join => self
                .join
                .iter_mut()
                .for_each(|mut n| n.display = Display::None),
        }
    }
}

fn exclusive(mut layers: Layers, mut before: Local<Vec<Layer>>) {
    let open = layers.open();
    for layer in crowded_out(&open, &before) {
        layers.close(layer);
    }
    *before = layers.open();
}

pub(crate) fn back(
    keys: Res<ButtonInput<KeyCode>>,
    picker: Query<(), With<crate::menu::HexPicker>>,
    mut layers: Layers,
) {
    // The join-code picker answers its own B.
    if !keys.just_pressed(KeyCode::Escape) || !picker.is_empty() {
        return;
    }
    if let Some(layer) = back_target(&layers.open()) {
        layers.close(layer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Layer::*;

    #[test]
    fn back_closes_one_layer_top_first() {
        assert_eq!(back_target(&[]), None);
        assert_eq!(back_target(&[Kori, Fault]), Some(Fault));
        assert_eq!(back_target(&[Fault, Settings, Creator]), Some(Settings));
        assert_eq!(back_target(&[Join]), Some(Join));
    }

    #[test]
    fn the_layer_that_opened_last_wins() {
        assert!(crowded_out(&[Fault], &[Fault]).is_empty());
        assert_eq!(crowded_out(&[Fault, Kori], &[Fault]), vec![Fault]);
        assert_eq!(crowded_out(&[Settings, Kori], &[Kori]), vec![Kori]);
        // Closing one leaves the rest alone.
        assert!(crowded_out(&[Kori], &[Kori, Fault]).is_empty());
    }

    /// A title screen with the four layers spawned shut.
    fn title() -> (App, Entity, Entity, Entity) {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<AppState>()
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<CreatorOpen>()
            .add_plugins(plugin);
        let shut = || Node {
            display: Display::None,
            ..default()
        };
        let settings = app.world_mut().spawn((SettingsPanel, shut())).id();
        let fault = app.world_mut().spawn((FaultPanel, shut())).id();
        let kori = app.world_mut().spawn((ShelfPanel, shut())).id();
        app.update();
        (app, settings, fault, kori)
    }

    fn shown(app: &App, e: Entity) -> bool {
        matches!(app.world().get::<Node>(e), Some(n) if n.display != Display::None)
    }

    fn open(app: &mut App, e: Entity) {
        if let Some(mut n) = app.world_mut().get_mut::<Node>(e) {
            n.display = Display::Flex;
        }
        app.update();
    }

    #[test]
    fn esc_closes_the_join_card_and_it_crowds_out_the_others() {
        let (mut app, settings, ..) = title();
        let card = app
            .world_mut()
            .spawn((
                JoinPanel,
                Node {
                    display: Display::None,
                    ..default()
                },
            ))
            .id();
        app.update();
        open(&mut app, card);
        assert!(shown(&app, card));
        open(&mut app, settings);
        assert!(shown(&app, settings) && !shown(&app, card));
        open(&mut app, card);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(!shown(&app, card));
    }

    #[test]
    fn opening_a_panel_closes_the_others_and_esc_closes_it() {
        let (mut app, settings, fault, kori) = title();
        open(&mut app, fault);
        assert!(shown(&app, fault));
        open(&mut app, kori);
        assert!(shown(&app, kori) && !shown(&app, fault));
        open(&mut app, settings);
        assert!(shown(&app, settings) && !shown(&app, kori));
        // B closes the one that is up, and a second B has nothing left to close.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(!shown(&app, settings));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.update();
    }
}
