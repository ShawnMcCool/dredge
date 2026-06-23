import { describe, expect, it } from "vitest";
import { stepCountInBeats } from "./count-in";

describe("stepCountInBeats", () => {
  it("clamps to 1..8 (never zero — on/off is separate)", () => {
    expect(stepCountInBeats(4, -1)).toBe(3);
    expect(stepCountInBeats(4, 1)).toBe(5);
    expect(stepCountInBeats(1, -1)).toBe(1);
    expect(stepCountInBeats(8, 1)).toBe(8);
  });
});
