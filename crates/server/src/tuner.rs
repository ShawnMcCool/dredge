//! Tuner control: owns a short-buffer input capture + a sampler thread that
//! detects pitch every ~50 ms and sends readings over a channel. `App::tick()`
//! drains them into `tuner_pitch` events. A `TunerControl` trait with Real/Mock
//! implementations keeps it testable.

use engine::capture::CaptureSession;
use engine::device::AudioDevice;
use engine::ring::RollingRing;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// One pitch reading pushed to clients. `hz` is median-stabilized; `confidence`
/// is 1.0 while a pitch is being tracked, 0.0 means "no steady pitch" (the UI
/// shows the listening state).
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
/// Median window over recent estimates (~50 ms each → ~250 ms). A median (not a
/// running average) is what rejects the single-frame octave jumps that make a
/// tuner readout jitter — standard practice for instrument tuners.
const MEDIAN_WINDOW: usize = 5;
/// Ticks to keep showing the last reading through a detection dropout before
/// blanking to "listening" (~50 ms each → ~300 ms grace).
const HOLD_TICKS: u32 = 6;

/// Everything App needs from the tuner side — real PipeWire or test mock.
///
/// Input *enumeration* is no longer the tuner's job: the frontend lists inputs
/// via `device.inputs` (shared with the devices tab) and passes a device id to
/// `start`. The tuner only opens a capture on that id.
pub trait TunerControl: Send {
    fn start(&mut self, device_id: &str, tx: Sender<TunerReading>) -> Result<(), String>;
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
    fn start(&mut self, device_id: &str, tx: Sender<TunerReading>) -> Result<(), String> {
        self.stop();
        let capture = engine::capture::start_capture_by_id(device_id, RING_SECS)
            .map_err(|e| e.to_string())?;
        let ring = capture.ring.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("dredge-tuner".into())
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

/// Snap `hz` to the octave nearest `reference`, correcting 2×/½× harmonic
/// errors (the usual cause of a tuner readout jumping octaves) while leaving a
/// genuinely same-octave estimate untouched.
fn fold_octave(hz: f32, reference: f32) -> f32 {
    if hz <= 0.0 || reference <= 0.0 {
        return hz;
    }
    let octaves = (hz / reference).log2().round();
    hz / 2f32.powi(octaves as i32)
}

/// Median of a small window (window is never empty when called).
fn median(window: &VecDeque<f32>) -> f32 {
    let mut v: Vec<f32> = window.iter().copied().collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v[v.len() / 2]
}

/// Sampler loop: snapshot the ring, detect, stabilize, send. Sends a
/// zero-confidence reading when no pitch is found so the UI shows "listening".
///
/// Stabilization (why a raw detector "jumps around"): each new estimate is
/// octave-folded toward the running median, then the median of the last
/// `MEDIAN_WINDOW` estimates is reported. The median discards single-frame
/// outliers (an average would fold them in), and the octave-fold catches
/// harmonic 2×/½× errors — together this is what turns a twitchy detector into a
/// steady tuner. Brief detection dropouts are bridged by a release-hold: the
/// last good reading is re-sent for up to `HOLD_TICKS` ticks before blanking.
fn tuner_loop(ring: Arc<Mutex<RollingRing>>, tx: Sender<TunerReading>, stop: Arc<AtomicBool>) {
    let debug = std::env::var("DREDGE_DEBUG").is_ok();
    let mut history: VecDeque<f32> = VecDeque::with_capacity(MEDIAN_WINDOW);
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
                Some(p) => eprintln!("tuner: rms={rms:.4} raw={:.1}Hz", p.hz),
                None => eprintln!("tuner: rms={rms:.4} no-pitch (miss {})", misses + 1),
            }
        }
        match detected {
            Some(p) => {
                let folded = if history.is_empty() {
                    p.hz
                } else {
                    fold_octave(p.hz, median(&history))
                };
                if history.len() == MEDIAN_WINDOW {
                    history.pop_front();
                }
                history.push_back(folded);
                let reading = TunerReading {
                    hz: median(&history),
                    confidence: 1.0,
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
                history.clear();
                last = None;
                let _ = tx.send(TunerReading {
                    hz: 0.0,
                    confidence: 0.0,
                });
            }
        }
    }
}

/// Test double: a scripted set of valid input ids; `start` validates the id
/// exists and emits one reading so `App::tick()` forwarding can be observed
/// without PipeWire.
#[derive(Default)]
pub struct MockTuner {
    pub inputs: Vec<AudioDevice>,
    pub running: bool,
}

impl TunerControl for MockTuner {
    fn start(&mut self, device_id: &str, tx: Sender<TunerReading>) -> Result<(), String> {
        if !self.inputs.iter().any(|d| d.id == device_id) {
            return Err(format!("input device not found: {device_id}"));
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

    fn dev(id: &str) -> AudioDevice {
        AudioDevice {
            id: id.to_owned(),
            name: format!("Device {id}"),
            is_default: false,
        }
    }

    #[test]
    fn mock_start_emits_a_reading_and_runs() {
        let (tx, rx) = mpsc::channel();
        let mut t = MockTuner {
            inputs: vec![dev("7")],
            running: false,
        };
        t.start("7", tx).unwrap();
        assert!(t.is_running());
        let r = rx.try_recv().unwrap();
        assert_eq!(r.hz, 110.0);
        assert!(r.confidence > 0.9);
    }

    #[test]
    fn mock_start_unknown_device_errors() {
        let (tx, _rx) = mpsc::channel();
        let mut t = MockTuner {
            inputs: vec![dev("1")],
            running: false,
        };
        assert!(t.start("99", tx).is_err());
        assert!(!t.is_running());
    }

    #[test]
    fn fold_octave_corrects_harmonic_errors() {
        // An octave-up harmonic error (2x) folds back to the reference octave.
        assert!((fold_octave(220.0, 110.0) - 110.0).abs() < 0.01);
        // An octave-down error (0.5x) folds up.
        assert!((fold_octave(55.0, 110.0) - 110.0).abs() < 0.01);
        // A correct same-octave estimate is left alone (slightly sharp stays).
        assert!((fold_octave(112.0, 110.0) - 112.0).abs() < 0.01);
        // Two octaves up also folds.
        assert!((fold_octave(440.0, 110.0) - 110.0).abs() < 0.01);
    }

    #[test]
    fn median_rejects_a_single_outlier() {
        // 110-ish readings with one wild octave outlier -> median ignores it.
        let w: VecDeque<f32> = [110.0, 111.0, 220.0, 109.0, 110.5].into_iter().collect();
        let m = median(&w);
        assert!((m - 110.5).abs() < 0.01, "median was {m}");
    }
}
