use engine::buffer::StemSet;
use engine::pipeline::{ClickMark, EngineCmd, EngineEvent};

/// Everything App needs from the audio side — real Engine or test mock.
pub trait AudioControl: Send {
    fn load(&mut self, set: StemSet);
    fn send(&mut self, cmd: EngineCmd);
    fn poll_events(&mut self) -> Vec<EngineEvent>;
    fn set_output_device(&mut self, target: Option<String>);
    fn set_click_schedule(&mut self, marks: Vec<ClickMark>);
    fn set_metronome(&mut self, cmd: EngineCmd);
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
    fn set_click_schedule(&mut self, marks: Vec<ClickMark>) {
        engine::Engine::set_click_schedule(self, marks);
    }
    fn set_metronome(&mut self, cmd: EngineCmd) {
        engine::Engine::send(self, cmd);
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
    /// The most recent `set_click_schedule` marks.
    pub click_schedule: Vec<ClickMark>,
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
    fn set_click_schedule(&mut self, marks: Vec<ClickMark>) {
        self.click_schedule = marks;
    }
    fn set_metronome(&mut self, cmd: EngineCmd) {
        self.sent.push(cmd);
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
    fn set_click_schedule(&mut self, marks: Vec<ClickMark>) {
        self.lock().unwrap().set_click_schedule(marks);
    }
    fn set_metronome(&mut self, cmd: EngineCmd) {
        self.lock().unwrap().set_metronome(cmd);
    }
}

#[cfg(test)]
mod metronome_control_tests {
    use super::*;
    use engine::metronome::{Cadence, Kit};

    #[test]
    fn mock_records_set_metronome() {
        let mut m = MockEngine::default();
        m.set_metronome(EngineCmd::SetMetronome {
            running: true,
            beat_secs: 0.5,
            beats_per_bar: 4,
            strong_mask: 0b101,
            cadence: Cadence::EveryBeat,
            kit: Kit::Click,
        });
        assert!(matches!(
            m.sent.last(),
            Some(EngineCmd::SetMetronome {
                running: true,
                beats_per_bar: 4,
                ..
            })
        ));
    }
}

#[cfg(test)]
mod click_schedule_tests {
    use super::*;
    use engine::pipeline::ClickMark;

    #[test]
    fn mock_records_last_schedule() {
        let mut m = MockEngine::default();
        m.set_click_schedule(vec![ClickMark {
            secs: 1.0,
            accent: true,
        }]);
        assert_eq!(m.click_schedule.len(), 1);
        assert!((m.click_schedule[0].secs - 1.0).abs() < 1e-9);
        assert!(m.click_schedule[0].accent);
    }
}
