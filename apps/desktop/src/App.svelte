<script lang="ts">
  import { onMount } from "svelte";
  import Transport from "./components/Transport.svelte";
  import Waveform from "./components/Waveform.svelte";
  import { installKeys, KEY_HELP } from "./lib/keys";
  import { initEvents } from "./lib/stores";

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
  <aside class="library"></aside>
  <main class="stage">
    <Waveform />
    <Transport />
    <footer class="help mono">{KEY_HELP}</footer>
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

  .help {
    margin-top: auto;
    padding-top: var(--space);
    font-size: 11px;
    color: var(--muted);
  }
</style>
