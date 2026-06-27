import { describe, it, expect } from "vitest";
import { migrateWorkspace } from "./workspace-migrate";
import type { Workspace } from "./dock";

const ALL = ["library", "a", "b"];
const keys = (ws: Workspace) => ({
  left: ws.left.layout.map((p) => p.tabs),
  right: ws.right.layout.map((p) => p.tabs),
});

describe("migrateWorkspace", () => {
  it("uses an existing workspace, reconciled", () => {
    const ws = migrateWorkspace(
      {
        workspace: {
          left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: true },
          right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false },
        },
      },
      ALL,
    );
    expect(keys(ws)).toEqual({ left: [["library"]], right: [["a", "b"]] });
    expect(ws.left.collapsed).toBe(true);
  });

  it("reconciles an existing workspace missing a new-in-code tab", () => {
    const ws = migrateWorkspace(
      {
        workspace: {
          left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: false },
          right: { layout: [{ tabs: ["a"], active: "a", weight: 1 }], collapsed: false },
        },
      },
      ALL,
    );
    expect(keys(ws).right).toEqual([["a", "b"]]);
  });

  it("migrates legacy panel_layout into right, library seeded left", () => {
    const ws = migrateWorkspace(
      {
        panel_layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }],
        library_collapsed: true,
        panels_collapsed: false,
      },
      ALL,
    );
    expect(keys(ws)).toEqual({ left: [["library"]], right: [["a", "b"]] });
    expect(ws.left.collapsed).toBe(true);
    expect(ws.right.collapsed).toBe(false);
  });

  it("falls back to default when nothing is stored", () => {
    expect(keys(migrateWorkspace({}, ALL))).toEqual({ left: [["library"]], right: [["a", "b"]] });
  });
});
