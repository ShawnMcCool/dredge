// Pure region math for the Drill box scratch span. No Svelte, no stores — the
// drill box edits an ephemeral span (seeded from the active loop) without ever
// touching the saved LoopRegion. Edges snap to a supplied grid (beats/downbeats)
// when one exists, else step by a fixed number of seconds.

export interface Span {
  start: number;
  end: number;
}

/** Smallest scratch span we allow, in seconds. */
export const MIN_DRILL_SPAN = 0.05;

const clampStart = (start: number, end: number): number =>
  Math.max(0, Math.min(start, end - MIN_DRILL_SPAN));

const clampEnd = (start: number, end: number, duration: number): number =>
  Math.min(duration, Math.max(end, start + MIN_DRILL_SPAN));

const nearest = (t: number, times: number[]): number => {
  let best = times[0];
  for (const x of times) if (Math.abs(x - t) < Math.abs(best - t)) best = x;
  return best;
};

/** The next grid time strictly beyond `time` in direction `dir`; falls back to
 *  a fixed seconds step when there is no grid (or none beyond the edge). */
export function nextGrid(
  time: number,
  gridTimes: number[],
  dir: 1 | -1,
  fallbackStep: number,
): number {
  const EPS = 1e-4;
  if (gridTimes.length === 0) return time + dir * fallbackStep;
  if (dir > 0) {
    for (const t of gridTimes) if (t > time + EPS) return t;
    return time + fallbackStep;
  }
  for (let i = gridTimes.length - 1; i >= 0; i--) if (gridTimes[i] < time - EPS) return gridTimes[i];
  return time - fallbackStep;
}

/** Move one edge of the span by one grid step (or `fallbackStep` seconds),
 *  keeping the other edge fixed and enforcing the minimum span / bounds. */
export function nudgeEdge(
  span: Span,
  edge: "start" | "end",
  dir: 1 | -1,
  gridTimes: number[],
  duration: number,
  fallbackStep: number,
): Span {
  if (edge === "start") {
    const moved = nextGrid(span.start, gridTimes, dir, fallbackStep);
    return { start: clampStart(moved, span.end), end: span.end };
  }
  const moved = nextGrid(span.end, gridTimes, dir, fallbackStep);
  return { start: span.start, end: clampEnd(span.start, moved, duration) };
}

/** Shrink the span to its first or second half, snapping the cut to the nearest
 *  interior grid line when a grid is supplied. */
export function bisect(span: Span, half: "first" | "second", gridTimes: number[] = []): Span {
  let mid = (span.start + span.end) / 2;
  if (gridTimes.length) {
    const n = nearest(mid, gridTimes);
    if (n > span.start && n < span.end) mid = n;
  }
  return half === "first" ? { start: span.start, end: mid } : { start: mid, end: span.end };
}

/** Extend (or retract) the span's start by `deltaBars` downbeats to rehearse the
 *  entrance into the passage. Positive = earlier start (more run-up); negative =
 *  pull the start later. Falls back to ~2 s/bar when there are no downbeats. The
 *  end never moves. */
export function runUp(
  span: Span,
  deltaBars: number,
  downbeats: number[],
  duration: number,
): Span {
  if (deltaBars === 0) return span;
  if (downbeats.length === 0) {
    return { start: clampStart(span.start - deltaBars * 2, span.end), end: span.end };
  }
  const EPS = 1e-4;
  // anchor = the downbeat at or just before the current start
  let anchor = 0;
  for (let i = 0; i < downbeats.length; i++) if (downbeats[i] <= span.start + EPS) anchor = i;
  let target = anchor - deltaBars;
  target = Math.max(0, Math.min(target, downbeats.length - 1));
  return { start: clampStart(downbeats[target], span.end), end: span.end };
}
