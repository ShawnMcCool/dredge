// The dock layout: the right aside is a vertical stack of panels, each holding
// an ordered set of tabs (pages) plus the active one. Reorder, join, split, and
// close-empty are all one idea — move a tab to a (panel, position), then
// normalize (drop empty panels, fix each active, renormalize weights). These are
// pure transforms; the component renders them and reads drag gestures.

import { defaultFlow, reconcileFlow, STAGE_BOXES, type FlowRegion } from "./stage";

/** One stackable unit: an ordered tab list, the active tab, and its vertical
 *  share (`weight`, relative — the render uses it as flex-grow). */
export interface Panel {
  tabs: string[];
  active: string;
  weight: number;
}

/** The whole dock, top → bottom. Invariants (held by `normalize`/`reconcile`):
 *  every known tab appears exactly once; no empty panels; `active ∈ tabs`. */
export type DockLayout = Panel[];

/** One panel holding every tab — the default, and the shape the old flat tab
 *  order collapses to. */
export function defaultLayout(allTabs: string[]): DockLayout {
  return [{ tabs: [...allTabs], active: allTabs[0] ?? "", weight: 1 }];
}

/** Migration: the old flat `tab_order` is one panel in that order. Reconciled
 *  against the known tabs so a stale/short order still yields a valid dock. */
export function fromTabOrder(order: string[], allTabs: string[]): DockLayout {
  return reconcile([{ tabs: [...order], active: order[0] ?? "", weight: 1 }], allTabs);
}

function normalizeWeights(layout: DockLayout): void {
  const ok = layout.length > 0 && layout.every((p) => Number.isFinite(p.weight) && p.weight > 0);
  if (!ok) {
    for (const p of layout) p.weight = 1 / layout.length;
    return;
  }
  const sum = layout.reduce((a, p) => a + p.weight, 0);
  for (const p of layout) p.weight = p.weight / sum;
}

/** Drop empty panels, fix each `active` into its tabs, renormalize weights.
 *  Applied after every transform so callers always get a valid layout back. The
 *  transforms only move tabs (never delete the set), so the result is non-empty. */
function normalize(layout: DockLayout): DockLayout {
  const next = layout.filter((p) => p.tabs.length > 0).map((p) => ({ ...p, tabs: [...p.tabs] }));
  for (const p of next) {
    if (!p.tabs.includes(p.active)) p.active = p.tabs[0];
  }
  normalizeWeights(next);
  return next;
}

/** Enforce the exactly-once invariant against the current code's tab set: keep
 *  the first occurrence of each tab, drop unknown tabs, append tabs new in code
 *  to the last panel, and default when nothing valid remains. Used on load. */
export function reconcile(layout: DockLayout, allTabs: string[]): DockLayout {
  const known = new Set(allTabs);
  const seen = new Set<string>();
  const next: DockLayout = [];
  for (const p of Array.isArray(layout) ? layout : []) {
    const src = Array.isArray(p?.tabs) ? p.tabs : [];
    const tabs = src.filter((t) => known.has(t) && !seen.has(t));
    for (const t of tabs) seen.add(t);
    if (tabs.length === 0) continue;
    const active = tabs.includes(p.active) ? p.active : tabs[0];
    const weight = Number.isFinite(p?.weight) && p.weight > 0 ? p.weight : 1;
    next.push({ tabs, active, weight });
  }
  const missing = allTabs.filter((t) => !seen.has(t));
  if (missing.length) {
    if (next.length) next[next.length - 1].tabs.push(...missing);
    else next.push({ tabs: missing, active: missing[0], weight: 1 });
  }
  if (next.length === 0) return defaultLayout(allTabs);
  normalizeWeights(next);
  return next;
}

function removeTab(layout: DockLayout, tab: string, skip?: Panel): void {
  for (const p of layout) {
    if (p === skip) continue;
    const i = p.tabs.indexOf(tab);
    if (i !== -1) {
      p.tabs.splice(i, 1);
      if (p.active === tab && p.tabs.length) p.active = p.tabs[Math.min(i, p.tabs.length - 1)];
      return;
    }
  }
}

/** Move `tab` to position `toIndex` in panel `toPanel`. Within the same panel
 *  this reorders (active is preserved); crossing panels joins (the moved tab
 *  becomes active in its new home). An emptied source panel is dropped. */
export function moveTab(layout: DockLayout, tab: string, toPanel: number, toIndex: number): DockLayout {
  const next = layout.map((p) => ({ ...p, tabs: [...p.tabs] }));
  const target = next[toPanel];
  if (!target) return layout;
  const samePanel = target.tabs.includes(tab);
  removeTab(next, tab);
  const at = Math.max(0, Math.min(toIndex, target.tabs.length));
  target.tabs.splice(at, 0, tab);
  if (!samePanel) target.active = tab;
  return normalize(next);
}

/** Split `tab` into a brand-new panel inserted at boundary index `atBoundary`
 *  (0 = above all, length = below all). The source panel is dropped if emptied. */
export function splitTab(layout: DockLayout, tab: string, atBoundary: number): DockLayout {
  const next = layout.map((p) => ({ ...p, tabs: [...p.tabs] }));
  const at = Math.max(0, Math.min(atBoundary, next.length));
  const fresh: Panel = { tabs: [tab], active: tab, weight: 1 / (next.length + 1) };
  next.splice(at, 0, fresh);
  removeTab(next, tab, fresh);
  return normalize(next);
}

/** Make `tab` the active page of panel `panel` (no-op if it isn't in it). */
export function setActive(layout: DockLayout, panel: number, tab: string): DockLayout {
  return layout.map((p, i) => (i === panel && p.tabs.includes(tab) ? { ...p, active: tab } : p));
}

/** Replace panel weights (positional), then renormalize. */
export function setWeights(layout: DockLayout, weights: number[]): DockLayout {
  const next = layout.map((p, i) => ({ ...p, weight: weights[i] ?? p.weight }));
  normalizeWeights(next);
  return next;
}

// ── Workspace: the window arrangement as two regions ───────────────────────
// A region is a dock (its DockLayout) plus a collapse flag. The workspace holds
// exactly two — left and right — with the stage fixed between them (not a region
// here). The exactly-once invariant is enforced ACROSS both regions, so a region
// may legally be empty (its dock renders nothing; only the rail shows). This is
// the one place where empty layouts are allowed; `normalizeRegion` keeps `[]`.
export type RegionId = "left" | "right";
export interface DockRegion {
  layout: DockLayout;
  collapsed: boolean;
}
export interface Workspace {
  left: DockRegion;
  right: DockRegion;
  stage: FlowRegion;
}

/** First-run shape: the first tab (library) alone on the left, the rest on the
 *  right, both expanded; the stage flow in its canonical order. */
export function defaultWorkspace(allTabs: string[]): Workspace {
  const [first, ...rest] = allTabs;
  return {
    left: { layout: first ? [{ tabs: [first], active: first, weight: 1 }] : [], collapsed: false },
    right: { layout: rest.length ? [{ tabs: rest, active: rest[0], weight: 1 }] : [], collapsed: false },
    stage: defaultFlow(),
  };
}

/** Reconcile a whole workspace against the code's tab set: every known tab
 *  appears exactly once ACROSS both regions (first occurrence wins, scanning
 *  left then right), unknown tabs dropped, tabs new-in-code appended to right's
 *  last panel, each region's weights normalized. Empty regions are legal; if
 *  BOTH end up empty the default workspace is returned. Collapse flags pass
 *  through. */
/** The shape `reconcileWorkspace` actually receives — untrusted, possibly-partial
 *  data read back from settings. Reconciled into a valid `Workspace`. */
type StoredWorkspace = {
  left?: { layout?: unknown; collapsed?: unknown };
  right?: { layout?: unknown; collapsed?: unknown };
  stage?: unknown;
};

export function reconcileWorkspace(ws: StoredWorkspace | null | undefined, allTabs: string[]): Workspace {
  const known = new Set(allTabs);
  const seen = new Set<string>();
  const prune = (layout: unknown): DockLayout => {
    const next: DockLayout = [];
    for (const p of (Array.isArray(layout) ? layout : []) as Panel[]) {
      const src = Array.isArray(p?.tabs) ? (p.tabs as string[]) : [];
      const tabs = src.filter((t) => known.has(t) && !seen.has(t));
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
  // nothing valid was stored → first-run default (library left, rest right)
  // rather than dumping every tab onto one side.
  if (seen.size === 0) return defaultWorkspace(allTabs);
  const missing = allTabs.filter((t) => !seen.has(t));
  if (missing.length) {
    if (right.length) right[right.length - 1].tabs.push(...missing);
    else right.push({ tabs: missing, active: missing[0], weight: 1 });
  }
  normalizeWeights(left);
  normalizeWeights(right);
  return {
    left: { layout: left, collapsed: !!ws?.left?.collapsed },
    right: { layout: right, collapsed: !!ws?.right?.collapsed },
    stage: reconcileFlow((ws?.stage ?? {}) as { order?: unknown; collapsed?: unknown; hidden?: unknown }, STAGE_BOXES),
  };
}

function regionOf(ws: Workspace, tab: string): RegionId | null {
  if (ws.left.layout.some((p) => p.tabs.includes(tab))) return "left";
  if (ws.right.layout.some((p) => p.tabs.includes(tab))) return "right";
  return null;
}

/** Pure: a copy of `layout` with `tab` removed from wherever it is. */
function removeFrom(layout: DockLayout, tab: string): DockLayout {
  const next = layout.map((p) => ({ ...p, tabs: [...p.tabs] }));
  removeTab(next, tab);
  return next;
}

/** Like `normalize`, but a region may end empty: drop empty panels, fix each
 *  active, renormalize the survivors; `[]` stays `[]`. */
function normalizeRegion(layout: DockLayout): DockLayout {
  const next = layout.filter((p) => p.tabs.length > 0).map((p) => ({ ...p, tabs: [...p.tabs] }));
  for (const p of next) {
    if (!p.tabs.includes(p.active)) p.active = p.tabs[0];
  }
  normalizeWeights(next);
  return next;
}

/** Move `tab` to (region, panel, index). Within-region reorders/joins (delegates
 *  to `moveTab`); across regions it leaves the source (possibly empty) and joins
 *  the target, focusing the moved tab. */
export function moveTabTo(ws: Workspace, tab: string, toRegion: RegionId, toPanel: number, toIndex: number): Workspace {
  const from = regionOf(ws, tab);
  if (!from) return ws;
  if (from === toRegion) {
    return { ...ws, [toRegion]: { ...ws[toRegion], layout: moveTab(ws[toRegion].layout, tab, toPanel, toIndex) } };
  }
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

/** Split `tab` into a brand-new panel at boundary `atBoundary` in `toRegion`
 *  (cross-region split included). */
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

/** Make `tab` the active page of panel `panel` in `region`. */
export function setActiveIn(ws: Workspace, region: RegionId, panel: number, tab: string): Workspace {
  return { ...ws, [region]: { ...ws[region], layout: setActive(ws[region].layout, panel, tab) } };
}

/** Replace `region`'s panel weights (positional), then renormalize. */
export function setWeightsIn(ws: Workspace, region: RegionId, weights: number[]): Workspace {
  return { ...ws, [region]: { ...ws[region], layout: setWeights(ws[region].layout, weights) } };
}

/** Set a region's collapse flag. */
export function setCollapsed(ws: Workspace, region: RegionId, collapsed: boolean): Workspace {
  return { ...ws, [region]: { ...ws[region], collapsed } };
}
