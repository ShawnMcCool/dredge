<script lang="ts">
  import { prepareState, workSample, vram, type PrepareStepState } from "../lib/stores";

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

  // Running min/max per metric over the current run. workSample is instantaneous,
  // so we accumulate here; the component unmounts when prepareState clears between
  // runs, so these reset on remount. Plain accumulators (not $state) keep the
  // effect's only dependency $workSample — no self-referential loop.
  type Range = { min: number; max: number };
  let cpu = $state<Range | null>(null);
  let gpu = $state<Range | null>(null);
  let ram = $state<Range | null>(null);
  let accCpu: Range | null = null;
  let accGpu: Range | null = null;
  let accRam: Range | null = null;
  const grow = (r: Range | null, v: number): Range =>
    r ? { min: Math.min(r.min, v), max: Math.max(r.max, v) } : { min: v, max: v };

  $effect(() => {
    const s = $workSample;
    if (!s) return;
    cpu = accCpu = grow(accCpu, s.cpu_pct);
    if (s.gpu_util != null) gpu = accGpu = grow(accGpu, s.gpu_util);
    if (s.ram_used_mb != null) ram = accRam = grow(accRam, s.ram_used_mb);
  });

  const gb = (mb: number) => (mb / 1024).toFixed(1);
  const gbi = (mb: number) => Math.round(mb / 1024);
  const pct = (n: number) => `${Math.round(n)}%`;
  const clamp = (n: number) => Math.max(0, Math.min(100, n));
</script>

<!-- one bar: fill to current, faint ticks at the run's min (muted) and max (peak) -->
{#snippet bar(cur: number, scale: number, lo: number | null, hi: number | null)}
  <span class="bar">
    <span class="fill" style="width: {clamp((cur / scale) * 100)}%"></span>
    {#if lo != null}<span class="tick lo" style="left: {clamp((lo / scale) * 100)}%"></span>{/if}
    {#if hi != null}<span class="tick hi" style="left: {clamp((hi / scale) * 100)}%"></span>{/if}
  </span>
{/snippet}

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
        <div class="meter">
          <span class="mlabel mono">cpu</span>
          {@render bar($workSample.cpu_pct, 800, cpu?.min ?? null, cpu?.max ?? null)}
          <span class="mval mono">{pct($workSample.cpu_pct)}</span>
          <span class="mrange mono">{cpu ? `${Math.round(cpu.min)}–${Math.round(cpu.max)}` : ""}</span>
        </div>

        {#if $workSample.gpu_util != null}
          <div class="meter">
            <span class="mlabel mono">gpu</span>
            {@render bar($workSample.gpu_util, 100, gpu?.min ?? null, gpu?.max ?? null)}
            <span class="mval mono">{pct($workSample.gpu_util)}</span>
            <span class="mrange mono">{gpu ? `${Math.round(gpu.min)}–${Math.round(gpu.max)}` : ""}</span>
          </div>
        {/if}

        {#if $workSample.ram_total_mb != null}
          <div class="meter">
            <span class="mlabel mono">ram</span>
            {@render bar($workSample.ram_used_mb ?? 0, $workSample.ram_total_mb, ram?.min ?? null, ram?.max ?? null)}
            <span class="mval mono">{gb($workSample.ram_used_mb ?? 0)} / {gbi($workSample.ram_total_mb)} GB</span>
            <span class="mrange mono">{ram ? `${gb(ram.min)}–${gb(ram.max)}` : ""}</span>
          </div>
        {/if}

        {#if $vram && $vram.used.length}
          <!-- vram ramps over a run, so its track is a full-width history sparkline
               with peak (orange) and min (muted) guide lines -->
          <div class="meter">
            <span class="mlabel mono">vram</span>
            <span class="hist" title="VRAM over the run">
              <svg viewBox="0 0 {$vram.used.length} 100" preserveAspectRatio="none">
                {#each $vram.used as u, i (i)}
                  <rect x={i} y={100 - (u / $vram.total) * 100} width="1.05" height={(u / $vram.total) * 100} />
                {/each}
                <line class="hi" x1="0" x2={$vram.used.length} y1={100 - ($vram.peak / $vram.total) * 100} y2={100 - ($vram.peak / $vram.total) * 100} />
                <line class="lo" x1="0" x2={$vram.used.length} y1={100 - ($vram.min / $vram.total) * 100} y2={100 - ($vram.min / $vram.total) * 100} />
              </svg>
            </span>
            <span class="mval mono">{gb($vram.used[$vram.used.length - 1])} / {gbi($vram.total)} GB</span>
            <span class="mrange mono">{gb($vram.min)}–{gb($vram.peak)}</span>
          </div>
        {/if}

        <div class="legend mono">
          <span><span class="swatch fill"></span> now</span>
          <span><span class="swatch lo"></span> min</span>
          <span><span class="swatch hi"></span> peak</span>
        </div>
      </div>
    {/if}
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

  /* meters: a full-width column, bars stretch between label and the readouts */
  .meters { display: flex; flex-direction: column; gap: 5px; margin: 8px 0 4px 1.2em; min-width: 0; }
  .meter { display: flex; align-items: center; gap: 8px; }
  .mlabel { font-size: 10px; color: var(--muted); width: 2.6em; flex: 0 0 auto; }
  .bar { position: relative; flex: 1 1 auto; height: 8px; min-width: 0; background: var(--bg-raised); border-radius: 2px; overflow: hidden; }
  .fill { position: absolute; left: 0; top: 0; height: 100%; background: var(--accent); }
  .tick { position: absolute; top: 0; height: 100%; width: 1px; }
  .tick.hi { background: var(--shaky); }
  .tick.lo { background: var(--muted); }
  .hist { position: relative; flex: 1 1 auto; height: 22px; min-width: 0; background: var(--bg-raised); border-radius: 2px; overflow: hidden; }
  .hist svg { width: 100%; height: 100%; display: block; }
  .hist rect { fill: var(--accent); }
  .hist line.hi { stroke: var(--shaky); stroke-width: 1; vector-effect: non-scaling-stroke; }
  .hist line.lo { stroke: var(--muted); stroke-width: 1; vector-effect: non-scaling-stroke; opacity: 0.7; }
  .mval { font-size: 10px; color: var(--fg); width: 8.5em; text-align: right; flex: 0 0 auto; }
  .mrange { font-size: 10px; color: var(--muted); width: 6em; text-align: right; flex: 0 0 auto; }

  .legend { display: flex; gap: 12px; margin-top: 4px; font-size: 9px; color: var(--muted); }
  .legend span { display: inline-flex; align-items: center; gap: 4px; }
  .swatch { width: 8px; height: 8px; border-radius: 1px; display: inline-block; }
  .swatch.fill { background: var(--accent); }
  .swatch.hi { background: var(--shaky); }
  .swatch.lo { background: var(--muted); }

  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  @media (prefers-reduced-motion: reduce) { .glyph.running { animation: none; } }
</style>
