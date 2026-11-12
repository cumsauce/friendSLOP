//! The commune journal (Figma 3:1977; UI polish, not a friend idea): one
//! big lavender panel over a dim scrim, with tabs for the party, the bag,
//! skills, help and credits. Hold Tab to peek, J to keep it open, Esc to
//! close. While it's open the mouse is free so the tabs can be clicked.
//!
//! It only shows what features already publish: the per-player tables they
//! drop into [`HudSlot::Scoreboard`], the explanations in [`HudSlot::Help`],
//! public components for the party table, and `fault::faults` for credits.

use bevy::prelude::*;

use super::{Overlay, at, body, head};
use crate::AppState;
use crate::features::fps::FpsView;
use crate::features::gods_tears::Transformation;
use crate::features::organ_currency::OrganPurse;
use crate::features::skills::{Skill, Skills};
use crate::features::totems::Totem;
use crate::hud::{HudSlot, InSlot};
use crate::net::NetStatus;
use crate::player::{LocalPlayer, Player};
use crate::rules::morality::Morality;
use crate::theme::indigo::{BORDER, BUTTER, CREAM, INDIGO, INK, LAVENDER, PAPER};
use crate::voice::VoiceMuted;
use crate::voice_chat::{SpeakerId, VoiceActivity};

const B: f32 = 2.0;
const DESIGN: &str = include_str!("../../../../GAME_DESIGN.md");

/// The journal's tabs, in the order they're drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Party,
    Bag,
    Skills,
    Help,
    Credits,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Party, Tab::Bag, Tab::Skills, Tab::Help, Tab::Credits];

    pub fn name(self) -> &'static str {
        match self {
            Tab::Party => "Party",
            Tab::Bag => "Bag",
            Tab::Skills => "Skills",
            Tab::Help => "Help",
            Tab::Credits => "Credits",
        }
    }
}

/// Is the journal open, and on which tab. Tab held peeks; J or H keeps it.
#[derive(Resource, Default, Debug)]
pub struct Journal {
    pub kept: bool,
    pub tab: Tab,
    /// We freed the mouse on opening (so give it back on closing).
    freed: bool,
}

/// What the keys do to the journal this frame: (kept open, tab).
pub fn keys_to_journal(
    kept: bool,
    tab: Tab,
    j: bool,
    h: bool,
    esc: bool,
    tab_held: bool,
) -> (bool, Tab) {
    if esc && (kept || tab_held) {
        return (false, tab);
    }
    if h {
        // H jumps straight to help; H again on help closes it.
        let on_help = kept && tab == Tab::Help;
        return (!on_help, Tab::Help);
    }
    if j {
        return (!kept, if kept { tab } else { Tab::Party });
    }
    (kept, tab)
}

/// One row of the party table.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub name: String,
    pub tears: String,
    pub morality: String,
    pub organs: String,
    pub totems: String,
    pub talking: bool,
}

/// Everything a page shows, so it's rebuilt only when something changed.
#[derive(Clone, Debug, PartialEq, Default)]
struct Page {
    tab: Tab,
    status: String,
    rows: Vec<Row>,
    skills: Vec<(String, u32)>,
    mine: Vec<String>,
    tables: Vec<String>,
    help: Vec<String>,
    invite: String,
}

#[derive(Component)]
struct Root;
#[derive(Component)]
struct Content;
#[derive(Component, Clone, Copy)]
struct TabButton(Tab);

pub fn plugin(app: &mut App) {
    app.init_resource::<Journal>()
        .add_systems(OnExit(AppState::InGame), |mut j: ResMut<Journal>| {
            *j = Journal::default();
        })
        .add_systems(
            Update,
            (
                spawn.run_if(not(any_with_component::<Root>)),
                keys,
                show,
                fill,
                look_tabs,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        )
        // Published after everything ran, so Esc this frame closes the
        // journal without also leaving the game.
        .add_systems(
            PostUpdate,
            |j: Res<Journal>, keys: Res<ButtonInput<KeyCode>>, mut o: ResMut<Overlay>| {
                o.journal = j.kept || keys.pressed(KeyCode::Tab);
            },
        );
}

fn spawn(mut commands: Commands) {
    let tabs: Vec<Entity> = Tab::ALL
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            commands
                .spawn((
                    TabButton(t),
                    BackgroundColor(PAPER),
                    BorderColor::all(BORDER),
                    Node {
                        width: px(132),
                        height: px(42),
                        border: UiRect::all(px(B)),
                        border_radius: BorderRadius::all(px(12)),
                        ..at(26.0 + 144.0 * i as f32, 77.0, B)
                    },
                    children![(Text::new(t.name()), body(17.0, INK), at(11.0, 8.0, B))],
                ))
                .observe(move |_: On<PointerClick>, mut j: ResMut<Journal>| {
                    j.tab = t;
                    j.kept = true;
                })
                .id()
        })
        .collect();
    let panel = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(40),
                top: px(28),
                width: px(1200),
                height: px(664),
                border: UiRect::all(px(B)),
                border_radius: BorderRadius::all(px(24)),
                ..default()
            },
            BackgroundColor(LAVENDER),
            BorderColor::all(BORDER),
            children![
                (
                    Text::new("The commune journal"),
                    head(35.0, INK),
                    at(26.0, 18.0, B)
                ),
                (
                    Node {
                        width: px(148),
                        height: px(42),
                        border: UiRect::all(px(B)),
                        border_radius: BorderRadius::all(px(12)),
                        ..at(1018.0, 20.0, B)
                    },
                    BackgroundColor(PAPER),
                    BorderColor::all(BORDER),
                    children![(Text::new("Esc   close"), body(17.0, INK), at(11.0, 8.0, B))],
                ),
                (
                    Content,
                    Node {
                        width: px(1196),
                        height: px(520),
                        ..at(0.0, 134.0, B)
                    }
                ),
            ],
        ))
        .add_children(&tabs)
        .id();
    commands
        .spawn((
            Root,
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                display: Display::None,
                ..default()
            },
            BackgroundColor(INK.with_alpha(0.63)),
            GlobalZIndex(30),
            DespawnOnExit(AppState::InGame),
        ))
        .add_child(panel);
}

fn keys(keys: Res<ButtonInput<KeyCode>>, overlay: Res<Overlay>, mut journal: ResMut<Journal>) {
    let tab_held = keys.pressed(KeyCode::Tab);
    // Esc goes to the chapter intro first when it's up.
    let esc = keys.just_pressed(KeyCode::Escape) && !overlay.intro;
    let (kept, tab) = keys_to_journal(
        journal.kept,
        journal.tab,
        keys.just_pressed(KeyCode::KeyJ),
        keys.just_pressed(KeyCode::KeyH),
        esc,
        tab_held,
    );
    if journal.kept != kept {
        journal.kept = kept;
    }
    if journal.tab != tab {
        journal.tab = tab;
    }
    // Tab released after a peek: back to the party page next time.
    if !kept && !tab_held && journal.tab != Tab::Party {
        journal.tab = Tab::Party;
    }
}

fn show(
    keys: Res<ButtonInput<KeyCode>>,
    mut journal: ResMut<Journal>,
    mut view: ResMut<FpsView>,
    mut root: Query<&mut Node, With<Root>>,
) {
    let open = journal.kept || keys.pressed(KeyCode::Tab);
    // Free the mouse so the tabs can be clicked; give it back after.
    if open && view.first_person && !view.released {
        view.released = true;
        journal.freed = true;
    } else if !open && journal.freed {
        journal.freed = false;
        view.released = false;
    }
    let want = if open { Display::Flex } else { Display::None };
    for mut n in &mut root {
        if n.display != want {
            n.display = want;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn fill(
    mut commands: Commands,
    journal: Res<Journal>,
    keys: Res<ButtonInput<KeyCode>>,
    me: Option<Res<LocalPlayer>>,
    net: Option<Res<NetStatus>>,
    muted: Option<Res<VoiceMuted>>,
    activity: Option<Res<VoiceActivity>>,
    players: Query<(
        &Player,
        Option<&Transformation>,
        Option<&Morality>,
        Option<&OrganPurse>,
        Option<&Skills>,
    )>,
    totems: Query<&Totem>,
    lines: Query<(&Text, &InSlot)>,
    content: Query<Entity, With<Content>>,
    mut last: Local<Option<Page>>,
) {
    if !(journal.kept || keys.pressed(KeyCode::Tab)) {
        return;
    }
    let Ok(content) = content.single() else {
        return;
    };
    let me = me.map(|m| m.0);
    let talking = |id: u64| {
        activity.as_ref().is_some_and(|a| {
            if Some(id) == me {
                a.transmitting
            } else {
                a.heard.contains(&SpeakerId(id))
            }
        })
    };
    let mut ps: Vec<_> = players.iter().collect();
    ps.sort_by_key(|(p, ..)| (Some(p.id) != me, p.id));
    let rows: Vec<Row> = ps
        .iter()
        .take(6)
        .map(|(p, tears, moral, purse, _)| {
            let held = totems.iter().filter(|t| t.holder == Some(p.id)).count();
            let t = tears.copied().unwrap_or(Transformation(0));
            Row {
                name: super::team::name_for(p.id, Some(p.id) == me),
                tears: format!("{} · {}", t.0, t.label()),
                morality: moral.map_or("-".into(), |m| m.label().to_string()),
                organs: format!("${}", purse.map_or(0, |o| o.total_value())),
                totems: if held == 0 {
                    "-".into()
                } else {
                    held.to_string()
                },
                talking: talking(p.id),
            }
        })
        .collect();
    let speaker = ps
        .iter()
        .find(|(p, ..)| talking(p.id))
        .map(|(p, ..)| super::team::name_for(p.id, Some(p.id) == me));
    let mic = if muted.is_some_and(|m| m.0) {
        "M  muted"
    } else {
        "M  mic on"
    };
    let mut status = format!("{} friends     ·     {mic}", ps.len());
    if let Some(s) = speaker {
        status.push_str(&format!(
            "     ·     {s} {}",
            if s == "You" {
                "are talking"
            } else {
                "is talking"
            }
        ));
    }
    let my_skills = ps
        .iter()
        .find(|(p, ..)| Some(p.id) == me)
        .and_then(|(.., s)| s.copied())
        .unwrap_or_default();
    let skills = Skill::ALL
        .iter()
        .map(|&s| (s.name().to_string(), my_skills.level(s)))
        .collect();
    let slot_lines = |slot: HudSlot| -> Vec<String> {
        let mut v: Vec<String> = lines
            .iter()
            .filter(|(t, s)| s.0 == slot && !t.0.trim().is_empty())
            .map(|(t, _)| t.0.clone())
            .collect();
        v.sort();
        v
    };
    let tables = slot_lines(HudSlot::Scoreboard);
    let mine = me
        .map(|me| {
            tables
                .iter()
                .flat_map(|t| crate::hud::own_lines(t, me))
                .map(|l| crate::hud::clip(&l))
                .collect()
        })
        .unwrap_or_default();
    let mut help = vec![crate::hud::help_text()];
    help.extend(slot_lines(HudSlot::Help));
    let code = net
        .and_then(|n| n.join_code.clone())
        .map(|c| crate::hud::short_code(&c));
    let invite = match code {
        Some(c) => format!("INVITE   {c}    ·    {} friends", ps.len()),
        None => format!("{} friends", ps.len()),
    };
    let page = Page {
        tab: journal.tab,
        status,
        rows,
        skills,
        mine,
        tables,
        help,
        invite,
    };
    if last.as_ref() == Some(&page) {
        return;
    }
    commands.entity(content).despawn_children();
    commands.entity(content).with_children(|c| match page.tab {
        Tab::Party => party(c, &page),
        Tab::Bag => text_page(c, "Everything everyone carries", &page.tables),
        Tab::Skills => skills_page(c, &page),
        Tab::Help => text_page(c, "How do I…", &page.help),
        Tab::Credits => credits(c),
    });
    *last = Some(page);
}

fn look_tabs(
    journal: Res<Journal>,
    mut tabs: Query<(
        &TabButton,
        &Children,
        &mut BackgroundColor,
        &mut BorderColor,
    )>,
    mut texts: Query<&mut TextColor>,
) {
    for (t, kids, mut bg, mut line) in &mut tabs {
        let on = t.0 == journal.tab;
        let (fill, ink) = if on { (INDIGO, CREAM) } else { (PAPER, INK) };
        bg.set_if_neq(BackgroundColor(fill));
        line.set_if_neq(BorderColor::all(if on { INDIGO } else { BORDER }));
        for k in kids {
            if let Ok(mut c) = texts.get_mut(*k) {
                c.set_if_neq(TextColor(ink));
            }
        }
    }
}

/// Content-area coordinates are the panel's minus the 134 px header.
fn place(x: f32, y: f32) -> Node {
    at(x, y - 134.0, 0.0)
}

fn party(c: &mut ChildSpawnerCommands, page: &Page) {
    c.spawn((
        Text::new(page.status.clone()),
        body(18.0, INDIGO),
        place(26.0, 137.0),
    ));
    // The party table.
    c.spawn((
        Node {
            width: px(756),
            height: px(260),
            border_radius: BorderRadius::all(px(16)),
            ..place(26.0, 174.0)
        },
        BackgroundColor(PAPER),
    ))
    .with_children(|t| {
        for (x, h) in [
            (16.0, "PLAYER"),
            (167.0, "TEARS"),
            (342.0, "MORALITY"),
            (486.0, "ORGANS"),
            (645.0, "TOTEMS"),
        ] {
            t.spawn((Text::new(h), body(13.0, INDIGO), at(x, 14.0, 0.0)));
        }
        for (i, r) in page.rows.iter().enumerate() {
            t.spawn((
                Node {
                    width: px(736),
                    height: px(30),
                    border_radius: BorderRadius::all(px(8)),
                    ..at(10.0, 44.0 + 34.0 * i as f32, 0.0)
                },
                BackgroundColor(if r.talking { BUTTER } else { Color::NONE }),
            ))
            .with_children(|row| {
                row.spawn((
                    Text::new(r.name.clone()),
                    body(15.0, INK),
                    at(6.0, 5.0, 0.0),
                ));
                for (x, s) in [
                    (157.0, &r.tears),
                    (332.0, &r.morality),
                    (476.0, &r.organs),
                    (635.0, &r.totems),
                ] {
                    row.spawn((
                        Text::new(s.clone()),
                        body(14.0, INK),
                        TextLayout::no_wrap(),
                        at(x, 6.0, 0.0),
                    ));
                }
            });
        }
    });
    c.spawn((
        Text::new("YOUR SKILLS   1–100"),
        body(15.0, INDIGO),
        place(26.0, 456.0),
    ));
    for (i, (name, lvl)) in page.skills.iter().enumerate() {
        let (x, y) = (26.0 + 250.0 * (i % 3) as f32, 486.0 + 53.0 * (i / 3) as f32);
        c.spawn((
            BackgroundColor(PAPER),
            BorderColor::all(BORDER),
            Node {
                width: px(237),
                height: px(42),
                border: UiRect::all(px(B)),
                border_radius: BorderRadius::all(px(12)),
                ..place(x, y)
            },
            children![(
                Text::new(format!("{name}  {lvl}")),
                body(17.0, INK),
                at(11.0, 8.0, B)
            )],
        ));
    }
    // Your bag: your own lines from every table.
    let mine = if page.mine.is_empty() {
        "Nothing yet. Go and get your hands dirty.".to_string()
    } else {
        page.mine.join("\n")
    };
    c.spawn((
        Node {
            width: px(360),
            height: px(224),
            border: UiRect::all(px(B)),
            border_radius: BorderRadius::all(px(18)),
            overflow: Overflow::clip(),
            ..place(810.0, 134.0)
        },
        BackgroundColor(PAPER),
        BorderColor::all(BORDER),
        children![
            (
                Text::new("One bag. Zero dignity."),
                head(25.0, INK),
                at(16.0, 12.0, B)
            ),
            (
                Text::new(mine),
                body(15.0, INK),
                Node {
                    width: px(328),
                    ..at(16.0, 58.0, B)
                }
            ),
        ],
    ));
    c.spawn((
        Node {
            width: px(360),
            height: px(181),
            border_radius: BorderRadius::all(px(18)),
            ..place(810.0, 377.0)
        },
        BackgroundColor(PAPER),
        children![
            (Text::new("H   How do I…"), head(24.0, INK), at(18.0, 14.0, 0.0)),
            (
                Text::new(
                    "Every key and what's going on\nlives in the Help tab.\n\nH opens it, H again closes it."
                ),
                body(15.0, INK),
                at(18.0, 60.0, 0.0)
            ),
        ],
    ));
    c.spawn((
        Node {
            width: px(1144),
            height: px(45),
            border_radius: BorderRadius::all(px(12)),
            ..place(26.0, 594.0)
        },
        BackgroundColor(INK),
        children![
            (
                Text::new(page.invite.clone()),
                body(17.0, CREAM),
                at(16.0, 11.0, 0.0)
            ),
            (
                Text::new("C  copy invite     ·     Tab  release to close     ·     J  keep open"),
                body(16.0, CREAM),
                at(568.0, 12.0, 0.0)
            ),
        ],
    ));
}

/// A titled paper card of free text, in two columns when there's a lot.
fn text_page(c: &mut ChildSpawnerCommands, title: &str, blocks: &[String]) {
    c.spawn((Text::new(title), head(25.0, INK), place(26.0, 140.0)));
    let empty = ["Nothing here yet.".to_string()];
    let blocks = if blocks.is_empty() {
        &empty[..]
    } else {
        blocks
    };
    // Split into two columns by line count.
    let total: usize = blocks.iter().map(|b| b.lines().count() + 1).sum();
    let mut cols = [String::new(), String::new()];
    let mut seen = 0;
    for b in blocks {
        let col = usize::from(seen > total / 2 && total > 18);
        cols[col].push_str(b);
        cols[col].push_str("\n\n");
        seen += b.lines().count() + 1;
    }
    for (i, col) in cols.iter().enumerate() {
        if col.is_empty() {
            continue;
        }
        c.spawn((
            Node {
                width: px(558),
                height: px(470),
                padding: UiRect::all(px(16)),
                border_radius: BorderRadius::all(px(16)),
                overflow: Overflow::clip(),
                ..place(26.0 + 586.0 * i as f32, 182.0)
            },
            BackgroundColor(PAPER),
            children![(
                Text::new(col.trim_end().to_string()),
                body(14.0, INK),
                Node {
                    width: px(526),
                    ..default()
                }
            )],
        ));
    }
}

fn skills_page(c: &mut ChildSpawnerCommands, page: &Page) {
    c.spawn((
        Text::new("Your skills"),
        head(25.0, INK),
        place(26.0, 140.0),
    ));
    c.spawn((
        Text::new("Levels go 1 to 100. Do a thing to get better at it."),
        body(16.0, INDIGO),
        place(26.0, 180.0),
    ));
    for (i, (name, lvl)) in page.skills.iter().enumerate() {
        let (x, y) = (
            26.0 + 380.0 * (i % 3) as f32,
            220.0 + 100.0 * (i / 3) as f32,
        );
        c.spawn((
            Node {
                width: px(364),
                height: px(84),
                border: UiRect::all(px(B)),
                border_radius: BorderRadius::all(px(16)),
                ..place(x, y)
            },
            BackgroundColor(PAPER),
            BorderColor::all(BORDER),
            children![
                (Text::new(name.clone()), head(24.0, INK), at(16.0, 12.0, B)),
                (
                    Text::new(format!("level {lvl}")),
                    body(16.0, INDIGO),
                    at(16.0, 48.0, B)
                ),
            ],
        ));
    }
}

fn credits(c: &mut ChildSpawnerCommands) {
    let lines: Vec<String> = crate::fault::faults(DESIGN)
        .into_iter()
        .map(|f| format!("{} (#{}): {}", f.who, f.issue, crate::hud::clip(&f.what)))
        .collect();
    text_page(c, "Whose fault is all this", &[lines.join("\n")]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_open_close_and_jump() {
        // J toggles, starting on the party page.
        assert_eq!(
            keys_to_journal(false, Tab::Bag, true, false, false, false),
            (true, Tab::Party)
        );
        assert_eq!(
            keys_to_journal(true, Tab::Bag, true, false, false, false),
            (false, Tab::Bag)
        );
        // H opens help; H on help closes.
        assert_eq!(
            keys_to_journal(false, Tab::Party, false, true, false, false),
            (true, Tab::Help)
        );
        assert_eq!(
            keys_to_journal(true, Tab::Help, false, true, false, false),
            (false, Tab::Help)
        );
        // Esc closes; Esc with nothing open does nothing here.
        assert!(!keys_to_journal(true, Tab::Skills, false, false, true, false).0);
        assert_eq!(
            keys_to_journal(false, Tab::Party, false, false, true, false),
            (false, Tab::Party)
        );
    }
}
