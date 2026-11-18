//! The real-time engine: asks the composer for a bar at every bar line,
//! fires its notes at the right samples and mixes the band. Allocation-free
//! once built, so it can run inside the audio callback.

use std::sync::Arc;

use super::compose::{Bar, Composer, Cue, Mood, SCORE, Section};
use super::synth::{BLOCK, Band, Bank, Ev, RATE};

pub struct Engine {
    pub composer: Composer,
    band: Band,
    bar: Bar,
    /// Samples into the current bar.
    pos: u32,
    next_ev: usize,
    started: bool,
    /// Stingers fired so far (the game's count), and the hits waiting to
    /// play at the next bar, which starts at once.
    stinger_seq: u32,
    stinger: Vec<Ev>,
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
            stinger_seq: 0,
            stinger: Vec::with_capacity(64),
        }
    }

    /// The game's stinger count and which one (`Score::stinger` index) it
    /// fired last. A new count cuts the bar short: the band starts its next
    /// bar now, with the stinger's hits on its first beat (no waiting for a
    /// bar line).
    pub fn set_stinger(&mut self, seq: u32, which: usize) {
        if seq == self.stinger_seq {
            return;
        }
        self.stinger_seq = seq;
        let score = SCORE.get();
        let Some(hits) = score.stinger(which) else {
            return;
        };
        self.stinger.clear();
        for h in hits.iter().take(self.stinger.capacity()) {
            let ms = |ms: f32| (ms.max(0.0) * 0.001 * RATE as f32) as u32;
            self.stinger.push(Ev {
                at: ms(h.ms),
                inst: h.inst,
                note: h.note,
                vel: h.vel,
                len: ms(h.len_ms),
                glide: false,
            });
        }
        // Ends the bar being played; the next one is planned on the spot.
        if self.started {
            self.pos = self.bar.len;
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

    /// Leave buses out of the mix (bit per bus), for analysing stems.
    pub fn set_mute(&mut self, mask: u8) {
        self.band.mute = mask;
    }

    /// Duck the music (1 = full level), e.g. under voice chat. Eased: down
    /// in a quarter second, back up over a second or so.
    /// The game's live signals, in [`crate::live::names`] order (0..1).
    pub fn set_live(&mut self, values: &[f32]) {
        for (slot, v) in self.band.live.iter_mut().zip(values) {
            *slot = v.clamp(0.0, 1.0);
        }
    }

    pub fn set_duck(&mut self, level: f32) {
        self.band.duck_target = level.clamp(0.0, 1.0);
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
                self.band.echo_song = self.bar.echo_mix;
                self.band.set_tempo(self.composer.bpm());
                self.band.master_target = self.bar.gain;
                self.band.bright = self.bar.bright;
                self.band.auto.clone_from(&self.bar.auto);
                self.band.auto_pos = self.bar.at_bar;
                self.band.auto_step = 1.0 / self.bar.len.max(1) as f32;
                self.band.follow.clone_from(&self.bar.follow);
                self.band.song_mix = self.bar.mix;
                self.band.mix_lanes.clone_from(&self.bar.mix_lanes);
                self.band.mix_pos = self.bar.at_bar;
                self.band.mix_step = 1.0 / self.bar.len.max(1) as f32;
                self.band.wet_target = self.bar.wet;
                self.band.echo_target = self.bar.echo;
                self.band.tone_target = self.bar.tone;
                if !self.stinger.is_empty() {
                    self.bar.events.append(&mut self.stinger);
                    self.bar.events.sort_unstable_by_key(|e| e.at);
                }
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
    use crate::synth::RATE;
    use std::sync::OnceLock;

    fn bank() -> Arc<Bank> {
        static BANK: OnceLock<Arc<Bank>> = OnceLock::new();
        BANK.get_or_init(|| Arc::new(Bank::build())).clone()
    }

    fn cue(mood: Mood) -> Cue {
        Cue {
            mood,
            intensity: mood.base_intensity(),
            ..Cue::default()
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
        for m in Mood::all() {
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
        assert!(max_voices <= 48, "voices {max_voices}");
    }

    /// The director only ever changes mood or section at a bar line, however
    /// the cue changes mid-bar.
    #[test]
    fn transitions_only_at_bar_lines() {
        let mut e = Engine::new(bank(), 4);
        let order = [
            Mood::named("Village"),
            Mood::named("Duel"),
            Mood::named("Still"),
            Mood::named("Forest"),
            Mood::named("Hiding"),
            Mood::named("Seeking"),
            Mood::named("Abuela"),
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
                ..Cue::default()
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

    /// A stinger cuts the bar short: the new mood starts at once with the
    /// hits on its first beat, instead of at the next bar line.
    #[test]
    fn a_stinger_cuts_in_at_once() {
        let mut e = Engine::new(bank(), 5);
        let (calm, chase) = (Mood::named("FultonAmbient"), Mood::named("FultonChase"));
        let mut buf = vec![0.0f32; 2 * 256];
        let mut peak = |e: &mut Engine, mood: Mood| {
            e.render(&mut buf, &mut || cue(mood));
            buf.iter().fold(0.0f32, |m, x| m.max(x.abs()))
        };
        // Well into a calm bar, so a bar line is far off.
        for _ in 0..(RATE as usize * 3 / 256) {
            peak(&mut e, calm);
        }
        assert_eq!(e.mood(), calm);
        let mut quiet = 0.0f32;
        for _ in 0..(RATE as usize / 4 / 256) {
            quiet = quiet.max(peak(&mut e, calm));
        }
        let which = SCORE
            .get()
            .stinger_names()
            .position(|n| n == "fulton_spotted")
            .expect("the fulton_spotted stinger");
        e.set_stinger(1, which);
        // The very next block is the new mood, hit and all, with no bar line waited for.
        let mut loud = peak(&mut e, chase);
        assert_eq!(e.mood(), chase);
        for _ in 0..(RATE as usize / 4 / 256) {
            loud = loud.max(peak(&mut e, chase));
        }
        assert!(loud > 1.5 * quiet.max(0.01), "stinger {loud} vs {quiet}");
        // The same count again does nothing.
        e.set_stinger(1, which);
        assert!(e.stinger.is_empty());
    }
}
