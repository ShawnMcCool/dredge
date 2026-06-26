// The dock layout: the right aside is a vertical stack of panels, each holding
// an ordered set of tabs (pages) plus the active one. Reorder, join, split, and
// close-empty are all one idea — move a tab to a (panel, position), then
// normalize (drop empty panels, fix each active, renormalize weights). These are
// pure transforms; the component renders them and reads drag gestures.

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
  for (const p of layout) {
    const tabs = p.tabs.filter((t) => known.has(t) && !seen.has(t));
    for (const t of tabs) seen.add(t);
    if (tabs.length === 0) continue;
    const active = tabs.includes(p.active) ? p.active : tabs[0];
    next.push({ tabs, active, weight: p.weight });
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
