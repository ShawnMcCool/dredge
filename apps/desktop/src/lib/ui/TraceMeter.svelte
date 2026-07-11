<script lang="ts">
  // One run meter: a resource's sampled history zoomed to its own min–max
  // window (every pixel is variation), a thin absolute capacity gauge on the
  // right edge marking where that window sits in 0..total (tinted by peak
  // pressure), and the numeric now + min–max readout. Unit-agnostic — callers
  // pass formatters; all geometry/pressure math lives in lib/meter-math.
  import { gaugeWindow, pressure, tracePoints, traceWindow } from "../meter-math";

  interface Props {
    label: string;
    /** The run's samples so far, oldest first, in the same unit as `total`. */
    hist: number[];
    /** Absolute capacity the gauge positions against. */
    total: number;
    /** Formats the current (last) sample for the readout. */
    format: (v: number) => string;
    /** Formats the run's min–peak for the readout's second line. */
    formatRange: (min: number, peak: number) => string;
    /** Headline metric gets a taller box. */
    tall?: boolean;
  }
  let { label, hist, total, format, formatRange, tall = false }: Props = $props();

  const min = $derived(hist.length ? Math.min(...hist) : 0);
  const peak = $derived(hist.length ? Math.max(...hist) : 0);
  const cur = $derived(hist.length ? hist[hist.length - 1] : 0);
  const win = $derived(traceWindow(min, peak, total));
  const pts = $derived(tracePoints(hist, win));
  const gauge = $derived(gaugeWindow(min, peak, total));
</script>

{#if hist.length >= 2}
  <div class="meter" class:tall>
    <span class="mlabel mono">{label}</span>
    <span class="hist">
      <svg class="trace" viewBox="0 0 {hist.length - 1} 100" preserveAspectRatio="none">
        <polygon points="0,100 {pts} {hist.length - 1},100" />
        <polyline points={pts} vector-effect="non-scaling-stroke" />
      </svg>
      <svg class="gauge {pressure(peak / total)}" viewBox="0 0 6 100" preserveAspectRatio="none">
        <rect class="back" x="0" y="0" width="6" height="100" />
        <rect class="win" x="0" y={gauge.y} width="6" height={gauge.h} />
      </svg>
    </span>
    <span class="vals mono"
      ><span class="now {pressure(cur / total)}">{format(cur)}</span><span class="range">{formatRange(min, peak)}</span></span
    >
  </div>
{/if}

<style>
  .meter {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .mlabel {
    font-size: 10px;
    color: var(--muted);
    width: 2.6em;
    flex: 0 0 auto;
  }

  .hist {
    position: relative;
    flex: 1 1 auto;
    height: 34px;
    min-width: 0;
    background: var(--bg-raised);
    border-radius: 2px;
    overflow: hidden;
  }
  .meter.tall .hist {
    height: 44px;
  }
  .trace {
    position: absolute;
    inset: 0;
    width: calc(100% - 7px);
    height: 100%;
    display: block;
  }
  .trace polygon {
    fill: color-mix(in srgb, var(--meter) 45%, transparent);
  }
  .trace polyline {
    fill: none;
    stroke: color-mix(in srgb, var(--fg) 75%, transparent);
    stroke-width: 1.2;
  }
  .gauge {
    position: absolute;
    right: 0;
    top: 0;
    width: 6px;
    height: 100%;
  }
  .gauge .back {
    fill: color-mix(in srgb, var(--line) 70%, transparent);
  }
  .gauge.ok .win {
    fill: var(--meter);
  }
  .gauge.warm .win {
    fill: var(--accent);
  }
  .gauge.hot .win {
    fill: var(--miss);
  }

  .vals {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    line-height: 1.25;
    flex: 0 0 auto;
    min-width: 6.5em;
  }
  .now {
    font-size: 10px;
    color: var(--fg);
  }
  .now.warm {
    color: var(--accent);
  }
  .now.hot {
    color: var(--miss);
  }
  .range {
    font-size: 9px;
    color: var(--muted);
  }
</style>
