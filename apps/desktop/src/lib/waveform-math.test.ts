import { describe, expect, it } from "vitest";
import {
  playheadSecs,
  secToX,
  snapToGrid,
  visibleBuckets,
  xToSec,
  zoom,
  type View,
} from "./waveform-math";

const view: View = { startSec: 10, endSec: 30, width: 800 };

describe("secToX / xToSec", () => {
  it("maps view edges to canvas edges", () => {
    expect(secToX(view, 10)).toBe(0);
    expect(secToX(view, 30)).toBe(800);
    expect(secToX(view, 20)).toBe(400);
  });

  it("round-trips", () => {
    for (const s of [10, 13.37, 22.5, 30]) {
      expect(xToSec(view, secToX(view, s))).toBeCloseTo(s, 9);
    }
    for (const x of [0, 123, 400, 800]) {
      expect(secToX(view, xToSec(view, x))).toBeCloseTo(x, 9);
    }
  });
});

describe("playheadSecs", () => {
  it("extrapolates by wall time times rate", () => {
    const t0 = 5000;
    const pos = { secs: 10, rate: 0.75, playing: true, at: t0 };
    expect(playheadSecs(pos, t0 + 2000)).toBeCloseTo(11.5, 9);
  });

  it("freezes when paused", () => {
    const pos = { secs: 10, rate: 0.75, playing: false, at: 5000 };
    expect(playheadSecs(pos, 9000)).toBe(10);
  });
});

describe("zoom", () => {
  const duration = 100;

  it("keeps the anchor's pixel position stable", () => {
    const anchor = 22.5;
    const xBefore = secToX(view, anchor);
    const zoomed = zoom(view, anchor, 0.5, duration);
    expect(secToX(zoomed, anchor)).toBeCloseTo(xBefore, 6);
    expect(zoomed.endSec - zoomed.startSec).toBeCloseTo(10, 9);
  });

  it("clamps at song bounds", () => {
    const out = zoom({ startSec: 0, endSec: 90, width: 800 }, 80, 2, duration);
    expect(out.startSec).toBeGreaterThanOrEqual(0);
    expect(out.endSec).toBeLessThanOrEqual(duration);
    expect(out.endSec - out.startSec).toBeCloseTo(duration, 9);
  });

  it("clamps at the 2 s minimum span", () => {
    const tight: View = { startSec: 20, endSec: 22.5, width: 800 };
    const out = zoom(tight, 21, 0.1, duration);
    expect(out.endSec - out.startSec).toBeCloseTo(2, 9);
  });

  it("never moves the window start below zero", () => {
    const out = zoom({ startSec: 0, endSec: 20, width: 800 }, 1, 2, duration);
    expect(out.startSec).toBeGreaterThanOrEqual(0);
  });
});

describe("snapToGrid", () => {
  // view: 20 s over 800 px → 40 px per second; threshold 10 px = 0.25 s
  const downbeats = [12, 14, 16, 18];

  it("snaps to the nearest downbeat within the pixel threshold", () => {
    expect(snapToGrid(14.2, downbeats, view, 10)).toBe(14);
    expect(snapToGrid(13.8, downbeats, view, 10)).toBe(14);
  });

  it("leaves values outside the threshold alone", () => {
    expect(snapToGrid(13.0, downbeats, view, 10)).toBe(13.0);
    expect(snapToGrid(14.6, downbeats, view, 10)).toBe(14.6);
  });

  it("threshold scales with zoom (px, not seconds)", () => {
    // 4× wider view → 10 px is 1 s, so 14.6 now snaps
    const wide: View = { startSec: 0, endSec: 80, width: 800 };
    expect(snapToGrid(14.6, downbeats, wide, 10)).toBe(14);
  });

  it("exact downbeat stays put", () => {
    expect(snapToGrid(16, downbeats, view, 10)).toBe(16);
  });

  it("no downbeats → identity", () => {
    expect(snapToGrid(13.9, [], view, 10)).toBe(13.9);
  });
});

describe("visibleBuckets", () => {
  // 1024 frames per bucket at 48 kHz ≈ 21.33 ms per bucket
  const fpb = 1024;
  const sr = 48000;

  it("returns the bucket range covering the view", () => {
    const v: View = { startSec: 1, endSec: 2, width: 800 };
    const { first, last } = visibleBuckets(v, fpb, sr, 10000);
    expect(first).toBe(Math.floor((1 * sr) / fpb));
    expect(last).toBe(Math.ceil((2 * sr) / fpb));
  });

  it("clamps to [0, totalBuckets - 1]", () => {
    const v: View = { startSec: -5, endSec: 999, width: 800 };
    const { first, last } = visibleBuckets(v, fpb, sr, 100);
    expect(first).toBe(0);
    expect(last).toBe(99);
  });
});
