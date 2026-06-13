# Operation profiling + analysis device control

Date: 2026-06-13

## Problem

Earworm's heavy operations — song decode/import, structural analysis (beat_this
+ SongFormer/novelty), and Demucs stem separation — run for seconds to minutes
and lean on out-of-process Python and the GPU. Today the binary records **no
timing whatsoever** (no `Instant`, no `tracing` — only `eprintln`), and surfaces
**nothing** about what an operation cost or how it ran.

This produced a concrete, repeated confusion: a song analyzed at import time
with the inferior novelty detector (because the SongFormer venv didn't yet
exist), and later a SongFormer run that **silently fell back to novelty** on a
CUDA out-of-memory while another GPU app held VRAM. From the UI both look
identical — anonymous `A/B/C` sections with no signal that the good model never
ran, why, or how long anything took.

Two needs fall out:

1. **Visibility** — first-class, in-app insight into the heavy operations: how
   long each took, on which device, which engine produced the result, and a
   history of past runs.
2. **Control** — a way to choose CPU vs GPU for analysis so a tight-VRAM machine
   reliably gets SongFormer (slower, but it can't OOM), plus an automatic
   GPU→CPU recovery so a transient OOM stops silently degrading the result.

## Goals

- Surface per-operation **timing, device, and engine** for the heavy ops, live
  and as persisted history, in the app.
- Let the user pick analysis **device** (`auto` GPU-first vs forced `cpu`), and
  **automatically recover** a CUDA-OOM SongFormer run onto CPU instead of
  dropping to novelty.
- Make a silent novelty fallback **impossible to miss** (show the engine).
- Keep all of it **internal to the `earworm` binary** — no new external
  dependency, no new Rust crate, no change to `scripts/*` or the analyzer's JSON
  contract.

## Non-goals

- Developer-grade performance profiling (flamegraphs, `tracing` spans, audio
  xrun/latency metrics, lock-contention timing). Explicitly a separate, future
  layer — this spec is in-app *operational* visibility for the user.
- Profiling the real-time audio engine or every dispatcher command. Scope is the
  heavy ops only: `song.open`, `song.import`, `capture.grab`, `analysis.run`,
  `stems.separate`.
- Per-stage timing *inside* the Python (beat_this vs SongFormer split) — that
  would require the script to report it, which the internal-only constraint
  rules out. Rust times the whole subprocess.
- Per-stage timing inside Demucs (it stays whole-op).

## Constraints

**Internal to the binary.** All timing uses `std::time::Instant`. Persistence
uses the already-bundled `rusqlite`; serialization the existing `serde`.
**Zero new dependencies.** The Python scripts and the analyzer's
`{bpm,beats,downbeats,sections,engine}` stdout contract are **not modified** —
the `engine` field already distinguishes `songformer` from `beat_this+novelty`,
which is all Rust needs.

## Design

### Data model (`crates/practice/src/model.rs`)

```
struct ProfileStage { name: String, ms: u64, note: Option<String> }

struct ProfileRun {
    op: String,            // "analysis" | "stems" | "open" | "import" | "grab"
    song_id: Option<SongId>,
    started_at: String,    // RFC3339, like other timestamped rows
    total_ms: u64,
    ok: bool,
    error: Option<String>,
    device: Option<String>,   // "gpu" | "cpu" | null (n/a, e.g. novelty/non-ML)
    engine: Option<String>,   // analysis only: "songformer" | "beat_this+novelty"
    stages: Vec<ProfileStage>,
}
```

Plain wire types, serialized like everything else. `stages` holds the Rust-
visible sub-phases (e.g. analysis → `decode?`, `analyze`; open → `decode`,
`hash`, `peaks`).

### Settings: analysis device (`settings` table, existing V3)

One new durable key `analysis_device` ∈ `{"auto","cpu"}`, default `"auto"`,
stored as JSON like the other settings — **no schema change**. `auto` =
GPU-first with automatic CPU recovery (below); `cpu` = never touch the GPU.

### Device control + auto-recovery (Rust-orchestrated, `analysis.rs` / `app.rs`)

The `Analyzer::analyze` and `StemSeparator::separate` trait methods gain a
`force_cpu: bool` parameter (the `Fake*` test doubles ignore it). When
`force_cpu`, the impl sets `CUDA_VISIBLE_DEVICES=""` on the spawned `Command` —
which forces the entire subprocess (beat_this + SongFormer, or Demucs) onto CPU.
This is the only mechanism; no `-d` flag, no script awareness.

`analysis.run` reads `analysis_device` and orchestrates recovery **in Rust**:

1. If setting is `cpu` → run once with `force_cpu = true`. `device = "cpu"`.
2. If setting is `auto` → run with `force_cpu = false` (GPU-first).
   - Result engine `songformer` → done. `device = "gpu"`.
   - Result engine `beat_this+novelty` **and** a SongFormer venv is present
     (Rust checks the venv path, mirroring `songformer_python()`'s existence
     test) → the GPU attempt fell back; **retry once with `force_cpu = true`**.
     If the retry yields `songformer`, use it with `device = "cpu"`; record both
     attempts as stages (`analyze (gpu)` → note "OOM/fallback", `analyze (cpu)`).
   - No venv, or retry still non-songformer → keep the result; `device = null`.

This resolves the device-accuracy question from the design discussion: because
Rust drives the fallback, it *knows* the actual device in `auto` mode (gpu if
the first attempt produced SongFormer, cpu if the recovery retry did). Accepted
cost: the recovery retry re-runs beat_this redundantly (~seconds within a
multi-minute CPU run) — acceptable for a rare recovery path, optimizable later.

Stems have no second engine; `stems.separate` honors the same setting
(`force_cpu` when `cpu`) but has no fallback — Demucs either runs or errors.
Because Rust does not orchestrate a stems fallback and the internal-only
constraint forbids parsing Demucs output, its `device` is recorded as `"cpu"`
when forced and otherwise as `"auto"` (the configured mode, not a guaranteed
actual device) — unlike analysis, where the orchestration yields the exact
device.

### Instrumentation points (a small `Timer`, `crates/server`)

A minimal helper — an `Instant` plus a `Vec<ProfileStage>`, ~20 lines, no
framework:

```
let mut t = Timer::new("analysis", song_id);
t.stage("analyze", || analyzer.analyze(audio, force_cpu));
// ...
let run: ProfileRun = t.finish(ok, error, device, engine);
```

Wired at the seams that already exist:

- **`*_phased`** (`song.open`/`import`, `capture.grab`): time each slow
  sub-phase the function already performs as a stage — e.g. `decode`, `hash`,
  `peaks` for open/import; `snapshot` write for grab.
- **`analysis.run`** worker thread: the `analyze` stage(s) per the orchestration
  above; `engine` from the parsed JSON, `device` as derived.
- **`stems.separate`** worker thread: one `demucs` stage; `device` from setting.

### Persistence (schema **V4**, `crates/practice/src/store.rs`)

Add the next `PRAGMA user_version` block — a `profiles` table (id, op, song_id
nullable, started_at, total_ms, ok, error, device, engine, `stages_json`).
`stages` stored as `serde_json` in a `*_json` column, consistent with the
existing complex-sub-object convention. Methods `save_profile(&ProfileRun)` and
`list_profiles(limit)` (most-recent-first). **Bounded history:** `save_profile`
trims to the most recent 200 rows so it never grows without limit. Rows are not
foreign-keyed to `songs` (a profile of a since-deleted import is still useful
history); `song_id` is a soft reference.

### Events (existing `tick()` broadcast)

On completion, emit one `profile_run` event carrying the `ProfileRun`, through
the same `tick()` path `analysis_progress`/`stems_progress` already use — socket
subscribers and the webview both receive it. No new transport. The existing
`*_progress` events continue to drive the live "running" state; `profile_run`
adds the durations and the persisted record.

### Frontend (`apps/desktop/src`)

- **Settings** (`SettingsModal.svelte`, `stores.ts` known keys): an
  `analysis_device` radio — `Auto (GPU when it fits, else CPU)` /
  `CPU (slower, no VRAM limit)`. Writes via the existing `settings.set`.
- **Engine surfacing** (`Sections.svelte`): a compact caption on the section
  lane — `SongFormer` vs `novelty (SongFormer unavailable)` — driven by the
  already-present `analysis.engine`. Closes the "silent fallback" gap directly.
- **Profiling panel** (new component, sibling to `Library`/`DuePanel`): recent
  `ProfileRun`s — op, song, total time, `device`/`engine` badges, and a
  horizontal stage-bar breakdown. A new `profiles` store slice mirrors
  `profile_run` events and a `profiles.list` fetch at launch.
- **Prepare modal**: a one-line "last run" summary (`229 s · CPU · SongFormer`)
  from the latest matching `ProfileRun`.

### Backend command

**`profiles.list`** `{limit?}` → `Vec<ProfileRun>` (most recent first), for the
panel's initial load. Live updates arrive via the `profile_run` event.

### Testing

- **`Timer` (unit):** stage accumulation and total are pure/deterministic —
  inject elapsed values rather than sleeping.
- **Store (Rust, in-memory DB):** `save_profile` round-trips including
  `stages_json`; `list_profiles` orders most-recent-first; the 200-row trim
  holds.
- **Device control (`app.rs`, `FakeAnalyzer`):** `analysis_device = "cpu"` calls
  the analyzer with `force_cpu = true`; `auto` + a novelty result + present venv
  triggers exactly one CPU retry; `auto` + songformer result does **not** retry.
  Assert the resulting `ProfileRun.device`/`engine`.
- **Frontend (vitest):** the `profiles` store appends on a `profile_run` event;
  the settings radio writes `analysis_device`; `Sections` shows the engine
  caption. Mock `cmd` as `ipc.test.ts` does.

## Out of scope / future

- A `tracing`-based developer-profiling layer (spans, flamegraph export) and
  real-time audio-engine metrics — a separate spec, deliberately deferred.
- Per-stage timing inside the Python analyzer and inside Demucs (would need the
  scripts to report it; excluded by the internal-only constraint).
- Skipping the redundant beat_this pass on the CPU recovery retry.

## Open questions (resolved)

- **Device accuracy in `auto` mode.** Initially flagged as unknowable without
  the Python reporting it. *Resolved: the Rust-orchestrated GPU→CPU recovery
  makes the actual device knowable internally — gpu if the first attempt
  produced SongFormer, cpu if the recovery did.*
- **History growth.** *Resolved: bounded to the most recent 200 runs, trimmed in
  `save_profile`.*
- **Profiles of deleted tracks.** *Resolved: keep them — `song_id` is a soft
  reference, not a cascading FK; past-run history outlives the track.*
