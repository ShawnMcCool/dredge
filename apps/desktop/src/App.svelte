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
  import { BOX_LABELS, type BoxId } from "./lib/stage";
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
  // present ∧ ¬hidden render on the stage, in saved order; present ∧ hidden are
  // offered in the `+ tool` restore menu. A hidden contextual box (e.g. drill)
  // only reappears in the menu once its context is back — never auto-shown.
  const stageBoxes = $derived(
    $workspace.stage.order.filter((id) => STAGE_REGISTRY[id].present() && !$workspace.stage.hidden.includes(id)),
  );
  const hiddenBoxes = $derived(
    $workspace.stage.order.filter((id) => STAGE_REGISTRY[id].present() && $workspace.stage.hidden.includes(id)),
  );
  let addOpen = $state(false);
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
    <!-- the stage flow region: present, non-hidden boxes in saved order. Order +
         per-box collapse + hidden live in workspace.stage; the flow controller
         drives reorder (drag a box header), tap-collapse, and hide. The boxes
         wrap to fill the stage; a `+ tool` tail restores hidden boxes. -->
    <div class="boxes" class:dragging={stageFlow.dragId !== null} use:registerStage>
      {#each stageBoxes as id (id)}
        {@const Tool = STAGE_REGISTRY[id].component}
        <Tool />
      {/each}
    </div>
    <!-- restore dock: a quiet + pinned to the stage's bottom-right corner,
         present only while a tool is hidden. Clicking raises a menu of the
         hidden tools; picking one returns it to the flow. -->
    {#if hiddenBoxes.length}
      <div class="add-dock">
        {#if addOpen}
          <!-- click-away catcher behind the menu -->
          <button class="add-backdrop" aria-label="close menu" onclick={() => (addOpen = false)}></button>
          <div class="add-menu">
            {#each hiddenBoxes as id (id)}
              <button
                onclick={() => {
                  stageFlow.show(id);
                  addOpen = false;
                }}>{BOX_LABELS[id]}</button
              >
            {/each}
          </div>
        {/if}
        <button
          class="add-fab"
          class:open={addOpen}
          title="add a hidden tool to the stage"
          aria-label="add a hidden tool to the stage"
          onclick={() => (addOpen = !addOpen)}>+</button
        >
      </div>
    {/if}
    <!-- drag cues (viewport-fixed, mirror the dock's insertion caret): the bar
         shows where the box will land; the ghost chip says which box is moving -->
    {#if stageFlow.caret}
      <div
        class="stage-caret"
        style="left: {stageFlow.caret.x}px; top: {stageFlow.caret.y}px; height: {stageFlow.caret.h}px"
      ></div>
    {/if}
    {#if stageFlow.dragId && stageFlow.pointer}
      <div class="drag-ghost" style="left: {stageFlow.pointer.x}px; top: {stageFlow.pointer.y}px">
        {BOX_LABELS[stageFlow.dragId as BoxId]}
      </div>
    {/if}
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
  /* while reordering, the whole flow reads as "grabbing" */
  .boxes.dragging,
  .boxes.dragging :global(.head) {
    cursor: grabbing;
  }

  /* the restore control: a quiet + in the stage's bottom-right corner. margin-top
     auto eats the free vertical space so it pins to the bottom whenever the boxes
     don't fill the stage; when they do, it flows to the end (scroll to reach) and
     never overlaps a box. position: relative anchors the pop-up menu to it. */
  .add-dock {
    position: relative;
    margin-top: auto;
    align-self: flex-end;
    z-index: 30;
  }
  .add-fab {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: 6px;
    color: var(--muted);
    cursor: pointer;
    font-size: 20px;
    line-height: 1;
    box-shadow: 0 2px 8px -2px rgb(0 0 0 / 0.5);
  }
  .add-fab:hover {
    color: var(--fg);
    border-color: var(--accent-dim);
  }
  .add-fab.open {
    color: var(--accent);
    border-color: var(--accent-dim);
  }
  /* full-viewport click-away catcher behind the open menu */
  .add-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: none;
    border: none;
    cursor: default;
  }
  /* menu rises upward from the corner + (start-menu style), right-aligned */
  .add-menu {
    position: absolute;
    bottom: calc(100% + 6px);
    right: 0;
    z-index: 41;
    display: flex;
    flex-direction: column;
    min-width: 130px;
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: 4px;
    box-shadow: 0 4px 12px -4px rgb(0 0 0 / 0.5);
    overflow: hidden;
  }
  .add-menu button {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    text-align: left;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    padding: 7px 12px;
  }
  .add-menu button:hover {
    color: var(--fg);
    background: var(--accent-dim);
  }

  /* insertion bar — where the dragged box will land; glides like the dock's */
  .stage-caret {
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
  /* ghost chip — the box's name trailing the cursor, says what's in flight */
  .drag-ghost {
    position: fixed;
    z-index: 51;
    transform: translate(12px, 12px);
    pointer-events: none;
    background: var(--bg-raised);
    border: 1px solid var(--accent-dim);
    border-radius: 4px;
    color: var(--fg);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    padding: 4px 8px;
    box-shadow: 0 4px 12px -4px rgb(0 0 0 / 0.5);
  }
</style>
