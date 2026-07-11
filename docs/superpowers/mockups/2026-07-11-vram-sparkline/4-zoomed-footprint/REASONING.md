# 4 · zoomed footprint + capacity gauge

## Style

Auto-zoomed area chart: the y-range is the run's own min–max window (plus
padding), so the trace fills the box with visible variation. A 6px vertical
capacity gauge on the right edge shows the full 0–16 GB scale with the zoom
window marked on it, tinted by peak pressure.

## Decisions

- Attacks the blob *structurally*: the monochrome mass was the flat baseline
  below the run's min; cropping it means every rendered pixel is signal.
- The gauge preserves the one thing zooming loses — absolute position within
  the card — in a glance-sized strip, colored by the ok/warm/hot scheme.
- Fill stays muted grey; the widget only gains color when near capacity.

## Requirements mapping

- "Too monochromatic" → variation now dominates the box (shape everywhere),
  and pressure color appears on the gauge when it matters.
- All existing data preserved: current, min–max, capacity, peak (gauge top).

## Trade-offs

- Gains: maximum detail from the same pixels; scales to any usage pattern.
- Sacrifices: two marks to learn (trace + gauge); y-scale differs per run, so
  two runs' traces aren't visually comparable without reading the numbers;
  most implementation surface of the four.
