# Non-blocking analysis with a live work readout

Date: 2026-06-13

## Problem

Analysis already runs on a background thread — audio playback, looping, and the
tick pump all keep working during it. But the UI *feels* locked because the
`PrepareModal` is a full-screen blocking overlay that's only dismissible on
failure, so it covers everything for the whole run (which, on the CPU-recovery
path, is ~3.6 min). There is also no live indication of what the run is doing —
which stage it's in, how long it's taken, or how hard the machine is working.

## Goals

- **Never lock the app during prepare.** Replace the blocking modal with an
  in-place, non-blocking section so the user can loop/play/edit while a run
  proceeds.
- **Show live "work" while running:** current step + stage, a running elapsed
  timer, live **CPU%** of the analysis processes, and live **GPU utilization +
  VRAM**.
- Place it in the center column **directly below the stem mixer** (a new section
  in `<main>`), per the agreed mockup.
- When idle, collapse to a one-line summary of the most recent run.

## Non-goals

- Replacing or duplicating the right-column **Profile** panel — that keeps the
  full run *history*; this new section is only the *current* run (+ an idle
  last-run line).
- Per-stage timing inside the Python/Demucs subprocesses (unchanged).
- A general system monitor — sampling is scoped to while a prepare run is active.

## Constraint note (deliberate exception)

Earlier work held profiling "internal to the binary, no external deps." For this
feature the user has **explicitly accepted one exception**: GPU metrics via a
best-effort `nvidia-smi` subprocess. Everything else stays internal (`/proc`,
`std`). No new Rust crate is added. GPU fields degrade to `null` when
`nvidia-smi` is absent.

## Design

### Backend

**Shared work-state.** `Arc<Mutex<Option<WorkState>>>` held by `App`, where:

```rust
struct WorkState { op: String, stage: String, started: std::time::Instant }
```

A lightweight `WorkReporter` (clone of the Arc) is handed to each heavy worker:
- `begin(op, stage)` sets `Some(WorkState{..})`,
- `stage(stage)` updates the stage label in place,
- `end()` sets `None`.

The **analysis** worker calls `begin("analysis", "GPU attempt")` (or
`"analyzing structure"` when the device setting is `cpu`); `analyze_with_recovery`
calls `reporter.stage("CPU recovery")` at the same point it records the
`analyze (cpu)` timer stage; the worker calls `end()` when done. The **stems**
worker uses `begin("stems", "separating stems")` / `end()`. The
`Analyzer`/`StemSeparator` traits are **not** changed — the reporter is passed
into the worker closure and `analyze_with_recovery`.

**Sampler thread.** One thread, spawned at App startup, holding a clone of the
work-state Arc and a `Sender<WorkSample>`. Loop: sleep ~750 ms; if `WorkState`
is `Some`, build and send a sample. `tick()` drains the channel and emits a
`work_sample` event (same broadcast path as `profile_run`). Sampling off the
pump means no playback-mirror jitter.

```rust
struct WorkSample {
    op: String,
    stage: String,
    elapsed_ms: u64,
    cpu_pct: u32,                  // summed across the analysis processes
    gpu_util: Option<u32>,        // percent
    gpu_mem_used_mb: Option<u32>,
    gpu_mem_total_mb: Option<u32>,
}
```

**CPU sampling (internal, `/proc`).** Scan `/proc/<pid>/cmdline` for processes
matching `analyze_impl`, `songformer_impl`, or `demucs`; for each, read
`utime+stime` (fields 14,15 of `/proc/<pid>/stat`). `cpu_pct = round(Δticks /
CLK_TCK / Δseconds * 100)` summed across matches (so a 5-core SongFormer reads
~480%). The sampler keeps the previous totals + timestamp between iterations. A
pure helper computes the percentage from (prev_ticks, cur_ticks, dt) so it's
unit-testable; the `/proc` walk and `CLK_TCK` (sysconf, default 100) are thin
wrappers.

**GPU sampling (best-effort, `nvidia-smi`).** Spawn
`nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total --format=csv,noheader,nounits`
and parse the single CSV line (e.g. `38, 5120, 16376`). A pure parse helper is
unit-testable. On any failure (binary missing, non-zero exit, unparseable) the
GPU fields are `None`; after the first failure the sampler stops re-spawning it
for the rest of the run to avoid churn.

### Frontend (`apps/desktop/src`)

- **`workSample` store** (`lib/stores.ts`): `writable<WorkSample | null>`, set on
  each `work_sample` event, cleared when `prepareState` goes `null` (run ended).
  Mirror the wire shape with a TS interface.
- **`LiveProgress.svelte`** (new), placed in `<main>` immediately after
  `<StemMixer/>`. Two sources drive it:
  - `prepareState` — the two step rows (analyzing structure / separating stems)
    with their state glyphs (running/done/cached/failed), reusing the existing
    `GLYPHS`/`terminal` logic from `PrepareModal`.
  - `workSample` — for the active step: a pulsing `● running` dot, the live
    `stage` line, the elapsed timer, and `cpu` / `gpu` mini-bars (gpu bar hidden
    when GPU fields are null). VRAM shown as `5.1 / 16 GB`.
  - **Idle:** when `prepareState` is null, collapse to a single muted line from
    the most recent `profiles` entry (`analysis · 217 s · cpu · songformer`);
    render nothing if there are no runs yet.
- **Remove `<PrepareModal />`** from `App.svelte` (delete the import + element);
  `prepare()` is unchanged otherwise — it still drives `prepareState`, which now
  feeds `LiveProgress` instead of the overlay. The `PrepareModal.svelte` file is
  deleted (its glyph/step rendering moves into `LiveProgress`).

### Data flow

`prepare()` → `analysis.run`/`stems.separate` → worker sets `WorkReporter` +
runs → sampler thread reads work-state ~1/s → `work_sample` events → `workSample`
store → `LiveProgress` renders live. Step state still flows via the existing
`prepareState` + `*_progress` events; the final `profile_run` still lands in the
Profile panel and the idle last-run line.

### Testing

- **Rust (unit):** the CPU-percent helper (prev/cur ticks + dt → percent), the
  `nvidia-smi` CSV parse helper (valid line, empty/garbage → None), and
  `WorkReporter` begin/stage/end transitions on the shared Arc.
- **Rust:** the sampler thread itself (timer + real `/proc`/`nvidia-smi`) is not
  unit-tested hermetically; coverage is the pure helpers above. A targeted app
  test may assert that setting work-state and ticking surfaces a `work_sample`
  if the sampler is made injectable, but that's optional.
- **Frontend (vitest):** `workSample` store updates on a `work_sample` event and
  clears when `prepareState` becomes null (mirrors `profiles.test.ts`).
- **Frontend:** `LiveProgress.svelte` verified via `svelte-check` + build (the
  repo does not render-test components).

## Out of scope / future

- NVML-based GPU reads (avoids the subprocess) — a later swap if `nvidia-smi`
  churn ever matters.
- Per-core CPU breakdown, GPU temperature/power, or sampling outside prepare.
- Surfacing the same live view for socket/headless clients (it rides the event
  stream already, so a client *could* consume `work_sample`).

## Open questions (resolved)

- **GPU source:** `nvidia-smi` subprocess, best-effort, no new crate. *Resolved
  — accepted external-dep exception; degrades to null when absent.*
- **CPU attribution through bash→python:** match processes by cmdline in `/proc`
  rather than threading the child PID through the analyzer trait. *Resolved —
  keeps the traits untouched.*
- **Pump jitter:** sample on a dedicated thread, not in `tick()`. *Resolved.*
- **Idle state:** collapse to a one-line last-run summary; nothing before the
  first run. *Resolved.*
