<script lang="ts">
  import { onMount } from "svelte";
  import { actions, inputDevice, type AudioDevice, type CalibrationResult } from "../lib/stores";
  import { resolveInputDevice } from "../lib/devices";
  import { cmd } from "../lib/ipc";
  import { errMsg } from "../lib/errors";
  import Modal from "../lib/ui/Modal.svelte";
  import Button from "../lib/ui/Button.svelte";

  interface Props {
    open: boolean;
    onclose: () => void;
  }
  let { open, onclose }: Props = $props();

  let devices = $state<AudioDevice[]>([]);
  let selectedInput = $state<string>("default");
  let measuring = $state(false);
  let result = $state<CalibrationResult | null>(null);
  let error = $state<string | null>(null);

  let resolvedInput = $derived(resolveInputDevice(selectedInput, $inputDevice, devices));

  onMount(() => {
    void cmd<AudioDevice[]>("device.inputs")
      .then((d) => { devices = d; })
      .catch(() => {});
  });

  async function measure() {
    if (!resolvedInput) return;
    measuring = true;
    result = null;
    error = null;
    try {
      result = await actions.calibrateLatency(resolvedInput);
    } catch (e) {
      error = errMsg(e);
    } finally {
      measuring = false;
    }
  }

  async function useAuto() {
    try { await actions.resetLatency(); } catch { /* ignore */ }
    result = null;
    error = null;
    onclose();
  }

  // SVG layout constants
  const W = 400;
  const WAVE_TOP = 10;   // waveform area top y
  const WAVE_BOT = 70;   // waveform area bottom y
  const WAVE_H = WAVE_BOT - WAVE_TOP; // 60
  const SVG_H = 80;      // total SVG height

  function buildEnvelopePath(env: number[]): string {
    const len = env.length;
    if (len === 0) return "";
    let d = `M 0,${WAVE_BOT}`;
    for (let i = 0; i < len; i++) {
      const x = (i / len) * W;
      const y = WAVE_BOT - env[i] * WAVE_H;
      d += ` L ${x.toFixed(1)},${y.toFixed(1)}`;
    }
    d += ` L ${W},${WAVE_BOT} Z`;
    return d;
  }

  let envelopePath = $derived(result ? buildEnvelopePath(result.envelope) : "");
  let returnX = $derived(result ? (result.onset_index / result.envelope.length) * W : 0);

  // Keep "returned" label from clipping the right edge
  let returnLabelAnchor = $derived(returnX > W * 0.75 ? "end" : "start");
  let returnLabelX = $derived(returnX > W * 0.75 ? returnX - 3 : returnX + 3);
</script>

<Modal {open} title="recording latency" closable {onclose}>
  <p class="copy">Patch an output to an input. dredge plays a click and measures how long it takes to come back.</p>

  <div class="row">
    <select bind:value={selectedInput} disabled={measuring} aria-label="input device">
      <option value="default">default (follow devices)</option>
      {#each devices as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
    </select>
    <Button
      disabled={measuring || !resolvedInput}
      onclick={() => void measure()}
    >{measuring ? "measuring…" : result || error ? "re-measure" : "measure"}</Button>
  </div>

  {#if result}
    <div class="viz">
      <!-- svelte-ignore a11y_interactive_supports_focus -->
      <svg viewBox="0 0 {W} {SVG_H}" style="width:100%;display:block;" aria-hidden="true">
        <!-- envelope filled area -->
        <path d={envelopePath} fill="var(--wave)" opacity="0.85" />
        <!-- emit line (always at left edge, index 0) -->
        <line x1="1" y1={WAVE_TOP} x2="1" y2={WAVE_BOT + 2} stroke="var(--muted)" stroke-width="1" />
        <text x="4" y={WAVE_TOP - 1} fill="var(--muted)" font-size="8" dominant-baseline="auto">sent</text>
        <!-- return line -->
        <line x1={returnX} y1={WAVE_TOP} x2={returnX} y2={WAVE_BOT + 2} stroke="var(--accent)" stroke-width="1.5" />
        <text
          x={returnLabelX}
          y={WAVE_TOP - 1}
          fill="var(--accent)"
          font-size="8"
          text-anchor={returnLabelAnchor}
          dominant-baseline="auto"
        >returned</text>
        <!-- x-axis labels -->
        <text x="1" y={SVG_H - 1} fill="var(--muted)" font-size="8" dominant-baseline="auto">0</text>
        <text x={W - 1} y={SVG_H - 1} fill="var(--muted)" font-size="8" text-anchor="end" dominant-baseline="auto"
          >{result.window_ms.toFixed(0)} ms</text>
      </svg>
      <div class="measurement">{result.latency_ms.toFixed(1)} ms · {result.latency_frames} samples</div>
    </div>
  {/if}

  {#if error}
    <div class="err">{error}</div>
  {/if}

  <div class="actions">
    <Button onclick={useAuto}>use auto</Button>
    <Button onclick={onclose}>done</Button>
  </div>
</Modal>

<style>
  .copy {
    margin: 0 0 var(--space);
    font-size: 12px;
    color: var(--muted);
  }

  .row {
    display: flex;
    gap: 8px;
    align-items: center;
    margin-bottom: var(--space);
  }

  select {
    flex: 1;
    background: var(--bg);
    color: var(--fg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    height: var(--control-h);
    padding: 0 6px;
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }

  select:disabled {
    color: var(--muted);
    cursor: default;
  }

  .viz {
    margin: var(--space) 0;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--bg);
  }

  .measurement {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--accent);
    text-align: center;
    padding: 4px 0;
    border-top: 1px solid var(--line);
  }

  .err {
    margin: var(--space) 0;
    font-size: 12px;
    color: var(--miss);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 6px 10px;
    background: var(--bg-raised);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: calc(var(--space) * 1.5);
  }
</style>
