import { describe, expect, it } from "vitest";
import { clampBpm, tapTempo, type TapState } from "./metronome";

describe("clampBpm", () => {
  it("clamps to 30..300 and rounds", () => {
    expect(clampBpm(12)).toBe(30);
    expect(clampBpm(999)).toBe(300);
    expect(clampBpm(120.4)).toBe(120);
  });
});

describe("tapTempo", () => {
  it("returns no bpm on the first tap", () => {
    const s: TapState = { taps: [] };
    const r = tapTempo(s, 1000);
    expect(r.bpm).toBeNull();
    expect(r.state.taps).toEqual([1000]);
  });

  it("computes bpm from steady 500ms taps (120 bpm)", () => {
    let s: TapState = { taps: [] };
    let bpm: number | null = null;
    for (const t of [0, 500, 1000, 1500]) {
      const r = tapTempo(s, t);
      s = r.state;
      bpm = r.bpm;
    }
    expect(bpm).toBe(120);
  });

  it("resets the window after a long gap", () => {
    const s: TapState = { taps: [0, 500, 1000] };
    const r = tapTempo(s, 1000 + 5000); // 5s later → fresh window
    expect(r.bpm).toBeNull();
    expect(r.state.taps).toEqual([6000]);
  });

  it("averages only the last few taps (rolling window)", () => {
    // many fast taps then the window only keeps the last 4
    let s: TapState = { taps: [] };
    let r = tapTempo(s, 0); s = r.state;
    r = tapTempo(s, 250); s = r.state;
    r = tapTempo(s, 500); s = r.state;
    r = tapTempo(s, 750); s = r.state;
    r = tapTempo(s, 1000); s = r.state;
    expect(s.taps.length).toBeLessThanOrEqual(4);
    expect(r.bpm).toBe(240); // 250ms intervals → 240 bpm
  });
});
