use engine::buffer::StemSet;
use engine::pipeline::{EngineCmd, EngineEvent};

/// Everything App needs from the audio side — real Engine or test mock.
pub trait AudioControl: Send {
    fn load(&mut self, set: StemSet);
    fn send(&mut self, cmd: EngineCmd);
    fn poll_events(&mut self) -> Vec<EngineEvent>;
    fn set_output_device(&mut self, target: Option<String>);
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
    fn set_output_device(&mut self, target: Option<String>) {
        if engine::Engine::set_output_device(self, target).is_err() {
            let _ = engine::Engine::set_output_device(self, None);
        }
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
    /// Log of every `set_output_device` call — `None` means "follow default".
    pub output_device_log: Vec<Option<String>>,
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
    fn set_output_device(&mut self, target: Option<String>) {
        self.output_device_log.push(target);
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
    fn set_output_device(&mut self, target: Option<String>) {
        self.lock().unwrap().set_output_device(target);
    }
}
