//! cpal output backend (non-Linux, e.g. CoreAudio on macOS). Mirrors the
//! PipeWire backend: a dedicated thread owns a cpal output stream whose data
//! callback drives the shared `RenderCore`. The thread parks to keep the
//! stream (which is `!Send` on some hosts) alive and on one thread.

use crate::buffer::{StemSet, CHANNELS, SAMPLE_RATE};
use crate::error::Error;
use crate::pipeline::{EngineCmd, EngineEvent};
use crate::render_core::RenderCore;
use arc_swap::ArcSwapOption;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use std::thread::JoinHandle;

pub fn spawn(
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
) -> crate::error::Result<JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("earworm-audio".into())
        .spawn(move || {
            if let Err(e) = run(cmd_rx, evt_tx, song_slot) {
                eprintln!("earworm audio thread failed: {e}");
            }
        })?;
    Ok(handle)
}

fn run(
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
) -> crate::error::Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
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

    let mut core = RenderCore::new(cmd_rx, evt_tx, song_slot);

    let stream = device
        .build_output_stream(
            &config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // cpal hands us an interleaved f32 buffer sized to the device
                // request; RenderCore fills exactly out.len() samples.
                core.fill(out);
            },
            move |err| eprintln!("earworm cpal stream error: {err}"),
            None,
        )
        .map_err(|e| Error::Audio(format!("build output stream: {e}")))?;

    stream
        .play()
        .map_err(|e| Error::Audio(format!("play stream: {e}")))?;

    // The Engine owns this JoinHandle and never joins it; park to keep the
    // stream alive on this thread. park() may wake spuriously — re-parking is
    // harmless, we never need to do work here.
    loop {
        std::thread::park();
    }
}
