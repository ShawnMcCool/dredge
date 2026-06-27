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
  import DockRegion from "./lib/ui/DockRegion.svelte";
  import { createDockDrag, setDockDrag } from "./lib/dock-drag.svelte";
  import type { DockLayout, RegionId } from "./lib/dock";
  import { createStageFlow, setStageFlow } from "./lib/stage-flow.svelte";
  import type { BoxId } from "./lib/stage";
  import { installKeys } from "./lib/keys";
  import { initTheme } from "./lib/theme";
  import { initTrace } from "./lib/trace";
  import { initDecorations } from "./lib/window";
  import { initZoom, resyncZoom } from "./lib/zoom";
  import {
    actions,
    ALL_TABS,
    drillSpan,
    initEvents,
    loopsOpen,
    openSong,
    sectionsOpen,
    settingsOpen,
    workspace,
  } from "./lib/stores";

  // one view per tab key — keyed by the canonical ALL_TABS set in stores.ts
  const TAB_VIEWS: Record<(typeof ALL_TABS)[number], Component> = {
    library: Library,
    structure: Sections,
    loops: Loops,
    routines: Routines,
    export: Export,
    profile: ProfilingPanel,
    devices: Devices,
    settings: SettingsPanel,
    guide: Guide,
  };

  // The window arrangement is two regions of one workspace; the shared drag
  // coordinator (provided via context) lets a tab drag cross between them. Each
  // region's within-region changes (select / resize) write back through
  // `setLayout`; cross-region moves write the whole workspace.
  const drag = createDockDrag(
    () => $workspace,
    (ws) => void actions.setWorkspace(ws),
  );
  setDockDrag(drag);

  const setLayout = (region: RegionId) => (layout: DockLayout) =>
    void actions.setWorkspace({ ...$workspace, [region]: { ...$workspace[region], layout } });

  // The stage is a flow region: a registry maps each box id → its tool component
  // + a presence predicate (the stage analogue of TAB_VIEWS). The flow controller
  // (context) owns per-box collapse + header-drag reorder; App renders the
  // present boxes in saved order. Transport + waveform are the fixed stage head,
  // outside the flow.
  const STAGE_REGISTRY: Record<BoxId, { component: Component; present: () => boolean }> = {
    metronome: { component: MetronomeBox, present: () => true },
    isolation: { component: Isolation, present: () => !!$openSong },
    click: { component: ClickTrack, present: () => !!$openSong },
    notes: { component: Notes, present: () => !!$openSong },
    recordings: { component: Recordings, present: () => !!$openSong },
    tuner: { component: Tuner, present: () => true },
    drill: { component: Drill, present: () => !!$openSong && !!$drillSpan },
  };
  const stageFlow = createStageFlow(
    () => $workspace.stage,
    (flow) => void actions.setWorkspace({ ...$workspace, stage: flow }),
  );
  setStageFlow(stageFlow);
  const stageBoxes = $derived($workspace.stage.order.filter((id) => STAGE_REGISTRY[id].present()));
  function registerStage(el: HTMLElement) {
    stageFlow.registerContainer(el);
    return {};
  }

  // open-settings / open-structure / open-loops shortcuts reveal their tab
  $effect(() => {
    if ($settingsOpen) {
      void actions.revealTab("settings");
      settingsOpen.set(false);
    }
  });
  $effect(() => {
    if ($sectionsOpen) {
      void actions.revealTab("structure");
      sectionsOpen.set(false);
    }
  });
  $effect(() => {
    if ($loopsOpen) {
      void actions.revealTab("loops");
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

<div class="shell" class:lib-collapsed={$workspace.left.collapsed} class:panels-collapsed={$workspace.right.collapsed}>
  <DockRegion
    side="left"
    layout={$workspace.left.layout}
    collapsed={$workspace.left.collapsed}
    views={TAB_VIEWS}
    onlayout={setLayout("left")}
    ontoggle={() => void actions.toggleRegion("left")}
  />
  <main class="stage">
    <Waveform />
    {#if $openSong}
      <Transport />
    {/if}
    <!-- the stage flow region: present boxes in saved order. Order + per-box
         collapse live in workspace.stage; the flow controller drives reorder
         (drag a box header) and collapse. The boxes wrap to fill the stage. -->
    <div class="boxes" use:registerStage>
      {#each stageBoxes as id (id)}
        {@const Tool = STAGE_REGISTRY[id].component}
        <Tool />
      {/each}
    </div>
  </main>
  <DockRegion
    side="right"
    layout={$workspace.right.layout}
    collapsed={$workspace.right.collapsed}
    views={TAB_VIEWS}
    onlayout={setLayout("right")}
    ontoggle={() => void actions.toggleRegion("right")}
  />
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

  /* The left/right grid columns are filled by <DockRegion> (its own rail +
     collapse + Dock). The shell only owns the column widths + collapse below. */

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
