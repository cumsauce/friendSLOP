//! `cargo run -p friendslop-speech --example say -- out.wav "text" [pitch] [speed] [key=value ...]`
//! Writes a mono 16-bit WAV. Delivery keys are the `Delivery` fields, e.g. `jitter=0.02 oq=0.45`.

use friendslop_speech::{Voice, post};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(out), Some(text)) = (args.first(), args.get(1)) else {
        eprintln!("usage: say out.wav \"text\" [pitch] [speed] [key=value ...] [phone|pa]");
        return;
    };
    let mut voice = Voice::new();
    let mut effect = "";
    for (i, arg) in args.iter().enumerate().skip(2) {
        match (i, arg.split_once('=')) {
            (2, None) => voice.pitch = arg.parse().unwrap_or(voice.pitch),
            (3, None) => voice.speed = arg.parse().unwrap_or(voice.speed),
            (_, None) => effect = arg,
            (_, Some((k, v))) => {
                let v: f32 = v.parse().unwrap_or(0.0);
                let d = &mut voice.delivery;
                match k {
                    "pitch_mult" => d.pitch_mult = v,
                    "range_mult" => d.range_mult = v,
                    "rate_mult" => d.rate_mult = v,
                    "stress_len" => d.stress_len = v,
                    "declination" => d.declination = v,
                    "oq" => d.oq = v,
                    "tilt" => d.tilt = v,
                    "breath" => d.breath = v,
                    "jitter" => d.jitter = v,
                    "shimmer" => d.shimmer = v,
                    "drift" => d.drift = v,
                    "vibrato_depth" => d.vibrato_depth = v,
                    "vibrato_rate" => d.vibrato_rate = v,
                    "precision" => d.precision = v,
                    "final_rise" => d.final_rise = v,
                    "pause_mult" => d.pause_mult = v,
                    "burst" => d.burst = v,
                    "nasal" => d.nasal = v,
                    "seed" => voice.seed = v as u32,
                    _ => eprintln!("unknown key {k}"),
                }
            }
        }
    }
    let rate = voice.sample_rate as f32;
    let mut samples = voice.speak(text);
    match effect {
        "phone" => post::phone(&mut samples, rate),
        "pa" => post::pa(&mut samples, rate),
        _ => {}
    }
    let data: Vec<u8> = samples
        .iter()
        .flat_map(|s| ((s * 32767.0).round() as i16).to_le_bytes())
        .collect();
    let len = data.len() as u32;
    let rate = voice.sample_rate;
    let parts: [&[u8]; 13] = [
        b"RIFF",
        &(36 + len).to_le_bytes(),
        b"WAVEfmt ",
        &16u32.to_le_bytes(),
        &1u16.to_le_bytes(),
        &1u16.to_le_bytes(),
        &rate.to_le_bytes(),
        &(rate * 2).to_le_bytes(),
        &2u16.to_le_bytes(),
        &16u16.to_le_bytes(),
        b"data",
        &len.to_le_bytes(),
        &data,
    ];
    if let Err(e) = std::fs::write(out, parts.concat()) {
        eprintln!("{e}");
    }
    println!("{:.2} s", voice.duration(text));
}
