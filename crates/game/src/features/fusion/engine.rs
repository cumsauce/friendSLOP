//! The real-time engine: asks the composer for a bar at every bar line,
//! fires its notes at the right samples and mixes the band. Allocation-free
//! once built, so it can run inside the audio callback.

use std::sync::Arc;

use super::compose::{Bar, Composer, Cue, Mood, Section};
use super::synth::{BLOCK, Band, Bank};

pub struct Engine {
    pub composer: Composer,
    band: Band,
    bar: Bar,
    /// Samples into the current bar.
    pos: u32,
    next_ev: usize,
    started: bool,
}

impl Engine {
    pub fn new(bank: Arc<Bank>, seed: u64) -> Self {
        Self {
            composer: Composer::new(seed),
            band: Band::new(bank),
            bar: Bar::new(),
            pos: 0,
            next_ev: 0,
            started: false,
        }
    }

    pub fn mood(&self) -> Mood {
        self.composer.mood
    }

    pub fn section(&self) -> Section {
        self.composer.section
    }

    /// Samples into the current bar (0 right at a bar line).
    pub fn pos_in_bar(&self) -> u32 {
        self.pos
    }

    pub fn active_voices(&self) -> usize {
        self.band.active_voices()
    }

    /// Fill interleaved stereo `out`, asking `cue` for the game's wishes
    /// at each bar line it crosses.
    pub fn render(&mut self, out: &mut [f32], cue: &mut impl FnMut() -> Cue) {
        let frames = out.len() / 2;
        let mut done = 0;
        while done < frames {
            if !self.started || self.pos >= self.bar.len {
                self.started = true;
                self.pos = 0;
                self.next_ev = 0;
                self.composer.next_bar(cue(), &mut self.bar);
                self.band.bus_target = self.bar.bus;
            }
            while let Some(ev) = self.bar.events.get(self.next_ev) {
                if ev.at > self.pos {
                    break;
                }
                self.band.play(ev);
                self.next_ev += 1;
            }
            let until = self
                .bar
                .events
                .get(self.next_ev)
                .map_or(self.bar.len, |e| e.at)
                .min(self.bar.len);
            let span = ((until - self.pos) as usize)
                .clamp(1, BLOCK)
                .min(frames - done);
            self.band.render(&mut out[done * 2..(done + span) * 2]);
            self.pos += span as u32;
            done += span;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::fusion::synth::RATE;
    use std::sync::OnceLock;

    fn bank() -> Arc<Bank> {
        static BANK: OnceLock<Arc<Bank>> = OnceLock::new();
        BANK.get_or_init(|| Arc::new(Bank::build())).clone()
    }

    fn cue(mood: Mood) -> Cue {
        Cue {
            mood,
            intensity: mood.base_intensity(),
            victories: 0,
        }
    }

    /// Renders a tour of every mood: finite, never clipping, never silent
    /// for long, and within the polyphony budget.
    #[test]
    fn render_is_clean() {
        let mut e = Engine::new(bank(), 11);
        let mut buf = vec![0.0f32; 1024];
        let mut peak = 0.0f32;
        let mut max_voices = 0;
        for m in Mood::ALL {
            let mut sq = 0.0f64;
            let mut n = 0usize;
            for _ in 0..(RATE as usize * 6 / 512) {
                e.render(&mut buf, &mut || cue(m));
                for x in &buf {
                    assert!(x.is_finite());
                    peak = peak.max(x.abs());
                    sq += (*x as f64).powi(2);
                }
                n += buf.len();
                max_voices = max_voices.max(e.active_voices());
            }
            let rms = (sq / n as f64).sqrt();
            assert!(rms > 0.003, "{m:?} nearly silent: {rms}");
        }
        assert!(peak < 1.0, "peak {peak}");
        assert!(max_voices <= 25, "voices {max_voices}");
    }

    /// The director only ever changes mood or section at a bar line, however
    /// the cue changes mid-bar.
    #[test]
    fn transitions_only_at_bar_lines() {
        let mut e = Engine::new(bank(), 4);
        let order = [
            Mood::Village,
            Mood::Duel,
            Mood::Still,
            Mood::Forest,
            Mood::Hiding,
            Mood::Seeking,
            Mood::Abuela,
        ];
        let mut buf = vec![0.0f32; 2 * 97]; // odd block size on purpose
        let mut mood = e.mood();
        let mut section = e.section();
        let mut changes = 0;
        let mut t = 0usize;
        let mut victories = 0;
        for k in 0..(RATE as usize * 70 / 97) {
            let target = order[(t / (RATE as usize * 5)) % order.len()];
            if k % 3000 == 1500 {
                victories += 1;
            }
            e.render(&mut buf, &mut || Cue {
                mood: target,
                intensity: 0.6,
                victories,
            });
            t += 97;
            if e.mood() != mood || e.section() != section {
                // The change happened at the bar line inside this block.
                assert!(
                    e.pos_in_bar() <= 97,
                    "changed {} into a bar",
                    e.pos_in_bar()
                );
                mood = e.mood();
                section = e.section();
                changes += 1;
            }
        }
        assert!(changes >= 6, "only {changes} changes");
    }

    /// Renders a ~100 s tour of the moods (with a victory) to a WAV, and
    /// prints levels and speed:
    /// `FUSION_DEMO_WAV=out.wav cargo test -p friendslop fusion_demo -- --ignored --nocapture`
    #[test]
    #[ignore = "writes a WAV file; run by hand to listen"]
    fn fusion_demo() {
        const TOUR: &[(f32, Mood, f32)] = &[
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
        const SECS: f32 = 106.0;
        const VICTORY_AT: f32 = 25.0;
        let path = std::env::var("FUSION_DEMO_WAV").unwrap_or_else(|_| "fusion_demo.wav".into());
        let t0 = std::time::Instant::now();
        let bank = Arc::new(Bank::build());
        println!("sample bank: {:?}", t0.elapsed());
        let mut engine = Engine::new(bank, 2024);
        let total = (SECS * RATE as f32) as usize;
        let mut out = Vec::with_capacity(total * 2);
        let mut buf = vec![0.0f32; 512];
        let mut t = 0usize;
        let mut last = None;
        let start = std::time::Instant::now();
        while t < total {
            let secs = t as f32 / RATE as f32;
            let (mood, intensity) = TOUR
                .iter()
                .rev()
                .find(|(at, _, _)| secs >= *at)
                .map_or((Mood::Menu, 0.5), |(_, m, i)| (*m, *i));
            let victories = u32::from(secs >= VICTORY_AT);
            engine.render(&mut buf, &mut || Cue {
                mood,
                intensity,
                victories,
            });
            let now = Some((engine.mood(), engine.section()));
            if now != last {
                println!("{secs:7.2} s  {:?} / {:?}", engine.mood(), engine.section());
                last = now;
            }
            out.extend_from_slice(&buf);
            t += buf.len() / 2;
        }
        let took = start.elapsed().as_secs_f32();
        println!(
            "rendered {SECS} s in {took:.2} s: {:.0}x real time = {:.2}% of one core",
            SECS / took,
            100.0 * took / SECS
        );
        for (k, (at, m, _)) in TOUR.iter().enumerate() {
            let end = TOUR.get(k + 1).map_or(SECS, |n| n.0);
            let a = (*at * RATE as f32) as usize * 2;
            let b = ((end * RATE as f32) as usize * 2).min(out.len());
            let seg = &out[a..b];
            let peak = seg.iter().fold(0.0f32, |p, x| p.max(x.abs()));
            let rms = (seg.iter().map(|x| (x * x) as f64).sum::<f64>() / seg.len() as f64).sqrt();
            println!(
                "{m:>9?}: peak {:6.1} dBFS, rms {:6.1} dBFS",
                20.0 * peak.log10(),
                20.0 * (rms as f32).log10()
            );
        }
        let peak = out.iter().fold(0.0f32, |p, x| p.max(x.abs()));
        let clipped = out.iter().filter(|x| x.abs() >= 1.0).count();
        println!("overall peak {peak:.3}, clipped samples {clipped}");
        assert!(out.iter().all(|x| x.is_finite()) && clipped == 0);

        let data_len = (out.len() * 2) as u32;
        let mut wav = Vec::with_capacity(44 + out.len() * 2);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data_len).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&2u16.to_le_bytes());
        wav.extend_from_slice(&RATE.to_le_bytes());
        wav.extend_from_slice(&(RATE * 4).to_le_bytes());
        wav.extend_from_slice(&4u16.to_le_bytes());
        wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data_len.to_le_bytes());
        for x in &out {
            wav.extend_from_slice(&((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        assert!(std::fs::write(&path, wav).is_ok(), "can't write {path}");
        println!("wrote {path}");
    }
}
