// Pure math for run meters — the analyzing readout's footprint traces. A run
// meter draws one resource's sampled history against its absolute capacity:
// the trace zooms to the run's own min–max window so every pixel is variation;
// the capacity gauge marks where that window sits in 0..total. All geometry is
// in the SVG's 0..100 y-space (0 = top).

export interface TraceWindow {
  lo: number;
  hi: number;
}

/** Capacity pressure — the single color language for all meters: quiet grey
 *  until a resource fills up, accent when busy, red near its limit. */
export type Pressure = "ok" | "warm" | "hot";

export function pressure(frac: number): Pressure {
  return frac >= 0.9 ? "hot" : frac >= 0.72 ? "warm" : "ok";
}

/** The zoomed y-window for a run: min–max padded by 12% of the span plus 0.3%
 *  of capacity (so a flat run still gets a nonzero window), clamped to
 *  [0, total]. Guaranteed non-degenerate (hi > lo) for any total > 0. */
export function traceWindow(min: number, peak: number, total: number): TraceWindow {
  const pad = (peak - min) * 0.12 + total * 0.003;
  const lo = Math.max(0, min - pad);
  const hi = Math.min(total, peak + pad);
  if (hi <= lo) return { lo, hi: lo + 1 };
  return { lo, hi };
}

/** SVG polyline points for a history within a window: x = sample index,
 *  y = 0..100 top-down, clamped so out-of-window samples pin to the edges.
 *  Fewer than two samples yield "" — there is no line to draw. */
export function tracePoints(hist: number[], w: TraceWindow): string {
  if (hist.length < 2) return "";
  const span = w.hi - w.lo;
  return hist
    .map((u, i) => {
      const y = Math.max(0, Math.min(100, 100 - ((u - w.lo) / span) * 100));
      return `${i},${y.toFixed(2)}`;
    })
    .join(" ");
}

/** The gauge's window marker in 0..100 y-space: top offset + height of the
 *  run's min–peak band within 0..total, with a 2-unit floor so even a flat
 *  run stays visible. */
export function gaugeWindow(min: number, peak: number, total: number): { y: number; h: number } {
  return {
    y: 100 - (peak / total) * 100,
    h: Math.max(2, ((peak - min) / total) * 100),
  };
}
