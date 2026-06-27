# Workspace Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the window's arrangement one `workspace` value of two regions (left, right), each a dock; the library becomes a tab that can be dragged between sides.

**Architecture:** Pure workspace transforms in `lib/dock.ts` (built on the existing single-`DockLayout` helpers); a stateful drag coordinator (`lib/dock-drag.svelte.ts`) that sees both region roots and resolves cross-region drops; `Dock.svelte` reduced to a per-region renderer; new `DockRegion.svelte` (rail + collapse + Dock); `App.svelte` mounts two regions around the stage. One persisted `workspace` setting supersedes `panel_layout` + `library_collapsed` + `panels_collapsed`, migrated frontend-side.

**Tech Stack:** Svelte 5 (runes), TypeScript, Vitest. Frontend only — no Rust.

Spec: `docs/superpowers/specs/2026-06-27-workspace-unification-design.md`.

---

## File Structure

- `apps/desktop/src/lib/dock.ts` — **modify**: add `Region`, `Workspace`, `RegionId` types and workspace transforms (`defaultWorkspace`, `reconcileWorkspace`, `moveTabTo`, `splitTabTo`, `setActiveIn`, `setWeightsIn`, `setCollapsed`). Keep all single-layout helpers.
- `apps/desktop/src/lib/dock.test.ts` — **modify**: add a workspace `describe` block.
- `apps/desktop/src/lib/stores.ts` — **modify**: `workspace` store + `WORKSPACE` key; migration in `loadSettings`; `setWorkspace` / `toggleRegion`; retire `panelLayout`/`libraryCollapsed`/`panelsCollapsed` from the write path.
- `apps/desktop/src/lib/workspace-migrate.ts` — **create**: pure `migrateWorkspace(all, allTabs)` reading legacy keys → `Workspace` (so it's unit-testable without the store).
- `apps/desktop/src/lib/workspace-migrate.test.ts` — **create**.
- `apps/desktop/src/lib/dock-drag.svelte.ts` — **create**: the drag coordinator (rune module + factory).
- `apps/desktop/src/lib/ui/Dock.svelte` — **modify**: render one region; delegate drag to the coordinator; keep vertical resize.
- `apps/desktop/src/lib/ui/DockRegion.svelte` — **create**: rail + collapse + Dock for one side.
- `apps/desktop/src/App.svelte` — **modify**: provide the coordinator; mount `<DockRegion side="left"/>` and `<DockRegion side="right"/>`; add `library` to `ALL_TABS`/`TAB_VIEWS`; route reveal through the coordinator; delete the hand-rolled library aside.
- `apps/desktop/src/lib/keys.ts` — **modify**: Ctrl+[ / Ctrl+] → `toggleRegion`.

Verification commands:
- Unit: `cd apps/desktop && pnpm vitest run lib/dock.test.ts lib/workspace-migrate.test.ts`
- Lint: `just lint`
- Smoke (rendering/effects): vite `:5173` + chrome-devtools, watch for `effect_update_depth_exceeded`.

---

## Phase 1 — Workspace model + transforms (`dock.ts`)

### Task 1: Workspace types + `defaultWorkspace`

**Files:**
- Modify: `apps/desktop/src/lib/dock.ts`
- Test: `apps/desktop/src/lib/dock.test.ts`

- [ ] **Step 1: Write the failing test** — append to `dock.test.ts`:

```ts
import {
  defaultWorkspace, reconcileWorkspace, moveTabTo, splitTabTo,
  setActiveIn, setWeightsIn, setCollapsed,
} from "./dock";

const wkeys = (ws: { left: { layout: { tabs: string[] }[] }; right: { layout: { tabs: string[] }[] } }) => ({
  left: ws.left.layout.map((p) => p.tabs),
  right: ws.right.layout.map((p) => p.tabs),
});

describe("defaultWorkspace", () => {
  it("seeds the first tab left, the rest right, both expanded", () => {
    const ws = defaultWorkspace(["library", "a", "b"]);
    expect(wkeys(ws)).toEqual({ left: [["library"]], right: [["a", "b"]] });
    expect(ws.left.collapsed).toBe(false);
    expect(ws.right.collapsed).toBe(false);
  });
});
```

- [ ] **Step 2: Run to verify it fails** — `cd apps/desktop && pnpm vitest run lib/dock.test.ts` → FAIL ("defaultWorkspace is not exported").

- [ ] **Step 3: Implement** — append to `dock.ts`:

```ts
// ── Workspace: the window arrangement as two regions ───────────────────────
// A region is a dock (its DockLayout) plus a collapse flag. The workspace holds
// exactly two — left and right — with the stage fixed between them (not a region
// here). The exactly-once invariant is enforced across BOTH regions, so a region
// may legally be empty (its dock renders nothing; only the rail shows).
export type RegionId = "left" | "right";
export interface Region {
  layout: DockLayout;
  collapsed: boolean;
}
export interface Workspace {
  left: Region;
  right: Region;
}

/** First-run shape: the first tab (library) alone on the left, the rest on the
 *  right, both expanded. */
export function defaultWorkspace(allTabs: string[]): Workspace {
  const [first, ...rest] = allTabs;
  return {
    left: { layout: first ? [{ tabs: [first], active: first, weight: 1 }] : [], collapsed: false },
    right: { layout: rest.length ? [{ tabs: rest, active: rest[0], weight: 1 }] : [], collapsed: false },
  };
}
```

- [ ] **Step 4: Run to verify it passes** — same command → PASS.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/dock.ts apps/desktop/src/lib/dock.test.ts
git commit -m "feat(dock): workspace types + defaultWorkspace"
```

### Task 2: `reconcileWorkspace` (exactly-once across both regions)

**Files:** Modify `dock.ts`; Test `dock.test.ts`.

- [ ] **Step 1: Failing test**

```ts
describe("reconcileWorkspace", () => {
  it("keeps each known tab exactly once across both regions", () => {
    const ws = reconcileWorkspace(
      { left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: false },
        right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false } },
      ["library", "a", "b"],
    );
    expect(wkeys(ws)).toEqual({ left: [["library"]], right: [["a", "b"]] });
  });
  it("drops a tab duplicated across regions (first occurrence wins)", () => {
    const ws = reconcileWorkspace(
      { left: { layout: [{ tabs: ["a"], active: "a", weight: 1 }], collapsed: false },
        right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false } },
      ["a", "b"],
    );
    expect(wkeys(ws)).toEqual({ left: [["a"]], right: [["b"]] });
  });
  it("appends tabs new-in-code to right's last panel", () => {
    const ws = reconcileWorkspace(
      { left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: false },
        right: { layout: [{ tabs: ["a"], active: "a", weight: 1 }], collapsed: false } },
      ["library", "a", "b"],
    );
    expect(wkeys(ws).right).toEqual([["a", "b"]]);
  });
  it("allows an empty region", () => {
    const ws = reconcileWorkspace(
      { left: { layout: [], collapsed: true },
        right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false } },
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
```

- [ ] **Step 2: Run → FAIL.**

- [ ] **Step 3: Implement** — add to `dock.ts`. Reuse the per-layout `reconcile` machinery but split the known set across regions so the cross-region exactly-once holds:

```ts
/** Reconcile a whole workspace against the code's tab set: every known tab
 *  appears exactly once ACROSS both regions (first occurrence wins, scanning
 *  left then right), unknown tabs dropped, tabs new-in-code appended to right's
 *  last panel, each region's weights normalized. Empty regions are legal; if
 *  BOTH end up empty the default workspace is returned. Collapse flags pass
 *  through. */
export function reconcileWorkspace(ws: Workspace, allTabs: string[]): Workspace {
  const known = new Set(allTabs);
  const seen = new Set<string>();
  // keep first occurrence across left→right; drop unknown + duplicates
  const prune = (layout: DockLayout): DockLayout => {
    const next: DockLayout = [];
    for (const p of Array.isArray(layout) ? layout : []) {
      const tabs = (Array.isArray(p?.tabs) ? p.tabs : []).filter((t) => known.has(t) && !seen.has(t));
      for (const t of tabs) seen.add(t);
      if (tabs.length === 0) continue;
      const active = tabs.includes(p.active) ? p.active : tabs[0];
      const weight = Number.isFinite(p?.weight) && p.weight > 0 ? p.weight : 1;
      next.push({ tabs, active, weight });
    }
    return next;
  };
  const left = prune(ws?.left?.layout ?? []);
  const right = prune(ws?.right?.layout ?? []);
  const missing = allTabs.filter((t) => !seen.has(t));
  if (missing.length) {
    if (right.length) right[right.length - 1].tabs.push(...missing);
    else right.push({ tabs: missing, active: missing[0], weight: 1 });
  }
  if (left.length === 0 && right.length === 0) return defaultWorkspace(allTabs);
  normalizeWeights(left);
  normalizeWeights(right);
  return {
    left: { layout: left, collapsed: !!ws?.left?.collapsed },
    right: { layout: right, collapsed: !!ws?.right?.collapsed },
  };
}
```

Note: `normalizeWeights` is already defined in `dock.ts` and is a no-op on `[]`
(its `ok` guard fails on length 0, then the `for` loop has nothing to divide) —
verify it tolerates an empty array; if not, early-return when `layout.length === 0`.

- [ ] **Step 4: Run → PASS.**

- [ ] **Step 5: Commit** — `git commit -am "feat(dock): reconcileWorkspace with cross-region exactly-once"`

### Task 3: cross-region `moveTabTo` / `splitTabTo` + region setters

**Files:** Modify `dock.ts`; Test `dock.test.ts`.

- [ ] **Step 1: Failing test**

```ts
const baseWs = (): import("./dock").Workspace => ({
  left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: false },
  right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false },
});

describe("moveTabTo", () => {
  it("moves a tab from right to left", () => {
    const ws = moveTabTo(baseWs(), "a", "left", 0, 1);
    expect(wkeys(ws)).toEqual({ left: [["library", "a"]], right: [["b"]] });
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
});

describe("splitTabTo", () => {
  it("splits a tab into a new panel in the target region", () => {
    const ws = splitTabTo(baseWs(), "a", "left", 1);
    expect(wkeys(ws).left).toEqual([["library"], ["a"]]);
    expect(wkeys(ws).right).toEqual([["b"]]);
  });
});

describe("setActiveIn / setWeightsIn / setCollapsed", () => {
  it("sets the active tab in one region", () => {
    expect(setActiveIn(baseWs(), "right", 0, "b").right.layout[0].active).toBe("b");
  });
  it("sets collapse on one region", () => {
    expect(setCollapsed(baseWs(), "left", true).left.collapsed).toBe(true);
  });
});
```

- [ ] **Step 2: Run → FAIL.**

- [ ] **Step 3: Implement** — add to `dock.ts`. These remove the tab from whichever region holds it, then apply the existing single-layout transform to the target region:

```ts
function regionOf(ws: Workspace, tab: string): RegionId | null {
  if (ws.left.layout.some((p) => p.tabs.includes(tab))) return "left";
  if (ws.right.layout.some((p) => p.tabs.includes(tab))) return "right";
  return null;
}
const other = (r: RegionId): RegionId => (r === "left" ? "right" : "left");

/** Move `tab` to (region, panel, index). Within-region reorders/joins; across
 *  regions it leaves the source (possibly empty) and joins the target. */
export function moveTabTo(ws: Workspace, tab: string, toRegion: RegionId, toPanel: number, toIndex: number): Workspace {
  const from = regionOf(ws, tab);
  if (!from) return ws;
  if (from === toRegion) {
    return { ...ws, [toRegion]: { ...ws[toRegion], layout: moveTab(ws[toRegion].layout, tab, toPanel, toIndex) } };
  }
  // cross-region: pull from source, then insert into target at (panel, index)
  const srcLayout = removeFrom(ws[from].layout, tab);
  let dst = ws[toRegion].layout.map((p) => ({ ...p, tabs: [...p.tabs] }));
  if (dst.length === 0) {
    dst = [{ tabs: [tab], active: tab, weight: 1 }];
  } else {
    const target = dst[Math.max(0, Math.min(toPanel, dst.length - 1))];
    const at = Math.max(0, Math.min(toIndex, target.tabs.length));
    target.tabs.splice(at, 0, tab);
    target.active = tab;
  }
  return {
    ...ws,
    [from]: { ...ws[from], layout: normalizeRegion(srcLayout) },
    [toRegion]: { ...ws[toRegion], layout: normalizeRegion(dst) },
  };
}

/** Split `tab` into a new panel at `atBoundary` in `toRegion`. */
export function splitTabTo(ws: Workspace, tab: string, toRegion: RegionId, atBoundary: number): Workspace {
  const from = regionOf(ws, tab);
  if (!from) return ws;
  if (from === toRegion) {
    return { ...ws, [toRegion]: { ...ws[toRegion], layout: splitTab(ws[toRegion].layout, tab, atBoundary) } };
  }
  const srcLayout = removeFrom(ws[from].layout, tab);
  const dst = ws[toRegion].layout.map((p) => ({ ...p, tabs: [...p.tabs] }));
  const at = Math.max(0, Math.min(atBoundary, dst.length));
  dst.splice(at, 0, { tabs: [tab], active: tab, weight: 1 });
  return {
    ...ws,
    [from]: { ...ws[from], layout: normalizeRegion(srcLayout) },
    [toRegion]: { ...ws[toRegion], layout: normalizeRegion(dst) },
  };
}

export function setActiveIn(ws: Workspace, region: RegionId, panel: number, tab: string): Workspace {
  return { ...ws, [region]: { ...ws[region], layout: setActive(ws[region].layout, panel, tab) } };
}
export function setWeightsIn(ws: Workspace, region: RegionId, weights: number[]): Workspace {
  return { ...ws, [region]: { ...ws[region], layout: setWeights(ws[region].layout, weights) } };
}
export function setCollapsed(ws: Workspace, region: RegionId, collapsed: boolean): Workspace {
  return { ...ws, [region]: { ...ws[region], collapsed } };
}
```

Supporting helpers (add near `removeTab`/`normalize` in `dock.ts`). `removeFrom`
is a pure variant of the existing in-place `removeTab`; `normalizeRegion` is
`normalize` but **without** the non-empty guarantee — it keeps empty layouts as
`[]`:

```ts
/** Pure: a copy of `layout` with `tab` removed from wherever it is. */
function removeFrom(layout: DockLayout, tab: string): DockLayout {
  const next = layout.map((p) => ({ ...p, tabs: [...p.tabs] }));
  removeTab(next, tab);
  return next;
}
/** Like `normalize` but a region may end empty (drop empty panels, fix actives,
 *  renormalize the survivors; `[]` stays `[]`). */
function normalizeRegion(layout: DockLayout): DockLayout {
  const next = layout.filter((p) => p.tabs.length > 0).map((p) => ({ ...p, tabs: [...p.tabs] }));
  for (const p of next) if (!p.tabs.includes(p.active)) p.active = p.tabs[0];
  normalizeWeights(next);
  return next;
}
```

- [ ] **Step 4: Run → PASS** (full file: `pnpm vitest run lib/dock.test.ts`).

- [ ] **Step 5: Commit** — `git commit -am "feat(dock): cross-region move/split + region setters"`

---

## Phase 2 — Migration + persistence

### Task 4: pure `migrateWorkspace`

**Files:**
- Create: `apps/desktop/src/lib/workspace-migrate.ts`
- Test: `apps/desktop/src/lib/workspace-migrate.test.ts`

- [ ] **Step 1: Failing test** (`workspace-migrate.test.ts`):

```ts
import { describe, it, expect } from "vitest";
import { migrateWorkspace } from "./workspace-migrate";

const ALL = ["library", "a", "b"];
const keys = (ws: { left: { layout: { tabs: string[] }[] }; right: { layout: { tabs: string[] }[] } }) => ({
  left: ws.left.layout.map((p) => p.tabs), right: ws.right.layout.map((p) => p.tabs),
});

describe("migrateWorkspace", () => {
  it("uses an existing workspace, reconciled", () => {
    const ws = migrateWorkspace({ workspace: {
      left: { layout: [{ tabs: ["library"], active: "library", weight: 1 }], collapsed: true },
      right: { layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }], collapsed: false },
    } }, ALL);
    expect(keys(ws)).toEqual({ left: [["library"]], right: [["a", "b"]] });
    expect(ws.left.collapsed).toBe(true);
  });
  it("migrates legacy panel_layout into right, library seeded left", () => {
    const ws = migrateWorkspace({
      panel_layout: [{ tabs: ["a", "b"], active: "a", weight: 1 }],
      library_collapsed: true, panels_collapsed: false,
    }, ALL);
    expect(keys(ws)).toEqual({ left: [["library"]], right: [["a", "b"]] });
    expect(ws.left.collapsed).toBe(true);
  });
  it("falls back to default when nothing is stored", () => {
    expect(keys(migrateWorkspace({}, ALL))).toEqual({ left: [["library"]], right: [["a", "b"]] });
  });
});
```

- [ ] **Step 2: Run → FAIL** — `pnpm vitest run lib/workspace-migrate.test.ts`.

- [ ] **Step 3: Implement** (`workspace-migrate.ts`):

```ts
// One-time read of the durable settings into a Workspace. An existing
// `workspace` wins; otherwise the legacy `panel_layout` becomes the right
// region, `library` is seeded left, and the old collapse booleans carry over.
// Always reconciled against the current tab set. The legacy keys are read here
// and nowhere else — once written back as `workspace` they go quiet.
import { defaultWorkspace, reconcileWorkspace, type DockLayout, type Workspace } from "./dock";

export function migrateWorkspace(all: Record<string, unknown>, allTabs: string[]): Workspace {
  const existing = all.workspace;
  if (existing && typeof existing === "object" && "left" in existing && "right" in existing) {
    return reconcileWorkspace(existing as Workspace, allTabs);
  }
  const [first, ...rest] = allTabs;
  const rightLayout = Array.isArray(all.panel_layout) ? (all.panel_layout as DockLayout) : null;
  if (!rightLayout && !rest.length) return defaultWorkspace(allTabs);
  const ws: Workspace = {
    left: {
      layout: first ? [{ tabs: [first], active: first, weight: 1 }] : [],
      collapsed: all.library_collapsed === true,
    },
    right: {
      layout: rightLayout ?? (rest.length ? [{ tabs: rest, active: rest[0], weight: 1 }] : []),
      collapsed: all.panels_collapsed === true,
    },
  };
  return reconcileWorkspace(ws, allTabs);
}
```

- [ ] **Step 4: Run → PASS.**

- [ ] **Step 5: Commit** — `git add -A && git commit -m "feat(workspace): pure settings→workspace migration"`

### Task 5: `workspace` store + actions; wire migration into load

**Files:** Modify `apps/desktop/src/lib/stores.ts`.

- [ ] **Step 1:** Add the store, key, and `ALL_TABS` import source. Near the other
  durable-settings stores, **replace** `panelLayout` with:

```ts
import type { Workspace, RegionId } from "./dock";
import { migrateWorkspace } from "./workspace-migrate";
// ...
/** The window arrangement: left + right regions, each a dock stack plus its
 *  collapse flag. One source of truth; supersedes panel_layout / *_collapsed. */
export const workspace = writable<Workspace>(
  { left: { layout: [], collapsed: false }, right: { layout: [], collapsed: false } },
);
export const WORKSPACE = "workspace";
```

  Keep `PANEL_LAYOUT`, `LIBRARY_COLLAPSED`, `PANELS_COLLAPSED`, `TAB_ORDER_LEGACY`
  consts (read by the migration) but delete the `panelLayout`, `libraryCollapsed`,
  `panelsCollapsed` writables.

- [ ] **Step 2:** In `loadSettings`, replace the `PANEL_LAYOUT`/`TAB_ORDER`/
  `LIBRARY_COLLAPSED`/`PANELS_COLLAPSED` block with the tab set + migration. The
  canonical tab list now lives in one place — export it from stores and have
  `App.svelte` import it (removes the duplicate `ALL_TABS`):

```ts
export const ALL_TABS = ["library", "structure", "loops", "routines", "export", "profile", "devices", "settings", "guide"] as const;
// ...inside loadSettings, replacing the old layout/collapse lines:
workspace.set(migrateWorkspace(all, [...ALL_TABS]));
```

- [ ] **Step 3:** Replace the actions. Delete `setPanelLayout`, rewrite the toggles:

```ts
async setWorkspace(ws: Workspace): Promise<void> {
  workspace.set(ws);
  await this.setSetting(WORKSPACE, ws);
},
async toggleRegion(region: RegionId): Promise<void> {
  const ws = setCollapsed(get(workspace), region, !get(workspace)[region].collapsed);
  await this.setWorkspace(ws);
},
```

  Import `setCollapsed` from `./dock`. Delete `toggleLibrary` / `togglePanels`.

- [ ] **Step 4: Run** — `pnpm vitest run` (whole suite) and `just lint`. Expect
  type errors in `App.svelte`/`keys.ts` referencing the deleted stores — those are
  fixed in Phase 4/5; if landing phases separately, see the note below. Expected at
  THIS step: `stores.ts` itself type-checks; the suite's existing tests pass.

> **Phasing note:** Tasks 5→9 leave intermediate type errors in `App.svelte` and
> `keys.ts` until Phase 5. To keep each commit's `just lint` green, land Tasks 5–9
> as one working session (commit per task, run `just lint` only at Task 9's gate),
> OR temporarily keep `toggleLibrary`/`togglePanels` as thin shims over
> `toggleRegion` until keys.ts is updated. Recommended: land them together.

- [ ] **Step 5: Commit** — `git commit -am "feat(workspace): workspace store + migration on load"`

---

## Phase 3 — Drag coordinator

### Task 6: `dock-drag.svelte.ts`

**Files:** Create `apps/desktop/src/lib/dock-drag.svelte.ts`.

This ports `Dock.svelte`'s drag brain to a shared, region-aware coordinator. It
owns the active drag and resolves a drop against any registered region root.

- [ ] **Step 1: Implement** the coordinator factory:

```ts
// The shared drag brain for the workspace's regions. Each DockRegion registers
// its root element; a tab drag (started in any region) is resolved by
// hit-testing across ALL registered roots, so dropping into a sibling region
// just works. Pure layout math stays in dock.ts; this is the stateful gesture
// layer. One coordinator per app, provided via context.
import { getContext, setContext } from "svelte";
import {
  moveTabTo, splitTabTo, type RegionId, type Workspace,
} from "./dock";

export type Drop =
  | { kind: "tab"; region: RegionId; panel: number; index: number }
  | { kind: "split"; region: RegionId; at: number };

const KEY = Symbol("dock-drag");
const DRAG_PX = 4;

export interface DockDrag {
  dragTab: string | null;
  drop: Drop | null;
  caret: { x: number; y: number; h: number } | null;
  register(region: RegionId, el: HTMLElement): void;
  unregister(region: RegionId): void;
  onTabDown(e: PointerEvent, tab: string): void;
  onTabMove(e: PointerEvent): void;
  onTabUp(): void;
  didDrag(): boolean;
  reveal(tab: string): void;
}

export function createDockDrag(getWs: () => Workspace, onchange: (ws: Workspace) => void): DockDrag {
  const roots = new Map<RegionId, HTMLElement>();
  let dragTab = $state<string | null>(null);
  let drop = $state<Drop | null>(null);
  let caret = $state<{ x: number; y: number; h: number } | null>(null);
  let downTab: string | null = null;
  let downX = 0, downY = 0, didDragFlag = false;

  function regionAt(el: Element): RegionId | null {
    for (const [region, root] of roots) if (root.contains(el)) return region;
    return null;
  }
  function caretAt(bar: HTMLElement, els: HTMLElement[], index: number) {
    if (els.length === 0) { const r = bar.getBoundingClientRect(); return { x: r.left + 6, y: r.top + 5, h: 16 }; }
    if (index < els.length) { const r = els[index].getBoundingClientRect(); return { x: r.left - 3, y: r.top, h: r.height }; }
    const r = els[els.length - 1].getBoundingClientRect(); return { x: r.right + 1, y: r.top, h: r.height };
  }

  return {
    get dragTab() { return dragTab; },
    get drop() { return drop; },
    get caret() { return caret; },
    register(region, el) { roots.set(region, el); },
    unregister(region) { roots.delete(region); },
    didDrag() { return didDragFlag; },
    onTabDown(e, tab) {
      if (e.button !== 0) return;
      downTab = tab; downX = e.clientX; downY = e.clientY; didDragFlag = false;
      try { (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId); } catch { /* non-fatal */ }
    },
    onTabMove(e) {
      if (downTab === null) return;
      if (dragTab === null) {
        if (Math.abs(e.clientX - downX) < DRAG_PX && Math.abs(e.clientY - downY) < DRAG_PX) return;
        dragTab = downTab; didDragFlag = true;
      }
      const el = document.elementFromPoint(e.clientX, e.clientY);
      const region = el ? regionAt(el) : null;
      if (!el || !region) { drop = null; caret = null; return; }
      const root = roots.get(region)!;
      const panels = [...root.querySelectorAll<HTMLElement>(".dock-panel")];
      const barEl = el.closest<HTMLElement>(".tabs");
      if (barEl && root.contains(barEl)) {
        const pi = panels.indexOf(barEl.closest<HTMLElement>(".dock-panel") as HTMLElement);
        const els = [...barEl.querySelectorAll<HTMLElement>(".tab")].filter((b) => b.dataset.tab !== dragTab);
        let index = els.length;
        for (let i = 0; i < els.length; i++) {
          const r = els[i].getBoundingClientRect();
          if (e.clientY < r.top || (e.clientY <= r.bottom && e.clientX < r.left + r.width / 2)) { index = i; break; }
        }
        drop = { kind: "tab", region, panel: pi, index };
        caret = caretAt(barEl, els, index);
        return;
      }
      const panelEl = el.closest<HTMLElement>(".dock-panel");
      if (panelEl && root.contains(panelEl)) {
        const pi = panels.indexOf(panelEl);
        const r = panelEl.getBoundingClientRect();
        drop = { kind: "split", region, at: e.clientY < r.top + r.height / 2 ? pi : pi + 1 };
        caret = null;
        return;
      }
      // over a region but not a panel (e.g. empty region body) → split at 0
      drop = { kind: "split", region, at: 0 };
      caret = null;
    },
    onTabUp() {
      if (dragTab !== null && drop) {
        const ws = getWs();
        const next = drop.kind === "tab"
          ? moveTabTo(ws, dragTab, drop.region, drop.panel, drop.index)
          : splitTabTo(ws, dragTab, drop.region, drop.at);
        onchange(next);
      }
      dragTab = null; downTab = null; drop = null; caret = null;
    },
    reveal(tab) {
      const ws = getWs();
      const region: RegionId | null = ws.left.layout.some((p) => p.tabs.includes(tab)) ? "left"
        : ws.right.layout.some((p) => p.tabs.includes(tab)) ? "right" : null;
      if (!region) return;
      const pi = ws[region].layout.findIndex((p) => p.tabs.includes(tab));
      let next = ws;
      if (ws[region].collapsed) next = { ...next, [region]: { ...next[region], collapsed: false } };
      next = { ...next, [region]: { ...next[region], layout: next[region].layout.map((p, i) => i === pi ? { ...p, active: tab } : p) } };
      onchange(next);
    },
  };
}

export function setDockDrag(d: DockDrag) { setContext(KEY, d); }
export function getDockDrag(): DockDrag { return getContext(KEY); }
```

- [ ] **Step 2: Lint** — `just lint` (svelte-check covers `.svelte.ts`). Expected: clean.

- [ ] **Step 3: Commit** — `git add -A && git commit -m "feat(dock): shared region-aware drag coordinator"`

---

## Phase 4 — `Dock.svelte` as region renderer + `DockRegion.svelte`

### Task 7: refit `Dock.svelte` to one region + coordinator

**Files:** Modify `apps/desktop/src/lib/ui/Dock.svelte`.

- [ ] **Step 1:** Change `Props` to a region + region id; drop the internal drag
  state and `reveal` (now the coordinator's). New script shape:

```svelte
<script lang="ts">
  import { flip } from "svelte/animate";
  import type { Component } from "svelte";
  import { setWeights, type DockLayout, type RegionId } from "../dock";
  import { getDockDrag } from "../dock-drag.svelte";

  interface Props {
    region: RegionId;
    layout: DockLayout;                 // this region's panels (already reconciled by the parent)
    views: Record<string, Component>;
    onlayout: (layout: DockLayout) => void;  // within-region resize → new layout for THIS region
  }
  let { region, layout, views, onlayout }: Props = $props();
  const drag = getDockDrag();
  let dockEl: HTMLElement;

  $effect(() => { drag.register(region, dockEl); return () => drag.unregister(region); });

  function selectTab(panel: number, t: string) {
    if (drag.didDrag()) return;
    onlayout(setActive(layout, panel, t));   // import setActive too
  }
</script>
```

  - Replace the tab pointer handlers (`onTabDown/Move/Up`) on each `.tab` button
    with `drag.onTabDown(e, t)` / `drag.onTabMove` / `drag.onTabUp`.
  - Replace `class:dragging={dragTab === t}` with `class:dragging={drag.dragTab === t}`.
  - Drop indicators read the coordinator: `drop?.kind === "tab" && drop.region === region && drop.panel === pi`, etc. Bind `const drop = $derived(drag.drop)` and `const caret = $derived(drag.caret)` for the template; gate the caret render on `drag.dragTab && drop?.kind === "tab" && drop.region === region`.
  - Keep the **vertical resize** block (splitters + right-drag snap) exactly as is,
    but its `onchange(setWeights(...))` becomes `onlayout(setWeights(layout, resizeWeights))`, and every `effective` reference becomes `layout` (the parent now passes an already-reconciled region layout).

- [ ] **Step 2: Lint** — `just lint`. (The right aside isn't wired to the new Props
  yet; App still references the old Dock — expect App errors, resolved in Task 8.
  If landing 7–9 together, run lint at Task 9.) `Dock.svelte` itself type-checks.

- [ ] **Step 3: Commit** — `git commit -am "refactor(dock): Dock renders one region, drag via coordinator"`

### Task 8: `DockRegion.svelte`

**Files:** Create `apps/desktop/src/lib/ui/DockRegion.svelte`.

- [ ] **Step 1: Implement** — rail + collapse + Dock, porting the rail markup/CSS
  from `App.svelte`'s two asides into one component:

```svelte
<script lang="ts">
  import type { Component } from "svelte";
  import Dock from "./Dock.svelte";
  import type { DockLayout, RegionId } from "../dock";

  interface Props {
    side: RegionId;                 // "left" | "right" — which edge
    layout: DockLayout;
    collapsed: boolean;
    views: Record<string, Component>;
    onlayout: (layout: DockLayout) => void;
    ontoggle: () => void;
  }
  let { side, layout, collapsed, views, onlayout, ontoggle }: Props = $props();
  // chevron points "inward when expanded, outward when collapsed"
  const chevron = $derived(side === "left" ? (collapsed ? "›" : "‹") : (collapsed ? "‹" : "›"));
  const label = $derived(side === "left" ? "library" : "panels");
</script>

<aside class="region {side}" class:collapsed>
  {#if side === "right" && !collapsed}
    <Dock {side} {layout} {views} {onlayout} />
  {/if}
  <button
    class="rail"
    onclick={ontoggle}
    title={collapsed ? `show ${label}` : `hide ${label}`}
    aria-label={collapsed ? `show ${label}` : `hide ${label}`}
  >{chevron}</button>
  {#if side === "left" && !collapsed}
    <Dock {side} {layout} {views} {onlayout} />
  {/if}
</aside>

<style>
  /* ported from App.svelte's .library/.panels/.rail; the rail sits on the OUTER
     edge of each side (left rail leftmost, right rail rightmost) */
  .region { display: flex; flex-direction: row; min-width: 0; }
  .region.left { border-right: 1px solid var(--line); }
  .region.right { border-left: 1px solid var(--line); }
  .region.collapsed { border: none; }
  .rail {
    flex: 0 0 var(--rail-w); align-self: stretch; display: flex; align-items: center;
    justify-content: center; background: none; border: none; color: var(--muted);
    cursor: pointer; font-size: 14px; opacity: 0; transition: opacity 120ms ease;
  }
  .rail:hover { background: var(--bg-raised); color: var(--fg); opacity: 1; }
</style>
```

  Note the left/right ordering: for the left region the rail comes after the Dock
  in source but the rail must be leftmost — instead render rail-then-Dock for
  `left` and Dock-then-rail for `right`. (The snippet above renders Dock first for
  right and rail-first for left via the two `{#if}` blocks; the rail is the middle
  element. Verify visually in the smoke test and flip if the rail lands inboard.)

- [ ] **Step 2: Lint** — `just lint` (with Task 9 wiring, or together).

- [ ] **Step 3: Commit** — `git add -A && git commit -m "feat(dock): DockRegion (rail + collapse + Dock)"`

---

## Phase 5 — Wire both regions + cross-region drag (convergence)

### Task 9: `App.svelte` + `keys.ts`

**Files:** Modify `apps/desktop/src/App.svelte`, `apps/desktop/src/lib/keys.ts`.

- [ ] **Step 1: App script** — import `Library`, the workspace store, the coordinator;
  add `library` to `TAB_VIEWS`; import `ALL_TABS` from stores (delete the local
  copy); create + provide the coordinator:

```svelte
<script lang="ts">
  import { ALL_TABS, workspace, actions, /* …existing… */ } from "./lib/stores";
  import { createDockDrag, setDockDrag } from "./lib/dock-drag.svelte";
  import { setActiveIn, type DockLayout } from "./lib/dock";
  import DockRegion from "./lib/ui/DockRegion.svelte";
  import Library from "./components/Library.svelte";
  // TAB_VIEWS gains:  library: Library,  (keep the rest)

  const drag = createDockDrag(() => $workspace, (ws) => void actions.setWorkspace(ws));
  setDockDrag(drag);

  // reveal effects now go through the coordinator
  $effect(() => { if ($settingsOpen) { drag.reveal("settings"); settingsOpen.set(false); } });
  $effect(() => { if ($sectionsOpen) { drag.reveal("structure"); sectionsOpen.set(false); } });
  $effect(() => { if ($loopsOpen) { drag.reveal("loops"); loopsOpen.set(false); } });

  // per-region layout write-back (resize/reorder within a region)
  const setLayout = (region: "left" | "right") => (layout: DockLayout) =>
    void actions.setWorkspace({ ...$workspace, [region]: { ...$workspace[region], layout } });
</script>
```

  Remove the old `dock` ref/`bind:this` and the `Dock` import.

- [ ] **Step 2: App markup** — replace the two asides + keep the stage:

```svelte
<div class="shell"
     class:lib-collapsed={$workspace.left.collapsed}
     class:panels-collapsed={$workspace.right.collapsed}>
  <DockRegion side="left"
    layout={$workspace.left.layout} collapsed={$workspace.left.collapsed}
    views={TAB_VIEWS} onlayout={setLayout("left")} ontoggle={() => void actions.toggleRegion("left")} />
  <main class="stage"> … unchanged … </main>
  <DockRegion side="right"
    layout={$workspace.right.layout} collapsed={$workspace.right.collapsed}
    views={TAB_VIEWS} onlayout={setLayout("right")} ontoggle={() => void actions.toggleRegion("right")} />
</div>
```

  Delete the `.library`/`.panels`/`.rail`/`.pane` CSS now living in `DockRegion`
  (keep `.shell`, `--col-*`, the media query, the collapse `--col` overrides, and
  `.stage`/`.boxes`). The grid still places three columns by source order.

- [ ] **Step 3: keys.ts** — swap the toggles:

```ts
} else if (e.key === "[" && !isEditingTarget(e.target)) {
  e.preventDefault();
  await actions.toggleRegion("left");
} else if (e.key === "]" && !isEditingTarget(e.target)) {
  e.preventDefault();
  await actions.toggleRegion("right");
}
```

- [ ] **Step 4: Gate — full verification.**
  - `pnpm vitest run` → green.
  - `just lint` → clean (clippy/fmt unaffected; svelte-check clean).
  - Smoke-test (vite `:5173` + chrome-devtools): library renders in the left dock;
    drag a tab left↔right and it lands + persists across reload; drag a tab to a
    boundary splits; splitter resizes; Ctrl+[ / Ctrl+] collapse each side; emptying
    a region collapses it to its rail; **no `effect_update_depth_exceeded`** in the
    console. Confirm a freshly-removed `workspace` setting (or a DB with only the
    legacy keys) migrates correctly.

- [ ] **Step 5: Commit** — `git add -A && git commit -m "feat(dock): left region live; tabs flow between regions"`

---

## Self-Review

**Spec coverage:** model (Task 1–3) ✓; migration retiring three keys (Task 4–5) ✓;
coordinator with cross-root hit-test + reveal (Task 6) ✓; Dock→renderer (Task 7) ✓;
DockRegion unifying the rails (Task 8) ✓; App two-region mount + library tab + keys
(Task 9) ✓; empty-region behavior (Task 2/3 tests, Task 9 smoke) ✓; stage boxes
deferred (not in plan) ✓.

**Type consistency:** `Workspace`/`Region`/`RegionId` defined Task 1, used
throughout; `moveTabTo`/`splitTabTo`/`setActiveIn`/`setWeightsIn`/`setCollapsed`
signatures match Task 3 ↔ Task 5/6/9 calls; coordinator `Drop` carries `region`,
read with `drop.region === region` in Task 7; `Dock` Props (`region`,`layout`,
`views`,`onlayout`) match `DockRegion` (Task 8) and `App` (Task 9) call sites.

**Phasing risk noted:** Tasks 5–9 carry intermediate type errors until Task 9 — the
phasing note says land them as one session (commit per task, lint at the Task 9
gate). This honors "each phase shippable" at the Phase-5 boundary; within Phase
4–5 the commits are checkpoints, not independently green.
