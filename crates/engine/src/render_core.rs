//! Platform-agnostic audio render core. Both the PipeWire (Linux) and cpal
//! (non-Linux) output backends drive this: it detects song swaps, drains the
//! command ring into the pipeline, renders one interleaved-stereo block, and
//! pushes engine events out. Never allocates or locks on the steady path.

use crate::buffer::StemSet;
use crate::metronome::{Metronome, MetronomeBeat};
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
    /// Free-running metronome, mixed over the pipeline output (or silence) so it
    /// sounds with or without a song loaded.
    metronome: Metronome,
    metro_beats: Vec<MetronomeBeat>,
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
            metronome: Metronome::default(),
            metro_beats: Vec::with_capacity(16),
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
            if let EngineCmd::SetMetronome {
                running,
                beat_secs,
                beats_per_bar,
                cadence,
                kit,
            } = cmd
            {
                self.metronome
                    .configure(running, beat_secs, beats_per_bar, cadence, kit);
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
    use crate::buffer::CHANNELS;
    use crate::metronome::{Cadence, Kit};
    use crate::pipeline::EngineCmd;

    fn core() -> (RenderCore, rtrb::Producer<EngineCmd>) {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::<EngineCmd>::new(64);
        let (evt_tx, _evt_rx) = rtrb::RingBuffer::<EngineEvent>::new(256);
        let song_slot = Arc::new(ArcSwapOption::<StemSet>::empty());
        let click_slot = Arc::new(ArcSwapOption::<Vec<crate::pipeline::ClickMark>>::empty());
        (
            RenderCore::new(cmd_rx, evt_tx, song_slot, click_slot),
            cmd_tx,
        )
    }

    #[test]
    fn metronome_sounds_with_no_song_loaded() {
        let (mut rc, mut cmd_tx) = core();
        cmd_tx
            .push(EngineCmd::SetMetronome {
                running: true,
                beat_secs: 0.5,
                beats_per_bar: 4,
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
