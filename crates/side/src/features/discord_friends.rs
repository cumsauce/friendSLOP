//! The friends from the Discord walk the commune, and you can court them.
//! From issue #159 by catherine ("people in this discord and especially
//! those who have contributed ideas to the game should show up as
//! interactable and romancable characters").
//!
//! Every friend who has sent in an idea is a villager (see
//! `villagers::NAMES`, ids from [`villagers::FIRST_FRIEND`]): they wander,
//! hide among the hiders and chase farters like anyone else. Click one
//! ("chat up") and the server counts it: each chat is a heart, with a line
//! from them that gets sweeter (and mostly about their own ideas). At
//! [`HEARTS_TO_DATE`] they're dating you: everyone hears about it and pink
//! hearts float above them. A friend who's dating someone turns everybody
//! else down. Their evil twins are rats in the tunnels (`rats.rs`).

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::hide_and_seek::{HideSeek, Phase, Role};
use crate::features::interact::{SERVER_REACH, within};
use crate::features::villagers::{PokeVillager, Villager, is_friend};
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Tuning numbers: `assets/data/side/discord_friends.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Chats it takes for a friend to start dating you.
    pub hearts_to_date: u8,
    /// A friend only counts one chat from you this often (seconds).
    pub chat_cooldown: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/discord_friends.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// What each friend says as you win them over, one line per heart before
/// the last, in the order of `villagers::NAMES` from `FIRST_FRIEND`.
pub static LINES: friendslop_data::Data<Vec<Vec<String>>> =
    friendslop_data::Data::ron("data/text/discord_friends.ron");

/// What friend `id` says at `hearts` hearts (1 up to `HEARTS_TO_DATE - 1`).
pub fn line(id: u8, hearts: u8) -> String {
    let all = LINES.get();
    let friend = id.saturating_sub(crate::rules::villagers::FIRST_FRIEND) as usize;
    let Some(lines) = all.get(friend % all.len().max(1)) else {
        return String::new();
    };
    let i = (hearts.max(1) as usize - 1).min(lines.len().saturating_sub(1));
    lines.get(i).cloned().unwrap_or_default()
}

/// How a chat went.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chat {
    /// One heart closer: they now have this many for you.
    Warmer(u8),
    /// That was the last heart: they're dating you now.
    Dating,
    /// Already dating you; they just smile.
    Smitten,
    /// Already dating someone else (that player id).
    Taken(u64),
}

/// The decision: with `hearts` for `me` so far and their current `partner`,
/// what does one more chat do?
pub fn court(hearts: u8, partner: Option<u64>, me: u64) -> Chat {
    match partner {
        Some(p) if p == me => Chat::Smitten,
        Some(p) => Chat::Taken(p),
        None if hearts + 1 >= tuning().hearts_to_date => Chat::Dating,
        None => Chat::Warmer(hearts + 1),
    }
}

/// Server -> everyone: `player` chatted up friend `friend`, and how it went.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Courted {
    pub player: u64,
    pub friend: u8,
    pub chat: Chat,
}

/// Server only: who's sweet on whom.
#[derive(Resource, Default, Debug)]
pub struct Courtship {
    /// (player, friend) -> hearts.
    pub hearts: HashMap<(u64, u8), u8>,
    /// friend -> the player they're dating.
    pub partner: HashMap<u8, u64>,
    /// (player, friend) -> when their last chat counted.
    last: HashMap<(u64, u8), f32>,
}

pub fn protocol(app: &mut App) {
    app.init_resource::<Courtship>()
        .add_server_event::<Courted>(Channel::Ordered)
        .add_observer(chat_up)
        .add_observer(heartbreak_on_leave);
}

/// Server: a click on a friend is a chat.
#[allow(clippy::too_many_arguments)]
fn chat_up(
    ev: On<FromClient<PokeVillager>>,
    mut commands: Commands,
    time: Res<Time>,
    mut court_state: ResMut<Courtship>,
    villagers: Query<&Villager>,
    players: Query<(&Player, &PlayerOwner, &Walker, Option<&Role>)>,
    hide_seek: Query<&HideSeek>,
    still: Query<&super::stillness::Stillness>,
) {
    if !is_friend(ev.id) || super::stillness::is_frozen(&still) {
        return;
    }
    let Some(v) = villagers.iter().find(|v| v.id == ev.id) else {
        return;
    };
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(e) => Some(e),
    };
    let Some((player, _, walker, role)) = players.iter().find(|(_, o, ..)| o.0 == owner) else {
        return;
    };
    // A seeker mid-search is tagging, not flirting (villagers.rs handles it).
    let seeking = hide_seek.single().is_ok_and(|s| s.phase == Phase::Seeking);
    if seeking && role.is_some_and(|r| r.seeks()) {
        return;
    }
    if !within(walker.pos, Vec3::new(v.pos.x, 0.0, v.pos.y), SERVER_REACH) {
        return;
    }
    let key = (player.id, v.id);
    let now = time.elapsed_secs();
    if court_state
        .last
        .get(&key)
        .is_some_and(|t| now - t < tuning().chat_cooldown)
    {
        return;
    }
    court_state.last.insert(key, now);
    let hearts = court_state.hearts.get(&key).copied().unwrap_or(0);
    let chat = court(hearts, court_state.partner.get(&v.id).copied(), player.id);
    match chat {
        Chat::Warmer(h) => {
            court_state.hearts.insert(key, h);
        }
        Chat::Dating => {
            court_state.hearts.insert(key, tuning().hearts_to_date);
            court_state.partner.insert(v.id, player.id);
        }
        Chat::Smitten | Chat::Taken(_) => {}
    }
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: Courted {
            player: player.id,
            friend: v.id,
            chat,
        },
    });
}

/// Server: a player who leaves is single again, and so are their dates.
fn heartbreak_on_leave(
    remove: On<Remove<Player>>,
    server: Res<State<ServerState>>,
    players: Query<&Player>,
    mut court_state: ResMut<Courtship>,
) {
    if *server.get() != ServerState::Running {
        return;
    }
    let Ok(player) = players.get(remove.entity) else {
        return;
    };
    let gone = player.id;
    court_state.partner.retain(|_, p| *p != gone);
    court_state.hearts.retain(|(p, _), _| *p != gone);
    court_state.last.retain(|(p, _), _| *p != gone);
}

// ---------------------------------------------------------------- local side

pub fn plugin(app: &mut App) {
    app.add_observer(hear_chat);
}

/// Show how the chat went: the line to whoever chatted, the news to everyone
/// when someone starts dating, and hearts above the happy friend.
#[allow(clippy::too_many_arguments)]
fn hear_chat(
    ev: On<Courted>,
    mut commands: Commands,
    state: Option<Res<State<AppState>>>,
    me: Option<Res<LocalPlayer>>,
    villagers: Query<&Villager>,
    toast: Option<ResMut<Toast>>,
    materials: Option<ResMut<Assets<StandardMaterial>>>,
) {
    if state.is_none_or(|s| *s.get() != AppState::InGame) {
        return;
    }
    let name = crate::rules::villagers::name(ev.friend as usize);
    let mine = me.is_some_and(|m| m.0 == ev.player);
    let text = match (ev.chat, mine) {
        (Chat::Warmer(h), true) => Some(format!(
            "{name}: {} ({})",
            line(ev.friend, h),
            hearts_meter(h)
        )),
        (Chat::Dating, true) => Some(format!("{name} is dating you now! <3")),
        (Chat::Dating, false) => Some(format!("{name} is dating player {} now <3", ev.player)),
        (Chat::Smitten, true) => Some(format!("{name}: hi, you. <3")),
        (Chat::Taken(p), true) => {
            Some(format!("{name}: sorry, i'm with player {p}. it's serious."))
        }
        _ => None,
    };
    if let (Some(text), Some(mut toast)) = (text, toast) {
        toast.show(text);
    }
    if ev.chat == Chat::Dating
        && let (Some(v), Some(mut materials)) =
            (villagers.iter().find(|v| v.id == ev.friend), materials)
    {
        let ground = crate::terrain::height_at(v.pos.x, v.pos.y);
        super::romance::spawn_hearts(
            &mut commands,
            &mut materials,
            Vec3::new(v.pos.x, ground + 1.8, v.pos.y),
        );
    }
}

/// "<3 <3 . . ." for `hearts` out of `HEARTS_TO_DATE`.
pub fn hearts_meter(hearts: u8) -> String {
    (0..tuning().hearts_to_date)
        .map(|i| if i < hearts { "<3" } else { "." })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::villagers::{COUNT, FIRST_FRIEND, name};

    #[test]
    fn five_chats_and_they_are_yours() {
        let mut hearts = 0;
        let mut partner = None;
        for _ in 0..tuning().hearts_to_date - 1 {
            match court(hearts, partner, 7) {
                Chat::Warmer(h) => hearts = h,
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(court(hearts, partner, 7), Chat::Dating);
        partner = Some(7);
        assert_eq!(court(hearts, partner, 7), Chat::Smitten);
        assert_eq!(court(0, partner, 3), Chat::Taken(7));
    }

    #[test]
    fn every_friend_has_their_lines() {
        assert_eq!(COUNT - FIRST_FRIEND as usize, LINES.get().len());
        for id in FIRST_FRIEND..COUNT as u8 {
            assert!(is_friend(id), "{}", name(id as usize));
            for h in 1..tuning().hearts_to_date {
                assert!(!line(id, h).is_empty());
            }
        }
        assert!(!is_friend(0) && !is_friend(FIRST_FRIEND - 1));
        assert_eq!(line(FIRST_FRIEND + 1, 1), LINES.get()[1][0]);
        assert_eq!(hearts_meter(2), "<3 <3 . . .");
    }
}
