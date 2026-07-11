<script lang="ts">
  import { fmtElapsed } from "../lib/format";
  import { prepareState, workSample, vram, type PrepareStepState } from "../lib/stores";
  import TraceMeter from "../lib/ui/TraceMeter.svelte";

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
          <TraceMeter label="cpu" hist={cpuH} total={CPU_SCALE} format={pct} formatRange={pctRange} />
          {#if $workSample.ram_total_mb != null}
            <TraceMeter
              label="ram"
              hist={ramH}
              total={$workSample.ram_total_mb}
              format={(v) => `${gb(v)} / ${gbi($workSample!.ram_total_mb!)} GB`}
              formatRange={gbRange}
            />
          {/if}
        </div>
        <div class="pair">
          <TraceMeter label="gpu" hist={gpuH} total={100} format={pct} formatRange={pctRange} />
          {#if $vram && $vram.used.length}
            <TraceMeter
              label="vram"
              hist={$vram.used}
              total={$vram.total}
              format={(v) => `${gb(v)} / ${gbi($vram!.total)} GB`}
              formatRange={gbRange}
              tall
            />
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

  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  @media (prefers-reduced-motion: reduce) { .glyph.running { animation: none; } }
</style>
