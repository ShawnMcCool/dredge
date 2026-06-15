import { describe, expect, it } from "vitest";
import { bisect, nextGrid, nudgeEdge, runUp, type Span } from "./drill";

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
