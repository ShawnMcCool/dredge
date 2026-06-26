<script lang="ts">
  import { onMount, type Component } from "svelte";
  import { flip } from "svelte/animate";
  import ClickTrack from "./components/ClickTrack.svelte";
  import Devices from "./components/Devices.svelte";
  import Drill from "./components/Drill.svelte";
  import Export from "./components/Export.svelte";
  import Guide from "./components/Guide.svelte";
  import Library from "./components/Library.svelte";
  import Loops from "./components/Loops.svelte";
  import MetronomeBox from "./components/MetronomeBox.svelte";
  import ProfilingPanel from "./components/ProfilingPanel.svelte";
  import Isolation from "./components/Isolation.svelte";
  import Notes from "./components/Notes.svelte";
  import Recordings from "./components/Recordings.svelte";
  import Routines from "./components/Routines.svelte";
  import Sections from "./components/Sections.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import Transport from "./components/Transport.svelte";
  import Tuner from "./components/Tuner.svelte";
  import Waveform from "./components/Waveform.svelte";
  import { installKeys } from "./lib/keys";
  import { initTheme } from "./lib/theme";
  import { initTrace } from "./lib/trace";
  import { initDecorations } from "./lib/window";
  import { initZoom, resyncZoom } from "./lib/zoom";
  import {
    actions,
    drillSpan,
    initEvents,
    libraryCollapsed,
    loopsOpen,
    openSong,
    panelsCollapsed,
    sectionsOpen,
    settingsOpen,
    panelLayout,
  } from "./lib/stores";
  import { reconcile, moveTab, splitTab, setActive, setWeights, type DockLayout } from "./lib/dock";

  const ALL_TABS = ["structure", "loops", "routines", "export", "profile", "devices", "settings", "guide"] as const;
  type Tab = (typeof ALL_TABS)[number];
  // one panel view per tab — the nav and the body both drive off this map
  const TAB_VIEWS: Record<Tab, Component> = {
    structure: Sections,
    loops: Loops,
    routines: Routines,
    export: Export,
    profile: ProfilingPanel,
    devices: Devices,
    settings: SettingsPanel,
    guide: Guide,
  };
  // The dock: the right aside is a vertical stack of panels, each a set of tabs.
  // The layout is reconciled against the known tabs (adding/removing one in code
  // stays graceful).
  const layout = $derived(reconcile($panelLayout, [...ALL_TABS]));

  // Drag a tab to reorder it (drop on a tab bar), join it into another panel
  // (drop on that bar), or split it into a new panel (drop on a panel's body —
  // top half = above, bottom half = below). A drop indicator previews where it
  // lands; the move commits on release (FLIP animates the result). A small move
  // turns a click into a drag; a plain click selects the tab in its panel.
  type Drop = { kind: "tab"; panel: number; index: number } | { kind: "split"; at: number };
  let dragTab = $state<string | null>(null);
  let drop = $state<Drop | null>(null);
  // insertion caret (viewport coords) shown while hovering a tab bar
  let caret = $state<{ x: number; y: number; h: number } | null>(null);
  let downTab: string | null = null;
  let downX = 0;
  let downY = 0;
  let didDrag = false;
  const DRAG_PX = 4;

  function panelOf(l: DockLayout, t: string): number {
    return l.findIndex((p) => p.tabs.includes(t));
  }
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
    const barEl = el?.closest<HTMLElement>(".tabs");
    if (barEl) {
      // anywhere over a tab bar — a tab OR the gap between tabs — joins that
      // panel. The slot is the pointer's reading-order position among the bar's
      // tabs (wrap-aware), so there's no dead gap that falls through to a split.
      const panelEl = barEl.closest<HTMLElement>(".dock-panel");
      const panels = [...document.querySelectorAll<HTMLElement>(".dock .dock-panel")];
      const pi = panels.indexOf(panelEl as HTMLElement);
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
    const panelEl = el?.closest<HTMLElement>(".dock-panel");
    if (panelEl) {
      // over a panel body → split into a new panel above/below
      const panels = [...document.querySelectorAll<HTMLElement>(".dock .dock-panel")];
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
          ? moveTab(layout, dragTab, drop.panel, drop.index)
          : splitTab(layout, dragTab, drop.at);
      void actions.setPanelLayout(next);
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
    void actions.setPanelLayout(setActive(layout, panel, t));
  }

  // Vertical resize: a splitter at panel `i`'s top edge shifts the boundary
  // between panel i-1 (above) and i (below), trading their weights. Previewed
  // live via `resizeWeights`, persisted on release.
  let resizeWeights = $state<number[] | null>(null);
  let resizeIdx = 0;
  let resizeStartY = 0;
  let resizeH: [number, number] = [0, 0];
  let resizeCombined = 0;
  const MIN_PANEL_PX = 64;

  function panelWeight(pi: number): number {
    return resizeWeights ? resizeWeights[pi] : layout[pi].weight;
  }
  function onSplitDown(e: PointerEvent, i: number) {
    const panels = [...document.querySelectorAll<HTMLElement>(".dock .dock-panel")];
    const above = panels[i - 1];
    const below = panels[i];
    if (!above || !below) return;
    e.preventDefault();
    resizeIdx = i;
    resizeStartY = e.clientY;
    resizeH = [above.offsetHeight, below.offsetHeight];
    resizeCombined = layout[i - 1].weight + layout[i].weight;
    resizeWeights = layout.map((p) => p.weight);
    try {
      (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    } catch {
      /* non-fatal */
    }
  }
  function onSplitMove(e: PointerEvent) {
    if (!resizeWeights) return;
    const total = resizeH[0] + resizeH[1];
    const h0 = Math.max(MIN_PANEL_PX, Math.min(total - MIN_PANEL_PX, resizeH[0] + (e.clientY - resizeStartY)));
    const w = resizeWeights.slice();
    w[resizeIdx - 1] = resizeCombined * (h0 / total);
    w[resizeIdx] = resizeCombined * ((total - h0) / total);
    resizeWeights = w;
  }
  function onSplitUp() {
    if (resizeWeights) void actions.setPanelLayout(setWeights(layout, resizeWeights));
    resizeWeights = null;
  }

  /** Bring `key` to the front of whichever panel holds it — the open-settings /
   *  open-structure / open-loops shortcuts. */
  function revealTab(key: Tab) {
    const pi = panelOf(layout, key);
    if (pi >= 0) void actions.setPanelLayout(setActive(layout, pi, key));
  }

  $effect(() => {
    if ($settingsOpen) {
      revealTab("settings");
      settingsOpen.set(false);
    }
  });

  $effect(() => {
    if ($sectionsOpen) {
      revealTab("structure");
      sectionsOpen.set(false);
    }
  });

  $effect(() => {
    if ($loopsOpen) {
      revealTab("loops");
      loopsOpen.set(false);
    }
  });

  onMount(() => {
    void initTrace();
    // settings drive zoom (ui_scale), the window frame, and session defaults
    void actions.loadSettings().then(() => {
      void initZoom();
      void initDecorations();
      initTheme();
    });
    const unlisten = initEvents();
    const uninstall = installKeys();
    // Suppress the webview's native right-click menu app-wide so dredge reads
    // as a desktop app, not a web page. Right-click gestures (waveform + tab
    // resize) are driven by pointerdown, so this doesn't disturb them.
    const blockContextMenu = (e: MouseEvent) => e.preventDefault();
    window.addEventListener("contextmenu", blockContextMenu);
    // A viewport resize (esp. fullscreen) can desync the webview's render scale
    // from its hit-test scale, drifting clicks. Re-assert the zoom once the
    // resize settles to resync them.
    let zoomResync: ReturnType<typeof setTimeout> | undefined;
    const onResize = () => {
      clearTimeout(zoomResync);
      zoomResync = setTimeout(() => void resyncZoom(), 150);
    };
    window.addEventListener("resize", onResize);
    return () => {
      uninstall();
      void unlisten.then((f) => f());
      window.removeEventListener("contextmenu", blockContextMenu);
      window.removeEventListener("resize", onResize);
      clearTimeout(zoomResync);
    };
  });
</script>

<div class="shell" class:lib-collapsed={$libraryCollapsed} class:panels-collapsed={$panelsCollapsed}>
  <aside class="library" class:collapsed={$libraryCollapsed}>
    <button
      class="rail"
      onclick={() => actions.toggleLibrary()}
      title={$libraryCollapsed ? "show library (Ctrl+[)" : "hide library (Ctrl+[)"}
      aria-label={$libraryCollapsed ? "show library" : "hide library"}
    >{$libraryCollapsed ? "›" : "‹"}</button>
    {#if !$libraryCollapsed}
      <div class="pane"><Library /></div>
    {/if}
  </aside>
  <main class="stage">
    <Waveform />
    {#if $openSong}
      <Transport />
    {/if}
    <!-- boxes flow to fill the stage width and wrap to the next row as they run
         out of room; every row (even a lone box) spans the full width. The tuner
         is always present (useful with no song open); the song-scoped boxes join
         the row once a track is open. The drill sits last, after the standing
         boxes, since it only appears mid-practice. -->
    <div class="boxes">
      <MetronomeBox />
      {#if $openSong}
        <Isolation />
        <ClickTrack />
        <Notes />
        <Recordings />
      {/if}
      <Tuner />
      {#if $openSong && $drillSpan}
        <Drill />
      {/if}
    </div>
  </main>
  <aside class="panels" class:collapsed={$panelsCollapsed}>
    {#if !$panelsCollapsed}
      <div class="dock">
        {#each layout as panel, pi (pi)}
          <section
            class="dock-panel"
            class:droptab={drop?.kind === "tab" && drop.panel === pi}
            class:splitabove={drop?.kind === "split" && drop.at === pi}
            class:splitbelow={drop?.kind === "split" && drop.at === pi + 1 && pi === layout.length - 1}
            style="flex-grow: {panelWeight(pi)}"
          >
            {#if pi > 0}
              <div
                class="splitter"
                onpointerdown={(e) => onSplitDown(e, pi)}
                onpointermove={onSplitMove}
                onpointerup={onSplitUp}
                onpointercancel={onSplitUp}
                title="drag to resize"
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
                {@const View = TAB_VIEWS[panel.active as Tab]}
                <div class="fade-in">
                  <View />
                </div>
              {/key}
            </div>
          </section>
        {/each}
      </div>
    {/if}
    <button
      class="rail"
      onclick={() => actions.togglePanels()}
      title={$panelsCollapsed ? "show panels (Ctrl+])" : "hide panels (Ctrl+])"}
      aria-label={$panelsCollapsed ? "show panels" : "hide panels"}
    >{$panelsCollapsed ? "‹" : "›"}</button>
  </aside>
  {#if dragTab && drop?.kind === "tab" && caret}
    <div class="drop-caret" style="left: {caret.x}px; top: {caret.y}px; height: {caret.h}px"></div>
  {/if}
</div>

<style>
  .shell {
    /* per-column widths as custom props so collapse + the responsive media
       query can each set them without fighting over one shorthand */
    --col-lib: minmax(170px, 240px);
    --col-center: minmax(320px, 1fr);
    --col-panels: minmax(250px, 340px);
    --rail-w: 22px;
    display: grid;
    grid-template-columns: var(--col-lib) var(--col-center) var(--col-panels);
    height: 100vh;
  }

  /* below the point where the preferred minimums fit, shrink all three
     columns further instead of pushing the right rail off-screen */
  @media (max-width: 745px) {
    .shell {
      --col-lib: minmax(110px, 240px);
      --col-center: minmax(220px, 1fr);
      --col-panels: minmax(130px, 340px);
    }
  }

  /* collapsed side columns become thin rails (two-class specificity beats the
     media query's single-class rule, so collapse holds at every width) */
  .shell.lib-collapsed {
    --col-lib: var(--rail-w);
  }
  .shell.panels-collapsed {
    --col-panels: var(--rail-w);
  }

  /* Each aside is a flex row: an always-present full-height rail on the outer
     edge plus the slidable pane. The rail is the single collapse/expand handle —
     a slam to the window's outer edge lands on it at any height. */
  .library {
    display: flex;
    flex-direction: row;
    border-right: 1px solid var(--line);
    min-width: 0;
  }
  .panels {
    display: flex;
    flex-direction: row;
    border-left: 1px solid var(--line);
    min-width: 0;
  }
  /* collapsed: only the rail remains, so the divider has nothing to separate */
  .library.collapsed,
  .panels.collapsed {
    border: none;
  }

  /* full-height edge rail — toggles its pane. Expanded → collapse; collapsed →
     expand. Stays quiet (chevron hidden) until hovered, like the old handles. */
  .rail {
    flex: 0 0 var(--rail-w);
    align-self: stretch;
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 14px;
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .rail:hover {
    background: var(--bg-raised);
    color: var(--fg);
    opacity: 1;
  }

  /* the slidable content inboard of the rail */
  .pane {
    flex: 1 1 auto;
    min-width: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding: var(--space);
  }

  /* the dock: a vertical stack of panels filling the aside inboard of the rail */
  .dock {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
  }
  .dock-panel {
    position: relative;
    display: flex;
    flex-direction: column;
    flex-basis: 0; /* flex-grow (inline, = weight) shares height */
    min-height: 0;
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
  .dock-panel + .dock-panel {
    border-top: 1px solid var(--line);
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

  .stage {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding: var(--space);
  }

  /* boxes pack horizontally, wrap when they run out of room, and each row grows
     to fill the full stage width */
  .boxes {
    display: flex;
    flex-wrap: wrap;
    align-items: stretch;
    gap: var(--space);
    padding: var(--space) 0;
    min-width: 0;
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

  /* generous, solid hit area — the whole padded chip is clickable, not just the
     glyphs. Small targets got unreliable when the panel wrapped to more rows at
     narrow (tiled) widths under fractional webview zoom. */
  .tab {
    /* natural width, left-aligned; the row-justify lives on `.tabs` */
    flex: 0 0 auto;
    text-align: center;
    background: none;
    border: none;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    padding: 6px 10px;
    border-radius: var(--radius);
    cursor: pointer;
    line-height: 1;
    touch-action: none; /* pointer drag-to-reorder, not scroll */
  }

  .tab:hover {
    color: var(--fg);
    background: var(--bg-raised);
  }

  .tab.active {
    color: var(--accent);
  }

  .tab.dragging {
    background: var(--bg-raised);
    color: var(--fg);
    opacity: 0.85;
    cursor: grabbing;
  }


</style>
