use crate::buffer::StemSet;
use crate::engine_state::EngineState;
use crate::pipeline::{ClickMark, EngineCmd, EngineEvent};
use crate::stream_clock::StreamClock;
use arc_swap::ArcSwapOption;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

pub struct Engine {
    cmd_tx: rtrb::Producer<EngineCmd>,
    evt_rx: rtrb::Consumer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
    click_slot: Arc<ArcSwapOption<Vec<ClickMark>>>,
    layer_slot: Arc<ArcSwapOption<Vec<crate::layers::Layer>>>,
    /// Publishes the audible song frame + output latency against the graph
    /// clock. Survives output retargets so the control thread keeps one handle.
    playback_clock: Arc<StreamClock>,
    /// Signals the current output thread to quit; replaced on each retarget.
    stop: Arc<AtomicBool>,
    /// Live snapshot of engine state, replayed onto a fresh pipeline when the
    /// output thread is respawned on a different device.
    state: EngineState,
    _audio_thread: Option<JoinHandle<()>>,
}

impl Engine {
    /// Spawns the audio output thread; returns the control handle.
    pub fn start() -> crate::error::Result<Self> {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::<EngineCmd>::new(256);
        let (evt_tx, evt_rx) = rtrb::RingBuffer::<EngineEvent>::new(1024);
        let song_slot = Arc::new(ArcSwapOption::<StemSet>::empty());
        let click_slot = Arc::new(ArcSwapOption::<Vec<ClickMark>>::empty());
        let layer_slot = Arc::new(ArcSwapOption::<Vec<crate::layers::Layer>>::empty());
        let playback_clock = Arc::new(StreamClock::default());
        let stop = Arc::new(AtomicBool::new(false));
        let audio_thread = crate::output::spawn(
            cmd_rx,
            evt_tx,
            song_slot.clone(),
            click_slot.clone(),
            layer_slot.clone(),
            playback_clock.clone(),
            None,
            stop.clone(),
        )?;
        Ok(Self {
            cmd_tx,
            evt_rx,
            song_slot,
            click_slot,
            layer_slot,
            playback_clock,
            stop,
            state: EngineState::default(),
            _audio_thread: Some(audio_thread),
        })
    }

    /// The playback timing publisher. Arm it briefly around a recording, then
    /// read `load()` to map graph time to the audible song frame; it does
    /// nothing on the steady playback path otherwise.
    pub fn playback_clock(&self) -> Arc<StreamClock> {
        self.playback_clock.clone()
    }

    /// Output-stream latency (frames) reported by the last published snapshot;
    /// `0` until the clock has been armed and a snapshot stored.
    pub fn output_delay_frames(&self) -> i64 {
        self.playback_clock.delay_frames()
    }

    /// Swap in a new song; audio thread picks it up at the next block.
    pub fn load(&self, set: StemSet) {
        self.song_slot.store(Some(Arc::new(set)));
    }

    /// Replace the active overdub layer set (atomic pointer swap; the audio
    /// thread picks it up on its next block).
    pub fn set_layers(&self, layers: Vec<crate::layers::Layer>) {
        self.layer_slot.store(Some(Arc::new(layers)));
    }

    /// Replace the section-click schedule; the audio thread picks it up next block.
    pub fn set_click_schedule(&self, marks: Vec<ClickMark>) {
        self.click_slot.store(Some(Arc::new(marks)));
    }

    pub fn send(&mut self, cmd: EngineCmd) {
        self.state.observe(&cmd);
        let _ = self.cmd_tx.push(cmd); // ring full = drop oldest-style: acceptable for UI cmds
    }

    pub fn poll_events(&mut self) -> Vec<EngineEvent> {
        let mut out = Vec::new();
        while let Ok(e) = self.evt_rx.pop() {
            if let EngineEvent::Position { secs, playing, .. } = e {
                self.state.set_position(secs, playing);
            }
            out.push(e);
        }
        out
    }

    /// Tear down the output thread and respawn it on `target` (None ⇒ default
    /// sink), restoring playback by replaying the live state snapshot onto the
    /// fresh pipeline. The song itself is held in `song_slot` and reloaded by
    /// the new `RenderCore` via swap detection on first fill.
    pub fn set_output_device(&mut self, target: Option<String>) -> crate::error::Result<()> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self._audio_thread.take() {
            let _ = h.join();
        }
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::<EngineCmd>::new(256);
        let (evt_tx, evt_rx) = rtrb::RingBuffer::<EngineEvent>::new(1024);
        self.stop = Arc::new(AtomicBool::new(false));
        let h = crate::output::spawn(
            cmd_rx,
            evt_tx,
            self.song_slot.clone(),
            self.click_slot.clone(),
            self.layer_slot.clone(),
            self.playback_clock.clone(),
            target,
            self.stop.clone(),
        )?;
        self.cmd_tx = cmd_tx;
        self.evt_rx = evt_rx;
        self._audio_thread = Some(h);
        for cmd in self.state.replay_cmds() {
            let _ = self.cmd_tx.push(cmd);
        }
        Ok(())
    }
}
