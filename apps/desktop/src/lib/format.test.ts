import { describe, expect, it } from "vitest";
import { fmtClock, fmtDur, fmtElapsed } from "./format";

describe("fmtDur", () => {
  it("formats sub-minute durations with unpadded minutes", () => {
    expect(fmtDur(0)).toBe("0:00");
    expect(fmtDur(5)).toBe("0:05");
    expect(fmtDur(59)).toBe("0:59");
  });

  it("formats multi-minute durations", () => {
    expect(fmtDur(60)).toBe("1:00");
    expect(fmtDur(125)).toBe("2:05");
    expect(fmtDur(605)).toBe("10:05");
  });

  it("floors seconds by default", () => {
    expect(fmtDur(65.9)).toBe("1:05");
  });

  it("rounds seconds when asked", () => {
    expect(fmtDur(65.9, true)).toBe("1:06");
    expect(fmtDur(65.4, true)).toBe("1:05");
  });

  it("clamps negatives to zero", () => {
    expect(fmtDur(-3)).toBe("0:00");
  });
});

describe("fmtClock", () => {
  it("renders a padded mm:ss.s playhead by default", () => {
    expect(fmtClock(0)).toBe("00:00.0");
    expect(fmtClock(5)).toBe("00:05.0");
    expect(fmtClock(65.4)).toBe("01:05.4");
    expect(fmtClock(605.2)).toBe("10:05.2");
  });

  it("renders a whole-second total with decimals=0", () => {
    expect(fmtClock(0, 0)).toBe("00:00");
    expect(fmtClock(65.9, 0)).toBe("01:05");
    expect(fmtClock(605, 0)).toBe("10:05");
  });

  it("clamps negatives to zero", () => {
    expect(fmtClock(-1)).toBe("00:00.0");
  });
});

describe("fmtElapsed", () => {
  it("uses milliseconds under a second", () => {
    expect(fmtElapsed(0)).toBe("0 ms");
    expect(fmtElapsed(999)).toBe("999 ms");
  });

  it("uses seconds with one decimal under a minute", () => {
    expect(fmtElapsed(1000)).toBe("1.0 s");
    expect(fmtElapsed(5500)).toBe("5.5 s");
    expect(fmtElapsed(59900)).toBe("59.9 s");
  });

  it("uses the m:ss clock at and beyond a minute", () => {
    expect(fmtElapsed(60000)).toBe("1:00");
    expect(fmtElapsed(125000)).toBe("2:05");
  });
});
