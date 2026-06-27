# Workspace unification — left dock + the window as one arrangement

Designed 2026-06-27 (brainstorm + from-first-principles redesign). Frontend only.
Work directly on `main`.

Extends the [dock-panels campaign](../campaigns/dock-panels.md), which built the
right aside into a dock and **deferred** "applying the dock model to the
left/library aside." This spec picks that up and goes one step further: the left
and right asides become **two regions of one workspace**, and tabs flow freely
between them.

## Core idea

> The window's arrangement is a single value — a **workspace** of **regions**,
> each region a **dock** of **panels**, each panel a stack of **tabs** — and every
> view (the library included) is just a tab that lives somewhere in it.

The library stops being a hand-rolled aside and becomes an instance of the same
thing the right dock already is. "Which tabs live where, which are collapsed, how
the stacks are weighted" becomes one source of truth instead of three scattered
ones (`panel_layout` + `library_collapsed` + `panels_collapsed`).

## Vocabulary

Carries the dock-panels terms and adds two:

- **workspace** — the whole window arrangement: its regions. One persisted value.
- **region** — a dockable edge of the window (`left`, `right`). A region owns a
  `DockLayout` (its vertical stack of panels) plus its `collapsed` flag.
- **dock**, **panel**, **tab** — unchanged from the dock-panels campaign. A region
  *is* a dock; the difference is that there are now two of them and tabs cross
  between.

The **stage** (center) is not a region in this slice — it stays the fixed work
surface with its flowing control boxes. The model is written so the stage *could*
become a region later, but that is **not built and not scaffolded for** here.

## The model (`lib/dock.ts`)

The single-`DockLayout` layer from dock-panels is unchanged and stays the building
block. A workspace layer is added on top.

```ts
// existing (unchanged):
interface Panel { tabs: string[]; active: string; weight: number }
type DockLayout = Panel[];                       // one region's stack, top → bottom

// new:
type RegionId = "left" | "right";
interface Region { layout: DockLayout; collapsed: boolean }
type Workspace = { left: Region; right: Region };
```

`Workspace` is an explicit two-field shape, not an open `Record` — there are
exactly two regions now and inventing N-region generality would be speculative.
The drag coordinator iterates `["left", "right"]` so adding a region later is a
small, honest change rather than a designed-in abstraction.

### Workspace transforms (pure, in `dock.ts`)

Built on the existing single-layout helpers (`moveTab`, `splitTab`, `setActive`,
`setWeights`, `normalize`). A tab's current region is found by scanning; a move
removes it from wherever it is and applies the target-region transform.

```ts
defaultWorkspace(allTabs): Workspace
  // library → left (one panel); the rest → right (one panel). The first-run shape.

reconcileWorkspace(ws, allTabs): Workspace
  // exactly-once across BOTH regions: keep first occurrence, drop unknown tabs,
  // append tabs new-in-code to right's last panel, normalize each region.
  // Each region independently non-empty is NOT required — an empty region is
  // legal (its dock renders nothing; only the rail shows).

moveTabTo(ws, tab, toRegion, toPanel, toIndex): Workspace
  // reorder / join / cross-region move. Within-region is the case toRegion ==
  // source region. Removes the tab from its source region first.

splitTabTo(ws, tab, toRegion, atBoundary): Workspace
  // new panel at a boundary in toRegion (cross-region split included).

setActiveIn(ws, region, panel, tab): Workspace
setWeightsIn(ws, region, weights): Workspace
setCollapsed(ws, region, collapsed): Workspace
```

Within-region drags reduce to `moveTabTo` / `splitTabTo` with the source region as
target, so there is one move operation, not two parallel ones — same principle the
dock-panels campaign established ("reorder, join, split, close-empty are all one
move").

**Empty-region note.** dock-panels' `normalize`/`reconcile` guaranteed a
non-empty single layout (a dock always had ≥1 tab). With two regions, dragging the
last tab out of a region must leave that region empty rather than snapping a tab
back. So region-level reconcile allows an empty `layout`; the invariant
"exactly-once" is enforced at the **workspace** level (across both regions), not
per region. `dock.test.ts` gains cases for this.

## Persistence & migration (frontend only)

Settings are JSON-per-key in the `settings` table, mirrored and migrated by the
frontend store at load — no Rust schema change.

- New key `workspace` (`WORKSPACE` const) holds the whole `Workspace`. Supersedes
  `panel_layout`, `library_collapsed`, `panels_collapsed`, which are **no longer
  written**.
- On load (`loadSettings`):
  1. If `workspace` is present → `reconcileWorkspace(it, ALL_TABS)`.
  2. Else migrate: `right.layout` from legacy `panel_layout` (or its own
     `tab_order` migration, already handled by `reconcile`); `left.layout` seeded
     with `library`; `collapsed` flags carried from `library_collapsed` /
     `panels_collapsed`; then `reconcileWorkspace`.
  3. Else → `defaultWorkspace(ALL_TABS)`.
- Every workspace edit writes the `workspace` setting through, same
  write-through pattern as `setPanelLayout` today.

This mirrors how dock-panels superseded `tab_order` — one representation of "how
the window is arranged," old keys retired, no parallel paths.

## Drag coordinator (`lib/dock-drag.svelte.ts`)

The one real refactor. Today `Dock.svelte`'s drag is scoped to its own root
(`dockEl.contains(el)`), which is correct for one dock but cannot resolve a drop
in a *sibling* dock. A cross-region move spans two `DockLayout`s, so the brain that
resolves a drop must see every region at once.

A rune-module coordinator owns that:

- Holds the active drag (`tab`, `sourceRegion`), the current `drop`
  (`{ region, kind: "tab" | "split", … }`) and `caret`.
- Each region **registers its root element** (`register(region, el)` on mount,
  unregister on destroy).
- On pointer move it hit-tests across **all** registered roots
  (`document.elementFromPoint` → which region's root contains it → tab-bar vs
  panel-body, exactly the dock-panels logic, now region-tagged).
- On pointer up it applies the workspace transform and calls a single
  `onchange(workspace)`.
- `reveal(tab)` — find the tab's region, clear its `collapsed`, `setActiveIn` it.
  Drives the existing settings / structure / loops shortcuts.

The coordinator is created in `App.svelte` from the `workspace` store + write-back
action and provided via Svelte context; both regions consume it. App stays the
composition root.

Pure layout math stays in `dock.ts`; the coordinator is the stateful gesture brain
(it imports the pure transforms). This keeps the dock-panels seam — pure logic in
tested `lib/*.ts`, gestures in the component layer — intact across the refactor.

## Components

- **`Dock.svelte`** — becomes a pure per-region renderer: panels from
  `region.layout`, tab bars, the `{#key active}` view per panel. It forwards
  pointer events to the coordinator and reads the coordinator's `drop`/`caret` to
  draw its own drop affordances; it no longer owns drag state. Within-region
  resize (splitters / right-drag-snap) stays here — it never crosses regions.
- **`DockRegion.svelte`** (new) — a region's shell: the always-present collapse
  rail + chevron, and the `Dock` when expanded. A `side: "left" | "right"` prop
  places the rail on the outer edge and points the chevron. This collapses the two
  near-identical rail/pane/collapse blocks in `App.svelte` into one component.
- **`App.svelte`** — reduces to the grid plus three children:
  `<DockRegion side="left"/>`, `<main class="stage">…</main>`,
  `<DockRegion side="right"/>`. The hand-rolled `.library` aside and the inline
  `<Dock>` wiring are deleted. `library` is added to `ALL_TABS` / `TAB_VIEWS`
  (`Library` is its view). The `settingsOpen` / `sectionsOpen` / `loopsOpen`
  effects route through `coordinator.reveal`.

## stores.ts

- Replace `panelLayout` / `libraryCollapsed` / `panelsCollapsed` writables with a
  single `workspace` writable.
- Replace `setPanelLayout` with the coordinator's write-back (`setWorkspace`).
- `toggleLibrary` / `togglePanels` → `toggleRegion("left" | "right")`
  (`setCollapsed` through the store). Keybindings Ctrl+[ / Ctrl+] keep their
  meaning (toggle left / right collapse).
- `PANEL_LAYOUT` / `LIBRARY_COLLAPSED` / `PANELS_COLLAPSED` consts retired from the
  write path; their *read* is kept only inside the one-time migration in
  `loadSettings`.

## Diff against the code — dispositions

| Existing | Disposition |
|---|---|
| `panel_layout` setting (`DockLayout`) | → `workspace.right.layout`; migrated then retired. **Fix now.** |
| `library_collapsed` / `panels_collapsed` settings | → `workspace.{left,right}.collapsed`; migrated then retired. **Fix now.** |
| `Dock.svelte` self-contained drag | Lifted to the coordinator so drops cross regions. **Fix now** (the slice's main cost). |
| Two hand-rolled rail/pane blocks in `App.svelte` | Unified into `DockRegion.svelte`. **Fix now.** |
| `library` not a tab | Added to `ALL_TABS` / `TAB_VIEWS`. **Fix now.** |
| Stage control boxes | A different layout idea (wrap-flow, not a resizable stack). **Scheduled — later campaign step.** Designed-for (a region could host them), not built. |

No silent orphans: the three retired settings are read once during migration and
never written again; the stage-boxes deferral is the only scheduled convergence
point and it is named.

## Honest cost

The bolt-on (a second independent `<Dock>` on the left with its own
`left_panel_layout`, no cross-dock drag) is roughly a quarter of this. It is
rejected: it makes the unified pool impossible and leaves arrangement state split
across parallel representations — exactly the rot this redesign exists to prevent.
The coherent path touches `dock.ts`, `Dock.svelte`, new `DockRegion.svelte`, new
`dock-drag.svelte.ts`, `App.svelte`, `stores.ts`. No backend.

## Phases

Each phase leaves the tree coherent and shippable, with a verification gate and a
commit. Frontend smoke-test (vite + chrome) on any phase that touches rendering or
effects, per the ui-runtime-smoke-test discipline.

### Phase 1 — workspace model + transforms (`dock.ts`)
`Region` / `Workspace` types; `defaultWorkspace`, `reconcileWorkspace`,
`moveTabTo`, `splitTabTo`, `setActiveIn`, `setWeightsIn`, `setCollapsed`, built on
the existing single-layout helpers. Empty-region handling.
**Gate:** `dock.test.ts` extended — cross-region move/split, exactly-once across
both regions, last-tab-out leaves a region empty, migration shape, weight
normalize per region. `pnpm vitest run` green.

### Phase 2 — persistence + migration (`stores.ts`)
`workspace` store + key, write-through, and the load-time migration from
`panel_layout` / `library_collapsed` / `panels_collapsed`. Old keys cease to be
written. No UI change yet (existing components still read the old stores — keep
them building by deriving the old stores from `workspace` *temporarily within this
phase only* if needed, or land Phases 2–4 together if the seam is cleaner that
way).
**Gate:** unit test the migration (legacy keys → expected workspace); `vitest`
green; app still launches with prior layout restored.

### Phase 3 — drag coordinator (`dock-drag.svelte.ts`)
The rune-module coordinator: registration, cross-root hit-testing, `drop`/`caret`,
`reveal`, `onchange(workspace)`. Pure unit coverage where the logic is testable
(hit-test resolution given mock rects is hard to unit-test; cover the transform
selection and `reveal`).
**Gate:** `vitest` green for the testable surface; no runtime use yet.

### Phase 4 — `Dock.svelte` as region renderer + `DockRegion.svelte`
Refit `Dock` to render one region and delegate drag to the coordinator; add
`DockRegion` (rail + collapse + Dock). Right aside switches to
`<DockRegion side="right"/>` driven by `workspace.right`. Behavior identical to
today's right dock.
**Gate:** right dock reorder / join / split / resize / collapse all still work and
persist; smoke-test, no effect loop.

### Phase 5 — left region live + cross-region drag
`App.svelte` mounts `<DockRegion side="left"/>` with `library` as a tab; delete
the hand-rolled `.library` aside; route reveal shortcuts through the coordinator.
Cross-region drag now resolves (the coordinator already sees both roots).
**Gate:** library renders in the left dock; a tab dragged left↔right lands and
persists; Ctrl+[ / Ctrl+] collapse each side; an emptied region collapses to its
rail; full reload restores the arrangement; smoke-test, no effect loop.
**This is the convergence point** — at its commit the three legacy settings are
fully superseded and the workspace is the single source of truth.

## Deferred (scheduled convergence points)

- **Stage control boxes as a region** — the named later campaign step. Not built,
  not scaffolded.
- A third/floating region, tear-off windows, horizontal nesting — out of scope,
  not designed-for (would be speculative).
