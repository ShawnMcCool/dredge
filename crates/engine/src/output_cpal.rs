//! cpal output backend (non-Linux, e.g. CoreAudio on macOS). Mirrors the
//! PipeWire backend: a dedicated thread owns a cpal output stream whose data
//! callback drives the shared `RenderCore`. The thread parks to keep the
//! stream (which is `!Send` on some hosts) alive and on one thread.

use crate::buffer::{StemSet, CHANNELS, SAMPLE_RATE};
use crate::error::Error;
use crate::pipeline::{ClickMark, EngineCmd, EngineEvent};
use crate::render_core::RenderCore;
use crate::stream_clock::StreamClock;
use arc_swap::ArcSwapOption;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

#[allow(clippy::too_many_arguments)]
pub fn spawn(
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
    click_slot: Arc<ArcSwapOption<Vec<ClickMark>>>,
    layer_slot: Arc<ArcSwapOption<Vec<crate::layers::Layer>>>,
    playback_clock: Arc<StreamClock>,
    target: Option<String>,
    stop: Arc<AtomicBool>,
) -> crate::error::Result<JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("dredge-audio".into())
        .spawn(move || {
            if let Err(e) = run(
                cmd_rx,
                evt_tx,
                song_slot,
                click_slot,
                layer_slot,
                playback_clock,
                target,
                stop,
            ) {
                eprintln!("dredge audio thread failed: {e}");
            }
        })?;
    Ok(handle)
}

#[allow(clippy::too_many_arguments)]
fn run(
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
    click_slot: Arc<ArcSwapOption<Vec<ClickMark>>>,
    layer_slot: Arc<ArcSwapOption<Vec<crate::layers::Layer>>>,
    // cpal exposes no graph-clock `pw_time`, so the playback clock is not
    // published on this backend; accepted to keep one `spawn` signature.
    _playback_clock: Arc<StreamClock>,
    target: Option<String>,
    stop: Arc<AtomicBool>,
) -> crate::error::Result<()> {
    let host = cpal::default_host();
    // Pick the device whose name matches `target`; fall back to the default.
    let device = match &target {
        Some(name) => host
            .output_devices()
            .ok()
            .and_then(|mut ds| {
                ds.find(|d| {
                    d.name()
                        .map(|n| n.as_str() == name.as_str())
                        .unwrap_or(false)
                })
            })
            .or_else(|| host.default_output_device()),
        None => host.default_output_device(),
    }
    .ok_or_else(|| Error::Audio("no default output device".into()))?;

    // Request the engine's native format (48 kHz stereo f32). CoreAudio
    // devices support 48 kHz; if a host doesn't, build_output_stream errors
    // and we surface it rather than silently resampling (resampling fallback
    // is a documented follow-up).
    let config = cpal::StreamConfig {
        channels: CHANNELS as u16,
        sample_rate: cpal::SampleRate(SAMPLE_RATE),
        buffer_size: cpal::BufferSize::Default,
    };

    let mut core = RenderCore::new(cmd_rx, evt_tx, song_slot, click_slot, layer_slot);

    let stream = device
        .build_output_stream(
            &config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // cpal hands us an interleaved f32 buffer sized to the device
                // request; RenderCore fills exactly out.len() samples.
                core.fill(out);
            },
            move |err| eprintln!("dredge cpal stream error: {err}"),
            None,
        )
        .map_err(|e| Error::Audio(format!("build output stream: {e}")))?;

    stream
        .play()
        .map_err(|e| Error::Audio(format!("play stream: {e}")))?;

    // Keep the stream alive on this thread, parking until the engine signals a
    // teardown (retarget). A short park timeout polls `stop`; park() may also
    // wake spuriously, which is harmless — we just re-check and re-park.
    while !stop.load(Ordering::Relaxed) {
        std::thread::park_timeout(std::time::Duration::from_millis(100));
    }
    drop(stream);
    Ok(())
}
