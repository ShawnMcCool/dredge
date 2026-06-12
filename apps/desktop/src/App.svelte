<script lang="ts">
  import { onMount } from "svelte";
  import Waveform from "./components/Waveform.svelte";
  import { initEvents } from "./lib/stores";

  onMount(() => {
    const unlisten = initEvents();
    return () => {
      void unlisten.then((f) => f());
    };
  });
</script>

<div class="shell">
  <aside class="library"></aside>
  <main class="stage">
    <Waveform />
  </main>
  <aside class="panels"></aside>
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
</style>
