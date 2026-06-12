import { describe, expect, it } from "vitest";
import { posToValue, valueToPos01 } from "./fader-math";

describe("posToValue", () => {
  it("maps track endpoints to min/max", () => {
    expect(posToValue(0, 0.25, 2, 0.05)).toBe(0.25);
    expect(posToValue(1, 0.25, 2, 0.05)).toBe(2);
  });

  it("rounds to the nearest step from min", () => {
    // 0..1 step 0.01: position 0.567 → 0.57
    expect(posToValue(0.567, 0, 1, 0.01)).toBeCloseTo(0.57, 12);
    // 0.25..2 step 0.05: midpoint 1.125 rounds onto the step grid
    expect(posToValue(0.5, 0.25, 2, 0.05)).toBeCloseTo(1.15, 12);
  });

  it("produces clean stepped values, no float drift", () => {
    // 0.25 + 9 * 0.05 must be exactly 0.7, not 0.7000000000000001
    expect(posToValue(9 / 35, 0.25, 2, 0.05)).toBe(0.7);
  });

  it("clamps out-of-range positions to the bounds", () => {
    expect(posToValue(-0.5, 0, 1, 0.01)).toBe(0);
    expect(posToValue(1.5, 0, 1, 0.01)).toBe(1);
  });

  it("clamps step rounding at the bounds", () => {
    // step does not divide the range: nearest step to pos=1 is 0.9, clamp keeps ≤ max
    expect(posToValue(0.99, 0, 1, 0.3)).toBeLessThanOrEqual(1);
    expect(posToValue(0.99, 0, 1, 0.3)).toBeCloseTo(0.9, 12);
  });

  it("ignores a non-positive step", () => {
    expect(posToValue(0.42, 0, 1, 0)).toBeCloseTo(0.42, 12);
  });

  it("guards min == max", () => {
    expect(posToValue(0.5, 1, 1, 0.1)).toBe(1);
    expect(posToValue(0.5, 2, 1, 0.1)).toBe(2);
  });
});

describe("valueToPos01", () => {
  it("maps min/max to 0/1", () => {
    expect(valueToPos01(0.25, 0.25, 2)).toBe(0);
    expect(valueToPos01(2, 0.25, 2)).toBe(1);
  });

  it("maps interior values proportionally", () => {
    expect(valueToPos01(0.5, 0, 1)).toBe(0.5);
    expect(valueToPos01(1.125, 0.25, 2)).toBeCloseTo(0.5, 12);
  });

  it("clamps out-of-range values", () => {
    expect(valueToPos01(-1, 0, 1)).toBe(0);
    expect(valueToPos01(3, 0, 1)).toBe(1);
  });

  it("guards min == max", () => {
    expect(valueToPos01(1, 1, 1)).toBe(0);
  });

  it("round-trips with posToValue on the step grid", () => {
    for (const v of [0.25, 0.7, 1.0, 1.55, 2]) {
      expect(posToValue(valueToPos01(v, 0.25, 2), 0.25, 2, 0.05)).toBe(v);
    }
  });
});
