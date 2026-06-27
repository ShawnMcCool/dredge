# Stage Box Management

Spec: `docs/superpowers/specs/2026-06-27-stage-box-management-design.md`.

This is a **design plan** — what the UI should be, not how to build it. The
implementation plan (file structure, TDD tasks) is produced next via
`/new-feature` referencing this document.

## Problem Statement

The stage's control boxes became a managed, reorderable, collapsible flow in the
[stage flow region](2026-06-27-stage-flow-region-design.md) slice, but three
edges were left rough:

- The reorder drag gives almost no feedback — the source dims in place, nothing
  follows the cursor, and the drop position appears only on release. On a wide
  stage where boxes wrap across rows, you can't tell what's moving or where it
  will land.
- Collapse hides behind a caret glyph that carries no information the box isn't
  already showing through its height.
- There is no user control over which boxes occupy the stage. Presence is purely
  state-driven, so a tool you never use can't be removed.

## Design Objectives

- **Legibility** — at every moment of a reorder, you know which box is moving and
  exactly where it will land.
- **Consistency** — match the dock's existing drag idiom (its live insertion
  bar); don't invent a second drag language.
- **All-visible by design** — tools you keep stay on screen; curation removes
  tools from the stage, it does not hide them behind tabs.
- **Minimal surface, minimal gesture** — no new persistent panel, no second drag
  gesture; reuse the header, the reorder, and the `HoverActions` idiom already
  present.

## User-Facing Behavior

- **Collapse** — a short tap on a box header toggles it between full box and a
  header-only strip. The caret glyph is gone; state reads from height. Persists.
- **Reorder** — drag a box by its header. A ghost chip carrying the box's label
  tracks the cursor; a dashed placeholder marks the source; a live insertion bar
  marks where the box will land; the cursor shows `grabbing` throughout. The box
  drops exactly where the bar indicated. Boxes do not shuffle mid-drag.
- **Hide** — hovering a header reveals an `×` at its trailing edge (after any
  existing header tools). Clicking it removes the box from the stage. Persists.
- **Restore** — when at least one available box is hidden, a `+ tool` control
  appears at the end of the flow. Clicking it lists the hidden tools; picking one
  returns it to the stage, where it can be dragged to position. The control is
  absent when nothing is hidden.
- **Sticky hide** — a hidden contextual box (e.g. the drill box) stays hidden
  even when its context returns; it reappears in the `+ tool` menu, not on the
  stage, until restored.

## Acceptance Criteria

- [ ] Dragging a box shows a cursor-tracking ghost chip with its label, a dashed
      source placeholder, and a live insertion bar at the computed drop point.
- [ ] The box drops where the insertion bar indicated; boxes don't reflow or
      oscillate mid-drag.
- [ ] A short tap on a header toggles collapse; collapsed shows only the header
      strip; persists across reload.
- [ ] Tapping a header tool button or the hide-`×` neither toggles collapse nor
      starts a drag.
- [ ] Hovering a header reveals a trailing `×`; clicking it removes the box;
      removal persists.
- [ ] When ≥1 available box is hidden, a `+ tool` control appears, lists the
      hidden tools, and re-adds the picked one. When none are hidden, it is absent.
- [ ] A hidden contextual box stays hidden when its context returns — it appears
      in the `+ tool` menu, not on the stage.
- [ ] No caret glyph remains on any box header.

## Anti-patterns

- **Tabbed tool palette**: tab-ifying the tools defeats the stage's all-visible
  premise and adds a competing surface plus a second drag gesture. Rejected.
- **Live order mutation during drag**: the insertion bar is an overlay only; the
  order rewrites once, on drop, avoiding hovered-target oscillation.
- **Glyph clutter**: no caret, no drag-grip; the header itself is the affordance,
  and the `×` / `+ tool` stay quiet (hover / on-demand).
- **Always-on add chrome**: the `+ tool` control never shows when there is
  nothing to restore.
- **Right-button overload**: right-drag is the magnetic fuzzy handle; box
  management uses left-tap, left-drag, and hover only.

## Deferred

- Box↔tab interchange (a tool stashed in the dock, a page pulled to the stage) —
  still parked; tools are all-visible by design.
- Box resizing.
- Keyboard equivalents for collapse and reorder (pointer-only; hide stays
  keyboard-reachable as a button).

## Follow-up

- After the feature is implemented and verified, **ship a minor version**
  (per the user's instruction at design time). This is a release step, not part
  of the design.

## Decisions

See `docs/superpowers/specs/2026-06-27-stage-box-management-design.md` for the
design record and rationale (the three interaction decisions, the `FlowRegion`
model widening with `hidden`, and the collapse-vs-hide distinction).
