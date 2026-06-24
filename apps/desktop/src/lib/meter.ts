// Time-signature estimate from the analysis beat grid. Shared by the structure
// tab's "n/4" readout and the count-in's default beat count, so they agree.

/** Beats per bar (the time-signature numerator), derived as beats ÷ downbeats,
 *  or `null` when there is no sane estimate (no grid, or an implausible value).
 *  The denominator isn't inferred — the structure tab shows it as `n/4`. */
export function meterNumerator(
  a: { beats?: number[]; downbeats?: number[] } | null | undefined,
): number | null {
  if (!a?.beats?.length || !a?.downbeats?.length) return null;
  const per = Math.round(a.beats.length / a.downbeats.length);
  return per >= 2 && per <= 12 ? per : null;
}
