// Time/duration formatting shared across the UI. Kept pure and colocated-tested
// so the various m:ss / mm:ss.s / elapsed shapes stay consistent.

/**
 * Compact duration: unpadded minutes + zero-padded seconds (`m:ss`).
 * Used for song/loop/section lengths. `round` rounds seconds (section
 * boundaries) instead of flooring them (durations).
 */
export function fmtDur(secs: number, round = false): string {
  const s = Math.max(secs, 0);
  const m = Math.floor(s / 60);
  const r = round ? Math.round(s % 60) : Math.floor(s % 60);
  return `${m}:${String(r).padStart(2, "0")}`;
}

/**
 * Zero-padded transport clock (`mm:ss.s`). `decimals` controls the fractional
 * seconds: 1 for the live playhead readout, 0 for a whole-second total.
 */
export function fmtClock(secs: number, decimals = 1): string {
  const s = Math.max(secs, 0);
  const m = Math.floor(s / 60);
  const r = s - m * 60;
  const sec =
    decimals > 0
      ? r.toFixed(decimals).padStart(3 + decimals, "0")
      : String(Math.floor(r)).padStart(2, "0");
  return `${String(m).padStart(2, "0")}:${sec}`;
}

/**
 * Elapsed time that adapts its unit: milliseconds under a second, seconds with
 * one decimal under a minute, then the compact `m:ss` clock. Used by progress
 * readouts where the magnitude varies widely.
 */
export function fmtElapsed(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const s = ms / 1000;
  return s < 60 ? `${s.toFixed(1)} s` : fmtDur(s);
}
