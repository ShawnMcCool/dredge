//! PipeWire output thread: a thin shell around `RenderCore`.
//!
//! The process callback dequeues a PipeWire buffer, delegates the
//! swap/drain/render work to `RenderCore::fill`, then copies the rendered
//! F32LE samples into the mapped device buffer. It never allocates or locks
//! on the steady path.

use crate::buffer::{CHANNELS, SAMPLE_RATE};
use crate::pipeline::{EngineCmd, EngineEvent};
use crate::render_core::RenderShared;
use crate::stream_clock::{ClockSnapshot, StreamClock};
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::pod::Pod;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

// The render fast-path casts f32 samples straight to F32LE output bytes; that is
// only correct on a little-endian host.
const _: () = assert!(cfg!(target_endian = "little"));

/// Upper bound on frames per process callback we can render into.
const MAX_QUANTUM_FRAMES: usize = 8192;

struct State {
    core: crate::render_core::RenderCore,
    render_buf: Vec<f32>,
    /// Publishes the audible song frame against the graph clock. A no-op on the
    /// steady path unless the control thread arms it around a recording.
    playback_clock: Arc<StreamClock>,
    /// One-shot guard for the `DREDGE_DEBUG` raw-`pw_time` dump.
    debug_printed: bool,
}

pub fn spawn(
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    shared: RenderShared,
    target: Option<String>,
    stop: Arc<AtomicBool>,
) -> crate::error::Result<JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("dredge-pw".into())
        .spawn(move || {
            if let Err(e) = run(cmd_rx, evt_tx, shared, target, stop) {
                eprintln!("dredge pipewire thread failed: {e}");
            }
        })?;
    Ok(handle)
}

fn run(
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    shared: RenderShared,
    target: Option<String>,
    stop: Arc<AtomicBool>,
) -> Result<(), pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;

    let mut props = properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_ROLE => "Music",
        *pw::keys::MEDIA_CATEGORY => "Playback",
        *pw::keys::AUDIO_CHANNELS => "2",
        *pw::keys::NODE_NAME => "dredge",
        // playback tool, not an instrument chain — a modest quantum is right
        *pw::keys::NODE_LATENCY => "1024/48000",
    };
    // Pin output to the chosen sink by object.serial. None ⇒ no insert ⇒ the
    // stream follows the default sink (the historical behaviour).
    if let Some(serial) = &target {
        props.insert(*pw::keys::TARGET_OBJECT, serial.as_str());
    }

    let stream = pw::stream::StreamBox::new(&core, "dredge", props)?;

    let playback_clock = shared.playback_clock.clone();
    let state = State {
        core: crate::render_core::RenderCore::new(cmd_rx, evt_tx, shared),
        render_buf: vec![0.0; MAX_QUANTUM_FRAMES * CHANNELS],
        playback_clock,
        debug_printed: false,
    };

    let _listener = stream
        .add_local_listener_with_user_data(state)
        .process(|stream, state| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let stride = std::mem::size_of::<f32>() * CHANNELS;
            let requested = buffer.requested() as usize;
            let datas = buffer.datas_mut();
            let data = &mut datas[0];
            let n_frames = if let Some(slice) = data.data() {
                let mut n_frames = (slice.len() / stride).min(MAX_QUANTUM_FRAMES);
                if requested > 0 {
                    n_frames = n_frames.min(requested);
                }
                let out = &mut state.render_buf[..n_frames * CHANNELS];
                state.core.fill(out);
                // F32LE device buffer + little-endian host (asserted at module
                // load): render_buf bytes are already in destination layout.
                let bytes: &[u8] = bytemuck::cast_slice(&out[..]);
                slice[..bytes.len()].copy_from_slice(bytes);
                n_frames
            } else {
                0
            };
            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = stride as _;
            *chunk.size_mut() = (stride * n_frames) as _;

            // Publish the audible song frame against the graph clock so the
            // control thread can map graph time → song frame (the playback
            // mirror of the capture clock). Only touch `stream.time()` while
            // armed, so the steady playback path pays nothing. Skip when no song
            // is loaded — there is no song frame to anchor.
            if state.playback_clock.is_armed() {
                if let Some((song_frame, song_rate_hz)) = state.core.playback_position() {
                    if let Ok(t) = stream.time() {
                        let raw = t.as_raw();
                        if !state.debug_printed && std::env::var("DREDGE_DEBUG").is_ok() {
                            eprintln!(
                                "dredge playback pw_time: now={} rate.num={} rate.denom={} ticks={} delay={}",
                                raw.now, raw.rate.num, raw.rate.denom, raw.ticks, raw.delay
                            );
                            state.debug_printed = true;
                        }
                        let snap = ClockSnapshot {
                            now_ns: t.now(),
                            ticks: song_frame,
                            rate_hz: song_rate_hz,
                        };
                        // `ring_total` is capture-specific; unused for playback.
                        state.playback_clock.store(snap, 0, raw.delay);
                    }
                }
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
        spa::utils::Direction::Output,
        None,
        pw::stream::StreamFlags::AUTOCONNECT
            | pw::stream::StreamFlags::MAP_BUFFERS
            | pw::stream::StreamFlags::RT_PROCESS,
        &mut params,
    )?;

    // poll the stop flag; quit the loop when the engine retargets/tears down
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
