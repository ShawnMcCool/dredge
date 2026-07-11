# 3 · line only + guides

## Style

Scope-trace minimalism: a thin near-foreground line on the raised background,
dotted guides for min (muted) and peak (capacity-tinted). No fill anywhere.

## Decisions

- Removing the fill removes the blob *by construction* — there is no mass to
  read as monochrome, only a trace.
- Neutral line color (fg-mix, not accent) keeps the widget from competing
  with real accent-colored controls; guides do the semantic work.
- Min guide earns its place again: with no fill, the band between guides
  frames the run's working range.

## Requirements mapping

- "Too monochromatic" → answered by subtraction: shape + two guides instead
  of a mass of one grey.
- Matches dredge's instrument feel (tuner, meters) — quiet, precise.

## Trade-offs

- Gains: maximum shape legibility per pixel of ink; calm at any usage level.
- Sacrifices: absolute "how full is the card" reads only from the line's
  vertical position, weaker than a fill; loses visual kinship with the
  bar-fill cpu/gpu/ram meters above it.
