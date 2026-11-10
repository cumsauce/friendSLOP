//! EventFeed, left side under the quest card: at most three toasts, repeats
//! within 2 s merge into one "×N", each stays 4 s then fades over 300 ms, and
//! extra ones wait their turn in a bounded queue.
//!
//! It is fed by everything that used to flash a line: [`Toast::show`] and the
//! lines features put in [`HudSlot::Toasts`] (which is itself hidden now).
//! A slot line updates its own toast in place instead of stacking a new one.

use std::collections::VecDeque;

use bevy::prelude::*;

use super::{HudRoot, body};
use crate::AppState;
use crate::hud::{HudSlot, InSlot, Toast};
use crate::theme::indigo::{BORDER, CREAM, EDGE, INK};

pub const MAX_VISIBLE: usize = 3;
pub const MERGE_SECS: f32 = 2.0;
pub const HOLD_SECS: f32 = 4.0;
pub const FADE_SECS: f32 = 0.3;
/// Waiting toasts beyond this drop the oldest.
pub const MAX_QUEUE: usize = 12;
/// Where the feed starts (left, top), and how wide a toast is.
const AT: Vec2 = Vec2::new(EDGE, 260.0);
const W: f32 = 386.0;

#[derive(Clone, Debug, PartialEq)]
enum Key {
    Text(String),
    Line(Entity),
}

#[derive(Clone, Debug)]
struct Entry {
    key: Key,
    text: String,
    count: u32,
    /// Seconds since it was shown or last changed.
    age: f32,
    /// Seconds since the same thing was last pushed (for merging).
    since_push: f32,
}

/// The toasts on screen and the ones waiting.
#[derive(Resource, Default, Debug)]
pub struct EventFeed {
    shown: Vec<Entry>,
    queue: VecDeque<Entry>,
}

impl EventFeed {
    /// A new event. The same text again within [`MERGE_SECS`] counts up instead.
    pub fn push(&mut self, text: impl Into<String>) {
        let text = text.into();
        if text.trim().is_empty() {
            return;
        }
        let key = Key::Text(text.clone());
        if let Some(e) = self
            .shown
            .iter_mut()
            .chain(self.queue.iter_mut())
            .find(|e| e.key == key && e.since_push < MERGE_SECS)
        {
            e.count += 1;
            e.since_push = 0.0;
            e.age = 0.0;
            return;
        }
        self.enqueue(key, text);
    }

    /// A feature's slot line changed: refresh its toast, or show a new one.
    fn set_line(&mut self, line: Entity, text: &str) {
        if text.trim().is_empty() {
            return; // let it fade on its own
        }
        let key = Key::Line(line);
        if let Some(e) = self
            .shown
            .iter_mut()
            .chain(self.queue.iter_mut())
            .find(|e| e.key == key)
        {
            if e.text != text {
                e.text = text.to_string();
                e.age = 0.0;
            }
            return;
        }
        self.enqueue(key, text.to_string());
    }

    fn enqueue(&mut self, key: Key, text: String) {
        self.queue.push_back(Entry {
            key,
            text,
            count: 1,
            age: 0.0,
            since_push: 0.0,
        });
        while self.queue.len() > MAX_QUEUE {
            self.queue.pop_front();
        }
        self.fill();
    }

    fn fill(&mut self) {
        while self.shown.len() < MAX_VISIBLE {
            let Some(mut e) = self.queue.pop_front() else {
                break;
            };
            e.age = 0.0;
            self.shown.push(e);
        }
    }

    /// Age the shown toasts, drop the faded ones, move waiting ones up.
    pub fn tick(&mut self, dt: f32) {
        for e in &mut self.shown {
            e.age += dt;
            e.since_push += dt;
        }
        for e in &mut self.queue {
            e.since_push += dt;
        }
        self.shown.retain(|e| e.age < HOLD_SECS + FADE_SECS);
        self.fill();
    }

    /// What's on screen, oldest first: (text with any ×N, opacity).
    pub fn visible(&self) -> impl Iterator<Item = (String, f32)> + '_ {
        self.shown.iter().map(|e| {
            let text = if e.count > 1 {
                format!("{}  {}", e.text, super::times(e.count))
            } else {
                e.text.clone()
            };
            (text, alpha(e.age))
        })
    }

    pub fn waiting(&self) -> usize {
        self.queue.len()
    }
}

/// Opacity `age` seconds after a toast showed (or last changed).
pub fn alpha(age: f32) -> f32 {
    (1.0 - (age - HOLD_SECS) / FADE_SECS).clamp(0.0, 1.0)
}

#[derive(Component)]
struct Feed;

#[derive(Component, Clone, Copy)]
struct ToastCard(usize);

pub fn plugin(app: &mut App) {
    app.init_resource::<EventFeed>()
        .add_systems(OnExit(AppState::InGame), |mut f: ResMut<EventFeed>| {
            *f = EventFeed::default();
        })
        .add_systems(
            Update,
            (spawn.run_if(not(any_with_component::<Feed>)), collect, draw)
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
}

fn spawn(mut commands: Commands, root: Query<Entity, With<HudRoot>>) {
    let Ok(root) = root.single() else { return };
    let b = 2.0;
    commands.entity(root).with_children(|c| {
        c.spawn((
            Feed,
            Node {
                position_type: PositionType::Absolute,
                left: px(AT.x),
                top: px(AT.y),
                width: px(W),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                ..default()
            },
        ))
        .with_children(|f| {
            for i in 0..MAX_VISIBLE {
                f.spawn((
                    ToastCard(i),
                    Node {
                        width: px(W),
                        min_height: px(55),
                        padding: UiRect {
                            left: px(12.0 - b),
                            right: px(12.0 - b),
                            top: px(14.0 - b),
                            bottom: px(12.0 - b),
                        },
                        border: UiRect::all(px(b)),
                        border_radius: BorderRadius::all(px(5)),
                        display: Display::None,
                        ..default()
                    },
                    BackgroundColor(INK),
                    BorderColor::all(BORDER),
                    children![(Text::new(""), body(16.0, CREAM))],
                ));
            }
        });
    });
}

/// Feed the feed from [`Toast`] and the toast slot's lines.
fn collect(
    time: Res<Time>,
    toast: Option<Res<Toast>>,
    mut last_secs: Local<f32>,
    mut feed: ResMut<EventFeed>,
    lines: Query<(Entity, &Text, &InSlot), Changed<Text>>,
) {
    if let Some(toast) = toast {
        // `show` resets the timer, so a jump up is a new toast (even the same text).
        if toast.secs_left > *last_secs + 1e-3 {
            for line in toast.text.lines() {
                feed.push(line.trim());
            }
        }
        *last_secs = toast.secs_left;
    }
    for (e, text, slot) in &lines {
        if slot.0 == HudSlot::Toasts {
            feed.set_line(e, &text.0);
        }
    }
    feed.tick(time.delta_secs());
}

fn draw(
    feed: Res<EventFeed>,
    mut cards: Query<(
        &ToastCard,
        &Children,
        &mut Node,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut texts: Query<(&mut Text, &mut TextColor)>,
) {
    let shown: Vec<(String, f32)> = feed.visible().collect();
    for (card, kids, mut node, mut bg, mut border) in &mut cards {
        let item = shown.get(card.0);
        let d = if item.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if node.display != d {
            node.display = d;
        }
        let Some((want, a)) = item else { continue };
        bg.set_if_neq(BackgroundColor(INK.with_alpha(*a)));
        border.set_if_neq(BorderColor::all(BORDER.with_alpha(*a)));
        for kid in kids.iter() {
            if let Ok((mut text, mut color)) = texts.get_mut(kid) {
                if &text.0 != want {
                    text.0.clone_from(want);
                }
                color.set_if_neq(TextColor(CREAM.with_alpha(*a)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_at_a_time_repeats_merge_and_old_ones_fade() {
        let mut f = EventFeed::default();
        f.push("You pocket a crow egg");
        f.push("You pocket a crow egg");
        f.push("You pocket a crow egg");
        f.push("Moss farted, then pointed at YOU");
        f.push("ACTION FAILED: everyone is bees");
        f.push("a fourth thing");
        let shown: Vec<String> = f.visible().map(|(t, _)| t).collect();
        assert_eq!(shown.len(), MAX_VISIBLE);
        assert_eq!(shown[0], "You pocket a crow egg  ×3");
        assert_eq!(f.waiting(), 1);
        // Past the merge window the same text is a new toast.
        f.tick(MERGE_SECS + 0.1);
        f.push("Moss farted, then pointed at YOU");
        assert_eq!(f.waiting(), 2);
        // Hold, then fade, then gone: the queue moves up.
        f.tick(HOLD_SECS - MERGE_SECS - 0.1 + FADE_SECS / 2.0);
        assert!(f.visible().all(|(_, a)| a < 1.0 && a > 0.0));
        f.tick(FADE_SECS);
        let shown: Vec<String> = f.visible().map(|(t, _)| t).collect();
        assert_eq!(
            shown,
            vec!["a fourth thing", "Moss farted, then pointed at YOU"]
        );
        assert_eq!(alpha(0.0), 1.0);
        assert_eq!(alpha(HOLD_SECS + FADE_SECS), 0.0);
    }

    #[test]
    fn queue_is_bounded() {
        let mut f = EventFeed::default();
        for i in 0..50 {
            f.push(format!("event {i}"));
        }
        assert_eq!(f.waiting(), MAX_QUEUE);
    }
}
