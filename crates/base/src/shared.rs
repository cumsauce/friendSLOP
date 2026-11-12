//! Slices of the game's top-level modules that features read: the types
//! live here so features don't depend on the game crate. The game's own
//! modules (`world`, `camera`, ...) re-export them, so paths are unchanged.

/// From the game's `world.rs`.
pub mod world {
    use bevy::prelude::*;

    /// How often HUD text is rebuilt (5 Hz, on real time so it keeps updating
    /// while the world stands still). Formatting strings every frame is waste.
    pub const HUD_PERIOD: std::time::Duration = std::time::Duration::from_millis(200);

    /// Run condition for HUD update systems: true every `HUD_PERIOD`.
    pub fn hud_tick() -> impl FnMut(Res<Time<Real>>) -> bool + Clone {
        bevy::time::common_conditions::on_real_timer(HUD_PERIOD)
    }

    #[derive(Component)]
    pub struct MainCamera;
}

/// From the game's `camera.rs`: the overhead camera's target.
pub mod camera {
    use bevy::prelude::*;

    /// Half the walkable world (the play area is 180 m square, see terrain.rs).
    pub const FLOOR_HALF: f32 = crate::terrain::PLAY_HALF;

    /// How close to the floor edge the focus may go.
    pub const EDGE_MARGIN: f32 = 2.0;

    pub const MIN_DIST: f32 = 6.0;

    pub const MAX_DIST: f32 = 38.0;

    /// Fraction of the distance each scroll line zooms.
    pub const ZOOM_STEP: f32 = 0.1;

    /// How far above the focus the camera aims (keeps some sky in view).
    pub const LOOK_LIFT: f32 = 2.5;

    /// Where the camera wants to be.
    #[derive(Resource, Clone, Copy, Debug, PartialEq)]
    pub struct CameraRig {
        pub focus: Vec3,
        pub distance: f32,
    }

    impl Default for CameraRig {
        fn default() -> Self {
            Self {
                focus: Vec3::ZERO,
                distance: view_offset().length(),
            }
        }
    }

    /// The original fixed view direction.
    pub fn view_offset() -> Vec3 {
        Vec3::new(0.0, 5.5, 17.0)
    }

    impl CameraRig {
        /// Move the focus by `delta` (world XZ) and keep it on the floor.
        pub fn pan(&mut self, delta: Vec2) {
            let lim = FLOOR_HALF - EDGE_MARGIN;
            self.focus.x = (self.focus.x + delta.x).clamp(-lim, lim);
            self.focus.z = (self.focus.z + delta.y).clamp(-lim, lim);
            self.focus.y = crate::terrain::height_at(self.focus.x, self.focus.z);
        }

        /// Zoom in (positive) or out (negative) by scroll lines.
        pub fn zoom(&mut self, lines: f32) {
            self.distance = (self.distance * (1.0 - lines * ZOOM_STEP)).clamp(MIN_DIST, MAX_DIST);
        }

        pub fn transform(&self) -> Transform {
            let eye = self.focus + view_offset().normalize() * self.distance;
            // Aim a little above the floor so the horizon and sky stay in frame.
            Transform::from_translation(eye).looking_at(self.focus + Vec3::Y * LOOK_LIFT, Vec3::Y)
        }
    }
}

/// From the game's `controls.rs`.
pub mod controls {
    use bevy::prelude::*;

    /// Movement and look from touch / gamepad this frame, read by `fps.rs`.
    #[derive(Resource, Default, Debug, Clone, Copy)]
    pub struct VirtualInput {
        /// -1..=1 each way, `y` forward.
        pub stick: Vec2,
        /// Radians to turn this frame (`x` yaw right, `y` pitch down).
        pub look: Vec2,
        pub jump: bool,
        pub run: bool,
    }
}

/// From the game's `ui/`.
pub mod ui {
    use bevy::prelude::*;

    /// The HUD's small white glyphs (`assets/ui/*.png`, exported from the Figma
    /// kit), tinted per use. A missing file just draws nothing: every glyph has
    /// text beside it that says the same thing.
    #[derive(Resource, Clone, Default)]
    pub struct Icons {
        pub mic: Handle<Image>,
        pub mic_slash: Handle<Image>,
        pub tear: Handle<Image>,
        pub gas: Handle<Image>,
        pub cat: Handle<Image>,
        pub warn: Handle<Image>,
        pub target: Handle<Image>,
        pub hand: Handle<Image>,
    }

    /// Text in the UI face (Nunito Bold) once it has loaded.
    #[derive(Component, Clone, Copy, Debug, Default)]
    pub struct Body;

    /// Text in the display face (Fredoka SemiBold, or Nunito ExtraBold when the
    /// Fredoka file is missing) once it has loaded.
    #[derive(Component, Clone, Copy, Debug, Default)]
    pub struct Head;

    /// UI-face text: `size` logical px, bold, in `color`.
    pub fn body(size: f32, color: Color) -> impl Bundle {
        (
            TextFont {
                font_size: FontSize::Px(size),
                weight: FontWeight(700),
                ..default()
            },
            TextColor(color),
            Body,
        )
    }

    /// Which big overlays are up (as of the end of the last frame), so Esc and
    /// clicks go to them instead of leaving the game or the world.
    #[derive(Resource, Default, Debug, Clone, Copy)]
    pub struct Overlay {
        pub journal: bool,
        pub intro: bool,
    }

    impl Overlay {
        pub fn any(self) -> bool {
            self.journal || self.intro
        }
    }

    /// Where the lane goes: the danger mount in the top-centre slot column,
    /// so features' top-centre lines flow in under it.
    #[derive(Component)]
    pub struct DangerMount;

    /// Where the stack goes: the mount in the bottom-right slot column, so
    /// features' bottom-right lines stack up above it.
    #[derive(Component)]
    pub struct StatusMount;
}

/// From the game's `zones.rs`.
pub mod zones {
    use bevy::prelude::*;

    /// Nothing goes past this half-extent.
    pub const MAP_HALF: f32 = 88.0;

    /// The commune's own dressing (scenery.rs) covers this half-extent.
    pub const COMMUNE_HALF: f32 = 27.0;

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Zone {
        Commune,
        Village,
        Forest,
        Ruins,
        Valley,
        Meadow,
        Outside,
    }

    pub fn zone_of(p: Vec2) -> Zone {
        if p.x.abs() > MAP_HALF || p.y.abs() > MAP_HALF {
            Zone::Outside
        } else if p.x.abs() < COMMUNE_HALF && p.y.abs() < COMMUNE_HALF {
            Zone::Commune
        } else if (33.0..82.0).contains(&p.x) && (-37.0..37.0).contains(&p.y) {
            Zone::Village
        } else if p.y < -35.0 {
            Zone::Forest
        } else if p.x < -30.0 && p.y > 30.0 {
            Zone::Ruins
        } else if p.x < -35.0 && p.y.abs() <= 30.0 {
            Zone::Valley
        } else {
            Zone::Meadow
        }
    }
}

/// From the game's `scenery.rs`.
pub mod scenery {
    use bevy::prelude::*;

    /// Where the commune keeps its campfire, beside the ruins and Dave.
    pub const BONFIRE_AT: Vec3 = Vec3::new(
        friendslop_world::places::BONFIRE_AT.x,
        0.0,
        friendslop_world::places::BONFIRE_AT.y,
    );
}
