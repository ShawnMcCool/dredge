import { describe, expect, it } from "vitest";
import { meterNumerator } from "./meter";

describe("meterNumerator", () => {
  it("returns beats per bar for a 4/4 grid", () => {
    expect(meterNumerator({ beats: Array(16).fill(0), downbeats: Array(4).fill(0) })).toBe(4);
  });
  it("returns 3 for a 3/4 grid", () => {
    expect(meterNumerator({ beats: Array(12).fill(0), downbeats: Array(4).fill(0) })).toBe(3);
  });
  it("is null without a grid", () => {
    expect(meterNumerator({ beats: [], downbeats: [] })).toBeNull();
    expect(meterNumerator(null)).toBeNull();
  });
  it("is null for an implausible beats-per-bar", () => {
    // one downbeat for 40 beats → 40/bar, not a real meter
    expect(meterNumerator({ beats: Array(40).fill(0), downbeats: [0] })).toBeNull();
  });
});
