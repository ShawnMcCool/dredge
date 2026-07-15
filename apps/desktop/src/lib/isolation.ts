// Per-song isolation state: the saved wire shape for the isolation box (bass
// focus + per-stem levels/mutes/solos) and the pure maps between it and the
// live `StemMix` store. Mirrors `practice::model::Isolation`. Kept as its own
// module so the maps are unit-testable without the Tauri seams in `stores.ts`.
import { STEM_LABELS, type StemMix } from "./stores";

export interface Isolation {
  bass_focus: boolean;
  levels: number[];
  mutes: boolean[];
  solos: boolean[];
}

/** Pad/truncate `v` to the stem count, filling missing entries with `fill`. */
function fit<T>(v: T[], fill: T): T[] {
  const out = v.slice(0, STEM_LABELS.length);
  while (out.length < STEM_LABELS.length) out.push(fill);
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
