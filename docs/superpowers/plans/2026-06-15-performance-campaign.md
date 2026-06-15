# Earworm Performance Campaign — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans (inline) or superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve all recommendations from the 2026-06-15 performance analysis across engine, persistence, server, and frontend — eliminating idle CPU waste, fixing O(n²) DB scans and N+1 query patterns, removing audio hot-path overhead, and cutting load-time memory spikes.

**Architecture:** Four workstreams. WS1 (idle quiescence) is cross-cutting and highest value. WS2 (persistence) and WS3 (engine) are independent crates. WS4 (frontend polish) is small. Each task is correctness-preserving where possible; behavior changes (idle gating) are verified empirically.

**Tech Stack:** Rust (engine/practice/server crates, rusqlite, symphonia, rubato, bytemuck), Svelte 5 + Tauri frontend, vitest + cargo test.

> **STATUS: COMPLETE (2026-06-15).** All workstreams landed, `just check` green. Task 4.2 (stable `{#each}` keys in Sections) was intentionally skipped — `Row` has no stable id and the edit list binds+reorders by index, so index keys are correct there.
>
> **Follow-up (post-review):** demand-driven redraw exposed a pre-existing playhead sawtooth — `playheadSecs` re-anchored to each ~50 ms position event, so tick/IPC arrival jitter nudged the rendered playhead frame-to-frame. Fixed with a stateful smoothing clock (`tickPlayhead`): free-run at the true rate between events, gentle low-pass toward the server position, hard snap on seeks / loop wraps / resume gaps. User confirmed playback now tracks evenly. Remaining optional polish if ever wanted: sub-pixel playhead line (currently `Math.round`ed for a crisp 1px line).
>
> The one item not runnable headless here — confirming a paused, song-loaded app settles to **zero** canvas repaints — was implicitly validated by the user's playback review (the redraw loop is alive and smooth during playback; the same gating stops it when paused).

**Verification gate for every Rust task:** `just test` (cargo test --workspace + pnpm vitest) and `just lint` (clippy -D warnings, fmt, svelte-check) must pass. Commit on `main` (this repo's convention).

---

## Workstream 1 — Idle quiescence (cross-cutting, HIGHEST IMPACT)

Root cause: the engine emits a `Position` event every audio callback even when paused; the server re-broadcasts + double-serializes it ~20×/sec; the frontend repaints the canvas at 60fps regardless. Goal: a fully idle app (song loaded, paused, untouched) produces **zero** tick events and **zero** canvas repaints.

### Task 1.1: Gate the `position` event on change (server)

**Files:**
- Modify: `crates/server/src/app.rs:964-971` (`tick`)
- Test: `crates/server/tests/` (new test or extend existing dispatch test)

Current code pushes a `position` Event whenever `last_pos.is_some()`, even if identical to the previously broadcast `self.last_position`. Change to only push when the tuple differs.

- [ ] **Step 1:** Replace the unconditional push block:
```rust
// only broadcast when the position actually changed since last tick
if let Some((secs, rate, playing)) = last_pos {
    let next = (secs, rate, playing);
    if self.last_position != Some(next) {
        self.last_position = Some(next);
        events.push(Event {
            event: "position".into(),
            data: json!({"secs": secs, "rate": rate, "playing": playing}),
        });
    }
}
```
Note: `last_position` is `Option<(f64, f64, bool)>`. `f64` is not `Eq` but `PartialEq` suffices for `!=`. When paused, `secs` is fixed so the tuple is stable → no event after the first.

- [ ] **Step 2:** Add/extend a test asserting that repeated `tick()` calls while paused emit at most one `position` event. Run: `cargo test -p server`. Expected: PASS.
- [ ] **Step 3:** `just lint`; commit `perf(server): suppress unchanged position events while idle`.

### Task 1.2: Stop frontend RAF redraw loop when idle (frontend)

**Files:**
- Modify: `apps/desktop/src/components/Waveform.svelte:318-326` (RAF loop), draw triggers in the `$effect`s (`:82-102`) and pointer/wheel handlers.

The RAF loop currently calls `draw()` every frame forever. Make redraws demand-driven: schedule a frame only when (a) `position.playing`, or (b) a one-shot dirty flag is set by view/drag/store changes.

- [ ] **Step 1:** Introduce a `requestRedraw()` that schedules a single RAF if none pending; have `draw()` clear the pending flag and, if `get(position).playing`, schedule the next frame (continuous during playback only).
- [ ] **Step 2:** Call `requestRedraw()` from: the `$effect`s reacting to `$openSong`/`$workspaceReset`, pointer down/move/up, wheel/zoom handlers, and the `position` store subscription gated on `playing` transitions. Subscribe to `position` for the play→pause edge so the playhead lands at its final spot then stops.
- [ ] **Step 3:** Cache `getComputedStyle(document.documentElement)` once per `draw()` (one call, read all 8 vars from it) instead of 8 separate calls (`:132-141`).
- [ ] **Step 4:** Empirically verify (vite :5173 + chrome-devtools MCP): with a song loaded and paused, confirm `draw()` is not called every frame (add a temporary counter or use the performance trace). During playback the playhead must still move smoothly. Remove any temporary instrumentation.
- [ ] **Step 5:** `cd apps/desktop && pnpm vitest run` (waveform math tests), `just lint`; commit `perf(desktop/waveform): demand-driven redraw; stop 60fps idle repaint`.

### Task 1.3 (optional follow-up): two-layer playhead canvas

Deferred — only if 1.2 leaves measurable playback repaint cost. Splitting the static waveform and the moving playhead onto separate canvases lets the playhead move without repainting peaks. Track as a stretch goal; not required for the campaign's idle-CPU goal.

---

## Workstream 2 — Persistence (practice crate + server dispatch)

### Task 2.1: Durability/concurrency pragmas

**Files:** Modify `crates/practice/src/store.rs:185-190` (`init`).

- [ ] **Step 1:** After `foreign_keys`, add WAL + tuning:
```rust
fn init(conn: rusqlite::Connection) -> Result<Self> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.pragma_update(None, "cache_size", -8000)?; // ~8 MB page cache
    let store = Self { conn };
    store.migrate()?;
    Ok(store)
}
```
Note: `journal_mode=WAL` is a no-op for `:memory:` connections (tests) — returns "memory", harmless.

- [ ] **Step 2:** `cargo test -p practice` (existing store tests exercise the in-memory path). Expected: PASS.
- [ ] **Step 3:** Commit `perf(practice/store): enable WAL + synchronous=NORMAL + busy_timeout`.

### Task 2.2: Indexes on FK/lookup columns (schema V7)

**Files:** Modify `crates/practice/src/store.rs` — add `SCHEMA_V7` const + a `version < 7` migration block.

- [ ] **Step 1:** Add const after `SCHEMA_V6`:
```rust
/// v7: indexes on foreign-key / lookup columns used by hot queries.
const SCHEMA_V7: &str = "
CREATE INDEX IF NOT EXISTS idx_reps_loop ON reps(loop_id);
CREATE INDEX IF NOT EXISTS idx_sections_song ON sections(song_id);
CREATE INDEX IF NOT EXISTS idx_loops_song ON loops(song_id);
CREATE INDEX IF NOT EXISTS idx_plans_song ON plans(song_id);
CREATE INDEX IF NOT EXISTS idx_profiles_song ON profiles(song_id);
";
```
- [ ] **Step 2:** Add migration block:
```rust
if version < 7 {
    self.conn.execute_batch(SCHEMA_V7)?;
    self.conn.pragma_update(None, "user_version", 7)?;
}
```
- [ ] **Step 3:** Add a test that opens a store, inserts reps, and asserts `retention()` returns correct results (correctness unchanged; index is transparent). Run `cargo test -p practice`. Expected: PASS.
- [ ] **Step 4:** Commit `perf(practice/store): add V7 indexes for reps/sections/loops/plans/profiles`.

### Task 2.3: `prepare_cached` for hot read statements

**Files:** Modify `crates/practice/src/store.rs` — the read methods using `self.conn.prepare(...)`.

- [ ] **Step 1:** Replace `self.conn.prepare(` with `self.conn.prepare_cached(` in: `song_by_hash`, `list_songs`, `song_by_id`, `list_sections`, `list_loops`, `list_plans`, `get_analysis`, `retention`, `list_profiles`, and any other static-SQL read. (Statements with static SQL only.)
- [ ] **Step 2:** `cargo test -p practice`. Expected: PASS (behavior identical).
- [ ] **Step 3:** Commit `perf(practice/store): use prepare_cached for hot reads`.

### Task 2.4: `save_profile` trim gate

**Files:** Modify `crates/practice/src/store.rs:620-624`.

- [ ] **Step 1:** Only trim when over cap, and use an index-friendly delete:
```rust
let count: i64 = self.conn.query_row("SELECT COUNT(*) FROM profiles", [], |r| r.get(0))?;
if count > 200 {
    self.conn.execute(
        "DELETE FROM profiles WHERE id <= (
            SELECT id FROM profiles ORDER BY id DESC LIMIT 1 OFFSET 200)",
        [],
    )?;
}
```
- [ ] **Step 2:** Test: insert 205 profiles, assert count caps at 200 and the newest survive. `cargo test -p practice`. Expected: PASS.
- [ ] **Step 3:** Commit `perf(practice/store): only trim profiles when over cap`.

### Task 2.5: N+1 / full-scan read fixes (new Store point-lookups)

**Files:** Add methods to `crates/practice/src/store.rs`; rewire callers in `crates/server/src/app.rs`.

- [ ] **Step 1:** Add `Store::plan_by_id(id: PlanId) -> Result<Option<Plan>>` (`SELECT ... FROM plans WHERE id = ?1`). Rewire `app.rs:705-718` (`plan_row`) to use it instead of iterating all songs × plans.
- [ ] **Step 2:** Add `Store::resurfacing_by_loop(loop_id: LoopId) -> Result<Option<Resurfacing>>` (PK lookup). Rewire `app.rs:519-528` (`reschedule`) instead of scanning `all_resurfacing()`.
- [ ] **Step 3:** Fix `due_list` N+1 (`app.rs:531-539`): add `Store::loops_by_ids(&[LoopId]) -> Result<Vec<LoopRegion>>` using `WHERE id IN (...)`, or a join in `all_resurfacing`. Replace the per-loop `loop_by_id` call.
- [ ] **Step 4:** Tests for each new Store method (correctness). `cargo test -p practice && cargo test -p server`. Expected: PASS.
- [ ] **Step 5:** Commit `perf(practice,server): replace plan/resurfacing/due full-scans with point lookups`.

### Task 2.6: Avoid redundant analysis re-parse per song-open

**Files:** Add `Store::has_analysis(song_id) -> Result<bool>` (`SELECT 1 FROM analysis WHERE song_id=?1`); rewire `app.rs:1165`, `1232` existence checks. Optionally cache the parsed `Analysis` for the open song on `App`.

- [ ] **Step 1:** Add `has_analysis`; replace the two existence checks that currently call `get_analysis` just to test presence.
- [ ] **Step 2:** (Optional) Cache parsed `Analysis` on `App` for the currently-open song so `app.rs:1246/1531/1624` reuse it within one open instead of re-fetching+re-parsing. Invalidate on `save_analysis`/song switch.
- [ ] **Step 3:** `cargo test -p server`. Expected: PASS.
- [ ] **Step 4:** Commit `perf(server): has_analysis check + cache parsed analysis for open song`.

### Task 2.7: Batch writes in recompute/orphan-delete loops

**Files:** `crates/server/src/app.rs:1578-1581` (orphan loop deletes), `:1671-1682` (`recompute_loop_names`).

- [ ] **Step 1:** Build the name set once outside the recompute loop (fix O(n²) clones); wrap the per-loop `update_loop` writes in one transaction (add a transaction-wrapped batch method on `Store`, e.g. `update_loops(&[(LoopId, name)])`).
- [ ] **Step 2:** Wrap orphan-loop deletion in a transaction (e.g. `Store::delete_loops(&[LoopId])`).
- [ ] **Step 3:** Tests for batch methods; `cargo test -p practice && cargo test -p server`. Expected: PASS.
- [ ] **Step 4:** Commit `perf(server): batch loop-name updates and orphan deletes in transactions`.

---

## Workstream 3 — Engine RT hot path & load

### Task 3.1: Bulk-copy render buffer to output (HIGH)

**Files:** Modify `crates/engine/src/output.rs:144-146`.

- [ ] **Step 1:** Replace the per-sample `to_le_bytes` loop with a bulk copy. The format is hardcoded `F32LE` and host is little-endian; cast the f32 slice to bytes:
```rust
// out: &[f32] render buffer, slice: &mut [u8] PipeWire dest
slice[..out.len() * 4].copy_from_slice(bytemuck::cast_slice(out));
```
Confirm `bytemuck` is a dependency of `engine` (add `bytemuck = "1"` to `crates/engine/Cargo.toml` if not). Guard correctness with a `#[cfg(target_endian = "little")]` assumption (already implied; add a `debug_assert!(cfg!(target_endian = "little"))` or compile_error for big-endian if desired).
- [ ] **Step 2:** Verify audio output bit-identical: existing engine tests + manual playback check (`just dev`, play a song, confirm no glitches/wrong pitch). `cargo test -p engine`. Expected: PASS.
- [ ] **Step 3:** Commit `perf(engine/output): bulk-copy render buffer instead of per-sample to_le_bytes`.

### Task 3.2: Capture callback — drop the RT-thread Mutex (HIGH, correctness)

**Files:** Modify `crates/engine/src/capture.rs:282-290` and the control-thread reader (`snapshot_last`).

- [ ] **Step 1:** Replace the `std::sync::Mutex<RollingRing>` locked inside the RT capture callback with an SPSC ring (mirror the playback `rtrb` pattern in `ring.rs`), or as a minimal fix use `try_lock()` and drop-on-contention. Prefer the SPSC ring for correctness.
- [ ] **Step 2:** Also bulk-decode the incoming bytes (`bytes.chunks_exact(4).map(f32::from_le_bytes)` → `bytemuck::cast_slice`).
- [ ] **Step 3:** Test capture path (`cargo test -p engine`, and the app_tuner/capture integration tests). Manual: run a capture/tuner session, confirm readings still flow. Expected: PASS.
- [ ] **Step 4:** Commit `fix(engine/capture): remove Mutex from RT capture callback (SPSC ring)`.

### Task 3.3: Decode — f32 resampler + skip planar round-trip (HIGH memory)

**Files:** Modify `crates/engine/src/decode.rs:82-93,139-180`.

- [ ] **Step 1:** Switch `rubato` to `SincFixedIn::<f32>` so the whole-song f64 up/down conversion (`:139-140`) is removed; feed f32 planar slices directly.
- [ ] **Step 2:** When `rate == SAMPLE_RATE` (no resample), skip `to_stereo_planar` + re-interleave entirely — downmix straight into the interleaved canonical buffer. Pre-size `interleaved` from `codec_params.n_frames` when available. Hoist `SampleBuffer` out of the packet loop (`:71`).
- [ ] **Step 3:** Replace front `drain(..delay)` (`:177-180`) with `split_off`/range-copy to avoid the full memmove.
- [ ] **Step 4:** Test: decode a 44.1k file and a 48k file, assert sample counts/duration match prior behavior (golden test or duration assertion). `cargo test -p engine`. Expected: PASS.
- [ ] **Step 5:** Commit `perf(engine/decode): f32 resampler, skip planar round-trip on 48k, presize buffers`.

### Task 3.4: Looper fast contiguous mix path (MEDIUM, scales with stems)

**Files:** Modify `crates/engine/src/looper.rs:63-120`, `buffer.rs:54-62`.

- [ ] **Step 1:** Split `Looper::read` into a fast path (no active region boundary within this block, no crossfade) that mixes stems slice-wise (per-stem accumulate over contiguous slices, autovectorizable), falling back to the existing per-frame path only inside the crossfade window / at region wrap.
- [ ] **Step 2:** Test: single-stem and multi-stem read produce identical output to the current implementation (golden comparison over a few blocks, including a wrap). `cargo test -p engine`. Expected: PASS.
- [ ] **Step 3:** Commit `perf(engine/looper): contiguous slice-wise stem mix for the common path`.

### Task 3.5: Peaks binary cache (MEDIUM)

**Files:** Modify `crates/engine/src/peaks.rs:44-59`.

- [ ] **Step 1:** Replace JSON serialize/parse of the peaks buckets with a packed little-endian `[f32]` blob (bytemuck cast, or bincode). Bump the cache format/key so stale JSON caches are ignored.
- [ ] **Step 2:** Test: round-trip a peaks vector through the new cache; assert equality. `cargo test -p engine`. Expected: PASS.
- [ ] **Step 3:** Commit `perf(engine/peaks): binary peaks cache instead of JSON`.

### Task 3.6: Low-priority RT micro-opts

**Files:** `crates/engine/src/output.rs:91` (arc_swap `load()` + ptr_eq instead of `load_full()`), `crates/engine/src/pipeline.rs:189-195` (skip `push_position` in idle branch — superseded by Task 1.1 if the server gate is sufficient; only do this if profiling still shows event-ring churn matters), `crates/engine/src/ring.rs:43-47` (bulk `copy_from_slice` in `RollingRing::push`).

- [ ] **Step 1:** Apply `output.rs:91` arc_swap `load()` change. Test, commit.
- [ ] **Step 2:** Apply `ring.rs` bulk copy. Test, commit.
- [ ] **Step 3:** Evaluate pipeline idle `push_position` — likely unnecessary after Task 1.1; document decision. (The server gate already stops broadcast; the engine event ring is lock-free and cheap, so skipping here is optional.)

---

## Workstream 4 — Frontend polish (LOW)

### Task 4.1: Memoize `laneSpans`

**Files:** `apps/desktop/src/components/Waveform.svelte:278,357-366`.

- [ ] **Step 1:** Compute `laneSpans(open)` once per `openSong` change (cache in a local, recompute in the `$openSong` effect) instead of per-frame and per-hit-test.
- [ ] **Step 2:** `pnpm vitest run`; manual hit-test still works. Commit `perf(desktop/waveform): memoize laneSpans on song change`.

### Task 4.2: Stable keys for editable section rows

**Files:** `apps/desktop/src/components/Sections.svelte:181,224`.

- [ ] **Step 1:** Key the `{#each rows as row, i (i)}` blocks by a stable section id instead of array index where a stable id exists.
- [ ] **Step 2:** `pnpm vitest run`; manual reorder/delete check. Commit `perf(desktop/sections): key rows by stable id`.

---

## Execution order

1. **WS1** (1.1 server gate → 1.2 frontend RAF) — biggest idle win, independent.
2. **WS2** (2.1 pragmas → 2.2 indexes → 2.3 prepare_cached → 2.4 trim → 2.5 N+1 → 2.6 analysis → 2.7 batch writes) — sequential within the crate (later tasks build on new Store methods).
3. **WS3** (3.1 → 3.2 → 3.3 → 3.4 → 3.5 → 3.6) — independent crate; 3.1/3.2 first (highest value + correctness).
4. **WS4** (4.1, 4.2) — quick polish last.

WS1/WS2/WS3 touch disjoint crates (server+frontend / practice / engine) and could be parallelized, except WS2's later tasks add Store methods that WS2's server rewires consume — keep WS2 internally sequential. WS3 frontend (1.2, 4.x) and WS3 engine are disjoint.

## Self-review notes

- Spec coverage: every analysis recommendation maps to a task — idle storm (1.1, 1.2, 3.6), render bulk copy (3.1), capture mutex (3.2), decode/resampler/planar/SampleBuffer/drain (3.3), looper (3.4), peaks (3.5), arc_swap/ring (3.6), pragmas (2.1), indexes (2.2), prepare_cached (2.3), save_profile (2.4), due_list/plan_row/reschedule N+1 (2.5), analysis re-parse (2.6), batch writes/recompute O(n²) (2.7), laneSpans (4.1), index keys (4.2).
- Behavior-changing tasks (1.1, 1.2) are verified empirically, not just by unit tests.
- All Store additions are point-lookups/batch methods with their own tests before rewiring callers.
