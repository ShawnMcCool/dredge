import { describe, expect, it } from "vitest";
import { hzToReading } from "./tuner-math";

describe("hzToReading", () => {
  it("maps A440 to A4, 0 cents", () => {
    expect(hzToReading(440)).toEqual({ note: "A", octave: 4, cents: 0 });
  });

  it("maps middle C (261.63) to C4 ~0 cents", () => {
    const r = hzToReading(261.63);
    expect(r.note).toBe("C");
    expect(r.octave).toBe(4);
    expect(Math.abs(r.cents)).toBeLessThanOrEqual(1);
  });

  it("maps A#4 (466.16) to A#4 ~0 cents", () => {
    const r = hzToReading(466.16);
    expect(r.note).toBe("A#");
    expect(r.octave).toBe(4);
    expect(Math.abs(r.cents)).toBeLessThanOrEqual(1);
  });

  it("maps low E (82.41) to E2", () => {
    const r = hzToReading(82.41);
    expect(r.note).toBe("E");
    expect(r.octave).toBe(2);
  });

  it("reports a sharp A as positive cents", () => {
    const r = hzToReading(448); // ~31 cents sharp of A4
    expect(r.note).toBe("A");
    expect(r.cents).toBeGreaterThan(20);
  });
});
