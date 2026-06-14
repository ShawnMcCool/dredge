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
  // runs, so these reset on remount. Plain accumulators keep the effect's only
  // dependency $workSample (no self-referential loop).
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
  const clamp = (n: number) => Math.max(0, Math.min(100, n));
  // bars are neutral grey until a resource fills up: amber when busy, red when
  // near its limit. The colour, not the bar itself, carries the meaning.
  const lvl = (frac: number) => (frac >= 0.9 ? "hot" : frac >= 0.72 ? "warm" : "ok");
</script>

<!-- one bar meter: neutral fill to current (tinted by capacity), a peak marker,
     now value over a muted min–max range. `scale` maps units onto 0..100%. -->
{#snippet meter(label: string, cur: number, scale: number, r: Range | null, now: string, range: string)}
  <div class="meter">
    <span class="mlabel mono">{label}</span>
    <span class="bar">
      <span class="fill {lvl(cur / scale)}" style="width: {clamp((cur / scale) * 100)}%"></span>
      {#if r}<span class="tick hi" style="left: {clamp((r.max / scale) * 100)}%"></span>{/if}
    </span>
    <span class="vals mono"><span class="now {lvl(cur / scale)}">{now}</span><span class="range">{range}</span></span>
  </div>
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
        <!-- compute, left -->
        <div class="col">
          {@render meter("cpu", $workSample.cpu_pct, 800, cpu, `${Math.round($workSample.cpu_pct)}%`, cpu ? `${Math.round(cpu.min)}–${Math.round(cpu.max)}%` : "")}
          {#if $workSample.gpu_util != null}
            {@render meter("gpu", $workSample.gpu_util, 100, gpu, `${Math.round($workSample.gpu_util)}%`, gpu ? `${Math.round(gpu.min)}–${Math.round(gpu.max)}%` : "")}
          {/if}
        </div>

        <!-- memory, right -->
        <div class="col">
          {#if $workSample.ram_total_mb != null}
            {@render meter("ram", $workSample.ram_used_mb ?? 0, $workSample.ram_total_mb, ram, `${gb($workSample.ram_used_mb ?? 0)} / ${gbi($workSample.ram_total_mb)} GB`, ram ? `${gb(ram.min)}–${gb(ram.max)}` : "")}
          {/if}
          {#if $vram && $vram.used.length}
            {@const peakFrac = $vram.peak / $vram.total}
            <!-- vram ramps over a run → a tall neutral history sparkline; the peak
                 guide line tints by capacity (amber/red), the min line is muted -->
            <div class="meter tall">
              <span class="mlabel mono">vram</span>
              <span class="hist" title="VRAM over the run">
                <svg viewBox="0 0 {$vram.used.length} 100" preserveAspectRatio="none">
                  {#each $vram.used as u, i (i)}
                    <rect x={i} y={100 - (u / $vram.total) * 100} width="1.05" height={(u / $vram.total) * 100} />
                  {/each}
                  <line class="hi {lvl(peakFrac)}" x1="0" x2={$vram.used.length} y1={100 - peakFrac * 100} y2={100 - peakFrac * 100} />
                  <line class="lo" x1="0" x2={$vram.used.length} y1={100 - ($vram.min / $vram.total) * 100} y2={100 - ($vram.min / $vram.total) * 100} />
                </svg>
              </span>
              <span class="vals mono"><span class="now {lvl($vram.used[$vram.used.length - 1] / $vram.total)}">{gb($vram.used[$vram.used.length - 1])} / {gbi($vram.total)} GB</span><span class="range">{gb($vram.min)}–{gb($vram.peak)}</span></span>
            </div>
          {/if}
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

  /* two halves: compute (cpu/gpu) left, memory (ram/vram) right */
  .meters { display: flex; gap: 18px; align-items: center; margin: 8px 0 4px 1.2em; min-width: 0; }
  .col { flex: 1; display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .meter { display: flex; align-items: center; gap: 8px; min-width: 0; }

  .mlabel { font-size: 10px; color: var(--muted); width: 2.6em; flex: 0 0 auto; }
  .bar { position: relative; flex: 1 1 auto; height: 8px; min-width: 0; background: var(--bg-raised); border-radius: 2px; overflow: hidden; }
  .fill { position: absolute; left: 0; top: 0; height: 100%; background: var(--meter); }
  .tick.hi { position: absolute; top: 0; height: 100%; width: 1px; background: color-mix(in srgb, var(--fg) 55%, transparent); }

  .hist { position: relative; flex: 1 1 auto; height: 44px; min-width: 0; background: var(--bg-raised); border-radius: 2px; overflow: hidden; }
  .hist svg { width: 100%; height: 100%; display: block; }
  .hist rect { fill: var(--meter); }
  .hist line { stroke-width: 1; vector-effect: non-scaling-stroke; }
  .hist line.lo { stroke: var(--muted); opacity: 0.7; }

  /* capacity colours — shared by bar fill, vram peak line and the now readout */
  .fill.warm, .hist line.hi.warm { background: var(--accent); stroke: var(--accent); }
  .fill.hot, .hist line.hi.hot { background: var(--miss); stroke: var(--miss); }
  .hist line.hi.ok { stroke: color-mix(in srgb, var(--fg) 55%, transparent); }

  .vals { display: flex; flex-direction: column; align-items: flex-end; line-height: 1.25; flex: 0 0 auto; min-width: 6.5em; }
  .now { font-size: 10px; color: var(--fg); }
  .now.warm { color: var(--accent); }
  .now.hot { color: var(--miss); }
  .range { font-size: 9px; color: var(--muted); }

  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  @media (prefers-reduced-motion: reduce) { .glyph.running { animation: none; } }
</style>
