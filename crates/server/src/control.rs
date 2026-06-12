use engine::buffer::SongBuffer;
use engine::pipeline::{EngineCmd, EngineEvent};

/// Everything App needs from the audio side — real Engine or test mock.
pub trait AudioControl: Send {
    fn load(&mut self, buf: SongBuffer);
    fn send(&mut self, cmd: EngineCmd);
    fn poll_events(&mut self) -> Vec<EngineEvent>;
}

impl AudioControl for engine::Engine {
    fn load(&mut self, buf: SongBuffer) {
        engine::Engine::load(self, buf);
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
}

impl AudioControl for MockEngine {
    fn load(&mut self, buf: SongBuffer) {
        self.loaded_frames = Some(buf.frames());
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
    fn load(&mut self, buf: SongBuffer) {
        self.lock().unwrap().load(buf);
    }
    fn send(&mut self, cmd: EngineCmd) {
        self.lock().unwrap().send(cmd);
    }
    fn poll_events(&mut self) -> Vec<EngineEvent> {
        self.lock().unwrap().poll_events()
    }
}
