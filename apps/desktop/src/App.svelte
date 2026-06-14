<script lang="ts">
  import { onMount } from "svelte";
  import Analysis from "./components/Analysis.svelte";
  import Capture from "./components/Capture.svelte";
  import DuePanel from "./components/DuePanel.svelte";
  import Guide from "./components/Guide.svelte";
  import Library from "./components/Library.svelte";
  import Loops from "./components/Loops.svelte";
  import PlanBuilder from "./components/PlanBuilder.svelte";
  import PlanRunner from "./components/PlanRunner.svelte";
  import ProfilingPanel from "./components/ProfilingPanel.svelte";
  import Sections from "./components/Sections.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import StemMixer from "./components/StemMixer.svelte";
  import Transport from "./components/Transport.svelte";
  import Waveform from "./components/Waveform.svelte";
  import { installKeys } from "./lib/keys";
  import { initZoom } from "./lib/zoom";
  import {
    actions,
    initEvents,
    libraryCollapsed,
    panelsCollapsed,
    pendingRatings,
    planStatus,
    PRACTICE_TOOLS,
    quickPromptVisible,
    quickSavedName,
    sectionsOpen,
    sessionSummary,
    settings,
    settingsOpen,
  } from "./lib/stores";

  const ALL_TABS = ["sections", "loops", "plan", "capture", "due", "profile", "settings", "guide"] as const;
  type Tab = (typeof ALL_TABS)[number];
  // the practice-routine tabs — concealed unless "practice tools" is on in settings
  const PRACTICE_TABS: Tab[] = ["plan", "due"];
  // one-line purpose blurb shown under each tab — answers "what is this for?"
  const TAB_DESC: Record<Tab, string> = {
    sections: "The song's structural map (verse/chorus). Drives the transition loops you practice.",
    loops: "Your saved practice loops, plus auto-derived transitions at section boundaries.",
    plan: "Assemble an evidence-based practice plan from loops and steps.",
    capture: "Record audio from a system source straight into the library.",
    due: "How recent retests of this song's loops landed — the spaced-practice log.",
    profile: "Timing breakdown of the last analysis & stem-separation runs.",
    settings: "App preferences — UI scale, grid snap, capture buffer, analysis device.",
    guide: "Keyboard shortcuts and what the concepts mean.",
  };
  // practice tools are off by default; the routine tabs only appear once enabled
  let practiceOn = $derived($settings[PRACTICE_TOOLS] === true);
  let tabs = $derived(ALL_TABS.filter((t) => practiceOn || !PRACTICE_TABS.includes(t)));
  let tab = $state<Tab>("sections");

  // if practice tools get switched off while viewing one of their tabs, fall back
  $effect(() => {
    if (!practiceOn && PRACTICE_TABS.includes(tab)) tab = "sections";
  });
  let running = $derived(
    $planStatus !== null ||
      $pendingRatings.length > 0 ||
      $sessionSummary !== null ||
      $quickPromptVisible ||
      $quickSavedName !== null,
  );

  $effect(() => {
    if ($settingsOpen) {
      tab = "settings";
      settingsOpen.set(false);
    }
  });

  $effect(() => {
    if ($sectionsOpen) {
      tab = "sections";
      sectionsOpen.set(false);
    }
  });

  onMount(() => {
    // settings drive zoom (ui_scale) and session defaults — load first
    void actions.loadSettings().then(() => initZoom());
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
    <Transport />
    <div class="results">
      <StemMixer />
      <Analysis />
    </div>
  </main>
  <aside class="panels" class:collapsed={$panelsCollapsed}>
    {#if $panelsCollapsed}
      <button class="rail" onclick={() => actions.togglePanels()} title="show panels (Ctrl+])" aria-label="show panels">‹</button>
    {:else}
      <button class="edge right" onclick={() => actions.togglePanels()} title="hide panels (Ctrl+])" aria-label="hide panels">›</button>
      {#if running}
        <PlanRunner />
    {:else}
      <nav class="tabs">
        {#each tabs as t (t)}
          <button class="tab" class:active={tab === t} onclick={() => (tab = t)}>{t}</button>
        {/each}
      </nav>
      <p class="tab-desc">{TAB_DESC[tab]}</p>
      {#key tab}
        <div class="fade-in">
          {#if tab === "sections"}
            <Sections />
          {:else if tab === "loops"}
            <Loops />
          {:else if tab === "plan"}
            <PlanBuilder />
          {:else if tab === "capture"}
            <Capture />
          {:else if tab === "due"}
            <DuePanel />
          {:else if tab === "profile"}
            <ProfilingPanel />
          {:else if tab === "settings"}
            <SettingsPanel />
          {:else}
            <Guide />
          {/if}
        </div>
      {/key}
      {/if}
    {/if}
  </aside>
</div>

<button class="corner tl" onclick={() => actions.toggleLibrary()} title="toggle library (Ctrl+[)" aria-label="toggle library"></button>
<button class="corner tr" onclick={() => actions.togglePanels()} title="toggle panels (Ctrl+])" aria-label="toggle panels"></button>

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

  .library.collapsed,
  .panels.collapsed {
    padding: 0;
    overflow: hidden;
  }

  /* thin expand rail shown when a side column is collapsed */
  .rail {
    width: 100%;
    height: 100%;
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 14px;
  }
  .rail:hover {
    background: var(--bg-raised);
    color: var(--fg);
  }

  /* small collapse handle pinned to a column's outer edge */
  .edge {
    position: absolute;
    top: 4px;
    z-index: 2;
    width: 18px;
    height: 22px;
    padding: 0;
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    color: var(--muted);
    font-size: 11px;
    cursor: pointer;
  }
  .edge:hover {
    color: var(--fg);
    border-color: var(--muted);
  }
  /* handles live on each column's outer edge (far left / far right) */
  .edge.left {
    left: 4px;
  }
  .edge.right {
    right: 4px;
  }

  /* Corner toggle hotspots. They BLEED 6px past the viewport edges so the very
     corner pixel (0,0 etc.) lands in the button's interior — under fractional
     HiDPI scaling (e.g. dpr 1.75) a box flush to top:0/left:0 snaps just inside
     the edge, so a slam to the literal corner falls through to the column
     underneath. The over-bleed guarantees the corner-slam always hits. */
  .corner {
    position: fixed;
    top: -6px;
    width: 34px;
    height: 34px;
    margin: 0;
    padding: 0;
    border: none;
    background: transparent;
    z-index: 100;
    cursor: pointer;
  }
  .corner.tl {
    left: -6px;
  }
  .corner.tr {
    right: -6px;
  }
  .corner:hover {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
  }

  .stage {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding: var(--space);
  }

  /* stems + structure boxes side by side, filling the stage width */
  .results {
    display: flex;
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

  .tab {
    background: none;
    border: none;
    font-size: 11px;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    padding: 2px 6px;
  }

  .tab.active {
    color: var(--accent);
  }

  .tab-desc {
    margin: 0 0 var(--space);
    font-size: 11px;
    line-height: 1.4;
    color: var(--muted);
  }

</style>
