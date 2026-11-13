//! Where New York City is (#283): a flat city floor beyond the south rim
//! hills, outside the valley's soft wall. Features that keep bodies inside
//! the valley let them walk the city instead.

use bevy::prelude::*;

/// The city floor's corners (x, z): south of the rim hills (the valley ends at 110 m).
pub const CITY_MIN: Vec2 = Vec2::new(-90.0, 116.0);
pub const CITY_MAX: Vec2 = Vec2::new(90.0, 330.0);
/// The city's street level.
pub const CITY_Y: f32 = 0.0;

/// Is (x, z) on the city floor?
pub fn in_city(x: f32, z: f32) -> bool {
    (CITY_MIN.x..=CITY_MAX.x).contains(&x) && (CITY_MIN.y..=CITY_MAX.y).contains(&z)
}

/// Keep a body that's in the city on the city floor (the soft wall of the
/// city), or `None` when it isn't in the city.
pub fn keep_in_city(from: Vec3, to: Vec3) -> Option<Vec3> {
    in_city(from.x, from.z).then(|| {
        Vec3::new(
            to.x.clamp(CITY_MIN.x + 1.0, CITY_MAX.x - 1.0),
            to.y,
            to.z.clamp(CITY_MIN.y + 1.0, CITY_MAX.y - 1.0),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_city_is_beyond_the_valley() {
        assert!(!in_city(0.0, 100.0));
        assert!(in_city(0.0, 200.0));
        let kept = keep_in_city(Vec3::new(0.0, 0.0, 200.0), Vec3::new(500.0, 0.0, 0.0));
        assert_eq!(kept, Some(Vec3::new(89.0, 0.0, 117.0)));
        assert_eq!(keep_in_city(Vec3::ZERO, Vec3::ONE), None);
    }
}
