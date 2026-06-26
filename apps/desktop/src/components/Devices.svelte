<script lang="ts">
  import { onMount } from "svelte";
  import { actions, inputDevice, inputDevices, latencyStatus, outputDevice, outputDevices } from "../lib/stores";
  import { asyncAction } from "../lib/async-action.svelte";
  import { defaultName } from "../lib/devices";
  import Button from "../lib/ui/Button.svelte";
  import CalibrateModal from "./CalibrateModal.svelte";

  const act = asyncAction();
  let calOpen = $state(false);

  function pickOutput(id: string | null) {
    return act.run(() => actions.setOutputDevice(id));
  }

  function pickInput(id: string | null) {
    return act.run(() => actions.setInputDevice(id));
  }

  function resetToSystem() {
    return act.run(async () => {
      await actions.setOutputDevice(null);
      await actions.setInputDevice(null);
    });
  }

  // frames → ms at the engine's fixed 48 kHz, or "—" when not yet measured.
  function latencyMs(frames: number | null): string {
    return frames == null ? "—" : `${(frames / 48000 * 1000).toFixed(1)} ms`;
  }

  onMount(() => {
    void act.run(() => actions.refreshOutputs());
    void act.run(() => actions.refreshInputs());
    void actions.refreshLatency();
  });
</script>

<h2>devices</h2>

{#if act.error}
  <div class="error">{act.error}</div>
{/if}

<section class="group">
  <h3 class="group-head">output</h3>
  <div class="picker">
    <button class="dev" class:sel={$outputDevice === null} onclick={() => pickOutput(null)}>
      System default{defaultName($outputDevices) ? ` (${defaultName($outputDevices)})` : ""}
    </button>
    {#each $outputDevices as d (d.id)}
      <button class="dev" class:sel={$outputDevice === d.id} onclick={() => pickOutput(d.id)}>{d.name}</button>
    {/each}
  </div>
</section>

<section class="group">
  <h3 class="group-head">input</h3>
  <div class="picker">
    <button class="dev" class:sel={$inputDevice === null} onclick={() => pickInput(null)}>
      System default{defaultName($inputDevices) ? ` (${defaultName($inputDevices)})` : ""}
    </button>
    {#each $inputDevices as d (d.id)}
      <button class="dev" class:sel={$inputDevice === d.id} onclick={() => pickInput(d.id)}>{d.name}</button>
    {/each}
  </div>
  <Button onclick={() => (calOpen = true)}>calibrate latency…</Button>
  {#if $latencyStatus}
    <dl class="latency">
      <dt>recording latency</dt>
      <dd>
        <span class="lat-row" class:active={$latencyStatus.source !== "loopback"}>
          auto-detected: {latencyMs($latencyStatus.auto_frames)}
          {#if $latencyStatus.source !== "loopback"}<span class="tag">in use</span>{/if}
        </span>
        <span class="lat-row" class:active={$latencyStatus.source === "loopback"}>
          calibrated: {latencyMs($latencyStatus.loopback_frames)}
          {#if $latencyStatus.source === "loopback"}<span class="tag">in use</span>{/if}
        </span>
      </dd>
    </dl>
  {/if}
</section>

<Button onclick={() => resetToSystem()}>reset to system</Button>

<CalibrateModal open={calOpen} onclose={() => (calOpen = false)} />

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

  .latency {
    margin: calc(var(--space) / 2) 0 0;
    font-size: 11px;
    color: var(--muted);
  }

  .latency dt {
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 10px;
    margin-bottom: 2px;
  }

  .latency dd {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .lat-row {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .lat-row.active {
    color: var(--accent);
  }

  .tag {
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--accent);
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
