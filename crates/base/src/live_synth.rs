//! Live synthesis in the audio graph (not a friend idea).
//!
//! A [`LiveNode`] is a firewheel node with no inputs and two outputs that makes
//! its sound on the audio thread, a sample at a time, from a generator the
//! game hands it ([`play`]): nothing is rendered ahead of time and there is no
//! sample asset to load. It plays into the sound effects bus, ends when the
//! generator runs out, and is despawned then (which takes it out of the graph).
//!
//! ```text
//!  game ── play(make) ── LiveNode entity ── processor builds the generator at the graph's rate
//!  audio thread: generator() -> Some(sample) | None (ended) -> both channels x volume
//! ```

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy_seedling::prelude::{Connect, RegisterNode, SoundEffectsBus};
use firewheel::channel_config::{ChannelConfig, ChannelCount};
use firewheel::node::{
    AudioNode, AudioNodeInfo, AudioNodeProcessor, ConstructProcessorContext, EmptyConfig,
    NodeError, ProcBuffers, ProcExtra, ProcInfo, ProcessStatus,
};

/// The next mono sample of a live sound, or `None` once it has finished.
pub type Generator = Box<dyn FnMut() -> Option<f32> + Send>;

/// Builds the generator at the graph's sample rate (on the audio thread).
type Factory = Box<dyn FnOnce(u32) -> Generator + Send>;

/// A live-synthesized sound in the graph: one node per playback.
#[derive(Component, Clone)]
pub struct LiveNode {
    make: Arc<Mutex<Option<Factory>>>,
    volume: f32,
    done: Arc<AtomicBool>,
}

impl AudioNode for LiveNode {
    type Configuration = EmptyConfig;

    fn info(&self, _: &EmptyConfig) -> Result<AudioNodeInfo, NodeError> {
        Ok(AudioNodeInfo::new()
            .debug_name("live synth")
            .channel_config(ChannelConfig {
                num_inputs: ChannelCount::ZERO,
                num_outputs: ChannelCount::STEREO,
            }))
    }

    fn construct_processor(
        &self,
        _: &EmptyConfig,
        cx: ConstructProcessorContext,
    ) -> Result<impl AudioNodeProcessor, NodeError> {
        // Taken once; a rebuilt node plays nothing.
        let factory = self.make.lock().ok().and_then(|mut f| f.take());
        let rate = cx.stream_info.sample_rate.get();
        Ok(LiveProcessor {
            gen_: factory.map(|f| f(rate)),
            volume: self.volume,
            done: self.done.clone(),
        })
    }
}

struct LiveProcessor {
    gen_: Option<Generator>,
    volume: f32,
    done: Arc<AtomicBool>,
}

impl AudioNodeProcessor for LiveProcessor {
    fn process(
        &mut self,
        info: &ProcInfo,
        ProcBuffers { outputs, .. }: ProcBuffers,
        _: &mut ProcExtra,
    ) -> ProcessStatus {
        let [l, r, ..] = outputs else {
            return ProcessStatus::ClearAllOutputs;
        };
        let Some(gen_) = self.gen_.as_mut() else {
            self.done.store(true, Ordering::Relaxed);
            return ProcessStatus::ClearAllOutputs;
        };
        let n = info.frames.min(l.len()).min(r.len());
        for i in 0..n {
            match gen_() {
                Some(s) => {
                    let s = s.clamp(-1.0, 1.0) * self.volume;
                    l[i] = s;
                    r[i] = s;
                }
                None => {
                    self.gen_ = None;
                    self.done.store(true, Ordering::Relaxed);
                    l[i..n].fill(0.0);
                    r[i..n].fill(0.0);
                    break;
                }
            }
        }
        ProcessStatus::OutputsModified
    }
}

/// Play a live sound once, flat (not placed in the world), at `volume`. `make`
/// is called with the graph's sample rate and returns the generator that
/// makes the sound a sample at a time. Returns its entity (despawn to stop).
pub fn play(
    commands: &mut Commands,
    volume: f32,
    make: impl FnOnce(u32) -> Generator + Send + 'static,
) -> Entity {
    let node = LiveNode {
        make: Arc::new(Mutex::new(Some(Box::new(make)))),
        volume,
        done: Arc::new(AtomicBool::new(false)),
    };
    let e = commands.spawn(node).id();
    commands.entity(e).connect(SoundEffectsBus);
    e
}

/// A sound that has finished (the audio thread says so) is despawned, which
/// takes its node out of the graph.
fn end_sounds(mut commands: Commands, sounds: Query<(Entity, &LiveNode)>) {
    for (e, node) in &sounds {
        if node.done.load(Ordering::Relaxed) {
            commands.entity(e).despawn();
        }
    }
}

/// Register the node (the sound plugin calls this when there is an audio graph).
pub fn install(app: &mut App) {
    app.register_simple_node::<LiveNode>()
        .add_systems(Update, end_sounds);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generator_that_runs_out_ends_the_sound() {
        let mut n = 0;
        let mut gen_: Generator = Box::new(move || {
            n += 1;
            (n <= 3).then_some(0.5)
        });
        assert_eq!(
            (0..5).map(|_| gen_()).collect::<Vec<_>>(),
            vec![Some(0.5), Some(0.5), Some(0.5), None, None]
        );
    }
}
