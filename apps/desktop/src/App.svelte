<script lang="ts">
  import { onMount, type Component } from "svelte";
  import Drill from "./components/Drill.svelte";
  import Export from "./components/Export.svelte";
  import Guide from "./components/Guide.svelte";
  import Library from "./components/Library.svelte";
  import Loops from "./components/Loops.svelte";
  import ProfilingPanel from "./components/ProfilingPanel.svelte";
  import Isolation from "./components/Isolation.svelte";
  import Sections from "./components/Sections.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import Transport from "./components/Transport.svelte";
  import Tuner from "./components/Tuner.svelte";
  import Waveform from "./components/Waveform.svelte";
  import { installKeys } from "./lib/keys";
  import { initTheme } from "./lib/theme";
  import { initTrace } from "./lib/trace";
  import { initDecorations } from "./lib/window";
  import { initZoom } from "./lib/zoom";
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
  } from "./lib/stores";

  const ALL_TABS = ["structure", "loops", "export", "profile", "settings", "guide"] as const;
  type Tab = (typeof ALL_TABS)[number];
  // one panel view per tab — the nav and the body both drive off this map
  const TAB_VIEWS: Record<Tab, Component> = {
    structure: Sections,
    loops: Loops,
    export: Export,
    profile: ProfilingPanel,
    settings: SettingsPanel,
    guide: Guide,
  };
  const tabs = ALL_TABS;
  let tab = $state<Tab>("structure");

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
    return () => {
      uninstall();
      void unlisten.then((f) => f());
    };
  });
</script>

<div class="shell" class:lib-collapsed={$libraryCollapsed} class:panels-collapsed={$panelsCollapsed}>
  <aside class="library" class:collapsed={$libraryCollapsed}>
    {#if $libraryCollapsed}
      <button class="rail" onclick={() => actions.toggleLibrary()} title="show library (Ctrl+[)" aria-label="show library">›</button>
    {:else}
      <button class="edge left" onclick={() => actions.toggleLibrary()} title="hide library (Ctrl+[)" aria-label="hide library">‹</button>
      <Library />
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
      {#if $openSong}
        <Isolation />
      {/if}
      <Tuner />
      {#if $openSong && $drillSpan}
        <Drill />
      {/if}
    </div>
  </main>
  <aside class="panels" class:collapsed={$panelsCollapsed}>
    {#if $panelsCollapsed}
      <button class="rail" onclick={() => actions.togglePanels()} title="show panels (Ctrl+])" aria-label="show panels">‹</button>
    {:else}
      <button class="edge right" onclick={() => actions.togglePanels()} title="hide panels (Ctrl+])" aria-label="hide panels">›</button>
      <nav class="tabs">
        {#each tabs as t (t)}
          <button class="tab" class:active={tab === t} onclick={() => (tab = t)}>{t}</button>
        {/each}
      </nav>
      {#key tab}
        {@const View = TAB_VIEWS[tab]}
        <div class="fade-in">
          <View />
        </div>
      {/key}
    {/if}
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

  .library {
    position: relative;
    border-right: 1px solid var(--line);
    padding: var(--space);
    min-width: 0;
    overflow-y: auto;
  }

  /* overflow visible (not hidden) so the rail can bleed past the corner below */
  .library.collapsed,
  .panels.collapsed {
    padding: 0;
    overflow: visible;
    /* the divider belongs to the open pane — once collapsed there's nothing to
       separate, so drop it rather than leave a free-floating vertical line */
    border: none;
  }

  /* thin expand rail shown when a side column is collapsed. Absolutely placed so
     it can BLEED 6px past the top + outer edges (same fractional-HiDPI fix as
     .edge) — a slam to the literal corner/edge then lands inside the rail. */
  .rail {
    position: absolute;
    top: -6px;
    bottom: 0;
    width: auto;
    height: auto;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 14px;
    /* expand chevron stays hidden until the cursor is over the collapsed rail */
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .library.collapsed .rail {
    left: -6px;
    right: 0;
  }
  .panels.collapsed .rail {
    left: 0;
    right: 0;
  }
  .rail:hover {
    background: var(--bg-raised);
    color: var(--fg);
    opacity: 1;
  }

  /* Collapse handle tucked into the window's top outer corner. It BLEEDS 6px
     past the top + outer edges (negative offsets) so the very corner pixel
     (0,0 etc.) lands in the handle's interior — under fractional HiDPI scaling
     (e.g. dpr 1.75) a box flush to the edge snaps just inside it, so a slam to
     the literal corner falls through to the column. The off-screen bleed makes
     the corner-slam reliable; padding pushes the chevron into the visible part. */
  .edge {
    position: absolute;
    top: -6px;
    z-index: 2;
    width: 28px;
    height: 28px;
    background: var(--bg);
    border: 1px solid var(--line);
    color: var(--muted);
    font-size: 11px;
    cursor: pointer;
    /* collapse chevron stays hidden until the cursor nears the corner (its hit
       area is unchanged, so the corner-slam still collapses) */
    opacity: 0;
    transition: opacity 120ms ease;
  }
  .edge:hover {
    color: var(--fg);
    border-color: var(--muted);
    opacity: 1;
  }
  /* square the off-screen (outer) corner; pad the chevron into the on-screen part */
  .edge.left {
    left: -6px;
    padding: 6px 0 0 6px;
    border-top-left-radius: 0;
    border-bottom-right-radius: var(--radius);
  }
  /* right side: the slam lands at (W-1, 0) — already 1px inside horizontally, so
     only the top edge needs the bleed (no rightward bleed → no panel overflow) */
  .edge.right {
    right: 0;
    padding: 6px 6px 0 0;
    border-top-right-radius: 0;
    border-bottom-left-radius: var(--radius);
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



  .panels {
    position: relative;
    border-left: 1px solid var(--line);
    padding: var(--space);
    min-width: 0;
    overflow-x: hidden;
    overflow-y: auto;
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
  }

  .tab:hover {
    color: var(--fg);
    background: var(--bg-raised);
  }

  .tab.active {
    color: var(--accent);
  }


</style>
