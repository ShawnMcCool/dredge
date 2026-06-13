<script lang="ts">
  import { prepareState, workSample, vram, profiles, type PrepareStepState } from "../lib/stores";
  import { effortSummaries } from "../lib/livesummary";

  const STEPS = [
    { key: "analysis", label: "analyzing structure", op: "analysis", model: "SongFormer" },
    { key: "stems", label: "separating stems", op: "stems", model: "Demucs" },
  ] as const;

  const GLYPHS: Record<PrepareStepState, string> = {
    pending: "·", running: "◌", done: "✓", cached: "✓", failed: "✗",
  };

  function fmt(ms: number): string {
    if (ms < 1000) return `${ms} ms`;
    const s = ms / 1000;
    return s < 60 ? `${s.toFixed(1)} s` : `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  }

  let summaries = $derived(effortSummaries($profiles));
</script>

{#if $prepareState}
  <section class="live">
    <h3 class="mono">ANALYZING</h3>
    {#each STEPS as step (step.key)}
      {@const s = $prepareState.steps[step.key]}
      {@const active = $workSample && $workSample.op === step.op && s === "running"}
      <div class="step">
        <span class="glyph mono" class:running={s === "running"} class:done={s === "done" || s === "cached"} class:failed={s === "failed"}>{GLYPHS[s]}</span>
        <span class="name">{step.label}</span>
        <span class="model mono">· {step.model}</span>
        {#if active}
          <span class="stage mono">{$workSample.stage}</span>
          <span class="elapsed mono">{fmt($workSample.elapsed_ms)}</span>
        {:else if s === "cached"}
          <span class="muted mono">cached</span>
        {/if}
        {#if $prepareState.errors[step.key]}
          <span class="error">{$prepareState.errors[step.key]}</span>
        {/if}
      </div>
    {/each}
    {#if $workSample}
      <div class="meters">
        <div class="bars">
          <div class="meter">
            <span class="mlabel mono">cpu</span>
            <span class="bar"><span class="fill" style="width: {Math.min(100, $workSample.cpu_pct / 8)}%"></span></span>
            <span class="mval mono">{$workSample.cpu_pct}%</span>
          </div>
          {#if $workSample.gpu_util != null}
            <div class="meter">
              <span class="mlabel mono">gpu</span>
              <span class="bar"><span class="fill" style="width: {$workSample.gpu_util}%"></span></span>
              <span class="mval mono">{$workSample.gpu_util}%</span>
            </div>
          {/if}
        </div>
        {#if $vram && $vram.used.length}
          <div class="vramcol">
            <span class="hist">
              <svg viewBox="0 0 60 100" preserveAspectRatio="none">
                {#each $vram.used as u, i (i)}
                  <rect x={i} y={100 - (u / $vram.total) * 100} width="1" height={(u / $vram.total) * 100} />
                {/each}
                <line x1="0" x2="60" y1={100 - ($vram.peak / $vram.total) * 100} y2={100 - ($vram.peak / $vram.total) * 100} class="peak" />
              </svg>
            </span>
            <span class="mval mono">{($vram.used[$vram.used.length - 1] / 1024).toFixed(1)} / {Math.round($vram.total / 1024)} GB</span>
          </div>
        {/if}
      </div>
    {/if}
  </section>
{:else if summaries.length}
  <section class="live idle">
    {#each summaries as e (e.op)}
      <div class="effort">
        <div class="ehead mono">{e.op} · {fmt(e.total_ms)}{#if e.device} · {e.device}{/if}{#if e.engine} · {e.engine}{/if}</div>
        {#if e.stages.length}
          <div class="esub mono">{e.stages.map((st) => `${st.name} ${fmt(st.ms)}`).join(" · ")}</div>
        {/if}
        {#if e.maxLine}
          <div class="esub mono">{e.maxLine}</div>
        {/if}
      </div>
    {/each}
  </section>
{/if}

<style>
  .live { padding: var(--space); border-top: 1px solid var(--bg-raised); margin-top: var(--space); }
  .live h3 { font-size: 10px; letter-spacing: 1px; color: var(--muted); margin-bottom: var(--space); }
  .step { display: flex; align-items: baseline; gap: var(--space); margin-bottom: 4px; min-width: 0; }
  .glyph { flex: 0 0 auto; width: 1.2em; text-align: center; color: var(--muted); }
  .glyph.running { color: var(--accent); animation: pulse 1s ease-in-out infinite; }
  .glyph.done { color: var(--solid); }
  .glyph.failed { color: var(--miss); }
  .name { font-size: 13px; }
  .model { font-size: 10px; color: var(--muted); }
  .stage { font-size: 11px; color: var(--accent); }
  .elapsed { margin-left: auto; font-size: 11px; color: var(--muted); }
  .muted { color: var(--muted); font-size: 11px; }
  .error { color: var(--miss); font-size: 11px; }
  .meters { display: flex; gap: var(--space); align-items: flex-start; margin: 6px 0 6px 1.2em; }
  .bars { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
  .meter { display: flex; align-items: center; gap: 6px; }
  .mlabel { font-size: 10px; color: var(--muted); width: 2em; }
  .bar { flex: 1; height: 4px; background: var(--bg-raised); border-radius: 2px; overflow: hidden; max-width: 220px; }
  .fill { display: block; height: 100%; background: var(--accent); }
  .mval { font-size: 10px; color: var(--muted); }
  .vramcol { display: flex; flex-direction: column; gap: 2px; flex: 0 0 auto; align-items: flex-start; }
  .hist { height: 28px; width: 160px; background: var(--bg-raised); border-radius: 2px; overflow: hidden; }
  .hist svg { width: 100%; height: 100%; display: block; }
  .hist rect { fill: var(--accent); }
  .hist line.peak { stroke: var(--shaky); stroke-width: 1; vector-effect: non-scaling-stroke; }
  .effort { margin-bottom: 6px; }
  .ehead { font-size: 11px; }
  .esub { font-size: 10px; color: var(--muted); margin-left: 1em; }
  .idle { color: var(--muted); }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  @media (prefers-reduced-motion: reduce) { .glyph.running { animation: none; } }
</style>
