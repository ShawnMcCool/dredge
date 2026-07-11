# 1 · accent area + line

## Style

Classic sparkline: a 1.4px theme-accent line traces usage; a 14%-opacity
accent fill sits under it. The dashed peak guide stays, tinting amber/red only
under pressure (reusing the readout's ok/warm/hot scheme).

## Decisions

- The *line* becomes the primary mark — shape reads instantly, no blob.
- Fill stays anchored to zero so absolute magnitude vs capacity is still
  visible (a run using 4/16 GB looks quarter-full).
- Accent color ties the widget to the theme (`--accent`), per the repo rule
  that active/informative states use the accent, not hardcoded hues.

## Requirements mapping

- "Too monochromatic" → accent line + soft fill replaces uniform grey bars.
- Keeps every existing datum: current, min–max readout, peak guide, capacity.

## Trade-offs

- Gains: legible shape, theme-consistent, minimal change (still one SVG).
- Sacrifices: low-variation runs still compress into a flattish line near the
  bottom (zero-anchored scale); accent green now means "data" here but "on
  state" elsewhere.
