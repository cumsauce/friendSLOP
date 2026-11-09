//! The cat palette's colour maths (#12): what a cat sees, popped into a bright
//! storybook look. The game repaints its materials with it (`features/cat_palette.rs`).

use bevy::prelude::*;

/// Viénot et al. (1999) dichromat (deuteranope) simulation in linear RGB. Rows 1 and 2
/// are equal, so red and green become indistinguishable, like for a cat.
pub fn cat_vision(c: LinearRgba) -> LinearRgba {
    let r = 0.290_31 * c.red + 0.709_69 * c.green;
    let b = -0.021_97 * c.red + 0.021_97 * c.green + c.blue;
    LinearRgba::new(r, r, b.clamp(0.0, 1.0), c.alpha)
}

/// Cat vision plus a bright, saturated cartoon look: shadows lifted, colours pushed.
pub fn cat_palette(color: Color) -> Color {
    let seen = cat_vision(color.to_linear());
    let mut lch = Oklcha::from(seen);
    lch.lightness = 0.25 + 0.75 * lch.lightness;
    lch.chroma = (lch.chroma * 1.5).min(0.25);
    let s = Srgba::from(lch);
    Color::Srgba(Srgba::new(
        s.red.clamp(0.0, 1.0),
        s.green.clamp(0.0, 1.0),
        s.blue.clamp(0.0, 1.0),
        s.alpha,
    ))
}
