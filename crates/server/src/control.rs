use engine::buffer::StemSet;
use engine::pipeline::{ClickMark, EngineCmd, EngineEvent};
use engine::stream_clock::ClockSnapshot;

/// Everything App needs from the audio side — real Engine or test mock.
pub trait AudioControl: Send {
    fn load(&mut self, set: StemSet);
    fn send(&mut self, cmd: EngineCmd);
    fn poll_events(&mut self) -> Vec<EngineEvent>;
    fn set_output_device(&mut self, target: Option<String>);
    fn set_click_schedule(&mut self, marks: Vec<ClickMark>);
    fn set_metronome(&mut self, cmd: EngineCmd);
    /// Replace the active overdub layer set (atomic swap on the engine side).
    fn set_layers(&self, layers: Vec<engine::layers::Layer>);
    /// Arm the playback (song-frame) clock so the output RT thread starts
    /// publishing timing snapshots. Call around a recording.
    fn arm_playback_clock(&self);
    /// Latest playback song-frame clock snapshot, or `None` if nothing has been
    /// published (e.g. clock never armed, or a non-PipeWire backend).
    fn playback_clock_snapshot(&self) -> Option<ClockSnapshot>;
    /// Stop the playback clock publishing.
    fn disarm_playback_clock(&self);
    /// Output round-trip delay (frames) reported by the audio graph.
    fn output_delay_frames(&self) -> i64;
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
    fn set_layers(&self, layers: Vec<engine::layers::Layer>) {
        engine::Engine::set_layers(self, layers);
    }
    fn arm_playback_clock(&self) {
        engine::Engine::playback_clock(self).arm();
    }
    fn playback_clock_snapshot(&self) -> Option<ClockSnapshot> {
        engine::Engine::playback_clock(self).load()
    }
    fn disarm_playback_clock(&self) {
        engine::Engine::playback_clock(self).disarm();
    }
    fn output_delay_frames(&self) -> i64 {
        engine::Engine::output_delay_frames(self)
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
    /// Layer count from the most recent `set_layers`.
    pub layers_len: usize,
    /// Canned output-stream delay (frames) returned by `output_delay_frames`.
    pub output_delay: i64,
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
    fn set_layers(&self, _layers: Vec<engine::layers::Layer>) {
        // A bare MockEngine can't record through `&self`; the realistic test
        // path wraps it in `Arc<Mutex<_>>` (below), which can. Left a no-op so
        // the trait is satisfied for any direct use.
    }
    fn arm_playback_clock(&self) {}
    fn playback_clock_snapshot(&self) -> Option<ClockSnapshot> {
        Some(ClockSnapshot {
            now_ns: 0,
            ticks: 0,
            rate_hz: 48_000,
        })
    }
    fn disarm_playback_clock(&self) {}
    fn output_delay_frames(&self) -> i64 {
        self.output_delay
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
    fn set_layers(&self, layers: Vec<engine::layers::Layer>) {
        self.lock().unwrap().layers_len = layers.len();
    }
    fn arm_playback_clock(&self) {
        self.lock().unwrap().arm_playback_clock();
    }
    fn playback_clock_snapshot(&self) -> Option<ClockSnapshot> {
        self.lock().unwrap().playback_clock_snapshot()
    }
    fn disarm_playback_clock(&self) {
        self.lock().unwrap().disarm_playback_clock();
    }
    fn output_delay_frames(&self) -> i64 {
        self.lock().unwrap().output_delay_frames()
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
