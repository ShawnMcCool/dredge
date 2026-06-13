<script lang="ts">
  import {
    prepareState,
    workSample,
    profiles,
    type PrepareStepState,
  } from "../lib/stores";

  const STEPS = [
    { key: "analysis", label: "analyzing structure", op: "analysis" },
    { key: "stems", label: "separating stems", op: "stems" },
  ] as const;

  const GLYPHS: Record<PrepareStepState, string> = {
    pending: "·",
    running: "◌",
    done: "✓",
    cached: "✓",
    failed: "✗",
  };

  function fmt(ms: number): string {
    if (ms < 1000) return `${ms} ms`;
    const s = ms / 1000;
    return s < 60 ? `${s.toFixed(1)} s` : `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  }

  // idle: most recent finished run as a one-liner
  let last = $derived($profiles[0]);
  let lastLine = $derived.by(() => {
    if (!last) return null;
    return [last.op, fmt(last.total_ms), last.device, last.engine].filter(Boolean).join(" · ");
  });
</script>

{#if $prepareState}
  <section class="live">
    <h3 class="mono">PREPARING</h3>
    {#each STEPS as step (step.key)}
      {@const s = $prepareState.steps[step.key]}
      {@const active = $workSample && $workSample.op === step.op && s === "running"}
      <div class="step">
        <span class="glyph mono" class:running={s === "running"} class:done={s === "done" || s === "cached"} class:failed={s === "failed"}>{GLYPHS[s]}</span>
        <span class="name">{step.label}</span>
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
      {#if active}
        <div class="meters">
          <div class="meter">
            <span class="mlabel mono">cpu</span>
            <span class="bar"><span class="fill" style="width: {Math.min(100, $workSample.cpu_pct / 8)}%"></span></span>
            <span class="mval mono">{$workSample.cpu_pct}%</span>
          </div>
          {#if $workSample.gpu_util != null}
            <div class="meter">
              <span class="mlabel mono">gpu</span>
              <span class="bar"><span class="fill" style="width: {$workSample.gpu_util}%"></span></span>
              <span class="mval mono">{$workSample.gpu_util}%{#if $workSample.gpu_mem_total_mb} · {($workSample.gpu_mem_used_mb ?? 0) / 1024 | 0}/{($workSample.gpu_mem_total_mb / 1024) | 0} GB{/if}</span>
            </div>
          {/if}
        </div>
      {/if}
    {/each}
  </section>
{:else if lastLine}
  <section class="live idle"><span class="muted mono">last run · {lastLine}</span></section>
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
  .stage { font-size: 11px; color: var(--accent); }
  .elapsed { margin-left: auto; font-size: 11px; color: var(--muted); }
  .muted { color: var(--muted); font-size: 11px; }
  .error { color: var(--miss); font-size: 11px; }
  .meters { display: flex; flex-direction: column; gap: 2px; margin: 0 0 6px 1.2em; }
  .meter { display: flex; align-items: center; gap: 6px; }
  .mlabel { font-size: 10px; color: var(--muted); width: 2em; }
  .bar { flex: 1; height: 4px; background: var(--bg-raised); border-radius: 2px; overflow: hidden; max-width: 220px; }
  .fill { display: block; height: 100%; background: var(--accent); }
  .mval { font-size: 10px; color: var(--muted); width: 9em; }
  .idle { color: var(--muted); }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  @media (prefers-reduced-motion: reduce) { .glyph.running { animation: none; } }
</style>
