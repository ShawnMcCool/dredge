<script lang="ts">
  // A dock renders ONE region of the workspace: a vertical stack of resizable
  // panels, each holding an ordered set of tabs (pages). Tab drags — reorder,
  // join into another panel, or split into a new one, including ACROSS to the
  // other region — are driven by the shared coordinator (`lib/dock-drag`), which
  // is the only thing that can see both regions at once. Within-region resize
  // (splitters / right-drag-snap) stays here, since it never crosses regions.
  // Pure layout transforms live in `lib/dock.ts`; this component renders the
  // region's layout and reads the coordinator's drag state.
  import type { Component } from "svelte";
  import { flip } from "svelte/animate";
  import { setActive, setWeights, type DockLayout, type RegionId } from "../dock";
  import { getDockDrag } from "../dock-drag.svelte";

  interface Props {
    /** Which region this dock renders. */
    region: RegionId;
    /** This region's panels (already reconciled by the parent). */
    layout: DockLayout;
    /** Tab key → the view rendered when that tab is the panel's active one. */
    views: Record<string, Component>;
    /** New layout for THIS region after a within-region change (select / resize).
     *  Cross-region tab moves go through the coordinator, not this. */
    onlayout: (layout: DockLayout) => void;
  }
  let { region, layout, views, onlayout }: Props = $props();

  const drag = getDockDrag();
  let dockEl: HTMLElement;

  // register this region's root so the coordinator can hit-test drops into it
  $effect(() => {
    drag.register(region, dockEl);
    return () => drag.unregister(region);
  });

  // coordinator drag state, scoped to this region for the drop affordances
  const drop = $derived(drag.drop);
  const caret = $derived(drag.caret);
  const tabDropHere = $derived(drop?.kind === "tab" && drop.region === region);
  const showCaret = $derived(drag.dragTab !== null && tabDropHere && caret !== null);

  function selectTab(panel: number, t: string) {
    if (drag.didDrag()) return; // the pointer gesture was a drag, not a select
    onlayout(setActive(layout, panel, t));
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
    return resizeWeights ? resizeWeights[pi] : layout[pi].weight;
  }
  function beginResize(i: number, clientY: number, captureEl: HTMLElement, pointerId: number) {
    const panels = [...dockEl.querySelectorAll<HTMLElement>(".dock-panel")];
    const above = panels[i - 1];
    const below = panels[i];
    if (!above || !below) return;
    resizeIdx = i;
    resizeStartY = clientY;
    resizeH = [above.offsetHeight, below.offsetHeight];
    resizeCombined = layout[i - 1].weight + layout[i].weight;
    resizeWeights = layout.map((p) => p.weight);
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
    if (resizeWeights && resizeMoved) onlayout(setWeights(layout, resizeWeights));
    resizeWeights = null;
  }
</script>

<!-- the dock's pointer handlers drive the right-drag-snap resize (a mouse
     enhancement; the splitter handle is the discoverable affordance) -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
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
  {#each layout as panel, pi (pi)}
    <section
      class="dock-panel"
      class:droptab={tabDropHere && drop?.kind === "tab" && drop.panel === pi}
      class:splitabove={drop?.kind === "split" && drop.region === region && drop.at === pi}
      class:splitbelow={drop?.kind === "split" &&
        drop.region === region &&
        drop.at === pi + 1 &&
        pi === layout.length - 1}
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
            class:dragging={drag.dragTab === t}
            data-tab={t}
            data-panel={pi}
            onpointerdown={(e) => drag.onTabDown(e, t)}
            onpointermove={(e) => drag.onTabMove(e)}
            onpointerup={() => drag.onTabUp()}
            onpointercancel={() => drag.onTabUp()}
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
{#if showCaret && caret}
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
  /* a clear seam between stacked panels — the line plus the next panel's raised
     header banner make each boundary obvious */
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

  /* a panel's tab bar — a raised header banner pinned at the panel top (the view
     scrolls below it), so each stacked panel reads as its own header + body */
  .tabs {
    flex: 0 0 auto;
    background: var(--bg-raised);
    /* lift the banner off the content so each stacked panel clearly reads as
       its own header + body */
    box-shadow: 0 2px 5px -2px rgb(0 0 0 / 0.45);
    z-index: 1;
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
