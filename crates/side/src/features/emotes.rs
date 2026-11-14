//! Emotes with other players (#383, from catherine: "add emotes involving
//! other players like high five, rock paper scissor, and dapping up").
//!
//! U picks an emote (high five, dap up, rock paper scissors), O offers it to
//! the nearest player within [`Tuning::reach`]. If they press O while the
//! offer is open it lands: both get a toast (rock paper scissors throws for
//! both and says who won). Purely friendly: it changes nothing else.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// One emote.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Emote {
    pub name: String,
    /// Line both players see; `{a}` offered, `{b}` accepted.
    pub done: String,
}

/// Tuning and lines: `assets/data/side/emotes.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// How far (metres) the other player can be.
    pub reach: f32,
    /// Seconds an offer stays open.
    pub window: f32,
    /// The emotes U cycles through.
    pub emotes: Vec<Emote>,
    /// Rock paper scissors throws; each beats the next one round.
    pub throws: Vec<String>,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/emotes.ron");

/// Client -> server: offer emote `kind`, or accept an open offer to me.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct EmoteAsk {
    pub kind: u8,
}

/// Server -> the two players: an offer or a finished emote.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum EmoteNews {
    Offered {
        from: u64,
        to: u64,
        kind: u8,
    },
    Done {
        a: u64,
        b: u64,
        kind: u8,
        throws: Option<(u8, u8)>,
    },
}

/// Server only: open offers (from, to, kind, when).
#[derive(Resource, Default)]
struct Offers(Vec<(u64, u64, u8, f32)>);

/// Client only: the emote U has picked.
#[derive(Resource, Default)]
struct Picked(u8);

pub fn protocol(app: &mut App) {
    app.add_client_event::<EmoteAsk>(Channel::Ordered)
        .add_server_event::<EmoteNews>(Channel::Ordered)
        .init_resource::<Offers>()
        .add_observer(hear_ask);
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Picked>()
        .add_observer(show_news)
        .add_systems(Update, keys.run_if(in_state(AppState::InGame)));
}

/// The nearest other player within `reach` of `me`.
pub fn nearest(me: u64, reach: f32, players: &[(u64, Vec2)]) -> Option<u64> {
    let at = players.iter().find(|p| p.0 == me)?.1;
    players
        .iter()
        .filter(|p| p.0 != me && p.1.distance(at) <= reach)
        .min_by(|a, b| a.1.distance(at).total_cmp(&b.1.distance(at)))
        .map(|p| p.0)
}

/// Who wins rock paper scissors: `Some(true)` if `a` wins, `None` on a tie.
/// Each throw beats the next one round.
pub fn rps(a: u8, b: u8, n: u8) -> Option<bool> {
    let n = n.max(1);
    if a % n == b % n {
        None
    } else {
        Some((a + 1) % n == b % n)
    }
}

/// What the server does with an ask: `Ok` an offer to accept (the one from
/// whoever offered `me`), or `Err(target)` a new offer.
fn accept_or_offer(
    offers: &[(u64, u64, u8, f32)],
    me: u64,
    target: Option<u64>,
    now: f32,
    window: f32,
) -> Option<Result<(u64, u8), u64>> {
    if let Some(o) = offers
        .iter()
        .filter(|o| o.1 == me && now - o.3 <= window)
        .max_by(|a, b| a.3.total_cmp(&b.3))
    {
        return Some(Ok((o.0, o.2)));
    }
    target.map(Err)
}

fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut picked: ResMut<Picked>,
    toast: Option<ResMut<Toast>>,
    mut commands: Commands,
) {
    let t = TUNING.get();
    let n = t.emotes.len().max(1) as u8;
    if keys.just_pressed(KeyCode::KeyU) {
        picked.0 = (picked.0 + 1) % n;
        if let (Some(mut toast), Some(e)) = (toast, t.emotes.get(picked.0 as usize)) {
            toast.show(format!("emote: {} ({{offer}} to offer it)", e.name));
        }
    }
    if keys.just_pressed(KeyCode::KeyO) {
        commands.client_trigger(EmoteAsk { kind: picked.0 % n });
    }
}

fn hear_ask(
    ev: On<FromClient<EmoteAsk>>,
    time: Res<Time>,
    mut offers: ResMut<Offers>,
    mut commands: Commands,
    players: Query<(&Player, &PlayerOwner, &Walker)>,
) {
    let t = TUNING.get();
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some(me) = players.iter().find(|p| p.1.0 == owner).map(|p| p.0.id) else {
        return;
    };
    let now = time.elapsed_secs();
    offers.0.retain(|o| now - o.3 <= t.window);
    let all: Vec<(u64, Vec2)> = players.iter().map(|p| (p.0.id, p.2.pos.xz())).collect();
    let target = nearest(me, t.reach, &all);
    let Some(choice) = accept_or_offer(&offers.0, me, target, now, t.window) else {
        return;
    };
    let (news, a, b) = match choice {
        Ok((from, kind)) => {
            offers.0.retain(|o| o.0 != from && o.1 != me);
            let throws = t
                .emotes
                .get(kind as usize)
                .filter(|e| e.name == "rock paper scissors")
                .map(|_| {
                    let n = t.throws.len().max(1) as u32;
                    let s = (now * 1000.0) as u32 ^ (from as u32).wrapping_mul(2654435761);
                    ((s % n) as u8, ((s / 7 + me as u32) % n) as u8)
                });
            let news = EmoteNews::Done {
                a: from,
                b: me,
                kind,
                throws,
            };
            (news, from, me)
        }
        Err(to) => {
            offers.0.retain(|o| o.0 != me);
            offers.0.push((me, to, ev.kind, now));
            let news = EmoteNews::Offered {
                from: me,
                to,
                kind: ev.kind,
            };
            (news, me, to)
        }
    };
    for (p, o, _) in &players {
        if p.id != a && p.id != b {
            continue;
        }
        let target = match o.0 {
            None => ClientId::Server,
            Some(c) => ClientId::Client(c),
        };
        commands.server_trigger(ToClients {
            targets: SendTargets::Single(target),
            message: news.clone(),
        });
    }
}

fn who(id: u64, me: Option<u64>) -> String {
    if me == Some(id) {
        "you".into()
    } else {
        format!("player {id}")
    }
}

/// The toast line for a piece of news, as seen by `me`.
pub fn line(t: &Tuning, news: &EmoteNews, me: Option<u64>) -> String {
    let name = |k: u8| {
        t.emotes
            .get(k as usize)
            .map_or("emote".to_string(), |e| e.name.clone())
    };
    match news {
        EmoteNews::Offered { from, to, kind } => {
            if me == Some(*from) {
                format!("you offer {} a {}", who(*to, me), name(*kind))
            } else {
                format!(
                    "{} wants a {} ({{offer}} to accept)",
                    who(*from, me),
                    name(*kind)
                )
            }
        }
        EmoteNews::Done {
            a,
            b,
            kind,
            throws: Some((x, y)),
        } => {
            let throw = |i: u8| t.throws.get(i as usize).cloned().unwrap_or_default();
            let n = t.throws.len() as u8;
            let result = match rps(*x, *y, n) {
                None => "tie!".to_string(),
                Some(true) => format!("{} wins!", who(*a, me)),
                Some(false) => format!("{} wins!", who(*b, me)),
            };
            let _ = kind;
            format!(
                "{}: {}, {}: {}. {result}",
                who(*a, me),
                throw(*x),
                who(*b, me),
                throw(*y)
            )
        }
        EmoteNews::Done { a, b, kind, .. } => t
            .emotes
            .get(*kind as usize)
            .map_or(String::new(), |e| e.done.clone())
            .replace("{a}", &who(*a, me))
            .replace("{b}", &who(*b, me)),
    }
}

fn show_news(ev: On<EmoteNews>, me: Option<Res<LocalPlayer>>, toast: Option<ResMut<Toast>>) {
    let Some(mut toast) = toast else { return };
    toast.show(line(&TUNING.get(), &ev, me.map(|m| m.0)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_skips_me_and_far_ones() {
        let ps = [
            (0, Vec2::ZERO),
            (1, Vec2::new(2.0, 0.0)),
            (2, Vec2::new(1.0, 0.0)),
            (3, Vec2::new(9.0, 0.0)),
        ];
        assert_eq!(nearest(0, 3.0, &ps), Some(2));
        assert_eq!(nearest(3, 3.0, &ps), None);
    }

    #[test]
    fn rock_beats_scissors() {
        assert_eq!(rps(0, 1, 3), Some(true));
        assert_eq!(rps(1, 0, 3), Some(false));
        assert_eq!(rps(2, 0, 3), Some(true));
        assert_eq!(rps(1, 1, 3), None);
    }

    #[test]
    fn accepting_beats_offering() {
        let offers = [(1, 0, 2, 1.0)];
        assert_eq!(
            accept_or_offer(&offers, 0, Some(1), 2.0, 6.0),
            Some(Ok((1, 2)))
        );
        assert_eq!(accept_or_offer(&offers, 0, Some(1), 9.0, 6.0), Some(Err(1)));
        assert_eq!(accept_or_offer(&[], 0, None, 0.0, 6.0), None);
    }
}
