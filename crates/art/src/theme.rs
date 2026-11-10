//! The cosy UI theme (not a friend idea; UI polish): one pastel palette, a
//! spacing/radius scale, soft shadows and a few cheap tweens shared by the
//! HUD, menu, character creator and loading screen.
//!
//! Everything here is local decoration. Animations only write `UiTransform` /
//! colours while they are actually moving, so idle UI costs nothing.

use bevy::picking::hover::Hovered;
use bevy::prelude::*;

// Palette: warm cream paper, plum ink, pastel accents.
pub const CREAM: Color = Color::srgb(1.0, 0.973, 0.93);
pub const CREAM_GLASS: Color = Color::srgba(1.0, 0.97, 0.93, 0.9);
pub const CARD: Color = Color::srgba(1.0, 0.975, 0.955, 0.93);
pub const FIELD: Color = Color::srgb(1.0, 0.92, 0.95);
pub const INK: Color = Color::srgb(0.38, 0.22, 0.36);
pub const SOFT_INK: Color = Color::srgb(0.58, 0.44, 0.56);
pub const PINK: Color = Color::srgb(1.0, 0.55, 0.74);
pub const BERRY: Color = Color::srgb(0.86, 0.3, 0.55);
pub const LILAC: Color = Color::srgb(0.7, 0.6, 0.98);
pub const MINT: Color = Color::srgb(0.42, 0.8, 0.68);
pub const BUTTER: Color = Color::srgb(1.0, 0.86, 0.48);
pub const PEACH: Color = Color::srgb(1.0, 0.7, 0.58);
pub const ROSE_RED: Color = Color::srgb(0.88, 0.27, 0.36);
pub const GREY: Color = Color::srgb(0.66, 0.6, 0.68);
/// Thin outline around cream panels.
pub const LINE: Color = Color::srgba(0.86, 0.62, 0.76, 0.85);
/// Soft plum drop shadow.
pub const SHADOW: Color = Color::srgba(0.35, 0.15, 0.35, 0.28);

// Spacing and corner radius scale (px).
pub const S1: f32 = 4.0;
pub const S2: f32 = 8.0;
pub const S3: f32 = 12.0;
pub const S4: f32 = 18.0;
pub const R_CHIP: f32 = 12.0;
pub const R_PANEL: f32 = 22.0;
pub const R_PILL: f32 = 999.0;

/// The "E · Indigo journal" UI direction (Figma spec node 3:2918): solid deep
/// indigo cards with cream text, so the HUD reads over both the bright meadow
/// and dark interiors. Authored at 1280x720 logical pixels.
pub mod indigo {
    use bevy::prelude::Color;

    /// Surface / ink: HUD cards, journal cover, loading curtain. #30294E
    pub const INK: Color = Color::srgb(0.188, 0.161, 0.306);
    /// Text and glyphs on ink. #FFF8E7
    pub const CREAM: Color = Color::srgb(1.0, 0.973, 0.906);
    /// Expanded journal and settings backdrop. #EDF0FF
    pub const LAVENDER: Color = Color::srgb(0.929, 0.941, 1.0);
    /// Light controls and content cards. #F7F4FF
    pub const PAPER: Color = Color::srgb(0.969, 0.957, 1.0);
    /// Inset cover buttons, input fields, meter tracks. #51456D
    pub const RAISED: Color = Color::srgb_u8(0x51, 0x45, 0x6D);
    /// Quiet card and checkbox outlines. #7C709C
    pub const BORDER: Color = Color::srgb(0.486, 0.439, 0.612);
    /// Secondary text on ink, loading progress. #AABCF5
    pub const PERIWINKLE: Color = Color::srgb(0.667, 0.737, 0.961);
    /// Utility primary buttons, checked boxes, sliders, selected tabs. #3D4DA4
    pub const INDIGO: Color = Color::srgb(0.239, 0.302, 0.643);
    /// Warnings: attack, cat critical, speaker highlight, Host. #FFE174
    pub const BUTTER: Color = Color::srgb(1.0, 0.882, 0.455);
    /// Logo extrusion and world facets. #7660B8
    pub const PURPLE: Color = Color::srgb(0.463, 0.376, 0.722);

    /// Logical size the UI is authored at.
    pub const AUTHOR_W: f32 = 1280.0;
    pub const AUTHOR_H: f32 = 720.0;
    /// Gap between HUD cards and the screen edge.
    pub const EDGE: f32 = 24.0;

    /// Uniform UI scale for a window of `w` x `h` logical pixels: height / 720,
    /// but never wider than the window (so 16:10 and 4:3 don't overlap).
    pub fn ui_scale(w: f32, h: f32) -> f32 {
        if w <= 0.0 || h <= 0.0 {
            return 1.0;
        }
        (h / AUTHOR_H).min(w / AUTHOR_W)
    }
}

/// A soft drop shadow under a panel.
pub fn shadow(y: f32, blur: f32) -> BoxShadow {
    BoxShadow::new(SHADOW, px(0), px(y), px(0), px(blur))
}

/// Darker shade of a colour, for button lips and outlines.
pub fn shade(c: Color, by: f32) -> Color {
    c.mix(&INK, by)
}

/// easeOutBack: overshoots a little past 1 then settles (the "boing").
pub fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let c1 = 1.70158;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

/// Pop a UI node in: it scales up from `from` with a little bounce.
/// Restart it with [`Pop::restart`].
#[derive(Component, Clone, Copy, Debug)]
pub struct Pop {
    pub age: f32,
    pub from: f32,
    pub secs: f32,
}

impl Default for Pop {
    fn default() -> Self {
        Self {
            age: f32::INFINITY,
            from: 0.6,
            secs: 0.38,
        }
    }
}

impl Pop {
    pub fn now(from: f32) -> Self {
        Self {
            age: 0.0,
            from,
            ..default()
        }
    }
    pub fn restart(&mut self) {
        self.age = 0.0;
    }
    /// Scale `age` seconds into the pop.
    pub fn scale(&self) -> f32 {
        let t = self.age / self.secs;
        self.from + (1.0 - self.from) * ease_out_back(t)
    }
}

/// Restart the [`Pop`] whenever this node's `display` goes from none to shown.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct PopOnShow(pub bool);

/// Buttons squish: a little bigger on hover, smaller while pressed.
#[derive(Component, Clone, Copy, Debug)]
pub struct Squishy {
    pub pressed: bool,
    pub scale: f32,
}

impl Default for Squishy {
    fn default() -> Self {
        Self {
            pressed: false,
            scale: 1.0,
        }
    }
}

/// Gentle endless bob (loading dots, the little critter).
#[derive(Component, Clone, Copy, Debug)]
pub struct Bob {
    pub phase: f32,
    pub height: f32,
    pub speed: f32,
}

pub fn plugin(app: &mut App) {
    app.add_systems(Update, (pop_on_show, tick_pop, squish, bob).chain())
        .add_observer(press)
        .add_observer(release)
        .add_observer(leave);
}

fn tick_pop(time: Res<Time<Real>>, mut q: Query<(&mut Pop, &mut UiTransform)>) {
    let dt = time.delta_secs();
    for (mut pop, mut tf) in &mut q {
        if pop.age > pop.secs {
            continue;
        }
        pop.age += dt;
        let s = if pop.age > pop.secs { 1.0 } else { pop.scale() };
        tf.scale = Vec2::splat(s);
    }
}

fn pop_on_show(mut q: Query<(&Node, &mut Pop, &mut PopOnShow), Changed<Node>>) {
    for (node, mut pop, mut shown) in &mut q {
        let now = node.display != Display::None;
        if now && !shown.0 {
            *pop = Pop::now(0.88);
        }
        shown.0 = now;
    }
}

fn squish(
    time: Res<Time<Real>>,
    mut q: Query<(&mut Squishy, &Hovered, &mut UiTransform), Without<Pop>>,
) {
    let k = (time.delta_secs() * 18.0).min(1.0);
    for (mut sq, hovered, mut tf) in &mut q {
        let want = if sq.pressed {
            0.93
        } else if hovered.get() {
            1.06
        } else {
            1.0
        };
        if (sq.scale - want).abs() < 0.001 {
            continue;
        }
        sq.scale += (want - sq.scale) * k;
        if (sq.scale - want).abs() < 0.002 {
            sq.scale = want;
        }
        tf.scale = Vec2::splat(sq.scale);
    }
}

fn press(ev: On<PointerPress>, mut q: Query<&mut Squishy>) {
    if let Ok(mut s) = q.get_mut(ev.entity) {
        s.pressed = true;
    }
}

fn release(ev: On<PointerRelease>, mut q: Query<&mut Squishy>) {
    if let Ok(mut s) = q.get_mut(ev.entity) {
        s.pressed = false;
    }
}

fn leave(ev: On<PointerLeave>, mut q: Query<&mut Squishy>) {
    if let Ok(mut s) = q.get_mut(ev.entity) {
        s.pressed = false;
    }
}

fn bob(time: Res<Time<Real>>, mut q: Query<(&Bob, &mut UiTransform)>) {
    let t = time.elapsed_secs();
    for (b, mut tf) in &mut q {
        let y = -(((t * b.speed + b.phase).sin()).max(0.0)) * b.height;
        tf.translation = Val2::px(0.0, y);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pop_settles_at_one() {
        assert!((ease_out_back(1.0) - 1.0).abs() < 1e-5);
        assert!(ease_out_back(0.0).abs() < 1e-5);
        // overshoots a little on the way
        assert!(ease_out_back(0.7) > 1.0);
        let p = Pop::now(0.5);
        assert!((p.scale() - 0.5).abs() < 1e-5);
    }

    #[test]
    fn ui_scales_by_height_without_overflowing_width() {
        assert_eq!(indigo::ui_scale(1280.0, 720.0), 1.0);
        assert_eq!(indigo::ui_scale(1920.0, 1080.0), 1.5);
        // 16:10 is limited by its width.
        assert!((indigo::ui_scale(1440.0, 900.0) - 1.125).abs() < 1e-5);
        assert_eq!(indigo::ui_scale(0.0, 0.0), 1.0);
    }
}
