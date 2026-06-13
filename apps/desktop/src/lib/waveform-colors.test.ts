import { describe, expect, it } from "vitest";
import { KNOWN_LABELS, labelColor } from "./waveform-colors";

const alphaOf = (color: string): number => {
  const m = color.match(/hsla\(\s*[\d.]+,\s*[\d.]+%,\s*[\d.]+%,\s*([\d.]+)\s*\)/);
  expect(m, `not an hsla() color: ${color}`).not.toBeNull();
  return Number(m![1]);
};

const hueOf = (color: string): number => Number(color.match(/hsla\(\s*([\d.]+)/)![1]);

describe("labelColor", () => {
  it("gives every known SongFormer label a fixed, stable color", () => {
    // the 8-class inference set of SongFormer (SongForm-HX-8Class)
    expect(KNOWN_LABELS).toEqual([
      "intro",
      "verse",
      "chorus",
      "bridge",
      "inst",
      "outro",
      "silence",
      "pre-chorus",
    ]);
    for (const label of KNOWN_LABELS) {
      expect(labelColor(label)).toEqual(labelColor(label));
    }
    // distinct hues across the fixed set
    const hues = KNOWN_LABELS.map((l) => hueOf(labelColor(l).fill));
    expect(new Set(hues).size).toBe(KNOWN_LABELS.length);
  });

  it("keeps chorus in the amber accent family", () => {
    const hue = hueOf(labelColor("chorus").fill);
    expect(hue).toBeGreaterThanOrEqual(25);
    expect(hue).toBeLessThanOrEqual(45);
  });

  it("is case- and whitespace-insensitive", () => {
    expect(labelColor(" Chorus ")).toEqual(labelColor("chorus"));
  });

  it("maps unknown labels to a deterministic fallback hue", () => {
    expect(labelColor("A")).toEqual(labelColor("A"));
    expect(labelColor("B")).toEqual(labelColor("B"));
    expect(labelColor("A").fill).not.toEqual(labelColor("B").fill);
    expect(labelColor("breakdown")).toEqual(labelColor("breakdown"));
  });

  it("keeps all fills within the muted alpha bounds, edges stronger", () => {
    for (const label of [...KNOWN_LABELS, "A", "B", "C", "mainriff", "quietchorus"]) {
      const { fill, edge } = labelColor(label);
      const fillAlpha = alphaOf(fill);
      expect(fillAlpha).toBeGreaterThanOrEqual(0.1);
      expect(fillAlpha).toBeLessThanOrEqual(0.25);
      expect(alphaOf(edge)).toBeGreaterThan(fillAlpha);
    }
  });
});
