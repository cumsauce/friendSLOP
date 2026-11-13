//! The composer and music director: decides, one bar at a time, what the
//! band plays next.
//!
//! The game only sets a target [`Mood`], an intensity and a victory
//! counter. At every bar line the director looks at them and picks the next
//! measures: keep the section going, plan the next one (Intro, A groove,
//! Hook, B bridge, Solo, Breakdown) with a turnaround and maybe a key change,
//! or play a one-bar transition (drum fill over an approach chord) into a
//! new mood. Nothing ever changes mid-bar.
//!
//! The writing: rootless, voice-led EP voicings over ii-V-Is, tritone subs,
//! modal interchange and lydian colours; slap-bass templates with octave
//! pops, ghosts and chromatic approaches; 16th-note kits with ghost notes
//! and fills; a lead that states a motif, answers it, varies it
//! (sequence, displacement, inversion, fragmentation) and cadences, landing
//! on chord tones on strong beats with chromatic approach tones between;
//! bebop and pentatonic runs in solos; unison kimes at section ends and the
//! odd 7/8 bar.

use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

use crate::synth::{BUSES, Bus, Ev, Inst, RATE};
use crate::theory::{Chord, Quality, Quality::*, Voicing, voice_lead};

mod bass;
mod drums;
mod form;
mod harmony;
mod humanize;
mod melody;
mod picks;
mod score;

use form::*;
use melody::*;
use picks::pk;
use score::*;

pub use form::LIFT_MAX;
pub use score::{SCORE, Score};

/// What the game wants the music to say.
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    Default,
    serde::Serialize,
    serde::Deserialize,
    PartialOrd,
    Ord,
)]
pub enum Mood {
    /// The title screen: the signature theme.
    #[default]
    Menu,
    /// Wandering the meadows: laid back.
    Explore,
    /// Commune and village: warm city-pop.
    Village,
    /// Forest and valley: airy, flute and harp.
    Forest,
    /// The maze and the ruins: mysterious minor.
    Maze,
    /// Up on the sky islands: dreamy bells.
    Sky,
    /// With Abuela in heaven: a gentle bolero.
    Abuela,
    /// Hide and seek, hiding: tense, muted, ticking.
    Hiding,
    /// Hide and seek, seeking: driving.
    Seeking,
    /// A duel: double-time fusion shred.
    Duel,
    /// The landlord and his rent: dissonant, building.
    Landlord,
    /// The stillness: everything holds on a pad.
    Still,
    /// Under the commune, in the sewers and passages (#127): dub-wet minor,
    /// a bottle-flute lead and kalimba drips echoing down the tunnels.
    Underground,
    /// Inside Wendy's (#117): a too-cheerful fast-food jingle with something
    /// off about it: organ, doo-wah singers, vibes and bongos.
    Wendys,
    /// Up in Wendy's attic: the jingle gone wrong. Slow, warped minor, a
    /// music-box vibes figure, singers far off, the clock ticking.
    WendysAttic,
    /// You farted in front of someone (shame): a blaring, panicky alarm of a tune.
    FartAlert,
    /// New York City: a bustling, metropolitan beat (horns, slap bass, traffic-paced drums).
    Nyc,
    /// Inside the NYC bodega: a Mexican-American cumbia/ranchera theme.
    Bodega,
}

/// How a mood sounds on top of what it plays: the band's overall colour
/// and feel, so the same instruments fit each place.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Tone {
    /// Above ~1 kHz, the mix is cut (positive, warm and woolly) or lifted
    /// (negative, crisp): the top end's gain is `1 - 0.5 * warmth`.
    pub warmth: f32,
    /// Stereo width: 1 as mixed, below narrower, above wider.
    pub width: f32,
    /// Tape-style pitch wobble, 0..1 (1 is about 4 cents): old gear drifting.
    pub wow: f32,
    /// The backbeat (snare, clap, rim) sits this many ms behind the grid.
    pub lay_back_ms: f32,
    /// Hats and shakers wander by up to this many ms either way.
    pub loose_ms: f32,
}

impl Tone {
    pub const NEUTRAL: Tone = Tone {
        warmth: 0.0,
        width: 1.0,
        wow: 0.0,
        lay_back_ms: 0.0,
        loose_ms: 0.0,
    };
}

impl Mood {
    /// The mood's colour and feel (see [`Tone`]).
    pub fn tone(self) -> Tone {
        table().moods.get(&self).map_or(Tone::NEUTRAL, |ms| ms.tone)
    }

    pub const ALL: [Mood; 18] = [
        Mood::Menu,
        Mood::Explore,
        Mood::Village,
        Mood::Forest,
        Mood::Maze,
        Mood::Sky,
        Mood::Abuela,
        Mood::Hiding,
        Mood::Seeking,
        Mood::Duel,
        Mood::Landlord,
        Mood::Still,
        Mood::Underground,
        Mood::Wendys,
        Mood::WendysAttic,
        Mood::FartAlert,
        Mood::Nyc,
        Mood::Bodega,
    ];

    pub fn from_index(i: u8) -> Mood {
        Self::ALL.get(i as usize).copied().unwrap_or_default()
    }

    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0) as u8
    }

    /// The intensity this mood sits at with nothing else going on.
    pub fn base_intensity(self) -> f32 {
        style(self).intensity
    }

    /// Make-up gain (dB) so calm, sparse moods aren't a cliff quieter
    /// than busy ones when you walk between areas.
    pub fn level_db(self) -> f32 {
        table().moods.get(&self).map_or(0.0, |ms| ms.level_db)
    }

    /// Moods that cut in at the very next bar line instead of after a
    /// transition bar.
    fn urgent(self) -> bool {
        matches!(
            self,
            Mood::Duel | Mood::Seeking | Mood::Still | Mood::Menu | Mood::FartAlert
        )
    }
}

/// The parts of a tune.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Section {
    Intro,
    A,
    Hook,
    B,
    Solo,
    Breakdown,
    Victory,
    Still,
}

/// What the director reads from the game, once per bar.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Cue {
    pub mood: Mood,
    pub intensity: f32,
    /// Bumped by the game for every victory (round won, chapter done...).
    pub victories: u32,
    /// How high the player has climbed, in storeys (0 on the ground, up to
    /// [`LIFT_MAX`]): the key climbs a step of the scale for each.
    pub lift: u8,
    /// Which way the player is moving vertically, -1 (falling, heading
    /// down) .. 1 (climbing): melodies and arpeggios lean that way.
    pub climb: f32,
}

/// One planned bar: its length and what to play in it.
pub struct Bar {
    pub len: u32,
    pub events: Vec<Ev>,
    pub bus: [f32; BUSES],
    /// Master gain for the bar (linear), see [`Mood::level_db`].
    pub gain: f32,
    /// How bright the band plays (0..1): rises through a tune's sections
    /// and across each one, so the sounds open up and settle back.
    pub bright: f32,
    /// Reverb and echo send scales for the bar.
    pub wet: f32,
    pub echo: f32,
    /// The mood's colour and feel.
    pub tone: Tone,
}

impl Bar {
    pub fn new() -> Self {
        Self {
            len: RATE,
            events: Vec::with_capacity(EVENT_CAP),
            bus: [0.0; BUSES],
            gain: 1.0,
            bright: 0.5,
            wet: 1.0,
            echo: 1.0,
            tone: Tone::NEUTRAL,
        }
    }
}

impl Default for Bar {
    fn default() -> Self {
        Self::new()
    }
}

const EVENT_CAP: usize = 384;

/// The composer and director (lives on the audio thread).
pub struct Composer {
    rng: u64,
    /// The mood being played (changes only at bar lines).
    pub mood: Mood,
    /// The style used while still (the mood before the stillness).
    style_mood: Mood,
    /// Which song each mood is playing (see [`songs`]).
    song: [u8; Mood::ALL.len()],
    pub section: Section,
    intensity: f32,
    bpm: f32,
    plan: Plan,
    next: Option<Plan>,
    /// Mood the transition bar now playing leads into.
    switching: Option<Mood>,
    /// The keys anticipated this bar's downbeat at the end of the last one.
    pushed: bool,
    /// Mood to return to after a victory.
    key_shift: u8,
    ep: Voicing,
    pad: Voicing,
    horn: Voicing,
    /// Which comping/ostinato variant this section uses.
    comp_var: usize,
    /// The horns held a swell this bar (the pad sits out).
    swelled: bool,
    motif: Motif,
    hook: Motif,
    last_note: u8,
    /// Where this phrase's motif started (restated later).
    anchor: u8,
    /// The lead's last notes (for the answering instrument) and where its
    /// notes in this bar end.
    frag: Line,
    lead_end: u32,
    lead_this_bar: bool,
    victories: u32,
    /// The storey the key is on, and the one this bar's second half is
    /// modulating to (over `bridge`, the new key's dominant).
    lift: u8,
    lift_to: Option<u8>,
    bridge: Chord,
    /// A modulation landed on this bar's downbeat.
    landed: bool,
    climb: f32,
    /// The nylon guitar's left hand.
    gtr: Voicing,
    pub bars_played: u64,
    // Per-bar scratch.
    steps: u32,
    step_len: f32,
    swing: f32,
    /// Every lead note written on a strong beat with its chord (tests).
    #[cfg(test)]
    pub strong_log: Vec<(u8, Chord)>,
}

impl Composer {
    pub fn new(seed: u64) -> Self {
        let mood = Mood::Menu;
        let s = style(mood);
        let mut c = Self {
            rng: seed | 1,
            mood,
            style_mood: mood,
            song: [0; Mood::ALL.len()],
            section: Section::Intro,
            intensity: s.intensity,
            bpm: s.bpm,
            plan: Plan {
                kind: Section::Intro,
                bars: 4,
                bar: 0,
                key: s.key,
                prog: s.a,
                fixed: None,
                tail: None,
            },
            next: None,
            switching: None,
            pushed: false,
            key_shift: 0,
            ep: [53, 57, 60, 64],
            pad: [60, 64, 67, 71],
            horn: [62, 65, 69, 72],
            comp_var: 0,
            swelled: false,
            motif: Motif::default(),
            hook: Motif::default(),
            last_note: 72,
            anchor: 72,
            frag: Line::new(),
            lead_end: 0,
            lead_this_bar: false,
            victories: 0,
            lift: 0,
            lift_to: None,
            bridge: Chord::new(7, Dom7alt),
            landed: false,
            climb: 0.0,
            gtr: [52, 57, 60, 64],
            bars_played: 0,
            steps: 16,
            step_len: 1.0,
            swing: 0.0,
            #[cfg(test)]
            strong_log: Vec::new(),
        };
        c.motif = c.new_motif();
        c.hook = c.new_motif();
        c
    }

    /// The tempo of the bar just written.
    pub fn bpm(&self) -> f32 {
        self.bpm
    }

    fn rand(&mut self) -> f32 {
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng >> 40) as f32 / (1u64 << 24) as f32
    }

    fn chance(&mut self, p: f32) -> bool {
        self.rand() < p
    }

    fn pick(&mut self, n: usize) -> usize {
        ((self.rand() * n as f32) as usize).min(n.saturating_sub(1))
    }

    fn style(&self) -> &'static Style {
        if self.mood == Mood::Still {
            style_song(
                self.style_mood,
                self.song[self.style_mood as usize % Mood::ALL.len()],
            )
        } else {
            style_song(self.mood, self.song[self.mood as usize % Mood::ALL.len()])
        }
    }

    /// The chord `step` 16ths after this bar's start (may run into later
    /// bars or the next planned section). While the key is climbing, the
    /// second half of the bar is the new key's dominant and everything
    /// after it is heard in the new key.
    fn chord(&self, step: u32) -> Chord {
        if let Some(to) = self.lift_to {
            if step >= 16 {
                let c = self.raw_chord(step);
                let d = LIFT[to as usize] as i32 - LIFT[self.lift as usize] as i32;
                return Chord::new((c.root as i32 + d).rem_euclid(12) as u8, c.q);
            }
            if step >= 8 {
                return self.bridge;
            }
        }
        self.raw_chord(step)
    }

    fn raw_chord(&self, step: u32) -> Chord {
        let bar = self.plan.bar as u32 + step / 16;
        if bar >= self.plan.bars as u32 {
            if let Some(n) = &self.next {
                return n.chord_at((bar - self.plan.bars as u32) as u16, step % 16);
            }
            return self.plan.chord_at(self.plan.bars - 1, 15);
        }
        self.plan.chord_at(bar as u16, step % 16)
    }

    /// Decide and write the next bar. Called exactly at every bar line.
    pub fn next_bar(&mut self, cue: Cue, bar: &mut Bar) {
        bar.events.clear();
        self.intensity += (cue.intensity.clamp(0.0, 1.0) - self.intensity) * 0.5;
        self.climb = cue.climb.clamp(-1.0, 1.0);
        if self.bars_played > 0 {
            self.plan.bar += 1;
        }
        let mut new_mood = false;

        if cue.victories != self.victories {
            let fresh_victory = cue.victories > self.victories
                && !matches!(self.mood, Mood::Still | Mood::Menu)
                && self.plan.kind != Section::Victory;
            self.victories = cue.victories;
            if fresh_victory {
                let s = self.style();
                self.next = None;
                self.plan = Plan {
                    kind: Section::Victory,
                    bars: 2,
                    bar: 0,
                    key: (s.key + self.key_shift) % 12,
                    prog: VICTORY,
                    fixed: None,
                    tail: None,
                };
            }
        }

        if cue.mood != self.mood && self.plan.kind != Section::Victory {
            let target = cue.mood;
            if self.switching.is_some() || target.urgent() || self.mood == Mood::Still {
                // Cut in at this bar line.
                self.switching = None;
                if target == Mood::Still {
                    self.style_mood = self.mood;
                }
                let fresh = target != Mood::Still && self.mood != Mood::Still;
                let from_still = self.mood == Mood::Still;
                self.mood = target;
                let prev = if from_still {
                    Section::Still
                } else {
                    self.plan.kind
                };
                self.plan = self.plan_section(target, prev, fresh);
                self.next = None;
                new_mood = true;
            } else {
                // This bar is the transition: fill over an approach chord.
                self.switching = Some(target);
                let next = self.plan_section(target, self.plan.kind, true);
                self.plan.bar = self.plan.bar.min(self.plan.bars.saturating_sub(1));
                self.plan.bars = self.plan.bar + 1;
                let first = next.first();
                self.plan.tail = Some(self.approach(first));
                self.next = Some(next);
            }
        } else if cue.mood == self.mood {
            self.switching = None;
        }

        if self.plan.bar >= self.plan.bars {
            if let Some(m) = self.switching.take() {
                self.mood = m;
                new_mood = true;
            }
            let prev = self.plan.kind;
            let m = self.mood;
            let next = match self.next.take() {
                Some(n) => n,
                None => self.plan_section(m, prev, false),
            };
            self.plan = next;
        }
        // Entering the last bar of a section: plan the next one now so the
        // turnaround can lead into it.
        if self.plan.bar + 1 == self.plan.bars
            && self.next.is_none()
            && self.switching.is_none()
            && !matches!(self.plan.kind, Section::Victory | Section::Still)
        {
            let m = self.mood;
            let kind = self.plan.kind;
            let next = self.plan_section(m, kind, false);
            if next.key != self.plan.key || self.chance(0.5) {
                let first = next.first();
                self.plan.tail = Some(self.approach(first));
            }
            self.next = Some(next);
        }
        self.section = self.plan.kind;
        self.lift_step(cue.lift.min(LIFT_MAX), new_mood);

        let s = self.style();
        // Tempo eases towards the style's (an accelerando into a duel).
        let target_bpm = s.bpm * if self.mood == Mood::Still { 0.9 } else { 1.0 };
        let d = target_bpm - self.bpm;
        self.bpm = if d.abs() < 1.5 {
            target_bpm
        } else {
            self.bpm + d * 0.5
        };
        self.swing = s.swing;
        self.step_len = 60.0 / self.bpm / 4.0 * RATE as f32;
        let last = self.plan.bar + 1 == self.plan.bars;
        let fancy = matches!(self.plan.kind, Section::Hook | Section::Solo | Section::B);
        self.steps = if last
            && fancy
            && self.switching.is_none()
            && self.lift_to.is_none()
            && self.chance(s.odd)
        {
            14
        } else {
            16
        };
        bar.len = (self.steps as f32 * self.step_len) as u32;
        self.bars_played += 1;

        self.layers(&mut bar.bus);
        bar.gain = 10f32.powf(self.mood.level_db() / 20.0);
        self.colour(bar);
        self.write(bar, new_mood);
        self.transpose(bar);
        self.humanize(bar);
        bar.events.sort_unstable_by_key(|e| e.at);
    }

    /// How the band's sound moves through the tune: brightness climbs from
    /// the intro to the hooks and solos and across each section, and the
    /// room gets wetter in intros, bridges and breakdowns.
    fn colour(&self, bar: &mut Bar) {
        bar.tone = self.mood.tone();
        let (base, wet, echo) = match self.plan.kind {
            Section::Intro => (0.25, 1.35, 1.2),
            Section::A => (0.4, 1.0, 1.0),
            Section::Hook => (0.62, 0.95, 1.0),
            Section::B => (0.5, 1.2, 1.15),
            Section::Solo => (0.75, 1.05, 1.45),
            Section::Breakdown => (0.2, 1.5, 1.4),
            Section::Victory => (0.9, 1.2, 1.0),
            Section::Still => (0.15, 1.6, 1.3),
        };
        let through = self.plan.bar as f32 / self.plan.bars.max(1) as f32;
        let calm = 1.0 - self.intensity;
        bar.bright = (base + 0.18 * through + 0.25 * (self.intensity - 0.5)).clamp(0.05, 1.0);
        bar.wet = 1.3 * wet * (1.0 + 0.3 * calm);
        bar.echo = 1.2 * echo;
        // Higher up the air is clearer and the room bigger.
        let up = self.lift as f32;
        bar.bright = (bar.bright + 0.05 * up).min(1.0);
        bar.wet *= 1.0 + 0.07 * up;
        // Down in the tunnels everything rings off wet brick (#127).
        if self.mood == Mood::Underground {
            bar.wet *= 1.6;
            bar.echo *= 1.6;
        }
    }

    /// Bus levels: which instruments are in for this bar.
    fn layers(&self, bus: &mut [f32; BUSES]) {
        let s = self.style();
        let i = self.intensity;
        let kind = self.plan.kind;
        let on = |b: bool| if b { 1.0 } else { 0.0 };
        *bus = [0.0; BUSES];
        if kind == Section::Still {
            bus[Bus::Pad as usize] = 1.0;
            bus[Bus::Perc as usize] = 0.6;
            return;
        }
        let drums = match kind {
            Section::Intro => on(i > 0.5) * 0.6,
            _ => on(i > 0.12) * if s.groove == Groove::Dream { 0.7 } else { 1.0 },
        };
        bus[Bus::Drums as usize] = drums;
        bus[Bus::Bass as usize] = on(kind != Section::Intro || i > 0.4);
        bus[Bus::Keys as usize] = 1.0;
        bus[Bus::Lead as usize] = on(s.lead != LeadKind::Silent);
        bus[Bus::Pad as usize] = if s.pad {
            1.0
        } else {
            on(kind == Section::Breakdown || i < 0.3) * 0.8
        };
        bus[Bus::Perc as usize] = 1.0;
        bus[Bus::Brass as usize] = on(s.horns != Horns::None);
        bus[Bus::Guitar as usize] =
            on(s.nylon != Nylon::None || s.comp == Comp::Bolero || s.answer == Some(pk().guitar));
    }

    /// Is the nylon guitar picking (instead of the keys) in this section?
    fn picking(&self) -> bool {
        match self.style().nylon {
            Nylon::Pick(sections) => sections.contains(&self.plan.kind),
            _ => false,
        }
    }

    /// Sample offset of 16th `step`, with the off-16ths swung late.
    fn at(&self, step: u32) -> u32 {
        let swung = step as f32 + if step % 2 == 1 { self.swing } else { 0.0 };
        (swung * self.step_len).max(0.0) as u32
    }

    fn len(&self, steps: f32) -> u32 {
        (steps * self.step_len) as u32
    }

    fn push(bar: &mut Bar, ev: Ev) {
        if bar.events.len() < EVENT_CAP {
            let ev = Ev {
                at: ev.at.min(bar.len.saturating_sub(1)),
                ..ev
            };
            bar.events.push(ev);
        }
    }

    fn write(&mut self, bar: &mut Bar, new_mood: bool) {
        let kind = self.plan.kind;
        let last = self.plan.bar + 1 == self.plan.bars;
        let i = self.intensity;
        let s = self.style();
        if kind == Section::Still {
            self.write_still(bar);
            return;
        }
        let phrase_end = (self.plan.bar % 4) == 3 || last;
        let kime = self.steps == 14
            || (kind == Section::Victory && self.plan.bar == 0)
            || (last && self.switching.is_none() && i > 0.5 && self.chance(s.kime * 0.6));
        let crash = (self.plan.bar == 0 && i > 0.4 && kind != Section::Intro)
            || (self.landed && i > 0.3)
            || new_mood && i > 0.5
            || (kind == Section::Victory && self.plan.bar == 1);
        self.swelled = false;
        // A reversed cymbal rising into a bigger section or a new mood.
        let into_big = self.switching.is_some()
            || self.lift_to.is_some_and(|t| t > self.lift)
            || (last
                && self
                    .next
                    .is_some_and(|n| matches!(n.kind, Section::Hook | Section::Solo)));
        if into_big && (i > 0.35 || self.lift_to.is_some()) {
            let swell = (super::drums::SWELL_SECS * RATE as f32) as u32;
            if bar.len > swell {
                Self::push(bar, hit(bar.len - swell, pk().swell, 57, 0.8 + 0.3 * i));
            }
        }
        if kime {
            self.pushed = false;
            self.write_kime(bar, crash);
        } else {
            let fill =
                (phrase_end && i > 0.3 && self.plan.bar % 8 >= 3) || self.switching.is_some();
            self.write_drums(bar, fill, crash);
            self.write_bass(bar);
            self.write_comp(bar);
            // A note held over the bar line still counts as the lead playing.
            self.lead_this_bar = false;
            self.lead_end = self.lead_end.saturating_sub(16);
            self.write_lead(bar);
            self.write_answer(bar);
            self.write_horns(bar);
        }
        self.write_pad(bar);
        self.write_arp(bar);
    }

    fn write_still(&mut self, bar: &mut Bar) {
        let c = self.chord(0);
        let pad_inst = self.style().pad_inst;
        if self.plan.bar.is_multiple_of(2) {
            self.pad = voice_lead(c, &self.pad, 55, 67);
            let len = self.len(self.steps as f32 * 2.2);
            for n in self.pad {
                Self::push(
                    bar,
                    Ev {
                        at: 0,
                        inst: pad_inst,
                        note: n,
                        vel: 0.6,
                        len,
                        glide: false,
                    },
                );
            }
        }
        if self.plan.bar % 4 == 1 {
            // A single high bell on the 9th, very soft.
            let n = c.root_from(84) + 2;
            let at = self.at(4);
            Self::push(
                bar,
                Ev {
                    at,
                    inst: pk().chime,
                    note: n.min(100),
                    vel: 0.35,
                    len: 0,
                    glide: false,
                },
            );
        }
    }

    // --- Drums ----------------------------------------------------------
}

/// Fold a note into the lead's range by octaves.
fn fold(mut n: u8) -> u8 {
    while n > LEAD_HI {
        n -= 12;
    }
    while n < LEAD_LO {
        n += 12;
    }
    n
}

/// Fold only notes well outside the lead's range, so a line that pokes a
/// little over the edge keeps its shape instead of jumping an octave.
fn soft_fold(mut n: u8) -> u8 {
    while n > LEAD_HI + 3 {
        n -= 12;
    }
    while n + 2 < LEAD_LO {
        n += 12;
    }
    n
}

/// The note of pitch class `pc` nearest `near`.
fn nearest_pc(near: u8, pc: u8) -> u8 {
    let base = near - near % 12 + pc;
    let cands = [base.saturating_sub(12), base, base + 12];
    let mut best = base;
    for c in cands {
        if (c as i32 - near as i32).abs() < (best as i32 - near as i32).abs() {
            best = c;
        }
    }
    best
}

/// The guide tone (third or seventh) of `ch` within `max` semitones of
/// `note`, nearest first.
fn nearest_guide(ch: Chord, note: u8, max: u8) -> Option<u8> {
    let g = ch.q.guide();
    let is_guide = |n: u8| g.contains(&((n + 12 - ch.root % 12) % 12));
    for d in 0..=max {
        if is_guide(note.saturating_sub(d)) {
            return Some(note.saturating_sub(d));
        }
        if is_guide(note + d) {
            return Some(note + d);
        }
    }
    None
}

/// Move `steps` chord tones up or down from `note`.
fn chord_tone_step(ch: Chord, note: u8, steps: i32) -> u8 {
    let mut n = ch.nearest_strong(note) as i32;
    for _ in 0..steps.abs() {
        loop {
            n += steps.signum();
            if !(24..108).contains(&n) || ch.is_strong(n as u8) {
                break;
            }
        }
    }
    n.clamp(24, 107) as u8
}

/// Four-way close "block" voicing under a melody note: the next chord tones
/// down from `top`, never a semitone just under the melody (it rubs).
fn block_voicing(ch: Chord, top: u8) -> Voicing {
    let pcs = ch.q.voicing().map(|t| (ch.root + t) % 12);
    let ok = |n: u8| pcs.contains(&(n % 12)) || n % 12 == ch.root;
    let mut v = [top; 4];
    let mut n = top;
    for k in (0..3).rev() {
        n -= 1;
        while !ok(n) || (k == 2 && top - n == 1) || n == v[k + 1] {
            n -= 1;
        }
        v[k] = n;
    }
    v
}

/// Horns don't like semitone rubs at the edges of a voicing: lift a
/// bottom-note rub an octave, drop a top-note rub an octave.
fn no_top_rub(mut v: Voicing) -> Voicing {
    if v[1] - v[0] == 1 {
        v[0] += 12;
        v.sort_unstable();
    }
    if v[3] - v[2] == 1 {
        v[2] -= 12;
        v.sort_unstable();
    }
    v
}

fn vel(pat: &str, k: usize) -> Option<f32> {
    match pat.as_bytes().get(k) {
        Some(b'x') => Some(1.0),
        Some(b'o') => Some(0.72),
        Some(b'O') => Some(0.8),
        Some(b'g') => Some(0.38),
        _ => None,
    }
}

fn ev(at: u32, inst: Inst, note: u8, vel: f32, len: u32) -> Ev {
    Ev {
        at,
        inst,
        note,
        vel,
        len,
        glide: false,
    }
}

fn hit(at: u32, inst: Inst, note: u8, vel: f32) -> Ev {
    Ev {
        at,
        inst,
        note,
        vel,
        len: 0,
        glide: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(mood: Mood) -> Cue {
        Cue {
            mood,
            intensity: mood.base_intensity(),
            ..Cue::default()
        }
    }

    #[test]
    fn strong_beats_land_on_chord_tones() {
        let mut c = Composer::new(7);
        let mut bar = Bar::new();
        for (k, m) in Mood::ALL
            .iter()
            .cycle()
            .take(Mood::ALL.len() * 30)
            .enumerate()
        {
            let mut q = cue(*m);
            q.intensity = (k % 7) as f32 / 6.0;
            c.next_bar(q, &mut bar);
            assert!(bar.events.windows(2).all(|w| w[0].at <= w[1].at));
            assert!(bar.events.iter().all(|e| e.at < bar.len));
        }
        let mut c = Composer::new(99);
        for m in Mood::ALL {
            for _ in 0..24 {
                c.next_bar(cue(m), &mut bar);
            }
        }
        assert!(c.strong_log.len() > 100, "{}", c.strong_log.len());
        for (n, ch) in &c.strong_log {
            assert!(ch.is_strong(*n), "{n} over {ch:?}");
        }
    }

    /// Climbing storeys walks the key up the ladder a step at a time, each
    /// step prepared by a dominant in the bar before; coming down walks it
    /// back.
    /// The village and explore moods have several songs: every one comes
    /// up, and each plays notes at its own tempo.
    #[test]
    fn every_song_gets_played() {
        for (m, n) in [(Mood::Village, 3), (Mood::Explore, 2)] {
            let mut tempos = std::collections::BTreeSet::new();
            for seed in 0..40 {
                let mut c = Composer::new(seed);
                let mut bar = Bar::new();
                c.next_bar(cue(Mood::Menu), &mut bar);
                for _ in 0..6 {
                    c.next_bar(cue(m), &mut bar);
                }
                assert!(!bar.events.is_empty());
                tempos.insert(c.bpm() as u32);
            }
            assert_eq!(tempos.len(), n, "{m:?}: {tempos:?}");
        }
    }

    #[test]
    fn climbing_lifts_the_key() {
        let mut c = Composer::new(7);
        let mut bar = Bar::new();
        let mut run = |c: &mut Composer, lift: u8, bars: usize| {
            let mut seen = Vec::new();
            for _ in 0..bars {
                c.next_bar(
                    Cue {
                        lift,
                        climb: 1.0,
                        ..cue(Mood::Explore)
                    },
                    &mut bar,
                );
                if c.lift_to.is_some() {
                    assert!(c.bridge.q.is_dominant(), "bridge {:?}", c.bridge);
                    assert_eq!(c.chord(8), c.bridge);
                }
                seen.push(c.lift);
            }
            seen
        };
        run(&mut c, 0, 6);
        let up = run(&mut c, LIFT_MAX, 14);
        assert_eq!(*up.last().unwrap_or(&0), LIFT_MAX, "{up:?}");
        assert!(
            up.windows(2).all(|w| w[1] == w[0] || w[1] == w[0] + 1),
            "{up:?}"
        );
        let down = run(&mut c, 0, 14);
        assert_eq!(*down.last().unwrap_or(&9), 0, "{down:?}");
    }

    #[test]
    fn every_mood_writes_music() {
        let mut bar = Bar::new();
        for m in Mood::ALL {
            let mut c = Composer::new(3);
            let mut notes = 0;
            for _ in 0..16 {
                c.next_bar(cue(m), &mut bar);
                notes += bar.events.len();
            }
            assert!(notes > 10, "{m:?} wrote {notes}");
            assert_eq!(c.mood, m);
        }
    }

    #[test]
    fn victories_play_a_cadence_then_resume() {
        let mut c = Composer::new(5);
        let mut bar = Bar::new();
        for _ in 0..6 {
            c.next_bar(cue(Mood::Village), &mut bar);
        }
        let mut q = cue(Mood::Village);
        q.victories = 1;
        c.next_bar(q, &mut bar);
        assert_eq!(c.section, Section::Victory);
        c.next_bar(q, &mut bar);
        assert_eq!(c.section, Section::Victory);
        c.next_bar(q, &mut bar);
        assert_eq!(c.section, Section::A);
    }

    fn name(n: u8) -> String {
        const N: [&str; 12] = [
            "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
        ];
        format!("{}{}", N[n as usize % 12], (n / 12) as i32 - 1)
    }

    /// Print a score of 28 bars, to read what the composer writes:
    /// `PEEK=duel cargo test -p fusion peek_score -- --ignored --nocapture`
    /// (duel, abuela, menu, forest, explore, seeking; else village).
    #[test]
    #[ignore = "prints a score"]
    fn peek_score() {
        let want = std::env::var("PEEK").unwrap_or_default();
        let mood = Mood::ALL
            .into_iter()
            .find(|m| format!("{m:?}").eq_ignore_ascii_case(&want))
            .unwrap_or(Mood::Village);
        // PEEK_CLIMB=1: up a storey every two bars from bar 4, then down.
        let climbing = std::env::var_os("PEEK_CLIMB").is_some();
        let mut c = Composer::new(42);
        let mut bar = Bar::new();
        for b in 0..28u8 {
            let lift = if climbing {
                (b.saturating_sub(4) / 2).min(LIFT_MAX) - (b.saturating_sub(18) / 2).min(LIFT_MAX)
            } else {
                0
            };
            c.next_bar(
                Cue {
                    mood,
                    intensity: 0.7,
                    lift,
                    climb: if climbing { 1.0 } else { 0.0 },
                    ..Cue::default()
                },
                &mut bar,
            );
            let sl = c.step_len;
            // As heard: in the storey's key.
            let up = LIFT[c.lift as usize];
            let chords: Vec<String> = (0..4)
                .map(|k| {
                    let ch = c.chord(k * 4);
                    format!("{}{:?}", N12[((ch.root + up) % 12) as usize], ch.q)
                })
                .collect();
            println!(
                "{b:2} {:?}/{:?} bar{} storey {}{} [{}] steps {}",
                c.mood,
                c.section,
                c.plan.bar,
                c.lift,
                c.lift_to.map_or(String::new(), |t| format!("->{t}")),
                chords.join(" "),
                c.steps
            );
            for inst in [
                pk().guitar,
                pk().lead,
                pk().mini,
                pk().brass,
                pk().flute,
                pk().poly_brass,
                pk().bass,
                pk().fretless,
                pk().epiano,
                pk().kalimba,
            ] {
                let notes: Vec<String> = bar
                    .events
                    .iter()
                    .filter(|e| e.inst == inst)
                    .map(|e| format!("{:.1}:{}", e.at as f32 / sl, name(e.note)))
                    .collect();
                if !notes.is_empty() {
                    println!("   {inst:?}: {}", notes.join(" "));
                }
            }
        }
    }

    const N12: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ];
}
