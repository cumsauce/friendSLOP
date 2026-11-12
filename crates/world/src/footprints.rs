//! Ground that features have claimed, so the map's dressing (trees, bushes,
//! rocks) keeps off it. A feature calls [`claim`] once when its plugin is
//! built; scenery asks [`claimed`]. This is how a feature keeps its spot
//! clear without the map importing it (see "Crates" in AGENTS.md).

use std::sync::RwLock;

use bevy::prelude::*;

static CLAIMS: RwLock<Vec<(Vec2, f32)>> = RwLock::new(Vec::new());

/// Keep a circle of `radius` metres around `at` (x, z) clear of dressing.
pub fn claim(at: Vec2, radius: f32) {
    if let Ok(mut c) = CLAIMS.write()
        && !c.contains(&(at, radius))
    {
        c.push((at, radius));
    }
}

/// Is `p` (with `margin` metres of footprint of its own) on claimed ground?
pub fn claimed(p: Vec2, margin: f32) -> bool {
    let near = |(at, r): &(Vec2, f32)| p.distance(*at) < r + margin;
    // Models the map places claim their own ground.
    CLAIMS.read().is_ok_and(|c| c.iter().any(near))
        || crate::places::layout().props.iter().any(near)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claimed_ground_stays_clear() {
        claim(Vec2::new(500.0, 500.0), 3.0);
        claim(Vec2::new(500.0, 500.0), 3.0);
        assert!(claimed(Vec2::new(502.0, 500.0), 0.0));
        assert!(claimed(Vec2::new(504.0, 500.0), 1.5));
        assert!(!claimed(Vec2::new(510.0, 500.0), 1.0));
    }
}
