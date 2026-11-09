//! Render and measure the soundtrack without the game.
//!
//! ```text
//! cargo run -p fusion --bin fusion-render -- help
//! ```

use fusion::compose::Mood;
use fusion::render::{
    BANDS, BUS_NAMES, TARGET, TOUR_SECS, audition, bus_by_name, inst_by_name, mood_by_name,
    octave_bands, peak_rms_db, render_score, tour_plan, wav,
};
use fusion::synth::{BUSES, Inst};

const HELP: &str = "fusion-render: audition friendSLOP's music without building the game

  fusion-render tour  [out.wav]             the ~100 s tour of every mood
  fusion-render mood  <mood> [secs] [out.wav] [--intensity 0..1] [--victory <sec>]
  fusion-render inst  <instrument> [out.wav]  one instrument alone
  fusion-render stems                       tour spectrum per bus vs a reference mix
  fusion-render list                        moods, instruments and buses

options for tour/mood: --solo <bus>  --mute <bus>  --seed <n>
Each run prints levels and octave bands against a typical jazz/pop mix balance.";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut pos: Vec<&str> = Vec::new();
    let mut seed = 2024u64;
    let mut mute = 0u8;
    let mut intensity: Option<f32> = None;
    let mut victory: Option<f32> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().map(String::as_str).unwrap_or("");
        match a.as_str() {
            "--seed" => seed = val().parse().unwrap_or(seed),
            "--intensity" => intensity = val().parse().ok(),
            "--victory" => victory = val().parse().ok(),
            "--solo" => match bus_by_name(val()) {
                Some(b) => mute = !(1u8 << b),
                None => return fail("unknown bus"),
            },
            "--mute" => match bus_by_name(val()) {
                Some(b) => mute |= 1 << b,
                None => return fail("unknown bus"),
            },
            _ => pos.push(a),
        }
    }
    match pos.first().copied().unwrap_or("help") {
        "tour" => {
            let out = pos.get(1).copied().unwrap_or("fusion_tour.wav");
            let (audio, log) = render_score(seed, TOUR_SECS, mute, tour_plan);
            for (s, what) in log {
                println!("{s:7.2} s  {what}");
            }
            finish(out, &audio);
        }
        "mood" => {
            let Some(mood) = pos.get(1).and_then(|m| mood_by_name(m)) else {
                return fail("which mood? (fusion-render list)");
            };
            let secs: f32 = pos.get(2).and_then(|s| s.parse().ok()).unwrap_or(30.0);
            let out = pos.get(3).copied().unwrap_or("fusion_mood.wav");
            let i = intensity.unwrap_or(mood.base_intensity());
            let (audio, log) = render_score(seed, secs, mute, |s| {
                (mood, i, u32::from(victory.is_some_and(|v| s >= v)))
            });
            for (s, what) in log {
                println!("{s:7.2} s  {what}");
            }
            finish(out, &audio);
        }
        "inst" => {
            let Some(inst) = pos.get(1).and_then(|m| inst_by_name(m)) else {
                return fail("which instrument? (fusion-render list)");
            };
            let out = pos.get(2).copied().unwrap_or("fusion_inst.wav");
            finish(out, &audition(inst));
        }
        "stems" => stems(seed),
        "list" => {
            let moods: Vec<String> = Mood::ALL.iter().map(|m| format!("{m:?}")).collect();
            let insts: Vec<String> = Inst::ALL.iter().map(|m| format!("{m:?}")).collect();
            println!("moods: {}", moods.join(" "));
            println!("instruments: {}", insts.join(" "));
            println!("buses: {}", BUS_NAMES.join(" "));
        }
        _ => println!("{HELP}"),
    }
}

fn fail(msg: &str) {
    eprintln!("{msg}\n\n{HELP}");
    std::process::exit(2);
}

fn finish(path: &str, audio: &[f32]) {
    let (peak, rms) = peak_rms_db(audio);
    let clipped = audio.iter().filter(|x| x.abs() >= 1.0).count();
    println!("peak {peak:.1} dBFS, rms {rms:.1} dBFS, clipped samples {clipped}");
    let b = octave_bands(audio);
    println!("  band    level  vs-63Hz  target   diff");
    for k in 0..BANDS.len() {
        let rel = b[k] - b[1];
        println!(
            "{:6.0} {:8.1} {:8.1} {:7.1} {:+6.1}",
            BANDS[k],
            b[k],
            rel,
            TARGET[k],
            rel - TARGET[k]
        );
    }
    match std::fs::write(path, wav(audio)) {
        Ok(()) => println!("wrote {path}"),
        Err(e) => eprintln!("can't write {path}: {e}"),
    }
}

/// The tour, then each bus alone: octave bands relative to the full mix's
/// 63 Hz band, to see which part owns which frequencies.
fn stems(seed: u64) {
    let (mix, _) = render_score(seed, TOUR_SECS, 0, tour_plan);
    let m = octave_bands(&mix);
    let r = m[1];
    let solos: Vec<[f32; 10]> = (0..BUSES)
        .map(|b| octave_bands(&render_score(seed, TOUR_SECS, !(1u8 << b), tour_plan).0))
        .collect();
    print!("  band    mix target   diff");
    for n in BUS_NAMES {
        print!("{n:>7}");
    }
    println!();
    for k in 0..BANDS.len() {
        print!(
            "{:6.0} {:6.1} {:6.1} {:+6.1}",
            BANDS[k],
            m[k] - r,
            TARGET[k],
            m[k] - r - TARGET[k]
        );
        for s in &solos {
            print!("{:7.1}", s[k] - r);
        }
        println!();
    }
}
