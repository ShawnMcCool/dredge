use engine::buffer::StemSet;
use engine::pipeline::{EngineCmd, EngineEvent};

/// Everything App needs from the audio side — real Engine or test mock.
pub trait AudioControl: Send {
    fn load(&mut self, set: StemSet);
    fn send(&mut self, cmd: EngineCmd);
    fn poll_events(&mut self) -> Vec<EngineEvent>;
}

impl AudioControl for engine::Engine {
    fn load(&mut self, set: StemSet) {
        engine::Engine::load(self, set);
    }
    fn send(&mut self, cmd: EngineCmd) {
        engine::Engine::send(self, cmd);
    }
    fn poll_events(&mut self) -> Vec<EngineEvent> {
        engine::Engine::poll_events(self)
    }
}

/// Test double: records commands, plays back queued events.
#[derive(Default)]
pub struct MockEngine {
    pub sent: Vec<EngineCmd>,
    pub queued_events: std::collections::VecDeque<EngineEvent>,
    pub loaded_frames: Option<usize>,
    /// The full StemSet of the most recent `load` — stem-count assertions.
    pub loaded: Option<StemSet>,
}

impl AudioControl for MockEngine {
    fn load(&mut self, set: StemSet) {
        self.loaded_frames = Some(set.frames());
        self.loaded = Some(set);
    }
    fn send(&mut self, cmd: EngineCmd) {
        self.sent.push(cmd);
    }
    fn poll_events(&mut self) -> Vec<EngineEvent> {
        self.queued_events.drain(..).collect()
    }
}

/// Shared handle so tests can keep a clone while App owns the AudioControl.
impl AudioControl for std::sync::Arc<std::sync::Mutex<MockEngine>> {
    fn load(&mut self, set: StemSet) {
        self.lock().unwrap().load(set);
    }
    fn send(&mut self, cmd: EngineCmd) {
        self.lock().unwrap().send(cmd);
    }
    fn poll_events(&mut self) -> Vec<EngineEvent> {
        self.lock().unwrap().poll_events()
    }
}
