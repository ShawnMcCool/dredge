import { describe, expect, it } from "vitest";
import { bisect, nextGrid, nudgeEdge, rateForRep, runUp, type Span } from "./drill";
import type { TempoCurve } from "./stores";

const span = (start: number, end: number): Span => ({ start, end });

describe("nextGrid", () => {
  const grid = [0, 1, 2, 3, 4];
  it("finds the next line forward", () => {
    expect(nextGrid(1.4, grid, 1, 0.25)).toBe(2);
  });
  it("finds the next line backward", () => {
    expect(nextGrid(1.4, grid, -1, 0.25)).toBe(1);
  });
  it("skips a line we are sitting exactly on", () => {
    expect(nextGrid(2, grid, 1, 0.25)).toBe(3);
    expect(nextGrid(2, grid, -1, 0.25)).toBe(1);
  });
  it("falls back to a fixed step with no grid", () => {
    expect(nextGrid(5, [], 1, 0.25)).toBe(5.25);
    expect(nextGrid(5, [], -1, 0.25)).toBe(4.75);
  });
  it("falls back past the last grid line", () => {
    expect(nextGrid(4, grid, 1, 0.25)).toBe(4.25);
  });
});

describe("nudgeEdge", () => {
  const grid = [0, 1, 2, 3, 4, 5, 6];
  it("nudges the end out to the next grid line", () => {
    expect(nudgeEdge(span(2, 4), "end", 1, grid, 6, 0.25)).toEqual(span(2, 5));
  });
  it("nudges the start in to the next grid line", () => {
    expect(nudgeEdge(span(2, 4), "start", 1, grid, 6, 0.25)).toEqual(span(3, 4));
  });
  it("never lets the start cross the end (min span)", () => {
    const out = nudgeEdge(span(3.9, 4), "start", 1, grid, 6, 0.25);
    expect(out.start).toBeLessThanOrEqual(out.end - 0.05 + 1e-9);
    expect(out.end).toBe(4);
  });
  it("clamps the end to the song duration", () => {
    const out = nudgeEdge(span(2, 5.9), "end", 1, grid, 6, 0.25);
    expect(out.end).toBeLessThanOrEqual(6);
  });
});

describe("bisect", () => {
  it("takes the first half", () => {
    expect(bisect(span(0, 8), "first")).toEqual(span(0, 4));
  });
  it("takes the second half", () => {
    expect(bisect(span(0, 8), "second")).toEqual(span(4, 8));
  });
  it("snaps the cut to the nearest interior grid line", () => {
    // midpoint 4.0 snaps to the nearby downbeat at 4.2
    expect(bisect(span(0, 8), "first", [0, 4.2, 8]).end).toBe(4.2);
  });
  it("ignores grid lines outside the span", () => {
    expect(bisect(span(2, 6), "first", [0, 10]).end).toBe(4);
  });
});

describe("runUp", () => {
  const downbeats = [0, 2, 4, 6, 8, 10];
  it("extends the start back by N downbeats", () => {
    expect(runUp(span(6, 8), 2, downbeats, 10)).toEqual(span(2, 8));
  });
  it("pulls the start forward with a negative delta", () => {
    expect(runUp(span(2, 8), -1, downbeats, 10)).toEqual(span(4, 8));
  });
  it("clamps at the first downbeat", () => {
    expect(runUp(span(2, 8), 5, downbeats, 10)).toEqual(span(0, 8));
  });
  it("never crosses the end when pulling forward", () => {
    const out = runUp(span(2, 8), -10, downbeats, 10);
    expect(out.start).toBeLessThanOrEqual(out.end - 0.05 + 1e-9);
    expect(out.end).toBe(8);
  });
  it("falls back to ~2s/bar with no downbeats", () => {
    expect(runUp(span(6, 8), 1, [], 10)).toEqual(span(4, 8));
  });
});

describe("rateForRep (mirrors practice::tempo)", () => {
  it("dwell is constant", () => {
    const c: TempoCurve = { curve: "dwell", rate: 0.9 };
    expect(rateForRep(c, 0)).toBe(0.9);
    expect(rateForRep(c, 99)).toBe(0.9);
  });
  it("ladder climbs and clamps at target", () => {
    const c: TempoCurve = { curve: "ladder", start: 0.6, step: 0.1, target: 0.9 };
    expect(rateForRep(c, 0)).toBeCloseTo(0.6);
    expect(rateForRep(c, 2)).toBeCloseTo(0.8);
    expect(rateForRep(c, 3)).toBeCloseTo(0.9);
    expect(rateForRep(c, 50)).toBeCloseTo(0.9);
  });
  it("oscillate touches high every period", () => {
    const c: TempoCurve = { curve: "oscillate", low: 0.7, high: 1.0, period: 3 };
    expect(rateForRep(c, 0)).toBe(0.7);
    expect(rateForRep(c, 1)).toBe(0.7);
    expect(rateForRep(c, 2)).toBe(1.0);
    expect(rateForRep(c, 5)).toBe(1.0);
  });
  it("oscillate period zero is treated as one", () => {
    const c: TempoCurve = { curve: "oscillate", low: 0.7, high: 1.0, period: 0 };
    expect(rateForRep(c, 0)).toBe(1.0);
  });
  it("clamps to [0.25, 2.0]", () => {
    expect(rateForRep({ curve: "dwell", rate: 5 }, 0)).toBe(2.0);
    expect(rateForRep({ curve: "dwell", rate: 0.01 }, 0)).toBe(0.25);
  });
});
