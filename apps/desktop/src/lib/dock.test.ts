import { describe, it, expect } from "vitest";
import { defaultLayout, fromTabOrder, reconcile, moveTab, splitTab, setActive, setWeights } from "./dock";

const ALL = ["a", "b", "c", "d"];
const keys = (layout: { tabs: string[] }[]) => layout.map((p) => p.tabs);

describe("defaultLayout", () => {
  it("is one panel with every tab", () => {
    const l = defaultLayout(ALL);
    expect(l).toHaveLength(1);
    expect(l[0].tabs).toEqual(ALL);
    expect(l[0].active).toBe("a");
    expect(l[0].weight).toBe(1);
  });
});

describe("fromTabOrder", () => {
  it("migrates a flat order to a single panel", () => {
    const l = fromTabOrder(["c", "a", "b", "d"], ALL);
    expect(l).toHaveLength(1);
    expect(l[0].tabs).toEqual(["c", "a", "b", "d"]);
  });
  it("reconciles a short/stale order against the known tabs", () => {
    const l = fromTabOrder(["b"], ALL);
    expect(l[0].tabs).toEqual(["b", "a", "c", "d"]); // missing appended
  });
});

describe("reconcile", () => {
  it("keeps a valid two-panel layout and normalizes weights to sum 1", () => {
    const l = reconcile(
      [
        { tabs: ["a", "b"], active: "b", weight: 3 },
        { tabs: ["c", "d"], active: "c", weight: 1 },
      ],
      ALL,
    );
    expect(keys(l)).toEqual([["a", "b"], ["c", "d"]]);
    expect(l[0].active).toBe("b");
    expect(l.reduce((s, p) => s + p.weight, 0)).toBeCloseTo(1);
    expect(l[0].weight).toBeCloseTo(0.75);
  });
  it("drops duplicates (first wins) and unknown tabs, appends missing", () => {
    const l = reconcile(
      [
        { tabs: ["a", "b", "zzz"], active: "a", weight: 1 },
        { tabs: ["b", "c"], active: "b", weight: 1 }, // duplicate b dropped here
      ],
      ALL,
    );
    expect(keys(l)).toEqual([["a", "b"], ["c", "d"]]); // d appended to last panel
  });
  it("fixes an active that isn't in its panel", () => {
    const l = reconcile([{ tabs: ["a", "b", "c", "d"], active: "gone", weight: 1 }], ALL);
    expect(l[0].active).toBe("a");
  });
  it("falls back to the default when nothing valid remains", () => {
    const l = reconcile([{ tabs: ["x"], active: "x", weight: 1 }], ALL);
    expect(l).toEqual(defaultLayout(ALL));
  });
});

describe("moveTab", () => {
  const two = () =>
    reconcile(
      [
        { tabs: ["a", "b"], active: "a", weight: 1 },
        { tabs: ["c", "d"], active: "c", weight: 1 },
      ],
      ALL,
    );

  it("reorders within a panel, preserving the active tab", () => {
    const l = moveTab(two(), "b", 0, 0); // b to front of panel 0
    expect(keys(l)).toEqual([["b", "a"], ["c", "d"]]);
    expect(l[0].active).toBe("a"); // within-panel move doesn't steal focus
  });
  it("joins a tab into another panel and focuses it there", () => {
    const l = moveTab(two(), "a", 1, 0); // a into panel 1, front
    expect(keys(l)).toEqual([["b"], ["a", "c", "d"]]);
    expect(l[1].active).toBe("a");
  });
  it("drops a source panel that empties", () => {
    let l = two();
    l = moveTab(l, "a", 1, 0); // → [["b"], ["a","c","d"]]
    l = moveTab(l, "b", 1, 99); // b into panel 1 (end) → panel 0 empties, dropped
    expect(keys(l)).toEqual([["a", "c", "d", "b"]]);
    expect(l).toHaveLength(1);
  });
});

describe("splitTab", () => {
  it("splits a tab into a new panel at the bottom", () => {
    const l = splitTab(defaultLayout(ALL), "c", 1); // boundary 1 = below the one panel
    expect(keys(l)).toEqual([["a", "b", "d"], ["c"]]);
    expect(l[1].active).toBe("c");
  });
  it("splits at the top (boundary 0)", () => {
    const l = splitTab(defaultLayout(ALL), "c", 0);
    expect(keys(l)).toEqual([["c"], ["a", "b", "d"]]);
  });
  it("removes a panel when its last tab is split out", () => {
    const THREE = ["a", "b", "c"];
    let l = reconcile(
      [
        { tabs: ["a", "b"], active: "a", weight: 1 },
        { tabs: ["c"], active: "c", weight: 1 }, // single-tab panel
      ],
      THREE,
    );
    expect(keys(l)).toEqual([["a", "b"], ["c"]]);
    l = splitTab(l, "c", 2); // split c (its panel's only tab) to the bottom
    expect(keys(l)).toEqual([["a", "b"], ["c"]]); // old single panel gone, new at bottom
    expect(l).toHaveLength(2);
  });
});

describe("setActive / setWeights", () => {
  it("sets a panel's active tab", () => {
    const l = setActive(defaultLayout(ALL), 0, "c");
    expect(l[0].active).toBe("c");
  });
  it("ignores an active not in the panel", () => {
    const l = setActive(defaultLayout(ALL), 0, "nope");
    expect(l[0].active).toBe("a");
  });
  it("sets and renormalizes weights", () => {
    const l = setWeights(
      reconcile(
        [
          { tabs: ["a", "b"], active: "a", weight: 1 },
          { tabs: ["c", "d"], active: "c", weight: 1 },
        ],
        ALL,
      ),
      [3, 1],
    );
    expect(l[0].weight).toBeCloseTo(0.75);
    expect(l[1].weight).toBeCloseTo(0.25);
  });
});
