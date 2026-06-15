import { describe, expect, it } from "vitest";
import { hexToHue, KNOWN_LABELS, labelColor } from "./waveform-colors";

const alphaOf = (color: string): number => {
  const m = color.match(/hsla\(\s*[\d.]+,\s*[\d.]+%,\s*[\d.]+%,\s*([\d.]+)\s*\)/);
  expect(m, `not an hsla() color: ${color}`).not.toBeNull();
  return Number(m![1]);
};

const hueOf = (color: string): number => Number(color.match(/hsla\(\s*([\d.]+)/)![1]);

// circular distance between two hues, 0–180
const hueDist = (a: number, b: number): number => {
  const d = Math.abs(a - b) % 360;
  return d > 180 ? 360 - d : d;
};

const AMBER = 36;
const CYAN = 188;

describe("labelColor", () => {
  it("gives every known SongFormer label a stable color for a fixed accent", () => {
    expect(KNOWN_LABELS).toEqual([
      "chorus",
      "pre-chorus",
      "verse",
      "bridge",
      "intro",
      "outro",
      "inst",
      "silence",
    ]);
    for (const label of KNOWN_LABELS) {
      expect(labelColor(label, AMBER)).toEqual(labelColor(label, AMBER));
    }
    // distinct appearance across the fixed set (silence shares chorus's hue but
    // is near-grey, so distinctness is by the full colour, not hue alone)
    const fills = KNOWN_LABELS.map((l) => labelColor(l, AMBER).fill);
    expect(new Set(fills).size).toBe(KNOWN_LABELS.length);
  });

  it("makes chorus ride the accent hue (whatever the theme)", () => {
    expect(hueDist(hueOf(labelColor("chorus", AMBER).fill), AMBER)).toBeLessThan(1);
    expect(hueDist(hueOf(labelColor("chorus", CYAN).fill), CYAN)).toBeLessThan(1);
  });

  it("derives hues from the accent — the lane re-tints when it changes", () => {
    const verseAmber = hueOf(labelColor("verse", AMBER).fill);
    const verseCyan = hueOf(labelColor("verse", CYAN).fill);
    expect(verseAmber).not.toEqual(verseCyan);
    // each type keeps its offset from the base, so the gap is preserved
    expect(hueDist(verseAmber, AMBER)).toBeCloseTo(hueDist(verseCyan, CYAN), 5);
  });

  it("is case- and whitespace-insensitive", () => {
    expect(labelColor(" Chorus ", AMBER)).toEqual(labelColor("chorus", AMBER));
  });

  it("maps unknown labels to a deterministic fallback hue", () => {
    expect(labelColor("A", AMBER)).toEqual(labelColor("A", AMBER));
    expect(labelColor("A", AMBER).fill).not.toEqual(labelColor("B", AMBER).fill);
    expect(labelColor("breakdown", AMBER)).toEqual(labelColor("breakdown", AMBER));
  });

  it("keeps all fills within the muted alpha bounds, edges stronger", () => {
    for (const label of [...KNOWN_LABELS, "A", "B", "C", "mainriff", "quietchorus"]) {
      const { fill, edge } = labelColor(label, AMBER);
      const fillAlpha = alphaOf(fill);
      expect(fillAlpha).toBeGreaterThanOrEqual(0.1);
      expect(fillAlpha).toBeLessThanOrEqual(0.25);
      expect(alphaOf(edge)).toBeGreaterThan(fillAlpha);
    }
  });
});

describe("hexToHue", () => {
  it("reads the hue of a #rrggbb accent", () => {
    expect(hueDist(hexToHue("#e0a458"), 33)).toBeLessThan(3); // amber
    expect(hueDist(hexToHue("#4fc3d4"), 188)).toBeLessThan(3); // cyan
  });

  it("tolerates a missing # and casing", () => {
    expect(hexToHue("E0A458")).toBeCloseTo(hexToHue("#e0a458"), 5);
  });

  it("falls back to amber's hue on bad input", () => {
    expect(hexToHue("")).toBe(36);
    expect(hexToHue("not-a-color")).toBe(36);
  });
});
