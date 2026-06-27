// The shared drag brain for the workspace's regions. Each DockRegion registers
// its root element; a tab drag (started in any region) is resolved by
// hit-testing across ALL registered roots, so dropping a tab into a sibling
// region just works — the one place that can see both regions at once. Pure
// layout math stays in dock.ts; this is the stateful gesture layer. One
// coordinator per app, provided via context.
import { getContext, setContext } from "svelte";
import { moveTabTo, splitTabTo, type RegionId, type Workspace } from "./dock";

export type Drop =
  | { kind: "tab"; region: RegionId; panel: number; index: number }
  | { kind: "split"; region: RegionId; at: number };

const KEY = Symbol("dock-drag");
const DRAG_PX = 4;

export interface DockDrag {
  readonly dragTab: string | null;
  readonly drop: Drop | null;
  readonly caret: { x: number; y: number; h: number } | null;
  register(region: RegionId, el: HTMLElement): void;
  unregister(region: RegionId): void;
  onTabDown(e: PointerEvent, tab: string): void;
  onTabMove(e: PointerEvent): void;
  onTabUp(): void;
  didDrag(): boolean;
}

/** Build a coordinator over a workspace getter + a change sink. */
export function createDockDrag(getWs: () => Workspace, onchange: (ws: Workspace) => void): DockDrag {
  const roots = new Map<RegionId, HTMLElement>();
  let dragTab = $state<string | null>(null);
  let drop = $state<Drop | null>(null);
  let caret = $state<{ x: number; y: number; h: number } | null>(null);
  let downTab: string | null = null;
  let downX = 0;
  let downY = 0;
  let didDragFlag = false;

  function regionAt(el: Element): RegionId | null {
    for (const [region, root] of roots) if (root.contains(el)) return region;
    return null;
  }
  function caretAt(bar: HTMLElement, els: HTMLElement[], index: number) {
    if (els.length === 0) {
      const r = bar.getBoundingClientRect();
      return { x: r.left + 6, y: r.top + 5, h: 16 };
    }
    if (index < els.length) {
      const r = els[index].getBoundingClientRect();
      return { x: r.left - 3, y: r.top, h: r.height };
    }
    const r = els[els.length - 1].getBoundingClientRect();
    return { x: r.right + 1, y: r.top, h: r.height };
  }

  return {
    get dragTab() {
      return dragTab;
    },
    get drop() {
      return drop;
    },
    get caret() {
      return caret;
    },
    register(region, el) {
      roots.set(region, el);
    },
    unregister(region) {
      roots.delete(region);
    },
    didDrag() {
      return didDragFlag;
    },
    onTabDown(e, tab) {
      if (e.button !== 0) return;
      downTab = tab;
      downX = e.clientX;
      downY = e.clientY;
      didDragFlag = false;
      try {
        (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
      } catch {
        /* non-fatal */
      }
    },
    onTabMove(e) {
      if (downTab === null) return;
      if (dragTab === null) {
        if (Math.abs(e.clientX - downX) < DRAG_PX && Math.abs(e.clientY - downY) < DRAG_PX) return;
        dragTab = downTab;
        didDragFlag = true;
      }
      const el = document.elementFromPoint(e.clientX, e.clientY);
      const region = el ? regionAt(el) : null;
      if (!el || !region) {
        drop = null;
        caret = null;
        return;
      }
      const root = roots.get(region)!;
      const panels = [...root.querySelectorAll<HTMLElement>(".dock-panel")];
      const barEl = el.closest<HTMLElement>(".tabs");
      if (barEl && root.contains(barEl)) {
        // anywhere over a tab bar joins that panel; the slot is the pointer's
        // reading-order position among the non-dragged tabs.
        const pi = panels.indexOf(barEl.closest<HTMLElement>(".dock-panel") as HTMLElement);
        const els = [...barEl.querySelectorAll<HTMLElement>(".tab")].filter((b) => b.dataset.tab !== dragTab);
        let index = els.length;
        for (let i = 0; i < els.length; i++) {
          const r = els[i].getBoundingClientRect();
          if (e.clientY < r.top || (e.clientY <= r.bottom && e.clientX < r.left + r.width / 2)) {
            index = i;
            break;
          }
        }
        drop = { kind: "tab", region, panel: pi, index };
        caret = caretAt(barEl, els, index);
        return;
      }
      const panelEl = el.closest<HTMLElement>(".dock-panel");
      if (panelEl && root.contains(panelEl)) {
        // over a panel body → split into a new panel above/below
        const pi = panels.indexOf(panelEl);
        const r = panelEl.getBoundingClientRect();
        drop = { kind: "split", region, at: e.clientY < r.top + r.height / 2 ? pi : pi + 1 };
        caret = null;
        return;
      }
      // over a region but not on any panel (e.g. an empty region body) → seed it
      drop = { kind: "split", region, at: 0 };
      caret = null;
    },
    onTabUp() {
      if (dragTab !== null && drop) {
        const ws = getWs();
        const next =
          drop.kind === "tab"
            ? moveTabTo(ws, dragTab, drop.region, drop.panel, drop.index)
            : splitTabTo(ws, dragTab, drop.region, drop.at);
        onchange(next);
      }
      dragTab = null;
      downTab = null;
      drop = null;
      caret = null;
    },
  };
}

export function setDockDrag(d: DockDrag) {
  setContext(KEY, d);
}
export function getDockDrag(): DockDrag {
  return getContext(KEY);
}
