//! Voice chat audio in the browser (#166). Only compiled for `wasm32`.
//!
//! `bevy_voice_chat` reaches audio hardware only through its `AudioIo` seam.
//! On the desktop that is cpal on its own threads. The browser has no threads,
//! so this is a Web Audio version: `getUserMedia` for the microphone and two
//! `ScriptProcessorNode`s (mic in, voices out) on one shared `AudioContext`.
//! Their callbacks run on the main thread between frames and only touch the
//! same bounded channels and atomics the cpal threads do. Everything else
//! (codec, gate, jitter buffers, proximity mixing, the replicon messages) is
//! the same code as on the desktop, so browser and desktop peers hear each
//! other.
//!
//! Browsers keep audio asleep until the player touches the page, so the
//! context is resumed on the first click, tap or key press. The microphone
//! needs the page's permission prompt and a secure page (https or localhost).
//! Saying no, or having no mic, leaves you able to hear everyone.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError};

use bevy::prelude::*;
use bevy_voice_chat::codec::{MonoFrame, VOICE_RATE};
use bevy_voice_chat::dsp::FrameAssembler;
use bevy_voice_chat::io::{
    AudioIo, DEVICE_FAILED, DEVICE_LIVE, DeviceLists, MicSession, MicShared, OutputPump,
    OutputSession, OutputShared, ScanJob,
};
use bevy_voice_chat::mixer::StereoFrame;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioContext, AudioProcessingEvent, MediaStream, MediaStreamTrack};

/// The one microphone the browser offers (it picks which device itself).
pub const MIC_LABEL: &str = "Browser microphone";
/// The one output the browser offers.
pub const SPEAKERS_LABEL: &str = "Browser speakers";
/// Samples per callback (about 21 ms at 48 kHz).
const BLOCK: u32 = 1024;

/// Web Audio behind `bevy_voice_chat`'s device seam.
#[derive(Clone, Copy, Debug, Default)]
pub struct WebAudioIo;

thread_local! {
    static CONTEXT: RefCell<Option<AudioContext>> = const { RefCell::new(None) };
}

/// The shared `AudioContext`, made on first use and woken by the first
/// gesture on the page.
fn context() -> Result<AudioContext, JsValue> {
    CONTEXT.with(|c| {
        if let Some(ctx) = c.borrow().as_ref() {
            return Ok(ctx.clone());
        }
        let ctx = AudioContext::new()?;
        wake_on_gesture(&ctx);
        *c.borrow_mut() = Some(ctx.clone());
        Ok(ctx)
    })
}

/// Resume the context now and on every click, tap or key press (a no-op once
/// it runs; iOS Safari also suspends it again when the page is hidden).
fn wake_on_gesture(ctx: &AudioContext) {
    let _ = ctx.resume();
    let Some(window) = web_sys::window() else {
        return;
    };
    let ctx = ctx.clone();
    let wake = Closure::<dyn FnMut()>::new(move || {
        if ctx.state() != web_sys::AudioContextState::Running {
            let _ = ctx.resume();
        }
    });
    for event in ["pointerdown", "touchend", "keydown", "click"] {
        let _ = window.add_event_listener_with_callback(event, wake.as_ref().unchecked_ref());
    }
    // Lives as long as the page.
    wake.forget();
}

fn js_err(e: JsValue) -> String {
    e.as_string()
        .or_else(|| {
            e.dyn_ref::<js_sys::Error>()
                .map(|e| String::from(e.message()))
        })
        .unwrap_or_else(|| format!("{e:?}"))
}

/// The session was dropped (the game closed the device).
fn stopped(stop: &std::sync::mpsc::Receiver<()>) -> bool {
    matches!(stop.try_recv(), Err(TryRecvError::Disconnected))
}

fn stop_tracks(stream: &MediaStream) {
    for t in stream.get_tracks().iter() {
        if let Ok(t) = t.dyn_into::<MediaStreamTrack>() {
            t.stop();
        }
    }
}

async fn start_mic(
    shared: Arc<MicShared>,
    frames: SyncSender<MonoFrame>,
    stop: std::sync::mpsc::Receiver<()>,
) -> Result<(), JsValue> {
    let ctx = context()?;
    let window = web_sys::window().ok_or("no window")?;
    let devices = window.navigator().media_devices()?;
    let constraints = web_sys::MediaStreamConstraints::new();
    // The browser's own echo cancelling keeps friends' voices out of your mic.
    let audio = js_sys::Object::new();
    for key in ["echoCancellation", "noiseSuppression", "autoGainControl"] {
        js_sys::Reflect::set(&audio, &key.into(), &true.into())?;
    }
    constraints.set_audio(&audio);
    let stream: MediaStream =
        JsFuture::from(devices.get_user_media_with_constraints(&constraints)?)
            .await?
            .dyn_into()?;
    if stopped(&stop) {
        stop_tracks(&stream);
        return Ok(());
    }
    let rate = ctx.sample_rate() as u32;
    shared.rate.store(rate, Ordering::Relaxed);
    shared.channels.store(1, Ordering::Relaxed);
    shared
        .latency_us
        .store(BLOCK * 1_000_000 / rate.max(1), Ordering::Relaxed);
    let source = ctx.create_media_stream_source(&stream)?;
    let node = ctx
        .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
            BLOCK, 1, 1,
        )?;
    // A processor only runs when it leads somewhere: into the speakers, muted.
    let mute = ctx.create_gain()?;
    mute.gain().set_value(0.0);

    let mut assembler = FrameAssembler::new(rate, 1, VOICE_RATE);
    let mut buf = vec![0.0_f32; BLOCK as usize];
    let (me, src, sink, meter) = (node.clone(), source.clone(), mute.clone(), shared.clone());
    let on_audio =
        Closure::<dyn FnMut(AudioProcessingEvent)>::new(move |ev: AudioProcessingEvent| {
            if stopped(&stop) {
                me.set_onaudioprocess(None);
                let _ = src.disconnect();
                let _ = me.disconnect();
                let _ = sink.disconnect();
                stop_tracks(&stream);
                return;
            }
            let Ok(input) = ev.input_buffer() else {
                return;
            };
            let n = input.length() as usize;
            if buf.len() < n {
                buf.resize(n, 0.0);
            }
            let Some(block) = buf.get_mut(..n) else {
                return;
            };
            if input.copy_from_channel(block, 0).is_err() {
                return;
            }
            let mut loudest = 0.0_f32;
            assembler.push_with(
                block,
                |s: f32| {
                    if s.is_finite() {
                        loudest = loudest.max(s.abs());
                    }
                    s
                },
                |frame| {
                    if let Err(TrySendError::Full(_)) = frames.try_send(*frame) {
                        meter.overflow.fetch_add(1, Ordering::Relaxed);
                    }
                },
            );
            meter.publish_peak(loudest);
        });
    node.set_onaudioprocess(Some(on_audio.as_ref().unchecked_ref()));
    // Unhooked (above) when the session closes; the closure itself is tiny.
    on_audio.forget();
    source.connect_with_audio_node(&node)?;
    node.connect_with_audio_node(&mute)?;
    mute.connect_with_audio_node(&ctx.destination())?;
    if let Ok(mut d) = shared.device.lock() {
        *d = MIC_LABEL.into();
    }
    shared.set_state(DEVICE_LIVE);
    Ok(())
}

fn start_output(
    shared: Arc<OutputShared>,
    frames: Receiver<StereoFrame>,
    stop: std::sync::mpsc::Receiver<()>,
) -> Result<(), JsValue> {
    let ctx = context()?;
    let rate = ctx.sample_rate() as u32;
    shared.rate.store(rate, Ordering::Relaxed);
    shared
        .latency_us
        .store(BLOCK * 1_000_000 / rate.max(1), Ordering::Relaxed);
    // One (unused) input: older Safari won't run a processor without one.
    let node = ctx
        .create_script_processor_with_buffer_size_and_number_of_input_channels_and_number_of_output_channels(
            BLOCK, 1, 2,
        )?;
    let mut pump = OutputPump::new(frames, VOICE_RATE, rate, shared.clone());
    let mut mixed = vec![0.0_f32; BLOCK as usize * 2];
    let mut left = vec![0.0_f32; BLOCK as usize];
    let mut right = vec![0.0_f32; BLOCK as usize];
    let me = node.clone();
    let on_audio =
        Closure::<dyn FnMut(AudioProcessingEvent)>::new(move |ev: AudioProcessingEvent| {
            if stopped(&stop) {
                me.set_onaudioprocess(None);
                let _ = me.disconnect();
                return;
            }
            let Ok(out) = ev.output_buffer() else {
                return;
            };
            let n = out.length() as usize;
            if left.len() < n {
                mixed.resize(n * 2, 0.0);
                left.resize(n, 0.0);
                right.resize(n, 0.0);
            }
            let (Some(m), Some(l), Some(r)) = (
                mixed.get_mut(..n * 2),
                left.get_mut(..n),
                right.get_mut(..n),
            ) else {
                return;
            };
            pump.fill(m, 2);
            crate::voice::deinterleave(m, l, r);
            let _ = out.copy_to_channel(l, 0);
            let _ = out.copy_to_channel(r, 1);
        });
    node.set_onaudioprocess(Some(on_audio.as_ref().unchecked_ref()));
    on_audio.forget();
    node.connect_with_audio_node(&ctx.destination())?;
    if let Ok(mut d) = shared.device.lock() {
        *d = SPEAKERS_LABEL.into();
    }
    shared.set_state(DEVICE_LIVE);
    Ok(())
}

impl AudioIo for WebAudioIo {
    fn start_scan(&self) -> ScanJob {
        let job = ScanJob::default();
        job.deliver(DeviceLists {
            inputs: vec![MIC_LABEL.into()],
            outputs: vec![SPEAKERS_LABEL.into()],
        });
        job
    }

    fn open_mic(&self, device: Option<String>) -> MicSession {
        let (session, frames, stop) = MicSession::new(device);
        let shared = session.shared.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = start_mic(shared.clone(), frames, stop).await {
                warn!(
                    "voice chat: no microphone in the browser ({}), you can still listen",
                    js_err(e)
                );
                shared.set_state(DEVICE_FAILED);
            }
        });
        session
    }

    fn open_output(&self, device: Option<String>) -> OutputSession {
        let (session, frames, stop) = OutputSession::new(device);
        if let Err(e) = start_output(session.shared.clone(), frames, stop) {
            warn!("voice chat: no audio output in the browser ({})", js_err(e));
            session.shared.set_state(DEVICE_FAILED);
        }
        session
    }

    fn mic_kind(&self) -> String {
        "browser microphone".into()
    }
}
