# B · mixed scale

## Style

Identical anatomy to variant A (trace + capacity gauge + readout), but the
scaling rule splits by metric type: cpu/gpu (bounded percentages) render on
their absolute 0–max scale; ram/vram (wide absolute ranges where the footprint
is the story) zoom to their working window.

## Decisions

- Percentages are inherently absolute quantities — a gpu trace hugging the top
  *means* saturated, and that read is preserved.
- Memory meters keep the direction-4 zoom because their interesting variation
  is a thin slice of a huge range; absolute scale is what created the blob.
- The gauge appears on all four anyway, so the anatomy — and the pressure
  color language — stays uniform even though the scales differ.

## Requirements mapping

- "…or at least reconsider them so they relate visually" — full kinship in
  marks and color; scaling honesty preserved per metric.

## Trade-offs

- Gains: each meter keeps its most truthful reading; no misleading fullness.
- Sacrifices: two rules in one block — a reader who notices vram is zoomed
  might briefly assume gpu is too; low-utilization percentage runs render as
  near-flat lines at the bottom (truthful, but less pretty).
