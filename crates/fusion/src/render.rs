//! Offline rendering and analysis: render the score (a tour of moods, one
//! mood, or one instrument on its own) to a WAV and measure it. Used by the
//! `fusion-render` tool so instruments can be auditioned and mixed without
//! building the game.

use std::sync::Arc;

use crate::compose::{Cue, Mood};
use crate::dsp::{Biquad, Eq};
use crate::engine::Engine;
use crate::inst::Engine as Sound;
use crate::synth::{BLOCK, BUSES, Band, Bank, Bus, Ev, Inst, RATE};
use crate::theory::{Chord, Quality, voice_lead};

/// The demo tour: (start second, mood, intensity).
pub const TOUR: &[(f32, Mood, f32)] = &[
    (0.0, Mood::Menu, 0.55),
    (14.0, Mood::Village, 0.6),
    (30.0, Mood::Forest, 0.35),
    (42.0, Mood::Maze, 0.3),
    (51.0, Mood::Sky, 0.25),
    (59.0, Mood::Abuela, 0.4),
    (69.0, Mood::Hiding, 0.4),
    (75.0, Mood::Seeking, 0.8),
    (83.0, Mood::Duel, 0.95),
    (91.0, Mood::Still, 0.2),
    (96.0, Mood::Village, 0.7),
];
pub const TOUR_SECS: f32 = 106.0;
const TOUR_VICTORY: f32 = 25.0;

pub const BUS_NAMES: [&str; BUSES] = Bus::NAMES;

/// Render `secs` of the score, asking `plan` what the game wants at each
/// second. `mute` leaves buses out (bit per bus). Also returns where each
/// mood/section change fell.
pub fn render_score(
    seed: u64,
    secs: f32,
    mute: u8,
    plan: impl Fn(f32) -> Cue,
) -> (Vec<f32>, Vec<(f32, String)>) {
    let bank = Arc::new(Bank::build());
    let mut engine = Engine::new(bank, seed);
    engine.set_mute(mute);
    let total = (secs * RATE as f32) as usize;
    let mut out = Vec::with_capacity(total * 2);
    let mut buf = vec![0.0f32; 512];
    let mut t = 0usize;
    let mut last = None;
    let mut log = Vec::new();
    while t < total {
        let s = t as f32 / RATE as f32;
        let cue = plan(s);
        engine.render(&mut buf, &mut || cue);
        let now = Some((engine.mood(), engine.section(), cue.lift));
        if now != last {
            let lift = if cue.lift > 0 {
                format!("  (storey {})", cue.lift)
            } else {
                String::new()
            };
            log.push((
                s,
                format!(
                    "{:?} / {:?}{lift} ({:.0} bpm)",
                    engine.mood(),
                    engine.section(),
                    engine.composer.bpm()
                ),
            ));
            last = now;
        }
        out.extend_from_slice(&buf);
        t += buf.len() / 2;
    }
    (out, log)
}

/// The tour's plan at second `s`.
pub fn tour_plan(s: f32) -> Cue {
    let (mood, intensity) = TOUR
        .iter()
        .rev()
        .find(|(at, _, _)| s >= *at)
        .map_or((Mood::Menu, 0.5), |(_, m, i)| (*m, *i));
    Cue {
        mood,
        intensity,
        victories: u32::from(s >= TOUR_VICTORY),
        ..Cue::default()
    }
}

/// A climb in `mood`: on the ground for a while, up a storey every
/// `per` seconds to the top, a rest up there, then back down.
pub fn climb_plan(mood: Mood, per: f32) -> impl Fn(f32) -> Cue {
    let top = crate::compose::LIFT_MAX as f32;
    move |s: f32| {
        let up_from = 2.0 * per;
        let down_from = up_from + top * per + 2.0 * per;
        let (lift, climb) = if s < up_from {
            (0.0, 0.0)
        } else if s < up_from + top * per {
            ((s - up_from) / per + 1.0, 1.0)
        } else if s < down_from {
            (top, 0.0)
        } else {
            (top - (s - down_from) / per, -1.0)
        };
        let lift = lift.clamp(0.0, top) as u8;
        Cue {
            mood,
            intensity: mood.base_intensity(),
            lift,
            climb: if lift == 0 && climb < 0.0 { 0.0 } else { climb },
            ..Cue::default()
        }
    }
}

/// How long [`climb_plan`] takes, there and back.
pub fn climb_secs(per: f32) -> f32 {
    let top = crate::compose::LIFT_MAX as f32;
    2.0 * per + top * per + 2.0 * per + top * per + 2.0 * per
}

/// A short phrase on one instrument alone, to judge its sound: hits at a
/// few velocities for drums; chords over a ii-V-I for keys, pads and horns;
/// a line with slides for leads and basses; arpeggios for mallets.
pub fn audition(inst: Inst) -> Vec<f32> {
    let bank = Arc::new(Bank::build());
    let mut band = Band::new(bank);
    band.bus_target = [1.0; BUSES];
    band.bus_gain = [1.0; BUSES];
    band.set_tempo(100.0);
    let beat = RATE as f32 * 0.6;
    let mut evs: Vec<Ev> = Vec::new();
    let ev = |at: f32, note: u8, vel: f32, len: f32, glide: bool| Ev {
        at: at as u32,
        inst,
        note,
        vel,
        len: len as u32,
        glide,
    };
    let prog = [
        Chord::new(2, Quality::Min9),
        Chord::new(7, Quality::Dom13),
        Chord::new(0, Quality::Maj9),
        Chord::new(9, Quality::Dom7alt),
    ];
    let roots: Vec<u8> = match crate::inst::table().get(inst).map(|s| &s.engine) {
        Some(Sound::Kit { samples, .. }) => samples.iter().map(|s| s.root).collect(),
        _ => Vec::new(),
    };
    match inst.bus() {
        Bus::Drums => {
            for k in 0..16 {
                let vel = [1.0, 0.5, 0.75, 0.35][k % 4];
                // Each of the drum's samples in turn.
                let note = roots.get(k % roots.len().max(1)).copied().unwrap_or(60);
                evs.push(ev(k as f32 * beat * 0.5, note, vel, 0.0, false));
            }
        }
        Bus::Bass | Bus::Lead => {
            // A line through the changes: chord tones on the beats,
            // stepwise in between, a few slides, a long note to finish.
            let (lo, line): (u8, &[i8]) = if inst.bus() == Bus::Bass {
                (33, &[0, 7, 12, 10, 7, 5, 4, 2, 0, 12, 7, 0])
            } else {
                (69, &[0, 3, 5, 7, 10, 12, 10, 7, 9, 5, 3, 0])
            };
            for (k, d) in line.iter().enumerate() {
                let ch = prog[k / 3 % prog.len()];
                let note = (ch.root_from(lo) as i32 + *d as i32) as u8;
                let vel = if k % 3 == 0 { 0.95 } else { 0.65 };
                let len = if k + 1 == line.len() {
                    beat * 3.0
                } else {
                    beat * 0.95
                };
                evs.push(ev(k as f32 * beat, note, vel, len, k % 4 == 2));
            }
        }
        Bus::Guitar => {
            // Fingerpicking through the changes (thumb on the beat, fingers
            // in between), then a slow strum to let the body ring.
            let mut shape = [52u8, 57, 60, 64];
            for (c, ch) in prog.iter().enumerate() {
                shape = voice_lead(*ch, &shape, 52, 60);
                let thumb = ch.root_from(40);
                for k in 0..8 {
                    let at = (c * 8 + k) as f32 * beat * 0.5;
                    let note = if k % 4 == 0 {
                        thumb + if k == 4 { 7 } else { 0 }
                    } else {
                        shape[[1, 2, 3, 2][k % 4]]
                    };
                    let vel = if k % 4 == 0 { 0.8 } else { 0.55 };
                    evs.push(ev(at, note, vel, beat * 2.0, false));
                }
            }
            let end = prog.len() as f32 * beat * 4.0;
            let last = voice_lead(prog[0], &shape, 52, 60);
            for (j, n) in std::iter::once(prog[0].root_from(40))
                .chain(last)
                .enumerate()
            {
                evs.push(ev(end + j as f32 * 2200.0, n, 0.7, beat * 6.0, false));
            }
        }
        Bus::Perc => {
            for (c, ch) in prog.iter().enumerate() {
                let v = ch.q.voicing();
                for k in 0..8 {
                    let note = ch.root_from(67) + v[k % 4] + 12 * (k as u8 / 4);
                    let at = (c * 8 + k) as f32 * beat * 0.5;
                    evs.push(ev(
                        at,
                        note.min(96),
                        0.5 + 0.1 * (k % 3) as f32,
                        beat,
                        false,
                    ));
                }
            }
        }
        _ => {
            // Chords: a soft long one, then two stabs, per change.
            let mut voicing = [60, 64, 67, 71];
            for (c, ch) in prog.iter().enumerate() {
                voicing = voice_lead(*ch, &voicing, 55, 64);
                let at = c as f32 * beat * 4.0;
                for &n in &voicing {
                    evs.push(ev(at, n, 0.55, beat * 2.5, false));
                    evs.push(ev(at + beat * 2.5, n, 0.95, beat * 0.3, false));
                    evs.push(ev(at + beat * 3.0, n, 0.8, beat * 0.6, false));
                }
            }
        }
    }
    evs.sort_by_key(|e| e.at);
    let end = evs.iter().map(|e| e.at + e.len).max().unwrap_or(0) + RATE * 2;
    let mut out = vec![0.0f32; end as usize * 2];
    let mut t = 0usize;
    let mut next = 0;
    while t < end as usize {
        while next < evs.len() && evs[next].at as usize <= t {
            band.play(&evs[next]);
            next += 1;
        }
        let until = evs.get(next).map_or(end as usize, |e| e.at as usize);
        let span = (until - t).clamp(1, BLOCK).min(end as usize - t);
        band.render(&mut out[t * 2..(t + span) * 2]);
        t += span;
    }
    out
}

/// Octave band centres measured by [`octave_bands`].
pub const BANDS: [f32; 10] = [
    31.5, 63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0,
];
/// A rough long-term average balance of commercial jazz and pop mixes,
/// dB per octave band relative to the 63 Hz band.
pub const TARGET: [f32; 10] = [-5.0, 0.0, 0.0, -2.0, -4.5, -7.0, -10.0, -13.0, -17.0, -25.0];

/// Energy per octave band of interleaved stereo audio, in dB.
pub fn octave_bands(stereo: &[f32]) -> [f32; 10] {
    let mut out = [0.0f32; 10];
    for (o, &c) in out.iter_mut().zip(&BANDS) {
        // Two cascaded band-passes approximate an octave filter.
        let mut a = Biquad::new(Eq::Bp(c, 1.41));
        let mut b = Biquad::new(Eq::Bp(c, 1.41));
        let mut e = 0.0f64;
        for fr in stereo.as_chunks::<2>().0 {
            let y = b.tick(a.tick(0.5 * (fr[0] + fr[1])));
            e += (y * y) as f64;
        }
        *o = 10.0 * ((e / (stereo.len() / 2).max(1) as f64) + 1e-20).log10() as f32;
    }
    out
}

pub fn peak_rms_db(stereo: &[f32]) -> (f32, f32) {
    let peak = stereo.iter().fold(0.0f32, |p, x| p.max(x.abs()));
    let rms = (stereo.iter().map(|x| (x * x) as f64).sum::<f64>() / stereo.len().max(1) as f64)
        .sqrt() as f32;
    (20.0 * peak.max(1e-9).log10(), 20.0 * rms.max(1e-9).log10())
}

/// 16-bit stereo WAV bytes.
pub fn wav(stereo: &[f32]) -> Vec<u8> {
    let data_len = (stereo.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + stereo.len() * 2);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data_len).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&2u16.to_le_bytes());
    w.extend_from_slice(&RATE.to_le_bytes());
    w.extend_from_slice(&(RATE * 4).to_le_bytes());
    w.extend_from_slice(&4u16.to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data_len.to_le_bytes());
    for x in stereo {
        w.extend_from_slice(&((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
    }
    w
}

/// Look an instrument, mood or bus up by name (case-insensitive).
pub fn inst_by_name(name: &str) -> Option<Inst> {
    Inst::find(name)
}

pub fn mood_by_name(name: &str) -> Option<Mood> {
    Mood::ALL
        .iter()
        .copied()
        .find(|m| format!("{m:?}").eq_ignore_ascii_case(name))
}

pub fn bus_by_name(name: &str) -> Option<usize> {
    BUS_NAMES.iter().position(|b| b.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_instrument_auditions() {
        let all = Inst::all();
        assert!(!all.is_empty(), "no instruments loaded");
        for inst in all {
            let a = audition(inst);
            assert!(a.iter().all(|x| x.is_finite()), "{inst:?}");
            let (peak, _) = peak_rms_db(&a);
            assert!(peak > -40.0 && peak < 0.0, "{inst:?}: {peak} dBFS");
        }
    }

    #[test]
    fn bands_find_a_tone() {
        let tone: Vec<f32> = (0..RATE as usize)
            .flat_map(|i| {
                let x = (i as f32 * 1000.0 / RATE as f32 * std::f32::consts::TAU).sin();
                [x, x]
            })
            .collect();
        let b = octave_bands(&tone);
        assert!(b[5] > b[3] + 15.0 && b[5] > b[7] + 15.0, "{b:?}");
    }
}
