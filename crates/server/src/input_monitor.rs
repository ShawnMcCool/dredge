//! Input level monitor: opens a capture on an input device and streams peak /
//! RMS levels so the Recordings box can show whether signal is present *before*
//! a take is committed. Mirrors `tuner.rs` — a capture session plus a sampler
//! thread whose readings are drained in `App::tick` and pushed as events. It
//! deliberately does NOT run during an actual recording pass: the recorder owns
//! the device then (`recording_start` stops the monitor first).

use engine::capture::CaptureSession;
use engine::ring::RollingRing;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Capture ring length. Short — the meter only ever reads the most recent window.
const RING_SECS: f64 = 0.25;
/// Window each tick reduces to a single (peak, rms) reading.
const SNAPSHOT_SECS: f64 = 0.05;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(50);

/// One level reading: peak = max |sample|, rms over the window. Both are in
/// linear amplitude (0.0..~1.0); the frontend maps to a meter / dBFS.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct InputLevel {
    pub peak: f32,
    pub rms: f32,
}

/// Real/Mock seam, matching `TunerControl`. The frontend passes a device id from
/// `device.inputs` (or "default"); the monitor only opens a capture on it.
pub trait InputMonitorControl: Send {
    fn start(&mut self, device_id: &str, tx: Sender<InputLevel>) -> Result<(), String>;
    fn stop(&mut self);
    fn is_running(&self) -> bool;
}

struct Session {
    capture: CaptureSession,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// Production implementation over `engine::capture`.
#[derive(Default)]
pub struct RealInputMonitor {
    session: Option<Session>,
}

impl InputMonitorControl for RealInputMonitor {
    fn start(&mut self, device_id: &str, tx: Sender<InputLevel>) -> Result<(), String> {
        self.stop();
        let capture = engine::capture::start_capture_by_id(device_id, RING_SECS)
            .map_err(|e| e.to_string())?;
        let ring = capture.ring.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("dredge-input-monitor".into())
                .spawn(move || monitor_loop(ring, tx, stop))
                .map_err(|e| e.to_string())?
        };
        self.session = Some(Session {
            capture,
            stop,
            thread: Some(thread),
        });
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(s) = self.session.take() {
            let Session {
                capture,
                stop,
                mut thread,
            } = s;
            stop.store(true, Ordering::Relaxed);
            if let Some(t) = thread.take() {
                let _ = t.join();
            }
            capture.stop();
        }
    }

    fn is_running(&self) -> bool {
        self.session.is_some()
    }
}

impl Drop for RealInputMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Test seam: never opens a device, never sends.
#[derive(Default)]
pub struct MockInputMonitor {
    running: bool,
}

impl InputMonitorControl for MockInputMonitor {
    fn start(&mut self, _device_id: &str, _tx: Sender<InputLevel>) -> Result<(), String> {
        self.running = true;
        Ok(())
    }
    fn stop(&mut self) {
        self.running = false;
    }
    fn is_running(&self) -> bool {
        self.running
    }
}

/// Reduce an interleaved snapshot to a single (peak, rms) reading.
fn level_of(snap: &[f32]) -> InputLevel {
    if snap.is_empty() {
        return InputLevel {
            peak: 0.0,
            rms: 0.0,
        };
    }
    let mut peak = 0.0f32;
    let mut sq = 0.0f64;
    for &s in snap {
        peak = peak.max(s.abs());
        sq += (s as f64) * (s as f64);
    }
    let rms = (sq / snap.len() as f64).sqrt() as f32;
    InputLevel { peak, rms }
}

/// Sampler loop: snapshot the ring, reduce to a level, send. Exits when `stop`
/// is set or the receiver is gone.
fn monitor_loop(ring: Arc<Mutex<RollingRing>>, tx: Sender<InputLevel>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(SAMPLE_INTERVAL);
        let snap = match ring.lock() {
            Ok(r) => r.snapshot_last(SNAPSHOT_SECS),
            Err(_) => break,
        };
        if tx.send(level_of(&snap)).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_of_empty_is_zero() {
        let l = level_of(&[]);
        assert_eq!(l.peak, 0.0);
        assert_eq!(l.rms, 0.0);
    }

    #[test]
    fn level_of_reports_peak_and_rms() {
        // full-scale square wave: peak 1.0, rms 1.0
        let sq = [1.0f32, -1.0, 1.0, -1.0];
        let l = level_of(&sq);
        assert!((l.peak - 1.0).abs() < 1e-6);
        assert!((l.rms - 1.0).abs() < 1e-6);
        // half-amplitude: peak 0.5, rms 0.5
        let half = [0.5f32, -0.5, 0.5, -0.5];
        let l = level_of(&half);
        assert!((l.peak - 0.5).abs() < 1e-6);
        assert!((l.rms - 0.5).abs() < 1e-6);
    }

    #[test]
    fn mock_tracks_running_state() {
        let (tx, _rx) = std::sync::mpsc::channel();
        let mut m = MockInputMonitor::default();
        assert!(!m.is_running());
        m.start("default", tx).unwrap();
        assert!(m.is_running());
        m.stop();
        assert!(!m.is_running());
    }
}
