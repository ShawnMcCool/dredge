import { describe, expect, it } from "vitest";
import { framesToMs, msToFrames } from "./recording-math";

describe("nudge ms<->frames", () => {
  it("converts ms to frames at 48kHz", () => {
    expect(msToFrames(10)).toBe(480);
    expect(msToFrames(-10)).toBe(-480);
  });
  it("converts frames to ms", () => {
    expect(framesToMs(480)).toBeCloseTo(10, 9);
  });
  it("round-trips whole-ms values", () => {
    for (const ms of [-50, -5, 0, 5, 50]) {
      expect(framesToMs(msToFrames(ms))).toBeCloseTo(ms, 6);
    }
  });
});
