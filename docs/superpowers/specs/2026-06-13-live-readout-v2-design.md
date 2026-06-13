# Live readout v2 — shared meters, persisted max metrics, completion summary

Date: 2026-06-13

## Problem

The live work readout (`LiveProgress.svelte`) shipped its first version: per-step
cpu/gpu bars and a vram histogram nested under the active step. Refinements:

- The cpu/gpu/vram meters are generically useful and shouldn't be nested per
  step — they should sit **once** below the step list and run during both
  efforts.
- The **vram histogram** belongs in **its own column** beside the cpu/gpu bars,
  not stacked under them.
- Each step should name the **model** doing that task.
- When a run finishes, the readout should show a **completion summary**: max CPU,
  max GPU, and max VRAM utilization, plus the time **broken down by each stage**,
  per effort. These maxes should **persist** (in the profile) so they survive a
  restart and enrich the Profile history panel.

## Goals

1. One shared meters block below the step list, active during both analysis and
   stems.
2. VRAM histogram in its own column to the right of the cpu/gpu bars.
3. Model name next to each step.
4. A completion summary per effort: total + per-stage times + max cpu/gpu/vram.
5. Max metrics persisted into each `ProfileRun` (also visible in the Profile
   panel) and surviving restart.

## Non-goals

- New sampling sources — CPU via `/proc`, GPU via `nvidia-smi` are unchanged.
- Grouping the two profiles into a single "prepare" record. The summary shows
  the newest analysis profile and the newest stems profile independently.
- Cross-stage maxes in one number — maxes are per effort (per profile).

## Design

### Backend: persist max metrics (schema V5)

**`WorkState`** (`crates/server/src/sampler.rs`) gains running maxes, reset on
`begin`:
```rust
pub max_cpu: u32,
pub max_gpu_util: Option<u32>,
pub max_vram_used_mb: Option<u32>,
pub vram_total_mb: Option<u32>,
```
The **sampler loop**, each tick after computing `cpu`/`gpu`, locks the shared
`WorkState` (when `Some`) and folds the new sample into the maxes
(`max_cpu = max(max_cpu, cpu)`, and for GPU `max(prev, util)` / used / set total).
This is a second brief lock per tick, off the pump.

**`WorkReporter`** gains:
- `begin` initializes the max fields to `0`/`None`.
- `maxes(&self) -> Option<(u32, Option<u32>, Option<u32>, Option<u32>)>` reads the
  current maxes (cpu, gpu_util, vram_used, vram_total) from the shared state.

**Worker** (analysis + stems, `app.rs`): after the heavy call returns and BEFORE
`reporter.end()`, capture `let m = reporter.maxes();`, then `end()`, then stamp
the maxes onto the `ProfileRun` before sending it.

**`ProfileRun`** (`crates/practice/src/model.rs`) gains four optional fields:
```rust
#[serde(default, skip_serializing_if = "Option::is_none")] pub max_cpu_pct: Option<u32>,
#[serde(default, skip_serializing_if = "Option::is_none")] pub max_gpu_util: Option<u32>,
#[serde(default, skip_serializing_if = "Option::is_none")] pub max_vram_used_mb: Option<u32>,
#[serde(default, skip_serializing_if = "Option::is_none")] pub vram_total_mb: Option<u32>,
```

**Store** (`crates/practice/src/store.rs`): schema **V5** adds the four columns to
the `profiles` table (`max_cpu_pct INTEGER`, `max_gpu_util INTEGER`,
`max_vram_used_mb INTEGER`, `vram_total_mb INTEGER`, all nullable). `save_profile`
inserts them; `list_profiles` reads them (NULL → None). Old (pre-V5) rows read
back with `None` maxes.

`Timer::finish` is unchanged; the worker sets the max fields on the returned
`ProfileRun` from `reporter.maxes()`. Analysis maxes span GPU attempt + CPU
recovery (the whole analysis run); stems maxes span demucs.

### Frontend: `LiveProgress.svelte` redesign

**Types** (`stores.ts`): `ProfileRun` interface gains `max_cpu_pct?`,
`max_gpu_util?`, `max_vram_used_mb?`, `vram_total_mb?` (numbers).

**Active view** (`$prepareState` set):
- Header `PREPARING` → **`ANALYZING`**.
- **Step rows** show the model: `analyzing structure · SongFormer` /
  `separating stems · Demucs` (static names — the tool for each task). The active
  step still shows its live stage + elapsed.
- **One shared meters block** below the step list (moved out of the per-step
  `{#if active}`), rendered while `$workSample` is set regardless of which step is
  running. Layout is two columns: **left** stacks the `cpu` and `gpu` bar rows;
  **right** is the `vram` histogram (its own column, taller) with the amber peak
  line and the `used / total GB` label beneath.

**Completion / idle view** (`$prepareState` null) — replaces the one-line "last
run" with a per-effort summary, derived from the persisted `profiles` store
(newest profile per op). For each of analysis and stems that exists:
- a heading line: `op · total s · device · engine` (device/engine omitted when
  absent),
- per-stage times: `GPU attempt 22.6 s · CPU recovery 194.4 s` (from
  `profile.stages`),
- a max line (only when present): `max cpu 496% · gpu 41% · vram 6.1/16 GB`.

A small pure helper selects the newest profile for an op and formats these lines;
it is unit-tested. Renders nothing when there are no profiles.

### Data flow

sampler folds maxes into `WorkState` → worker reads `reporter.maxes()` at end →
`ProfileRun` (with maxes) persisted (V5) + emitted (`profile_run`) → `profiles`
store → completion summary + Profile panel. Live meters still ride
`workSample`/`vram` as before.

## Testing

- **Rust (unit):** `WorkState`/sampler max-folding via the reporter (begin resets,
  fold updates, `maxes()` reads); store V5 round-trip including the four max
  columns (and a pre-V5-style row with NULL maxes → None).
- **Rust (app):** existing profiling regression test still green; an analysis run
  produces a `profile_run` whose data includes a `max_cpu_pct` field.
- **Frontend (vitest):** the completion-summary helper picks the newest profile
  per op and formats heading/stages/max lines (incl. the no-max case).
- **Frontend:** `LiveProgress.svelte` via `svelte-check` + `pnpm build`.

## Open questions (resolved)

- **Max source:** persisted in `ProfileRun` (schema V5), not session-only.
  *Resolved.*
- **Stage breakdown:** per effort (separate analysis / stems blocks). *Resolved.*
- **Model names:** static per step (SongFormer / Demucs); actual engine shown in
  the completion summary. *Resolved.*
- **Which profiles drive the summary:** newest profile per op from the persisted
  store (no session-only state; survives restart). *Resolved.*
