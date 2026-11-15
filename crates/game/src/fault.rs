//! "Whose fault is it" (#149, catherine): a menu screen crediting every idea
//! in the game to the friend who asked for it, newest first.
//!
//! The list is the "Ideas so far" section of GAME_DESIGN.md, built into the
//! game, so it keeps itself up to date as Claude records each idea there.

use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::AppState;
use crate::creator::small_button;
use crate::theme::kit::{self as t, size};
use crate::ui::kit::{self, Look};
use crate::ui::{Regular, body};

const DESIGN: &str = include_str!("../../../GAME_DESIGN.md");

/// One idea: who (whose fault), which issue, and what they said.
#[derive(Clone, Debug, PartialEq)]
pub struct Fault {
    pub who: String,
    pub issue: u32,
    pub what: String,
}

/// Every entry under "## Ideas so far" (`- name (#N...): text`, wrapped
/// lines joined), newest issue first.
pub fn faults(design: &str) -> Vec<Fault> {
    let Some(start) = design.find("## Ideas so far") else {
        return Vec::new();
    };
    let body = &design[start..];
    let body = body[3..].find("\n## ").map_or(body, |end| &body[..end + 3]);
    let mut items: Vec<String> = Vec::new();
    for line in body.lines().skip(1) {
        if let Some(rest) = line.strip_prefix("- ") {
            items.push(rest.trim().to_string());
        } else if line.starts_with("  ")
            && let Some(last) = items.last_mut()
        {
            last.push(' ');
            last.push_str(line.trim());
        }
    }
    let mut out: Vec<Fault> = items
        .iter()
        .filter_map(|it| {
            // `name (#N...): text`, or unnamed `(#N): text` / `#N: text`.
            let open = it.find("(#").or_else(|| it.starts_with('#').then_some(0))?;
            let digits = &it[open + it[open..].find('#')? + 1..];
            let issue = digits
                .split(|c: char| !c.is_ascii_digit())
                .next()?
                .parse()
                .ok()?;
            let colon = open + it[open..].find(':')?;
            let what = it[colon + 1..].trim().to_string();
            let who = it[..open].trim();
            let who = if who.is_empty() {
                "a mystery friend"
            } else {
                who
            };
            Some(Fault {
                who: who.to_string(),
                issue,
                what,
            })
        })
        .collect();
    out.sort_by_key(|f| std::cmp::Reverse(f.issue));
    out
}

/// The screen; toggled by the menu's "Whose fault is it" button.
#[derive(Component)]
pub struct FaultPanel;

#[derive(Component)]
struct FaultList;

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::Menu), spawn_panel)
        .add_systems(Update, scroll.run_if(in_state(AppState::Menu)));
}

/// Show or hide the screen.
pub fn toggle(panel: &mut Query<&mut Node, With<FaultPanel>>) {
    for mut node in panel.iter_mut() {
        node.display = if node.display == Display::None {
            Display::Flex
        } else {
            Display::None
        };
    }
}

/// Hide the screen.
pub fn close<'a>(panel: impl Iterator<Item = Mut<'a, Node>>) {
    for mut node in panel {
        node.display = Display::None;
    }
}

fn spawn_panel(mut commands: Commands) {
    let list = faults(DESIGN);
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: percent(50),
                top: percent(50),
                width: px(620),
                max_width: percent(94),
                max_height: percent(80),
                margin: UiRect::new(px(-310), px(0), px(-280), px(0)),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(22)),
                row_gap: px(8),
                border_radius: BorderRadius::all(px(t::R_PANEL)),
                border: UiRect::all(px(2)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(t::SOOT_800),
            BorderColor::all(t::SOOT_500),
            GlobalZIndex(20),
            FaultPanel,
            DespawnOnExit(AppState::Menu),
        ))
        .with_children(|ui| {
            ui.spawn(kit::display("Whose fault is it", size::H1, t::TEXT));
            ui.spawn((
                Text::new(format!(
                    "{} ideas, and the friends to blame (scroll)",
                    list.len()
                )),
                body(size::LABEL, t::TEXT_DIM),
            ));
            ui.spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(10),
                    overflow: Overflow::scroll_y(),
                    flex_grow: 1.0,
                    min_height: px(0),
                    ..default()
                },
                ScrollPosition::default(),
                FaultList,
            ))
            .with_children(|l| {
                for f in &list {
                    l.spawn(Node {
                        flex_direction: FlexDirection::Column,
                        flex_shrink: 0.0,
                        ..default()
                    })
                    .with_children(|e| {
                        e.spawn((
                            Text::new(format!("#{}: {}'s fault", f.issue, f.who)),
                            body(size::BODY, t::TEXT),
                        ));
                        e.spawn((
                            Text::new(f.what.clone()),
                            body(size::LABEL, t::TEXT_DIM),
                            Regular,
                        ));
                    });
                }
            });
            small_button(ui, "Back", Look::Plain, 150.0).observe(
                |_: On<PointerClick>, mut panel: Query<&mut Node, With<FaultPanel>>| {
                    close(panel.iter_mut());
                },
            );
        });
}

/// How fast the pad's stick or D-pad scrolls the list (px per second).
const PAD_SCROLL: f32 = 600.0;

/// Mouse wheel, stick or D-pad scrolls the list while the screen is open.
fn scroll(
    mut wheel: MessageReader<MouseWheel>,
    time: Res<Time>,
    pads: Query<&Gamepad>,
    panel: Query<&Node, With<FaultPanel>>,
    mut lists: Query<&mut ScrollPosition, With<FaultList>>,
) {
    let open = panel.iter().any(|n| n.display != Display::None);
    let held = pads
        .iter()
        .map(|p| crate::ui::pad_nav::held_dir(p).y)
        .find(|y| *y != 0)
        .unwrap_or(0);
    if open && held != 0 {
        for mut pos in &mut lists {
            pos.y = (pos.y + held as f32 * PAD_SCROLL * time.delta_secs()).max(0.0);
        }
    }
    for ev in wheel.read() {
        if !open {
            continue;
        }
        let dy = match ev.unit {
            MouseScrollUnit::Line => ev.y * 40.0,
            MouseScrollUnit::Pixel => ev.y,
        };
        for mut pos in &mut lists {
            pos.y = (pos.y - dy).max(0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_recorded_idea_is_someones_fault() {
        let f = faults(DESIGN);
        assert!(f.len() >= 50, "only {} ideas parsed", f.len());
        assert!(f.windows(2).all(|w| w[0].issue >= w[1].issue));
        assert!(f.iter().any(|x| x.who == "corvy" && x.issue == 2));
        assert!(f.iter().all(|x| !x.who.is_empty() && !x.what.is_empty()));
    }
}
