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
  import { reconcile, moveTab, splitTab, setActive, type DockLayout } from "./lib/dock";

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
  let downTab: string | null = null;
  let downX = 0;
  let downY = 0;
  let didDrag = false;
  const DRAG_PX = 4;

  function panelOf(l: DockLayout, t: string): number {
    return l.findIndex((p) => p.tabs.includes(t));
  }
  /** Insertion index in a panel (excluding the dragged tab) for a drop landing
   *  before/after `overTab`. */
  function dropIndex(tabs: string[], overTab: string, after: boolean, dragged: string): number {
    const without = tabs.filter((t) => t !== dragged);
    const i = without.indexOf(overTab);
    if (i === -1) return without.length;
    return after ? i + 1 : i;
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
    const tabEl = el?.closest<HTMLElement>("[data-tab]");
    if (tabEl && tabEl.dataset.tab !== dragTab) {
      const panel = Number(tabEl.dataset.panel);
      const r = tabEl.getBoundingClientRect();
      const after = e.clientX > r.left + r.width / 2;
      drop = { kind: "tab", panel, index: dropIndex(layout[panel].tabs, tabEl.dataset.tab!, after, dragTab) };
      return;
    }
    const panelEl = el?.closest<HTMLElement>(".dock-panel");
    if (panelEl) {
      const panels = [...document.querySelectorAll(".dock .dock-panel")];
      const pi = panels.indexOf(panelEl);
      const r = panelEl.getBoundingClientRect();
      drop = { kind: "split", at: e.clientY < r.top + r.height / 2 ? pi : pi + 1 };
      return;
    }
    drop = null; // over the dragged tab itself or outside → no-op on release
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
  }
  function selectTab(panel: number, t: string) {
    if (didDrag) {
      didDrag = false;
      return; // the pointer gesture was a drag, not a select
    }
    void actions.setPanelLayout(setActive(layout, panel, t));
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
            style="flex-grow: {panel.weight}"
          >
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
    display: flex;
    flex-direction: column;
    flex-basis: 0; /* flex-grow (inline, = weight) shares height */
    min-height: 0;
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
    gap: calc(var(--space) / 2);
    padding: calc(var(--space) / 2) var(--space);
    border-bottom: 1px solid var(--line);
    min-width: 0;
  }

  /* generous, solid hit area — the whole padded chip is clickable, not just the
     glyphs. Small targets got unreliable when the panel wrapped to more rows at
     narrow (tiled) widths under fractional webview zoom. */
  .tab {
    /* grow to fill the row so every pixel of each wrapped line is a button —
       a sparse last row (e.g. settings/guide) left wide dead strips of bare
       container where the finger cursor flickered off. */
    flex: 1 1 auto;
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
