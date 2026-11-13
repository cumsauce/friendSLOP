//! Boba status effects (#356): every drink you buy in New York's boba shops
//! adds a timed stack to your [`Buzz`]. Stacks add up (up to the shop's cap)
//! and wear off one by one. The walk reads [`Buzz::speed`].

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// One drink still working on you.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Stack {
    /// The drink's name ("Brown Sugar Pearl Milk Tea").
    pub drink: String,
    /// Walking speed this stack adds (0.1 = +10%).
    pub speed: f32,
    /// Seconds left (whole seconds, ticked by the server).
    pub secs: f32,
    /// The screen tint it gives (r, g, b).
    pub tint: (f32, f32, f32),
}

/// A player's boba stacks. Replicated, on the player's body.
#[derive(Component, Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Buzz {
    pub stacks: Vec<Stack>,
}

impl Buzz {
    /// Walking and running speed multiplier from every stack.
    pub fn speed(&self) -> f32 {
        1.0 + self.stacks.iter().map(|s| s.speed).sum::<f32>()
    }

    /// Add a drink, dropping the one closest to wearing off past `max` stacks.
    pub fn add(&mut self, stack: Stack, max: usize) {
        self.stacks.push(stack);
        while self.stacks.len() > max.max(1) {
            let Some(i) = self
                .stacks
                .iter()
                .enumerate()
                .min_by(|a, b| a.1.secs.total_cmp(&b.1.secs))
                .map(|(i, _)| i)
            else {
                break;
            };
            self.stacks.remove(i);
        }
    }

    /// `dt` seconds pass: stacks wear down, finished ones drop off.
    pub fn tick(&mut self, dt: f32) {
        for s in &mut self.stacks {
            s.secs -= dt;
        }
        self.stacks.retain(|s| s.secs > 0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(speed: f32, secs: f32) -> Stack {
        Stack {
            drink: "taro".into(),
            speed,
            secs,
            tint: (1.0, 1.0, 1.0),
        }
    }

    #[test]
    fn stacks_add_up_cap_and_wear_off() {
        let mut b = Buzz::default();
        b.add(s(0.1, 10.0), 2);
        b.add(s(0.2, 30.0), 2);
        assert!((b.speed() - 1.3).abs() < 1e-5);
        b.add(s(0.1, 20.0), 2); // the 10 s one goes
        assert!((b.speed() - 1.3).abs() < 1e-5);
        assert!(b.stacks.iter().all(|x| x.secs > 10.0));
        b.tick(25.0);
        assert_eq!(b.stacks.len(), 1);
        b.tick(10.0);
        assert_eq!(b.speed(), 1.0);
    }
}
