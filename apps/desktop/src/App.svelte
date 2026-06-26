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
    tabOrder,
  } from "./lib/stores";

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
  let tab = $state<Tab>("structure");

  // Effective tab order: the persisted order (known tabs only) followed by any
  // tabs added in code since — so reordering survives, and a new/removed tab
  // stays graceful. A live drag renders from `dragOrder` instead.
  function isTab(t: string): t is Tab {
    return (ALL_TABS as readonly string[]).includes(t);
  }
  const orderedTabs = $derived.by<Tab[]>(() => {
    const known = $tabOrder.filter(isTab);
    const missing = ALL_TABS.filter((t) => !known.includes(t));
    return [...known, ...missing];
  });
  let dragOrder = $state<Tab[] | null>(null);
  const shownTabs = $derived(dragOrder ?? orderedTabs);

  // Drag-to-reorder the tabs. A small move turns a click into a drag; on each
  // move the dragged tab swaps into the slot of whatever tab is under the
  // pointer (FLIP animates the rest). Persist on drop.
  let dragKey = $state<Tab | null>(null);
  let downKey: Tab | null = null;
  let downX = 0;
  let downY = 0;
  let didDrag = false;
  const DRAG_PX = 4;

  function onTabDown(e: PointerEvent, t: Tab) {
    if (e.button !== 0) return;
    downKey = t;
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
    if (downKey === null) return;
    if (dragKey === null) {
      if (Math.abs(e.clientX - downX) < DRAG_PX && Math.abs(e.clientY - downY) < DRAG_PX) return;
      dragKey = downKey;
      didDrag = true;
      dragOrder = [...orderedTabs];
    }
    const el = document.elementFromPoint(e.clientX, e.clientY)?.closest<HTMLElement>("[data-tab]");
    const over = el?.dataset.tab;
    if (!over || !dragOrder || over === dragKey) return;
    const from = dragOrder.indexOf(dragKey);
    const to = dragOrder.findIndex((t) => t === over);
    if (from === -1 || to === -1 || from === to) return;
    const next = dragOrder.slice();
    next.splice(to, 0, next.splice(from, 1)[0]);
    dragOrder = next;
  }
  function onTabUp() {
    if (dragKey !== null && dragOrder) void actions.setTabOrder(dragOrder);
    dragKey = null;
    downKey = null;
    dragOrder = null;
  }
  function onTabClick(t: Tab) {
    if (didDrag) {
      didDrag = false;
      return; // the pointer gesture was a reorder, not a select
    }
    tab = t;
  }

  $effect(() => {
    if ($settingsOpen) {
      tab = "settings";
      settingsOpen.set(false);
    }
  });

  $effect(() => {
    if ($sectionsOpen) {
      tab = "structure";
      sectionsOpen.set(false);
    }
  });

  $effect(() => {
    if ($loopsOpen) {
      tab = "loops";
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
      <div class="pane">
        <nav class="tabs">
          {#each shownTabs as t (t)}
            <button
              class="tab"
              class:active={tab === t}
              class:dragging={dragKey === t}
              data-tab={t}
              onpointerdown={(e) => onTabDown(e, t)}
              onpointermove={onTabMove}
              onpointerup={onTabUp}
              onpointercancel={onTabUp}
              onclick={() => onTabClick(t)}
              animate:flip={{ duration: 180 }}
              title="drag to reorder"
            >
              {t}
            </button>
          {/each}
        </nav>
        {#key tab}
          {@const View = TAB_VIEWS[tab]}
          <div class="fade-in">
            <View />
          </div>
        {/key}
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




  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: calc(var(--space) / 2);
    margin-bottom: var(--space);
    border-bottom: 1px solid var(--line);
    padding-bottom: var(--space);
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
