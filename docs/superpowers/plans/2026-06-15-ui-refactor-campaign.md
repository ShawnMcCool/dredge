# UI Refactor Campaign Implementation Plan

> **For agentic workers:** Execute task-by-task. Steps use checkbox (`- [ ]`) syntax. After every phase: run `pnpm vitest run` + `pnpm svelte-check --threshold error` from `apps/desktop`, then commit on `main`.

**Goal:** Bring the Earworm desktop frontend to "designed-from-scratch" quality by completing the half-built widget kit, breaking up the two god-files (`stores.ts`, `Waveform.svelte`), and sealing remaining seams — without changing behavior.

**Architecture:** The dispatch-mirror architecture stays. Work is additive-then-retrofit: build missing primitives first, then retrofit call sites; split god-modules behind unchanged public imports (barrel re-exports) so consuming components don't change; decompose the waveform into pure logic + layers. Every phase keeps vitest + svelte-check green.

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest, Tauri. pnpm under `apps/desktop`.

**Verification baseline (must stay true after every phase):** `pnpm vitest run` → all pass; `pnpm svelte-check --threshold error` → 0 errors.

**Conventions:** Commit directly on `main` (project rule). One commit per task unless noted. New pure logic gets a colocated `*.test.ts`.

---

## Phase A — Complete the widget kit, then retrofit

Highest leverage, lowest risk. Build each primitive with a test where it has logic, retrofit call sites, verify, commit.

### Task A1: `lib/format.ts` — duration formatting

**Files:**
- Create: `apps/desktop/src/lib/format.ts`
- Create: `apps/desktop/src/lib/format.test.ts`
- Modify: `apps/desktop/src/components/Library.svelte` (replace local `fmtDur`)
- Modify: `apps/desktop/src/components/Loops.svelte`, `DuePanel.svelte` (any local duration formatting)

- [ ] Write `fmtDur(seconds: number): string` (m:ss) + `fmtClock` if needed; test edge cases (0, <60, >=60, rounding, negative guard).
- [ ] Run `pnpm vitest run lib/format.test.ts` — expect pass.
- [ ] Replace the inline duration formatters in components with `import { fmtDur } from "../lib/format"`.
- [ ] Verify: `pnpm vitest run` + `pnpm svelte-check --threshold error`.
- [ ] Commit: `refactor(desktop): extract lib/format.ts, dedupe duration formatting`.

### Task A2: `lib/ui/EmptyState.svelte`

**Files:**
- Create: `apps/desktop/src/lib/ui/EmptyState.svelte`
- Modify: `Loops.svelte`, `PlanBuilder.svelte`, `Capture.svelte`, `DuePanel.svelte`, `ProfilingPanel.svelte`, `Sections.svelte` (CTA variant)

- [ ] Props: `{ title?: string; children?: Snippet; action?: Snippet }`. Compact (`font-size: 11px; color: var(--muted)`) base; optional title + action slot for the CTA shape currently in `Sections.svelte`.
- [ ] Retrofit the 6 ad-hoc empty/cta blocks to use it.
- [ ] Verify + commit: `feat(desktop/ui): EmptyState primitive; retrofit empty/cta blocks`.

### Task A3: `lib/ui/ListRow.svelte`

**Files:**
- Create: `apps/desktop/src/lib/ui/ListRow.svelte`
- Modify: `Loops.svelte`, `Sections.svelte`, `PlanBuilder.svelte` (step row), `ProfilingPanel.svelte`, `Capture.svelte`

- [ ] Props: `{ active?: boolean; current?: boolean; suggested?: boolean; onclick?; hoverActions?: Snippet; children: Snippet }`. Flex row + gap + left-border state colors + hover-reveal actions (opacity 0→1) consolidated from `Sections.svelte`'s row CSS.
- [ ] Retrofit row markup in the 5 components. Preserve each component's existing visual states (selected loop highlight, suggested-section styling).
- [ ] Verify (svelte-check + vitest) and **run the app** (see Phase E verify) to eyeball the lists.
- [ ] Commit: `feat(desktop/ui): ListRow primitive; retrofit list rows`.

### Task A4: `lib/ui/Field.svelte`

**Files:**
- Create: `apps/desktop/src/lib/ui/Field.svelte`
- Modify: `PlanBuilder.svelte` (~20 label+input pairs), `Loops.svelte` (time inputs), `Sections.svelte` (edit-mode inputs)

- [ ] Props: `{ label: string; value; type?: "number"|"text"; step?; min?; max?; width?; oninput?; onchange? }` rendering `<label>{label} <input .../></label>` with the shared `.fields` styling. Support `bind:value` via `$bindable`.
- [ ] Retrofit PlanBuilder form fields first (biggest win), then Loops/Sections inputs.
- [ ] Verify + commit: `feat(desktop/ui): Field primitive; retrofit form inputs`.

### Task A5: `lib/use-async-action.ts` — error/busy helper

**Files:**
- Create: `apps/desktop/src/lib/use-async-action.ts`
- Create: `apps/desktop/src/lib/use-async-action.test.ts`
- Modify: `Capture.svelte`, `PlanBuilder.svelte`, `Tuner.svelte`, `Sections.svelte`

- [ ] Provide a small helper that wraps an async fn and exposes `{ error, busy, run }` (Svelte 5 `$state` class or factory). Normalizes `e instanceof Error ? e.message : String(e)`. Unit-test the error-normalization + busy-toggle logic (pure parts).
- [ ] Retrofit the 4 components' hand-rolled `try/catch → error` blocks. Standardize the error class name (`Tuner` uses `.err` → `.error`).
- [ ] Verify + commit: `refactor(desktop): shared async-action error/busy helper`.

---

## Phase B — Split `stores.ts` (1064 lines) behind an unchanged barrel

Keep `lib/stores.ts` as a re-export barrel so **no component import changes**. Move each domain to `lib/stores/<domain>.ts`. Extract the two orchestrators as testable units.

**Files:**
- Create: `apps/desktop/src/lib/stores/` modules: `settings.ts`, `song.ts`, `playback.ts`, `annotations.ts` (loops+sections), `planning.ts`, `prepare.ts`, `capture.ts`, `tuner.ts`, `stems.ts`, `profiling.ts`, plus `wire.ts` (shared wire types) and `core.ts` (event init + launch restore).
- Create: `apps/desktop/src/lib/stores/prepare-orchestrator.ts` (+ test), `apps/desktop/src/lib/stores/plan-session.ts` (+ test).
- Modify: `apps/desktop/src/lib/stores.ts` → thin barrel re-exporting `stores`, `actions`, all wire types, and calling `initEvents()`.

- [ ] Extract wire types to `wire.ts`; re-export from barrel. Verify green.
- [ ] Move leaf domains with no cross-domain logic first (`profiling`, `capture`, `tuner`, `stems`, `settings`). Each: move stores + actions, re-export, verify, commit.
- [ ] Move `song`, `playback`, `annotations`. Verify, commit.
- [ ] Extract `PrepareOrchestrator` (the `prepare()` state machine + waiters) into a class with unit tests for state transitions; wire `prepare`/`closePrepare`/`loadAnalysis`/`reanalyze` to it. Verify, commit.
- [ ] Extract `PlanSession` (`dueAtPlanStart`/`repsThisPlan`/`stepsThisPlan` + rating logic) into a class with unit tests; wire planning actions to it. Verify, commit.
- [ ] Reduce `stores.ts` to barrel + `initEvents`. Run full `pnpm vitest run` (the mirror tests exercise actions) + svelte-check. Commit: `refactor(desktop): split stores into per-domain modules behind barrel`.

**Guardrail:** existing `lib/*.test.ts` (library, profiles, livesample) import from `./stores` and mock `cmd()` — they must keep passing unchanged. That is the regression net for this phase.

---

## Phase C — Decompose `Waveform.svelte` (962 lines)

Extract pure logic to `lib/` (testable), split rendering into layer functions, unify the duplicated canvas/scrollbar navigation, cache CSS lookups. **Local view `$state` and the `workspaceReset` nonce contract stay exactly as-is.**

**Files:**
- Create: `apps/desktop/src/lib/waveform-hit.ts` (+ test) — pure hit detection: `hitLoopBody`, `hitLoopEdge`, `nearestLoopEdge`, `hitLaneSpan`, `spanAtTime` as pure fns taking `(x,y,view,loops/sections,consts)`.
- Create: `apps/desktop/src/lib/waveform-layers.ts` — `drawWaveform`, `drawGrid`, `drawLoops`, `drawSelection`, `drawZoomPreview`, `drawStructureLane`, `drawPlayhead`, each `(ctx, params) => void`.
- Create: `apps/desktop/src/lib/waveform-nav.ts` (+ test) — unify pan/zoom/edge-drag math shared by canvas wheel and scrollbar (wraps existing `adjustWindow`/`zoom`).
- Modify: `apps/desktop/src/components/Waveform.svelte` — consume the above; cache `getComputedStyle` color reads once per draw (not ~20×/frame); keep gesture wiring + local state.

- [ ] Extract hit detection to `waveform-hit.ts` with unit tests; replace component fns. Verify, commit.
- [ ] Extract draw layers to `waveform-layers.ts`; `draw()` becomes orchestration calling layers with a cached color palette. Verify (svelte-check + **run app**, confirm waveform/loops/grid/playhead render). Commit.
- [ ] Extract nav math to `waveform-nav.ts` with tests; route both canvas wheel/drag and scrollbar through it. Verify + **run app** (zoom via wheel, pan, scrollbar drag, edge-resize). Commit.

**Guardrail:** this is interaction-critical and weakly unit-tested at the component level — empirical app verification (vite + chrome-devtools) is mandatory at each step, not optional.

---

## Phase D — Polish

### Task D1: Tab registry in `App.svelte`
- [ ] Replace the 8-branch `{#if tab === ...}` chain + the per-store `$effect` tab-setters with a `Record<Tab, Component>` map and a single `setTab(t)` action; render via `{#key tab}<svelte:component this={TAB_COMPONENTS[tab]} />{/key}`. Verify + commit.

### Task D2: Transport sub-components
- [ ] Extract `PlayButton`, `VolumeControl`, `BassFocusToggle`, `SpeedControl`, `PitchControl` into `apps/desktop/src/components/transport/`; `Transport.svelte` becomes layout + composition (~60 lines). Verify (svelte-check + **run app**, exercise each control). Commit.

### Task D3: Seal the file-picker seam
- [ ] Create `apps/desktop/src/lib/file-picker.ts` exporting `async function pickAudioFile(): Promise<string | null>` wrapping `@tauri-apps/plugin-dialog`. Replace the direct import in `Library.svelte`. Verify + commit.

### Task D4: Docs
- [ ] Update `CLAUDE.md`: the structure box is `Sections.svelte` (remove the stale `Analysis.svelte` reference), note the new `lib/ui/` primitives (Box, Button, Fader, Modal, Group, Toolbar, HoverActions, EmptyState, ListRow, Field) and `lib/stores/` layout. Commit.

---

## Phase E — Final verification

- [ ] `pnpm vitest run` (all pass) + `pnpm svelte-check --threshold error` (0 errors).
- [ ] `cargo build` is untouched, but run `just lint` once (clippy + fmt + svelte-check) to confirm the whole gate is green.
- [ ] Run the app (`just dev` or vite :5173 + chrome-devtools): open a song, exercise transport, stems, structure/loops/plan tabs, capture, tuner, waveform zoom/loop-create. Confirm no regressions and no console errors.
- [ ] Final commit if any doc/cleanup remains.

---

## Self-review notes
- Public import surface (`./lib/stores`, `./lib/ui/*`) is preserved by barrels/additive files → retrofits are localized.
- Each phase is independently shippable and leaves the suite green.
- Pure logic extracted (`format`, `use-async-action`, `waveform-hit`, `waveform-nav`, orchestrators) gets unit tests — this is the net for the weakly-tested component layer.
