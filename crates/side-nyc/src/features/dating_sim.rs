//! Andrew dating sim (#462, from catherine: "surprise me").
//!
//! Click a computer terminal in the Fulton St station to go on a date with Andrew. It is a tiny visual novel: he says
//! something, you pick a reply (1-4), your answers add up to an affection
//! score, and the score decides how the date ends. Purely local: nothing is
//! sent to anyone. Esc walks away. The script is
//! `assets/data/side/dating_sim.ron`; the rules are pure functions
//! ([`Date::choose`], [`ending`]) so they are tested without a window.

use std::collections::BTreeMap;

use bevy::prelude::*;
use friendslop_base::areas::AreaApp as _;
use serde::Deserialize;

use crate::AppState;
use crate::features::fps::HandsBusy;

/// One thing you can say.
#[derive(Deserialize, Debug, Clone)]
pub struct Choice {
    pub text: String,
    pub affection: i32,
    pub next: String,
}

/// One beat of the date.
#[derive(Deserialize, Debug, Clone)]
pub struct Scene {
    pub line: String,
    pub choices: Vec<Choice>,
}

/// How it can end, best first.
#[derive(Deserialize, Debug, Clone)]
pub struct Ending {
    pub min: i32,
    pub title: String,
    pub text: String,
}

/// `data/side/dating_sim.ron` (read at run time).
#[derive(Deserialize, Default, Debug, Clone)]
pub struct Script {
    pub start: String,
    pub scenes: BTreeMap<String, Scene>,
    pub endings: Vec<Ending>,
}

pub static SCRIPT: friendslop_data::Data<Script> =
    friendslop_data::Data::ron("data/side/dating_sim.ron");

/// A date in progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Date {
    pub scene: String,
    pub affection: i32,
}

impl Date {
    pub fn new(script: &Script) -> Self {
        Self {
            scene: script.start.clone(),
            affection: 0,
        }
    }

    /// The scene you are in, if the script has it.
    pub fn current<'a>(&self, script: &'a Script) -> Option<&'a Scene> {
        script.scenes.get(&self.scene)
    }

    /// Over once the scene has nothing left to say or choose.
    pub fn over(&self, script: &Script) -> bool {
        self.current(script).is_none_or(|s| s.choices.is_empty())
    }

    /// Answer with choice `index`; false when there is no such choice.
    pub fn choose(&mut self, script: &Script, index: usize) -> bool {
        let Some(c) = self
            .current(script)
            .and_then(|s| s.choices.get(index))
            .cloned()
        else {
            return false;
        };
        self.affection += c.affection;
        self.scene = c.next;
        true
    }
}

/// The ending your affection earns: the first whose `min` it reaches.
pub fn ending(script: &Script, affection: i32) -> Option<&Ending> {
    script.endings.iter().find(|e| affection >= e.min)
}

/// What the screen shows for a date.
pub fn render(script: &Script, date: &Date) -> String {
    let hearts = |n: i32| "<3 ".repeat(n.clamp(0, 12) as usize);
    if date.over(script) {
        let e = ending(script, date.affection);
        return format!(
            "ANDREW DATING SIM  --  ENDING: {}\n\n{}\n\naffection {} {}\n\nEnter: go on another date, Esc: leave",
            e.map_or("?", |e| e.title.as_str()),
            e.map_or("", |e| e.text.as_str()),
            date.affection,
            hearts(date.affection)
        );
    }
    let Some(scene) = date.current(script) else {
        return String::new();
    };
    let mut out = format!(
        "ANDREW DATING SIM   affection {} {}\n\n{}\n",
        date.affection,
        hearts(date.affection),
        scene.line
    );
    for (i, c) in scene.choices.iter().enumerate() {
        out.push_str(&format!("\n  [{}] {}", i + 1, c.text));
    }
    out.push_str("\n\nEsc: leave");
    out
}

/// Local: a terminal was clicked; start a date.
#[derive(Event)]
pub struct StartDate;

/// The date you are on, if any (local).
#[derive(Resource, Default)]
struct Dating(Option<Date>);

#[derive(Component)]
struct DateText;

pub fn plugin(app: &mut App) {
    app.init_resource::<Dating>()
        .add_hud(spawn_ui)
        .add_observer(start_date)
        .add_systems(Update, date_play.run_if(in_state(AppState::InGame)))
        .add_systems(OnExit(AppState::InGame), stop);
}

fn spawn_ui(mut commands: Commands) {
    commands.spawn((
        Text::new(""),
        DateText,
        crate::hud::slot(crate::hud::HudSlot::CenterBanner),
        DespawnOnExit(AppState::InGame),
    ));
}

fn start_date(_: On<StartDate>, mut dating: ResMut<Dating>, mut busy: ResMut<HandsBusy>) {
    if dating.0.is_none() && !busy.0 {
        dating.0 = Some(Date::new(&SCRIPT.get()));
        busy.0 = true;
    }
}

fn stop(mut dating: ResMut<Dating>, mut busy: ResMut<HandsBusy>) {
    if dating.0.take().is_some() {
        busy.0 = false;
    }
}

const NUMBERS: [KeyCode; 4] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
];

fn date_play(
    keys: Res<ButtonInput<KeyCode>>,
    mut dating: ResMut<Dating>,
    mut busy: ResMut<HandsBusy>,
    mut text: Query<&mut Text, With<DateText>>,
) {
    let script = SCRIPT.get();
    if let Some(date) = dating.0.as_mut() {
        if keys.just_pressed(KeyCode::Escape) {
            dating.0 = None;
            busy.0 = false;
        } else if date.over(&script) {
            if keys.just_pressed(KeyCode::Enter) {
                *date = Date::new(&script);
            }
        } else if let Some(i) = NUMBERS.iter().position(|k| keys.just_pressed(*k)) {
            date.choose(&script, i);
        }
    }
    let want = dating
        .0
        .as_ref()
        .map(|d| render(&script, d))
        .unwrap_or_default();
    for mut t in &mut text {
        if t.0 != want {
            t.0 = want.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script() -> Script {
        SCRIPT.get().as_ref().clone()
    }

    #[test]
    fn the_script_is_complete() {
        let s = script();
        assert!(s.scenes.contains_key(&s.start));
        for (name, scene) in &s.scenes {
            for c in &scene.choices {
                assert!(s.scenes.contains_key(&c.next), "{name} -> {}", c.next);
            }
        }
        // Something always ends the date, and every ending can be reached.
        assert!(ending(&s, -99).is_some());
        assert!(s.endings.windows(2).all(|w| w[0].min > w[1].min));
    }

    #[test]
    fn every_path_ends_and_covers_every_ending() {
        let s = script();
        // Walk every path, collecting the affection each reaches the end with.
        fn walk(s: &Script, d: Date, depth: usize, out: &mut Vec<i32>) {
            assert!(depth < 40, "a loop in the script");
            if d.over(s) {
                out.push(d.affection);
                return;
            }
            for i in 0..d.current(s).map_or(0, |sc| sc.choices.len()) {
                let mut n = d.clone();
                n.choose(s, i);
                walk(s, n, depth + 1, out);
            }
        }
        let mut ends = vec![];
        walk(&s, Date::new(&s), 0, &mut ends);
        for e in &s.endings {
            let next = s
                .endings
                .iter()
                .filter(|o| o.min > e.min)
                .map(|o| o.min)
                .min();
            assert!(
                ends.iter()
                    .any(|a| *a >= e.min && next.is_none_or(|n| *a < n)),
                "no path earns {}",
                e.title
            );
        }
    }

    #[test]
    fn choosing_moves_the_date_along() {
        let s = script();
        let mut d = Date::new(&s);
        assert!(!d.choose(&s, 99));
        assert!(d.choose(&s, 0));
        assert_ne!(d.scene, s.start);
        assert!(render(&s, &d).contains("[1]"));
    }
}
