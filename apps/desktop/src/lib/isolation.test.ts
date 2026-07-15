import { describe, it, expect } from "vitest";
import { isolationToStemMix, stemMixToIsolation, type Isolation } from "./isolation";

const N = 6;

describe("isolation <-> stemMix", () => {
  it("round-trips full state", () => {
    const iso: Isolation = {
      bass_focus: true,
      levels: [80, 0, 100, 100, 100, 50],
      mutes: [false, true, false, false, false, false],
      solos: [false, false, true, false, false, false],
    };
    const mix = isolationToStemMix(iso);
    expect(mix.levels).toEqual(iso.levels);
    expect(mix.mutes).toEqual(iso.mutes);
    expect(mix.solos).toEqual(iso.solos);
    const back = stemMixToIsolation(mix, iso.bass_focus);
    expect(back).toEqual(iso);
  });

  it("pads a short saved state to the stem count", () => {
    const iso: Isolation = { bass_focus: false, levels: [10, 20, 30, 40], mutes: [], solos: [] };
    const mix = isolationToStemMix(iso);
    expect(mix.levels).toEqual([10, 20, 30, 40, 100, 100]);
    expect(mix.mutes).toEqual(Array(N).fill(false));
    expect(mix.solos).toEqual(Array(N).fill(false));
  });

  it("truncates an over-long saved state to the stem count", () => {
    const iso: Isolation = {
      bass_focus: false,
      levels: Array(N + 3).fill(70),
      mutes: Array(N + 2).fill(true),
      solos: Array(N + 1).fill(true),
    };
    const mix = isolationToStemMix(iso);
    expect(mix.levels.length).toBe(N);
    expect(mix.mutes.length).toBe(N);
    expect(mix.solos.length).toBe(N);
  });
});
