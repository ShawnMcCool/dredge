//! Platform-agnostic audio render core. Both the PipeWire (Linux) and cpal
//! (non-Linux) output backends drive this: it detects song swaps, drains the
//! command ring into the pipeline, renders one interleaved-stereo block, and
//! pushes engine events out. Never allocates or locks on the steady path.

use crate::buffer::StemSet;
use crate::layers::Layer;
use crate::metronome::{Metronome, MetronomeBeat};
use crate::pipeline::{ClickMark, EngineCmd, EngineEvent, Pipeline};
use crate::stream_clock::StreamClock;
use arc_swap::ArcSwapOption;
use std::sync::atomic::{AtomicBool, AtomicI64};
use std::sync::Arc;

/// One-shot loopback round-trip-latency (RTL) calibration request. The control
/// thread sets `pending`; the output RT callback emits a short impulse on its
/// next block, records the graph-clock time it went out in `emit_ns`, and clears
/// `pending`. Atomics only — RT-safe, no allocation.
#[derive(Default)]
pub struct ImpulseSlot {
    pub pending: AtomicBool,
    pub emit_ns: AtomicI64,
}

/// Lock-free state the control thread publishes into and the render core reads.
/// Cheap to clone (all `Arc`s). Bundles the per-spawn shared slots so the output
/// `spawn`/`run` and `RenderCore::new` don't each carry several separate params.
#[derive(Clone)]
pub struct RenderShared {
    pub song: Arc<ArcSwapOption<StemSet>>,
    pub clicks: Arc<ArcSwapOption<Vec<ClickMark>>>,
    pub layers: Arc<ArcSwapOption<Vec<Layer>>>,
    /// Publishes the audible song frame against the graph clock (PipeWire only).
    pub playback_clock: Arc<StreamClock>,
    /// Loopback RTL calibration impulse request (PipeWire only).
    pub impulse: Arc<ImpulseSlot>,
}

pub struct RenderCore {
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
    click_slot: Arc<ArcSwapOption<Vec<ClickMark>>>,
    layer_slot: Arc<ArcSwapOption<Vec<Layer>>>,
    pipeline: Option<Pipeline>,
    current_song: Option<Arc<StemSet>>,
    current_clicks: Option<Arc<Vec<ClickMark>>>,
    current_layers: Option<Arc<Vec<Layer>>>,
    events: Vec<EngineEvent>,
    /// User volume, held here (not just in the Pipeline) so it survives song
    /// swaps and a SetVolume that arrives before any song is loaded.
    volume: f32,
    /// Free-running metronome, mixed over the pipeline output (or silence) so it
    /// sounds with or without a song loaded.
    metronome: Metronome,
    metro_beats: Vec<MetronomeBeat>,
}

impl RenderCore {
    pub fn new(
        cmd_rx: rtrb::Consumer<EngineCmd>,
        evt_tx: rtrb::Producer<EngineEvent>,
        shared: RenderShared,
    ) -> Self {
        Self {
            cmd_rx,
            evt_tx,
            song_slot: shared.song,
            click_slot: shared.clicks,
            layer_slot: shared.layers,
            pipeline: None,
            current_song: None,
            current_clicks: None,
            current_layers: None,
            events: Vec::with_capacity(64),
            volume: 1.0,
            metronome: Metronome::default(),
            metro_beats: Vec::with_capacity(16),
        }
    }

    /// Latest audible song frame + song-frame rate (frames/sec), for the
    /// playback clock. `None` when no song is loaded — nothing to anchor to, so
    /// the caller skips publishing. Read right after `fill`.
    pub fn playback_position(&self) -> Option<(i64, i64)> {
        self.pipeline
            .as_ref()
            .map(|p| (p.audible_song_frame(), p.song_rate_hz()))
    }

    /// Render `out.len() / CHANNELS` interleaved stereo frames into `out`.
    /// Call from the audio thread: does not allocate or lock on the steady
    /// path (a song swap is the one exception, building a fresh pipeline).
    pub fn fill(&mut self, out: &mut [f32]) {
        // Song swap detection: compare the slot against the buffer the current
        // pipeline was built from. `load()` gives a guard (no refcount clone)
        // for the common no-swap path; only clone the Arc out on an actual swap.
        let guard = self.song_slot.load();
        let swapped = match (guard.as_ref(), self.current_song.as_ref()) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        };
        if swapped {
            let song = (*guard).clone();
            // Seed the fresh pipeline with the current user volume so swaps
            // don't reset it to the Pipeline default.
            self.pipeline = song.clone().map(|s| {
                let mut p = Pipeline::new((*s).clone());
                p.apply(EngineCmd::SetVolume(self.volume));
                p
            });
            self.current_song = song;
        }

        // Click-schedule swap: detect by pointer like the song slot. Also
        // re-apply on a song swap, since that built a fresh pipeline.
        let cguard = self.click_slot.load();
        let cswapped = match (cguard.as_ref(), self.current_clicks.as_ref()) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        };
        if cswapped || swapped {
            let clicks = (*cguard).clone();
            if let Some(p) = self.pipeline.as_mut() {
                p.set_click_schedule(clicks.clone().unwrap_or_default());
            }
            self.current_clicks = clicks;
        }

        // Layer-set swap: detect by pointer like the click slot; re-apply on a
        // song swap too, since that built a fresh pipeline.
        let lguard = self.layer_slot.load();
        let lswapped = match (lguard.as_ref(), self.current_layers.as_ref()) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        };
        if lswapped || swapped {
            let layers = (*lguard).clone();
            if let Some(p) = self.pipeline.as_mut() {
                p.set_layers(layers.clone().unwrap_or_default());
            }
            self.current_layers = layers;
        }

        // Drain control commands. SetVolume is latched into self.volume so it
        // persists across song swaps and survives arriving before any pipeline.
        while let Ok(cmd) = self.cmd_rx.pop() {
            if let EngineCmd::SetVolume(v) = cmd {
                self.volume = v;
            }
            if let EngineCmd::SetMetronome {
                running,
                beat_secs,
                beats_per_bar,
                strong_mask,
                cadence,
                kit,
            } = cmd
            {
                self.metronome.configure(
                    running,
                    beat_secs,
                    beats_per_bar,
                    strong_mask,
                    cadence,
                    kit,
                );
                continue;
            }
            if let Some(p) = self.pipeline.as_mut() {
                p.apply(cmd);
            }
        }

        match self.pipeline.as_mut() {
            Some(p) => {
                self.events.clear();
                p.render(out, &mut self.events);
                for ev in self.events.drain(..) {
                    let _ = self.evt_tx.push(ev); // drop on full
                }
            }
            None => out.fill(0.0),
        }

        // The metronome runs regardless of song/pipeline; mix it over whatever
        // the pipeline produced (audio, or the silence fill).
        self.metro_beats.clear();
        self.metronome
            .render(out, self.volume, &mut self.metro_beats);
        for b in self.metro_beats.drain(..) {
            let _ = self.evt_tx.push(EngineEvent::MetronomeBeat {
                beat: b.beat,
                of: b.of,
                sounded: b.sounded,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::{SongBuffer, CHANNELS, SAMPLE_RATE};
    use crate::layers::Layer;
    use crate::metronome::{Cadence, Kit};
    use crate::pipeline::EngineCmd;

    fn core() -> (RenderCore, rtrb::Producer<EngineCmd>) {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::<EngineCmd>::new(64);
        let (evt_tx, _evt_rx) = rtrb::RingBuffer::<EngineEvent>::new(256);
        let shared = RenderShared {
            song: Arc::new(ArcSwapOption::<StemSet>::empty()),
            clicks: Arc::new(ArcSwapOption::<Vec<crate::pipeline::ClickMark>>::empty()),
            layers: Arc::new(ArcSwapOption::<Vec<Layer>>::empty()),
            playback_clock: Arc::new(StreamClock::default()),
            impulse: Arc::new(ImpulseSlot::default()),
        };
        (RenderCore::new(cmd_rx, evt_tx, shared), cmd_tx)
    }

    fn core_with(
        song: StemSet,
        layers: Option<Vec<Layer>>,
    ) -> (RenderCore, rtrb::Producer<EngineCmd>) {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::<EngineCmd>::new(16);
        let (evt_tx, _evt_rx) = rtrb::RingBuffer::<EngineEvent>::new(64);
        let shared = RenderShared {
            song: Arc::new(ArcSwapOption::new(Some(Arc::new(song)))),
            clicks: Arc::new(ArcSwapOption::<Vec<ClickMark>>::empty()),
            layers: Arc::new(ArcSwapOption::new(layers.map(Arc::new))),
            playback_clock: Arc::new(StreamClock::default()),
            impulse: Arc::new(ImpulseSlot::default()),
        };
        let core = RenderCore::new(cmd_rx, evt_tx, shared);
        (core, cmd_tx)
    }

    fn peak(core: &mut RenderCore) -> f32 {
        let mut max = 0.0f32;
        let mut out = vec![0.0f32; 1024 * CHANNELS];
        for _ in 0..32 {
            core.fill(&mut out);
            for s in &out {
                max = max.max(s.abs());
            }
        }
        max
    }

    #[test]
    fn a_loud_layer_becomes_audible_over_a_silent_song() {
        let silent = StemSet::single(SongBuffer {
            data: vec![0.0; SAMPLE_RATE as usize * CHANNELS],
        });
        let layer = Layer {
            samples: Arc::new(SongBuffer {
                data: vec![0.5; SAMPLE_RATE as usize * CHANNELS],
            }),
            start_frame: 0,
            gain: 1.0,
            muted: false,
        };
        let (mut core, mut tx) = core_with(silent, Some(vec![layer]));
        tx.push(EngineCmd::Play).unwrap();
        assert!(peak(&mut core) > 0.1, "layer should be audible");
    }

    #[test]
    fn silent_song_with_no_layers_stays_silent() {
        let silent = StemSet::single(SongBuffer {
            data: vec![0.0; SAMPLE_RATE as usize * CHANNELS],
        });
        let (mut core, mut tx) = core_with(silent, None);
        tx.push(EngineCmd::Play).unwrap();
        assert!(peak(&mut core) < 1e-3, "should be silent");
    }

    #[test]
    fn metronome_sounds_with_no_song_loaded() {
        let (mut rc, mut cmd_tx) = core();
        cmd_tx
            .push(EngineCmd::SetMetronome {
                running: true,
                beat_secs: 0.5,
                beats_per_bar: 4,
                strong_mask: 0b101,
                cadence: Cadence::EveryBeat,
                kit: Kit::Click,
            })
            .unwrap();
        let mut any = false;
        let mut out = vec![0.0f32; 256 * CHANNELS];
        for _ in 0..((0.6 * crate::buffer::SAMPLE_RATE as f64) as usize / 256) {
            out.iter_mut().for_each(|s| *s = 0.0);
            rc.fill(&mut out);
            if out.iter().any(|s| s.abs() > 0.01) {
                any = true;
            }
        }
        assert!(any, "metronome produced audio with no song loaded");
    }
}
