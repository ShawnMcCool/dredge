<script lang="ts">
  import { onMount, type Component } from "svelte";
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
  import Dock from "./lib/ui/Dock.svelte";
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
  // The right aside is a reusable Dock (lib/ui/Dock.svelte) over these tabs; its
  // layout persists in `panelLayout`. `dock.reveal(tab)` activates a tab for the
  // open-settings / open-structure / open-loops shortcuts below.
  let dock: { reveal: (key: string) => void } | undefined = $state();

  $effect(() => {
    if ($settingsOpen) {
      dock?.reveal("settings");
      settingsOpen.set(false);
    }
  });

  $effect(() => {
    if ($sectionsOpen) {
      dock?.reveal("structure");
      sectionsOpen.set(false);
    }
  });

  $effect(() => {
    if ($loopsOpen) {
      dock?.reveal("loops");
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
      <Dock
        bind:this={dock}
        layout={$panelLayout}
        tabs={ALL_TABS}
        views={TAB_VIEWS}
        onchange={(l) => void actions.setPanelLayout(l)}
      />
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




</style>
