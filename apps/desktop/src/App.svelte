<script lang="ts">
  import { onMount } from "svelte";
  import Capture from "./components/Capture.svelte";
  import DuePanel from "./components/DuePanel.svelte";
  import ExitModal from "./components/ExitModal.svelte";
  import Library from "./components/Library.svelte";
  import Loops from "./components/Loops.svelte";
  import PlanBuilder from "./components/PlanBuilder.svelte";
  import PlanRunner from "./components/PlanRunner.svelte";
  import PrepareModal from "./components/PrepareModal.svelte";
  import Sections from "./components/Sections.svelte";
  import StemMixer from "./components/StemMixer.svelte";
  import Transport from "./components/Transport.svelte";
  import Waveform from "./components/Waveform.svelte";
  import { installKeys, KEY_HELP } from "./lib/keys";
  import { initZoom } from "./lib/zoom";
  import {
    initEvents,
    pendingRatings,
    planStatus,
    quickPromptVisible,
    quickSavedName,
    sessionSummary,
  } from "./lib/stores";

  const TABS = ["sections", "loops", "plan", "capture", "due"] as const;
  // due panel greets you on app start — the schedule is the product
  let tab = $state<(typeof TABS)[number]>("due");
  let running = $derived(
    $planStatus !== null ||
      $pendingRatings.length > 0 ||
      $sessionSummary !== null ||
      $quickPromptVisible ||
      $quickSavedName !== null,
  );

  onMount(() => {
    void initZoom();
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
          {:else}
            <DuePanel />
          {/if}
        </div>
      {/key}
    {/if}
  </aside>
</div>

<!-- portal-at-root: the overlays cover all three columns -->
<PrepareModal />
<ExitModal />

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
</style>
