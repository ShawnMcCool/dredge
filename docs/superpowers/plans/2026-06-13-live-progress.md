# Live Work Readout (non-blocking prepare) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the blocking prepare modal with a live, non-blocking work readout (stage, elapsed, CPU%, GPU util/VRAM) in the center column below the stem mixer.

**Architecture:** Heavy workers publish their op+stage into a shared `WorkState`; a sampler thread (spawned in `socket::serve`, off the tick pump) emits ~1/s `work_sample` events with elapsed + CPU% (from `/proc`) + GPU util/VRAM (best-effort `nvidia-smi`). The Svelte frontend renders a `LiveProgress` section driven by `work_sample` + the existing `prepareState`, and the blocking `PrepareModal` is removed.

**Tech Stack:** Rust (`server` crate), `std::time`/`std::fs`/`std::process` (no new Cargo deps), Svelte 5, Vitest, svelte-check.

**Depends on:** the profiling feature (through `23e2592`): `analyze_with_recovery`, the profile channel/`tick()` emit pattern, `prepareState`, the `profiles` store.

**Scope:** One feature, backend + frontend. Backend is testable via `cargo test`; frontend store logic via Vitest, components via svelte-check + build.

---

## File Structure

- `crates/server/src/sampler.rs` — **new**: `WorkState`, `WorkReporter`, `WorkSample`, the pure helpers (`cpu_pct`, `parse_nvidia_smi`), and the sampler `run()` loop.
- `crates/server/src/lib.rs` — register `mod sampler;`.
- `crates/server/src/app.rs` — `work_state` Arc + `work_sample` channel; `tick()` emit; `sampler_handles()`; wire `WorkReporter` into the analysis + stems workers.
- `crates/server/src/analysis.rs` — `analyze_with_recovery` reports the `CPU recovery` stage.
- `crates/server/src/socket.rs` — spawn the sampler thread in `serve()`.
- `apps/desktop/src/lib/stores.ts` — `WorkSample` type, `workSample` store, `work_sample` event, clear-on-idle.
- `apps/desktop/src/lib/livesample.test.ts` — **new**: Vitest for the store.
- `apps/desktop/src/components/LiveProgress.svelte` — **new**: the readout.
- `apps/desktop/src/App.svelte` — mount `LiveProgress`, remove `PrepareModal`.
- `apps/desktop/src/components/PrepareModal.svelte` — **deleted**.

---

## Task 1: pure sampling helpers + module

**Files:**
- Create: `crates/server/src/sampler.rs`
- Modify: `crates/server/src/lib.rs`

- [ ] **Step 1: Create `crates/server/src/sampler.rs` with the helpers + tests**

```rust
//! Live work sampling for the prepare flow: a shared work-state the heavy
//! workers update, plus a thread that samples elapsed/CPU/GPU ~1/s and emits
//! `WorkSample`s. CPU is read from /proc; GPU is a best-effort `nvidia-smi`.

use serde::Serialize;

/// CPU percent over an interval: tick delta / clock-ticks-per-second / seconds.
/// Returns a process-tree-style number (can exceed 100 across cores).
pub fn cpu_pct(prev_ticks: u64, cur_ticks: u64, dt_secs: f64, clk_tck: u64) -> u32 {
    if dt_secs <= 0.0 || cur_ticks < prev_ticks || clk_tck == 0 {
        return 0;
    }
    let cpu_secs = (cur_ticks - prev_ticks) as f64 / clk_tck as f64;
    (cpu_secs / dt_secs * 100.0).round() as u32
}

/// Parse one `nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total
/// --format=csv,noheader,nounits` line, e.g. "38, 5120, 16376".
pub fn parse_nvidia_smi(line: &str) -> Option<(u32, u32, u32)> {
    let mut it = line.split(',').map(|s| s.trim().parse::<u32>());
    let util = it.next()?.ok()?;
    let used = it.next()?.ok()?;
    let total = it.next()?.ok()?;
    Some((util, used, total))
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkSample {
    pub op: String,
    pub stage: String,
    pub elapsed_ms: u64,
    pub cpu_pct: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_util: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_mem_used_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gpu_mem_total_mb: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_pct_computes_process_tree_percent() {
        // 250 ticks over 0.5 s at 100 Hz = 2.5 cpu-seconds / 0.5 s = 500%
        assert_eq!(cpu_pct(1000, 1250, 0.5, 100), 500);
        // no progress → 0
        assert_eq!(cpu_pct(1000, 1000, 0.5, 100), 0);
        // guards
        assert_eq!(cpu_pct(1000, 900, 0.5, 100), 0);
        assert_eq!(cpu_pct(0, 100, 0.0, 100), 0);
    }

    #[test]
    fn parse_nvidia_smi_reads_three_fields() {
        assert_eq!(parse_nvidia_smi("38, 5120, 16376"), Some((38, 5120, 16376)));
        assert_eq!(parse_nvidia_smi("0,0,8192"), Some((0, 0, 8192)));
        assert_eq!(parse_nvidia_smi(""), None);
        assert_eq!(parse_nvidia_smi("garbage"), None);
        assert_eq!(parse_nvidia_smi("38, 5120"), None);
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/server/src/lib.rs`, add alongside the other `mod` lines:
```rust
mod sampler;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p server --lib sampler`
Expected: PASS (both helper tests). `cargo build -p server` will warn that
`WorkSample`/helpers are unused until later tasks — that is fine; do not add
`#[allow(dead_code)]`.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/sampler.rs crates/server/src/lib.rs
git commit -m "feat(server): sampler helpers (cpu_pct, parse_nvidia_smi) + WorkSample"
```

---

## Task 2: WorkState + WorkReporter

**Files:**
- Modify: `crates/server/src/sampler.rs`

- [ ] **Step 1: Add the failing test**

Add to the `#[cfg(test)] mod tests` in `sampler.rs`:
```rust
    #[test]
    fn reporter_begin_stage_end_drive_shared_state() {
        let state = std::sync::Arc::new(std::sync::Mutex::new(None));
        let r = WorkReporter::new(state.clone());
        assert!(state.lock().unwrap().is_none());

        r.begin("analysis", "GPU attempt");
        {
            let g = state.lock().unwrap();
            let ws = g.as_ref().unwrap();
            assert_eq!(ws.op, "analysis");
            assert_eq!(ws.stage, "GPU attempt");
        }

        r.stage("CPU recovery");
        assert_eq!(state.lock().unwrap().as_ref().unwrap().stage, "CPU recovery");

        r.end();
        assert!(state.lock().unwrap().is_none());
    }
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p server --lib reporter_begin_stage_end`
Expected: FAIL — `WorkReporter`/`WorkState` not found.

- [ ] **Step 3: Add the types**

Add to `sampler.rs` (above the `#[cfg(test)]` block):
```rust
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// What a heavy run is currently doing. Not serialized — internal only.
pub struct WorkState {
    pub op: String,
    pub stage: String,
    pub started: Instant,
}

/// The shared slot the sampler reads and the workers write.
pub type SharedWork = Arc<Mutex<Option<WorkState>>>;

/// Handle a worker uses to publish its progress into the shared slot.
#[derive(Clone)]
pub struct WorkReporter {
    state: SharedWork,
}

impl WorkReporter {
    pub fn new(state: SharedWork) -> Self {
        Self { state }
    }

    pub fn begin(&self, op: &str, stage: &str) {
        *self.state.lock().unwrap() = Some(WorkState {
            op: op.into(),
            stage: stage.into(),
            started: Instant::now(),
        });
    }

    pub fn stage(&self, stage: &str) {
        if let Some(ws) = self.state.lock().unwrap().as_mut() {
            ws.stage = stage.into();
        }
    }

    pub fn end(&self) {
        *self.state.lock().unwrap() = None;
    }
}
```

- [ ] **Step 4: Run to confirm pass**

Run: `cargo test -p server --lib sampler`
Expected: PASS (3 tests now).

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/sampler.rs
git commit -m "feat(server): WorkState + WorkReporter shared work-state"
```

---

## Task 3: the sampler loop

**Files:**
- Modify: `crates/server/src/sampler.rs`

- [ ] **Step 1: Add the cmdline-match test**

Add to the tests block:
```rust
    #[test]
    fn matches_analysis_process_cmdlines() {
        assert!(is_analysis_cmd("/x/songformer-venv/bin/python /x/scripts/songformer_impl.py a.mp3"));
        assert!(is_analysis_cmd("/x/analyze-venv/bin/python /x/scripts/analyze_impl.py a.mp3"));
        assert!(is_analysis_cmd("/home/u/.local/bin/demucs -n htdemucs -o /tmp a.mp3"));
        assert!(!is_analysis_cmd("/usr/bin/firefox"));
        assert!(!is_analysis_cmd(""));
    }
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p server --lib matches_analysis_process`
Expected: FAIL — `is_analysis_cmd` not found.

- [ ] **Step 3: Implement the sampler + helpers**

Add to `sampler.rs` (above the tests block). Add `use std::sync::atomic::{AtomicBool, Ordering};` and `use std::sync::mpsc::Sender;` and `use std::time::Duration;` to the imports.

```rust
const CLK_TCK: u64 = 100; // USER_HZ on effectively all Linux
const SAMPLE_INTERVAL: Duration = Duration::from_millis(750);

/// True for the analysis/stems subprocess command lines we want to attribute
/// CPU to.
pub fn is_analysis_cmd(cmd: &str) -> bool {
    cmd.contains("songformer_impl") || cmd.contains("analyze_impl") || cmd.contains("demucs")
}

/// Sum utime+stime (clock ticks) across all processes whose cmdline matches
/// `is_analysis_cmd`. Best-effort: unreadable entries are skipped.
fn analysis_cpu_ticks() -> u64 {
    let mut total = 0u64;
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return 0;
    };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().filter(|s| s.bytes().all(|b| b.is_ascii_digit())) else {
            continue;
        };
        let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
        // cmdline is NUL-separated; join with spaces for matching
        let cmd: String = cmdline
            .split(|b| *b == 0)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        if !is_analysis_cmd(&cmd) {
            continue;
        }
        if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            // fields after the last ')': index 0 = state (field 3); utime = field
            // 14 -> index 11, stime = field 15 -> index 12.
            if let Some(rest) = stat.rsplit(')').next() {
                let f: Vec<&str> = rest.split_whitespace().collect();
                if f.len() > 12 {
                    let utime = f[11].parse::<u64>().unwrap_or(0);
                    let stime = f[12].parse::<u64>().unwrap_or(0);
                    total += utime + stime;
                }
            }
        }
    }
    total
}

/// Best-effort GPU snapshot via `nvidia-smi`. None on any failure.
fn gpu_snapshot() -> Option<(u32, u32, u32)> {
    let out = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    parse_nvidia_smi(text.lines().next()?)
}

/// Sampler loop: while a run is active, emit a `WorkSample` ~every 750 ms.
/// Exits when `shutdown` is set. Runs on its own thread (never the pump).
pub fn run(state: SharedWork, tx: Sender<WorkSample>, shutdown: Arc<AtomicBool>) {
    let mut prev_ticks = 0u64;
    let mut prev_at = Instant::now();
    let mut gpu_ok = true; // stop probing nvidia-smi after the first failure
    while !shutdown.load(Ordering::SeqCst) {
        std::thread::sleep(SAMPLE_INTERVAL);
        let (op, stage, elapsed_ms) = {
            let guard = state.lock().unwrap();
            match guard.as_ref() {
                Some(ws) => (ws.op.clone(), ws.stage.clone(), ws.started.elapsed().as_millis() as u64),
                None => {
                    prev_ticks = 0; // reset between runs
                    continue;
                }
            }
        };
        let now = Instant::now();
        let cur_ticks = analysis_cpu_ticks();
        let dt = now.duration_since(prev_at).as_secs_f64();
        let cpu = if prev_ticks == 0 { 0 } else { cpu_pct(prev_ticks, cur_ticks, dt, CLK_TCK) };
        prev_ticks = cur_ticks;
        prev_at = now;
        let gpu = if gpu_ok {
            match gpu_snapshot() {
                Some(g) => Some(g),
                None => { gpu_ok = false; None }
            }
        } else {
            None
        };
        let _ = tx.send(WorkSample {
            op,
            stage,
            elapsed_ms,
            cpu_pct: cpu,
            gpu_util: gpu.map(|g| g.0),
            gpu_mem_used_mb: gpu.map(|g| g.1),
            gpu_mem_total_mb: gpu.map(|g| g.2),
        });
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p server --lib sampler`
Expected: PASS (4 tests). `cargo build -p server` succeeds (unused-until-wired warnings are fine).

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/sampler.rs
git commit -m "feat(server): sampler loop — /proc CPU + nvidia-smi GPU sampling"
```

---

## Task 4: App wiring — channel, tick emit, sampler handles, serve spawn

**Files:**
- Modify: `crates/server/src/app.rs`, `crates/server/src/socket.rs`

- [ ] **Step 1: Add imports + fields to `app.rs`**

Add near the top `use` lines:
```rust
use crate::sampler::{SharedWork, WorkReporter, WorkSample};
```

In the `App` struct, after the `profile_tx`/`profile_rx` fields:
```rust
    /// Shared "what's running now" slot, read by the sampler thread.
    work_state: SharedWork,
    /// Live work samples from the sampler thread; drained by `tick()`.
    work_sample_tx: mpsc::Sender<WorkSample>,
    work_sample_rx: mpsc::Receiver<WorkSample>,
```

In `App::new`, where the other channels are created:
```rust
        let (work_sample_tx, work_sample_rx) = mpsc::channel();
```
and add to the `Self { .. }` literal:
```rust
            work_state: std::sync::Arc::new(std::sync::Mutex::new(None)),
            work_sample_tx,
            work_sample_rx,
```

- [ ] **Step 2: Add accessors for the sampler**

Add to `impl App` (near `set_analyzer`):
```rust
    /// Handles the sampler thread needs (work-state slot + sample sender).
    /// Cloned out once by `serve` before it spawns the sampler.
    pub fn sampler_handles(&self) -> (SharedWork, mpsc::Sender<WorkSample>) {
        (self.work_state.clone(), self.work_sample_tx.clone())
    }

    /// A reporter the heavy workers use to publish their stage.
    fn work_reporter(&self) -> WorkReporter {
        WorkReporter::new(self.work_state.clone())
    }
```

- [ ] **Step 3: Drain + emit `work_sample` in `tick()`**

In `tick()`, after the `while let Ok(mut run) = self.profile_rx.try_recv()` loop, add:
```rust
        // live work samples from the sampler thread
        while let Ok(sample) = self.work_sample_rx.try_recv() {
            if let Ok(data) = serde_json::to_value(&sample) {
                events.push(Event { event: "work_sample".into(), data });
            }
        }
```

- [ ] **Step 4: Spawn the sampler in `serve()`**

In `crates/server/src/socket.rs`, inside `serve()`, AFTER `let shutdown = Arc::new(AtomicBool::new(false));` and BEFORE the pump `let thread = { .. }`, add:
```rust
    // live work sampler — its own thread, never touches the App mutex during a tick
    {
        let (work_state, sample_tx) = app.lock().unwrap().sampler_handles();
        let shutdown = shutdown.clone();
        std::thread::spawn(move || crate::sampler::run(work_state, sample_tx, shutdown));
    }
```

- [ ] **Step 5: Build + existing tests pass**

Run: `cargo build -p server` then `cargo test -p server`
Expected: builds (unused `work_reporter` warning until Task 5 is fine); all existing tests pass.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/app.rs crates/server/src/socket.rs
git commit -m "feat(server): App work-state + work_sample channel; sampler spawned in serve"
```

---

## Task 5: wire the reporter into the analysis + stems workers

**Files:**
- Modify: `crates/server/src/app.rs`, `crates/server/src/analysis.rs`

- [ ] **Step 1: Add the failing test**

Append to `crates/server/tests/app_profiling.rs` — drive an analysis and assert the
shared work-state is populated while it runs. Since the worker clears it at the
end, capture it via a `work_sample`-independent route: assert a `work_sample`
event is NOT relied on (the sampler isn't running in tests); instead assert the
reporter set state by exposing a test accessor. Simplest: assert the live event
path by ticking — but the sampler isn't spawned in tests, so instead verify the
reporter wiring at the unit boundary. Add this app-level test that the analysis
run still completes with the reporter in place (regression guard):

```rust
#[test]
fn analysis_with_reporter_still_completes_and_profiles() {
    let mut ctx = setup(Arc::new(DeviceAwareAnalyzer));
    req(&mut ctx.app, "settings.set", json!({"key":"analysis_device","value":"cpu"}));
    req(&mut ctx.app, "analysis.run", json!({"song_id": ctx.song_id, "force": true}));
    let data = wait_for_event(&mut ctx.app, "profile_run");
    assert_eq!(data["op"], "analysis");
    assert_eq!(data["engine"], "songformer");
}
```

(The deeper stage-reporting behavior is covered by the `WorkReporter` unit test
in Task 2; this guards that adding the reporter to the worker doesn't break the
run.)

- [ ] **Step 2: Run to confirm it passes against current code, then implement**

Run: `cargo test -p server --test app_profiling analysis_with_reporter_still_completes_and_profiles`
Expected: PASS already (it's a regression guard). Now make the reporter changes and keep it green.

- [ ] **Step 3: Thread the reporter through `analyze_with_recovery`**

In `crates/server/src/analysis.rs`, change the signature and the recovery branch
to report the CPU-recovery stage:
```rust
pub fn analyze_with_recovery(
    analyzer: &dyn Analyzer,
    audio: &Path,
    device_setting: &str,
    timer: &mut crate::profile::Timer,
    reporter: &crate::sampler::WorkReporter,
) -> (Result<Analysis, String>, Option<String>) {
    if device_setting == "cpu" {
        let r = timer.stage("analyze", || analyzer.analyze(audio, true));
        return (r, Some("cpu".into()));
    }
    let r = timer.stage("analyze (gpu)", || analyzer.analyze(audio, false));
    match &r {
        Ok(a) if a.engine == "songformer" => (r, Some("gpu".into())),
        Ok(_) if songformer_venv_present() => {
            timer.note_last("songformer fell back; retrying on cpu");
            reporter.stage("CPU recovery");
            let r2 = timer.stage("analyze (cpu)", || analyzer.analyze(audio, true));
            match &r2 {
                Ok(a2) if a2.engine == "songformer" => (r2, Some("cpu".into())),
                _ => (r2, None),
            }
        }
        _ => (r, None),
    }
}
```

- [ ] **Step 4: Begin/end the reporter in the analysis worker (`app.rs`)**

In `analysis_run`, inside the spawned thread, wrap the work. Get the reporter
before spawn (`let reporter = self.work_reporter();`), move it in, and:
```rust
        let reporter = self.work_reporter();
        // ...existing clones (analyzer, tx, profile_tx, audio_path, song_id, device_setting)...
        std::thread::spawn(move || {
            let first_stage = if device_setting == "cpu" { "analyzing structure" } else { "GPU attempt" };
            reporter.begin("analysis", first_stage);
            let mut timer = crate::profile::Timer::new("analysis", Some(song_id));
            let (result, device) = crate::analysis::analyze_with_recovery(
                analyzer.as_ref(),
                &audio_path,
                &device_setting,
                &mut timer,
                &reporter,
            );
            reporter.end();
            let engine = result.as_ref().ok().map(|a| a.engine.clone());
            let err = result.as_ref().err().cloned();
            let run = timer.finish(result.is_ok(), err, device, engine);
            let _ = tx.send((song_id, result));
            let _ = profile_tx.send(run);
        });
```

- [ ] **Step 5: Begin/end the reporter in the stems worker (`app.rs`)**

In `stems_separate`, in the spawned thread, get `let reporter = self.work_reporter();`
before spawn, move it in, and wrap:
```rust
        std::thread::spawn(move || {
            reporter.begin("stems", "separating stems");
            let mut timer = crate::profile::Timer::new("stems", Some(song_id));
            let result = timer.stage("demucs", || separator.separate(&audio_path, &cache, force_cpu));
            reporter.end();
            separating.lock().unwrap().remove(&song_id.0);
            let err = result.as_ref().err().cloned();
            let run = timer.finish(result.is_ok(), err.clone(), Some(device), None);
            let data = match result {
                Ok(_) => json!({"song_id": song_id, "state": "done"}),
                Err(e) => json!({"song_id": song_id, "state": "failed", "error": e}),
            };
            let _ = tx.send(Event { event: "stems_progress".into(), data });
            let _ = profile_tx.send(run);
        });
```

- [ ] **Step 6: Run tests + clippy**

Run: `cargo test -p server` then `cargo clippy -p server --all-targets -- -D warnings`
Expected: all PASS, no warnings (the `work_reporter` method is now used).

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/app.rs crates/server/src/analysis.rs crates/server/tests/app_profiling.rs
git commit -m "feat(server): workers publish stage via WorkReporter (GPU attempt -> CPU recovery)"
```

---

## Task 6: frontend `workSample` store + event

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts`
- Create: `apps/desktop/src/lib/livesample.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/desktop/src/lib/livesample.test.ts`:
```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

const cmdMock = vi.fn();
vi.mock("./ipc", () => ({
  cmd: (...args: unknown[]) => cmdMock(...args),
  onEvent: () => () => {},
  initialSong: () => null,
}));

import { actions, workSample, type WorkSample } from "./stores";

beforeEach(() => {
  cmdMock.mockReset();
  cmdMock.mockResolvedValue([]);
  workSample.set(null);
});

const sample = (): WorkSample => ({
  op: "analysis",
  stage: "CPU recovery",
  elapsed_ms: 102000,
  cpu_pct: 483,
  gpu_util: 38,
  gpu_mem_used_mb: 5120,
  gpu_mem_total_mb: 16376,
});

describe("recordWorkSample", () => {
  it("stores the latest sample", () => {
    actions.recordWorkSample(sample());
    expect(get(workSample)?.stage).toBe("CPU recovery");
    expect(get(workSample)?.cpu_pct).toBe(483);
  });
});
```

- [ ] **Step 2: Confirm it fails**

Run: `cd apps/desktop && pnpm vitest run lib/livesample.test.ts`
Expected: FAIL — `workSample` / `actions.recordWorkSample` not exported.

- [ ] **Step 3: Add the type + store**

In `apps/desktop/src/lib/stores.ts`, near the `ProfileRun` interface, add:
```ts
export interface WorkSample {
  op: string;
  stage: string;
  elapsed_ms: number;
  cpu_pct: number;
  gpu_util?: number;
  gpu_mem_used_mb?: number;
  gpu_mem_total_mb?: number;
}
```

Near the `profiles` writable, add:
```ts
/** Latest live work sample while a prepare run is active; null when idle. */
export const workSample = writable<WorkSample | null>(null);
```

- [ ] **Step 4: Add the action + event handling**

In the `actions` object, add:
```ts
  /** Store the latest live work sample (from a `work_sample` event). */
  recordWorkSample(sample: WorkSample): void {
    workSample.set(sample);
  },
```

In the `switch (ev.event)` inside `initEvents()`, add:
```ts
      case "work_sample":
        actions.recordWorkSample(ev.data as WorkSample);
        break;
```

- [ ] **Step 5: Clear the sample when prepare ends**

Clear `workSample` whenever the run flow ends so the live readout collapses to
the idle line. There are exactly three spots (read the file to confirm line
numbers):

1. **Start of `prepare()`** — right after the opening `prepareState.set({...})`
   (around line 683), add:
   ```ts
    workSample.set(null);
   ```
2. **The success linger** inside `prepare()` (around line 736) — change
   ```ts
      setTimeout(() => prepareState.set(null), 1500);
   ```
   to
   ```ts
      setTimeout(() => { prepareState.set(null); workSample.set(null); }, 1500);
   ```
3. **`closePrepare()`** (a separate action, around line 741) — after its
   `prepareState.set(null);`, add:
   ```ts
    workSample.set(null);
   ```

- [ ] **Step 6: Confirm pass + type-check**

Run: `cd apps/desktop && pnpm vitest run lib/livesample.test.ts`
Expected: PASS.
Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: no new errors.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src/lib/stores.ts apps/desktop/src/lib/livesample.test.ts
git commit -m "feat(desktop): workSample store + work_sample event"
```

---

## Task 7: LiveProgress component, mount it, remove the modal

**Files:**
- Create: `apps/desktop/src/components/LiveProgress.svelte`
- Modify: `apps/desktop/src/App.svelte`
- Delete: `apps/desktop/src/components/PrepareModal.svelte`

- [ ] **Step 1: Create `apps/desktop/src/components/LiveProgress.svelte`**

```svelte
<script lang="ts">
  import {
    prepareState,
    workSample,
    profiles,
    type PrepareStepState,
  } from "../lib/stores";

  const STEPS = [
    { key: "analysis", label: "analyzing structure", op: "analysis" },
    { key: "stems", label: "separating stems", op: "stems" },
  ] as const;

  const GLYPHS: Record<PrepareStepState, string> = {
    pending: "·",
    running: "◌",
    done: "✓",
    cached: "✓",
    failed: "✗",
  };
  const terminal = (s: PrepareStepState) => s !== "pending" && s !== "running";

  function fmt(ms: number): string {
    if (ms < 1000) return `${ms} ms`;
    const s = ms / 1000;
    return s < 60 ? `${s.toFixed(1)} s` : `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  }

  // idle: most recent finished run as a one-liner
  let last = $derived($profiles[0]);
  let lastLine = $derived.by(() => {
    if (!last) return null;
    return [last.op, fmt(last.total_ms), last.device, last.engine].filter(Boolean).join(" · ");
  });
</script>

{#if $prepareState}
  <section class="live">
    <h3 class="mono">PREPARING</h3>
    {#each STEPS as step (step.key)}
      {@const s = $prepareState.steps[step.key]}
      {@const active = $workSample && $workSample.op === step.op && s === "running"}
      <div class="step">
        <span class="glyph mono" class:running={s === "running"} class:done={s === "done" || s === "cached"} class:failed={s === "failed"}>{GLYPHS[s]}</span>
        <span class="name">{step.label}</span>
        {#if active}
          <span class="stage mono">{$workSample.stage}</span>
          <span class="elapsed mono">{fmt($workSample.elapsed_ms)}</span>
        {:else if s === "cached"}
          <span class="muted mono">cached</span>
        {/if}
        {#if $prepareState.errors[step.key]}
          <span class="error">{$prepareState.errors[step.key]}</span>
        {/if}
      </div>
      {#if active}
        <div class="meters">
          <div class="meter">
            <span class="mlabel mono">cpu</span>
            <span class="bar"><span class="fill" style="width: {Math.min(100, $workSample.cpu_pct / 8)}%"></span></span>
            <span class="mval mono">{$workSample.cpu_pct}%</span>
          </div>
          {#if $workSample.gpu_util != null}
            <div class="meter">
              <span class="mlabel mono">gpu</span>
              <span class="bar"><span class="fill" style="width: {$workSample.gpu_util}%"></span></span>
              <span class="mval mono">{$workSample.gpu_util}%{#if $workSample.gpu_mem_total_mb} · {($workSample.gpu_mem_used_mb ?? 0) / 1024 | 0}/{($workSample.gpu_mem_total_mb / 1024) | 0} GB{/if}</span>
            </div>
          {/if}
        </div>
      {/if}
    {/each}
  </section>
{:else if lastLine}
  <section class="live idle"><span class="muted mono">last run · {lastLine}</span></section>
{/if}

<style>
  .live { padding: var(--space); border-top: 1px solid var(--bg-raised); margin-top: var(--space); }
  .live h3 { font-size: 10px; letter-spacing: 1px; color: var(--muted); margin-bottom: var(--space); }
  .step { display: flex; align-items: baseline; gap: var(--space); margin-bottom: 4px; min-width: 0; }
  .glyph { flex: 0 0 auto; width: 1.2em; text-align: center; color: var(--muted); }
  .glyph.running { color: var(--accent); animation: pulse 1s ease-in-out infinite; }
  .glyph.done { color: var(--solid); }
  .glyph.failed { color: var(--miss); }
  .name { font-size: 13px; }
  .stage { font-size: 11px; color: var(--accent); }
  .elapsed { margin-left: auto; font-size: 11px; color: var(--muted); }
  .muted { color: var(--muted); font-size: 11px; }
  .error { color: var(--miss); font-size: 11px; }
  .meters { display: flex; flex-direction: column; gap: 2px; margin: 0 0 6px 1.2em; }
  .meter { display: flex; align-items: center; gap: 6px; }
  .mlabel { font-size: 10px; color: var(--muted); width: 2em; }
  .bar { flex: 1; height: 4px; background: var(--bg-raised); border-radius: 2px; overflow: hidden; max-width: 220px; }
  .fill { display: block; height: 100%; background: var(--accent); }
  .mval { font-size: 10px; color: var(--muted); width: 9em; }
  .idle { color: var(--muted); }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  @media (prefers-reduced-motion: reduce) { .glyph.running { animation: none; } }
</style>
```

- [ ] **Step 2: Mount it in `App.svelte`, remove the modal**

In `App.svelte` `<script>`: add the import and remove the `PrepareModal` import:
```ts
  import LiveProgress from "./components/LiveProgress.svelte";
```
(delete the line `import PrepareModal from "./components/PrepareModal.svelte";`)

In `<main class="stage">`, add `<LiveProgress />` immediately after `<StemMixer />`:
```svelte
    <StemMixer />
    <LiveProgress />
```

Remove the `<PrepareModal />` element from the markup (it sits with `<ExitModal />`/`<SettingsModal />` near the end).

- [ ] **Step 3: Delete the modal component**

```bash
git rm apps/desktop/src/components/PrepareModal.svelte
```

- [ ] **Step 4: Type-check + build + full frontend gate**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 errors (no dangling `PrepareModal` references).
Run: `cd apps/desktop && pnpm build`
Expected: clean.
Run: `cd apps/desktop && pnpm vitest run`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/components/LiveProgress.svelte apps/desktop/src/App.svelte
git commit -m "feat(desktop): live non-blocking prepare readout below the stem mixer; remove blocking modal"
```

---

## Manual verification (after Task 7)

- [ ] `just build` then `just run`.
- [ ] Free some VRAM, then RE-PREPARE: the new section appears below the stem mixer with a pulsing dot, live stage (`GPU attempt` → if it falls back, `CPU recovery`), a climbing elapsed timer, and `cpu` / `gpu` bars updating ~1/s. The app stays fully usable (play/loop while it runs).
- [ ] On completion the section collapses to `last run · analysis · … · songformer`.
- [ ] No blocking overlay appears at any point.

---

## Self-review checklist (done while writing)

- **Spec coverage:** non-blocking (modal removed, T7) · live stage+elapsed (T5 reporter + T3 sampler + T7 view) · CPU% from /proc (T3) · GPU via nvidia-smi best-effort (T3) · placement below stem mixer (T7) · idle last-run line (T7) · sampler off the pump in serve with shutdown (T4) · traits unchanged (reporter passed in, T5). All covered.
- **Placeholder scan:** none — full code each step. Two read-first spots flagged: the three `workSample.set(null)` insertions in `prepare()` (T6-S5) and the `<PrepareModal/>` element location (T7-S2).
- **Type consistency:** `WorkState`/`WorkReporter`/`SharedWork`/`WorkSample` (Rust) and `WorkSample`/`workSample`/`recordWorkSample` (TS) consistent across tasks; `analyze_with_recovery` new `reporter` param matches the call site in T5-S4; `sampler_handles()`/`work_reporter()` used in T4/T5.
