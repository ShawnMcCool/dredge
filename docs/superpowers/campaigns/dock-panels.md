# Campaign: Dock panels

Turn the right aside from a single tabbed view into a **dock**: a vertical stack
of **panels**, each holding multiple **tabs** (pages), where tabs can be dragged
to reorder, joined into another panel, or split off into a new one — with
resizable heights and no fixed limit on panels or tabs-per-panel. Designed
2026-06-26 (brainstorm + from-first-principles redesign). Work directly on
`main`. Frontend only.

> **For agentic workers:** phases are dependency-ordered. Each phase has a
> verification gate and a commit, and **each leaves the tree coherent and
> shippable** — never a half-wired state. The pure layout logic lives in a tested
> module (`lib/dock.ts`); the component only renders and reads drag gestures.

## Vocabulary (agreed)

- **dock** — the right aside as a whole (the container). Replaces the old loose
  use of "panel" for the entire aside.
- **panel** — one stackable unit inside the dock: a tab bar + the active tab's
  view, with a vertical size. The dock holds an ordered top→bottom list of these.
- **tab** — a page that lives in exactly one panel (structure / loops / routines
  / export / profile / devices / settings / guide).

(Updates the [[ui-vocabulary]] note once shipped: "Panel (right)" → the **dock**
of stacked **panels**, each with **tabs**.)

## What it is

The dock is a **partition of the page set into a vertical, ordered stack of
panels**. Each panel is an ordered list of tabs plus the one that's active.
Multiple panels' active views are visible at once, stacked. You can:

- **reorder** a tab within its panel's tab bar,
- **join** a tab into another panel (drop on that panel's tab bar),
- **split** a tab into a new panel by dropping it at any boundary — the top, the
  bottom, or the gap between two panels,
- **resize** panels by dragging the splitter between them.

Empty panels close automatically. There is no theoretical limit on panel count
or tabs per panel.

## The core idea (from first principles)

> Reorder, join, split, and close-empty are all the **same operation**: move a
> tab to a `(panel, position)` in the partition, then normalize (drop empty
> panels, fix each panel's active tab, renormalize weights). The dock is a
> `Panel[]`; a drag computes a target `(panel, position)` and applies that one
> move.

## Decisions (final)

- **One layout type, owned by the frontend, persisted.** Pure UI-layout state —
  no backend. A `panel_layout` setting in the settings table, same pattern as the
  other UI prefs in `stores.ts`.
- **`panel_layout` supersedes `tab_order`.** The flat `tab_order` shipped in
  `f314135` is the degenerate case of this — "the single panel's tab order." Two
  representations of "how the right aside is arranged" would be rot, so this
  campaign **replaces** `tab_order`: on load, an existing `panel_layout` wins;
  otherwise migrate `tab_order` → a one-panel layout and stop writing `tab_order`.
  The tab-reorder drag (`onTabDown`/`onTabMove` in `App.svelte`) is absorbed into
  the dock's within-panel reorder, not left as a parallel path.
- **Pure transforms in `lib/dock.ts`, gestures in the component.** Matches the
  codebase's "pure logic in `lib/*.ts` with colocated `*.test.ts`, views in
  `components/`" seam. The component never hand-rolls layout math.
- **Vertical-only.** Panels stack top→bottom; a panel's `weight` is its vertical
  share. Side-by-side (horizontal) splitting is a different, nested model and is
  **not** built or designed for here (no speculative nesting).
- **Split at any boundary; resize in the MVP.** Drop zones are every inter-panel
  gap plus the dock's top and bottom edge. Draggable splitters adjust weights
  from the start (stacked panels at fixed heights aren't useful).
- **Right dock only, but the type generalizes.** The left aside (library) stays a
  single pane this campaign. `DockLayout` is written generally so the left could
  adopt it later, but that is out of scope now.

## The model (`lib/dock.ts`)

```ts
type TabKey = (typeof ALL_TABS)[number];
interface Panel { tabs: TabKey[]; active: TabKey; weight: number } // weight = vertical share
type DockLayout = Panel[];                                          // top → bottom

// pure transforms — each returns a normalized layout (no empty panels, valid
// active per panel, weights summing sensibly):
reconcile(layout, allTabs): DockLayout   // exactly-once invariant; default when empty/invalid
fromTabOrder(order): DockLayout          // migration: one panel, that order
moveTab(layout, tab, toPanel, toIndex): DockLayout   // reorder or join; drops emptied source
splitTab(layout, tab, atBoundary): DockLayout        // insert new panel at boundary; drops emptied source
setActive(layout, panel, tab): DockLayout
setWeights(layout, weights): DockLayout
```

Invariants enforced by `reconcile` (and re-applied after every transform): every
known tab appears exactly once; no empty panels; `active ∈ tabs`; weights
normalized.

## Render

The dock is a vertical flex column. Each panel is a flex child with
`flex-grow: weight`, containing its tab bar (its tabs, the active one lit) + the
active tab's `<View />`. A resize splitter sits between adjacent panels. The
existing always-present collapse rail and the `.pane` scroll wrapper stay; the
single `{#key tab}` view becomes one view *per panel*.

## Phases

### Phase 1 — `lib/dock.ts` + persistence
The pure model and transforms with colocated vitest tests; `panel_layout`
setting + `panelLayout` store + `setPanelLayout` action + load (migrating from
`tab_order`, then ceasing to write it). No UI yet.
**Gate:** `dock.test.ts` covers reconcile / move / split / migrate / empty-drop /
weight-normalize; `pnpm vitest run` green.

### Phase 2 — Render the dock from the layout
Aside renders N panels from `panelLayout` (default = one panel, all tabs → today's
behavior), each with its tab bar + active view, weighted heights. Per-panel
active-tab switching on click.
**Gate:** a hand-authored 2-panel layout renders two stacked panels, each showing
its own active view; clicking a tab switches that panel only; smoke-test in
vite+chrome, no effect loop.

### Phase 3 — Within-panel reorder
Port the FLIP tab-drag, scoped per panel; drop within the same tab bar reorders.
Persists.
**Gate:** drag reorders within a panel; plain click still selects; order persists.

### Phase 4 — Join (cross-panel move)
Dropping a dragged tab on another panel's tab bar moves it there (`moveTab`), with
a drop-target highlight. Source panel closes if emptied.
**Gate:** a tab dragged onto another panel appears there and leaves the source;
emptied source panel disappears; persists.

### Phase 5 — Split (new panel at any boundary)
Drop zones at the dock top/bottom and every inter-panel gap insert a new panel via
`splitTab`. Drop indicator shows where the split lands.
**Gate:** dragging a tab to a boundary creates a new stacked panel there; dragging
the last tab out of a panel removes it; persists across reload.

### Phase 6 — Vertical resize
Draggable splitters between panels adjust `weight`s (smooth), persisted.
**Gate:** dragging a splitter resizes neighbors; weights persist; min-height keeps
a panel's tab bar usable.

## Surfaces this rides (verified 2026-06-26)

| Need | Mechanism | Location |
|------|-----------|----------|
| Page set + view map | `ALL_TABS`, `TAB_VIEWS` registry | `App.svelte:40,43` |
| Current selected tab | `let tab = $state(...)` | `App.svelte:53` |
| Tab reorder (to absorb) | `onTabDown`/`onTabMove`, `orderedTabs`/`shownTabs`, `animate:flip` | `App.svelte:79,61,67,229` |
| Flat order (to supersede) | `tabOrder` store, `TAB_ORDER` key, `setTabOrder`, load | `stores.ts:416,441,664,566` |
| Settings persistence | `setSetting(key,value)`; `loadSettings` | `stores.ts:639,561` |
| Dock container + scroll wrapper | `.panels` aside, `.pane`, `.rail` (flex row) | `App.svelte:214,216,245` |
| Reorder drag pattern | pointer + `elementFromPoint` + FLIP (built for tabs) | `App.svelte` (this campaign generalizes it) |
| Pure-logic seam | `lib/*.ts` + colocated `*.test.ts` (e.g. `waveform-math`) | `lib/` |

## Deferred

Horizontal (side-by-side) splitting and nested docks; applying the dock model to
the left/library aside; per-panel collapse; tear-off to a separate window; tab
overflow/scroll when a panel holds many tabs (handle if it bites). No backend
work in any phase.
