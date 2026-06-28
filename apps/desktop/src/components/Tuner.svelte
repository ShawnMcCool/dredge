<script lang="ts">
  import {
    actions,
    inputDevices,
    tunerInput,
    tunerOn,
    tunerReading,
  } from "../lib/stores";
  import { asyncAction } from "../lib/async-action.svelte";
  import { hzToReading } from "../lib/tuner-math";
  import Box from "../lib/ui/Box.svelte";
  import { getStageFlow } from "../lib/stage-flow.svelte";
  import MeterGauge from "./MeterGauge.svelte";

  const GATE = 0.5; // confidence below this = no steady pitch
  const IN_TUNE_CENTS = 5;
  const LOCK_MS = 500;

  let gearOpen = $state(false);
  const act = asyncAction();
  let lockedSince = $state<number | null>(null);
  let locked = $state(false);

  const r = $derived($tunerReading);
  const voiced = $derived(!!r && r.confidence >= GATE && r.hz > 0);
  const reading = $derived(voiced ? hzToReading(r!.hz) : null);
  const inTune = $derived(!!reading && Math.abs(reading.cents) <= IN_TUNE_CENTS);

  // The tuner listens whenever it's expanded; collapsing it stops listening. The
  // collapse caret thus IS the power switch — no separate button, no idle empty
  // state. Mirrors the box's collapse state (from the stage flow) onto the mic.
  const flow = getStageFlow();
  const collapsed = $derived(flow.isCollapsed("tuner"));
  $effect(() => {
    if (collapsed) {
      if ($tunerOn) void act.run(() => actions.tunerPowerOff());
    } else if (!$tunerOn) {
      void act.run(() => actions.tunerPowerOn());
    }
  });

  // hold-to-lock: in tune continuously for LOCK_MS
  $effect(() => {
    if (!$tunerOn || !inTune) {
      lockedSince = null;
      locked = false;
      return;
    }
    if (lockedSince === null) lockedSince = performance.now();
    const elapsed = performance.now() - lockedSince;
    if (elapsed >= LOCK_MS) {
      locked = true;
      return;
    }
    const t = setTimeout(() => {
      if (lockedSince !== null && performance.now() - lockedSince >= LOCK_MS) locked = true;
    }, LOCK_MS - elapsed);
    return () => clearTimeout(t);
  });

  function openGear() {
    gearOpen = !gearOpen;
    if (!gearOpen) return;
    return act.run(() => actions.refreshInputs());
  }

  function pick(sel: string) {
    gearOpen = false;
    return act.run(() => actions.setTunerInput(sel));
  }
</script>

<Box id="tuner" dim={!$tunerOn}>
  {#snippet tools()}
    {#if !collapsed}
      <button onclick={openGear} title="input device" aria-label="choose input">⚙</button>
    {/if}
  {/snippet}

  {#if gearOpen}
    <div class="picker">
      <button class="dev" class:sel={$tunerInput === "default"} onclick={() => pick("default")}>
        default (follow devices)
      </button>
      {#each $inputDevices as d (d.id)}
        <button class="dev" class:sel={$tunerInput === d.id} onclick={() => pick(d.id)}>
          {d.name || `device ${d.id}`}
        </button>
      {/each}
    </div>
  {/if}

  <div class="tuner-body">
    {#if act.error}
      <div class="error">{act.error}</div>
    {:else}
      <MeterGauge
        listening={!voiced}
        note={reading?.note ?? ""}
        octave={reading?.octave ?? 0}
        cents={reading?.cents ?? 0}
        {inTune}
        {locked}
      />
    {/if}
  </div>
</Box>

<style>
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

  /* power + readout centred in the box, both axes */
  .tuner-body {
    flex: 1 1 auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 6px 0;
  }

  .error {
    color: var(--miss);
    font-size: 0.85rem;
  }
</style>
