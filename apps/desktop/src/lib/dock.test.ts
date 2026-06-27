import { describe, it, expect } from "vitest";
import {
  defaultLayout,
  fromTabOrder,
  reconcile,
  moveTab,
  splitTab,
  setActive,
  setWeights,
  defaultWorkspace,
  reconcileWorkspace,
  moveTabTo,
  splitTabTo,
  setActiveIn,
  setCollapsed,
  type Workspace,
} from "./dock";

const ALL = ["a", "b", "c", "d"];
const keys = (layout: { tabs: string[] }[]) => layout.map((p) => p.tabs);
const wkeys = (ws: Workspace) => ({
  left: ws.left.layout.map((p) => p.tabs),
  right: ws.right.layout.map((p) => p.tabs),
});

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

// ── workspace (two regions) ──────────────────────────────────────────────────

describe("defaultWorkspace", () => {
  it("seeds the first tab left, the rest right, both expanded", () => {
    const ws = defaultWorkspace(["library", "a", "b"]);
    expect(wkeys(ws)).toEqual({ left: [["library"]], right: [["a", "b"]] });
    expect(ws.left.collapsed).toBe(false);
    expect(ws.right.collapsed).toBe(false);
  });
});

describe("reconcileWorkspace", () => {
  it("keeps each known tab exactly once across both regions", () => {
    const ws = reconcileWorkspace(
      {
        left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: false },
        right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false },
      },
      ["library", "a", "b"],
    );
    expect(wkeys(ws)).toEqual({ left: [["library"]], right: [["a", "b"]] });
  });
  it("drops a tab duplicated across regions (first occurrence wins)", () => {
    const ws = reconcileWorkspace(
      {
        left: { layout: [{ tabs: ["a"], active: "a", weight: 1 }], collapsed: false },
        right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false },
      },
      ["a", "b"],
    );
    expect(wkeys(ws)).toEqual({ left: [["a"]], right: [["b"]] });
  });
  it("appends tabs new-in-code to right's last panel", () => {
    const ws = reconcileWorkspace(
      {
        left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: false },
        right: { layout: [{ tabs: ["a"], active: "a", weight: 1 }], collapsed: false },
      },
      ["library", "a", "b"],
    );
    expect(wkeys(ws).right).toEqual([["a", "b"]]);
  });
  it("allows an empty region and passes its collapse flag through", () => {
    const ws = reconcileWorkspace(
      {
        left: { layout: [], collapsed: true },
        right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false },
      },
      ["a", "b"],
    );
    expect(wkeys(ws).left).toEqual([]);
    expect(ws.left.collapsed).toBe(true);
  });
  it("defaults when nothing valid remains", () => {
    const ws = reconcileWorkspace(
      { left: { layout: [], collapsed: false }, right: { layout: [], collapsed: false } },
      ["library", "a"],
    );
    expect(wkeys(ws)).toEqual({ left: [["library"]], right: [["a"]] });
  });
});

const baseWs = (): Workspace => ({
  left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: false },
  right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false },
});

describe("moveTabTo", () => {
  it("moves a tab from right to left", () => {
    const ws = moveTabTo(baseWs(), "a", "left", 0, 1);
    expect(wkeys(ws)).toEqual({ left: [["library", "a"]], right: [["b"]] });
    expect(ws.left.layout[0].active).toBe("a");
  });
  it("within-region reorder leaves the other region untouched", () => {
    const ws = moveTabTo(baseWs(), "b", "right", 0, 0);
    expect(wkeys(ws)).toEqual({ left: [["library"]], right: [["b", "a"]] });
  });
  it("moving the last tab out leaves the source region empty", () => {
    const ws = moveTabTo(baseWs(), "library", "right", 0, 0);
    expect(wkeys(ws).left).toEqual([]);
    expect(wkeys(ws).right).toEqual([["library", "a", "b"]]);
  });
  it("moving into an empty region creates a panel there", () => {
    const start: Workspace = {
      left: { layout: [], collapsed: false },
      right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false },
    };
    const ws = moveTabTo(start, "a", "left", 0, 0);
    expect(wkeys(ws)).toEqual({ left: [["a"]], right: [["b"]] });
  });
});

describe("splitTabTo", () => {
  it("splits a tab into a new panel in the target region", () => {
    const ws = splitTabTo(baseWs(), "a", "left", 1);
    expect(wkeys(ws).left).toEqual([["library"], ["a"]]);
    expect(wkeys(ws).right).toEqual([["b"]]);
  });
  it("within-region split keeps it on one side", () => {
    const ws = splitTabTo(baseWs(), "b", "right", 0);
    expect(wkeys(ws).right).toEqual([["b"], ["a"]]);
    expect(wkeys(ws).left).toEqual([["library"]]);
  });
});

describe("setActiveIn / setCollapsed", () => {
  it("sets the active tab in one region", () => {
    expect(setActiveIn(baseWs(), "right", 0, "b").right.layout[0].active).toBe("b");
  });
  it("sets collapse on one region", () => {
    expect(setCollapsed(baseWs(), "left", true).left.collapsed).toBe(true);
  });
});
