//! Furniture made of kit pieces, in the kit's current local frame.

use bevy::prelude::*;

use crate::{Kit, Palette, Shape};

/// Bed with a pillow and blanket, standing on the floor at `at`, lying
/// along z with its head at -z.
pub fn bed<M: Palette>(k: &mut Kit<M>, at: Vec3) {
    let p = k.begin_prop("bed", at, Vec3::ONE);
    k.prop(at, Vec3::new(1.0, 0.5, 2.0), M::WOOD);
    let c = at + Vec3::new(0.0, 0.55, 0.15);
    k.trinket(Shape::Cube, c, Vec3::new(0.95, 0.12, 1.6), M::CLOTH);
    let c = at + Vec3::new(0.0, 0.6, -0.75);
    k.trinket(Shape::Cube, c, Vec3::new(0.7, 0.14, 0.35), M::LINEN);
    k.end_prop(p);
}

/// A table with a bowl of fruit.
pub fn table<M: Palette>(k: &mut Kit<M>, at: Vec3) {
    let p = k.begin_prop("table", at, Vec3::ONE);
    k.prop(at + Vec3::Y * 0.7, Vec3::new(1.4, 0.08, 0.9), M::WOOD);
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let c = at + Vec3::new(sx * 0.6, 0.0, sz * 0.35);
        k.prop(c, Vec3::new(0.08, 0.7, 0.08), M::TIMBER);
    }
    let fruit = at + Vec3::new(0.2, 0.86, 0.0);
    k.trinket(Shape::Ball, fruit, Vec3::splat(0.16), M::FRUIT);
    let apple = at + Vec3::new(0.05, 0.86, 0.1);
    k.trinket(Shape::Ball, apple, Vec3::splat(0.14), M::RED);
    k.end_prop(p);
}

/// A crate, with a smaller one on top if `tall`.
pub fn crates<M: Palette>(k: &mut Kit<M>, at: Vec3, tall: bool) {
    let p = k.begin_prop(if tall { "crates" } else { "crates_single" }, at, Vec3::ONE);
    k.prop(at, Vec3::splat(0.85), M::WOOD);
    if tall {
        k.prop(at + Vec3::new(0.05, 0.85, 0.0), Vec3::splat(0.6), M::PLANK);
    }
    k.end_prop(p);
}

/// A tall shelf or wardrobe against a wall, `size` along x/z as given, with
/// goods on three shelves.
pub fn shelf<M: Palette>(k: &mut Kit<M>, at: Vec3, size: Vec3) {
    let p = k.begin_prop("shelf", at, size / Vec3::new(1.2, 1.8, 0.4));
    k.prop(at, size, M::PLANK);
    for i in 1..4 {
        let c = at + Vec3::Y * size.y * i as f32 / 4.0;
        let goods = size * Vec3::new(0.7, 0.0, 0.7) + Vec3::Y * 0.18;
        k.trinket(Shape::Cube, c, goods, M::RED);
    }
    k.end_prop(p);
}
