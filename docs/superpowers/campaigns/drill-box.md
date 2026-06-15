# Campaign: the Drill box

A live practice workbench for the loop you're currently grinding. Designed
2026-06-15 (brainstorm + category research). Work directly on `main`.

> **For agentic workers:** phases are dependency-ordered. Each phase has a
> verification gate and a commit. Frontend phases (1–6) are **backend-free** —
> they ride existing dispatch commands. Phase 7 (count-in) is the only engine
> work and is the explicit stretch/final phase; it can ship separately or be
> deferred without blocking 1–6.

## What it is

A **stage box** (`Drill.svelte`, label `drill`) that materializes only when a
loop is **active** (`$currentLoop !== null`) and clears when no loop is active
or the workspace is reset. It's a *wide* box — full row by default, going
compact only when the stage is wide. Ephemeral: no history, no ratings (that's
the deliberately-excluded "B" surface). It is the bridge between "just looping a
section" and "running an authored Plan," and is built so single-loop structured
practice ("C") can grow out of it later at low cost.

## The mental model (decisions, final)

- **Spine = a step-up tempo trainer.** The category-universal feature (Soundslice
  speed training, Anytune Loop Trainer, Amazing Slow Downer). It does *not*
  introduce a second tempo: it **autopilots the global Transport rate** across
  loop passes. The box's only new persistent idea is a **ramp recipe**
  (start / target / step / curve) — and the curve choices are exactly the
  existing `TempoCurve` (`Dwell`/`Ladder`/`Oscillate`).
- **One source of truth for rate.** The trainer animates `position.rate` via the
  existing `rate` command; the Transport slider visibly moves. Disarm leaves the
  rate where it landed; an explicit "reset rate" returns to 1.0.
- **Scratch span, saved loops untouched.** When a loop goes active, a
  `drillSpan {start,end}` is seeded from the loop's real bounds. Every
  region-moving toy (nudge / isolate / run-up) edits `drillSpan` only and
  re-issues `loop.set`; the saved `LoopRegion` is never mutated. "Reset span"
  snaps `drillSpan` back to the active loop's bounds. This mirrors how the
  waveform already keeps zoom/active-span as ephemeral state separate from
  stored loops.
- **Beat-snapped.** Region edits snap to the analysis beat/downbeat grid
  (`$openSong.analysis`, reusing `waveform-math` helpers) when a grid exists.
- **Graduation path (not built now):** the recipe *is* a `TempoCurve` and
  recall-every-Nth *is* a live `RecallTest`, so "save this drill → emit a
  `PlanStep` / promote `drillSpan` to a real loop" is later a small step.

## Toolkit (the five moves)

1. **Step-up tempo trainer** (spine) — arm a recipe, autopilot the global rate
   across loop cycles.
2. **Nudge** — move scratch in/out edges by one grid step, snapped.
3. **Isolate** — shrink to a sub-chunk (first-half / second-half bisect, or drag).
4. **Run-up** — extend the scratch *start* backward by N bars to rehearse the
   entrance ("back up 4–6 bars"), snapped to downbeats.
5. **Recall** — mute the recording for a pass (play from memory): "next pass
   silent" or "every Nth pass silent". Surfaces `RepMode::RecallSilent` live;
   the ear-first move the category lacks.
6. **Count-in** (re-frame, **engine work, Phase 7**) — N clicks on the detected
   beat grid before the loop top.

## Existing surfaces this rides (verified 2026-06-15)

| Need | Mechanism | Location |
|------|-----------|----------|
| Active loop | `currentLoop` store; `selectLoop()` sets it + `loop.set` | `stores.ts:231,583` |
| Loop region (transient) | `loop.set {start,end}` → `EngineCmd::SetLoopSecs` | `stores.ts:579`, `app.rs:597`, `looper.rs:48` |
| Clear loop | `loop.clear` → `ClearLoop` | `app.rs:413` |
| Rate | `rate {value}` 0.25–2.0; `position.rate` | `stores.ts:539`, `app.rs:577`, `stretch.rs:42` |
| Mute (recall) | `mute {on}`; `muted` store | `stores.ts:561`, `app.rs:619`, `pipeline.rs:107` (advances position while muted) |
| Loop cycle event | `loop_wrapped` event (per crossfaded wrap) | `pipeline.rs:130`, `app.rs:946`, `stores.ts:981` |
| Curve→rate | `TempoCurve::rate_for_rep(rep)` (port to TS) | `tempo.rs:5` |
| Beat grid | `$openSong.analysis.{beats,downbeats,bpm}` | `stores.ts:93,143` |
| Snap helpers | `subdivisionTimes`, `snapToGrid` | `waveform-math.ts:110,127` |
| Box widget | `Box.svelte` (`label`, `grow`, `tools`, `children`) | `lib/ui/Box.svelte` |
| Stage box row | `.boxes` flex-wrap; conditional render | `App.svelte:134` |
| Key registration | `installKeys()`; free letters incl. `d` | `keys.ts:139` |

**No click/tone generation exists in the engine** — Phase 7 must add it.

---

## Phase 1 — Scratch-span foundation (frontend)

**Goal:** a `drillSpan` store that is the single source of truth for what's
looping while a loop is active; region toys edit it; saved loops untouched; the
waveform highlights it.

**Files:** `apps/desktop/src/lib/stores.ts`; new
`apps/desktop/src/lib/drill.ts` + `drill.test.ts`; `components/Waveform.svelte`.

- [ ] **1.1** Add `export const drillSpan = writable<{start:number; end:number} | null>(null);` to `stores.ts`. Seed it whenever `currentLoop` becomes non-null (in `selectLoop` and anywhere `currentLoop.set(...)` lands a real loop), clear it where `currentLoop` is cleared (`clearTransportLoop`, `openSong`, `resetWorkspace`).
- [ ] **1.2** New `lib/drill.ts` — pure region math (unit-tested, no Svelte): `nudgeEdge(span, edge, dir, gridTimes)`, `bisect(span, half)`, `runUp(span, bars, downbeats)`, all clamped to `[0, duration]` and snapped to provided grid times (fall back to a fixed seconds step when no grid). Mirror `waveform-math` snap semantics.
- [ ] **1.3** Actions in `stores.ts`: `drillNudge`, `drillIsolate`, `drillRunUp`, `drillResetSpan` — each computes the new span via `lib/drill.ts`, sets `drillSpan`, and calls `loop.set`. `drillResetSpan` restores from `currentLoop`'s bounds.
- [ ] **1.4** Waveform: render the active loop highlight from `drillSpan` (when set) rather than `currentLoop` bounds, so isolate/run-up are visible. Inspect current loop-region drawing first; keep one highlight, not two fighting regions.
- [ ] **Gate:** `cd apps/desktop && pnpm vitest run lib/drill.test.ts` green; `just lint` (svelte-check) clean. **Commit:** `feat(desktop/drill): scratch-span foundation (drillSpan store + region math)`.

## Phase 2 — Drill box shell + appearance (frontend)

**Goal:** the box appears/disappears correctly and sits in the stage as a wide box.

**Files:** new `components/Drill.svelte`; `App.svelte`.

- [ ] **2.1** `Drill.svelte` using `Box` (`label="drill"`, `grow`), rendered in `App.svelte`'s `.boxes` row gated on `$openSong && $currentLoop`. Header shows the active loop name + a "reset span" tool.
- [ ] **2.2** Wide-by-default: full row unless the stage is wide enough (CSS `flex-basis: 100%` with a min-width breakpoint, matching the `.boxes` flex-wrap idiom). Verify it drops below the waveform/transport and wraps siblings sensibly.
- [ ] **Gate:** `just lint` clean; empirical check (vite :5173 + chrome-devtools) that the box shows on loop-select and clears on reset/clear. **Commit:** `feat(desktop/drill): drill box shell, appears for the active loop`.

## Phase 3 — Step-up tempo trainer (frontend, the spine)

**Goal:** arm a ramp recipe; it autopilots the global rate across loop cycles.

**Files:** `lib/drill.ts` (+ test); `stores.ts`; `components/Drill.svelte`.

- [ ] **3.1** Port `TempoCurve::rate_for_rep` to `lib/drill.ts` as `rateForRep(curve, rep)` for `dwell`/`ladder`/`oscillate`, clamped `[0.25,2.0]`. Unit-test against the Rust semantics (ladder climbs to target; oscillate hits `high` every `period`th rep). Reuse the `TempoCurve` wire type already in `stores.ts:44`.
- [ ] **3.2** Trainer state in `stores.ts`: `drillTrainer = writable<{ recipe: TempoCurve; armed: boolean; cycle: number } | null>`. Default recipe `ladder {start:0.7, step:0.05, target:1.0}`. Actions: `armTrainer(recipe)`, `disarmTrainer()`, `resetRate()` (→ `setRate(1.0)`).
- [ ] **3.3** Drive it from the existing `loop_wrapped` event in `initEvents` (`stores.ts:981`): when `drillTrainer.armed`, increment `cycle`, compute `rateForRep(recipe, cycle)`, call `actions.setRate(...)`. On arm, immediately apply rep 0's rate. (No backend change — the event already fires per wrap.)
- [ ] **3.4** `Drill.svelte` UI: arm/disarm toggle; curve picker (dwell/ladder/oscillate); numeric fields for the recipe (reuse `lib/ui/NumberField`); live readout of current rate + cycle count; "reset rate" button. Use the theme accent for the armed/on state (`--accent`, per house style — no hardcoded cyan/amber).
- [ ] **Gate:** `pnpm vitest run lib/drill.test.ts` green; empirical: arm a ladder, watch the Transport slider climb each loop pass. **Commit:** `feat(desktop/drill): step-up tempo trainer autopilots the global rate`.

## Phase 4 — Region toys UI (frontend)

**Goal:** nudge / isolate / run-up / reset wired to Phase 1 actions, beat-snapped.

**Files:** `components/Drill.svelte`; possibly small icon SVGs (Transport icon convention — viewBox 24, CSS-sized, stroke ~2).

- [ ] **4.1** Buttons: nudge in/out start & end (±1 grid step), isolate first-half / second-half, run-up +1 bar / −1 bar, reset span. Disable grid-dependent affordances gracefully when no analysis (fall back to a fixed seconds nudge).
- [ ] **4.2** Show the scratch span's current bounds + length vs. the saved loop, so divergence is legible.
- [ ] **Gate:** `just lint` clean; empirical: isolate halves the region, run-up extends the start, reset snaps back; saved loop list unchanged. **Commit:** `feat(desktop/drill): beat-snapped nudge / isolate / run-up region toys`.

## Phase 5 — Recall (strip) (frontend)

**Goal:** play a pass from memory by muting the recording, manual or every-Nth.

**Files:** `stores.ts`; `components/Drill.svelte`.

- [ ] **5.1** State: `drillRecall = writable<{ everyN: number | null; armNext: boolean } | null>`. "Next pass silent" sets `armNext`; "every Nth" sets `everyN`.
- [ ] **5.2** Drive from `loop_wrapped`: at each wrap, decide mute for the upcoming pass (armNext, or `cycle % everyN === 0`) and call `actions.mute(...)`; unmute after. Coordinate with the trainer's cycle counter (share one wrap handler). The engine already advances position while muted (`pipeline.rs:181`), so the recall pass stays in time.
- [ ] **5.3** UI toggles + a clear indicator when the *next* pass will be silent.
- [ ] **Gate:** empirical: every-Nth drops the audio for one full pass then returns, loop stays in sync; `just lint` clean. **Commit:** `feat(desktop/drill): mute-to-recall passes (next / every-Nth)`.

## Phase 6 — Polish + integration pass (frontend)

- [ ] **6.1** Optional `d` key: focus/summon the drill box when a loop is active (guarded by `isTyping`). Add a Guide-tab blurb for the drill box.
- [ ] **6.2** Reset/teardown audit: arming a trainer then clearing the loop / resetting the workspace must disarm the trainer, clear recall, restore mute=false and rate=1.0 — no leaked state. Add this to `resetWorkspace` and the `currentLoop`-cleared paths.
- [ ] **6.3** Make sure switching the active loop reseeds `drillSpan` and resets trainer/recall cycle counters.
- [ ] **Gate:** `just check` (full test + lint) green. **Commit:** `feat(desktop/drill): key + guide entry; teardown audit`.

## Phase 7 — Count-in (re-frame) — ENGINE, stretch/final

**Goal:** N metronome clicks on the detected beat grid before the loop top.
**Risk:** no click/tone generation exists in the engine today; this is real RT
DSP + scheduling work. Ship only after 1–6 are solid; can be a separate effort.

**Files:** `crates/engine/src/` (new click source + count-in scheduling),
`crates/engine/src/pipeline.rs` (`EngineCmd`), `crates/server/src/app.rs` (new
`count_in`/`loop.set` flag), `stores.ts`, `components/Drill.svelte`.

- [ ] **7.1** Engine: a click generator (short windowed sine/noise burst, e.g. 1 kHz / ~25 ms with a fast decay; accent click for downbeats). Mix into the output post-looper, pre-master gain. Unit-test the rendered burst shape.
- [ ] **7.2** Engine: count-in scheduling — on (re)entering a loop, emit `count` clicks spaced at the loop's beat interval before the audio starts, derived from the loop start tempo. Decide the contract: clicks computed from `bpm`/`beats` passed in, vs. engine-internal. Prefer passing explicit click times (frontend has the beat grid) to keep engine dumb.
- [ ] **7.3** Command surface: extend loop entry (or a `count_in {beats, click_times}` command) so the trainer/box can request a count-in before a pass. `EngineCmd` variant + `app.rs` handler (cheap, no phasing).
- [ ] **7.4** `Drill.svelte` UI: count-in on/off + 1-bar / 2-bar selector; compute click times from `$openSong.analysis` beats nearest the loop top.
- [ ] **Gate:** `cargo test -p engine` green; manual playback confirms an in-time count before the loop; `just check` green. **Commit:** `feat(engine,desktop/drill): beat-grid count-in before the loop`.

---

## Execution order & status

1–6 first (frontend, no backend risk, each independently shippable), then 7
(engine). Commit per phase on `main`. `just check` is the gate before declaring
a phase done; pure-logic phases also run their colocated vitest file.

**STATUS (2026-06-15): Phases 1–6 COMPLETE and committed on `main`** — the
whole frontend feature (scratch span, box shell, tempo trainer, region toys,
recall, teardown/key/guide). `pnpm vitest` 126 pass (28 in `lib/drill.test.ts`),
`svelte-check` 0 errors. Backend untouched. Visual/interaction verification is
pending in `just dev` (the Tauri webview can't be driven headlessly here).

**Phase 7 (count-in) NOT STARTED** — it's the engine DSP work and was scoped as
the stretch/separate effort; awaiting a go-ahead.

## Self-review notes
- Every brainstorm decision maps to a phase: scratch span (1), appearance/wide
  box (2), trainer-as-spine borrowing global rate (3), nudge/isolate/run-up (4),
  recall (5), teardown/keys (6), count-in (7).
- Excluded by design: tracking/ratings/due ("B"), pitch/key re-framing,
  transient-selection-loop trigger (keyed on `currentLoop` only for v1 — note as
  a possible later extension).
- Backend untouched for 1–6 by deliberate reuse of `loop.set`/`rate`/`mute` +
  the `loop_wrapped` event; the trainer is a frontend autopilot over the one
  global rate, honoring the "no second source of truth" frontend principle.
