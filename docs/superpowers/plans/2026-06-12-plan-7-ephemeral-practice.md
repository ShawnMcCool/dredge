# dredge — Plan 7: ephemeral practice (select → `p` → play)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Zero-ceremony practice: a waveform selection + `p` runs an instant micro-session (listen ×2 → 6 play reps on oscillate 0.7/1.0/3). Nothing persists unless the user rates it at the end — then the loop is auto-named, saved, the rated rep recorded, and resurfacing scheduled. Discard leaves no trace.

**Architecture:** Reuses `PlanRunner` wholesale. A transient `LoopRegion` with the existing unsaved sentinel `LoopId(0)` lives only in `ActivePlan.loops` plus a new `App.ephemeral: Option<LoopRegion>`. While an ephemeral session is active, `tick()` skips automatic rep recording (FK on `reps.loop_id` would reject id 0 anyway — the skip is the design, the FK is the backstop). `practice.quick_rate` is the single persistence point.

**Spec:** `docs/superpowers/specs/2026-06-12-dredge-design.md` (Ephemeral practice amendment)

---

### Task 1: Server — `practice.quick` / `quick_rate` / `quick_discard`

**Files:**
- Modify: `crates/server/src/app.rs`
- Test: `crates/server/tests/app_quick.rs`

- [x] **Step 1: Write failing tests**

`app_quick.rs` (setup helpers as in `app_plan_run.rs`: import + open a generated WAV, shared `Arc<Mutex<MockEngine>>`):

1. `quick_starts_listen_first_immediately` — `practice.quick {start: 0.5, end: 1.5}` → ok; mock `sent` ends with `SetLoopSecs{start:0.5,end:1.5}`, `SetRate(1.0)`, `Mute(false)`, `Play`; `status` shows a plan with mode `"listen"` and `plan_id` 0.
2. `quick_session_progression_and_no_rep_rows` — drive `LoopWrapped` through `tick()` 8 times: events show 2 listen reps then 6 play reps with rates following oscillate(0.7, 1.0, 3) = `[0.7, 0.7, 1.0, 0.7, 0.7, 1.0]`, then `plan_finished`; **the `reps` table stays empty** (query via a `loop.list`-style check is impossible for reps — assert through `retention` empty AND `due.list` empty; plus `loop.list` shows no loops persisted).
3. `quick_rate_persists_loop_rep_and_schedule` — after the session finishes, `practice.quick_rate {rating: "solid"}` → response is a persisted loop whose name matches `riff 0:00.5–0:01.5` (format `riff M:SS.t–M:SS.t`); `loop.list` now has 1; `retention`-style check: `due.list` is empty today but `store` has a resurfacing row (assert via a second `quick_rate` error OR expose through the existing `rep.rate` pattern — simplest: rate `"miss"` in a fresh test and assert `due.list` shows the loop tomorrow is impossible without time travel, so instead assert the response carries `{interval_idx: 0, due_on: <tomorrow>}` like `rep.rate` does).
4. `quick_discard_leaves_no_trace` — finish a session, `practice.quick_discard` → ok; `loop.list` empty; starting a new `practice.quick` works.
5. `quick_requires_open_song_and_valid_span` — without open song → error; `start >= end` → error.

- [x] **Step 2: Implement**

- `App.ephemeral: Option<LoopRegion>` field.
- `"practice.quick" {start, end}`: validate open song + `0 <= start < end <= duration`; clamp end to duration. Region: `LoopRegion { id: LoopId(0), song_id, name: auto, kind: Manual, start, end }`, auto-name `format!("riff {}–{}", fmt_ts(start), fmt_ts(end))` with `fmt_ts` = `M:SS.t`. Steps: `ListenFirst{loop_id: LoopId(0), reps: 2}`, `PlayReps{loop_id: LoopId(0), reps: 6, curve: Oscillate{low: 0.7, high: 1.0, period: 3}}`. Build `ActivePlan { plan_id: PlanId(0), runner, loops: {LoopId(0) => region} }`, set `ephemeral`, apply first rep (existing `apply_rep`). Starting a quick session replaces any active plan; `plan.start` and `plan.stop` clear `ephemeral`.
- `tick()` rep-recording branch: skip `store.record_rep` when `self.ephemeral.is_some()`.
- `"practice.quick_rate" {rating}`: require `ephemeral` Some (any time after start — mid-session rating is allowed and just ends it: stop plan, Pause). Persist: `store.insert_loop` (region fields), `store.record_rep` (mode `"play"`, rate = last position rate or 1.0, rating, is_retest false), resurfacing `next_state` + upsert (same code path as `rep.rate` — extract a private helper to avoid duplication), sidecar write, clear `ephemeral`, return `{loop: <persisted>, interval_idx, due_on}`.
- `"practice.quick_discard"`: clear `ephemeral`; if its plan is still active, stop + Pause. Always ok.

- [x] **Step 3: Run (fail→pass), full server suite, commit**

Run: `cargo test -p server`

```bash
git add -A && git commit -m "feat(server): ephemeral practice — quick start, rate-to-persist, discard"
```

---

### Task 2: UI — `p` key, runner integration, keep/discard prompt

**Files:**
- Modify: `apps/desktop/src/lib/keys.ts`, `apps/desktop/src/lib/stores.ts`, `apps/desktop/src/components/PlanRunner.svelte`

- [x] **Step 1: Implement**

- `stores.ts`: `quickActive` writable(false); actions `quickPractice(start, end)` (`practice.quick`, sets flag, clears selection), `quickRate(rating)` (`practice.quick_rate`, clears flag, refreshes loops — the new loop appears), `quickDiscard()`. `plan_finished` event while `quickActive` → set a `quickPromptVisible` flag instead of the normal summary.
- `keys.ts`: `p` → if selection exists and song open → `quickPractice(sel.start, sel.end)`. While `quickPromptVisible`: `1/2/3` → `quickRate(miss|shaky|solid)`, `Escape` → `quickDiscard()`. Help footer gains `p quick practice`.
- `PlanRunner.svelte`: when `quickActive`, header shows `QUICK` (muted) instead of the plan name; on `quickPromptVisible` show: "Keep this riff? 1 Miss · 2 Shaky · 3 Solid · Esc discard". Rating confirms with the saved loop name briefly; discard just closes.

- [x] **Step 2: Verify + commit**

Run: `pnpm build && pnpm vitest run` (in apps/desktop), then full gate `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`.

```bash
git add -A && git commit -m "feat(desktop): p-key ephemeral practice with keep/discard prompt"
```

---

### Task 3: Docs + live smoke

- [x] **Step 1: README** — add `p` to the workflow blurb (one line under "What it does": select → `p` → instant session; rate to keep).

- [x] **Step 2: Socket smoke** — against a live `dredged` (real engine): import the test sine, open, `practice.quick {start:1, end:3}` via `just cmd`, subscribe and observe `rep_changed` events progressing listen→play with the oscillate rates while audio plays; `practice.quick_rate {rating:"solid"}` → loop persisted (verify `loop.list`); cleanup temp db.

- [x] **Step 3: Commit**

```bash
git add -A && git commit -m "docs+test: ephemeral practice smoke verified"
```

---

## Self-review checklist

- Amendment coverage: select+`p` instant session ✔, nothing persisted unless rated ✔ (tick skip + single persistence point), auto-name ✔, resurfacing pickup ✔, discard traceless ✔, full builder untouched ✔.
- Reuse: PlanRunner, apply_rep, rating/resurfacing path (extracted helper), sidecar write — no new entity types.
- Edge: `p` during an active plan replaces it (documented in test 4's "starting a new quick works"); LoopId(0) sentinel never reaches the DB.
