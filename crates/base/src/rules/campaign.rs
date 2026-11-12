//! Campaign-wide events (campaign.rs, objective.rs).

#![allow(unused_imports)]
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// From `features/campaign.rs`.
/// Server only: the campaign starts over from chapter one (after the finale).
/// Things that last the whole campaign reset on this instead of every
/// [`RoundReset`]: the gods tears (they rained only once, #43), what they did
/// to you, your morality and your stats. Triggered after the `RoundReset`.
#[derive(Event, Clone, Copy, Debug)]
pub struct NewCampaign;

// From `features/objective.rs`.
/// Server only: a new round begins. Features observe this to reset their state.
#[derive(Event, Clone, Copy, Debug)]
pub struct RoundReset;
