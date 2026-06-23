<script lang="ts">
  import { onMount } from "svelte";
  import { actions, outputDevice, outputDevices } from "../lib/stores";
  import { asyncAction } from "../lib/async-action.svelte";
  import { defaultName } from "../lib/devices";
  import Button from "../lib/ui/Button.svelte";

  const act = asyncAction();

  function pick(id: string | null) {
    return act.run(() => actions.setOutputDevice(id));
  }

  onMount(() => {
    void act.run(() => actions.refreshOutputs());
  });
</script>

<h2>devices</h2>

{#if act.error}
  <div class="error">{act.error}</div>
{/if}

<section class="group">
  <h3 class="group-head">output</h3>
  <div class="picker">
    <button class="dev" class:sel={$outputDevice === null} onclick={() => pick(null)}>
      System default{defaultName($outputDevices) ? ` (${defaultName($outputDevices)})` : ""}
    </button>
    {#each $outputDevices as d (d.id)}
      <button class="dev" class:sel={$outputDevice === d.id} onclick={() => pick(d.id)}>{d.name}</button>
    {/each}
  </div>
</section>

<Button onclick={() => pick(null)}>reset to system</Button>

<style>
  .group {
    margin-bottom: calc(var(--space) * 2.5);
  }
  .group:last-child {
    margin-bottom: 0;
  }

  .group-head {
    margin: 0 0 calc(var(--space) / 2);
    padding-bottom: 6px;
    border-bottom: 1px solid var(--line);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--muted);
  }

  .picker {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 0 6px;
    margin-bottom: 6px;
    border-bottom: 1px solid var(--line);
  }

  .dev {
    text-align: left;
    background: none;
    border: 1px solid transparent;
    color: var(--fg);
    border-radius: 4px;
    padding: 4px 8px;
    cursor: pointer;
    font-size: 0.85rem;
  }

  .dev:hover {
    background: var(--bg-raised);
  }

  .dev.sel {
    border-color: var(--accent);
  }

  .error {
    color: var(--fg);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 6px 10px;
    font-size: 12px;
    margin-bottom: var(--space);
  }
</style>
