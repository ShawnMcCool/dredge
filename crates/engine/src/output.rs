//! PipeWire output thread: a thin shell around `Pipeline`.
//!
//! The process callback drains the command ring, renders, and pushes events
//! out the event ring. It never allocates or locks, except constructing a
//! fresh `Pipeline` at a song-swap boundary (acceptable: song loads only).

use crate::buffer::{StemSet, CHANNELS, SAMPLE_RATE};
use crate::pipeline::{EngineCmd, EngineEvent, Pipeline};
use arc_swap::ArcSwapOption;
use pipewire as pw;
use pw::{properties::properties, spa};
use spa::pod::Pod;
use std::sync::Arc;
use std::thread::JoinHandle;

// The render fast-path casts f32 samples straight to F32LE output bytes; that is
// only correct on a little-endian host.
const _: () = assert!(cfg!(target_endian = "little"));

/// Upper bound on frames per process callback we can render into.
const MAX_QUANTUM_FRAMES: usize = 8192;

struct State {
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
    pipeline: Option<Pipeline>,
    current_song: Option<Arc<StemSet>>,
    render_buf: Vec<f32>,
    events: Vec<EngineEvent>,
    /// User volume, held here (not just in the Pipeline) so it survives song
    /// swaps and an early SetVolume that arrives before any song is loaded.
    volume: f32,
}

pub fn spawn(
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
) -> crate::error::Result<JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("earworm-pw".into())
        .spawn(move || {
            if let Err(e) = run(cmd_rx, evt_tx, song_slot) {
                eprintln!("earworm pipewire thread failed: {e}");
            }
        })?;
    Ok(handle)
}

fn run(
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
) -> Result<(), pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;

    let stream = pw::stream::StreamBox::new(
        &core,
        "earworm",
        properties! {
            *pw::keys::MEDIA_TYPE => "Audio",
            *pw::keys::MEDIA_ROLE => "Music",
            *pw::keys::MEDIA_CATEGORY => "Playback",
            *pw::keys::AUDIO_CHANNELS => "2",
            *pw::keys::NODE_NAME => "earworm",
            // playback tool, not an instrument chain — a modest quantum is right
            *pw::keys::NODE_LATENCY => "1024/48000",
        },
    )?;

    let state = State {
        cmd_rx,
        evt_tx,
        song_slot,
        pipeline: None,
        current_song: None,
        render_buf: vec![0.0; MAX_QUANTUM_FRAMES * CHANNELS],
        events: Vec::with_capacity(64),
        volume: 1.0,
    };

    let _listener = stream
        .add_local_listener_with_user_data(state)
        .process(|stream, state| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            // Song swap detection: compare the slot against the buffer the
            // current pipeline was built from.
            let song = state.song_slot.load_full();
            let swapped = match (&song, &state.current_song) {
                (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
                (Some(_), None) => true,
                (None, Some(_)) => true,
                (None, None) => false,
            };
            if swapped {
                // StemSet clone is cheap: a Vec of Arcs + gains. Seed the fresh
                // pipeline with the current user volume so song swaps don't reset
                // it back to the Pipeline default.
                state.pipeline = song.clone().map(|s| {
                    let mut p = Pipeline::new((*s).clone());
                    p.apply(EngineCmd::SetVolume(state.volume));
                    p
                });
                state.current_song = song;
            }

            // Drain control commands into the pipeline. SetVolume is also latched
            // into State so it persists across song swaps and survives arriving
            // before any pipeline exists (e.g. the saved volume sent at boot).
            while let Ok(cmd) = state.cmd_rx.pop() {
                if let EngineCmd::SetVolume(v) = cmd {
                    state.volume = v;
                }
                if let Some(p) = state.pipeline.as_mut() {
                    p.apply(cmd);
                }
            }

            let stride = std::mem::size_of::<f32>() * CHANNELS;
            // The driver tells us how many frames this cycle wants; the
            // mapped buffer itself may be much larger (maxsize).
            let requested = buffer.requested() as usize;
            let datas = buffer.datas_mut();
            let data = &mut datas[0];
            let n_frames = if let Some(slice) = data.data() {
                let mut n_frames = (slice.len() / stride).min(MAX_QUANTUM_FRAMES);
                if requested > 0 {
                    n_frames = n_frames.min(requested);
                }
                let out = &mut state.render_buf[..n_frames * CHANNELS];
                match state.pipeline.as_mut() {
                    Some(p) => {
                        state.events.clear();
                        p.render(out, &mut state.events);
                        for ev in state.events.drain(..) {
                            let _ = state.evt_tx.push(ev); // drop on full
                        }
                    }
                    None => out.fill(0.0),
                }
                // The mapped buffer is F32LE (set below) and the host is
                // little-endian (asserted at module load), so render_buf's bytes
                // are already in destination layout — one bulk memcpy instead of
                // a per-sample to_le_bytes loop (~2048 tiny copies per quantum).
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

    mainloop.run();

    Ok(())
}
