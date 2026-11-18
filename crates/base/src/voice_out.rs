//! Voice playback through the audio graph (not a friend idea).
//!
//! `bevy_voice_chat` used to open its own output stream next to the graph's.
//! [`VoiceOutIo`] wraps the real backend (cpal on the desktop, Web Audio in the
//! browser) for the device scan and the microphone, but its `open_output` hands
//! the mixed-voice channel to a [`VoiceOutNode`] on the [`VoiceBus`], so there
//! is one output stream and the master volume covers voice.
//!
//! ```text
//!  voice (any thread) ── open_output ── VoiceOutSlot ── Bevy system ── spawns node on VoiceBus
//!  node processor: OutputPump (16 kHz stereo frames -> graph rate) -> two output channels
//! ```
//!
//! The output device setting has no effect (the graph plays to the default
//! device); the session reports the default output as open.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy_seedling::prelude::{Connect, RegisterNode};
use bevy_voice_chat::codec::VOICE_RATE;
use bevy_voice_chat::io::{
    AudioIo, DEVICE_LIVE, MicSession, OutputPump, OutputSession, OutputShared, ScanJob,
};
use bevy_voice_chat::mixer::StereoFrame;
use firewheel::channel_config::{ChannelConfig, ChannelCount};
use firewheel::node::{
    AudioNode, AudioNodeInfo, AudioNodeProcessor, ConstructProcessorContext, EmptyConfig,
    NodeError, ProcBuffers, ProcExtra, ProcInfo, ProcessStatus,
};

use crate::sound::{VoiceBus, stream_rate};

/// What the session hands to the graph.
struct Handoff {
    frames: Receiver<StereoFrame>,
    stop: Receiver<()>,
    shared: Arc<OutputShared>,
}

/// Where `open_output` (on whatever thread voice calls it from) leaves a new
/// session for the Bevy side to pick up.
#[derive(Resource, Clone, Default)]
pub struct VoiceOutSlot(Arc<Mutex<Option<Handoff>>>);

/// The voice backend with its playback moved into the graph.
pub struct VoiceOutIo {
    inner: Arc<dyn AudioIo>,
    slot: VoiceOutSlot,
}

impl VoiceOutIo {
    pub fn new(inner: Arc<dyn AudioIo>, slot: VoiceOutSlot) -> Self {
        Self { inner, slot }
    }
}

impl AudioIo for VoiceOutIo {
    fn start_scan(&self) -> ScanJob {
        self.inner.start_scan()
    }

    fn open_mic(&self, device: Option<String>) -> MicSession {
        self.inner.open_mic(device)
    }

    fn open_output(&self, device: Option<String>) -> OutputSession {
        let (session, frames, stop) = OutputSession::new(device);
        *session
            .shared
            .device
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = "Default output".into();
        session.shared.set_state(DEVICE_LIVE);
        if let Ok(mut slot) = self.slot.0.lock() {
            *slot = Some(Handoff {
                frames,
                stop,
                shared: session.shared.clone(),
            });
        }
        session
    }

    fn mic_kind(&self) -> String {
        self.inner.mic_kind()
    }
}

/// The voice stream as a graph node: stereo out, no inputs. The receiver is
/// taken once, when the processor is built.
#[derive(Component, Clone)]
pub struct VoiceOutNode {
    feed: Arc<Mutex<Option<(Receiver<StereoFrame>, Arc<OutputShared>)>>>,
}

impl AudioNode for VoiceOutNode {
    type Configuration = EmptyConfig;

    fn info(&self, _: &EmptyConfig) -> Result<AudioNodeInfo, NodeError> {
        Ok(AudioNodeInfo::new()
            .debug_name("voice chat")
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
        let taken = self.feed.lock().ok().and_then(|mut f| f.take());
        let (rx, shared) = taken.unwrap_or_else(|| {
            // Already taken (a rebuilt node): play silence.
            let (_, rx) = mpsc::sync_channel(1);
            (rx, Arc::default())
        });
        let rate = cx.stream_info.sample_rate.get();
        shared.rate.store(rate, Ordering::Relaxed);
        Ok(VoiceOutProcessor::new(
            rx,
            shared,
            rate,
            cx.stream_info.max_block_frames.get() as usize,
        ))
    }
}

/// The audio-thread side: the pump and an interleaved scratch buffer.
struct VoiceOutProcessor {
    pump: OutputPump,
    scratch: Vec<f32>,
}

impl VoiceOutProcessor {
    fn new(
        rx: Receiver<StereoFrame>,
        shared: Arc<OutputShared>,
        rate: u32,
        max_frames: usize,
    ) -> Self {
        Self {
            pump: OutputPump::new(rx, VOICE_RATE, rate, shared),
            scratch: vec![0.0; max_frames * 2],
        }
    }

    /// Fill `left` and `right` with the next frames of voice.
    fn render(&mut self, left: &mut [f32], right: &mut [f32]) {
        let frames = left.len().min(right.len()).min(self.scratch.len() / 2);
        let scratch = &mut self.scratch[..frames * 2];
        self.pump.fill(scratch, 2);
        crate::voice::deinterleave(scratch, &mut left[..frames], &mut right[..frames]);
    }
}

impl AudioNodeProcessor for VoiceOutProcessor {
    fn process(
        &mut self,
        info: &ProcInfo,
        ProcBuffers { outputs, .. }: ProcBuffers,
        _: &mut ProcExtra,
    ) -> ProcessStatus {
        let [l, r, ..] = outputs else {
            return ProcessStatus::ClearAllOutputs;
        };
        let n = info.frames.min(l.len()).min(r.len());
        self.render(&mut l[..n], &mut r[..n]);
        ProcessStatus::OutputsModified
    }
}

/// The node in the graph now, and the stop signal of the session it plays.
#[derive(Resource, Default)]
struct VoiceOutLive {
    node: Option<Entity>,
    stop: Option<Mutex<Receiver<()>>>,
}

/// Wire voice playback into the graph (the backend keeps the scan and mic).
pub fn install(app: &mut App, backend: Arc<dyn AudioIo>) {
    let slot = VoiceOutSlot::default();
    app.insert_resource(bevy_voice_chat::VoiceIo(Arc::new(VoiceOutIo::new(
        backend,
        slot.clone(),
    ))))
    .insert_resource(slot)
    .init_resource::<VoiceOutLive>()
    .register_simple_node::<VoiceOutNode>()
    .add_systems(Update, attach_voice_out);
}

fn attach_voice_out(
    slot: Res<VoiceOutSlot>,
    rate: Option<Res<bevy_seedling::context::SampleRate>>,
    mut live: ResMut<VoiceOutLive>,
    mut commands: Commands,
) {
    // The session closed (dropped): its node goes.
    let closed = live.stop.as_ref().is_some_and(|s| {
        s.lock()
            .map(|s| matches!(s.try_recv(), Err(TryRecvError::Disconnected)))
            .unwrap_or(true)
    });
    let new = slot.0.lock().ok().and_then(|mut s| s.take());
    if closed || new.is_some() {
        if let Some(e) = live.node.take() {
            commands.entity(e).despawn();
        }
        live.stop = None;
    }
    let Some(h) = new else { return };
    h.shared
        .rate
        .store(stream_rate(rate.as_deref()), Ordering::Relaxed);
    let node = VoiceOutNode {
        feed: Arc::new(Mutex::new(Some((h.frames, h.shared)))),
    };
    let id = commands.spawn(node).id();
    commands.entity(id).connect(VoiceBus);
    live.node = Some(id);
    live.stop = Some(Mutex::new(h.stop));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_voice_chat::codec::FRAME;

    #[test]
    fn node_plays_queued_voice_and_silence_after() {
        let (session, rx, _stop) = OutputSession::new(None);
        // Same rate as the voice: the pump passes samples through.
        let mut p = VoiceOutProcessor::new(rx, session.shared.clone(), VOICE_RATE, 512);
        let mut frame: StereoFrame = [0.0; FRAME * 2];
        for i in 0..FRAME {
            frame[i * 2] = 0.5;
            frame[i * 2 + 1] = -0.25;
        }
        assert!(session.frames.send(frame).is_ok());
        let (mut l, mut r) = (vec![9.0; FRAME + 80], vec![9.0; FRAME + 80]);
        p.render(&mut l, &mut r);
        let loud = l.iter().filter(|x| x.abs() > 0.4).count();
        assert!(loud >= FRAME - 4, "voice came out: {loud}");
        assert!(r.iter().any(|x| *x < -0.2));
        assert!(
            l[FRAME + 60..].iter().all(|x| x.abs() < 1e-6),
            "then silence"
        );
        assert_eq!(session.shared.consumed.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn open_output_hands_over_the_receiver() {
        let slot = VoiceOutSlot::default();
        let io = VoiceOutIo::new(Arc::new(bevy_voice_chat::io::NullIo), slot.clone());
        let session = io.open_output(Some("anything".into()));
        assert_eq!(session.shared.state(), DEVICE_LIVE);
        let Some(h) = slot.0.lock().ok().and_then(|mut s| s.take()) else {
            panic!("handed over");
        };
        assert!(session.frames.send([0.0; FRAME * 2]).is_ok());
        assert!(h.frames.try_recv().is_ok(), "the same channel");
        drop(session);
        assert!(matches!(h.stop.try_recv(), Err(TryRecvError::Disconnected)));
    }
}
