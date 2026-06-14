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
  import { applyTheme, type Accent } from "../lib/theme";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";
  import { applyDecorations } from "../lib/window";
  import { getZoom, setZoom } from "../lib/zoom";

  const ACCENTS: Accent[] = ["amber", "cyan"];

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
  let accent = $derived(($settings[COLOR_THEME] as Accent) === "cyan" ? "cyan" : "amber");

  function pickAccent(a: Accent) {
    applyTheme(a); // live swap
    void actions.setSetting(COLOR_THEME, a);
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
<div class="row">
  <span class="label">ui scale</span>
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
<div class="row">
  <span class="label">color theme</span>
  <div class="chips">
    {#each ACCENTS as a (a)}
      <Button variant="chip" active={accent === a} onclick={() => pickAccent(a)}>
        <span class="swatch {a}"></span>{a}
      </Button>
    {/each}
  </div>
</div>
<div class="row">
  <span class="label">native window frame</span>
  <Button variant="toggle" active={decorations} onclick={toggleDecorations}>
    {decorations ? "on" : "off"}
  </Button>
</div>
<p class="hint mono">the OS title bar + min/max/close · off = borderless (use your WM)</p>
<div class="row">
  <span class="label">grid snap by default</span>
  <Button variant="toggle" active={snapDefault} onclick={toggleSnap}>
    {snapDefault ? "on" : "off"}
  </Button>
</div>
<div class="row">
  <span class="label">practice tools</span>
  <Button
    variant="toggle"
    active={practiceOn}
    onclick={() => void actions.setSetting(PRACTICE_TOOLS, !practiceOn)}
  >
    {practiceOn ? "on" : "off"}
  </Button>
</div>
<p class="hint mono">adds the plan + spaced-practice tabs · off keeps the panel to song-shaping</p>
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
<div class="row">
  <span class="label">analysis device</span>
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
<p class="hint mono">auto = GPU when it fits, else CPU · cpu = slower, never out of VRAM</p>

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

  .hint {
    font-size: 10px;
    color: var(--muted);
    margin-top: calc(var(--space) * -0.5);
  }

  .swatch {
    display: inline-block;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    margin-right: 5px;
    vertical-align: middle;
  }
  .swatch.amber {
    background: #e0a458;
  }
  .swatch.cyan {
    background: #4fc3d4;
  }
</style>
