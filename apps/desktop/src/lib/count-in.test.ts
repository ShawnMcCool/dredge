import { describe, expect, it } from "vitest";
import { stepCountInBeats } from "./count-in";

describe("stepCountInBeats", () => {
  it("clamps to 0..8 and 0 means off", () => {
    expect(stepCountInBeats(4, -1)).toBe(3);
    expect(stepCountInBeats(4, 1)).toBe(5);
    expect(stepCountInBeats(0, -1)).toBe(0);
    expect(stepCountInBeats(8, 1)).toBe(8);
  });
});
