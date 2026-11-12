//! A synthesized 90s fusion kit and hand percussion, rendered once into
//! the sample bank: a punchy pitch-swept kick, a tight two-mode snare,
//! metallic hats, a pinging ride, a crash, clap, shaker, congas/bongos,
//! toms, rimshot and cowbell. Built from sines and filtered noise, so it
//! doesn't alias and doesn't need any asset files.

use std::f32::consts::TAU;

use super::dsp::{Biquad, Eq, Noise};
use super::synth::RATE;

const FS: f32 = RATE as f32;

fn n_for(secs: f32) -> usize {
    (secs * FS) as usize
}

/// Exponential decay with a 1 ms click-free onset.
#[inline]
fn env(t: f32, tau: f32) -> f32 {
    (-t / tau).exp() * (t / 0.001).min(1.0)
}

/// Kick: a sine diving from ~170 Hz to its 47 Hz body, a beater click,
/// a little tanh saturation for punch.
pub fn kick() -> Vec<f32> {
    let n = n_for(0.5);
    let mut out = Vec::with_capacity(n);
    let mut ph = 0.0f32;
    let mut noise = Noise(0xC0FFEE);
    let mut hp = Biquad::new(Eq::Hp(1800.0, 0.7));
    for i in 0..n {
        let t = i as f32 / FS;
        let f = 47.0 + 125.0 * (-t / 0.03).exp() + 40.0 * (-t / 0.004).exp();
        ph += f / FS;
        let body = (ph * TAU).sin() * env(t, 0.2);
        let click = hp.tick(noise.sample()) * (-t / 0.0022).exp() * 0.35;
        out.push((1.5 * (body + click)).tanh());
    }
    out
}

/// Snare: two drum-head modes plus wires (band-passed noise) and a stick
/// crack. `wires` is the noise decay; shorter for ghost notes.
pub fn snare(wires: f32, tone: f32) -> Vec<f32> {
    let n = n_for(0.32);
    let mut out = Vec::with_capacity(n);
    let (mut p1, mut p2) = (0.0f32, 0.0f32);
    let mut noise = Noise(0x5EED_5A1E);
    let mut bp = Biquad::new(Eq::Bp(3000.0, 0.7));
    let mut hp = Biquad::new(Eq::Hp(1400.0, 0.7));
    let mut crack = Biquad::new(Eq::Bp(1900.0, 0.9));
    for i in 0..n {
        let t = i as f32 / FS;
        let bend = 1.0 + 0.3 * (-t / 0.008).exp();
        p1 += 190.0 * bend / FS;
        p2 += 340.0 * bend / FS;
        let head = ((p1 * TAU).sin() + 0.55 * (p2 * TAU).sin()) * env(t, 0.07) * tone * 1.5;
        let w = noise.sample();
        let wire = hp.tick(bp.tick(w)) * 1.4 * env(t, wires);
        let stick = crack.tick(w) * (-t / 0.004).exp() * 1.6;
        out.push((1.2 * (head + wire + stick)).tanh());
    }
    out
}

/// Metal partials (sines at inharmonic 808-ish ratios) over high-passed
/// noise: hats and cymbals.
fn metal(secs: f32, tau: f32, base: f32, noise_amt: f32, hp_f: f32, seed: u32) -> Vec<f32> {
    let n = n_for(secs);
    let ratios = [1.0, 1.342, 1.2312, 1.6532, 1.9523, 2.1523];
    let mut ph = [0.0f32; 6];
    let mut noise = Noise(seed);
    let mut hp = Biquad::new(Eq::Hp(hp_f, 0.7));
    let mut hp2 = Biquad::new(Eq::Hp(hp_f, 0.7));
    let mut lp = Biquad::new(Eq::Lp(12500.0, 0.7));
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        let mut m = 0.0;
        for (p, r) in ph.iter_mut().zip(ratios) {
            *p = (*p + base * r / FS).fract();
            // A hint of square-ness from a few odd harmonics, all far
            // below Nyquist.
            let x = *p * TAU;
            m += x.sin() + 0.3 * (3.0 * x).sin();
        }
        let x = m * 0.12 + noise.sample() * noise_amt;
        out.push(lp.tick(hp2.tick(hp.tick(x))) * env(t, tau));
    }
    out
}

pub fn hat() -> Vec<f32> {
    metal(0.09, 0.028, 2400.0, 0.7, 7000.0, 0x1A7)
}

pub fn open_hat() -> Vec<f32> {
    metal(0.5, 0.22, 2400.0, 0.7, 6500.0, 0x0AE7)
}

pub fn crash() -> Vec<f32> {
    let mut c = metal(1.8, 0.55, 1650.0, 1.0, 2600.0, 0xC2A5);
    // A softer, slower swell under the hit.
    let mut wash = metal(1.8, 0.9, 1100.0, 0.8, 3500.0, 0xCA5);
    for (i, (a, b)) in c.iter_mut().zip(wash.iter_mut()).enumerate() {
        let t = i as f32 / FS;
        *a += *b * 0.5 * (1.0 - (-t / 0.08).exp());
    }
    c
}

/// Ride: a clear bell "ping" (a few inharmonic partials) over a quiet wash,
/// with the stick's tick.
pub fn ride() -> Vec<f32> {
    let partials = [(2950.0, 1.0), (4170.0, 0.7), (5320.0, 0.45), (6630.0, 0.3)];
    let mut ph = [0.0f32; 4];
    let mut wash = metal(1.4, 0.8, 1900.0, 0.6, 5500.0, 0x81DE);
    let mut noise = Noise(0x71C);
    let mut bp = Biquad::new(Eq::Bp(3200.0, 1.2));
    for (i, w) in wash.iter_mut().enumerate() {
        let t = i as f32 / FS;
        let mut ping = 0.0;
        for (p, (f, a)) in ph.iter_mut().zip(partials) {
            *p = (*p + f / FS).fract();
            ping += (*p * TAU).sin() * a;
        }
        let stick = bp.tick(noise.sample()) * (-t / 0.006).exp();
        *w = *w * 0.35 + ping * 0.22 * env(t, 0.45) + stick * 0.6;
    }
    wash
}

/// Clap: three quick bursts of band-passed noise and a short room tail.
pub fn clap() -> Vec<f32> {
    let n = n_for(0.32);
    let mut noise = Noise(0xC1A9);
    let mut bp = Biquad::new(Eq::Bp(1150.0, 1.3));
    let mut hp = Biquad::new(Eq::Hp(600.0, 0.7));
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        let mut e = 0.0f32;
        for k in 0..3 {
            let tk = t - k as f32 * 0.0105;
            if tk >= 0.0 {
                e = e.max((-tk / 0.0045).exp());
            }
        }
        let tail = if t > 0.03 {
            (-(t - 0.03) / 0.11).exp() * 0.6
        } else {
            0.0
        };
        out.push(hp.tick(bp.tick(noise.sample())) * (e + tail) * 2.5);
    }
    out
}

/// Shaker: bright band-passed noise with a soft attack (the beads
/// landing).
pub fn shaker() -> Vec<f32> {
    let n = n_for(0.12);
    let mut noise = Noise(0x5A4E);
    let mut bp = Biquad::new(Eq::Bp(6800.0, 1.1));
    let mut hp = Biquad::new(Eq::Hp(4000.0, 0.7));
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        let e = (t / 0.014).min(1.0).powi(2) * (-(t - 0.014).max(0.0) / 0.035).exp();
        out.push(hp.tick(bp.tick(noise.sample())) * e);
    }
    out
}

/// A tuned membrane (conga, bongo, tom): a sine that drops a little in
/// pitch, an overtone and a slap of noise.
pub fn membrane(f: f32, decay: f32, drop: f32, slap: f32) -> Vec<f32> {
    let n = n_for(decay * 4.0);
    let mut noise = Noise(0x0C0A ^ f as u32);
    let mut bp = Biquad::new(Eq::Bp(f * 6.0, 1.0));
    let (mut p1, mut p2) = (0.0f32, 0.0f32);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        let bend = 1.0 + drop * (-t / 0.03).exp();
        p1 += f * bend / FS;
        p2 += f * 1.59 * bend / FS;
        let body = (p1 * TAU).sin() * env(t, decay) + 0.35 * (p2 * TAU).sin() * env(t, decay * 0.3);
        let hit = bp.tick(noise.sample()) * (-t / 0.007).exp() * slap;
        out.push((1.1 * (body + hit)).tanh());
    }
    out
}

/// A reversed cymbal: wash and shimmer rising over 1.5 s to a peak at the
/// very end (it's placed to land on the next downbeat).
pub fn swell() -> Vec<f32> {
    let mut c = metal(SWELL_SECS, 10.0, 1500.0, 1.0, 1800.0, 0x5E11);
    let n = c.len();
    for (i, x) in c.iter_mut().enumerate() {
        let p = i as f32 / n as f32;
        *x *= p * p * p;
    }
    c
}

/// How long [`swell`] runs.
pub const SWELL_SECS: f32 = 1.5;

/// Rimshot (cross-stick): a hollow woody click.
pub fn rim() -> Vec<f32> {
    let n = n_for(0.08);
    let mut noise = Noise(0x819);
    let mut bp = Biquad::new(Eq::Bp(2900.0, 1.4));
    let (mut p1, mut p2) = (0.0f32, 0.0f32);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        p1 += 1720.0 / FS;
        p2 += 470.0 / FS;
        let tone = ((p1 * TAU).sin() + 0.6 * (p2 * TAU).sin()) * env(t, 0.011);
        let click = bp.tick(noise.sample()) * (-t / 0.004).exp();
        out.push(tone * 0.7 + click);
    }
    out
}

/// The 808 cowbell: two detuned tones through a band-pass, with a fast
/// then slow decay.
pub fn cowbell() -> Vec<f32> {
    let n = n_for(0.3);
    let mut bp = Biquad::new(Eq::Bp(2200.0, 0.9));
    let (mut p1, mut p2) = (0.0f32, 0.0f32);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        p1 = (p1 + 562.0 / FS).fract();
        p2 = (p2 + 845.0 / FS).fract();
        let sq = |p: f32| {
            let x = p * TAU;
            x.sin() + (3.0 * x).sin() / 3.0 + (5.0 * x).sin() / 5.0
        };
        let e = 0.6 * env(t, 0.02) + 0.4 * env(t, 0.13);
        out.push(bp.tick(sq(p1) + sq(p2)) * e);
    }
    out
}

/// Timbale: a thin, high metal-shelled drum: a tight head, a ringing
/// shell partial and a bright stick slap.
pub fn timbale(f: f32) -> Vec<f32> {
    let n = n_for(0.6);
    let mut noise = Noise(0x71B ^ f as u32);
    let mut bp = Biquad::new(Eq::Bp(4200.0, 0.9));
    let (mut p1, mut p2, mut p3) = (0.0f32, 0.0f32, 0.0f32);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        let bend = 1.0 + 0.04 * (-t / 0.02).exp();
        p1 += f * bend / FS;
        p2 += f * 2.31 / FS;
        p3 += f * 3.73 / FS;
        let head = (p1 * TAU).sin() * env(t, 0.12);
        let shell = (0.4 * (p2 * TAU).sin() + 0.25 * (p3 * TAU).sin()) * env(t, 0.22);
        let slap = bp.tick(noise.sample()) * (-t / 0.012).exp() * 0.9;
        out.push((1.2 * (head + shell + slap)).tanh());
    }
    out
}

/// Cajón: a box drum. Low "bass" in the middle of the face (`slap` 0), or
/// a slap near the edge (`slap` 1) that sets the snare wires buzzing.
pub fn cajon(slap: f32) -> Vec<f32> {
    let n = n_for(0.35);
    let mut noise = Noise(0xCA70 + (slap * 9.0) as u32);
    let mut wires = Biquad::new(Eq::Bp(2600.0, 0.8));
    let mut knock = Biquad::new(Eq::Bp(900.0, 1.2));
    let mut ph = 0.0f32;
    let f = 80.0 + 110.0 * slap;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        ph += f * (1.0 + 0.25 * (-t / 0.015).exp()) / FS;
        let body = (ph * TAU).sin() * env(t, 0.09 - 0.04 * slap);
        let w = noise.sample();
        let buzz = wires.tick(w) * env(t, 0.05 + 0.04 * slap) * (0.3 + 0.9 * slap);
        let hand = knock.tick(w) * (-t / 0.006).exp() * (0.4 + 0.6 * slap);
        out.push((1.3 * (body * (1.2 - 0.6 * slap) + buzz + hand)).tanh());
    }
    out
}

/// Tabla. The small dayan rings with near-harmonic overtones (the black
/// syahi patch tunes them); the big bayan's "ge" swoops up in pitch.
pub fn tabla(bayan: bool) -> Vec<f32> {
    let n = n_for(if bayan { 0.7 } else { 0.8 });
    let mut noise = Noise(if bayan { 0xBA7A } else { 0xDA7A });
    let mut bp = Biquad::new(Eq::Bp(if bayan { 700.0 } else { 3000.0 }, 1.0));
    let mut ph = [0.0f32; 4];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        let x = if bayan {
            // The heel of the hand presses in: the pitch rises.
            let f = 85.0 * (1.0 + 0.45 * (1.0 - (-t / 0.12).exp()));
            ph[0] += f / FS;
            (ph[0] * TAU).sin() * env(t, 0.28)
        } else {
            let f = 330.0;
            let mut y = 0.0;
            for (k, (p, a)) in ph.iter_mut().zip([1.0, 0.5, 0.35, 0.2]).enumerate() {
                *p += f * (k + 1) as f32 * 1.003f32.powi(k as i32) / FS;
                y += a * (*p * TAU).sin() * env(t, 0.35 / (1.0 + k as f32 * 0.8));
            }
            y
        };
        let tap = bp.tick(noise.sample()) * (-t / 0.004).exp() * 0.5;
        out.push((1.1 * (x + tap)).tanh());
    }
    out
}

/// Tambourine: a shake of jingles (inharmonic metal pings and bright
/// noise, a little after the hand hits) on a soft skin thud.
pub fn tambourine() -> Vec<f32> {
    let n = n_for(0.35);
    let mut noise = Noise(0x7A3B);
    let mut hp = Biquad::new(Eq::Hp(6000.0, 0.7));
    let mut bp = Biquad::new(Eq::Bp(8500.0, 1.2));
    let jingles = [5310.0, 6870.0, 7410.0, 9130.0];
    let mut ph = [0.0f32; 4];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        // The jingles clash twice: on the hit and as they fall back.
        let e = env(t, 0.06) + 0.5 * env((t - 0.03).max(0.0), 0.09) * (t > 0.03) as u8 as f32;
        let mut ring = 0.0;
        for (p, f) in ph.iter_mut().zip(jingles) {
            *p = (*p + f / FS).fract();
            ring += (*p * TAU).sin();
        }
        let hiss = bp.tick(hp.tick(noise.sample()));
        let thud = (t * 180.0 * TAU).sin() * env(t, 0.02) * 0.3;
        out.push((ring * 0.15 + hiss * 1.2) * e + thud);
    }
    out
}

/// Güiro: a stick scraped along a ridged gourd: a fast train of clicks
/// through the gourd's hollow resonance.
pub fn guiro() -> Vec<f32> {
    let n = n_for(0.3);
    let mut noise = Noise(0x6E1);
    let mut body = Biquad::new(Eq::Bp(2800.0, 2.0));
    let mut hp = Biquad::new(Eq::Hp(1200.0, 0.7));
    let mut out = Vec::with_capacity(n);
    let mut next = 0.0f32;
    let mut click = 0.0f32;
    for i in 0..n {
        let t = i as f32 / FS;
        // About 45 ridges a second, the stroke slowing at the end.
        if t >= next && t < 0.22 {
            click = 1.0;
            next = t + 1.0 / (55.0 - 60.0 * t);
        }
        click *= 0.86;
        let x = body.tick(hp.tick(noise.sample() * click + click * 0.5));
        let shape = (t / 0.02).min(1.0) * (1.0 - ((t - 0.2) / 0.08).clamp(0.0, 1.0));
        out.push(x * shape);
    }
    out
}

/// Claves: two hardwood sticks: one pure, high ping.
pub fn claves() -> Vec<f32> {
    let n = n_for(0.15);
    let mut out = Vec::with_capacity(n);
    let (mut p1, mut p2) = (0.0f32, 0.0f32);
    for i in 0..n {
        let t = i as f32 / FS;
        p1 = (p1 + 2500.0 / FS).fract();
        p2 = (p2 + 6800.0 / FS).fract();
        out.push((p1 * TAU).sin() * env(t, 0.035) + 0.15 * (p2 * TAU).sin() * env(t, 0.004));
    }
    out
}

/// Woodblock (or temple block at lower pitches): a hollow knock, two
/// resonances of the slot and a stick click.
pub fn woodblock(f: f32) -> Vec<f32> {
    let n = n_for(0.15);
    let mut noise = Noise(0x0B10 ^ f as u32);
    let mut click = Biquad::new(Eq::Bp(f * 3.0, 1.0));
    let (mut p1, mut p2) = (0.0f32, 0.0f32);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / FS;
        p1 = (p1 + f / FS).fract();
        p2 = (p2 + f * 1.62 / FS).fract();
        let knock = (p1 * TAU).sin() * env(t, 0.03) + 0.45 * (p2 * TAU).sin() * env(t, 0.015);
        let c = click.tick(noise.sample()) * (-t / 0.002).exp() * 0.6;
        out.push(knock + c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_drum_is_finite_and_audible() {
        let kit = [
            kick(),
            snare(0.15, 1.0),
            snare(0.06, 0.6),
            timbale(390.0),
            cajon(0.0),
            cajon(1.0),
            tabla(false),
            tabla(true),
            tambourine(),
            guiro(),
            claves(),
            woodblock(900.0),
            hat(),
            open_hat(),
            crash(),
            ride(),
            clap(),
            shaker(),
            membrane(200.0, 0.2, 0.12, 0.3),
            rim(),
            cowbell(),
        ];
        for (k, d) in kit.iter().enumerate() {
            assert!(d.iter().all(|x| x.is_finite()), "{k}");
            let peak = d.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            assert!(peak > 0.01, "{k} silent");
        }
    }
}
