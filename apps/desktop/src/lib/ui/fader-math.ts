// Pure slider value math — everything Fader computes, nothing it renders.

/** Map a 0..1 track position to a clamped, step-rounded value in [min, max]. */
export function posToValue(pos01: number, min: number, max: number, step: number): number {
  if (max <= min) return min;
  const p = Math.min(Math.max(pos01, 0), 1);
  const raw = min + p * (max - min);
  const stepped = step > 0 ? min + Math.round((raw - min) / step) * step : raw;
  // kill float drift (0.7000000000000001 → 0.7) before the final clamp
  const clean = Number(stepped.toPrecision(12));
  return Math.min(Math.max(clean, min), max);
}

/** Map a value to its 0..1 position along [min, max], clamped. */
export function valueToPos01(v: number, min: number, max: number): number {
  if (max <= min) return 0;
  return Math.min(Math.max((v - min) / (max - min), 0), 1);
}
