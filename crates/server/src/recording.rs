//! Overdub recording orchestration. Pure helpers (span resolution, calibration
//! click detection) are unit-tested here; device capture lives behind the
//! `RecordingControl` trait so the dispatcher can be tested with a fake.

use engine::buffer::{CHANNELS, SAMPLE_RATE};
use engine::stream_clock::ClockSnapshot;

/// Which region a recording pass covers, chosen by the user at record time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Span {
    Song,
    Selection { start: f64, end: f64 },
    Loop { start: f64, end: f64 },
}

/// Resolve a span to a `[start_frame, end_frame)` source-frame window, clamped
/// to the song. Returns `None` if the window is empty.
pub fn resolve_span(span: Span, song_frames: i64) -> Option<(i64, i64)> {
    let to_frame = |s: f64| (s.max(0.0) * SAMPLE_RATE as f64).round() as i64;
    let (start, end) = match span {
        Span::Song => (0, song_frames),
        Span::Selection { start, end } | Span::Loop { start, end } => {
            (to_frame(start), to_frame(end))
        }
    };
    let start = start.clamp(0, song_frames);
    let end = end.clamp(0, song_frames);
    if end > start {
        Some((start, end))
    } else {
        None
    }
}

/// Find the first frame whose absolute sample exceeds `threshold` in an
/// interleaved stereo recording. Used by latency calibration: emit a click at
/// recording frame 0, and the detected onset is the round-trip latency.
pub fn detect_click_onset(interleaved: &[f32], threshold: f32) -> Option<usize> {
    interleaved
        .chunks_exact(CHANNELS)
        .position(|f| f.iter().any(|s| s.abs() > threshold))
}

/// Capture backend. The real implementation taps a PipeWire/cpal input; the
/// fake returns canned audio so the dispatcher is testable.
pub trait RecordingControl: Send {
    /// Begin capturing from `device_id`, sizing the buffer for `len_frames`.
    fn start(&mut self, device_id: &str, len_frames: i64) -> Result<(), String>;
    /// Stop and return the captured interleaved-stereo f32 (up to `len_frames`).
    fn stop(&mut self) -> Result<Vec<f32>, String>;
    /// Arm the capture stream clock so it begins publishing timing snapshots.
    fn arm_clock(&self);
    /// Latest capture clock snapshot paired with the ring's
    /// `total_frames_written` sampled at the same instant, or `None` if nothing
    /// has been published yet.
    fn capture_snapshot(&self) -> Option<(ClockSnapshot, i64)>;
    /// Interleaved-stereo samples for the absolute ring-frame range
    /// `[ring_start, ring_start + len)`, or `None` if `ring_start` is negative or
    /// the range has been evicted from the ring.
    fn extract_range(&self, ring_start: i64, len: i64) -> Option<Vec<f32>>;
    /// Stop the capture clock publishing.
    fn disarm_clock(&self);
    /// Capture `secs` of input from `device_id` for latency calibration (the
    /// caller emits a click out the output and analyses the result with
    /// `detect_click_onset`). Returns the captured interleaved-stereo f32.
    /// NOTE: blocks the calling thread for ~`secs` seconds — dispatch it outside
    /// any hot lock.
    fn calibrate_capture(&mut self, device_id: &str, secs: f64) -> Result<Vec<f32>, String>;
}

#[cfg(test)]
pub struct FakeRecorder {
    pub canned: Vec<f32>,
    pub started: Option<(String, i64)>,
    pub stopped: bool,
}

#[cfg(test)]
impl RecordingControl for FakeRecorder {
    fn start(&mut self, device_id: &str, len_frames: i64) -> Result<(), String> {
        self.started = Some((device_id.to_string(), len_frames));
        Ok(())
    }
    fn stop(&mut self) -> Result<Vec<f32>, String> {
        self.stopped = true;
        Ok(self.canned.clone())
    }
    fn calibrate_capture(&mut self, _device_id: &str, _secs: f64) -> Result<Vec<f32>, String> {
        Ok(self.canned.clone())
    }
    fn arm_clock(&self) {}
    fn capture_snapshot(&self) -> Option<(ClockSnapshot, i64)> {
        Some((
            ClockSnapshot {
                now_ns: 0,
                ticks: 0,
                rate_hz: 48_000,
            },
            0,
        ))
    }
    fn extract_range(&self, _ring_start: i64, _len: i64) -> Option<Vec<f32>> {
        Some(self.canned.clone())
    }
    fn disarm_clock(&self) {}
}

#[derive(Default)]
pub struct RealRecorder {
    capture: Option<engine::capture::CaptureSession>,
    len_frames: i64,
}

impl RecordingControl for RealRecorder {
    fn start(&mut self, device_id: &str, len_frames: i64) -> Result<(), String> {
        let secs = (len_frames as f64 / SAMPLE_RATE as f64) + 1.0; // +1s margin
        let cap =
            engine::capture::start_capture_by_id(device_id, secs).map_err(|e| e.to_string())?;
        self.capture = Some(cap);
        self.len_frames = len_frames;
        Ok(())
    }

    fn stop(&mut self) -> Result<Vec<f32>, String> {
        let cap = self.capture.take().ok_or("not recording")?;
        let secs = self.len_frames as f64 / SAMPLE_RATE as f64;
        let snap = cap
            .ring
            .lock()
            .map_err(|_| "capture ring poisoned")?
            .snapshot_last(secs);
        // the snapshot's MutexGuard is a temporary that drops at the end of the
        // previous statement, so the lock is released before cap.stop() joins
        // the capture thread (no deadlock).
        cap.stop();
        Ok(snap)
    }

    fn arm_clock(&self) {
        if let Some(cap) = &self.capture {
            cap.clock().arm();
        }
    }

    fn capture_snapshot(&self) -> Option<(ClockSnapshot, i64)> {
        let clock = self.capture.as_ref()?.clock();
        let snap = clock.load()?;
        Some((snap, clock.ring_total_at_snapshot()))
    }

    fn extract_range(&self, ring_start: i64, len: i64) -> Option<Vec<f32>> {
        if ring_start < 0 {
            return None;
        }
        let cap = self.capture.as_ref()?;
        let ring = cap.ring.lock().ok()?;
        ring.read_range(ring_start as u64, (ring_start + len) as u64)
    }

    fn disarm_clock(&self) {
        if let Some(cap) = &self.capture {
            cap.clock().disarm();
        }
    }

    fn calibrate_capture(&mut self, device_id: &str, secs: f64) -> Result<Vec<f32>, String> {
        let cap = engine::capture::start_capture_by_id(device_id, secs + 0.5)
            .map_err(|e| e.to_string())?;
        // Caller emits a click out the output during this window.
        std::thread::sleep(std::time::Duration::from_secs_f64(secs));
        let snap = cap
            .ring
            .lock()
            .map_err(|_| "capture ring poisoned")?
            .snapshot_last(secs);
        // the snapshot's MutexGuard is a temporary that drops at the end of the
        // previous statement, so the lock is released before cap.stop() joins
        // the capture thread (no deadlock).
        cap.stop();
        Ok(snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_song_span_is_whole_song() {
        assert_eq!(resolve_span(Span::Song, 1000), Some((0, 1000)));
    }

    #[test]
    fn resolve_selection_converts_seconds_and_clamps() {
        let s = Span::Selection {
            start: 1.0,
            end: 2.0,
        };
        assert_eq!(
            resolve_span(s, 10 * SAMPLE_RATE as i64),
            Some((SAMPLE_RATE as i64, 2 * SAMPLE_RATE as i64))
        );
    }

    #[test]
    fn resolve_empty_span_is_none() {
        let s = Span::Selection {
            start: 2.0,
            end: 2.0,
        };
        assert_eq!(resolve_span(s, 10 * SAMPLE_RATE as i64), None);
    }

    #[test]
    fn detect_onset_finds_the_click() {
        let mut buf = vec![0.0f32; 50 * CHANNELS];
        buf.extend_from_slice(&[0.9, 0.9]);
        assert_eq!(detect_click_onset(&buf, 0.5), Some(50));
    }

    #[test]
    fn detect_onset_none_when_below_threshold() {
        let buf = vec![0.1f32; 100 * CHANNELS];
        assert_eq!(detect_click_onset(&buf, 0.5), None);
    }
}
