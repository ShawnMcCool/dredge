<script lang="ts">
  import { onMount } from "svelte";
  import Capture from "./components/Capture.svelte";
  import DuePanel from "./components/DuePanel.svelte";
  import Library from "./components/Library.svelte";
  import Loops from "./components/Loops.svelte";
  import PlanBuilder from "./components/PlanBuilder.svelte";
  import PlanRunner from "./components/PlanRunner.svelte";
  import Sections from "./components/Sections.svelte";
  import StemMixer from "./components/StemMixer.svelte";
  import Transport from "./components/Transport.svelte";
  import Waveform from "./components/Waveform.svelte";
  import { installKeys, KEY_HELP } from "./lib/keys";
  import { initEvents, pendingRatings, planStatus, sessionSummary } from "./lib/stores";

  const TABS = ["sections", "loops", "plan", "capture", "due"] as const;
  // due panel greets you on app start — the schedule is the product
  let tab = $state<(typeof TABS)[number]>("due");
  let running = $derived(
    $planStatus !== null || $pendingRatings.length > 0 || $sessionSummary !== null,
  );

  onMount(() => {
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

<style>
  .shell {
    display: grid;
    grid-template-columns: 240px 1fr 320px;
    height: 100vh;
  }

  .library {
    border-right: 1px solid var(--line);
    padding: var(--space);
    overflow-y: auto;
  }

  .stage {
    display: flex;
    flex-direction: column;
    min-width: 0;
    padding: var(--space);
  }

  .panels {
    border-left: 1px solid var(--line);
    padding: var(--space);
    overflow-y: auto;
  }

  .help {
    margin-top: auto;
    padding-top: var(--space);
    font-size: 11px;
    color: var(--muted);
  }

  .tabs {
    display: flex;
    gap: calc(var(--space) / 2);
    margin-bottom: var(--space);
    border-bottom: 1px solid var(--line);
    padding-bottom: var(--space);
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
