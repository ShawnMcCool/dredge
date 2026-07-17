// Pure hit-testing for the waveform canvas: which loop / lane span sits under a
// canvas point, and which loop edge is nearest. Geometry only — the component
// keeps thin store-reading wrappers that feed these the current loops/spans.

import type { LoopRegion, OpenSong } from "./stores";
import { secToX, xToSec, type View } from "./waveform-math";

export interface LaneSpan {
  name: string;
  start: number;
  end: number;
  suggested: boolean;
}

export type LoopEdge = { loop: LoopRegion; edge: "start" | "end" };

/** Structure-lane rows: saved sections when any exist, else analysis
 *  suggestions (never both — the Sections tab shows the rest). */
export function laneSpans(open: OpenSong): LaneSpan[] {
  if (open.sections.length > 0) {
    return open.sections.map((s) => ({ name: s.name, start: s.start, end: s.end, suggested: false }));
  }
  return (open.analysis?.sections ?? []).map((s) => ({
    name: s.label,
    start: s.start,
    end: s.end,
    suggested: true,
  }));
}

/** Topmost loop whose body is under a canvas point (below the lane). */
export function hitLoopBody(
  loops: LoopRegion[],
  view: View,
  x: number,
  y: number,
  laneH: number,
): LoopRegion | null {
  if (y < laneH) return null;
  const sec = xToSec(view, x);
  for (let i = loops.length - 1; i >= 0; i--) {
    const l = loops[i];
    if (sec >= l.start && sec <= l.end) return l;
  }
  return null;
}

/** A loop edge within `edgePx` of canvas x (first match). */
export function hitLoopEdge(loops: LoopRegion[], view: View, x: number, edgePx: number): LoopEdge | null {
  for (const l of loops) {
    if (Math.abs(secToX(view, l.start) - x) <= edgePx) return { loop: l, edge: "start" };
    if (Math.abs(secToX(view, l.end) - x) <= edgePx) return { loop: l, edge: "end" };
  }
  return null;
}

/** The loop edge (across all loops) nearest to canvas x — right-drag grabs this
 *  from anywhere, like snapping to the nearest tile border. */
export function nearestLoopEdge(loops: LoopRegion[], view: View, x: number): LoopEdge | null {
  let best: LoopEdge | null = null;
  let bestDist = Infinity;
  for (const l of loops) {
    const ds = Math.abs(secToX(view, l.start) - x);
    if (ds < bestDist) ((bestDist = ds), (best = { loop: l, edge: "start" }));
    const de = Math.abs(secToX(view, l.end) - x);
    if (de < bestDist) ((bestDist = de), (best = { loop: l, edge: "end" }));
  }
  return best;
}

/** Lane span containing a time (used while dragging across headers). */
export function spanAtTime(spans: LaneSpan[], sec: number): { start: number; end: number } | null {
  const s = spans.find((sp) => sec >= sp.start && sec <= sp.end);
  return s ? { start: s.start, end: s.end } : null;
}

/** Structure-lane span under a canvas point (lane y-band only). */
export function hitLaneSpan(
  spans: LaneSpan[],
  view: View,
  x: number,
  y: number,
  laneH: number,
): LaneSpan | null {
  if (y >= laneH) return null;
  const sec = xToSec(view, x);
  return spans.find((s) => sec >= s.start && sec <= s.end) ?? null;
}

/** Marker pip box: a numbered flag hanging right of the marker stem. */
export const MARKER_PIP_W = 12;
export const MARKER_PIP_H = 14;

/** Marker pip under a canvas point (its flag band, just below the lane). */
export function hitMarkerPip(
  view: View,
  markers: { slot: number; pos: number }[],
  x: number,
  y: number,
  laneTop: number,
): { slot: number; pos: number } | null {
  if (y < laneTop || y > laneTop + MARKER_PIP_H) return null;
  // draw order is ascending slot, so the highest slot paints on top; walk
  // backwards so an overlap hit returns the one actually visible.
  for (let i = markers.length - 1; i >= 0; i--) {
    const m = markers[i];
    const mx = secToX(view, m.pos);
    if (x >= mx - 2 && x <= mx + MARKER_PIP_W) return m;
  }
  return null;
}
