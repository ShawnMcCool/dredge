# Stop analyzing — design

**Date:** 2026-07-17
**Status:** approved, implementing

## Problem

The "prepare" flow (the modal that reads **ANALYZING**) runs two sequential
Python subprocesses per song: structure/beat **analysis** (`analysis.run`) then
Demucs **stem separation** (`stems.separate`). Each runs a blocking
`cmd.output()` inside a detached thread. There is no way to interrupt a run once
started — the modal (`LiveProgress.svelte`) only grows a close affordance after a
*failure*. A misfired or overlong run holds CPU/GPU/VRAM with no escape.

## Behavior

A **Stop** button on the running prepare modal aborts the step in flight and
prevents any later step from starting. A step that already finished keeps its
cached result (stop stems → the fresh analysis stays). Stopping reaps the Python
subprocess *and its uv/torch grandchildren* so VRAM is actually freed, and leaves
no partial cache.

## Why a process-group kill

`cmd.output()` owns the child with no handle to kill. Two facts force the
approach:

- The child is not in its own process group, and the `analyze`/`demucs` wrappers
  spawn grandchildren (uv → python → torch). `child.kill()` on the direct child
  can orphan them.
- `die_with_parent` (PR_SET_PDEATHSIG) only fires when *dredge itself* exits —
  useless for a manual cancel.

So a clean stop spawns the child in its own process group (`setpgid(0,0)`) and
kills the whole group with `killpg(pgid, SIGKILL)`.

## Components

### `crates/server/src/proc.rs` (new)

The kill primitive, co-located with the existing `die_with_parent`:

- `CancelToken` — cloneable; wraps `AtomicBool cancelled` + `Mutex<Option<pgid>>`.
  - `is_cancelled()`
  - `cancel()` — set the flag; if a child is registered, `killpg(pgid, SIGKILL)`.
- `run_cancellable(cmd, &token) -> Outcome { Done(Output) | Cancelled | Err }`
  - returns `Cancelled` if the token fired before spawn;
  - `pre_exec`: `setpgid(0,0)` (own group) plus the existing PDEATHSIG arming;
  - stores the child pgid (== child pid, the group leader) in the token, then
    re-checks `is_cancelled()` to close the spawn/cancel race;
  - `Cancelled` if the token fired by the time the child exits.

### Worker plumbing

- `Analyzer::analyze` / `StemSeparator::separate` gain a `&CancelToken` param
  (the Fakes ignore it); the Script/Demucs impls call `run_cancellable` instead
  of `cmd.output()`, mapping `Cancelled` to an `Err("cancelled")`.
- `analyze_with_recovery` takes the token and checks `is_cancelled()` before the
  GPU→CPU retry, so a stop never spawns the second child.

### App (`app.rs`)

- `analyzing: HashSet<i64>` → `HashMap<i64, CancelToken>`; `separating:
  Arc<Mutex<HashSet<i64>>>` → `Arc<Mutex<HashMap<i64, CancelToken>>>`. Status /
  busy guards become `contains_key`.
- New dispatch command **`prepare.cancel { song_id }`** — cancels whichever
  token exists for that song (one command for one Stop button).
- The analysis result channel gains a `Cancelled` variant
  (`AnalysisOutcome { Done | Failed | Cancelled }`). On cancel, the worker emits
  `analysis_progress { state: "cancelled" }` / `stems_progress {…}`, writes no
  cache, and skips the profiling record.

### Frontend

- `PrepareStepState` gains `"cancelled"` (rendered neutrally, not a red failure).
- Module flag `prepareCancelled`, reset at `prepare()` start. A **Stop** button on
  `LiveProgress.svelte` calls `actions.cancelPrepare()` → sets the flag +
  `cmd("prepare.cancel", { song_id })`.
- `prepare()`'s sequential body skips the stems step when the flag is set, still
  runs the completed-step refresh, then closes the modal. The `*_progress` waiter
  already resolves on any terminal event, so a `cancelled` state resolves it; the
  handlers already ignore non-`done`/`failed` states.

## Testing

- Rust: `run_cancellable` kills a `sleep` stub mid-run and returns `Cancelled`;
  cancel-before-spawn never runs the command; `analyze_with_recovery` skips the
  CPU retry when pre-cancelled.
- Vitest: `prepare()` cancelled during analysis skips the stems step and closes.

## Files

new `proc.rs`; `analysis.rs`, `stems.rs`, `app.rs`; `stores.ts`,
`LiveProgress.svelte`; tests alongside.
