//! Platform-agnostic audio render core. Both the PipeWire (Linux) and cpal
//! (non-Linux) output backends drive this: it detects song swaps, drains the
//! command ring into the pipeline, renders one interleaved-stereo block, and
//! pushes engine events out. Never allocates or locks on the steady path.

use crate::buffer::StemSet;
use crate::pipeline::{ClickMark, EngineCmd, EngineEvent, Pipeline};
use arc_swap::ArcSwapOption;
use std::sync::Arc;

pub struct RenderCore {
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
    click_slot: Arc<ArcSwapOption<Vec<ClickMark>>>,
    pipeline: Option<Pipeline>,
    current_song: Option<Arc<StemSet>>,
    current_clicks: Option<Arc<Vec<ClickMark>>>,
    events: Vec<EngineEvent>,
    /// User volume, held here (not just in the Pipeline) so it survives song
    /// swaps and a SetVolume that arrives before any song is loaded.
    volume: f32,
}

impl RenderCore {
    pub fn new(
        cmd_rx: rtrb::Consumer<EngineCmd>,
        evt_tx: rtrb::Producer<EngineEvent>,
        song_slot: Arc<ArcSwapOption<StemSet>>,
        click_slot: Arc<ArcSwapOption<Vec<ClickMark>>>,
    ) -> Self {
        Self {
            cmd_rx,
            evt_tx,
            song_slot,
            click_slot,
            pipeline: None,
            current_song: None,
            current_clicks: None,
            events: Vec::with_capacity(64),
            volume: 1.0,
        }
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

        // Drain control commands. SetVolume is latched into self.volume so it
        // persists across song swaps and survives arriving before any pipeline.
        while let Ok(cmd) = self.cmd_rx.pop() {
            if let EngineCmd::SetVolume(v) = cmd {
                self.volume = v;
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
    }
}
