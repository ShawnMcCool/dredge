import { describe, expect, it } from "vitest";
import { deriveLoopName } from "./loop-name";
import type { Section } from "./stores";

function sec(id: number, name: string, start: number, end: number, position: number): Section {
  return { id, song_id: 1, name, start, end, position };
}

// intro[0,10] verse[10,30] verse[30,50] chorus[50,70] — ports naming.rs tests.
const song = (): Section[] => [
  sec(1, "intro", 0, 10, 0),
  sec(2, "verse", 10, 30, 1),
  sec(3, "verse", 30, 50, 2),
  sec(4, "chorus", 50, 70, 3),
];

describe("deriveLoopName", () => {
  it("full single section uses occurrence", () => {
    expect(deriveLoopName(30, 50, song())).toBe("verse 2");
  });
  it("first occurrence is one", () => {
    expect(deriveLoopName(0, 10, song())).toBe("intro 1");
  });
  it("strict subset is sub", () => {
    expect(deriveLoopName(34, 46, song())).toBe("sub verse 2");
  });
  it("spans multiple names: first and last", () => {
    expect(deriveLoopName(30, 70, song())).toBe("verse 2 → chorus 1");
  });
  it("partial end section is sub", () => {
    expect(deriveLoopName(30, 60, song())).toBe("verse 2 → sub chorus 1");
  });
  it("partial start section is sub", () => {
    expect(deriveLoopName(40, 70, song())).toBe("sub verse 2 → chorus 1");
  });
  it("middle sections dropped", () => {
    expect(deriveLoopName(0, 70, song())).toBe("intro 1 → chorus 1");
  });
  it("boundary within eps reads as full", () => {
    expect(deriveLoopName(30.02, 49.97, song())).toBe("verse 2");
  });
  it("no section falls back to timestamp", () => {
    expect(deriveLoopName(83, 105.2, [])).toBe("riff 1:23.0–1:45.2");
  });
  it("collision gets numeric suffix", () => {
    expect(deriveLoopName(30, 50, song(), ["verse 2"])).toBe("verse 2 (2)");
  });
  it("collision skips taken suffixes", () => {
    expect(deriveLoopName(30, 50, song(), ["verse 2", "verse 2 (2)"])).toBe("verse 2 (3)");
  });
});
