//! Saying hi (#371, from andrew: "Hi", no details).
//!
//! Press T to say hi. The server tells everyone within [`Tuning::reach`] of
//! you, and the villagers that close each say hi back (a toast for whoever
//! hears it). Purely friendly: it changes nothing else.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::villagers::Villager;
use crate::hud::Toast;
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Tuning and lines: `assets/data/side/hi.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// How far (metres) a hi carries.
    pub reach: f32,
    /// Seconds before the same player can say hi again.
    pub cooldown: f32,
    /// Villager replies; `{name}` is the villager, `{you}` the greeter.
    pub replies: Vec<String>,
}

pub static TUNING: friendslop_data::Data<Tuning> = friendslop_data::Data::ron("data/side/hi.ron");

/// Client -> server: I say hi.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SayHi;

/// Server -> nearby players: `from` said hi, and these villagers answered.
#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HiSaid {
    pub from: u64,
    pub villagers: Vec<u8>,
}

/// Server only: last time (elapsed secs) each player said hi.
#[derive(Resource, Default)]
struct LastHi(Vec<(u64, f32)>);

pub fn protocol(app: &mut App) {
    app.add_client_event::<SayHi>(Channel::Ordered)
        .add_server_event::<HiSaid>(Channel::Ordered)
        .init_resource::<LastHi>()
        .add_observer(hear_hi);
}

pub fn plugin(app: &mut App) {
    app.add_observer(show_hi)
        .add_systems(Update, hi_key.run_if(in_state(AppState::InGame)));
}

/// The ids of everything within `reach` of `at`.
pub fn within<T: Copy>(at: Vec2, reach: f32, things: &[(T, Vec2)]) -> Vec<T> {
    things
        .iter()
        .filter(|(_, p)| p.distance(at) <= reach)
        .map(|(id, _)| *id)
        .collect()
}

/// A villager's reply line.
pub fn reply(lines: &[String], villager: u8, name: &str, you: &str) -> String {
    let Some(line) = lines.get(villager as usize % lines.len().max(1)) else {
        return format!("{name}: hi!");
    };
    line.replace("{name}", name).replace("{you}", you)
}

fn hi_key(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if keys.just_pressed(KeyCode::KeyT) {
        commands.client_trigger(SayHi);
    }
}

fn hear_hi(
    ev: On<FromClient<SayHi>>,
    time: Res<Time>,
    mut last: ResMut<LastHi>,
    mut commands: Commands,
    players: Query<(&Player, &PlayerOwner, &Walker)>,
    villagers: Query<&Villager>,
) {
    let t = TUNING.get();
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((me, _, walker)) = players.iter().find(|p| p.1.0 == owner) else {
        return;
    };
    let now = time.elapsed_secs();
    if last
        .0
        .iter()
        .any(|(id, at)| *id == me.id && now - at < t.cooldown)
    {
        return;
    }
    last.0.retain(|(id, _)| *id != me.id);
    last.0.push((me.id, now));
    let at = walker.pos.xz();
    let vs: Vec<(u8, Vec2)> = villagers.iter().map(|v| (v.id, v.pos)).collect();
    let said = HiSaid {
        from: me.id,
        villagers: within(at, t.reach, &vs),
    };
    for (_, o, w) in &players {
        if w.pos.xz().distance(at) > t.reach {
            continue;
        }
        let target = match o.0 {
            None => ClientId::Server,
            Some(c) => ClientId::Client(c),
        };
        commands.server_trigger(ToClients {
            targets: SendTargets::Single(target),
            message: said.clone(),
        });
    }
}

fn show_hi(ev: On<HiSaid>, me: Option<Res<LocalPlayer>>, toast: Option<ResMut<Toast>>) {
    let Some(mut toast) = toast else { return };
    let mine = me.is_some_and(|m| m.0 == ev.from);
    let you = if mine {
        "you".to_string()
    } else {
        format!("player {}", ev.from)
    };
    let mut lines = vec![if mine {
        "you: hi!".to_string()
    } else {
        format!("player {}: hi!", ev.from)
    }];
    let t = TUNING.get();
    for v in ev.villagers.iter().take(3) {
        let name = crate::features::villagers::name(*v as usize);
        lines.push(reply(&t.replies, *v, &name, &you));
    }
    toast.show(lines.join("\n"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_near_ones_hear() {
        let things = [(1u8, Vec2::new(3.0, 0.0)), (2, Vec2::new(30.0, 0.0))];
        assert_eq!(within(Vec2::ZERO, 12.0, &things), vec![1]);
    }

    #[test]
    fn replies_fill_in_names() {
        let lines = vec!["{name}: hi {you}!".to_string()];
        assert_eq!(reply(&lines, 4, "Mo", "you"), "Mo: hi you!");
        assert_eq!(reply(&[], 0, "Mo", "you"), "Mo: hi!");
    }
}
