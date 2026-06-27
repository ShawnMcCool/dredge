# Stage flow region — managed, reorderable control boxes

Designed 2026-06-27 (brainstorm + from-first-principles redesign). Frontend only.
Work directly on `main`.

The follow-on to the [workspace unification](2026-06-27-workspace-unification-design.md),
which widened the window into left/right dock regions and **deferred** "stage
control boxes as a region." This spec picks that up — but deliberately does *not*
turn the boxes into dock tabs.

## Pages vs. tools — why the stage is a different region kind

The dock holds **pages**: structure, loops, export, settings, library — one-of-N
visible (tabs), persistent, reference/config surfaces.

The stage holds **tools**: isolation, notes, tuner, drill, metronome, click,
recordings. They differ on three axes that make tab-ifying them wrong:

- **Visibility** — tools are *all visible at once* (watch the tuner *while*
  reading notes *while* the drill counts). Tabs hide things; that defeats a live
  tool.
- **Presence** — tools are *contextual*: drill only mid-drill, the song-scoped
  tools only with a song open. Pages are always present.
- **Arrangement** — tools wrap-flow horizontally; pages stack vertically.

So the stage is a **flow region**: an ordered list of always-visible boxes, each
collapsible to its header. A different *kind* of region from a dock, sharing only
the part that genuinely generalizes — a persisted arrangement (order + collapse)
reconciled against a known set, with drag-to-reorder.

The **waveform + transport are the fixed head of the stage** and are *not* in the
flow region (the play controls read against the wave, so they move with it). The
flow region is everything beneath the transport.

## Core idea

> The window arrangement is one workspace of regions of two kinds: **dock
> regions** (tabbed stacks — left/right) and a **flow region** (all-visible,
> reorderable, collapsible boxes — the stage). Every labelled surface — dock
> panel, stage box, in-page group — draws its header from one primitive.

## Model

`Workspace` widens from two dock regions to two docks plus the stage flow:

```ts
// dock region — today's Region, renamed for clarity:
interface DockRegion { layout: Panel[]; collapsed: boolean }
// flow region — the stage:
interface FlowRegion { order: BoxId[]; collapsed: BoxId[] }  // collapsed = a set, stored as an array for JSON
type Workspace = { left: DockRegion; right: DockRegion; stage: FlowRegion };
```

`BoxId` is the stable id of a stage tool: `"metronome" | "isolation" | "click" |
"notes" | "recordings" | "tuner" | "drill"`. The canonical default order is that
list (matching today's markup order).

### Presence is orthogonal to order

A box's *presence* is state-driven and unchanged from today:

| Box | present when |
|---|---|
| metronome, tuner | always |
| isolation, click, notes, recordings | a song is open |
| drill | a song is open **and** a drill span is active |

`FlowRegion.order` is a stable ordering over *all* boxes; the render filters to the
present ones, in saved order. A box new-in-code (not yet in a saved order) appends
at the end — the same `reconcile` idea the dock uses (known set, append missing,
drop unknown). Collapse is likewise stored for all boxes regardless of presence.

### Pure transforms — `lib/stage.ts` (new, colocated `stage.test.ts`)

```ts
export const STAGE_BOXES = ["metronome", "isolation", "click", "notes", "recordings", "tuner", "drill"] as const;
export type BoxId = (typeof STAGE_BOXES)[number];

defaultFlow(): FlowRegion                          // canonical order, nothing collapsed
reconcileFlow(flow, allBoxes): FlowRegion          // exactly-once order; drop unknown; append missing; collapsed pruned to known
moveBox(order, id, toIndex): BoxId[]               // 1-D reorder within the flow
toggleCollapsed(flow, id): FlowRegion              // add/remove id from the collapsed set
```

These join the dock's workspace transforms in spirit; the workspace-level helpers
(`reconcileWorkspace`, `defaultWorkspace`) extend to seed/repair `stage`.

## Surface-header unification (the shell)

Three single-label headers have drifted into near-duplicates:

- `Box.head` — a stage card's header (small-caps muted label + right-aligned tools, bottom border).
- `SectionHead` — an in-page group heading (same row, as a divider).

> Note: the dock's `.tabs` bar is **not** in this set — it's an interactive strip
> of N tabs, not a single label header. Forcing it in would be the bolt-on this
> redesign avoids. It keeps its own rendering.

Extract one **`SurfaceHead`** primitive — the shared label + optional `tools` slot
row — plus one optional adornment used only by stage boxes: a **collapse toggle**
(a caret, matching the dock's chevron idiom).

`Box` renders `SurfaceHead` with the collapse prop; `SectionHead` renders it with
just label + tools. One header definition, so a stage box, an in-page group, and
(visually) the rest of the app read identically. This is also the *mechanism* for
per-box collapse — the toggle lives in the shared header.

**Reorder grabs the whole header — no separate grip.** The box header *is* the
drag surface (this is a power-user affordance; an extra grip glyph is visual
noise we don't want). A pointerdown on the header that crosses the drag threshold
starts a reorder; one that doesn't is a normal click, so the collapse caret and
any `tools` buttons in the header still work — the same down→threshold→drag-or-click
discipline the dock tabs already use.

## How a box is managed without rewriting every tool

Each tool already renders exactly one `<Box label=… >` (verified: MetronomeBox,
Isolation, ClickTrack, Notes, Recordings, Tuner, Drill — one Box each). So `Box`
*is* the managed unit. The minimal, modular wiring:

- Each tool passes a stable `id` to its `Box` (e.g. `<Box id="tuner" label="tuner">`).
- `Box` reads a **stage context** (provided by the stage flow component) for its
  `collapsed` state, renders the collapse toggle in its `SurfaceHead`, and makes
  the header a drag surface for reorder. Collapsed → body hidden, header strip remains.
- **App renders the flow** by iterating `workspace.stage.order`, filtering to
  present boxes via a registry (`BoxId → { component, present }`, the stage
  analogue of `TAB_VIEWS`), so DOM order = saved order. Reorder = rewrite
  `order`; an optional `animate:flip` smooths it.

Tool components are otherwise untouched — they keep their bodies and their `Box`.

## Persistence & migration (frontend only)

Same `workspace` setting; no new key, no Rust change. `migrateWorkspace` /
`reconcileWorkspace` gain stage handling:

- A persisted workspace from the previous slice has `{left, right}` and **no
  `stage`** → seed `stage = defaultFlow()`.
- An existing `stage` → `reconcileFlow` against `STAGE_BOXES`.

## Diff against the code — dispositions

| Existing | Disposition |
|---|---|
| `Box.head` + `SectionHead` headers | Unified into `SurfaceHead`. **Fix now.** |
| dock `.tabs` strip | Left as-is (different kind — a tab strip, not a label). **Explicitly not merged.** |
| Stage box order hard-coded in `App.svelte` markup | Driven by `workspace.stage.order` + a box registry. **Fix now.** |
| Per-box collapse | New, via `SurfaceHead` caret + stage context. **Build.** |
| Reorder affordance | The whole box header is the drag surface — no grip glyph. **Build.** |
| `Workspace = {left, right}` | Widened to `{left, right, stage}`; migration seeds `stage`. **Fix now.** |
| Transport / waveform | Stay the fixed stage head, outside the flow region. **Unchanged.** |
| Box↔tab interchange | Parked — tools are always-visible by design. **Deferred, not designed-for.** |

## Phases

Each leaves the tree shippable, with a gate and a commit. Smoke-test (vite +
chrome) on rendering/effect-touching phases.

### Phase 1 — `SurfaceHead` extraction
Pull the shared label+tools row out of `Box` and `SectionHead` into one
`SurfaceHead` (with unused-yet collapse/drag props defined). Both render it. No
behavior change.
**Gate:** `just lint` clean; the stage and pages look identical to before; smoke-test.

### Phase 2 — stage model + migration (`stage.ts`, `dock.ts`, `stores.ts`)
`FlowRegion`, `STAGE_BOXES`, `defaultFlow`, `reconcileFlow`, `moveBox`,
`toggleCollapsed`; widen `Workspace`/`reconcileWorkspace`/`migrateWorkspace` to
carry `stage`. No UI change (App still renders the hard-coded order).
**Gate:** `stage.test.ts` + extended `dock.test.ts`/`workspace-migrate.test.ts`
green; app launches with prior layout, stage unchanged.

### Phase 3 — render the flow from the model + collapse
App renders the flow from `workspace.stage.order` via the box registry + presence
filter; each tool passes its `id`; `Box` reads the stage context for `collapsed`
and shows the collapse toggle. Per-box collapse works and persists.
**Gate:** boxes render in saved order; collapsing a box hides its body and
persists across reload; contextual presence still correct; smoke-test, no effect loop.

### Phase 4 — box reorder drag
Dragging a box by its header reorders within the flow (`moveBox`), persisted, with
a FLIP glide. The header is the drag surface (down→threshold→drag-or-click, so the
collapse caret and tools still click). Stage-only (no cross-region drag).
**Gate:** dragging a box header reorders the flow and persists; the collapse caret
and header tools still click without starting a drag; smoke-test.

## Deferred (not designed-for)

- **Box↔tab interchange** (a tool stashed in the dock, a page pulled to the stage)
  — parked unless a concrete need appears; tools are always-visible by design.
- Resizing boxes (the flow sizes to content + wrap; not a stack of weights).
