/** Clamp a count-in beat count to the 1..8 range. (On/off is a separate
 *  toggle, so the beat count is never zero — it's remembered while off.) */
export function stepCountInBeats(beats: number, delta: number): number {
  return Math.max(1, Math.min(8, beats + delta));
}
