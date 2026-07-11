import { describe, expect, it } from "vitest";
import { gaugeWindow, pressure, tracePoints, traceWindow } from "./meter-math";

describe("pressure", () => {
  it("maps capacity fractions onto the shared color language", () => {
    expect(pressure(0)).toBe("ok");
    expect(pressure(0.71)).toBe("ok");
    expect(pressure(0.72)).toBe("warm");
    expect(pressure(0.89)).toBe("warm");
    expect(pressure(0.9)).toBe("hot");
    expect(pressure(1)).toBe("hot");
  });
});

describe("traceWindow", () => {
  it("pads the run's min–max by 12% of span plus 0.3% of capacity", () => {
    const w = traceWindow(400, 600, 1000);
    expect(w.lo).toBeCloseTo(400 - (200 * 0.12 + 3));
    expect(w.hi).toBeCloseTo(600 + (200 * 0.12 + 3));
  });

  it("clamps to [0, total]", () => {
    const w = traceWindow(1, 999, 1000);
    expect(w.lo).toBe(0);
    expect(w.hi).toBe(1000);
  });

  it("a flat run still gets a nonzero window", () => {
    const w = traceWindow(500, 500, 1000);
    expect(w.hi).toBeGreaterThan(w.lo);
    expect(w.lo).toBeLessThan(500);
    expect(w.hi).toBeGreaterThan(500);
  });

  it("never degenerates, even at the extremes", () => {
    const zero = traceWindow(0, 0, 1000);
    expect(zero.hi).toBeGreaterThan(zero.lo);
    const full = traceWindow(1000, 1000, 1000);
    expect(full.hi).toBeGreaterThan(full.lo);
  });
});

describe("tracePoints", () => {
  it("yields x = sample index, y descending as usage rises", () => {
    const pts = tracePoints([0, 50, 100], { lo: 0, hi: 100 });
    expect(pts).toBe("0,100.00 1,50.00 2,0.00");
  });

  it("clamps samples outside the window to the edges", () => {
    const pts = tracePoints([-10, 200], { lo: 0, hi: 100 });
    expect(pts).toBe("0,100.00 1,0.00");
  });

  it("returns empty below two samples — no line to draw", () => {
    expect(tracePoints([], { lo: 0, hi: 100 })).toBe("");
    expect(tracePoints([42], { lo: 0, hi: 100 })).toBe("");
  });
});

describe("gaugeWindow", () => {
  it("marks the min–peak band within 0..total, top-down", () => {
    const g = gaugeWindow(250, 500, 1000);
    expect(g.y).toBeCloseTo(50);
    expect(g.h).toBeCloseTo(25);
  });

  it("keeps a 2-unit floor so a flat run stays visible", () => {
    expect(gaugeWindow(500, 500, 1000).h).toBe(2);
  });
});
