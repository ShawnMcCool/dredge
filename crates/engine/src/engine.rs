use crate::buffer::StemSet;
use crate::pipeline::{EngineCmd, EngineEvent};
use arc_swap::ArcSwapOption;
use std::sync::Arc;

pub struct Engine {
    cmd_tx: rtrb::Producer<EngineCmd>,
    evt_rx: rtrb::Consumer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
    _audio_thread: std::thread::JoinHandle<()>,
}

impl Engine {
    /// Spawns the PipeWire thread; returns the control handle.
    pub fn start() -> crate::error::Result<Self> {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::<EngineCmd>::new(256);
        let (evt_tx, evt_rx) = rtrb::RingBuffer::<EngineEvent>::new(1024);
        let song_slot = Arc::new(ArcSwapOption::<StemSet>::empty());
        let pw_thread = crate::output::spawn(cmd_rx, evt_tx, song_slot.clone())?;
        Ok(Self {
            cmd_tx,
            evt_rx,
            song_slot,
            _audio_thread: pw_thread,
        })
    }

    /// Swap in a new song; audio thread picks it up at the next block.
    pub fn load(&self, set: StemSet) {
        self.song_slot.store(Some(Arc::new(set)));
    }

    pub fn send(&mut self, cmd: EngineCmd) {
        let _ = self.cmd_tx.push(cmd); // ring full = drop oldest-style: acceptable for UI cmds
    }

    pub fn poll_events(&mut self) -> Vec<EngineEvent> {
        let mut out = Vec::new();
        while let Ok(e) = self.evt_rx.pop() {
            out.push(e);
        }
        out
    }
}
