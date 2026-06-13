# Campaign: drag section headers to select contiguous sections

Status: done (built 2026-06-13)
Raised: 2026-06-13

## Idea

Let the user **click-drag across the section-lane headers** (the labelled
`intro / verse / chorus …` spans above the waveform) to extend a selection
through multiple **contiguous** sections, then `l` to loop them. Today the only
way to loop two neighbouring sections is to manually drag a span on the waveform
body (with grid snap on); dragging the headers themselves would be the obvious,
precise gesture.

## Context

- The structure lane is drawn in `apps/desktop/src/components/Waveform.svelte`
  (`LANE_H = 24` px above the waveform; spans built from `open.sections` or
  `open.analysis?.sections`).
- Selection is the `selection` store (`{ start, end } | null`); dragging the
  waveform body already sets it (`Waveform.svelte` ~line 326), and `l` loops the
  current selection (`keys.ts` case `"l"` → `loopSelection`).
- Double-clicking a *suggested* span already seeds the selection to that one
  section (`Waveform.svelte` ~line 302-305). The new gesture extends that to a
  drag across several.

## Likely shape

A pointer-down on a header starts a section-drag; pointer-move over other headers
extends `selection` to span from the first dragged section's `start` to the
hovered section's `end` (clamped to contiguous sections); pointer-up ends it.
Grid-snap already aligns edges to section boundaries.

## Next step

Brainstorm → spec → plan → build (frontend-only, `Waveform.svelte`). Decide:
header-drag vs body-drag interaction precedence; whether it works on suggested
spans too; visual affordance during the drag.
