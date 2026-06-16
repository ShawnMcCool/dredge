//! PipeWire app-node capture: discover application output streams and tap one
//! into a rolling ring buffer.
//!
//! All PipeWire code for capture lives here. Discovery is a short-lived
//! registry scan on its own thread; a capture session runs its own mainloop
//! thread with an input stream targeting the chosen node.

use crate::buffer::{CHANNELS, SAMPLE_RATE};
use crate::ring::RollingRing;
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::pod::Pod;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct CaptureNode {
    pub id: u32,
    /// object.serial — what `target.object` actually matches on modern
    /// PipeWire (registry ids do NOT work; targeting by id silently falls
    /// back to the default source).
    pub serial: u64,
    pub app: String,   // application.name or node.name fallback
    pub media: String, // media.name (song title in Spotify/Firefox!) or ""
}

fn pw_err(e: pw::Error) -> crate::error::Error {
    std::io::Error::other(e.to_string()).into()
}

/// One-shot registry scan for capture sources (mics, audio interfaces:
/// media.class == "Audio/Source").
pub fn list_input_sources() -> crate::error::Result<Vec<CaptureNode>> {
    let handle = std::thread::Builder::new()
        .name("earworm-pw-scan-in".into())
        .spawn(scan_input_sources)?;
    handle
        .join()
        .map_err(|_| std::io::Error::other("pipewire scan thread panicked"))?
        .map_err(pw_err)
}

fn scan_input_sources() -> Result<Vec<CaptureNode>, pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let found: Rc<RefCell<Vec<CaptureNode>>> = Rc::new(RefCell::new(Vec::new()));
    let _listener = registry
        .add_listener_local()
        .global({
            let found = found.clone();
            move |global| {
                let Some(props) = global.props.as_ref() else {
                    return;
                };
                if props.get("media.class") != Some("Audio/Source") {
                    return;
                }
                // For physical sources application.name is usually empty, so
                // node.description (a friendly device name) is preferred.
                let app = props
                    .get("node.description")
                    .or_else(|| props.get("application.name"))
                    .or_else(|| props.get("node.name"))
                    .unwrap_or("")
                    .to_owned();
                let media = props.get("media.name").unwrap_or("").to_owned();
                let serial = props
                    .get("object.serial")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(u64::from(global.id));
                found.borrow_mut().push(CaptureNode {
                    id: global.id,
                    serial,
                    app,
                    media,
                });
            }
        })
        .register();

    let timer = mainloop.loop_().add_timer({
        let weak = mainloop.downgrade();
        move |_| {
            if let Some(ml) = weak.upgrade() {
                ml.quit();
            }
        }
    });
    timer
        .update_timer(Some(Duration::from_millis(300)), None)
        .into_result()
        .map_err(pw::Error::SpaError)?;

    mainloop.run();
    drop(timer);
    Ok(found.take())
}

/// A live capture of one application node into a rolling ring buffer.
pub struct CaptureSession {
    pub ring: Arc<Mutex<RollingRing>>,
    pub node: CaptureNode,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// Capture an application node's output into a rolling ring.
/// `buffer_secs` default 180.0 (≈66 MB — three minutes of grab-back).
pub fn start_capture(node: CaptureNode, buffer_secs: f64) -> crate::error::Result<CaptureSession> {
    let ring = Arc::new(Mutex::new(RollingRing::with_secs(buffer_secs)));
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let ring = ring.clone();
        let stop = stop.clone();
        let node = node.clone();
        std::thread::Builder::new()
            .name("earworm-pw-cap".into())
            .spawn(move || {
                if let Err(e) = run_capture(node, ring, stop) {
                    eprintln!("earworm capture thread failed: {e}");
                }
            })?
    };
    Ok(CaptureSession {
        ring,
        node,
        stop,
        thread: Some(thread),
    })
}

impl CaptureSession {
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join(); // stop-poll timer wakes the loop within 100 ms
        }
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.shutdown(); // idempotent: thread already taken after stop()
    }
}

struct CapState {
    ring: Arc<Mutex<RollingRing>>,
    scratch: Vec<f32>,
}

fn run_capture(
    node: CaptureNode,
    ring: Arc<Mutex<RollingRing>>,
    stop: Arc<AtomicBool>,
) -> Result<(), pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;

    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::AUDIO_CHANNELS => "2",
        *pw::keys::NODE_NAME => "earworm-capture",
    };
    // Target the chosen application output stream node directly; PipeWire
    // links us to its monitor ports. target.object matches object.serial
    // (or node.name) — never the registry id.
    props.insert(*pw::keys::TARGET_OBJECT, node.serial.to_string());

    let stream = pw::stream::StreamBox::new(&core, "earworm-capture", props)?;

    let state = CapState {
        ring,
        scratch: Vec::with_capacity(8192 * CHANNELS),
    };

    let _listener = stream
        .add_local_listener_with_user_data(state)
        .process(|stream, state| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let datas = buffer.datas_mut();
            if datas.is_empty() {
                return;
            }
            let data = &mut datas[0];
            let offset = data.chunk().offset() as usize;
            let size = data.chunk().size() as usize;
            let Some(slice) = data.data() else {
                return;
            };
            let end = (offset + size).min(slice.len());
            let bytes = &slice[offset.min(end)..end];
            state.scratch.clear();
            state.scratch.extend(
                bytes
                    .chunks_exact(4)
                    .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            );
            // This runs on PipeWire's RT thread (RT_PROCESS). Never *block* on
            // the lock — a blocking acquire here risks priority inversion / an
            // xrun if the control thread is mid-snapshot. try_lock and drop this
            // buffer on the rare contention instead (the control thread only
            // holds the lock briefly at grab time, and a dropped buffer during a
            // grab is samples past the grab point anyway).
            if let Ok(mut ring) = state.ring.try_lock() {
                ring.push(&state.scratch);
            }
        })
        .register()?;

    let mut audio_info = spa::param::audio::AudioInfoRaw::new();
    audio_info.set_format(spa::param::audio::AudioFormat::F32LE);
    audio_info.set_rate(SAMPLE_RATE);
    audio_info.set_channels(CHANNELS as u32);
    let mut position = [0; spa::param::audio::MAX_CHANNELS];
    position[0] = spa::sys::SPA_AUDIO_CHANNEL_FL;
    position[1] = spa::sys::SPA_AUDIO_CHANNEL_FR;
    audio_info.set_position(position);

    let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &pw::spa::pod::Value::Object(pw::spa::pod::Object {
            type_: spa::sys::SPA_TYPE_OBJECT_Format,
            id: spa::sys::SPA_PARAM_EnumFormat,
            properties: audio_info.into(),
        }),
    )
    .unwrap()
    .0
    .into_inner();

    let mut params = [Pod::from_bytes(&values).unwrap()];

    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    // poll the stop flag; quit the loop when the session is dropped/stopped
    let timer = mainloop.loop_().add_timer({
        let weak = mainloop.downgrade();
        move |_| {
            if stop.load(Ordering::Relaxed) {
                if let Some(ml) = weak.upgrade() {
                    ml.quit();
                }
            }
        }
    });
    timer
        .update_timer(
            Some(Duration::from_millis(100)),
            Some(Duration::from_millis(100)),
        )
        .into_result()
        .map_err(pw::Error::SpaError)?;

    mainloop.run();
    drop(timer);
    Ok(())
}

/// Sample rate from a WAV file's header — cheap (no decode).
pub fn wav_header_rate(path: &std::path::Path) -> crate::error::Result<u32> {
    Ok(hound::WavReader::open(path)
        .map_err(|e| std::io::Error::other(e.to_string()))?
        .spec()
        .sample_rate)
}

/// Write interleaved stereo f32 to a 16-bit WAV at 48 kHz. Returns Ok(()).
pub fn write_wav(path: &std::path::Path, interleaved: &[f32]) -> crate::error::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let spec = hound::WavSpec {
        channels: CHANNELS as u16,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w =
        hound::WavWriter::create(path, spec).map_err(|e| std::io::Error::other(e.to_string()))?;
    for s in interleaved {
        let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        w.write_sample(v)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    w.finalize()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(())
}
