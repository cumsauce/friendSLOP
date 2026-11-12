//! Kitty lemonade: your fat cat is distracted by pee puddles. From issue #213
//! by Kori.exe ("The cat that follows you around will be distracted by pee
//! puddles for any time between 10 and 30 seconds. It will approach urine
//! puddles and drink so the puddle should disappear after").
//!
//! Server only. A fat cat (#145) that isn't busy notices any pee puddle
//! (#176, X) within [`NOTICE`] metres, stops following its owner and walks
//! over to it. It's distracted for [`distraction_secs`] (10-30 s, different
//! per puddle): once its head is over the puddle it drinks ([`Lapping`]),
//! the puddle stops drying, and when the time is up the puddle is gone and
//! the cat goes back to its owner. If it never got there in time it gives up
//! and the puddle dries as usual. Each puddle is drunk by one cat at a time.

use bevy::prelude::*;
use bevy_replicon::prelude::*;

use super::fat_cat::{CatSet, FatCat, LAP_HEEL, Lapping, Lured};
use super::skills::{Lapped, PeePuddle};

/// Tuning numbers: `assets/data/afterlife/kitty_lemonade.ron` (read at run time, edit without a rebuild).
#[derive(serde::Deserialize, Default, Debug, Clone)]
pub struct Tuning {
    /// A cat smells pee this far away (metres, plus its size).
    pub notice: f32,
    /// Shortest distraction (seconds).
    pub min_secs: f32,
    /// Longest distraction (seconds).
    pub max_secs: f32,
}

pub static TUNING: friendslop_data::Data<Tuning> =
    friendslop_data::Data::ron("data/afterlife/kitty_lemonade.ron");

/// The current tuning numbers.
pub fn tuning() -> std::sync::Arc<Tuning> {
    TUNING.get()
}

/// Server only: on a cat going for (or drinking) `puddle`.
#[derive(Component, Debug)]
struct Distracted {
    puddle: Entity,
    left: f32,
}

/// How long puddle `id` distracts a cat: somewhere in 10-30 s, scattered
/// by the id so each puddle is different (and every run agrees).
pub fn distraction_secs(id: u32) -> f32 {
    let mut x = id.wrapping_mul(0x9E37_79B9) ^ 0x5bd1_e995;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    tuning().min_secs + (tuning().max_secs - tuning().min_secs) * (x % 10_001) as f32 / 10_000.0
}

/// Whether a cat of `size` at `cat` has its head over a puddle at `puddle`.
pub fn at_puddle(cat: Vec2, size: f32, puddle: Vec2) -> bool {
    cat.distance(puddle) <= LAP_HEEL * size + 0.3
}

/// The closest puddle a cat at `cat` of `size` can smell, out of `puddles`.
pub fn sniff<T: Copy>(cat: Vec2, size: f32, puddles: &[(T, Vec2)]) -> Option<T> {
    puddles
        .iter()
        .map(|(t, at)| (*t, cat.distance(*at)))
        .filter(|(_, d)| *d <= tuning().notice + size)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(t, _)| t)
}

pub fn protocol(app: &mut App) {
    app.add_systems(
        Update,
        (drink, notice)
            .chain()
            .before(CatSet)
            .run_if(in_state(ServerState::Running)),
    );
}

/// Server: idle cats go for the nearest free puddle.
fn notice(
    mut commands: Commands,
    still: Query<&super::stillness::Stillness>,
    cats: Query<(Entity, &FatCat), Without<Distracted>>,
    puddles: Query<(Entity, &PeePuddle), Without<Lapped>>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let mut free: Vec<(Entity, Vec2)> = puddles.iter().map(|(e, p)| (e, p.at.xz())).collect();
    for (cat_e, cat) in &cats {
        let Some(puddle_e) = sniff(cat.pos.xz(), cat.size, &free) else {
            continue;
        };
        let Ok((_, puddle)) = puddles.get(puddle_e) else {
            continue;
        };
        free.retain(|(e, _)| *e != puddle_e);
        commands.entity(puddle_e).insert(Lapped);
        commands.entity(cat_e).insert((
            Distracted {
                puddle: puddle_e,
                left: distraction_secs(puddle.id),
            },
            Lured(puddle.at),
        ));
    }
}

/// Server: distracted cats drink when they get there; when the time is up
/// the puddle is gone (if they drank) and they go home.
fn drink(
    mut commands: Commands,
    time: Res<Time>,
    still: Query<&super::stillness::Stillness>,
    mut cats: Query<(Entity, &FatCat, &mut Distracted, Option<&mut Lapping>)>,
    puddles: Query<&PeePuddle>,
) {
    if super::stillness::is_frozen(&still) {
        return;
    }
    let dt = time.delta_secs();
    for (cat_e, cat, mut d, lapping) in &mut cats {
        d.left -= dt;
        let Ok(puddle) = puddles.get(d.puddle) else {
            // Gone some other way (a new round, say): back to the owner.
            commands
                .entity(cat_e)
                .remove::<(Distracted, Lured, Lapping)>();
            continue;
        };
        let there = at_puddle(cat.pos.xz(), cat.size, puddle.at.xz());
        if d.left <= 0.0 {
            if lapping.is_some() || there {
                commands.entity(d.puddle).despawn();
            } else {
                commands.entity(d.puddle).remove::<Lapped>();
            }
            commands
                .entity(cat_e)
                .remove::<(Distracted, Lured, Lapping)>();
            continue;
        }
        match lapping {
            Some(mut l) => {
                // Only replicate when the shown second changes.
                let before = l.0.ceil();
                l.bypass_change_detection().0 = d.left;
                if d.left.ceil() != before {
                    l.set_changed();
                }
            }
            None if there => {
                commands.entity(cat_e).insert(Lapping(d.left));
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distractions_last_ten_to_thirty_seconds_and_vary() {
        let all: Vec<f32> = (0..500).map(distraction_secs).collect();
        assert!(
            all.iter()
                .all(|s| (tuning().min_secs..=tuning().max_secs).contains(s))
        );
        let lo = all.iter().copied().fold(f32::MAX, f32::min);
        let hi = all.iter().copied().fold(f32::MIN, f32::max);
        assert!(lo < 13.0 && hi > 27.0, "spread {lo}..{hi}");
    }

    #[test]
    fn cats_smell_the_nearest_puddle_in_range() {
        let puddles = [(1, Vec2::new(10.0, 0.0)), (2, Vec2::new(4.0, 0.0))];
        assert_eq!(sniff(Vec2::ZERO, 1.0, &puddles), Some(2));
        assert_eq!(sniff(Vec2::new(-100.0, 0.0), 1.0, &puddles), None);
        // A huge cat smells further.
        assert_eq!(sniff(Vec2::new(-20.0, 0.0), 10.0, &puddles), Some(2));
    }

    #[test]
    fn bigger_cats_drink_from_further_back() {
        assert!(!at_puddle(Vec2::ZERO, 1.0, Vec2::new(3.0, 0.0)));
        assert!(at_puddle(Vec2::ZERO, 10.0, Vec2::new(3.0, 0.0)));
    }
}
