use crate::buffer::StemSet;
use crate::engine_state::EngineState;
use crate::pipeline::{ClickMark, EngineCmd, EngineEvent};
use crate::render_core::RenderShared;
use crate::stream_clock::StreamClock;
use arc_swap::ArcSwapOption;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

pub struct Engine {
    cmd_tx: rtrb::Producer<EngineCmd>,
    evt_rx: rtrb::Consumer<EngineEvent>,
    /// Lock-free slots the control thread publishes into; cloned into each
    /// output-thread spawn (initial + retarget). `playback_clock` survives
    /// retargets so the control thread keeps one handle.
    shared: RenderShared,
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
        let shared = RenderShared {
            song: Arc::new(ArcSwapOption::<StemSet>::empty()),
            clicks: Arc::new(ArcSwapOption::<Vec<ClickMark>>::empty()),
            layers: Arc::new(ArcSwapOption::<Vec<crate::layers::Layer>>::empty()),
            playback_clock: Arc::new(StreamClock::default()),
            impulse: Arc::new(crate::render_core::ImpulseSlot::default()),
        };
        let stop = Arc::new(AtomicBool::new(false));
        let audio_thread =
            crate::output::spawn(cmd_rx, evt_tx, shared.clone(), None, stop.clone())?;
        Ok(Self {
            cmd_tx,
            evt_rx,
            shared,
            stop,
            state: EngineState::default(),
            _audio_thread: Some(audio_thread),
        })
    }

    /// The playback timing publisher. Arm it briefly around a recording, then
    /// read `load()` to map graph time to the audible song frame; it does
    /// nothing on the steady playback path otherwise.
    pub fn playback_clock(&self) -> Arc<StreamClock> {
        self.shared.playback_clock.clone()
    }

    /// Output-stream latency (frames) reported by the last published snapshot;
    /// `0` until the clock has been armed and a snapshot stored.
    pub fn output_delay_frames(&self) -> i64 {
        self.shared.playback_clock.delay_frames()
    }

    /// Request a one-shot loopback calibration impulse. The output RT callback
    /// emits it on its next block and records the graph-clock emit time; read it
    /// back with `impulse_emit_ns`. Clears any stale emit time first so a `0`
    /// read means "not emitted yet".
    pub fn emit_impulse(&self) {
        self.shared.impulse.emit_ns.store(0, Ordering::Release);
        self.shared.impulse.pending.store(true, Ordering::Release);
    }

    /// Graph-clock time (ns) the last calibration impulse went out, or `0` if
    /// none has been emitted since the request (e.g. no PipeWire output stream).
    pub fn impulse_emit_ns(&self) -> i64 {
        self.shared.impulse.emit_ns.load(Ordering::Acquire)
    }

    /// Swap in a new song; audio thread picks it up at the next block.
    pub fn load(&self, set: StemSet) {
        self.shared.song.store(Some(Arc::new(set)));
    }

    /// Replace the active overdub layer set (atomic pointer swap; the audio
    /// thread picks it up on its next block).
    pub fn set_layers(&self, layers: Vec<crate::layers::Layer>) {
        self.shared.layers.store(Some(Arc::new(layers)));
    }

    /// Replace the section-click schedule; the audio thread picks it up next block.
    pub fn set_click_schedule(&self, marks: Vec<ClickMark>) {
        self.shared.clicks.store(Some(Arc::new(marks)));
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
            self.shared.clone(),
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
