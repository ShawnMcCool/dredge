# A · uniform zoom

## Style

All four meters share the direction-4 anatomy — grey footprint trace zoomed to
the run's own min–max window, 6px absolute capacity gauge on the right edge,
numeric now + min–max readout. vram keeps its taller box (the headline metric
during separation); cpu/gpu/ram sit at 34px.

## Decisions

- One scaling rule for the whole block: the trace always shows *variation*,
  the gauge always shows *absolute position*. Nothing to learn twice.
- Color stays reserved for pressure (gauge tint + readout), so the block is
  calm until something approaches a limit.

## Requirements mapping

- "Same treatment for the other gauges" — literally applied.
- Visual kinship with the chosen vram indicator is total: same marks, same
  meaning, different heights only.

## Trade-offs

- Gains: strongest coherence; every meter gets legible shape.
- Sacrifices: a percentage trace no longer shows absolute utilization by line
  height — gpu wobbling 60–90% fills the box the same as one wobbling 2–5%;
  only the gauge and numbers distinguish them.
