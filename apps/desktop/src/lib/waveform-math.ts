// Pure waveform geometry — everything the canvas needs, nothing it draws.

export interface View {
  startSec: number;
  endSec: number;
  width: number; // px
}

export const MIN_SPAN_SECS = 2;

export const secToX = (v: View, s: number) =>
  ((s - v.startSec) / (v.endSec - v.startSec)) * v.width;

export const xToSec = (v: View, x: number) =>
  v.startSec + (x / v.width) * (v.endSec - v.startSec);

/** Extrapolate playhead: position event + elapsed wall time × rate. */
export function playheadSecs(
  pos: { secs: number; rate: number; playing: boolean; at: number },
  now: number,
): number {
  if (!pos.playing) return pos.secs;
  return pos.secs + ((now - pos.at) / 1000) * pos.rate;
}

/** Zoom around an anchor (e.g. cursor), clamped to [0, duration] and a 2 s minimum span. */
export function zoom(v: View, anchorSec: number, factor: number, duration: number): View {
  const span = v.endSec - v.startSec;
  const newSpan = Math.min(Math.max(span * factor, MIN_SPAN_SECS), duration);
  // keep the anchor at the same fraction of the window → same px position
  const frac = (anchorSec - v.startSec) / span;
  let startSec = anchorSec - frac * newSpan;
  startSec = Math.min(Math.max(startSec, 0), duration - newSpan);
  return { startSec, endSec: startSec + newSpan, width: v.width };
}

/** Snap a time to the nearest downbeat when it is within `thresholdPx` of it
 *  on screen (px so the feel is zoom-independent). Identity otherwise. */
export function snapToGrid(
  sec: number,
  downbeats: number[],
  v: View,
  thresholdPx: number,
): number {
  if (downbeats.length === 0) return sec;
  let best = downbeats[0];
  for (const d of downbeats) {
    if (Math.abs(d - sec) < Math.abs(best - sec)) best = d;
  }
  const pxPerSec = v.width / (v.endSec - v.startSec);
  return Math.abs(best - sec) * pxPerSec <= thresholdPx ? best : sec;
}

/** Bucket range of the peaks array visible in the view (for drawing). */
export function visibleBuckets(
  v: View,
  framesPerBucket: number,
  sampleRate: number,
  totalBuckets: number,
): { first: number; last: number } {
  const perBucket = framesPerBucket / sampleRate; // seconds
  const first = Math.min(Math.max(Math.floor(v.startSec / perBucket), 0), totalBuckets - 1);
  const last = Math.min(Math.max(Math.ceil(v.endSec / perBucket), 0), totalBuckets - 1);
  return { first, last };
}
