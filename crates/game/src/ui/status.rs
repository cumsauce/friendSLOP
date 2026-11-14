//! The bottom-right status stack (200 wide, 5 px off the bottom edge): gas
//! meter, mic chip and your fat cat's size. At critical mass the cat card
//! becomes the butter "PET. THE. CAT." fuse card, in its own slot above the
//! gas meter, never in the toast queue.

use bevy::prelude::*;

use super::{Icons, at, body, glyph, head, mmss};
use crate::AppState;
use crate::features::fart::Gas;
use crate::features::fat_cat::{FatCat, Fuse, Lapping, MAX_SIZE};
use crate::player::{LocalPlayer, Player};
use crate::theme::indigo::{BORDER, BUTTER, CREAM, INK, PERIWINKLE, RAISED};
use crate::voice::VoiceMuted;
use crate::voice_chat::VoiceActivity;
pub use friendslop_base::ui::StatusMount;

const W: f32 = 200.0;
const TRACK: f32 = 172.0;

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Card {
    Bomb,
    Gas,
    Mic,
    Cat,
}

#[derive(Component, Clone, Copy, PartialEq, Eq, Debug)]
enum Part {
    GasLabel,
    GasFill,
    GasHint,
    MicIcon,
    MicSlash,
    MicLabel,
    MicWave,
    CatLabel,
    BombTime,
    BombSize,
}

/// The gas card's three lines for `gas` percent.
pub fn gas_lines(gas: u8) -> (String, f32, &'static str) {
    let gas = gas.min(100);
    let ready = gas >= crate::features::fart::tuning().min_gas;
    let label = if ready {
        format!("{gas}% · READY")
    } else {
        format!("{gas}% · filling")
    };
    let hint = if ready {
        "{fart}  to fart"
    } else {
        "fills up over time"
    };
    (label, gas as f32 / 100.0, hint)
}

/// Is the gas card up: filled past `show_percent`, or ready to fart.
pub fn gas_shown(gas: u8, min_gas: u8, show_percent: f32) -> bool {
    gas >= min_gas || gas as f32 >= show_percent
}

/// Is your cat big enough that its size must stay on screen.
pub fn cat_urgent(size: f32, urgent_percent: f32) -> bool {
    size / MAX_SIZE * 100.0 >= urgent_percent
}

/// "4.2× / 25×".
pub fn cat_size(size: f32) -> String {
    format!("{:.1}× / {:.0}×", size.clamp(1.0, MAX_SIZE), MAX_SIZE)
}

pub fn plugin(app: &mut App) {
    app.add_systems(
        Update,
        (fill_mount, update)
            .chain()
            .run_if(in_state(AppState::InGame)),
    );
}

fn fill_mount(
    mut commands: Commands,
    icons: Res<Icons>,
    mounts: Query<Entity, Added<StatusMount>>,
) {
    for mount in &mounts {
        commands.entity(mount).insert(Node {
            width: px(W),
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            ..default()
        });
        commands.entity(mount).with_children(|c| {
            c.spawn(bomb(&icons));
            c.spawn(gas(&icons));
            c.spawn(mic(&icons));
            c.spawn(cat(&icons));
        });
    }
}

fn shell(card: Card, h: f32, radius: f32, fill: Color, line: Color) -> impl Bundle {
    (
        card,
        Node {
            width: px(W),
            height: px(h),
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(radius)),
            ..default()
        },
        BackgroundColor(fill),
        BorderColor::all(line),
    )
}

fn gas(icons: &Icons) -> impl Bundle {
    let b = 2.0;
    (
        shell(Card::Gas, 84.0, 18.0, INK, BORDER),
        children![
            glyph(&icons.gas, 25.0, 12.0, 12.0, b, CREAM),
            (
                Part::GasLabel,
                Text::new(""),
                body(18.0, PERIWINKLE),
                at(46.0, 10.0, b)
            ),
            (
                Node {
                    width: px(TRACK),
                    height: px(10),
                    border_radius: BorderRadius::all(px(5)),
                    overflow: Overflow::clip(),
                    ..at(12.0, 41.0, b)
                },
                BackgroundColor(RAISED),
                children![(
                    Part::GasFill,
                    Node {
                        width: px(0),
                        height: percent(100),
                        border_radius: BorderRadius::all(px(5)),
                        ..default()
                    },
                    BackgroundColor(BUTTER),
                )],
            ),
            (
                Part::GasHint,
                Text::new(""),
                body(15.0, CREAM),
                at(12.0, 56.0, b)
            ),
        ],
    )
}

fn mic(icons: &Icons) -> impl Bundle {
    let b = 2.0;
    (
        shell(Card::Mic, 47.0, 14.0, INK, INK),
        children![
            (Part::MicIcon, glyph(&icons.mic, 24.0, 10.0, 9.0, b, CREAM)),
            (
                Part::MicSlash,
                glyph(&icons.mic_slash, 24.0, 10.0, 9.0, b, Color::WHITE)
            ),
            (
                Part::MicLabel,
                Text::new(""),
                body(15.0, CREAM),
                at(42.0, 10.0, b)
            ),
            (
                Part::MicWave,
                Node {
                    width: px(12),
                    height: px(14),
                    ..at(176.0, 15.0, b)
                },
                children![bar(1.0, 4.0, 6.0), bar(5.0, 1.0, 12.0), bar(9.0, 3.0, 8.0)],
            ),
        ],
    )
}

fn bar(x: f32, y: f32, h: f32) -> impl Bundle {
    (
        Node {
            position_type: PositionType::Absolute,
            left: px(x),
            top: px(y),
            width: px(2),
            height: px(h),
            border_radius: BorderRadius::all(px(1)),
            ..default()
        },
        BackgroundColor(BUTTER),
    )
}

fn cat(icons: &Icons) -> impl Bundle {
    let b = 2.0;
    (
        shell(Card::Cat, 54.0, 18.0, INK, BORDER),
        children![
            glyph(&icons.cat, 28.0, 10.0, 12.0, b, CREAM),
            (
                Part::CatLabel,
                Text::new(""),
                body(17.0, CREAM),
                at(47.0, 13.0, b)
            ),
        ],
    )
}

fn bomb(icons: &Icons) -> impl Bundle {
    let b = 2.0;
    (
        shell(Card::Bomb, 110.0, 18.0, BUTTER, INK),
        children![
            glyph(&icons.warn, 28.0, 10.0, 12.0, b, INK),
            (
                Text::new("PET. THE. CAT."),
                body(18.0, INK),
                at(47.0, 10.0, b)
            ),
            (
                Part::BombTime,
                Text::new(""),
                head(36.0, INK),
                at(12.0, 34.0, b)
            ),
            (
                Part::BombSize,
                Text::new(""),
                body(14.0, INK),
                at(114.0, 46.0, b)
            ),
            (
                Text::new(""),
                crate::input::Hint("Hold {use} to pet it smaller".into()),
                body(13.0, INK),
                at(12.0, 82.0, b)
            ),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
fn update(
    time: Res<Time<Real>>,
    clock: Res<crate::hud::HudClock>,
    at: Res<crate::hud::LocalAt>,
    all: Res<crate::hud::ShowAll>,
    mut cat_seen: Local<crate::hud::Recent>,
    me: Option<Res<LocalPlayer>>,
    muted: Option<Res<VoiceMuted>>,
    activity: Option<Res<VoiceActivity>>,
    players: Query<(&Player, Option<&Gas>)>,
    cats: Query<(&FatCat, Option<&Fuse>, Has<Lapping>)>,
    mut cards: Query<(&Card, &mut Node, &mut BackgroundColor), Without<Part>>,
    mut parts: Query<
        (
            &Part,
            &mut Node,
            Option<&mut Text>,
            Option<&mut BackgroundColor>,
        ),
        Without<Card>,
    >,
) {
    let me = me.map(|m| m.0);
    let gas = players
        .iter()
        .find(|(p, _)| Some(p.id) == me)
        .and_then(|(_, g)| g.map(|g| g.0))
        .unwrap_or(0);
    let cat = cats.iter().find(|(c, ..)| Some(c.owner) == me);
    let fuse = cat.and_then(|(_, f, _)| f.map(|f| f.0));
    let muted = muted.is_some_and(|m| m.0);
    let talking = !muted && activity.is_some_and(|a| a.transmitting);
    // Blink like a bomb, twice a second (colour only; nothing moves).
    let blink = ((time.elapsed_secs() * 4.0) as u32).is_multiple_of(2);
    // The quiet rule (`hud::Calm`): gas once it's worth knowing, the mic chip
    // only when muted (or as a key hint early on), the cat when its size
    // steps up, when you're near it or when it's getting big.
    let t = crate::hud::tuning();
    let min_gas = crate::features::fart::tuning().min_gas;
    let gas_up = all.0 || gas_shown(gas, min_gas, t.gas_show_percent);
    let mic_up = all.0 || muted || crate::hud::hints_up(clock.0, &t);
    let cat_up = cat.is_some_and(|(c, ..)| {
        let age = cat_seen.tick(crate::hud::key_of(c.size.floor() as u32), time.delta_secs());
        let near = crate::hud::is_near(at.0, Some(c.pos), t.near_metres);
        all.0 || crate::hud::calm_alpha(age, near, cat_urgent(c.size, t.urgent_percent), &t) > 0.0
    });

    for (card, mut node, mut bg) in &mut cards {
        let show = match card {
            Card::Bomb => fuse.is_some(),
            Card::Cat => cat_up && fuse.is_none(),
            Card::Gas => gas_up,
            Card::Mic => mic_up,
        };
        let d = if show { Display::Flex } else { Display::None };
        if node.display != d {
            node.display = d;
        }
        if *card == Card::Mic {
            // The fuse card replaces the cat card but sits above the gas
            // meter, so keep the cat card's room (54 + 8) under the mic.
            let want = px(if fuse.is_some() && mic_up { 62.0 } else { 0.0 });
            if node.margin.bottom != want {
                node.margin.bottom = want;
            }
        }
        if *card == Card::Bomb {
            // 14 px to the gas meter, like the design (8 + 6).
            node.margin.bottom = px(6);
            let want = if blink {
                BUTTER
            } else {
                BUTTER.mix(&CREAM, 0.55)
            };
            bg.set_if_neq(BackgroundColor(want));
        }
    }

    let (gas_label, gas_frac, gas_hint) = gas_lines(gas);
    let ready = gas >= crate::features::fart::tuning().min_gas;
    for (part, mut node, text, bg) in &mut parts {
        let set = |s: String| {
            if let Some(mut t) = text
                && t.0 != s
            {
                t.0 = s;
            }
        };
        match part {
            Part::GasLabel => set(gas_label.clone()),
            Part::GasHint => set(crate::input::fill(gas_hint)),
            Part::GasFill => {
                let w = px(TRACK * gas_frac);
                if node.width != w {
                    node.width = w;
                }
                if let Some(mut bg) = bg {
                    bg.set_if_neq(BackgroundColor(if ready { BUTTER } else { PERIWINKLE }));
                }
            }
            Part::MicLabel => set(crate::input::fill(if muted {
                "{mic}  muted"
            } else {
                "{mic}  mic on"
            })),
            Part::MicSlash | Part::MicWave | Part::MicIcon => {
                let show = match part {
                    Part::MicSlash => muted,
                    Part::MicWave => talking,
                    _ => true,
                };
                let d = if show { Display::Flex } else { Display::None };
                if node.display != d {
                    node.display = d;
                }
            }
            Part::CatLabel => {
                let size = cat.map(|(c, ..)| c.size).unwrap_or(1.0);
                // Busy lapping up pee (it won't come to you meanwhile);
                // short enough to fit the card.
                let drinking = cat.is_some_and(|(_, _, lapping)| lapping);
                set(if drinking {
                    format!("Cat  {:.1}×  drinking", size.clamp(1.0, MAX_SIZE))
                } else {
                    format!("Cat  {}", cat_size(size))
                })
            }
            Part::BombTime => set(mmss(fuse.unwrap_or(0.0))),
            Part::BombSize => set(cat_size(cat.map(|(c, ..)| c.size).unwrap_or(1.0))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gas_says_ready_in_words_and_cat_sizes_clamp() {
        let (label, frac, hint) = gas_lines(100);
        assert_eq!(label, "100% · READY");
        assert_eq!(frac, 1.0);
        assert_eq!(hint, "{fart}  to fart");
        let (label, _, hint) = gas_lines(crate::features::fart::tuning().min_gas - 1);
        assert!(label.ends_with("filling"));
        assert_ne!(hint, "{fart}  to fart");
        assert_eq!(cat_size(4.2), "4.2× / 25×");
        assert_eq!(cat_size(24.8), "24.8× / 25×");
        assert_eq!(cat_size(99.0), "25.0× / 25×");
    }

    #[test]
    fn gas_shows_past_the_line_or_when_ready_and_big_cats_stay() {
        assert!(!gas_shown(10, 20, 50.0));
        assert!(gas_shown(20, 20, 50.0));
        assert!(gas_shown(60, 101, 50.0));
        assert!(!cat_urgent(2.0, 75.0));
        assert!(cat_urgent(20.0, 75.0));
    }
}
