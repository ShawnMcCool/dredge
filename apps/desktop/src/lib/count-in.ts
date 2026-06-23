/** Clamp a count-in beat count to the 0..8 range. 0 = off. */
export function stepCountInBeats(beats: number, delta: number): number {
  return Math.max(0, Math.min(8, beats + delta));
}
