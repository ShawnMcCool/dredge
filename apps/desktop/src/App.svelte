<script lang="ts">
  import { onMount } from "svelte";
  import Capture from "./components/Capture.svelte";
  import DuePanel from "./components/DuePanel.svelte";
  import Library from "./components/Library.svelte";
  import Loops from "./components/Loops.svelte";
  import PlanBuilder from "./components/PlanBuilder.svelte";
  import PlanRunner from "./components/PlanRunner.svelte";
  import LiveProgress from "./components/LiveProgress.svelte";
  import ProfilingPanel from "./components/ProfilingPanel.svelte";
  import Sections from "./components/Sections.svelte";
  import SettingsPanel from "./components/SettingsPanel.svelte";
  import StemMixer from "./components/StemMixer.svelte";
  import Transport from "./components/Transport.svelte";
  import Waveform from "./components/Waveform.svelte";
  import { installKeys, KEY_HELP } from "./lib/keys";
  import { initZoom } from "./lib/zoom";
  import {
    actions,
    initEvents,
    pendingRatings,
    planStatus,
    quickPromptVisible,
    quickSavedName,
    sessionSummary,
    settingsOpen,
  } from "./lib/stores";

  const TABS = ["sections", "loops", "plan", "capture", "due", "profile", "settings"] as const;
  // one-line purpose blurb shown under each tab — answers "what is this for?"
  const TAB_DESC: Record<(typeof TABS)[number], string> = {
    sections: "The song's structural map (verse/chorus). Drives the junction loops you practice.",
    loops: "Your saved practice loops, plus auto-derived junctions at section boundaries.",
    plan: "Assemble an evidence-based practice plan from loops and steps.",
    capture: "Record audio from a system source straight into the library.",
    due: "What's scheduled for practice right now — the spaced-repetition queue.",
    profile: "Timing breakdown of the last analysis & stem-separation runs.",
    settings: "App preferences — UI scale, grid snap, capture buffer, analysis device.",
  };
  // due panel greets you on app start — the schedule is the product
  let tab = $state<(typeof TABS)[number]>("due");
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

<div class="shell">
  <aside class="library">
    <Library />
  </aside>
  <main class="stage">
    <Waveform />
    <Transport />
    <StemMixer />
    <LiveProgress />
    <footer class="help mono">{KEY_HELP}</footer>
  </main>
  <aside class="panels">
    {#if running}
      <PlanRunner />
    {:else}
      <nav class="tabs">
        {#each TABS as t (t)}
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
          {:else}
            <SettingsPanel />
          {/if}
        </div>
      {/key}
    {/if}
  </aside>
</div>

<style>
  .shell {
    display: grid;
    grid-template-columns: minmax(170px, 240px) minmax(320px, 1fr) minmax(250px, 340px);
    height: 100vh;
  }

  /* below the point where the preferred minimums fit, shrink all three
     columns further instead of pushing the right rail off-screen */
  @media (max-width: 745px) {
    .shell {
      grid-template-columns: minmax(110px, 240px) minmax(220px, 1fr) minmax(130px, 340px);
    }
  }

  .library {
    border-right: 1px solid var(--line);
    padding: var(--space);
    min-width: 0;
    overflow-y: auto;
  }

  .stage {
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow-x: hidden;
    overflow-y: auto;
    padding: var(--space);
  }

  .panels {
    border-left: 1px solid var(--line);
    padding: var(--space);
    min-width: 0;
    overflow-x: hidden;
    overflow-y: auto;
  }

  .help {
    margin-top: auto;
    padding-top: var(--space);
    font-size: 11px;
    color: var(--muted);
    overflow-wrap: anywhere;
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
