// Per-song isolation state: the saved wire shape for the isolation box (bass
// focus + per-stem levels/mutes/solos) and the pure maps between it and the
// live `StemMix` store. Mirrors `practice::model::Isolation`. A leaf module
// (type-only dependency on `stores.ts`) so the maps are unit-testable without
// the Tauri seams and without a runtime import cycle.
import type { StemMix } from "./stores";

// The stem vocabulary's length — matches `STEM_LABELS` in `stores.ts` and
// `STEM_COUNT`/`STEM_NAMES` in the Rust core (vocals/drums/bass/guitar/piano/other).
const STEM_COUNT = 6;

export interface Isolation {
  bass_focus: boolean;
  levels: number[];
  mutes: boolean[];
  solos: boolean[];
}

/** Pad/truncate `v` to the stem count, filling missing entries with `fill`. */
function fit<T>(v: T[], fill: T): T[] {
  const out = v.slice(0, STEM_COUNT);
  while (out.length < STEM_COUNT) out.push(fill);
  return out;
}

/** Saved isolation → live stem-fader state (padded to the current stem count). */
export function isolationToStemMix(iso: Isolation): StemMix {
  return {
    levels: fit(iso.levels ?? [], 100),
    mutes: fit(iso.mutes ?? [], false),
    solos: fit(iso.solos ?? [], false),
  };
}

/** Live stem-fader state + bass focus → the saveable isolation snapshot. */
export function stemMixToIsolation(mix: StemMix, bassFocus: boolean): Isolation {
  return {
    bass_focus: bassFocus,
    levels: mix.levels.slice(),
    mutes: mix.mutes.slice(),
    solos: mix.solos.slice(),
  };
}

/** Lowest snapshot slot number not yet in use (1-based). */
export function nextFreeSlot(snaps: { slot: number }[]): number {
  const used = new Set(snaps.map((s) => s.slot));
  let slot = 1;
  while (used.has(slot)) slot += 1;
  return slot;
}
