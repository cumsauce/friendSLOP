//! Organs are the only legal tender. From issue #22 by Freyja Moth.
//!
//! An organ's value = how valuable its creature is × how important the organ is
//! to that creature (a brain is worth a lot, an appendix barely anything).
//! Every player carries a replicated `OrganPurse`. Everyone starts with a few
//! cheap animal organs so the currency is visible; how you earn or spend
//! organs is up to future ideas.

use bevy::prelude::*;
use bevy_replicon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::player::Player;

/// Who the organ came from.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Creature {
    Human,
    Cow,
    Pig,
    Chicken,
    Rat,
}

impl Creature {
    pub const ALL: [Creature; 5] = [
        Creature::Human,
        Creature::Cow,
        Creature::Pig,
        Creature::Chicken,
        Creature::Rat,
    ];

    /// How much the creature itself is worth.
    pub fn worth(self) -> u32 {
        match self {
            Creature::Human => 10,
            Creature::Cow => 5,
            Creature::Pig => 4,
            Creature::Chicken => 2,
            Creature::Rat => 1,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Creature::Human => "human",
            Creature::Cow => "cow",
            Creature::Pig => "pig",
            Creature::Chicken => "chicken",
            Creature::Rat => "rat",
        }
    }
}

/// Which organ it is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OrganKind {
    Brain,
    Heart,
    Lungs,
    Liver,
    Kidney,
    Stomach,
    Appendix,
}

impl OrganKind {
    pub const ALL: [OrganKind; 7] = [
        OrganKind::Brain,
        OrganKind::Heart,
        OrganKind::Lungs,
        OrganKind::Liver,
        OrganKind::Kidney,
        OrganKind::Stomach,
        OrganKind::Appendix,
    ];

    /// How important the organ is to keeping its creature going.
    pub fn importance(self) -> u32 {
        match self {
            OrganKind::Brain => 10,
            OrganKind::Heart => 9,
            OrganKind::Lungs => 7,
            OrganKind::Liver => 6,
            OrganKind::Kidney => 4,
            OrganKind::Stomach => 3,
            OrganKind::Appendix => 1,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            OrganKind::Brain => "brain",
            OrganKind::Heart => "heart",
            OrganKind::Lungs => "lungs",
            OrganKind::Liver => "liver",
            OrganKind::Kidney => "kidney",
            OrganKind::Stomach => "stomach",
            OrganKind::Appendix => "appendix",
        }
    }
}

/// One unit of currency.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Organ {
    pub creature: Creature,
    pub kind: OrganKind,
}

impl Organ {
    pub const fn new(creature: Creature, kind: OrganKind) -> Self {
        Self { creature, kind }
    }

    pub fn value(self) -> u32 {
        self.creature.worth() * self.kind.importance()
    }
}

/// The organs a player is carrying. Server-authoritative, replicated.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct OrganPurse(pub Vec<Organ>);

impl OrganPurse {
    pub fn total_value(&self) -> u32 {
        self.0.iter().map(|o| o.value()).sum()
    }
}

/// What everyone starts with: pocket change.
pub fn starting_purse() -> OrganPurse {
    OrganPurse(vec![
        Organ::new(Creature::Rat, OrganKind::Appendix),
        Organ::new(Creature::Chicken, OrganKind::Kidney),
        Organ::new(Creature::Pig, OrganKind::Stomach),
    ])
}

pub fn protocol(app: &mut App) {
    app.replicate::<OrganPurse>()
        .add_observer(give_starting_purse);
}

/// Server only: hand every new player their starting organs.
fn give_starting_purse(
    add: On<Add<Player>>,
    server: Res<State<ServerState>>,
    mut commands: Commands,
) {
    if *server.get() != ServerState::Running {
        return;
    }
    commands.entity(add.entity).insert(starting_purse());
}

pub fn plugin(app: &mut App) {
    app.add_systems(OnEnter(AppState::InGame), spawn_purse_hud)
        .add_systems(
            Update,
            update_purse_hud
                .run_if(crate::world::hud_tick())
                .run_if(in_state(AppState::InGame)),
        );
}

#[derive(Component)]
struct PurseHud;

fn spawn_purse_hud(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        PurseHud,
        Node {
            position_type: PositionType::Absolute,
            top: px(12),
            right: px(12),
            ..default()
        },
        DespawnOnExit(AppState::InGame),
    ));
}

fn update_purse_hud(
    purses: Query<(&Player, &OrganPurse)>,
    mut hud: Query<&mut Text, With<PurseHud>>,
) {
    let mut rows: Vec<_> = purses.iter().collect();
    rows.sort_by_key(|(p, _)| p.id);
    let mut lines = vec!["organs (legal tender)".to_string()];
    for (player, purse) in rows {
        let list: Vec<String> = purse
            .0
            .iter()
            .map(|o| format!("{} {}", o.creature.name(), o.kind.name()))
            .collect();
        lines.push(format!(
            "player {}: worth {} ({})",
            player.id,
            purse.total_value(),
            if list.is_empty() {
                "nothing".to_string()
            } else {
                list.join(", ")
            }
        ));
    }
    let text = lines.join("\n");
    for mut t in &mut hud {
        if t.0 != text {
            t.0.clone_from(&text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brain_beats_appendix_and_human_beats_rat() {
        for c in Creature::ALL {
            assert!(
                Organ::new(c, OrganKind::Brain).value()
                    > Organ::new(c, OrganKind::Appendix).value()
            );
        }
        for k in OrganKind::ALL {
            assert!(Organ::new(Creature::Human, k).value() > Organ::new(Creature::Rat, k).value());
        }
    }

    #[test]
    fn purse_totals() {
        assert_eq!(OrganPurse::default().total_value(), 0);
        // rat appendix 1 + chicken kidney 8 + pig stomach 12
        assert_eq!(starting_purse().total_value(), 21);
    }
}
