<script lang="ts">
  import { fmtElapsed } from "../lib/format";
  import { prepareState, workSample, vram, type PrepareStepState } from "../lib/stores";

  const STEPS = [
    { key: "analysis", label: "analyzing structure", op: "analysis", model: "SongFormer" },
    { key: "stems", label: "separating stems", op: "stems", model: "Demucs" },
  ] as const;

  const GLYPHS: Record<PrepareStepState, string> = {
    pending: "·", running: "◌", done: "✓", cached: "✓", failed: "✗",
  };

  // Per-metric run history. workSample is instantaneous, so we accumulate here;
  // the component unmounts when prepareState clears between runs, so these
  // reset on remount. Plain accumulators + fresh-array assignment keep the
  // effect's only dependency $workSample (no self-referential loop).
  let cpuH = $state<number[]>([]);
  let gpuH = $state<number[]>([]);
  let ramH = $state<number[]>([]);
  const accCpu: number[] = [];
  const accGpu: number[] = [];
  const accRam: number[] = [];

  $effect(() => {
    const s = $workSample;
    if (!s) return;
    accCpu.push(s.cpu_pct);
    cpuH = [...accCpu];
    if (s.gpu_util != null) {
      accGpu.push(s.gpu_util);
      gpuH = [...accGpu];
    }
    if (s.ram_used_mb != null) {
      accRam.push(s.ram_used_mb);
      ramH = [...accRam];
    }
  });

  // cpu_pct sums across cores; 800 is the same absolute ceiling the old bar used
  const CPU_SCALE = 800;
  const gb = (mb: number) => (mb / 1024).toFixed(1);
  const gbi = (mb: number) => Math.round(mb / 1024);
  const pct = (v: number) => `${Math.round(v)}%`;
  const pctRange = (min: number, peak: number) => `${Math.round(min)}–${Math.round(peak)}%`;
  const gbRange = (min: number, peak: number) => `${gb(min)}–${gb(peak)}`;
  // the capacity colours: neutral until a resource fills up — amber when busy,
  // red near its limit. The gauge tint, not the trace, carries the meaning.
  const lvl = (frac: number) => (frac >= 0.9 ? "hot" : frac >= 0.72 ? "warm" : "ok");
</script>

<!-- one meter: the run's history zoomed to its own min–max window (every pixel
     is variation — the flat mass below the run's floor is cropped away), plus
     a thin absolute capacity gauge on the right edge marking where that window
     sits in 0..total, tinted by peak pressure. -->
{#snippet trace(
  label: string,
  hist: number[],
  total: number,
  nowFmt: (v: number) => string,
  rangeFmt: (min: number, peak: number) => string,
  tall = false,
)}
  {#if hist.length >= 2}
    {@const min = Math.min(...hist)}
    {@const peak = Math.max(...hist)}
    {@const cur = hist[hist.length - 1]}
    {@const pad = (peak - min) * 0.12 + total * 0.003}
    {@const lo = Math.max(0, min - pad)}
    {@const hi = Math.min(total, peak + pad)}
    {@const pts = hist.map((u, i) => `${i},${(100 - ((u - lo) / (hi - lo)) * 100).toFixed(2)}`).join(" ")}
    <div class="meter" class:tall>
      <span class="mlabel mono">{label}</span>
      <span class="hist">
        <svg class="trace" viewBox="0 0 {hist.length - 1} 100" preserveAspectRatio="none">
          <polygon points="0,100 {pts} {hist.length - 1},100" />
          <polyline points={pts} vector-effect="non-scaling-stroke" />
        </svg>
        <svg class="gauge {lvl(peak / total)}" viewBox="0 0 6 100" preserveAspectRatio="none">
          <rect class="back" x="0" y="0" width="6" height="100" />
          <rect
            class="win"
            x="0"
            y={100 - (peak / total) * 100}
            width="6"
            height={Math.max(2, ((peak - min) / total) * 100)}
          />
        </svg>
      </span>
      <span class="vals mono"
        ><span class="now {lvl(cur / total)}">{nowFmt(cur)}</span><span class="range">{rangeFmt(min, peak)}</span></span
      >
    </div>
  {/if}
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
          <span class="elapsed mono">{fmtElapsed($workSample.elapsed_ms)}</span>
        {:else if s === "cached"}
          <span class="muted mono">cached</span>
        {/if}
      </div>
      {#if $prepareState.errors[step.key]}
        <p class="error">{$prepareState.errors[step.key]}</p>
      {/if}
    {/each}

    {#if $workSample}
      <!-- paired by hardware side — host (cpu+ram) then device (gpu+vram),
           since those are the meters that move together during a phase -->
      <div class="meters">
        <div class="pair">
          {@render trace("cpu", cpuH, CPU_SCALE, pct, pctRange)}
          {#if $workSample.ram_total_mb != null}
            {@render trace("ram", ramH, $workSample.ram_total_mb, (v) => `${gb(v)} / ${gbi($workSample!.ram_total_mb!)} GB`, gbRange)}
          {/if}
        </div>
        <div class="pair">
          {@render trace("gpu", gpuH, 100, pct, pctRange)}
          {#if $vram && $vram.used.length}
            {@render trace("vram", $vram.used, $vram.total, (v) => `${gb(v)} / ${gbi($vram!.total)} GB`, gbRange, true)}
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
  .name { font-size: 13px; white-space: nowrap; }
  .model { font-size: 10px; color: var(--muted); white-space: nowrap; }
  .stage { font-size: 11px; color: var(--accent); }
  .elapsed { margin-left: auto; font-size: 11px; color: var(--muted); }
  .muted { color: var(--muted); font-size: 11px; }
  /* a failure's message gets its own full-width line under the step, aligned
     with the step name, so long install hints wrap as readable prose */
  .error { color: var(--miss); font-size: 11px; line-height: 1.5; margin: 0 0 6px calc(1.2em + var(--space)); }

  .meters { display: flex; flex-direction: column; gap: 16px; margin: 8px 0 4px 1.2em; min-width: 0; }
  .pair { display: flex; flex-direction: column; gap: 8px; min-width: 0; }
  .meter { display: flex; align-items: center; gap: 8px; min-width: 0; }

  .mlabel { font-size: 10px; color: var(--muted); width: 2.6em; flex: 0 0 auto; }

  .hist { position: relative; flex: 1 1 auto; height: 34px; min-width: 0; background: var(--bg-raised); border-radius: 2px; overflow: hidden; }
  /* vram is the headline metric while separating — it gets the tall box */
  .meter.tall .hist { height: 44px; }
  .trace { position: absolute; inset: 0; width: calc(100% - 7px); height: 100%; display: block; }
  .trace polygon { fill: color-mix(in srgb, var(--meter) 45%, transparent); }
  .trace polyline { fill: none; stroke: color-mix(in srgb, var(--fg) 75%, transparent); stroke-width: 1.2; }
  .gauge { position: absolute; right: 0; top: 0; width: 6px; height: 100%; }
  .gauge .back { fill: color-mix(in srgb, var(--line) 70%, transparent); }
  .gauge.ok .win { fill: var(--meter); }
  .gauge.warm .win { fill: var(--accent); }
  .gauge.hot .win { fill: var(--miss); }

  .vals { display: flex; flex-direction: column; align-items: flex-end; line-height: 1.25; flex: 0 0 auto; min-width: 6.5em; }
  .now { font-size: 10px; color: var(--fg); }
  .now.warm { color: var(--accent); }
  .now.hot { color: var(--miss); }
  .range { font-size: 9px; color: var(--muted); }

  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  @media (prefers-reduced-motion: reduce) { .glyph.running { animation: none; } }
</style>
