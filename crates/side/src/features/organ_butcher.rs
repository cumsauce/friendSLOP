//! What organs are for (#217, "What are organs actually for?").
//!
//! Wendy's butcher buys them. Click the chopping block in the backroom and
//! the butcher takes your cheapest organ and pays you for it in dollars
//! ([`Tuning::value_per_dollar`] organ value to the dollar, at least one),
//! so a pig's stomach becomes a bodega hat. The landlord still wants organs
//! for the rent, which is the point: selling the small ones is easy, and
//! the big ones are what keeps the commune.
//!
//! Server-authoritative; the block lives at the `wendys` site.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::features::fps::Walker;
use crate::features::interact::{Clickable, Clicked, deny, in_reach};
use crate::features::organ_currency::{Organ, OrganPurse};
use crate::features::wallet::Cash;
use crate::features::wendys::{floor_y, wendys_at};
use crate::player::{LocalPlayer, Player, PlayerOwner};

/// Tuning and writing: `assets/data/side/organ_butcher.ron` (read at run time).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// Organ value that buys one dollar.
    pub value_per_dollar: u32,
    /// Sold ({organ}, {dollars}).
    pub sold_line: String,
    /// Nothing to sell.
    pub empty_line: String,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/side/organ_butcher.ron");

pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Where the chopping block's top is: backroom, west of the door (see `wendys.rs`).
pub fn block_spot() -> Vec3 {
    Vec3::new(wendys_at().x - 3.0, floor_y() + 0.95, wendys_at().y + 6.0)
}

/// Pure: dollars a butcher pays for `organ`.
pub fn payout(organ: Organ, value_per_dollar: u32) -> u32 {
    (organ.value() / value_per_dollar.max(1)).max(1)
}

/// Pure: which organ the butcher takes: the cheapest you have.
pub fn cheapest(purse: &OrganPurse) -> Option<usize> {
    purse
        .0
        .iter()
        .enumerate()
        .min_by_key(|(_, o)| o.value())
        .map(|(i, _)| i)
}

/// Client -> server: sell an organ at the block.
#[derive(Event, Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SellOrgan;

/// Server -> clients: a line for player `to`.
#[derive(Event, Serialize, Deserialize, Clone, Debug)]
pub struct ButcherNews {
    pub to: u64,
    pub text: String,
}

pub fn protocol(app: &mut App) {
    app.add_client_event::<SellOrgan>(Channel::Ordered)
        .add_server_event::<ButcherNews>(Channel::Ordered)
        .add_observer(sell);
}

fn sell(
    ev: On<FromClient<SellOrgan>>,
    mut commands: Commands,
    walkers: Query<(&PlayerOwner, &Walker)>,
    mut sellers: Query<(&Player, &PlayerOwner, &mut OrganPurse, &mut Cash)>,
) {
    if !in_reach(&mut commands, ev.client_id, &walkers, block_spot()) {
        return;
    }
    let owner = match ev.client_id {
        ClientId::Server => None,
        ClientId::Client(c) => Some(c),
    };
    let Some((player, _, mut purse, mut cash)) = sellers.iter_mut().find(|s| s.1.0 == owner) else {
        return;
    };
    let t = tuning();
    let Some(i) = cheapest(&purse) else {
        deny(&mut commands, ev.client_id, t.empty_line.clone());
        return;
    };
    let organ = purse.0.remove(i);
    let dollars = payout(organ, t.value_per_dollar);
    cash.dollars += dollars;
    let text = t
        .sold_line
        .replace(
            "{organ}",
            &format!("{} {}", organ.creature.name(), organ.kind.name()),
        )
        .replace("{dollars}", &dollars.to_string());
    commands.server_trigger(ToClients {
        targets: SendTargets::All,
        message: ButcherNews {
            to: player.id,
            text,
        },
    });
}

pub fn plugin(app: &mut App) {
    app.add_observer(show_news)
        .add_observer(click)
        .spawn_into("commune", build);
}

#[derive(Component)]
struct Block;

fn build(mut commands: Commands) {
    commands.spawn((
        Block,
        Transform::from_translation(block_spot()),
        Visibility::default(),
        Clickable::at_height(0.0).labelled("sell an organ to the butcher"),
        DespawnOnExit(AppState::InGame),
    ));
}

fn click(click: On<Clicked>, mut commands: Commands, blocks: Query<(), With<Block>>) {
    if blocks.contains(click.entity) {
        commands.client_trigger(SellOrgan);
    }
}

fn show_news(
    ev: On<ButcherNews>,
    me: Option<Res<LocalPlayer>>,
    toast: Option<ResMut<crate::hud::Toast>>,
) {
    let (Some(me), Some(mut toast)) = (me, toast) else {
        return;
    };
    if ev.to == me.0 {
        toast.show(ev.text.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::organ_currency::{Creature, OrganKind, starting_purse};

    #[test]
    fn the_butcher_takes_the_cheapest_and_pays_at_least_a_dollar() {
        let purse = starting_purse();
        let i = cheapest(&purse).expect("starting purse has organs");
        assert_eq!(purse.0[i], Organ::new(Creature::Rat, OrganKind::Appendix));
        assert_eq!(payout(purse.0[i], 4), 1);
        assert_eq!(payout(Organ::new(Creature::Human, OrganKind::Brain), 4), 25);
        assert_eq!(cheapest(&OrganPurse::default()), None);
    }
}
