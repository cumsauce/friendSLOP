//! Actions, bindings and button names for every device.
//!
//! Every action the player can do is a string id (`"fart"`, `"duel_accept"`)
//! defined in one data file, `assets/data/base/bindings.ron`: its keyboard
//! key(s), its gamepad button(s) (or a slot on the action wheel), its label
//! and its group on the help screen. Adding an action is one entry there.
//!
//! - Code names an action with [`Act`] (`pub const FART: Act = Act("fart")`)
//!   and asks [`Actions::just_pressed`]; the pad presses the action's key, so
//!   older code reading `ButtonInput<KeyCode>` keeps working.
//! - Text shown to players writes `{fart}` where the button goes; [`fill`]
//!   turns it into "F", "X", "Square"… for the device used last
//!   ([`InputDevice`]). Toasts fill themselves.
//! - Contexts (`contexts:` in the file) rebind pad buttons while something is
//!   going on (a duel invite: A accepts, B declines). Features switch theirs
//!   on and off with [`InputContexts::set`]; the top one wins and its prompt
//!   bar shows what's live.

use std::sync::atomic::{AtomicU8, Ordering};

use bevy::ecs::system::SystemParam;
use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;
use serde::Deserialize;

/// An action, by its id in `bindings.ron`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Act(pub &'static str);

/// One action.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct ActionDef {
    pub id: String,
    /// What it does, for the help screen, the wheel and prompts ("fart").
    pub label: String,
    /// Keyboard keys (Bevy `KeyCode` names, e.g. "KeyF", "Space", "Digit1"),
    /// or "Mouse:Left" / "Mouse:Move". The first key is the one a pad presses.
    #[serde(default)]
    pub keys: Vec<String>,
    /// Gamepad buttons (Bevy `GamepadButton` names: "South", "RightTrigger2",
    /// "DPadUp", "Select", "LeftStick"…), or "Wheel" for the action wheel.
    /// Empty: only inside a context.
    #[serde(default)]
    pub pad: Vec<String>,
    /// Help-screen group ("" = hidden from help).
    #[serde(default)]
    pub group: String,
}

/// A context: pad buttons it takes over while active.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct ContextDef {
    pub id: String,
    /// (button, action id) pairs that win over the normal layout.
    #[serde(default)]
    pub binds: Vec<(String, String)>,
    /// Actions shown in the prompt bar while it's on top.
    #[serde(default)]
    pub prompt: Vec<String>,
}

/// `assets/data/base/bindings.ron`.
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Bindings {
    pub actions: Vec<ActionDef>,
    pub contexts: Vec<ContextDef>,
    /// The action wheel's pages, each up to 8 action ids clockwise from the top.
    pub wheel: Vec<Vec<String>>,
}

pub static BINDINGS: friendslop_data::Data<Bindings> =
    friendslop_data::Data::ron("data/base/bindings.ron");

impl Bindings {
    pub fn action(&self, id: &str) -> Option<&ActionDef> {
        self.actions.iter().find(|a| a.id == id)
    }
    pub fn context(&self, id: &str) -> Option<&ContextDef> {
        self.contexts.iter().find(|c| c.id == id)
    }
    /// Is the action on the wheel? (page, slot)
    pub fn wheel_slot(&self, id: &str) -> Option<(usize, usize)> {
        self.wheel
            .iter()
            .enumerate()
            .find_map(|(p, page)| page.iter().position(|a| a == id).map(|s| (p, s)))
    }
    /// The action a pad button does with `contexts` active (top last).
    pub fn pad_action(&self, button: GamepadButton, contexts: &[String]) -> Option<&str> {
        for c in contexts.iter().rev().filter_map(|c| self.context(c)) {
            if let Some((_, a)) = c.binds.iter().find(|(b, _)| pad_button(b) == Some(button)) {
                return Some(a);
            }
        }
        self.actions
            .iter()
            .find(|a| a.pad.iter().any(|b| pad_button(b) == Some(button)))
            .map(|a| a.id.as_str())
    }
    /// The pad buttons that do `id` with `contexts` active.
    pub fn pad_for(&self, id: &str, contexts: &[String]) -> Vec<GamepadButton> {
        let mut out: Vec<GamepadButton> = Vec::new();
        for c in contexts.iter().rev().filter_map(|c| self.context(c)) {
            out.extend(
                c.binds
                    .iter()
                    .filter(|(_, a)| a == id)
                    .filter_map(|(b, _)| pad_button(b)),
            );
        }
        if let Some(a) = self.action(id) {
            for b in a.pad.iter().filter_map(|b| pad_button(b)) {
                // Only if no context took that button for something else.
                if self.pad_action(b, contexts) == Some(id) && !out.contains(&b) {
                    out.push(b);
                }
            }
        }
        out
    }
    /// The keyboard keys for `id`.
    pub fn keys_for(&self, id: &str) -> Vec<KeyCode> {
        self.action(id)
            .map(|a| a.keys.iter().filter_map(|k| key_code(k)).collect())
            .unwrap_or_default()
    }
}

/// A Bevy `GamepadButton` by name.
pub fn pad_button(name: &str) -> Option<GamepadButton> {
    use GamepadButton::*;
    Some(match name {
        "South" => South,
        "East" => East,
        "West" => West,
        "North" => North,
        "LeftTrigger" => LeftTrigger,
        "RightTrigger" => RightTrigger,
        "LeftTrigger2" => LeftTrigger2,
        "RightTrigger2" => RightTrigger2,
        "Select" => Select,
        "Start" => Start,
        "LeftThumb" => LeftThumb,
        "RightThumb" => RightThumb,
        "DPadUp" => DPadUp,
        "DPadDown" => DPadDown,
        "DPadLeft" => DPadLeft,
        "DPadRight" => DPadRight,
        _ => return None,
    })
}

/// Pad "buttons" that aren't buttons: sticks and the wheel.
pub fn pad_special(name: &str) -> bool {
    matches!(name, "LeftStick" | "RightStick" | "Wheel")
}

/// A Bevy `KeyCode` by name (the keys the game uses).
pub fn key_code(name: &str) -> Option<KeyCode> {
    use KeyCode::*;
    if let Some(l) = name.strip_prefix("Key")
        && l.len() == 1
    {
        let c = l.chars().next()?;
        return [
            KeyA, KeyB, KeyC, KeyD, KeyE, KeyF, KeyG, KeyH, KeyI, KeyJ, KeyK, KeyL, KeyM, KeyN,
            KeyO, KeyP, KeyQ, KeyR, KeyS, KeyT, KeyU, KeyV, KeyW, KeyX, KeyY, KeyZ,
        ]
        .get((c as u8).checked_sub(b'A')? as usize)
        .copied();
    }
    if let Some(d) = name.strip_prefix("Digit")
        && let Ok(n) = d.parse::<usize>()
    {
        return [
            Digit0, Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9,
        ]
        .get(n)
        .copied();
    }
    Some(match name {
        "Space" => Space,
        "Tab" => Tab,
        "Escape" => Escape,
        "Enter" => Enter,
        "ShiftLeft" => ShiftLeft,
        "ShiftRight" => ShiftRight,
        "BracketLeft" => BracketLeft,
        "BracketRight" => BracketRight,
        "Minus" => Minus,
        "Equal" => Equal,
        "Semicolon" => Semicolon,
        "Quote" => Quote,
        "ArrowUp" => ArrowUp,
        "ArrowDown" => ArrowDown,
        "ArrowLeft" => ArrowLeft,
        "ArrowRight" => ArrowRight,
        "F3" => F3,
        _ => return None,
    })
}

/// A key's name as shown ("F", "Space", "LMB").
pub fn key_label(name: &str) -> String {
    if let Some(l) = name.strip_prefix("Key") {
        return l.to_string();
    }
    if let Some(d) = name.strip_prefix("Digit") {
        return d.to_string();
    }
    match name {
        "Mouse:Left" => "LMB",
        "Mouse:Move" => "Mouse",
        "Escape" => "Esc",
        "ShiftLeft" | "ShiftRight" => "Shift",
        "BracketLeft" => "[",
        "BracketRight" => "]",
        "Minus" => "-",
        "Equal" => "=",
        "Semicolon" => ";",
        "Quote" => "'",
        "ArrowUp" => "Up",
        "ArrowDown" => "Down",
        "ArrowLeft" => "Left",
        "ArrowRight" => "Right",
        other => other,
    }
    .to_string()
}

/// Which kind of pad, for button names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PadFamily {
    /// Xbox names (also every pad we don't know).
    #[default]
    Xbox,
    PlayStation,
    Nintendo,
    /// Steam Controller / Steam Deck (Xbox layout, Steam's names for Back/Start).
    Steam,
}

impl PadFamily {
    /// From the pad's USB vendor id.
    pub fn from_vendor(vendor: Option<u16>) -> Self {
        match vendor {
            Some(0x054c) => Self::PlayStation,
            Some(0x057e) => Self::Nintendo,
            Some(0x28de) => Self::Steam,
            _ => Self::Xbox,
        }
    }
    /// A saved name ("auto" is `None`).
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "xbox" => Some(Self::Xbox),
            "playstation" => Some(Self::PlayStation),
            "nintendo" => Some(Self::Nintendo),
            "steam" => Some(Self::Steam),
            _ => None,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Xbox => "xbox",
            Self::PlayStation => "playstation",
            Self::Nintendo => "nintendo",
            Self::Steam => "steam",
        }
    }
}

/// A pad button's name on this family.
pub fn button_label(b: GamepadButton, f: PadFamily) -> &'static str {
    use GamepadButton::*;
    use PadFamily::*;
    match (b, f) {
        (South, PlayStation) => "×",
        (East, PlayStation) => "O",
        (West, PlayStation) => "Square",
        (North, PlayStation) => "Triangle",
        (LeftTrigger, PlayStation) => "L1",
        (RightTrigger, PlayStation) => "R1",
        (LeftTrigger2, PlayStation) => "L2",
        (RightTrigger2, PlayStation) => "R2",
        (Select, PlayStation) => "Create",
        (Start, PlayStation) => "Options",
        (LeftThumb, PlayStation) => "L3",
        (RightThumb, PlayStation) => "R3",
        // Nintendo's letters sit swapped: the bottom button is B.
        (South, Nintendo) => "B",
        (East, Nintendo) => "A",
        (West, Nintendo) => "Y",
        (North, Nintendo) => "X",
        (LeftTrigger, Nintendo) => "L",
        (RightTrigger, Nintendo) => "R",
        (LeftTrigger2, Nintendo) => "ZL",
        (RightTrigger2, Nintendo) => "ZR",
        (Select, Nintendo) => "-",
        (Start, Nintendo) => "+",
        (Select, Steam) => "Back",
        (Start, Steam) => "Start",
        (South, _) => "A",
        (East, _) => "B",
        (West, _) => "X",
        (North, _) => "Y",
        (LeftTrigger, _) => "LB",
        (RightTrigger, _) => "RB",
        (LeftTrigger2, _) => "LT",
        (RightTrigger2, _) => "RT",
        (Select, _) => "View",
        (Start, _) => "Menu",
        (LeftThumb, _) => "LS",
        (RightThumb, _) => "RS",
        (DPadUp, _) => "D-pad up",
        (DPadDown, _) => "D-pad down",
        (DPadLeft, _) => "D-pad left",
        (DPadRight, _) => "D-pad right",
        _ => "?",
    }
}

/// What the player used last.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum InputDevice {
    #[default]
    KeyboardMouse,
    Pad(PadFamily),
    Touch,
}

impl InputDevice {
    fn code(self) -> u8 {
        match self {
            Self::KeyboardMouse => 0,
            Self::Touch => 1,
            Self::Pad(PadFamily::Xbox) => 2,
            Self::Pad(PadFamily::PlayStation) => 3,
            Self::Pad(PadFamily::Nintendo) => 4,
            Self::Pad(PadFamily::Steam) => 5,
        }
    }
    fn from_code(c: u8) -> Self {
        match c {
            1 => Self::Touch,
            2 => Self::Pad(PadFamily::Xbox),
            3 => Self::Pad(PadFamily::PlayStation),
            4 => Self::Pad(PadFamily::Nintendo),
            5 => Self::Pad(PadFamily::Steam),
            _ => Self::KeyboardMouse,
        }
    }
    pub fn is_pad(self) -> bool {
        matches!(self, Self::Pad(_))
    }
}

/// The device, readable without a world (toasts fill themselves).
static DEVICE: AtomicU8 = AtomicU8::new(0);
/// The active contexts, readable without a world (for [`fill`]).
static CONTEXTS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// The device used last (see [`InputDevice`]).
pub fn device() -> InputDevice {
    InputDevice::from_code(DEVICE.load(Ordering::Relaxed))
}

/// Set by the game's input tracking (and `FRIENDSLOP_INPUT` in screenshots).
pub fn set_device(d: InputDevice) {
    DEVICE.store(d.code(), Ordering::Relaxed);
}

fn contexts_now() -> Vec<String> {
    CONTEXTS.lock().map(|c| c.clone()).unwrap_or_default()
}

/// How action `id` is done on `device` with `contexts` active: "F",
/// "Space / Shift", "A", "RB wheel: brag".
pub fn label_with(b: &Bindings, id: &str, device: InputDevice, contexts: &[String]) -> String {
    let Some(a) = b.action(id) else {
        return id.to_string();
    };
    match device {
        InputDevice::Pad(f) => {
            let buttons = b.pad_for(id, contexts);
            if !buttons.is_empty() {
                return buttons
                    .iter()
                    .map(|x| button_label(*x, f))
                    .collect::<Vec<_>>()
                    .join(" / ");
            }
            if let Some(s) = a.pad.iter().find(|p| pad_special(p)) {
                return match s.as_str() {
                    "LeftStick" => "Left stick".into(),
                    "RightStick" => "Right stick".into(),
                    _ => format!("{} wheel", button_label(GamepadButton::RightTrigger, f)),
                };
            }
            if b.wheel_slot(id).is_some() {
                return format!("{} wheel", button_label(GamepadButton::RightTrigger, f));
            }
            key_names(a)
        }
        InputDevice::Touch => a.label.clone(),
        InputDevice::KeyboardMouse => key_names(a),
    }
}

fn key_names(a: &ActionDef) -> String {
    if a.keys.len() == 4 && a.keys.iter().all(|k| k.len() == 4 && k.starts_with("Key")) {
        // A movement cluster (WASD).
        return a.keys.iter().map(|k| key_label(k)).collect();
    }
    a.keys
        .iter()
        .map(|k| key_label(k))
        .collect::<Vec<_>>()
        .join(" / ")
}

/// [`label_with`] for the current device and contexts.
pub fn label(id: &str) -> String {
    label_with(&BINDINGS.get(), id, device(), &contexts_now())
}

/// Replace every `{action_id}` in `text` with its button for `device`.
/// Unknown ids and other braces are left as they are.
pub fn fill_with(text: &str, b: &Bindings, device: InputDevice, contexts: &[String]) -> String {
    if !text.contains('{') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('}') {
            Some(j)
                if after[..j]
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    && b.action(&after[..j]).is_some() =>
            {
                out.push_str(&label_with(b, &after[..j], device, contexts));
                rest = &after[j + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// [`fill_with`] for the current device and contexts.
pub fn fill(text: &str) -> String {
    fill_with(text, &BINDINGS.get(), device(), &contexts_now())
}

/// Every binding for help screens, for `device`: (button(s), what it does),
/// in `bindings.ron`'s order; on a pad, wheel actions are listed as the wheel.
pub fn help_rows(device: InputDevice) -> Vec<(String, String)> {
    let b = BINDINGS.get();
    let none: Vec<String> = Vec::new();
    let pad = device.is_pad();
    let mut out: Vec<(String, String)> = b
        .actions
        .iter()
        .filter(|a| !a.group.is_empty())
        .filter(|a| {
            !pad || (b.wheel_slot(&a.id).is_none()
                && (!b.pad_for(&a.id, &none).is_empty() || a.pad.iter().any(|p| pad_special(p))))
        })
        .map(|a| (label_with(&b, &a.id, device, &none), a.label.clone()))
        .collect();
    if let InputDevice::Pad(f) = device {
        let rb = button_label(GamepadButton::RightTrigger, f);
        for (i, page) in b.wheel.iter().enumerate() {
            let names: Vec<&str> = page
                .iter()
                .filter_map(|a| b.action(a).map(|d| d.label.as_str()))
                .collect();
            let key = if i == 0 {
                format!("{rb} wheel")
            } else {
                format!("{rb} wheel p{}", i + 1)
            };
            out.push((key, names.join(", ")));
        }
        out.push((
            "In menus".into(),
            "D-pad / stick move, A press, B back".into(),
        ));
    }
    out
}

/// [`help_rows`] as text, one binding per line.
pub fn help_text() -> String {
    help_rows(device())
        .iter()
        .map(|(k, v)| format!("{k:>14}   {v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A `Text` whose `{action}` tokens follow the device: put `Hint(template)`
/// next to `Text` and it's filled now and whenever the device changes.
/// The template `{@help}` is the whole help table.
#[derive(Component, Clone, Debug)]
pub struct Hint(pub String);

impl Hint {
    pub fn text(&self) -> String {
        if self.0 == "{@help}" {
            help_text()
        } else {
            fill(&self.0)
        }
    }
}

fn refresh_hints(
    device: Res<InputDevice>,
    contexts: Res<InputContexts>,
    mut all: Query<(Ref<Hint>, &mut Text)>,
) {
    let every = device.is_changed() || contexts.is_changed();
    for (h, mut t) in &mut all {
        if every || h.is_changed() {
            let want = h.text();
            if t.0 != want {
                t.0 = want;
            }
        }
    }
}

/// Contexts active this frame, in the order they were switched on.
#[derive(Resource, Default, Debug, Clone)]
pub struct InputContexts(pub Vec<String>);

impl InputContexts {
    /// Switch context `id` on or off (cheap to call every frame).
    pub fn set(&mut self, id: &str, on: bool) {
        let has = self.0.iter().any(|c| c == id);
        if on && !has {
            self.0.push(id.to_string());
        } else if !on && has {
            self.0.retain(|c| c != id);
        }
    }
    /// Is `id` on?
    pub fn has(&self, id: &str) -> bool {
        self.0.iter().any(|c| c == id)
    }
    /// The one on top (its prompt bar shows).
    pub fn top(&self) -> Option<&str> {
        self.0.last().map(String::as_str)
    }
}

/// Switch context `id` on or off from a system, marking the resource changed
/// only when it really changes (call it every frame with "is it going on?").
pub fn switch(ctx: &mut ResMut<InputContexts>, id: &str, on: bool) {
    if ctx.has(id) != on {
        ctx.set(id, on);
    }
}

fn publish_contexts(c: Res<InputContexts>) {
    if c.is_changed()
        && let Ok(mut g) = CONTEXTS.lock()
    {
        g.clone_from(&c.0);
    }
}

/// Read actions instead of keys: `actions.just_pressed(FART)`.
#[derive(SystemParam)]
pub struct Actions<'w> {
    keys: Option<Res<'w, ButtonInput<KeyCode>>>,
    mouse: Option<Res<'w, ButtonInput<MouseButton>>>,
}

impl Actions<'_> {
    fn any(
        &self,
        act: Act,
        f: impl Fn(&ButtonInput<KeyCode>, KeyCode) -> bool,
        m: impl Fn(&ButtonInput<MouseButton>) -> bool,
    ) -> bool {
        let b = BINDINGS.get();
        let Some(a) = b.action(act.0) else {
            return false;
        };
        a.keys.iter().any(|k| {
            if k == "Mouse:Left" {
                return self.mouse.as_ref().is_some_and(|ms| m(ms));
            }
            match (key_code(k), &self.keys) {
                (Some(c), Some(keys)) => f(keys, c),
                _ => false,
            }
        })
    }
    pub fn just_pressed(&self, act: Act) -> bool {
        self.any(
            act,
            |k, c| k.just_pressed(c),
            |m| m.just_pressed(MouseButton::Left),
        )
    }
    pub fn pressed(&self, act: Act) -> bool {
        self.any(act, |k, c| k.pressed(c), |m| m.pressed(MouseButton::Left))
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<InputContexts>()
        .init_resource::<InputDevice>()
        .add_systems(PostUpdate, refresh_hints)
        .add_systems(Last, publish_contexts);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b() -> std::sync::Arc<Bindings> {
        let path = format!(
            "{}/../game/assets/{}",
            env!("CARGO_MANIFEST_DIR"),
            BINDINGS.path()
        );
        let Ok(bytes) = std::fs::read(&path) else {
            panic!("no {path}")
        };
        assert!(BINDINGS.set_bytes(&bytes), "{path} doesn't parse");
        BINDINGS.get()
    }

    /// Every `{id}` a hint asks for (in `fill(..)`, `Hint(..)`, `{{id}}` in a
    /// `format!` that gets filled, and the data files) and every `Act("id")` is an
    /// action bindings.ron defines.
    #[test]
    fn every_hint_token_is_a_real_action() {
        let b = b();
        let root = format!("{}/../..", env!("CARGO_MANIFEST_DIR"));
        let mut bad = Vec::new();
        let mut files = Vec::new();
        let mut dirs = vec![std::path::PathBuf::from(format!("{root}/crates"))];
        while let Some(d) = dirs.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if p.is_dir() {
                    if name != "target" && !name.starts_with('.') {
                        dirs.push(p);
                    }
                } else if name.ends_with(".rs") || name.ends_with(".ron") {
                    files.push(p);
                }
            }
        }
        for p in files {
            if p.ends_with("base/src/input.rs") {
                continue;
            }
            let Ok(s) = std::fs::read_to_string(&p) else {
                continue;
            };
            let ron = p.extension().is_some_and(|e| e == "ron");
            if !ron && !s.contains("input::fill") && !s.contains("Hint(") && !s.contains("Act(\"") {
                continue;
            }
            for (i, _) in s.match_indices('{') {
                let rest = &s[i + 1..];
                let double = rest.starts_with('{');
                let body = if double { &rest[1..] } else { rest };
                let Some(end) = body.find('}') else { continue };
                let id = &body[..end];
                if id.is_empty()
                    || !id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
                {
                    continue;
                }
                // Only tokens that name an action-ish word: escaped ones in .rs,
                // or any in a line that a hint filler sees.
                let line_start = s[..i].rfind('\n').map_or(0, |n| n + 1);
                let line = &s[line_start..s[i..].find('\n').map_or(s.len(), |n| i + n)];
                let hinted = (double && !ron)
                    || (!double && (line.contains("fill(\"") || line.contains("Hint(\"")))
                    || (ron && b.action(id).is_some());
                if hinted && b.action(id).is_none() {
                    bad.push(format!("{}: {{{id}}}", p.display()));
                }
            }
            for (i, _) in s.match_indices("Act(\"") {
                let rest = &s[i + 5..];
                let Some(end) = rest.find('"') else { continue };
                let id = &rest[..end];
                if b.action(id).is_none() && id != "id" {
                    bad.push(format!("{}: Act({id})", p.display()));
                }
            }
        }
        assert!(bad.is_empty(), "unknown actions: {bad:#?}");
    }

    #[test]
    fn the_bindings_file_parses_and_every_name_is_real() {
        let b = b();
        let mut ids = std::collections::HashSet::new();
        for a in &b.actions {
            assert!(ids.insert(a.id.as_str()), "{} twice", a.id);
            assert!(!a.label.is_empty(), "{}", a.id);
            for k in &a.keys {
                assert!(
                    key_code(k).is_some() || k.starts_with("Mouse:"),
                    "{}: no key {k}",
                    a.id
                );
            }
            for p in &a.pad {
                assert!(
                    pad_button(p).is_some() || pad_special(p),
                    "{}: no button {p}",
                    a.id
                );
            }
        }
        for c in &b.contexts {
            for (button, act) in &c.binds {
                assert!(pad_button(button).is_some(), "{}: no button {button}", c.id);
                assert!(b.action(act).is_some(), "{}: no action {act}", c.id);
            }
            for p in &c.prompt {
                assert!(b.action(p).is_some(), "{}: no action {p}", c.id);
            }
        }
        for page in &b.wheel {
            assert!(page.len() <= 8);
            for a in page {
                assert!(b.action(a).is_some(), "wheel: no action {a}");
            }
        }
        // No two actions share a pad button in the base layout.
        let mut seen = std::collections::HashMap::new();
        for a in &b.actions {
            for p in a.pad.iter().filter(|p| pad_button(p).is_some()) {
                assert!(
                    seen.insert(p.clone(), a.id.clone()).is_none(),
                    "{p} bound twice"
                );
            }
        }
    }

    #[test]
    fn labels_follow_the_device_and_contexts() {
        let b = b();
        let none: Vec<String> = Vec::new();
        assert_eq!(
            label_with(&b, "fart", InputDevice::KeyboardMouse, &none),
            "F"
        );
        assert_eq!(
            label_with(&b, "jump", InputDevice::Pad(PadFamily::Xbox), &none),
            "A"
        );
        assert_eq!(
            label_with(&b, "jump", InputDevice::Pad(PadFamily::PlayStation), &none),
            "×"
        );
        assert_eq!(
            label_with(&b, "jump", InputDevice::Pad(PadFamily::Nintendo), &none),
            "B"
        );
        assert_eq!(
            label_with(&b, "move", InputDevice::KeyboardMouse, &none),
            "WASD"
        );
        assert!(label_with(&b, "brag", InputDevice::Pad(PadFamily::Xbox), &none).contains("wheel"));
        // A duel invite takes A over for accepting.
        let duel = vec!["duel_invite".to_string()];
        assert_eq!(
            label_with(&b, "duel_accept", InputDevice::Pad(PadFamily::Xbox), &duel),
            "A"
        );
        assert_eq!(
            b.pad_action(GamepadButton::South, &duel),
            Some("duel_accept")
        );
        assert_eq!(b.pad_action(GamepadButton::South, &none), Some("jump"));
    }

    #[test]
    fn fill_swaps_tokens_and_leaves_the_rest() {
        let b = b();
        let none: Vec<String> = Vec::new();
        assert_eq!(
            fill_with(
                "{fart} to fart, {nope} {",
                &b,
                InputDevice::KeyboardMouse,
                &none
            ),
            "F to fart, {nope} {"
        );
        assert_eq!(
            fill_with(
                "${damage} {use}",
                &b,
                InputDevice::Pad(PadFamily::Xbox),
                &none
            ),
            "${damage} B / RT"
        );
    }

    #[test]
    fn pad_families_from_vendors() {
        assert_eq!(PadFamily::from_vendor(Some(0x054c)), PadFamily::PlayStation);
        assert_eq!(PadFamily::from_vendor(Some(0x28de)), PadFamily::Steam);
        assert_eq!(PadFamily::from_vendor(None), PadFamily::Xbox);
        assert_eq!(key_code("KeyF"), Some(KeyCode::KeyF));
        assert_eq!(key_code("Digit3"), Some(KeyCode::Digit3));
    }
}
