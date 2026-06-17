import { describe, expect, it } from "vitest";
import { activeLabel, type SectionSpan } from "./active-section";

const secs: SectionSpan[] = [
  { label: "intro 1", start: 0, end: 10 },
  { label: "verse 1", start: 10, end: 20 },
  { label: "verse 2", start: 20, end: 30 },
];

describe("activeLabel", () => {
  it("follows the playhead when nothing is pinned", () => {
    expect(activeLabel(secs, 5, null)).toBe("intro 1");
    expect(activeLabel(secs, 25, null)).toBe("verse 2");
  });
  it("returns the pinned label regardless of playhead", () => {
    expect(activeLabel(secs, 5, "verse 2")).toBe("verse 2");
  });
  it("falls back to the playhead when the pin no longer exists", () => {
    expect(activeLabel(secs, 25, "bridge 1")).toBe("verse 2");
  });
  it("clamps to the first section past the end / before the start", () => {
    expect(activeLabel(secs, 999, null)).toBe("intro 1");
  });
  it("returns null with no sections", () => {
    expect(activeLabel([], 5, null)).toBeNull();
  });
});
