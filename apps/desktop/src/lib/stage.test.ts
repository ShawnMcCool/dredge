import { describe, it, expect } from "vitest";
import { STAGE_BOXES, BOX_LABELS, defaultFlow, reconcileFlow, moveBox, toggleCollapsed, hide, show } from "./stage";

describe("defaultFlow", () => {
  it("is the canonical order, tuner collapsed, nothing hidden", () => {
    const f = defaultFlow();
    expect(f.order).toEqual([...STAGE_BOXES]);
    expect(f.collapsed).toEqual(["tuner"]);
    expect(f.hidden).toEqual([]);
  });
});

describe("BOX_LABELS", () => {
  it("names every box", () => {
    for (const id of STAGE_BOXES) expect(typeof BOX_LABELS[id]).toBe("string");
  });
});

describe("reconcileFlow", () => {
  it("keeps known order, drops unknown, appends missing, prunes collapsed + hidden", () => {
    const f = reconcileFlow(
      { order: ["tuner", "zzz", "metronome"], collapsed: ["zzz", "tuner"], hidden: ["metronome", "qqq"] },
      ["metronome", "tuner", "drill"],
    );
    expect(f.order).toEqual(["tuner", "metronome", "drill"]); // unknown dropped, missing appended
    expect(f.collapsed).toEqual(["tuner"]); // unknown collapsed pruned
    expect(f.hidden).toEqual(["metronome"]); // unknown hidden pruned
  });
  it("defaults from an empty/garbage flow", () => {
    expect(reconcileFlow({ order: [], collapsed: [] }, ["metronome", "tuner"])).toEqual({
      order: ["metronome", "tuner"],
      collapsed: [],
      hidden: [],
    });
  });
  it("seeds hidden when the stored flow predates it", () => {
    expect(reconcileFlow({ order: ["tuner"], collapsed: [] }, ["metronome", "tuner"]).hidden).toEqual([]);
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
    const a = toggleCollapsed({ order: ["x"], collapsed: [], hidden: [] } as never, "x" as never);
    expect(a.collapsed).toEqual(["x"]);
    expect(toggleCollapsed(a, "x" as never).collapsed).toEqual([]);
  });
});

describe("hide / show", () => {
  it("hide is idempotent and leaves collapse untouched", () => {
    const base = { order: ["tuner"], collapsed: ["tuner"], hidden: [] } as never;
    const h = hide(base, "tuner" as never);
    expect(h.hidden).toEqual(["tuner"]);
    expect(h.collapsed).toEqual(["tuner"]); // collapse is remembered across a hide
    expect(hide(h, "tuner" as never)).toBe(h); // idempotent → same object
  });
  it("show removes from hidden and is idempotent", () => {
    const hidden = { order: ["tuner"], collapsed: [], hidden: ["tuner"] } as never;
    const s = show(hidden, "tuner" as never);
    expect(s.hidden).toEqual([]);
    expect(show(s, "tuner" as never)).toBe(s); // already shown → same object
  });
});
