//! cpal input capture (non-Linux). Enumerates input devices and taps the
//! chosen one into a rolling ring — the tuner's source on macOS. A dedicated
//! thread owns the cpal input stream (kept on one thread; parks until stopped),
//! mirroring the PipeWire capture thread model so `CaptureSession` is shared.

use crate::buffer::{CHANNELS, SAMPLE_RATE};
use crate::capture::{CaptureNode, CaptureSession};
use crate::error::Error;
use crate::ring::RollingRing;
use crate::stream_clock::StreamClock;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Enumerate input devices. `serial` carries the device index (used by
/// `start_capture` to re-select it); `app` is the device name.
pub fn list_input_sources() -> crate::error::Result<Vec<CaptureNode>> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| Error::Audio(format!("enumerate input devices: {e}")))?
        .enumerate();
    let mut out = Vec::new();
    for (idx, dev) in devices {
        let name = dev.name().unwrap_or_else(|_| format!("input {idx}"));
        out.push(CaptureNode {
            id: idx as u32,
            serial: idx as u64,
            app: name,
            media: String::new(),
        });
    }
    Ok(out)
}

/// Tap the chosen input device (`node.serial` == device index) into a rolling
/// ring of `buffer_secs`.
pub fn start_capture(node: CaptureNode, buffer_secs: f64) -> crate::error::Result<CaptureSession> {
    let ring = Arc::new(Mutex::new(RollingRing::with_secs(buffer_secs)));
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let ring = ring.clone();
        let stop = stop.clone();
        let target = node.serial as usize;
        std::thread::Builder::new()
            .name("dredge-cap".into())
            .spawn(move || {
                if let Err(e) = run_capture(target, ring, stop) {
                    eprintln!("dredge capture thread failed: {e}");
                }
            })?
    };
    Ok(CaptureSession::from_parts(
        ring,
        node,
        Arc::new(StreamClock::default()),
        stop,
        thread,
    ))
}

/// Tap an input by its opaque device id (the `AudioDevice.id` from
/// `device::list_input_devices`). On cpal that id is the device NAME, so this
/// selects the input device whose name matches `id`.
pub fn start_capture_by_id(id: &str, buffer_secs: f64) -> crate::error::Result<CaptureSession> {
    let ring = Arc::new(Mutex::new(RollingRing::with_secs(buffer_secs)));
    let stop = Arc::new(AtomicBool::new(false));
    let node = CaptureNode {
        id: 0,
        serial: 0,
        app: id.to_owned(),
        media: String::new(),
    };
    let thread = {
        let ring = ring.clone();
        let stop = stop.clone();
        let target = id.to_owned();
        std::thread::Builder::new()
            .name("dredge-cap".into())
            .spawn(move || {
                if let Err(e) = run_capture_named(&target, ring, stop) {
                    eprintln!("dredge capture thread failed: {e}");
                }
            })?
    };
    Ok(CaptureSession::from_parts(
        ring,
        node,
        Arc::new(StreamClock::default()),
        stop,
        thread,
    ))
}

/// Capture from the input device whose name matches `name`, falling back to the
/// default input device if no name matches.
fn run_capture_named(
    name: &str,
    ring: Arc<Mutex<RollingRing>>,
    stop: Arc<AtomicBool>,
) -> crate::error::Result<()> {
    let host = cpal::default_host();
    let device = host
        .input_devices()
        .map_err(|e| Error::Audio(format!("enumerate input devices: {e}")))?
        .find(|d| d.name().map(|n| n == name).unwrap_or(false))
        .or_else(|| host.default_input_device())
        .ok_or_else(|| Error::Audio("no input device".into()))?;
    run_capture_on(device, ring, stop)
}

fn run_capture(
    target: usize,
    ring: Arc<Mutex<RollingRing>>,
    stop: Arc<AtomicBool>,
) -> crate::error::Result<()> {
    let host = cpal::default_host();
    let device = host
        .input_devices()
        .map_err(|e| Error::Audio(format!("enumerate input devices: {e}")))?
        .nth(target)
        .or_else(|| host.default_input_device())
        .ok_or_else(|| Error::Audio("no input device".into()))?;
    run_capture_on(device, ring, stop)
}

/// Shared stream setup once a device has been chosen.
fn run_capture_on(
    device: cpal::Device,
    ring: Arc<Mutex<RollingRing>>,
    stop: Arc<AtomicBool>,
) -> crate::error::Result<()> {
    let config = cpal::StreamConfig {
        channels: CHANNELS as u16,
        sample_rate: cpal::SampleRate(SAMPLE_RATE),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Non-blocking like the PipeWire path: drop a buffer on the
                // rare contention rather than risk an xrun on the audio thread.
                if let Ok(mut r) = ring.try_lock() {
                    r.push(data);
                }
            },
            move |err| eprintln!("dredge cpal capture error: {err}"),
            None,
        )
        .map_err(|e| Error::Audio(format!("build input stream: {e}")))?;

    stream
        .play()
        .map_err(|e| Error::Audio(format!("play input stream: {e}")))?;

    while !stop.load(Ordering::Relaxed) {
        std::thread::park_timeout(Duration::from_millis(100));
    }
    Ok(())
}
