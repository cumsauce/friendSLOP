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

/// The "commune noticeboard" kit (the UI restyle): soot panels, cardboard cards held on with
/// tape, one slime-green action, butter focus, landlord red for danger only. Colours, the
/// type scale and the sizes the menus and the HUD are built from. Authored at 1280x800 (the
/// Steam Deck) and scaled uniformly, so 1920x1080 is the same layout at x1.35.
pub mod kit {
    use bevy::prelude::Color;

    /// Screen scrims, the deepest background. #17130F
    pub const SOOT_900: Color = Color::srgb_u8(0x17, 0x13, 0x0F);
    /// Panels. #241E18
    pub const SOOT_800: Color = Color::srgb_u8(0x24, 0x1E, 0x18);
    /// Raised rows, empty slots, tracks. #352C23
    pub const SOOT_700: Color = Color::srgb_u8(0x35, 0x2C, 0x23);
    /// Borders and dividers. #5A4C3E
    pub const SOOT_500: Color = Color::srgb_u8(0x5A, 0x4C, 0x3E);
    /// Cardboard card face. #F3E6CC
    pub const CARD_100: Color = Color::srgb_u8(0xF3, 0xE6, 0xCC);
    /// Cardboard, secondary cards. #D9B98A
    pub const CARD_300: Color = Color::srgb_u8(0xD9, 0xB9, 0x8A);
    /// Cardboard shade layer. #A97E4B
    pub const CARD_500: Color = Color::srgb_u8(0xA9, 0x7E, 0x4B);
    /// Tape strips. #EDE4C8
    pub const TAPE_200: Color = Color::srgb_u8(0xED, 0xE4, 0xC8);
    /// The one thing to press, progress, OK. #A6D62E
    pub const SLOP_400: Color = Color::srgb_u8(0xA6, 0xD6, 0x2E);
    /// Slime shadow layer. #5F8A12
    pub const SLOP_600: Color = Color::srgb_u8(0x5F, 0x8A, 0x12);
    /// Focus ring, highlights, warnings. #FFD24A
    pub const BUTTER_400: Color = Color::srgb_u8(0xFF, 0xD2, 0x4A);
    /// Danger, the landlord, hell, leave. #E5483B
    pub const RENT_500: Color = Color::srgb_u8(0xE5, 0x48, 0x3B);
    /// Danger shadow layer. #8E241C
    pub const RENT_700: Color = Color::srgb_u8(0x8E, 0x24, 0x1C);
    /// Heaven, the cat, friends talking. #A99BF0
    pub const LILAC_400: Color = Color::srgb_u8(0xA9, 0x9B, 0xF0);
    /// Heaven light, info. #9FD4F2
    pub const SKY_300: Color = Color::srgb_u8(0x9F, 0xD4, 0xF2);
    /// Text on soot. #F6EEDC
    pub const TEXT: Color = Color::srgb_u8(0xF6, 0xEE, 0xDC);
    /// Secondary text on soot. #BBA98E
    pub const TEXT_DIM: Color = Color::srgb_u8(0xBB, 0xA9, 0x8E);
    /// Text on cardboard, slime and butter. #1E1813
    pub const INK: Color = Color::srgb_u8(0x1E, 0x18, 0x13);

    /// One colour per seat, so a friend is the same colour everywhere.
    pub const SEATS: [Color; 6] = [
        SLOP_400,
        Color::srgb_u8(0xFF, 0x8F, 0xB8),
        BUTTER_400,
        SKY_300,
        Color::srgb_u8(0xFF, 0x9A, 0x4A),
        LILAC_400,
    ];

    /// The colour of seat `i` (wraps past six).
    pub fn seat(i: usize) -> Color {
        SEATS[i % SEATS.len()]
    }

    /// The type scale, in logical px at 1280x800 (Deck): `label` is the smallest text
    /// anywhere, `body_reg` the size for reading.
    pub mod size {
        /// Logo words, YOU DIED, chapter numbers (Bowlby One).
        pub const DISPLAY: f32 = 60.0;
        /// Screen headings (Bowlby One).
        pub const H1: f32 = 34.0;
        /// Card titles and tabs (Lilita One).
        pub const H2: f32 = 28.0;
        /// Buttons and rows (Atkinson Hyperlegible Bold).
        pub const BODY: f32 = 22.0;
        /// Explanations and toasts (Atkinson Hyperlegible Regular).
        pub const BODY_REG: f32 = 20.0;
        /// Chips, hints and captions, the Deck minimum (Atkinson Hyperlegible Bold).
        pub const LABEL: f32 = 17.0;
        /// Timers, money, counts (Lilita One).
        pub const NUM: f32 = 30.0;
        /// Stamps and chip titles (Bowlby One).
        pub const STAMP: f32 = 22.0;
    }

    /// Gap between anything important and the screen edge.
    pub const EDGE: f32 = 24.0;
    /// Button height.
    pub const BUTTON_H: f32 = 56.0;
    /// Button corner radius.
    pub const R_BUTTON: f32 = 10.0;
    /// Panel corner radius.
    pub const R_PANEL: f32 = 14.0;
    /// Focus ring thickness.
    pub const FOCUS_RING: f32 = 4.0;
    /// How far a button's lip (its shade layer) sticks out below it.
    pub const LIP: f32 = 5.0;

    /// Height the UI is authored at (the Deck's 800).
    pub const AUTHOR_H: f32 = 800.0;
    /// Width the UI is authored at.
    pub const AUTHOR_W: f32 = 1280.0;

    /// Uniform UI scale for a window of `w` x `h` logical px: height / 800, but never
    /// wider than the window (16:10 is 1, 1920x1080 is 1.35, 1280x720 is 0.9).
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
    fn kit_scales_the_deck_to_one_and_a_tv_to_one_point_three_five() {
        assert_eq!(kit::ui_scale(1280.0, 800.0), 1.0);
        assert!((kit::ui_scale(1920.0, 1080.0) - 1.35).abs() < 1e-5);
        assert!((kit::ui_scale(1280.0, 720.0) - 0.9).abs() < 1e-5);
        assert_eq!(kit::ui_scale(0.0, 5.0), 1.0);
    }

    #[test]
    fn seats_wrap_after_six() {
        assert_eq!(kit::seat(0), kit::seat(6));
        assert_ne!(kit::seat(0), kit::seat(1));
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
