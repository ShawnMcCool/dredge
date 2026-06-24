/** Clamp a BPM to the supported range and round to an integer. */
export function clampBpm(bpm: number): number {
  return Math.max(30, Math.min(300, Math.round(bpm)));
}

export interface TapState {
  /** Tap timestamps (ms), oldest→newest, within the current window. */
  taps: number[];
}

const TAP_GAP_MS = 2000; // a gap longer than this starts a fresh tap window
const TAP_WINDOW = 4; // average over at most this many taps

/** Fold a tap at time `now` (ms) into the state, returning a BPM when derivable.
 *  Resets the window if the gap since the last tap exceeds TAP_GAP_MS. */
export function tapTempo(state: TapState, now: number): { state: TapState; bpm: number | null } {
  const last = state.taps[state.taps.length - 1];
  const taps =
    last != null && now - last > TAP_GAP_MS ? [now] : [...state.taps, now].slice(-TAP_WINDOW);
  if (taps.length < 2) {
    return { state: { taps }, bpm: null };
  }
  const span = taps[taps.length - 1] - taps[0];
  const avgInterval = span / (taps.length - 1);
  return { state: { taps }, bpm: clampBpm(60000 / avgInterval) };
}
