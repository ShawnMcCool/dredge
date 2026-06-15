<script lang="ts">
  // Durable settings — every control writes through to the server-side
  // settings table immediately; the UI-scale fader previews live but only
  // applies the zoom on release (a re-zoom + DB write per pixel is too heavy).
  import {
    actions,
    ANALYSIS_DEVICE,
    CAPTURE_BUFFER_SECS,
    COLOR_THEME,
    GRID_SNAP_DEFAULT,
    gridSnap,
    PRACTICE_TOOLS,
    settings,
    UI_SCALE,
    WINDOW_DECORATIONS,
  } from "../lib/stores";
  import { ACCENTS, applyTheme } from "../lib/theme";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";
  import { applyDecorations } from "../lib/window";
  import { getZoom, setZoom } from "../lib/zoom";

  const BUFFERS = [60, 120, 180, 300];

  let scale = $derived(Number($settings[UI_SCALE] ?? getZoom()));
  // live preview while dragging; zoom is only applied on release (a full
  // webview re-zoom + DB write is too heavy to run on every pixel)
  let preview = $state<number | null>(null);
  let shownScale = $derived(preview ?? scale);
  let snapDefault = $derived($settings[GRID_SNAP_DEFAULT] !== false);
  let bufferSecs = $derived(Number($settings[CAPTURE_BUFFER_SECS] ?? 180));
  let device = $derived(($settings[ANALYSIS_DEVICE] as string) ?? "auto");
  let practiceOn = $derived($settings[PRACTICE_TOOLS] === true);
  // default on: only an explicit false hides the frame
  let decorations = $derived($settings[WINDOW_DECORATIONS] !== false);
  let themeValue = $derived(
    typeof $settings[COLOR_THEME] === "string" ? ($settings[COLOR_THEME] as string) : "amber",
  );
  let activeName = $derived(ACCENTS.find((o) => o.value === themeValue)?.name ?? themeValue);

  function setAccent(value: string) {
    applyTheme(value); // live swap
    void actions.setSetting(COLOR_THEME, value);
  }

  function toggleSnap() {
    const next = !snapDefault;
    void actions.setSetting(GRID_SNAP_DEFAULT, next);
    gridSnap.set(next); // apply to the running session too
  }

  function toggleDecorations() {
    const next = !decorations;
    void applyDecorations(next); // apply to the live window immediately
    void actions.setSetting(WINDOW_DECORATIONS, next);
  }
</script>

<h2>settings</h2>

<section class="group">
  <h3 class="group-head">appearance</h3>

  <div class="setting stacked">
    <div class="text"><span class="name">ui scale</span></div>
    <div class="fader-row">
      <Fader
        value={shownScale}
        min={0.75}
        max={2.5}
        step={0.05}
        accent
        onchange={(v) => (preview = v)}
        oncommit={(v) => {
          preview = null;
          void setZoom(v);
        }}
        format={(v) => `ui scale ${Math.round(v * 100)}%`}
      />
      <span class="readout mono">{Math.round(shownScale * 100)}%</span>
    </div>
  </div>

  <div class="setting stacked">
    <div class="text"><span class="name">color theme</span></div>
    <div class="swatches">
      {#each ACCENTS as opt (opt.value)}
        <button
          class="swatch-btn"
          class:active={themeValue === opt.value}
          style="--dot: {opt.hex}"
          onclick={() => setAccent(opt.value)}
          title={opt.name}
          aria-label={opt.name}
        >
          <span class="dot"></span>
        </button>
      {/each}
      <span class="swatch-name">{activeName}</span>
    </div>
  </div>

  <div class="setting">
    <div class="text">
      <span class="name">native window frame</span>
      <span class="desc">the OS title bar + min/max/close · off is borderless (use your WM)</span>
    </div>
    <Button variant="toggle" active={decorations} onclick={toggleDecorations}>
      {decorations ? "on" : "off"}
    </Button>
  </div>
</section>

<section class="group">
  <h3 class="group-head">editing</h3>

  <div class="setting">
    <div class="text">
      <span class="name">grid snap by default</span>
      <span class="desc">loop + selection edges snap to analyzed downbeats</span>
    </div>
    <Button variant="toggle" active={snapDefault} onclick={toggleSnap}>
      {snapDefault ? "on" : "off"}
    </Button>
  </div>

  <div class="setting">
    <div class="text">
      <span class="name">practice tools</span>
      <span class="desc">adds the plan + spaced-practice tabs · off keeps the panel to song-shaping</span>
    </div>
    <Button
      variant="toggle"
      active={practiceOn}
      onclick={() => void actions.setSetting(PRACTICE_TOOLS, !practiceOn)}
    >
      {practiceOn ? "on" : "off"}
    </Button>
  </div>
</section>

<section class="group">
  <h3 class="group-head">capture &amp; analysis</h3>

  <div class="setting stacked">
    <div class="text"><span class="name">capture buffer</span></div>
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

  <div class="setting stacked">
    <div class="text">
      <span class="name">analysis device</span>
      <span class="desc">auto = GPU when it fits, else CPU · cpu = slower, never out of VRAM</span>
    </div>
    <div class="chips">
      <Button
        variant="chip"
        active={device === "auto"}
        onclick={() => void actions.setSetting(ANALYSIS_DEVICE, "auto")}
      >
        auto
      </Button>
      <Button
        variant="chip"
        active={device === "cpu"}
        onclick={() => void actions.setSetting(ANALYSIS_DEVICE, "cpu")}
      >
        cpu
      </Button>
    </div>
  </div>
</section>

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

  /* one setting: label/desc text block + its control, inline by default */
  .setting {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space);
    padding: 7px 0;
    min-width: 0;
  }

  /* continuous / multi-option controls drop below their label, full width */
  .setting.stacked {
    flex-direction: column;
    align-items: stretch;
    gap: 7px;
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .name {
    font-size: 13px;
    color: var(--fg);
  }
  .desc {
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
  }

  .fader-row {
    display: flex;
    align-items: center;
    gap: var(--space);
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

  /* curated accent palette — colour dots, active gets a neutral ring */
  .swatches {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .swatch-btn {
    width: 22px;
    height: 22px;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 50%;
    background: none;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .swatch-btn:hover {
    border-color: var(--line);
  }
  .swatch-btn.active {
    border-color: var(--fg);
  }
  .dot {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--dot);
  }
  .swatch-name {
    margin-left: 2px;
    font-size: 11px;
    color: var(--muted);
  }
</style>
