# 2 · pressure-tinted bars

## Style

Same bar-fill geometry as today; per-bar color encodes capacity pressure with
the existing ok/warm/hot thresholds (grey / accent-blend / red). No guides —
the color IS the peak information.

## Decisions

- Zero new geometry: smallest diff from the shipped component.
- Color budget spent only when it matters — a comfortable run stays grey,
  which respects dredge's quiet-by-default aesthetic; danger jumps out.
- Peak/min guide lines removed; the min–max text readout already carries the
  numbers, and the hottest bars mark the peak visually.

## Requirements mapping

- "Too monochromatic" → answered semantically: monochrome now *means*
  "healthy"; color appears exactly when headroom shrinks.
- Consistent with the cpu/gpu/ram bars, whose fills already tint warm/hot.

## Trade-offs

- Gains: information-bearing color, idiom-consistent, trivial to implement.
- Sacrifices: a healthy run still looks like today's grey blob (arguably
  correct, but it doesn't fix the aesthetic complaint on the happy path);
  fine shape detail stays hard to read within a solid fill.
