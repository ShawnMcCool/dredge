# Transport redesign — Direction 1: Dense Pro Bar

A single-line, icon-driven, professional-audio transport that lives directly
under the waveform across the centre stage column. The brief was committed to
fully: one horizontal strip, three whitespace-separated clusters, no pipe
rules, no chip soup, exactly one amber primary.

## Style

Near-black surface (`--bg`), one amber accent (`--accent #e0a458`), monospace
tabular numbers for every readout. Borders are hairline `--line`; nothing has a
drop shadow or a heavy outline. The only saturated element on the bar is the
play button — everything else is muted grey that resolves to `--fg` on hover.
The result reads as a high-end DAW transport, not a SaaS toolbar: calm,
dense, and quiet until touched.

The strip carries a faint 1px `--line` top border because the waveform sits
directly above it; the body shows a dimmed pseudo-waveform so the bar is judged
*in situ* rather than floating.

## Design decisions

### Three clusters, whitespace only
The controls are grouped by *intent*, not by widget type:

- **transport** — play · time readout (the "what's happening now")
- **shaping** — speed · pitch (the ear-training transforms)
- **output** — volume · bass focus · reset (signal out + recovery)

Clusters are separated by a generous 32px gap and `margin:auto` (shaping floats
to centre, output pins right). **No vertical rules** — the eye groups by
proximity, which is calmer and removes a dozen little lines from a dark UI.

### One filled-amber primary
Play/pause is the sole high-contrast control: a 34px filled-amber circle with a
`--bg`-coloured glyph. Because it is the only saturated object on the strip,
hierarchy is unambiguous at a glance — you always know where the one button
that *does the thing* is. Pause swaps the triangle for two bars in the same
circle.

### Segmented speed pill (kills the chip soup)
The four presets `50 · 70 · 85 · 100` are a **single bordered segmented
control** — one hairline box, internal dividers, the active segment filled
amber. The continuous 25–200% slider and the live `100%` value (in accent) sit
inline beside it. So speed is *one* visual object spanning preset + fine + live
value, instead of four loose outlined chips plus a stray number.

### Unified pitch stepper
Pitch is **one** hairline-bordered group: `−` | `0 st` | `+`. The value is
mono, tabular, centred, with `0 st` dimmed to muted (neutral) and non-zero
values (`−2 st`) in full `--fg` so an active transposition stands out. Three
loose chips collapse into a single stepper unit.

### Bass focus as a stateful toggle
A borderless pill with a leading status dot. Off: muted text, grey dot, no
border. On: amber text, amber dot, a `--accent-dim` hairline border and a
6%-amber wash — clearly "engaged" without shouting. Both states are shown
(off in the wide instance, on in the alternate-states instance).

### Icon system
All emoji replaced with inline Feather-style line SVGs on a 24-viewBox,
`stroke="currentColor"`, stroke-width 2, round caps/joins — except play/pause,
which are `fill="currentColor"` solids. Authored: play triangle, pause bars,
volume (speaker + two arcs), mute (speaker + ✕), rotate-ccw (reset). Because
they all inherit `currentColor`, every state change (muted → `--miss`, hover →
`--fg`, accent) is just a `color` change with zero per-icon CSS.

### Quiet reset at the far end
Reset workspace is the rotate-ccw icon, parked last in the output cluster at
`--wave` (barely visible) and lifting to `--fg` only on hover — a recovery
affordance that never competes for attention.

## Requirements mapping

| Requirement | Where |
|---|---|
| Monochrome SVG icons, no emoji | all 5 icons inline `currentColor` |
| No pipe/rule separators | clusters use whitespace + auto-margins only |
| One clear accent primary | filled-amber play circle (only saturated element) |
| Segmented / unified controls | speed segmented pill + pitch stepper group |
| Mono tabular readouts | `.mono` → `--mono` + `tabular-nums` on time/speed/pitch/vol |
| Volume mute + slider, 0–150%, % | mute toggle, `.vol-range`, `.vol-pct` |
| Bass focus off + on | both instances; `.toggle` / `.toggle.on` |
| Speed presets + continuous 25–200%, accent value | segpill + range + `.speed-val` |
| Pitch ±12 stepper, centred mono | `.stepper` with centred `.val` |
| Consistent control height ~26–28px | `--ctl-h:27px` on segpill / stepper / toggles |
| Calm, dense, wraps narrow | `flex-wrap` strip; ~520px copy proves it |
| Exact tokens on `:root` | verbatim from brief |

## Trade-offs

- **Density vs. touch targets.** Tuned for a keyboard-driven desktop app where
  the mouse is a fallback; the 27px controls are deliberately tight. Not a
  touch design.
- **Auto-margin centring vs. wrap order.** At full width, shaping centres and
  output pins right via auto-margins; when the bar wraps narrow those margins
  are reset so the clusters stack as clean left-aligned rows rather than
  scattering. This is the one place where wide and narrow need slightly
  different layout intent.
- **Two readouts for speed (presets + slider + value) feels redundant** but is
  intentional: presets are the muscle-memory path, the slider is the
  fine-adjust path, the accent value is the single source of truth. They share
  one cluster so it still reads as one control.
- **Labels (`speed` / `pitch`) cost horizontal space.** Kept because, stripped
  of pipe rules and boxes, tiny uppercase labels are what tells you which
  unified group is which. They're the cheapest disambiguation available.
