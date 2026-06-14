<script lang="ts">
  import {
    actions,
    tunerInputs,
    tunerInputName,
    tunerOn,
    tunerReading,
    type CaptureNode,
  } from "../lib/stores";
  import { hzToReading } from "../lib/tuner-math";
  import MeterGauge from "./MeterGauge.svelte";

  const GATE = 0.5; // confidence below this = no steady pitch
  const IN_TUNE_CENTS = 5;
  const LOCK_MS = 500;

  let gearOpen = $state(false);
  let error = $state<string | null>(null);
  let lockedSince = $state<number | null>(null);
  let locked = $state(false);

  const r = $derived($tunerReading);
  const voiced = $derived(!!r && r.confidence >= GATE && r.hz > 0);
  const reading = $derived(voiced ? hzToReading(r!.hz) : null);
  const inTune = $derived(!!reading && Math.abs(reading.cents) <= IN_TUNE_CENTS);

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

  async function togglePower() {
    error = null;
    try {
      if ($tunerOn) await actions.tunerPowerOff();
      else await actions.tunerPowerOn();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function openGear() {
    gearOpen = !gearOpen;
    if (!gearOpen) return;
    error = null;
    try {
      await actions.refreshTunerInputs();
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }

  async function pick(node: CaptureNode) {
    gearOpen = false;
    error = null;
    try {
      await actions.setTunerInput(node);
    } catch (e) {
      error = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<section class="box tuner" class:off={!$tunerOn}>
  <div class="head">
    <button
      class="power"
      class:on={$tunerOn}
      onclick={togglePower}
      title="power"
      aria-label="tuner power"
    >⏻</button>
    <span class="lbl">tuner</span>
    <span class="spacer"></span>
    <button class="gear" onclick={openGear} title="input device" aria-label="choose input">⚙</button>
  </div>

  {#if gearOpen}
    <div class="picker">
      {#each $tunerInputs as n (n.id)}
        <button class="dev" class:sel={n.app === $tunerInputName} onclick={() => pick(n)}>
          {n.app || `device ${n.id}`}
        </button>
      {:else}
        <span class="hint">no input devices</span>
      {/each}
    </div>
  {/if}

  <div class="body">
    {#if error}
      <div class="err">{error}</div>
    {:else if !$tunerOn}
      <div class="hint">off — click power to listen</div>
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
</section>

<style>
  .box {
    flex: 0 0 auto;
    min-width: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--bg-raised);
    display: flex;
    flex-direction: column;
  }

  .tuner.off {
    opacity: 0.8;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--line);
  }

  .lbl {
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .spacer {
    flex: 1;
  }

  .power {
    border: 1.5px solid var(--muted);
    color: var(--muted);
    border-radius: 50%;
    width: 20px;
    height: 20px;
    line-height: 1;
    cursor: pointer;
    background: none;
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 12px;
    padding: 0;
  }

  .power.on {
    border-color: var(--cyan);
    color: var(--cyan);
  }

  .gear {
    background: none;
    border: none;
    color: var(--muted);
    cursor: pointer;
    font-size: 0.95rem;
    padding: 0;
    line-height: 1;
  }

  .gear:hover {
    color: var(--fg);
  }

  .picker {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
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
    border-color: var(--cyan);
  }

  .body {
    padding: 10px;
  }

  .hint {
    color: var(--muted);
    font-style: italic;
    font-size: 0.85rem;
  }

  .err {
    color: var(--miss);
    font-size: 0.85rem;
  }
</style>
