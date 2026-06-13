# Campaign: waveform scrollbar / range selector (horizontal zoom + pan)

Status: backlog
Raised: 2026-06-13

## Idea

A horizontal **scrollbar** below the waveform that doubles as a zoom/range
selector:
- By **default** the waveform is fit exactly to the viewport (shows the whole
  track), and the scrollbar window spans the full width — inert.
- The scrollbar has **draggable handles**: drag an edge handle to **narrow the
  visible range** (zoom in on part of the track); drag the window body to
  **pan**. Narrowing the window "engages" the scrollbar — now it scrolls the
  zoomed waveform.
- The **structure lane** (section headers) above the waveform must zoom/pan in
  lockstep with the waveform.

## Context

- `apps/desktop/src/components/Waveform.svelte` already holds the visible window
  as `view: View = { startSec, endSec, width }` (state, ~line 36; `View` in
  `lib/waveform-math.ts`). ALL rendering — waveform bars, structure lane, loops,
  selection, playhead — is drawn through `secToX(view, s)` / `xToSec(view, x)`.
  So **changing `view.startSec`/`view.endSec` zooms and pans everything at once,
  including the structure lane** — the lane "adjusts to zoom" for free.
- Confirm how `view` is currently fit (likely an `$effect` setting it to
  `{ startSec: 0, endSec: duration, width }` on song open / resize). The
  scrollbar replaces that with a user-controllable sub-range, defaulting to the
  full track.
- NOTE: the `ctrl ±/0` "zoom" (keys.ts → `lib/zoom.ts`) is **UI scale**
  (font/element size, the `ui_scale` setting), NOT waveform time-zoom. This
  scrollbar introduces a *separate, new* horizontal time-zoom concept. Don't
  conflate them.
- Grid-snap, beat ticks, and downbeats already render relative to `view`, so
  they follow automatically.

## Likely shape

A thin region (its own `<canvas>` or DOM strip) under the main waveform showing
the **whole track** as an overview (faint waveform / section bands), with a
draggable **window** rectangle: edge handles resize it (zoom), body drag pans.
Pointer handlers map x → time over the FULL duration (not the zoomed view), clamp
to `[0, duration]`, enforce a minimum window width, and write
`view.startSec`/`view.endSec`. The main waveform redraws reactively. Optionally:
mouse-wheel over the waveform zooms toward the cursor; double-click the scrollbar
resets to full-fit.

## Next step

Brainstorm → spec → plan → build (frontend: `Waveform.svelte` + a new scrollbar
element, maybe a small `viewport-math` helper for window↔time mapping that's
unit-testable). Decide: separate canvas vs DOM strip; wheel-zoom yes/no; whether
to show section bands in the overview; min zoom window.
