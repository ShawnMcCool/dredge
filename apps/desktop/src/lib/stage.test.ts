import { describe, it, expect } from "vitest";
import { STAGE_BOXES, defaultFlow, reconcileFlow, moveBox, toggleCollapsed } from "./stage";

describe("defaultFlow", () => {
  it("is the canonical order, nothing collapsed", () => {
    const f = defaultFlow();
    expect(f.order).toEqual([...STAGE_BOXES]);
    expect(f.collapsed).toEqual([]);
  });
});

describe("reconcileFlow", () => {
  it("keeps known order, drops unknown, appends missing, prunes collapsed", () => {
    const f = reconcileFlow({ order: ["tuner", "zzz", "metronome"], collapsed: ["zzz", "tuner"] }, [
      "metronome",
      "tuner",
      "drill",
    ]);
    expect(f.order).toEqual(["tuner", "metronome", "drill"]); // unknown dropped, missing appended
    expect(f.collapsed).toEqual(["tuner"]); // unknown collapsed pruned
  });
  it("defaults from an empty/garbage flow", () => {
    expect(reconcileFlow({ order: [], collapsed: [] }, ["metronome", "tuner"])).toEqual({
      order: ["metronome", "tuner"],
      collapsed: [],
    });
  });
  it("dedupes a repeated id in the stored order", () => {
    expect(reconcileFlow({ order: ["tuner", "tuner", "metronome"], collapsed: [] }, ["metronome", "tuner"]).order).toEqual([
      "tuner",
      "metronome",
    ]);
  });
});

describe("moveBox", () => {
  it("reorders to a target index", () => {
    expect(moveBox(["a", "b", "c"] as never, "c" as never, 0)).toEqual(["c", "a", "b"]);
    expect(moveBox(["a", "b", "c"] as never, "a" as never, 2)).toEqual(["b", "c", "a"]);
  });
  it("is a no-op for an unknown id", () => {
    expect(moveBox(["a", "b"] as never, "x" as never, 0)).toEqual(["a", "b"]);
  });
});

describe("toggleCollapsed", () => {
  it("adds then removes an id from the set", () => {
    const a = toggleCollapsed({ order: ["x"] as never, collapsed: [] as never }, "x" as never);
    expect(a.collapsed).toEqual(["x"]);
    expect(toggleCollapsed(a, "x" as never).collapsed).toEqual([]);
  });
});
