# Working Loop — Design

**Date:** 2026-06-15
**Status:** Approved, implementing

## Problem

Today the "loop" gesture immediately persists a `LoopRegion` to SQLite: drag a
selection → ⟳ **loop** runs `loop.create`, selects it, and plays. Every drag you
want to try out becomes a saved row. There's no way to spin up a live, drillable
loop and decide *later* whether it's worth keeping.

This inverts the recent "loop = save the selection" decision: "loop" should
create a **working loop** — fully active and drillable, with all loop
properties — that only becomes a saved `LoopRegion` when you explicitly hit a
**save** icon.

## Decisions (from brainstorming)

- **Lifecycle:** at most **one** working loop. A new selection + loop silently
  replaces it (drill tweaks discarded). **Revised (post-build):** an established
  loop — working *or* saved — is **sticky**: a plain left-click and drawing a new
  selection box never clear it. It is only replaced via the loop icon (new
  selection → loop), or discarded via the ✕ on its hover cluster (working) / the
  reset-workspace ⟲ (either). Clicking a *visible* saved loop's body still
  establishes that one as active.
- **Look:** the working loop renders **solid, identical to a selected saved
  loop** (no dashed/provisional styling) — unsaved-ness is signalled only by the
  💾 save glyph in its hover cluster.
- **Resize:** the working loop is **right-drag resizable** like saved loops; the
  resized bounds are what save persists. Resizing follows drill home/scratch to
  the new bounds *without* a teardown (trainer/tools survive).
- **Save bounds:** save persists the working loop's **home bounds** (what "reset
  span" snaps back to), not transient drill reshaping (isolate half, run-up).
- **Save affordance:** lives **on the waveform region** (not the drill box).
- **"All loops visible":** the waveform renders **only the active loop** by
  default. A **persisted** toggle in the Loops tab brings the rest back as a dim
  overlay. Default OFF.

## Architecture (approach A — two client stores + derived read surface)

The whole feature is **frontend-only**. No schema/migration, no Rust changes.
The "all loops visible" flag rides the existing generic
`settings.get_all` / `settings.set` key-value store, like `gridSnap`.

### State (`apps/desktop/src/lib/stores.ts`, all client-side)

- New `workingLoop: Writable<Span | null>` — an unsaved, active loop. Mutually
  exclusive with `currentLoop: LoopRegion | null` (the active loop is either a
  saved DB row or a working span). A working loop is just another client-only
  span, like `selection` and `drillSpan` already are — `LoopRegion` keeps
  meaning "a real persisted row."
- New derived `activeLoop: Readable<{ id: number | null; start; end; name } | null>`
  = normalized `workingLoop ?? currentLoop`. The drill box, waveform region, and
  transport read **this**, so no consumer branches on saved-vs-working. `id ===
  null` ⇒ unsaved/working.
- New `allLoopsVisible: Writable<boolean>` + `ALL_LOOPS_VISIBLE` settings key.
  Applied in `loadSettings()`, written through `setSetting()`. Default `false`.

### Flow

1. Drag selection → ⟳ **loop** sets `workingLoop` (no `loop.create`), seeds
   drill, seeks + plays. (Replaces today's `saveAndSelectLoop`/`loopSelection`
   immediate-create path.)
2. Drill freely — existing scratch behavior (`drillSpan`, trainer, recall);
   nothing persisted.
3. 💾 **save** → `loop.create` with the working loop's **home bounds** → set
   `currentLoop`, clear `workingLoop`. Bounds are unchanged, so the promotion is
   metadata-only (gains id + name) and **`seedDrill` is NOT re-run** — an armed
   trainer / recall survive the save.
4. New selection + loop silently replaces `workingLoop`; clicking away dismisses
   it (existing `clearTransportLoop` path, extended to also clear
   `workingLoop`).

### Waveform rendering (`Waveform.svelte`)

- The draw loop (currently renders every loop region) renders **only
  `activeLoop`** by default. When `allLoopsVisible` is on, render all loops as a
  dim overlay with the active one still bold.
- A working loop's region renders **solid, like a selected saved loop** (revised
  from the original dashed/provisional idea — unsaved-ness shows only via the 💾
  glyph). While right-drag resizing it, the preview bounds win.
- The region's HoverActions cluster is three-state:
  - raw selection → ⟳ **loop**
  - working loop → 💾 **save** (SVG floppy, per the icon convention), ⟳
    **play**, ✕ **discard**
  - saved loop → ⟳ **play**, ✕ **delete** (unchanged). No "saved" glyph — the
    solid-vs-dashed region already carries that, and the *absence* of a save
    button is the signal it's saved.
- The active loop is seeded into the drill by a **bounds-keyed** subscription on
  `activeLoop`, not identity — so promoting a working loop to a saved one (same
  bounds, gains id+name) does not reseed, and an armed trainer / recall survive
  the save. `saveWorkingLoop` sets `currentLoop` *before* clearing `workingLoop`
  so `activeLoop`'s bounds never momentarily go null.
- Working loops are right-drag resizable on the waveform (via
  `setWorkingLoopBounds`), in addition to the drill region tools. The resize
  hit-test (`nearestResizeTarget`) considers the working loop's edges alongside
  saved loops and grabs whichever is nearest.

### Loops tab (`Loops.svelte`)

- Add an **"all loops visible"** toggle button (mirrors the grid toggles'
  style), wired to a new `setAllLoopsVisible()` action.

## Out of scope

- No schema change / migration; no Rust changes.
- Junction loops untouched.
- Saved-loop drill-edit persistence unchanged (drilling a *saved* loop still
  doesn't rewrite its bounds).

## Test strategy

- Vitest for any new pure logic (e.g. the `activeLoop` normalization if it lands
  in a `*.ts` helper).
- Empirical UI check via `just dev` (vite :5173) + chrome-devtools: loop a
  selection → confirm no row appears in the Loops tab until 💾; confirm save
  promotes without resetting an armed trainer; confirm the waveform shows only
  the active loop until the toggle is on.
- `just check` (cargo test + lint + svelte-check) green before commit.
