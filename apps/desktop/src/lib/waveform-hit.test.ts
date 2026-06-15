import { describe, expect, it } from "vitest";
import type { LoopRegion, OpenSong } from "./stores";
import {
  hitLaneSpan,
  hitLoopBody,
  hitLoopEdge,
  laneSpans,
  nearestLoopEdge,
  spanAtTime,
  type LaneSpan,
} from "./waveform-hit";
import type { View } from "./waveform-math";

// 10 px per second: secToX(s) = s*10, xToSec(x) = x/10.
const view: View = { startSec: 0, endSec: 100, width: 1000 };
const LANE_H = 24;

function loop(id: number, start: number, end: number): LoopRegion {
  return { id, song_id: 1, name: `loop ${id}`, name_override: null, start, end, kind: { kind: "manual" } };
}

function span(start: number, end: number, name = "x"): LaneSpan {
  return { name, start, end, suggested: false };
}

describe("hitLoopBody", () => {
  const loops = [loop(1, 10, 20), loop(2, 15, 25)];

  it("returns null above the lane line", () => {
    expect(hitLoopBody(loops, view, 180, LANE_H - 1, LANE_H)).toBeNull();
  });

  it("returns the topmost (last) loop under the point", () => {
    // sec 18 (x=180) is inside both loops; the later one wins
    expect(hitLoopBody(loops, view, 180, 50, LANE_H)?.id).toBe(2);
  });

  it("returns null when no loop is under the point", () => {
    expect(hitLoopBody(loops, view, 5, 50, LANE_H)).toBeNull();
  });
});

describe("hitLoopEdge", () => {
  const loops = [loop(1, 10, 20)];

  it("hits the start edge within the pixel radius", () => {
    expect(hitLoopEdge(loops, view, 102, 4)).toEqual({ loop: loops[0], edge: "start" });
  });

  it("hits the end edge within the pixel radius", () => {
    expect(hitLoopEdge(loops, view, 198, 4)).toEqual({ loop: loops[0], edge: "end" });
  });

  it("misses when outside the radius", () => {
    expect(hitLoopEdge(loops, view, 150, 4)).toBeNull();
  });
});

describe("nearestLoopEdge", () => {
  const loops = [loop(1, 10, 20), loop(2, 50, 60)];

  it("returns the closest edge across all loops", () => {
    // x=210 (sec 21): closest is loop1.end at x=200
    expect(nearestLoopEdge(loops, view, 210)).toEqual({ loop: loops[0], edge: "end" });
  });

  it("returns null with no loops", () => {
    expect(nearestLoopEdge([], view, 100)).toBeNull();
  });
});

describe("spanAtTime", () => {
  const spans = [span(0, 10), span(10, 20)];

  it("finds the span containing the time", () => {
    expect(spanAtTime(spans, 5)).toEqual({ start: 0, end: 10 });
  });

  it("returns null outside all spans", () => {
    expect(spanAtTime(spans, 25)).toBeNull();
  });
});

describe("hitLaneSpan", () => {
  const spans = [span(0, 10), span(10, 20)];

  it("returns null below the lane band", () => {
    expect(hitLaneSpan(spans, view, 50, LANE_H, LANE_H)).toBeNull();
  });

  it("returns the span under the point inside the lane band", () => {
    // x=50 (sec 5), y in lane band
    expect(hitLaneSpan(spans, view, 50, 5, LANE_H)?.start).toBe(0);
  });
});

describe("laneSpans", () => {
  it("maps saved sections (not suggested)", () => {
    const open = {
      sections: [{ id: 1, song_id: 1, name: "verse", start: 0, end: 8, position: 0 }],
      analysis: { sections: [{ label: "intro", start: 0, end: 4 }] },
    } as unknown as OpenSong;
    const out = laneSpans(open);
    expect(out).toEqual([{ name: "verse", start: 0, end: 8, suggested: false }]);
  });

  it("falls back to analysis suggestions when no saved sections", () => {
    const open = {
      sections: [],
      analysis: { sections: [{ label: "intro", start: 0, end: 4 }] },
    } as unknown as OpenSong;
    expect(laneSpans(open)).toEqual([{ name: "intro", start: 0, end: 4, suggested: true }]);
  });

  it("returns [] when neither sections nor analysis exist", () => {
    const open = { sections: [], analysis: null } as unknown as OpenSong;
    expect(laneSpans(open)).toEqual([]);
  });
});
