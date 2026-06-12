<script lang="ts">
  // Durable settings — every control writes through to the server-side
  // settings table immediately; the UI-scale fader live-applies on drag.
  import {
    actions,
    CAPTURE_BUFFER_SECS,
    GRID_SNAP_DEFAULT,
    gridSnap,
    settings,
    settingsOpen,
    UI_SCALE,
  } from "../lib/stores";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";
  import Modal from "../lib/ui/Modal.svelte";
  import { getZoom, setZoom } from "../lib/zoom";

  const BUFFERS = [60, 120, 180, 300];

  let scale = $derived(Number($settings[UI_SCALE] ?? getZoom()));
  let snapDefault = $derived($settings[GRID_SNAP_DEFAULT] !== false);
  let bufferSecs = $derived(Number($settings[CAPTURE_BUFFER_SECS] ?? 180));

  const close = () => settingsOpen.set(false);

  function toggleSnap() {
    const next = !snapDefault;
    void actions.setSetting(GRID_SNAP_DEFAULT, next);
    gridSnap.set(next); // apply to the running session too
  }
</script>

<Modal open={$settingsOpen} title="settings" closable onclose={close}>
  <div class="row">
    <span class="label">ui scale</span>
    <Fader
      value={scale}
      min={0.75}
      max={2.5}
      step={0.05}
      accent
      onchange={(v) => void setZoom(v)}
      format={(v) => `ui scale ${Math.round(v * 100)}%`}
    />
    <span class="readout mono">{Math.round(scale * 100)}%</span>
  </div>
  <div class="row">
    <span class="label">grid snap by default</span>
    <Button variant="toggle" active={snapDefault} onclick={toggleSnap}>
      {snapDefault ? "on" : "off"}
    </Button>
  </div>
  <div class="row">
    <span class="label">capture buffer</span>
    <div class="chips">
      {#each BUFFERS as b (b)}
        <Button
          variant="chip"
          active={bufferSecs === b}
          onclick={() => void actions.setSetting(CAPTURE_BUFFER_SECS, b)}
        >
          {b}s
        </Button>
      {/each}
    </div>
  </div>
</Modal>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: var(--space);
    margin-bottom: calc(var(--space) * 1.5);
    min-width: 0;
  }

  .row:last-child {
    margin-bottom: 0;
  }

  .label {
    flex: 0 0 auto;
    width: 40%;
    font-size: 12px;
    color: var(--muted);
  }

  .readout {
    flex: 0 0 auto;
    width: 4ch;
    text-align: right;
    font-size: 11px;
  }

  .chips {
    display: flex;
    gap: calc(var(--space) / 2);
    flex-wrap: wrap;
    min-width: 0;
  }
</style>
