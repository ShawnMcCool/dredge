# Stage box management — legible reorder drag, tap-to-collapse, user show/hide

Designed 2026-06-27 (brainstorm + from-first-principles redesign). Frontend only.
Work directly on `main`.

The follow-on to the [stage flow region](2026-06-27-stage-flow-region-design.md),
which made the stage's control boxes a managed, reorderable, collapsible flow but
left three rough edges: the reorder drag gives almost no feedback, collapse hides
behind a caret glyph, and there is **no user control over which boxes occupy the
stage** (presence is purely state-driven). This spec closes all three — and
revisits that spec's deferred "box visibility" without resurrecting the rejected
tab-ification of tools.

## What's wrong today

The reorder gesture works but is invisible while you use it:

- **Source identity** — the dragged box dims in place (`opacity: 0.5`); nothing
  follows the cursor. On a wide stage the boxes wrap across a 2-D area, so the
  source and the drop point can sit far apart, even on different rows. You can't
  tell at a glance *which* box is in flight.
- **No destination cue** — the drop position is computed only on pointer-up
  (`onHeadUp` hit-tests `elementFromPoint`). Nothing shows where it will land.
  The dock already solved exactly this: `createDockDrag` computes a live `caret`
  (`{x, y, h}`) on every `onTabMove` and renders an insertion bar. The stage flow
  is the poor cousin of an idiom that already exists in the same app.
- **Collapse affordance** — a caret button in the header toggles collapse. The
  caret carries no information the box isn't already showing (a collapsed box is
  just a header strip; an expanded one is tall), so it is glyph clutter.

And nothing lets you say "I never use the metronome, get it off my stage."

## Core idea

> Bring the stage flow's drag up to the dock's standard and let the user curate
> the box roster — **without adding any new surface or any new gesture**. The
> header is one dual-purpose surface (tap = collapse, drag = reorder); reorder
> gains three coordinated live cues; visibility extends the existing flow model
> with a `hidden` set, surfaced by a hover-`×` to remove and a quiet `+ tool`
> tail menu to restore.

## Three decisions (with rationale)

### 1. The header is the collapse surface — delete the caret

A short tap on a box header (pointer down → up without crossing the drag
threshold) toggles collapse. The header already runs the
`down → threshold → drag-or-click` discipline that let the caret survive on a
drag surface; we repoint the "click" branch from the caret to "toggle collapse"
and remove the caret glyph. Tap = collapse, drag = reorder, one surface.

Collapsed/expanded state stays legible from height alone, so no glyph is needed
to indicate it. Tradeoff accepted: collapse loses its keyboard path (the caret
was a focusable button). For a pointer-driven personal tool this is fine; the
new hide control stays a real button, so hide remains keyboard-reachable.

### 2. Reorder shows three coordinated live cues

Because the stage wraps across a 2-D area (unlike the dock's single narrow tab
bar), it needs *more* identity feedback than the dock, not the same. Each cue
answers one question:

- **Insertion bar** — *where will it land.* Port the dock's `caret`: compute the
  drop target on every `onHeadMove` (not just on `onHeadUp`) and render a
  vertical bar between boxes at that position. This is the headline fix and the
  consistency win.
- **Dashed placeholder** at the source — *where it came from* / what the row
  looks like without it. The dragged box's footprint as a dashed outline.
- **Ghost chip** at the pointer — *which box is in flight.* A small floating
  label-pill carrying the box's label, tracking the cursor.
- Plus a global `grabbing` cursor for the drag's duration so the whole stage
  reads as "dragging."

The ghost chip is the one cue that diverges from the dock (which has no
cursor-following ghost). It's justified: the dock's source tab and caret are
always close together in one bar; the stage's are not. Legibility beats strict
visual parity when the layout is genuinely different.

All three are **overlays**. The order mutates once, on drop — never during the
drag. (The current code already guards against live mutation, which makes a
hovered target oscillate as each move re-evaluates the just-changed order. The
insertion bar must not reintroduce that: it's a pure render of the computed drop
index, not a reflow.)

### 3. Show/hide curates the roster — no palette, no second gesture

Tab-ifying the tools was rejected by the stage-flow spec ("tabs hide things;
that defeats a live tool"), and a drag-from palette page is a softer version of
the same move — a persistent competing surface plus a second drag gesture whose
only real payoff is discoverability, which a personal tool doesn't need. So:

- **Hide** is a per-box action. On hover, the header reveals an `×` at its
  trailing edge (the existing `HoverActions` idiom — resting state stays clean).
  Clicking it removes the box from the stage. The `×` sits outermost, after any
  existing header tools, and only on hover. *Right-button gestures are
  deliberately not used* — right-drag is the magnetic fuzzy-handle gesture and
  must not be overloaded.
- **Restore** is one compact `+ tool` control at the tail of the flow, shown
  **only when ≥1 present box is hidden**. Click → a small menu of the hidden
  tools; pick one → it's appended back to the flow (un-hidden), then drag it to
  position with the reorder you already have. Absent when nothing is hidden.

This keeps the stage's all-visible premise intact — tools you *keep* are always
on screen, never behind a tab — while letting you curate which tools occupy it.

## Model

`FlowRegion` widens from `{ order, collapsed }` to add a hidden set:

```ts
interface FlowRegion {
  order: BoxId[];      // stable ordering over all known boxes
  collapsed: BoxId[];  // a set, stored as an array for JSON — minimized in place
  hidden: BoxId[];     // a set — removed from the stage, restorable from the + tool menu
}
```

Two derived sets drive the UI, both intersected with state-driven presence
(unchanged: metronome/tuner always; isolation/click/notes/recordings with a song
open; drill while a drill span is active):

- **Rendered on the stage** = `present ∧ ¬hidden`, in saved `order`.
- **Offered in the `+ tool` menu** = `present ∧ hidden`.

`hidden` and `collapsed` are orthogonal and both stored for *all* known boxes
regardless of current presence. A hidden contextual box (e.g. drill) stays
hidden when its context returns — it reappears in the `+ tool` menu, not on the
stage. Hide is sticky until you restore it.

`reconcileFlow` prunes `hidden` to the known box set exactly as it does for
`collapsed`; a persisted flow with no `hidden` seeds `hidden: []`. No Rust
change — same `workspace` setting, no new key.

## Collapse vs. hide — why both

They are different intents and stay distinct:

- **Collapse** (tap) — minimize *in place*; the labelled header strip stays on
  the stage as a bookmark you can re-expand inline.
- **Hide** (hover-`×`) — remove *from the stage* entirely; reclaim the space;
  restore later from the `+ tool` menu.

## Diff against the code — dispositions

| Existing | Disposition |
|---|---|
| Collapse caret in `SurfaceHead` / `Box` header | Removed. Tap on the header toggles collapse instead. **Change.** |
| `Box` header: down→threshold→drag-or-click, click → caret `oncollapse` | Click branch repointed to `toggle(id)`; tools/`×` still stop propagation. **Change.** |
| Stage drag: source dims, drop computed on `onHeadUp` only | Add live insertion bar + dashed source placeholder + cursor ghost chip + grabbing cursor; order still mutates once on drop. **Build.** |
| Dock `caret` insertion-bar rendering | Reused as the model for the stage's insertion bar. **Reuse, don't fork.** |
| `FlowRegion = { order, collapsed }` | Widened to add `hidden`; `reconcileFlow`/migration seed it. **Change.** |
| Box presence (state-driven only) | Now also gated by `¬hidden`; render filter = present ∧ ¬hidden. **Change.** |
| Hide affordance | New per-box hover-`×` (HoverActions idiom), outermost in the header. **Build.** |
| Restore affordance | New `+ tool` tail control + menu, shown only when something is hidden. **Build.** |
| Tabbed tool palette / box↔tab interchange | Still rejected / parked — tools are all-visible by design. **Not built.** |
| Right-button gestures on boxes | Left free for the magnetic fuzzy handle — not used here. **Avoid.** |

## Acceptance criteria

- Dragging a box shows a cursor-tracking ghost chip with its label, a dashed
  source placeholder, and a live insertion bar at the computed drop point;
  `grabbing` cursor throughout.
- The box drops where the insertion bar indicated; boxes do not reflow/oscillate
  mid-drag (order mutates once, on drop).
- A short tap on a header (no drag) toggles collapse; collapsed shows only the
  header strip; persists across reload.
- Tapping a header tool button or the hide-`×` neither toggles collapse nor
  starts a drag.
- Hovering a header reveals a trailing `×`; clicking it removes the box from the
  stage; removal persists across reload.
- When ≥1 present box is hidden, a `+ tool` control appears at the tail, lists
  the hidden tools, and re-adds the picked one to the flow; it persists. When
  nothing is hidden, the control is absent.
- A hidden contextual box (e.g. drill) stays hidden when its context returns — it
  appears in the `+ tool` menu, not on the stage.
- No caret glyph remains on any box header.

## Anti-patterns

- **Tabbed tool palette** — tab-ifying the tools defeats the stage's all-visible
  premise; adds a competing surface and a second drag gesture. Rejected.
- **Live order mutation during drag** — the insertion bar is an overlay only; the
  order rewrites once on drop, avoiding the hovered-target oscillation.
- **Glyph clutter** — no caret, no drag-grip; the header itself is the
  affordance. The `×` and `+ tool` stay quiet (hover / on-demand).
- **Always-on add chrome** — the `+ tool` control never shows when there's
  nothing to restore.
- **Right-button overload** — right-drag is the magnetic fuzzy handle; box
  management uses left-tap, left-drag, and hover only.

## Phases (design-level; implementation plan is separate)

Each leaves the tree shippable, with a gate and a commit. Smoke-test (vite +
chrome) on rendering/effect-touching phases.

1. **Tap-to-collapse, caret gone.** Repoint the header click branch to toggle
   collapse; delete the caret from `SurfaceHead`/`Box`. Collapse still persists.
2. **Hidden in the model.** Widen `FlowRegion` with `hidden`; `reconcileFlow` +
   migration seed/prune it; render filter excludes hidden. Pure model + filter,
   no new affordance yet.
3. **Hide + restore affordances.** Hover-`×` to hide; the `+ tool` tail control +
   menu to restore. Shown only when something is hidden.
4. **Drag feedback.** Live insertion bar (port the dock's `caret`), dashed source
   placeholder, cursor ghost chip, grabbing cursor — all overlays; drop unchanged.

## Deferred (not designed-for)

- Box↔tab interchange (a tool stashed in the dock, a page pulled to the stage) —
  still parked; tools are all-visible by design.
- Box resizing (the flow sizes to content + wrap).
- Keyboard equivalents for collapse and reorder — pointer-only, accepted; hide
  stays keyboard-reachable as a button.
