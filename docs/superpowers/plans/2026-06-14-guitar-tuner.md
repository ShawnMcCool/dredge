# Guitar Tuner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a chromatic guitar tuner as a powered on/off box in the stage stack — listens to a user-chosen audio input, shows the detected note + cents sharp/flat with a hold-to-lock "in tune" confirmation.

**Architecture:** A dedicated sampler thread reads a short live input-capture ring every ~50 ms, runs McLeod pitch detection + exponential smoothing off the audio thread, and sends readings over an mpsc channel that `App::tick()` drains into `tuner_pitch` events (same shape as the existing `work_sample` flow). The frontend converts Hz → note/cents (pure, unit-tested), renders one pluggable gauge (a linear meter), and drives hold-to-lock in the component. Everything expensive is shared; the gauge is the only thing a future style would duplicate.

**Tech Stack:** Rust (engine + server crates), `pitch-detection` crate (McLeod/YIN, pure Rust), PipeWire capture (existing), Svelte 5 + Tauri frontend, SQLite settings table (existing).

**Wire shape:** The `tuner_pitch` event carries `{ hz: f32, confidence: f32 }`. Note/octave/cents are derived frontend-side in `tuner-math.ts` (this refines the spec's illustrative JSON; behavior is identical). Confidence is the McLeod clarity; the frontend treats `confidence < 0.5` as "no steady pitch" (calm listening state).

**Key decisions already made:**
- Box lives in the stage, rendered **always** (even with no song open) as the last child of `<main class="stage">`, so you can tune before loading a song. (If the empty-stage look suffers, it can be moved inside the `{#if $openSong}` block — one-line change.)
- Disabled by default; a **power button** toggles capture+detection on/off.
- Input device chosen **behind a gear**, persisted by device **name** (`app` field) for stickiness across reboots; defaults to first available on first run.
- Ship **one** gauge (linear meter); gauge component takes generic props so other styles can be added later. No gauge-style setting in v1.
- A4 = 440 fixed.

**No App::new signature change:** the tuner is constructed inside `App::new()` (like `analyzer`), with a `set_tuner()` test hook (like `set_analyzer()`). No caller churn.

---

## Task 1: Pitch detection in the engine

**Files:**
- Modify: `crates/engine/Cargo.toml` (add dependency)
- Create: `crates/engine/src/pitch.rs`
- Modify: `crates/engine/src/lib.rs` (add `pub mod pitch;`)

- [ ] **Step 1: Add the dependency**

In `crates/engine/Cargo.toml`, under `[dependencies]`, add after the `hound = "3"` line:

```toml
pitch-detection = "0.3"
```

- [ ] **Step 2: Run to fetch + confirm it builds**

Run: `cargo build -p engine`
Expected: PASS (downloads `pitch-detection` + `rustfft` transitively; pure Rust, no system deps).

- [ ] **Step 3: Write the failing test**

Create `crates/engine/src/pitch.rs`:

```rust
//! Monophonic pitch detection for the tuner: McLeod pitch detection over a
//! window of mono samples, plus an interleaved-stereo entry point that downmixes
//! using the engine's capture format.

use crate::buffer::{CHANNELS, SAMPLE_RATE};
use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;

/// Detection window in samples. ~85 ms at 48 kHz — enough periods to resolve the
/// guitar low E (82 Hz) reliably while staying responsive.
pub const WINDOW: usize = 4096;

const POWER_THRESHOLD: f32 = 5.0;
const CLARITY_THRESHOLD: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchReading {
    pub hz: f32,
    pub clarity: f32,
}

/// Average channels down to mono. `channels <= 1` returns a copy.
pub fn downmix_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

/// Detect the fundamental over the last `WINDOW` mono samples. None when the
/// signal is too short, too quiet, or not pitched enough.
pub fn detect(mono: &[f32], sample_rate: u32) -> Option<PitchReading> {
    if mono.len() < WINDOW {
        return None;
    }
    let window = &mono[mono.len() - WINDOW..];
    let mut detector = McLeodDetector::new(WINDOW, WINDOW / 2);
    detector
        .get_pitch(window, sample_rate as usize, POWER_THRESHOLD, CLARITY_THRESHOLD)
        .map(|p| PitchReading {
            hz: p.frequency,
            clarity: p.clarity,
        })
}

/// Detect from an interleaved capture snapshot, downmixing with the engine's
/// channel count and detecting at the engine sample rate.
pub fn detect_interleaved(interleaved: &[f32]) -> Option<PitchReading> {
    let mono = downmix_mono(interleaved, CHANNELS);
    detect(&mono, SAMPLE_RATE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    fn sine(hz: f32, n: usize, rate: u32) -> Vec<f32> {
        (0..n)
            .map(|i| (TAU * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    #[test]
    fn detects_a440_within_a_hertz() {
        let signal = sine(440.0, WINDOW, SAMPLE_RATE);
        let r = detect(&signal, SAMPLE_RATE).expect("should detect a clean sine");
        assert!((r.hz - 440.0).abs() < 1.0, "got {}", r.hz);
        assert!(r.clarity > 0.9, "clarity {}", r.clarity);
    }

    #[test]
    fn detects_low_e_82hz() {
        let signal = sine(82.41, WINDOW, SAMPLE_RATE);
        let r = detect(&signal, SAMPLE_RATE).expect("should detect low E");
        assert!((r.hz - 82.41).abs() < 1.5, "got {}", r.hz);
    }

    #[test]
    fn silence_returns_none() {
        let signal = vec![0.0_f32; WINDOW];
        assert!(detect(&signal, SAMPLE_RATE).is_none());
    }

    #[test]
    fn downmix_averages_stereo() {
        // frames: (1.0, 3.0) -> 2.0 ; (0.0, 0.0) -> 0.0
        assert_eq!(downmix_mono(&[1.0, 3.0, 0.0, 0.0], 2), vec![2.0, 0.0]);
    }
}
```

Add `pub mod pitch;` to `crates/engine/src/lib.rs` after the `pub mod peaks;` line.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p engine pitch::tests`
Expected: PASS (4 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/engine/Cargo.toml crates/engine/Cargo.lock crates/engine/src/pitch.rs crates/engine/src/lib.rs
git commit -m "feat(engine): McLeod pitch detection for tuner"
```

---

## Task 2: Enumerate audio input sources

**Files:**
- Modify: `crates/engine/src/capture.rs` (add `list_input_sources` + `scan_input_sources`)

Today's `list_output_streams` filters `media.class == "Stream/Output/Audio"`. Mics / audio interfaces appear as `Audio/Source`. We add a sibling scanner; capture reuse comes for free (`start_capture` targets `node.serial`, which works for sources too).

- [ ] **Step 1: Add the source scanner**

In `crates/engine/src/capture.rs`, immediately after `list_output_streams` / `scan_output_streams` (after line 104), add:

```rust
/// One-shot registry scan for capture sources (mics, audio interfaces:
/// media.class == "Audio/Source"). Mirrors `list_output_streams`.
pub fn list_input_sources() -> crate::error::Result<Vec<CaptureNode>> {
    let handle = std::thread::Builder::new()
        .name("earworm-pw-scan-in".into())
        .spawn(scan_input_sources)?;
    handle
        .join()
        .map_err(|_| std::io::Error::other("pipewire scan thread panicked"))?
        .map_err(pw_err)
}

fn scan_input_sources() -> Result<Vec<CaptureNode>, pw::Error> {
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let found: Rc<RefCell<Vec<CaptureNode>>> = Rc::new(RefCell::new(Vec::new()));
    let _listener = registry
        .add_listener_local()
        .global({
            let found = found.clone();
            move |global| {
                let Some(props) = global.props.as_ref() else {
                    return;
                };
                if props.get("media.class") != Some("Audio/Source") {
                    return;
                }
                // For physical sources application.name is usually empty, so
                // node.name (a stable device name) lands in `app`.
                let app = props
                    .get("node.description")
                    .or_else(|| props.get("application.name"))
                    .or_else(|| props.get("node.name"))
                    .unwrap_or("")
                    .to_owned();
                let media = props.get("media.name").unwrap_or("").to_owned();
                let serial = props
                    .get("object.serial")
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(u64::from(global.id));
                found.borrow_mut().push(CaptureNode {
                    id: global.id,
                    serial,
                    app,
                    media,
                });
            }
        })
        .register();

    let timer = mainloop.loop_().add_timer({
        let weak = mainloop.downgrade();
        move |_| {
            if let Some(ml) = weak.upgrade() {
                ml.quit();
            }
        }
    });
    timer
        .update_timer(Some(Duration::from_millis(300)), None)
        .into_result()
        .map_err(pw::Error::SpaError)?;

    mainloop.run();
    drop(timer);
    Ok(found.take())
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -p engine`
Expected: PASS.

(No unit test: this is a live PipeWire registry scan, covered by the end-to-end check in Task 11. The logic mirrors the already-working `scan_output_streams`.)

- [ ] **Step 3: Commit**

```bash
git add crates/engine/src/capture.rs
git commit -m "feat(engine): enumerate Audio/Source capture inputs"
```

---

## Task 3: Tuner control + sampler thread (server)

**Files:**
- Create: `crates/server/src/tuner.rs`
- Modify: `crates/server/src/lib.rs` (add `pub mod tuner;`)

- [ ] **Step 1: Write the module with a failing test**

Create `crates/server/src/tuner.rs`:

```rust
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

/// Sampler loop: snapshot the ring, downmix+detect, smooth, send. Sends a
/// zero-confidence reading when no pitch is found so the UI shows "listening".
fn tuner_loop(ring: Arc<Mutex<RollingRing>>, tx: Sender<TunerReading>, stop: Arc<AtomicBool>) {
    let mut smoothed: Option<f32> = None;
    while !stop.load(Ordering::Relaxed) {
        std::thread::sleep(SAMPLE_INTERVAL);
        let snap = match ring.lock() {
            Ok(r) => r.snapshot_last(SNAPSHOT_SECS),
            Err(_) => break,
        };
        match engine::pitch::detect_interleaved(&snap) {
            Some(p) => {
                let hz = match smoothed {
                    Some(prev) => SMOOTH_ALPHA * p.hz + (1.0 - SMOOTH_ALPHA) * prev,
                    None => p.hz,
                };
                smoothed = Some(hz);
                let _ = tx.send(TunerReading {
                    hz,
                    confidence: p.clarity,
                });
            }
            None => {
                smoothed = None;
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
```

> NOTE: when `App` is dropped the `Sender` clone in the loop sees a dropped receiver; `let _ = tx.send(...)` swallows that and the loop exits on the next `stop` check (set by `RealTuner::stop`/`Drop`). No extra probe needed.

Add `pub mod tuner;` to `crates/server/src/lib.rs` after the `pub mod stems;` line.

- [ ] **Step 2: Run the tests**

Run: `cargo test -p server tuner::tests`
Expected: PASS (2 tests).

- [ ] **Step 3: Commit**

```bash
git add crates/server/src/tuner.rs crates/server/src/lib.rs
git commit -m "feat(server): tuner control + pitch sampler thread"
```

---

## Task 4: Wire the tuner into the App dispatcher

**Files:**
- Modify: `crates/server/src/app.rs` (imports, struct fields, `new`, `set_tuner`, commands, `tick`)

- [ ] **Step 1: Add imports**

At the top of `crates/server/src/app.rs`, near the existing `use crate::capture_control::CaptureControl;` (line 2), add:

```rust
use crate::tuner::{RealTuner, TunerControl, TunerReading};
```

- [ ] **Step 2: Add struct fields**

In `struct App` (after the `work_sample_rx` field, line 277), add:

```rust
    /// Live pitch readings from the tuner sampler thread; drained by `tick()`.
    tuner: Box<dyn TunerControl>,
    tuner_tx: mpsc::Sender<TunerReading>,
    tuner_rx: mpsc::Receiver<TunerReading>,
```

- [ ] **Step 3: Construct in `new()`**

In `App::new` (after `let (work_sample_tx, work_sample_rx) = mpsc::channel();`, line 302), add:

```rust
        let (tuner_tx, tuner_rx) = mpsc::channel();
```

And in the returned `Self { ... }` (after `work_sample_rx,`, line 325), add:

```rust
            tuner: Box::new(RealTuner::default()),
            tuner_tx,
            tuner_rx,
```

- [ ] **Step 4: Add the test hook**

After `set_analyzer` (line 337), add:

```rust
    /// Swap the tuner (tests use `MockTuner`).
    pub fn set_tuner(&mut self, tuner: Box<dyn TunerControl>) {
        self.tuner = tuner;
    }
```

- [ ] **Step 5: Add the commands**

In `dispatch_inner`, after the `"capture.status"`/`"capture.grab"` arms (line 420), add:

```rust
            "tuner.inputs" => serde_json::to_value(self.tuner.list_inputs()?).err_str(),
            "tuner.start" => self.tuner_start(p),
            "tuner.stop" => {
                self.tuner.stop();
                Ok(Value::Null)
            }
```

- [ ] **Step 6: Add the `tuner_start` handler**

In the `// --- capture ---` region, after `capture_start` (line 1232), add:

```rust
    fn tuner_start(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            node_id: u32,
        }
        let p: P = from_params(p)?;
        self.tuner.start(p.node_id, self.tuner_tx.clone())?;
        Ok(Value::Null)
    }
```

- [ ] **Step 7: Drain readings in `tick()`**

In `tick()`, after the `work_sample` drain loop (line 913), add:

```rust
        // live tuner readings from the sampler thread
        while let Ok(reading) = self.tuner_rx.try_recv() {
            if let Ok(data) = serde_json::to_value(reading) {
                events.push(Event {
                    event: "tuner_pitch".into(),
                    data,
                });
            }
        }
```

- [ ] **Step 8: Write the failing App test**

Find the `app.rs` test module (search for `mod tests` / existing `MockCapture` usage) and add this test. If the test module builds an `App` via a helper, reuse it; otherwise mirror an existing capture test's construction. Add:

```rust
    #[test]
    fn tuner_start_then_tick_emits_tuner_pitch() {
        let mut app = test_app(); // reuse the existing test App constructor
        app.set_tuner(Box::new(crate::tuner::MockTuner {
            inputs: vec![engine::capture::CaptureNode {
                id: 3,
                serial: 3,
                app: "Iface".into(),
                media: String::new(),
            }],
            running: false,
        }));

        app.dispatch_inner("tuner.start", serde_json::json!({ "node_id": 3 }))
            .unwrap();

        let events = app.tick();
        let pitch = events.iter().find(|e| e.event == "tuner_pitch").unwrap();
        assert_eq!(pitch.data["hz"], 110.0);
    }
```

> If there is no `test_app()` helper, search the test module for how other tests build `App::new(...)` (they pass `MockCapture`, a mock audio control, etc.) and copy that construction into a local `let mut app = ...;`. `dispatch_inner` is private but tests are in-module so it is callable.

- [ ] **Step 9: Run the test**

Run: `cargo test -p server tuner_start_then_tick_emits_tuner_pitch`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add crates/server/src/app.rs
git commit -m "feat(server): tuner.inputs/start/stop commands + tuner_pitch events"
```

---

## Task 5: Hz → note/cents math (frontend, pure + tested)

**Files:**
- Create: `apps/desktop/src/lib/tuner-math.ts`
- Create: `apps/desktop/src/lib/tuner-math.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/desktop/src/lib/tuner-math.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { hzToReading } from "./tuner-math";

describe("hzToReading", () => {
  it("maps A440 to A4, 0 cents", () => {
    expect(hzToReading(440)).toEqual({ note: "A", octave: 4, cents: 0 });
  });

  it("maps middle C (261.63) to C4 ~0 cents", () => {
    const r = hzToReading(261.63);
    expect(r.note).toBe("C");
    expect(r.octave).toBe(4);
    expect(Math.abs(r.cents)).toBeLessThanOrEqual(1);
  });

  it("maps A#4 (466.16) to A#4 ~0 cents", () => {
    const r = hzToReading(466.16);
    expect(r.note).toBe("A#");
    expect(r.octave).toBe(4);
    expect(Math.abs(r.cents)).toBeLessThanOrEqual(1);
  });

  it("maps low E (82.41) to E2", () => {
    const r = hzToReading(82.41);
    expect(r.note).toBe("E");
    expect(r.octave).toBe(2);
  });

  it("reports a sharp A as positive cents", () => {
    const r = hzToReading(448); // ~31 cents sharp of A4
    expect(r.note).toBe("A");
    expect(r.cents).toBeGreaterThan(20);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd apps/desktop && pnpm vitest run lib/tuner-math.test.ts`
Expected: FAIL ("Cannot find module './tuner-math'").

- [ ] **Step 3: Write the implementation**

Create `apps/desktop/src/lib/tuner-math.ts`:

```ts
/** Pure musical interpretation of a frequency. A4 = 440 Hz, equal temperament. */

const NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"] as const;

export interface NoteReading {
  note: string;
  octave: number;
  /** Signed cents from the nearest semitone, -50..+50. */
  cents: number;
}

export function hzToReading(hz: number): NoteReading {
  const midi = 69 + 12 * Math.log2(hz / 440);
  const rounded = Math.round(midi);
  const cents = Math.round((midi - rounded) * 100);
  const note = NOTE_NAMES[((rounded % 12) + 12) % 12];
  const octave = Math.floor(rounded / 12) - 1;
  return { note, octave, cents };
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd apps/desktop && pnpm vitest run lib/tuner-math.test.ts`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/tuner-math.ts apps/desktop/src/lib/tuner-math.test.ts
git commit -m "feat(desktop): pure Hz->note/cents tuner math"
```

---

## Task 6: Tuner stores, actions, settings, event wiring

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts`

- [ ] **Step 1: Add the wire type + stores**

After the `CaptureStatus` interface (line ~214), add:

```ts
export interface TunerReading {
  hz: number;
  /** McLeod clarity 0..1; < 0.5 means no steady pitch. */
  confidence: number;
}
```

After `export const captureStatus = writable<CaptureStatus>({ running: false });` (line 248), add:

```ts
/** Input devices (mics / interfaces) for the tuner; CaptureNode shape. */
export const tunerInputs = writable<CaptureNode[]>([]);
/** Latest pitch reading while the tuner is on; null when off. */
export const tunerReading = writable<TunerReading | null>(null);
/** Whether the tuner box is powered on (listening). */
export const tunerOn = writable(false);
/** Sticky chosen input device name; restored from settings at launch. */
export const tunerInputName = writable<string | null>(null);
```

- [ ] **Step 2: Add the settings key**

After `export const PRACTICE_TOOLS = "practice_tools_visible";` (line 288), add:

```ts
export const TUNER_INPUT_NAME = "tuner_input_name";
```

- [ ] **Step 3: Restore the sticky device in `loadSettings`**

In `loadSettings` (after the `COLOR_THEME`/`PRACTICE_TOOLS` lines, before the method ends ~line 383), add:

```ts
    if (typeof all[TUNER_INPUT_NAME] === "string") tunerInputName.set(all[TUNER_INPUT_NAME]);
```

- [ ] **Step 4: Add tuner actions**

After the capture actions block (`stopCapture` / `grabCapture`, line ~767), add:

```ts
  // --- tuner ---

  async refreshTunerInputs(): Promise<void> {
    tunerInputs.set(await cmd<CaptureNode[]>("tuner.inputs"));
  },

  /** Power on: resolve the sticky device (or first available) and start. */
  async tunerPowerOn(): Promise<void> {
    await this.refreshTunerInputs();
    const inputs = get(tunerInputs);
    if (inputs.length === 0) throw new Error("no audio input devices found");
    const savedName = get(tunerInputName);
    const node = inputs.find((n) => n.app === savedName) ?? inputs[0];
    tunerInputName.set(node.app);
    await cmd("tuner.start", { node_id: node.id });
    tunerOn.set(true);
  },

  async tunerPowerOff(): Promise<void> {
    await cmd("tuner.stop");
    tunerOn.set(false);
    tunerReading.set(null);
  },

  /** Pick a specific input; persist it and restart capture if already on. */
  async setTunerInput(node: CaptureNode): Promise<void> {
    tunerInputName.set(node.app);
    await cmd("settings.set", { key: TUNER_INPUT_NAME, value: node.app });
    if (get(tunerOn)) {
      await cmd("tuner.start", { node_id: node.id });
    }
  },
```

- [ ] **Step 5: Handle the event in `initEvents`**

In `initEvents` (after the `case "work_sample":` block, line ~996), add:

```ts
      case "tuner_pitch":
        tunerReading.set(ev.data as TunerReading);
        break;
```

- [ ] **Step 6: Verify the project type-checks**

Run: `cd apps/desktop && pnpm svelte-check`
Expected: PASS (no new errors). `get` is already imported in `stores.ts` (used by capture actions).

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src/lib/stores.ts
git commit -m "feat(desktop): tuner stores, actions, sticky device, event wiring"
```

---

## Task 7: The meter gauge component

**Files:**
- Create: `apps/desktop/src/components/MeterGauge.svelte`

Presentational only — same props regardless of future styles.

- [ ] **Step 1: Write the component**

Create `apps/desktop/src/components/MeterGauge.svelte`:

```svelte
<script lang="ts">
  interface Props {
    listening: boolean; // on but no steady pitch
    note: string;
    octave: number;
    cents: number;
    inTune: boolean;
    locked: boolean;
  }
  let { listening, note, octave, cents, inTune, locked }: Props = $props();

  // marker offset: -50..+50 cents -> 0..100%
  const pct = $derived(Math.max(0, Math.min(100, 50 + cents)));
  const word = $derived(cents > 0 ? "sharp" : cents < 0 ? "flat" : "");
</script>

<div class="gauge" class:intune={inTune} class:locked>
  {#if listening}
    <div class="hint">listening… play a note</div>
    <div class="bar"><span class="mid"></span><span class="mk idle"></span></div>
  {:else}
    <div class="head">
      <span class="note">{note}<span class="oct">{octave}</span></span>
      <span class="cents">{cents > 0 ? "+" : ""}{cents}¢ {inTune ? "✓" : word}{locked ? " · locked" : ""}</span>
    </div>
    <div class="bar">
      <span class="mid"></span>
      <span class="mk" style="left: {pct}%"></span>
    </div>
    <div class="scale"><span>♭ −50</span><span>0</span><span>+50 ♯</span></div>
  {/if}
</div>

<style>
  .gauge { display: flex; flex-direction: column; gap: 8px; }
  .hint { color: var(--muted); font-style: italic; font-size: 0.85rem; }
  .head { display: flex; align-items: baseline; gap: 12px; }
  .note { font-size: 1.9rem; font-weight: 600; line-height: 1; color: var(--text); }
  .oct { font-size: 0.9rem; color: var(--muted); }
  .cents { font-size: 0.85rem; color: var(--cyan, #4fc3d4); }
  .bar { position: relative; height: 16px; background: var(--bg-raised); border-radius: 8px; }
  .mid { position: absolute; left: 50%; top: 0; bottom: 0; width: 2px; background: var(--muted); }
  .mk { position: absolute; top: -3px; width: 8px; height: 22px; border-radius: 3px; background: var(--cyan, #4fc3d4); transform: translateX(-50%); transition: left 80ms linear; }
  .mk.idle { left: 50%; background: var(--muted); }
  .intune .cents, .intune .note { color: var(--solid, #5fd38a); }
  .intune .mk { background: var(--solid, #5fd38a); box-shadow: 0 0 8px var(--solid, #5fd38a); }
  .scale { display: flex; justify-content: space-between; font-size: 0.65rem; color: var(--muted); }
  .locked .mk { animation: pulse 0.4s ease-out; }
  @keyframes pulse { 0% { transform: translateX(-50%) scale(1.6); } 100% { transform: translateX(-50%) scale(1); } }
</style>
```

> NOTE: confirm the CSS custom property names against an existing box component (e.g. `Transport.svelte` / `StemMixer.svelte`) — this repo uses `--cyan #4fc3d4` (see global accent) and capacity colours. Adjust `--text`, `--muted`, `--bg-raised`, `--solid` to whatever the existing components use; fall back values are provided inline.

- [ ] **Step 2: Verify it type-checks**

Run: `cd apps/desktop && pnpm svelte-check`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/components/MeterGauge.svelte
git commit -m "feat(desktop): tuner meter gauge component"
```

---

## Task 8: The tuner box component

**Files:**
- Create: `apps/desktop/src/components/Tuner.svelte`

- [ ] **Step 1: Write the component**

Create `apps/desktop/src/components/Tuner.svelte`:

```svelte
<script lang="ts">
  import { actions, tunerInputs, tunerInputName, tunerOn, tunerReading } from "../lib/stores";
  import { hzToReading } from "../lib/tuner-math";
  import MeterGauge from "./MeterGauge.svelte";

  const GATE = 0.5; // confidence below this = no steady pitch
  const IN_TUNE_CENTS = 5;
  const LOCK_MS = 500;

  let gearOpen = $state(false);
  let error = $state<string | null>(null);
  let lockedSince = $state<number | null>(null);
  let locked = $state(false);

  const r = $derived($tunerReading);
  const voiced = $derived(!!r && r.confidence >= GATE && r.hz > 0);
  const reading = $derived(voiced ? hzToReading(r!.hz) : null);
  const inTune = $derived(!!reading && Math.abs(reading.cents) <= IN_TUNE_CENTS);

  // hold-to-lock: in tune continuously for LOCK_MS
  $effect(() => {
    if (!$tunerOn || !inTune) {
      lockedSince = null;
      locked = false;
      return;
    }
    if (lockedSince === null) lockedSince = performance.now();
    const elapsed = performance.now() - lockedSince;
    if (elapsed >= LOCK_MS) locked = true;
    else {
      const t = setTimeout(() => (locked = locked), LOCK_MS - elapsed); // re-eval
      return () => clearTimeout(t);
    }
  });

  async function togglePower() {
    error = null;
    try {
      if ($tunerOn) await actions.tunerPowerOff();
      else await actions.tunerPowerOn();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function openGear() {
    gearOpen = !gearOpen;
    if (gearOpen) await actions.refreshTunerInputs();
  }

  async function pick(node: (typeof $tunerInputs)[number]) {
    gearOpen = false;
    try {
      await actions.setTunerInput(node);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<section class="box tuner" class:off={!$tunerOn}>
  <header class="label">
    <button class="power" class:on={$tunerOn} onclick={togglePower} title="power" aria-label="tuner power">⏻</button>
    <span>tuner</span>
    <span class="spacer"></span>
    <button class="gear" onclick={openGear} title="input device" aria-label="choose input">⚙</button>
  </header>

  {#if gearOpen}
    <div class="picker">
      {#each $tunerInputs as n (n.id)}
        <button class="dev" class:sel={n.app === $tunerInputName} onclick={() => pick(n)}>{n.app || `device ${n.id}`}</button>
      {:else}
        <span class="hint">no input devices</span>
      {/each}
    </div>
  {/if}

  <div class="body">
    {#if error}
      <div class="err">{error}</div>
    {:else if !$tunerOn}
      <div class="hint">off — click power to listen</div>
    {:else}
      <MeterGauge
        listening={!voiced}
        note={reading?.note ?? ""}
        octave={reading?.octave ?? 0}
        cents={reading?.cents ?? 0}
        {inTune}
        {locked}
      />
    {/if}
  </div>
</section>

<style>
  .tuner.off { opacity: 0.8; }
  .label { display: flex; align-items: center; gap: 8px; }
  .spacer { flex: 1; }
  .power { border: 1.5px solid var(--muted); color: var(--muted); border-radius: 50%; width: 20px; height: 20px; line-height: 1; cursor: pointer; background: none; }
  .power.on { border-color: var(--cyan, #4fc3d4); color: var(--cyan, #4fc3d4); }
  .gear { background: none; border: none; color: var(--muted); cursor: pointer; font-size: 0.95rem; }
  .picker { display: flex; flex-direction: column; gap: 2px; padding: 6px 0; }
  .dev { text-align: left; background: var(--bg-raised); border: 1px solid transparent; color: var(--text); border-radius: 4px; padding: 4px 8px; cursor: pointer; font-size: 0.85rem; }
  .dev.sel { border-color: var(--cyan, #4fc3d4); }
  .body { padding-top: 8px; }
  .hint { color: var(--muted); font-style: italic; font-size: 0.85rem; }
  .err { color: var(--miss, #e06a5a); font-size: 0.85rem; }
</style>
```

> NOTE: match the `class="box"` / `class="label"` header markup to the other boxes (`Transport.svelte`, `StemMixer.svelte`) so styling is consistent — copy their box/label class structure if it differs from the above. The `$effect` hold-to-lock is intentionally simple; if it re-renders awkwardly, replace the timeout re-eval with an interval that the effect cleans up.

- [ ] **Step 2: Verify it type-checks**

Run: `cd apps/desktop && pnpm svelte-check`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/components/Tuner.svelte
git commit -m "feat(desktop): tuner box with power, input picker, hold-to-lock"
```

---

## Task 9: Render the box in the stage

**Files:**
- Modify: `apps/desktop/src/App.svelte`

- [ ] **Step 1: Import the component**

In `App.svelte` script imports (near line 16, by `import Transport from "./components/Transport.svelte";`), add:

```ts
  import Tuner from "./components/Tuner.svelte";
```

- [ ] **Step 2: Render it as the last child of the stage**

In the `<main class="stage">` block (lines 114-129), add `<Tuner />` after the closing `{/if}` of the `{#if $openSong}` block, so it is always present:

```svelte
  <main class="stage">
    <Waveform />
    {#if $openSong}
      <Transport />
      <div class="results">
        {#if anyResults}
          <StemMixer />
          <Analysis />
        {:else}
          <AnalyzePrompt />
        {/if}
      </div>
    {/if}
    <Tuner />
  </main>
```

- [ ] **Step 3: Verify it type-checks**

Run: `cd apps/desktop && pnpm svelte-check`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/App.svelte
git commit -m "feat(desktop): mount tuner box in the stage"
```

---

## Task 10: Full gate + end-to-end verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full check gate**

Run: `just check`
Expected: PASS — `cargo test --workspace`, `pnpm vitest run`, clippy (`-D warnings`), `cargo fmt --check`, `svelte-check` all green. Fix any clippy/fmt issues (e.g. run `cargo fmt`).

- [ ] **Step 2: Manual end-to-end**

Run: `just dev`

Verify:
- The tuner box appears in the stage, below the stems/structure area (and is visible even before a song is open).
- It starts dim/off with a ⏻ power button; clicking power turns it on and shows "listening… play a note".
- Open the gear, pick your audio interface — it persists (relaunch and confirm it's still selected, no re-pick).
- Play an open A (110 Hz) and low E (82 Hz): the note names correctly (`A2`, `E2`), the meter tracks bends, the readout stays calm during silence (no flailing), and holding a note in tune for ~½ s fires the lock pulse + "locked".
- Click power again: capture stops (the box goes idle, no further readings).

- [ ] **Step 3: Final commit (if any fmt/clippy fixups were needed)**

```bash
git add -A
git commit -m "chore: tuner lint/fmt fixups"
```

---

## Self-Review notes (already applied)

- **Spec coverage:** placement (Task 9), power model (Task 8), sticky device behind gear (Tasks 6, 8), confidence-gated states + listening (Tasks 3, 8, MeterGauge), hold-to-lock (Task 8), note+octave+cents display (Tasks 5, 7), one pluggable meter gauge (Task 7), dedicated sampler thread (Task 3), input enumeration (Task 2), pitch detection + smoothing (Tasks 1, 3), settings persistence (Task 6), A4=440 (Task 5), dependency `pitch-detection` only (Task 1) — all covered.
- **Type consistency:** `TunerReading { hz, confidence }` is identical across `tuner.rs`, the `tuner_pitch` event, and `stores.ts`. `CaptureNode` (`{ id, serial, app, media }`) is reused for inputs. `hzToReading` returns `{ note, octave, cents }`, consumed verbatim by `MeterGauge`/`Tuner`.
- **Known implementer call-outs (flagged inline):** confirm CSS custom-property names against existing boxes (`Transport.svelte`/`StemMixer.svelte`); reuse the test module's existing `App` construction for the Task 4 test; verify the exact `pitch-detection` API (`McLeodDetector::new(size, padding)` + `get_pitch(signal, sample_rate, power_threshold, clarity_threshold) -> Option<Pitch>`) against the resolved crate version.
```
