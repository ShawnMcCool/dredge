//! Tuner control: owns a short-buffer input capture + a sampler thread that
//! detects pitch every ~50 ms and sends readings over a channel. `App::tick()`
//! drains them into `tuner_pitch` events. Trait + Real/Mock mirror
//! `capture_control` for testability.

use engine::capture::{CaptureNode, CaptureSession};
use engine::ring::RollingRing;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// One pitch reading pushed to clients. `confidence` is McLeod clarity (0..1);
/// `hz` is exponentially smoothed. `confidence == 0.0` means "no steady pitch".
#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct TunerReading {
    pub hz: f32,
    pub confidence: f32,
}

/// Capture buffer length: a hair more than the detection window, refreshed fast.
const RING_SECS: f64 = 0.25;
/// How much of the ring to analyze each tick.
const SNAPSHOT_SECS: f64 = 0.15;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(50);
/// Exponential smoothing weight on the newest reading.
const SMOOTH_ALPHA: f32 = 0.4;
/// Ticks to keep showing the last reading through a detection dropout before
/// blanking to "listening" (~50 ms each → ~300 ms grace).
const HOLD_TICKS: u32 = 6;

/// Everything App needs from the tuner side — real PipeWire or test mock.
pub trait TunerControl: Send {
    fn list_inputs(&mut self) -> Result<Vec<CaptureNode>, String>;
    fn start(&mut self, node_id: u32, tx: Sender<TunerReading>) -> Result<(), String>;
    fn stop(&mut self);
    fn is_running(&self) -> bool;
}

struct TunerSession {
    capture: CaptureSession,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

/// Production implementation over `engine::capture` + `engine::pitch`.
#[derive(Default)]
pub struct RealTuner {
    session: Option<TunerSession>,
}

impl TunerControl for RealTuner {
    fn list_inputs(&mut self) -> Result<Vec<CaptureNode>, String> {
        engine::capture::list_input_sources().map_err(|e| e.to_string())
    }

    fn start(&mut self, node_id: u32, tx: Sender<TunerReading>) -> Result<(), String> {
        let node = self
            .list_inputs()?
            .into_iter()
            .find(|n| n.id == node_id)
            .ok_or_else(|| format!("input device not found: {node_id}"))?;
        self.stop();
        let capture = engine::capture::start_capture(node, RING_SECS).map_err(|e| e.to_string())?;
        let ring = capture.ring.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("earworm-tuner".into())
                .spawn(move || tuner_loop(ring, tx, stop))
                .map_err(|e| e.to_string())?
        };
        self.session = Some(TunerSession {
            capture,
            stop,
            thread: Some(thread),
        });
        Ok(())
    }

    fn stop(&mut self) {
        if let Some(s) = self.session.take() {
            let TunerSession {
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

impl Drop for RealTuner {
    /// Clean up the sampler thread + capture on shutdown, mirroring
    /// `CaptureSession`'s drop discipline.
    fn drop(&mut self) {
        self.stop();
    }
}

/// Sampler loop: snapshot the ring, downmix+detect, smooth, send. Sends a
/// zero-confidence reading when no pitch is found so the UI shows "listening".
///
/// Brief detection dropouts (common as thinner/higher strings decay and their
/// clarity dips below threshold) are bridged by a release-hold: the last good
/// reading is re-sent for up to `HOLD_TICKS` ticks before the display blanks, so
/// the gauge doesn't flicker out mid-note the way a raw gate would.
fn tuner_loop(ring: Arc<Mutex<RollingRing>>, tx: Sender<TunerReading>, stop: Arc<AtomicBool>) {
    let debug = std::env::var("EARWORM_DEBUG").is_ok();
    let mut smoothed: Option<f32> = None;
    let mut last: Option<TunerReading> = None;
    let mut misses: u32 = 0;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(SAMPLE_INTERVAL);
        let snap = match ring.lock() {
            Ok(r) => r.snapshot_last(SNAPSHOT_SECS),
            Err(_) => break,
        };
        let detected = engine::pitch::detect_interleaved(&snap);
        if debug {
            let n = snap.len().max(1) as f32;
            let rms = (snap.iter().map(|s| s * s).sum::<f32>() / n).sqrt();
            match detected {
                Some(p) => eprintln!("tuner: rms={rms:.4} hz={:.1} clarity={:.2}", p.hz, p.clarity),
                None => eprintln!("tuner: rms={rms:.4} no-pitch (miss {})", misses + 1),
            }
        }
        match detected {
            Some(p) => {
                let hz = match smoothed {
                    Some(prev) => SMOOTH_ALPHA * p.hz + (1.0 - SMOOTH_ALPHA) * prev,
                    None => p.hz,
                };
                smoothed = Some(hz);
                let reading = TunerReading {
                    hz,
                    confidence: p.clarity,
                };
                last = Some(reading);
                misses = 0;
                let _ = tx.send(reading);
            }
            None => {
                misses += 1;
                if misses <= HOLD_TICKS {
                    if let Some(reading) = last {
                        let _ = tx.send(reading);
                        continue;
                    }
                }
                smoothed = None;
                last = None;
                let _ = tx.send(TunerReading {
                    hz: 0.0,
                    confidence: 0.0,
                });
            }
        }
    }
}

/// Test double: scripted input list; `start` emits one reading so `App::tick()`
/// forwarding can be observed without PipeWire.
#[derive(Default)]
pub struct MockTuner {
    pub inputs: Vec<CaptureNode>,
    pub running: bool,
}

impl TunerControl for MockTuner {
    fn list_inputs(&mut self) -> Result<Vec<CaptureNode>, String> {
        Ok(self.inputs.clone())
    }

    fn start(&mut self, node_id: u32, tx: Sender<TunerReading>) -> Result<(), String> {
        if !self.inputs.iter().any(|n| n.id == node_id) {
            return Err(format!("input device not found: {node_id}"));
        }
        let _ = tx.send(TunerReading {
            hz: 110.0,
            confidence: 0.95,
        });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn node(id: u32) -> CaptureNode {
        CaptureNode {
            id,
            serial: id as u64,
            app: format!("Device {id}"),
            media: String::new(),
        }
    }

    #[test]
    fn mock_start_emits_a_reading_and_runs() {
        let (tx, rx) = mpsc::channel();
        let mut t = MockTuner {
            inputs: vec![node(7)],
            running: false,
        };
        t.start(7, tx).unwrap();
        assert!(t.is_running());
        let r = rx.try_recv().unwrap();
        assert_eq!(r.hz, 110.0);
        assert!(r.confidence > 0.9);
    }

    #[test]
    fn mock_start_unknown_device_errors() {
        let (tx, _rx) = mpsc::channel();
        let mut t = MockTuner {
            inputs: vec![node(1)],
            running: false,
        };
        assert!(t.start(99, tx).is_err());
        assert!(!t.is_running());
    }
}
