<script lang="ts">
  // A reusable dock: a vertical stack of resizable panels, each holding an
  // ordered set of tabs (pages). Tabs drag to reorder within a panel, join into
  // another panel, or split into a new one; splitters (or a right-drag anywhere,
  // which snaps to the nearest break) resize. The pure layout transforms live in
  // `lib/dock.ts`; this component renders them and reads the drag gestures. It is
  // self-contained — every DOM query is scoped to its own root — so multiple
  // docks can coexist.
  import type { Component } from "svelte";
  import { flip } from "svelte/animate";
  import { reconcile, moveTab, splitTab, setActive, setWeights, type DockLayout } from "../dock";

  interface Props {
    /** Persisted layout (may be empty/stale; reconciled internally for render). */
    layout: DockLayout;
    /** Known tab keys, in code order. */
    tabs: readonly string[];
    /** Tab key → the view rendered when that tab is the panel's active one. */
    views: Record<string, Component>;
    /** New layout on any reorder / join / split / resize / tab switch. The
     *  parent owns persistence. */
    onchange: (layout: DockLayout) => void;
  }
  let { layout, tabs, views, onchange }: Props = $props();

  const effective = $derived(reconcile(layout, [...tabs]));
  let dockEl: HTMLElement;

  // ── tab drag: reorder / join / split, with an insertion caret ──────────────
  type Drop = { kind: "tab"; panel: number; index: number } | { kind: "split"; at: number };
  let dragTab = $state<string | null>(null);
  let drop = $state<Drop | null>(null);
  let caret = $state<{ x: number; y: number; h: number } | null>(null);
  let downTab: string | null = null;
  let downX = 0;
  let downY = 0;
  let didDrag = false;
  const DRAG_PX = 4;

  function onTabDown(e: PointerEvent, t: string) {
    if (e.button !== 0) return;
    downTab = t;
    downX = e.clientX;
    downY = e.clientY;
    didDrag = false;
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* non-fatal */
    }
  }
  function onTabMove(e: PointerEvent) {
    if (downTab === null) return;
    if (dragTab === null) {
      if (Math.abs(e.clientX - downX) < DRAG_PX && Math.abs(e.clientY - downY) < DRAG_PX) return;
      dragTab = downTab;
      didDrag = true;
    }
    const el = document.elementFromPoint(e.clientX, e.clientY);
    if (!el || !dockEl.contains(el)) {
      drop = null;
      caret = null;
      return;
    }
    const panels = [...dockEl.querySelectorAll<HTMLElement>(".dock-panel")];
    const barEl = el.closest<HTMLElement>(".tabs");
    if (barEl) {
      // anywhere over a tab bar — a tab OR the gap between tabs — joins that
      // panel; the slot is the pointer's reading-order position among the tabs.
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
      drop = { kind: "tab", panel: pi, index };
      caret = caretAt(barEl, els, index);
      return;
    }
    const panelEl = el.closest<HTMLElement>(".dock-panel");
    if (panelEl) {
      // over a panel body → split into a new panel above/below
      const pi = panels.indexOf(panelEl);
      const r = panelEl.getBoundingClientRect();
      drop = { kind: "split", at: e.clientY < r.top + r.height / 2 ? pi : pi + 1 };
      caret = null;
      return;
    }
    drop = null;
    caret = null;
  }
  /** Insertion-caret rect (viewport coords) for slot `index` among a bar's
   *  non-dragged tabs `els`. */
  function caretAt(bar: HTMLElement, els: HTMLElement[], index: number): { x: number; y: number; h: number } {
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
  function onTabUp() {
    if (dragTab !== null && drop) {
      const next =
        drop.kind === "tab"
          ? moveTab(effective, dragTab, drop.panel, drop.index)
          : splitTab(effective, dragTab, drop.at);
      onchange(next);
    }
    dragTab = null;
    downTab = null;
    drop = null;
    caret = null;
  }
  function selectTab(panel: number, t: string) {
    if (didDrag) {
      didDrag = false;
      return; // the pointer gesture was a drag, not a select
    }
    onchange(setActive(effective, panel, t));
  }

  /** Bring `key` to the front of whichever panel holds it — for host shortcuts
   *  (e.g. "open settings"). */
  export function reveal(key: string) {
    const pi = effective.findIndex((p) => p.tabs.includes(key));
    if (pi >= 0) onchange(setActive(effective, pi, key));
  }

  // ── vertical resize: splitters + right-drag snap-to-nearest break ──────────
  let resizeWeights = $state<number[] | null>(null);
  let resizeIdx = 0;
  let resizeStartY = 0;
  let resizeH: [number, number] = [0, 0];
  let resizeCombined = 0;
  let resizeMoved = false;
  const MIN_PANEL_PX = 64;

  function panelWeight(pi: number): number {
    return resizeWeights ? resizeWeights[pi] : effective[pi].weight;
  }
  function beginResize(i: number, clientY: number, captureEl: HTMLElement, pointerId: number) {
    const panels = [...dockEl.querySelectorAll<HTMLElement>(".dock-panel")];
    const above = panels[i - 1];
    const below = panels[i];
    if (!above || !below) return;
    resizeIdx = i;
    resizeStartY = clientY;
    resizeH = [above.offsetHeight, below.offsetHeight];
    resizeCombined = effective[i - 1].weight + effective[i].weight;
    resizeWeights = effective.map((p) => p.weight);
    resizeMoved = false;
    try {
      captureEl.setPointerCapture(pointerId);
    } catch {
      /* non-fatal */
    }
  }
  function onSplitDown(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    e.preventDefault();
    beginResize(i, e.clientY, e.currentTarget as HTMLElement, e.pointerId);
  }
  /** The panel boundary (1..N-1) nearest a vertical position, or null with <2
   *  panels. */
  function nearestBoundary(y: number): number | null {
    const panels = [...dockEl.querySelectorAll<HTMLElement>(".dock-panel")];
    if (panels.length < 2) return null;
    let best = 1;
    let bestDist = Infinity;
    for (let i = 1; i < panels.length; i++) {
      const by = (panels[i - 1].getBoundingClientRect().bottom + panels[i].getBoundingClientRect().top) / 2;
      const d = Math.abs(y - by);
      if (d < bestDist) {
        bestDist = d;
        best = i;
      }
    }
    return best;
  }
  // right-drag anywhere in the dock grabs the nearest break and resizes it —
  // mirrors the waveform's right-drag-resizes-the-nearest-edge.
  function onDockPointerDown(e: PointerEvent) {
    if (e.button !== 2) return;
    const i = nearestBoundary(e.clientY);
    if (i === null) return;
    e.preventDefault();
    beginResize(i, e.clientY, dockEl, e.pointerId);
  }
  function onResizeMove(e: PointerEvent) {
    if (!resizeWeights) return;
    resizeMoved = true;
    const total = resizeH[0] + resizeH[1];
    const h0 = Math.max(MIN_PANEL_PX, Math.min(total - MIN_PANEL_PX, resizeH[0] + (e.clientY - resizeStartY)));
    const w = resizeWeights.slice();
    w[resizeIdx - 1] = resizeCombined * (h0 / total);
    w[resizeIdx] = resizeCombined * ((total - h0) / total);
    resizeWeights = w;
  }
  function onResizeUp() {
    if (resizeWeights && resizeMoved) onchange(setWeights(effective, resizeWeights));
    resizeWeights = null;
  }
</script>

<div
  class="dock"
  class:resizing={resizeWeights !== null}
  bind:this={dockEl}
  onpointerdown={onDockPointerDown}
  onpointermove={onResizeMove}
  onpointerup={onResizeUp}
  onpointercancel={onResizeUp}
  oncontextmenu={(e) => e.preventDefault()}
>
  {#each effective as panel, pi (pi)}
    <section
      class="dock-panel"
      class:droptab={drop?.kind === "tab" && drop.panel === pi}
      class:splitabove={drop?.kind === "split" && drop.at === pi}
      class:splitbelow={drop?.kind === "split" && drop.at === pi + 1 && pi === effective.length - 1}
      style="flex-grow: {panelWeight(pi)}"
    >
      {#if pi > 0}
        <div
          class="splitter"
          onpointerdown={(e) => onSplitDown(e, pi)}
          onpointermove={onResizeMove}
          onpointerup={onResizeUp}
          onpointercancel={onResizeUp}
          title="drag to resize · right-drag anywhere snaps to the nearest break"
          role="separator"
          aria-orientation="horizontal"
        ></div>
      {/if}
      <nav class="tabs">
        {#each panel.tabs as t (t)}
          <button
            class="tab"
            class:active={panel.active === t}
            class:dragging={dragTab === t}
            data-tab={t}
            data-panel={pi}
            onpointerdown={(e) => onTabDown(e, t)}
            onpointermove={onTabMove}
            onpointerup={onTabUp}
            onpointercancel={onTabUp}
            onclick={() => selectTab(pi, t)}
            animate:flip={{ duration: 180 }}
            title="drag to reorder"
          >
            {t}
          </button>
        {/each}
        <span class="tab-spacer" aria-hidden="true"></span>
      </nav>
      <div class="panel-view">
        {#key panel.active}
          {@const View = views[panel.active]}
          <div class="fade-in">
            <View />
          </div>
        {/key}
      </div>
    </section>
  {/each}
</div>
{#if dragTab && drop?.kind === "tab" && caret}
  <div class="drop-caret" style="left: {caret.x}px; top: {caret.y}px; height: {caret.h}px"></div>
{/if}

<style>
  /* a vertical stack of panels filling its container */
  .dock {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
  }
  .dock.resizing {
    cursor: row-resize;
  }
  .dock-panel {
    position: relative;
    display: flex;
    flex-direction: column;
    flex-basis: 0; /* flex-grow (inline, = weight) shares height */
    min-height: 0;
  }
  .dock-panel + .dock-panel {
    border-top: 1px solid var(--line);
  }
  /* draggable splitter overlaying a panel's top edge (the boundary above it) */
  .splitter {
    position: absolute;
    top: -3px;
    left: 0;
    right: 0;
    height: 7px;
    z-index: 4;
    cursor: row-resize;
    touch-action: none;
  }
  .splitter:hover {
    background: var(--accent-dim);
  }
  /* drop indicators while dragging a tab */
  .dock-panel.droptab {
    box-shadow: inset 0 0 0 1px var(--accent-dim); /* join into this panel */
  }
  .dock-panel.splitabove {
    box-shadow: inset 0 2px 0 0 var(--accent); /* new panel above */
  }
  .dock-panel.splitbelow {
    box-shadow: inset 0 -2px 0 0 var(--accent); /* new panel below */
  }
  /* insertion caret — the slot a dragged tab will join; glides between slots */
  .drop-caret {
    position: fixed;
    width: 2px;
    background: var(--accent);
    border-radius: 1px;
    z-index: 50;
    pointer-events: none;
    transition:
      left 90ms ease,
      top 90ms ease,
      height 90ms ease;
  }
  /* each panel's active view scrolls on its own */
  .panel-view {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding: var(--space);
  }

  /* a panel's tab bar — fixed at the panel top, the view scrolls below it */
  .tabs {
    flex: 0 0 auto;
    display: flex;
    flex-wrap: wrap;
    /* left-aligned tabs; completed (wrapped) rows justify to fill the width.
       space-between justifies every full row, and the trailing `.tab-spacer`
       (flex-grow) only ever lands on the last row — it eats that row's free
       space so the last row stays left-aligned. */
    justify-content: space-between;
    gap: calc(var(--space) / 2);
    padding: calc(var(--space) / 2) var(--space);
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }
  .tab-spacer {
    flex: 1 1 auto;
    min-width: 0;
  }

  .tab {
    /* natural width, left-aligned; the row-justify lives on `.tabs` */
    position: relative;
    flex: 0 0 auto;
    text-align: center;
    background: none;
    border: none;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    padding: 5px 8px 7px;
    cursor: pointer;
    line-height: 1;
    touch-action: none; /* pointer drag-to-reorder, not scroll */
  }
  /* selected/hover anchor: an underline under the tab — the classic tab metaphor,
     so the strip reads as tabs and the active page is obvious */
  .tab::after {
    content: "";
    position: absolute;
    left: 4px;
    right: 4px;
    bottom: 0;
    height: 2px;
    border-radius: 1px;
    background: transparent;
    transition:
      background-color 100ms ease,
      opacity 100ms ease;
  }
  .tab:hover {
    color: var(--fg);
  }
  .tab:hover::after {
    background: var(--accent-dim);
  }
  .tab.active {
    color: var(--accent);
  }
  .tab.active::after {
    background: var(--accent);
  }
  .tab.dragging {
    color: var(--fg);
    opacity: 0.4;
    cursor: grabbing;
  }
</style>
