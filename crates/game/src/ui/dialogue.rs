//! The dialogue card (the quest system): talking to someone with a marker
//! over their head brings up a cardboard card at the bottom of the screen
//! with their name, what they say, and what you can say back. Click a reply
//! (or press its number, or the pad's A on it); Esc walks away. Saying yes to
//! a quest sends it to the server (`quest_givers::say`).
//!
//! The talk itself (who says what, what a reply does) is the story crate's
//! [`Talking`]; this only draws it.

use bevy::prelude::*;
use bevy::ui_widgets::{Activate, Button, ButtonPlugin};

use super::pad_nav::PadFocusable;
use super::{Overlay, Regular, body, head};
use crate::AppState;
use crate::features::fps::HandsBusy;
use crate::features::quest_givers::{Talking, View, end_talk, say};
use crate::features::quests::DIALOGUE;
use crate::theme::kit::{CARD_100, CARD_300, CARD_500, INK, TAPE_200, size};

#[derive(Component)]
struct Card;

/// The reply a button says.
#[derive(Component, Clone, Copy)]
struct Reply(usize);

pub fn plugin(app: &mut App) {
    if !app.is_plugin_added::<ButtonPlugin>() {
        app.add_plugins(ButtonPlugin);
    }
    app.add_systems(
        Update,
        (keys, draw).chain().run_if(in_state(AppState::InGame)),
    )
    .add_systems(
        PostUpdate,
        |talking: Res<Talking>,
         mut o: ResMut<Overlay>,
         mut ctx: ResMut<crate::input::InputContexts>| {
            let up = talking.0.is_some();
            crate::input::switch(&mut ctx, "dialogue", up);
            if o.talk != up {
                o.talk = up;
            }
        },
    );
}

const NUMBERS: [KeyCode; 4] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
];

/// Number keys reply, Esc walks away.
fn keys(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut talking: ResMut<Talking>,
    mut busy: ResMut<HandsBusy>,
) {
    if talking.0.is_none() {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        end_talk(&mut talking, &mut busy);
    } else if let Some(i) = NUMBERS.iter().position(|k| keys.just_pressed(*k)) {
        say(&mut commands, &mut talking, &mut busy, i);
    }
}

/// The card for the talk as it stands (rebuilt only when it changes).
fn draw(
    mut commands: Commands,
    talking: Res<Talking>,
    mut shown: Local<Option<View>>,
    card: Query<Entity, With<Card>>,
) {
    let view = talking.0.as_ref().map(|c| c.view(&DIALOGUE.get()));
    if *shown == view {
        return;
    }
    for e in &card {
        commands.entity(e).despawn();
    }
    if let Some(v) = &view {
        spawn_card(&mut commands, v);
    }
    *shown = view;
}

/// The line as shown: keys named for your device, words swapped about.
fn line_text(line: &str) -> String {
    crate::input::fill(&crate::features::random_dialogue::say(line))
}

fn spawn_card(commands: &mut Commands, v: &View) {
    let card = commands
        .spawn((
            Card,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                bottom: px(110),
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(30),
            DespawnOnExit(AppState::InGame),
        ))
        .id();
    let inner = commands
        .spawn((
            Node {
                width: px(640),
                max_width: percent(92),
                flex_direction: FlexDirection::Column,
                padding: UiRect::axes(px(26), px(20)),
                row_gap: px(10),
                border_radius: BorderRadius::all(px(4)),
                ..default()
            },
            // Cardboard on its shade layer, held on by tape, like the quest card.
            BackgroundColor(CARD_100),
            BoxShadow::new(CARD_500, px(5), px(6), px(0), px(0)),
            UiTransform::from_rotation(Rot2::degrees(0.4)),
            children![
                (
                    Node {
                        position_type: PositionType::Absolute,
                        top: px(-10),
                        left: px(40),
                        width: px(90),
                        height: px(22),
                        ..default()
                    },
                    BackgroundColor(TAPE_200.with_alpha(0.9)),
                    UiTransform::from_rotation(Rot2::degrees(-3.0)),
                ),
                (Text::new(v.name.clone()), head(size::H2, INK)),
                (
                    Text::new(line_text(&v.line)),
                    body(size::BODY_REG, INK),
                    Regular,
                ),
            ],
        ))
        .id();
    commands.entity(card).add_child(inner);
    for (i, text) in v.choices.iter().enumerate() {
        let reply = commands
            .spawn((
                Reply(i),
                Button,
                PadFocusable,
                Node {
                    padding: UiRect::axes(px(14), px(8)),
                    border_radius: BorderRadius::all(px(4)),
                    ..default()
                },
                BackgroundColor(CARD_300),
                children![(
                    Text::new(format!("{}  {}", i + 1, line_text(text))),
                    body(size::BODY, INK),
                )],
            ))
            .observe(
                |ev: On<Activate>,
                 replies: Query<&Reply>,
                 mut commands: Commands,
                 mut talking: ResMut<Talking>,
                 mut busy: ResMut<HandsBusy>| {
                    let Ok(r) = replies.get(ev.entity) else {
                        return;
                    };
                    say(&mut commands, &mut talking, &mut busy, r.0);
                },
            )
            .id();
        commands.entity(inner).add_child(reply);
    }
    let hint = commands
        .spawn((
            Node {
                align_items: AlignItems::Center,
                column_gap: px(8),
                margin: UiRect::top(px(4)),
                ..default()
            },
            crate::input::KeyRow::new(
                crate::input::Hint("{back} walk away".into()),
                crate::input::KeyStyle::Card,
            ),
        ))
        .id();
    commands.entity(inner).add_child(hint);
}
