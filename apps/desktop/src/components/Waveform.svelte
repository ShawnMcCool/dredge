<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import {
    actions,
    currentLoop,
    openSong,
    position,
    selection,
    type LoopRegion,
  } from "../lib/stores";
  import {
    playheadSecs,
    secToX,
    visibleBuckets,
    xToSec,
    zoom,
    type View,
  } from "../lib/waveform-math";

  const SAMPLE_RATE = 48000;
  const LANE_H = 24; // section lane above the waveform
  const WAVE_H = 200;
  const EDGE_PX = 4; // loop-edge hit zone
  const CLICK_PX = 5; // below this a drag is a click → seek

  let canvas: HTMLCanvasElement;
  let container: HTMLDivElement;
  let view: View = $state({ startSec: 0, endSec: 1, width: 1 });
  let lastSongId: number | null = null;

  // pointer interaction state
  type Drag =
    | { mode: "select"; anchorX: number; moved: boolean }
    | { mode: "resize"; loop: LoopRegion; edge: "start" | "end"; start: number; end: number };
  let drag: Drag | null = null;

  // reset the view when a different song opens
  $effect(() => {
    const open = $openSong;
    if (open && open.song.id !== lastSongId) {
      lastSongId = open.song.id;
      view = { startSec: 0, endSec: Math.max(open.song.duration_secs, 2), width: view.width };
    }
  });

  function duration(): number {
    return get(openSong)?.song.duration_secs ?? 0;
  }

  function css(name: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  }

  function loopBounds(l: LoopRegion): { start: number; end: number } {
    if (drag?.mode === "resize" && drag.loop.id === l.id) {
      return { start: drag.start, end: drag.end };
    }
    return { start: l.start, end: l.end };
  }

  function draw() {
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const w = view.width;
    const open = get(openSong);

    ctx.fillStyle = css("--bg");
    ctx.fillRect(0, 0, w, LANE_H + WAVE_H);

    if (!open) {
      ctx.fillStyle = css("--muted");
      ctx.font = "13px " + css("--mono");
      ctx.fillText("no song open", 8, LANE_H + WAVE_H / 2);
      return;
    }

    const playhead = playheadSecs(get(position), performance.now());
    const playheadX = secToX(view, playhead);
    const mid = LANE_H + WAVE_H / 2;
    const peaks = open.peaks;
    const perBucket = peaks.frames_per_bucket / SAMPLE_RATE;
    const { first, last } = visibleBuckets(
      view,
      peaks.frames_per_bucket,
      SAMPLE_RATE,
      peaks.buckets.length,
    );

    // waveform: per px column aggregate the buckets it covers
    const wave = css("--wave");
    const played = css("--accent-dim");
    for (let x = 0; x < w; x++) {
      const b0 = Math.max(Math.floor(xToSec(view, x) / perBucket), first);
      const b1 = Math.min(Math.max(Math.floor(xToSec(view, x + 1) / perBucket), b0), last);
      let lo = Infinity;
      let hi = -Infinity;
      for (let b = b0; b <= b1; b++) {
        const bucket = peaks.buckets[b];
        if (!bucket) continue;
        lo = Math.min(lo, bucket[0]);
        hi = Math.max(hi, bucket[1]);
      }
      if (lo > hi) continue;
      ctx.fillStyle = x < playheadX ? played : wave;
      const y0 = mid - hi * (WAVE_H / 2 - 2);
      const y1 = mid - lo * (WAVE_H / 2 - 2);
      ctx.fillRect(x, y0, 1, Math.max(y1 - y0, 1));
    }

    // loop regions — translucent accent, junction edges dashed
    const accent = css("--accent");
    for (const l of open.loops) {
      const { start, end } = loopBounds(l);
      const x0 = secToX(view, start);
      const x1 = secToX(view, end);
      if (x1 < 0 || x0 > w) continue;
      ctx.globalAlpha = get(currentLoop)?.id === l.id ? 0.16 : 0.08;
      ctx.fillStyle = accent;
      ctx.fillRect(x0, LANE_H, x1 - x0, WAVE_H);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = accent;
      ctx.lineWidth = 1;
      ctx.setLineDash(l.kind.kind === "junction" ? [3, 3] : []);
      ctx.beginPath();
      ctx.moveTo(x0 + 0.5, LANE_H);
      ctx.lineTo(x0 + 0.5, LANE_H + WAVE_H);
      ctx.moveTo(x1 - 0.5, LANE_H);
      ctx.lineTo(x1 - 0.5, LANE_H + WAVE_H);
      ctx.stroke();
      ctx.setLineDash([]);
    }

    // selection — brighter
    const sel = get(selection);
    if (sel) {
      const x0 = secToX(view, sel.start);
      const x1 = secToX(view, sel.end);
      ctx.globalAlpha = 0.18;
      ctx.fillStyle = accent;
      ctx.fillRect(x0, LANE_H, x1 - x0, WAVE_H);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = css("--fg");
      ctx.strokeRect(x0 + 0.5, LANE_H + 0.5, x1 - x0 - 1, WAVE_H - 1);
    }

    // section lane
    ctx.font = "11px " + css("--mono");
    for (const s of open.sections) {
      const x0 = secToX(view, s.start);
      const x1 = secToX(view, s.end);
      if (x1 < 0 || x0 > w) continue;
      ctx.fillStyle = css("--line");
      ctx.fillRect(x0, 2, x1 - x0 - 1, LANE_H - 4);
      ctx.fillStyle = css("--fg");
      ctx.fillText(s.name, x0 + 4, LANE_H - 8, Math.max(x1 - x0 - 8, 0));
    }

    // playhead — 1 px accent line
    if (playheadX >= 0 && playheadX <= w) {
      ctx.fillStyle = accent;
      ctx.fillRect(Math.round(playheadX), 0, 1, LANE_H + WAVE_H);
    }
  }

  function resize() {
    if (!canvas || !container) return;
    const dpr = window.devicePixelRatio || 1;
    const w = container.clientWidth;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round((LANE_H + WAVE_H) * dpr);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${LANE_H + WAVE_H}px`;
    view = { ...view, width: w };
  }

  onMount(() => {
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(container);
    let raf = 0;
    const frame = () => {
      draw();
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => {
      ro.disconnect();
      cancelAnimationFrame(raf);
    };
  });

  function hitLoopEdge(x: number): { loop: LoopRegion; edge: "start" | "end" } | null {
    const open = get(openSong);
    if (!open) return null;
    for (const l of open.loops) {
      if (Math.abs(secToX(view, l.start) - x) <= EDGE_PX) return { loop: l, edge: "start" };
      if (Math.abs(secToX(view, l.end) - x) <= EDGE_PX) return { loop: l, edge: "end" };
    }
    return null;
  }

  function canvasX(e: PointerEvent | WheelEvent): number {
    return e.clientX - canvas.getBoundingClientRect().left;
  }

  function onPointerDown(e: PointerEvent) {
    if (!get(openSong)) return;
    canvas.setPointerCapture(e.pointerId);
    const x = canvasX(e);
    const edge = hitLoopEdge(x);
    drag = edge
      ? { mode: "resize", loop: edge.loop, edge: edge.edge, start: edge.loop.start, end: edge.loop.end }
      : { mode: "select", anchorX: x, moved: false };
  }

  function onPointerMove(e: PointerEvent) {
    if (!drag) return;
    const x = canvasX(e);
    const secs = Math.min(Math.max(xToSec(view, x), 0), duration());
    if (drag.mode === "resize") {
      if (drag.edge === "start") drag.start = Math.min(secs, drag.end - 0.05);
      else drag.end = Math.max(secs, drag.start + 0.05);
    } else {
      if (Math.abs(x - drag.anchorX) >= CLICK_PX) drag.moved = true;
      if (drag.moved) {
        const a = xToSec(view, drag.anchorX);
        selection.set({ start: Math.min(a, secs), end: Math.max(a, secs) });
      }
    }
  }

  function onPointerUp(e: PointerEvent) {
    const d = drag;
    drag = null;
    if (!d) return;
    if (d.mode === "resize") {
      void actions.updateLoop(d.loop.id, { start: d.start, end: d.end });
    } else if (!d.moved) {
      void actions.seek(Math.min(Math.max(xToSec(view, canvasX(e)), 0), duration()));
    }
  }

  function onWheel(e: WheelEvent) {
    if (!get(openSong)) return;
    e.preventDefault();
    if (e.shiftKey) {
      // pan
      const span = view.endSec - view.startSec;
      const shift = (e.deltaY / view.width) * span;
      let startSec = Math.min(Math.max(view.startSec + shift, 0), duration() - span);
      view = { ...view, startSec, endSec: startSec + span };
    } else {
      const factor = e.deltaY > 0 ? 1.25 : 0.8;
      view = zoom(view, xToSec(view, canvasX(e)), factor, duration());
    }
  }

  function autoLoopName(): string {
    const open = get(openSong);
    const n = (open?.loops.filter((l) => l.kind.kind === "manual").length ?? 0) + 1;
    return `loop ${n}`;
  }

  async function loopSelection() {
    const sel = get(selection);
    if (!sel) return;
    const l = await actions.createLoop(autoLoopName(), sel.start, sel.end);
    await actions.selectLoop(l);
    selection.set(null);
  }

  async function playSelection() {
    const sel = get(selection);
    if (!sel) return;
    await actions.setTransportLoop(sel.start, sel.end);
    await actions.seek(sel.start);
    await actions.play();
  }

  let chipLeft = $derived($selection ? Math.min(secToX(view, $selection.end) + 8, view.width - 180) : 0);
</script>

<div class="waveform" bind:this={container}>
  <canvas
    bind:this={canvas}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onwheel={onWheel}
  ></canvas>
  {#if $selection}
    <div class="chip fade-in" style="left: {chipLeft}px">
      <button onclick={loopSelection}>Loop selection</button>
      <button onclick={playSelection}>Play selection</button>
    </div>
  {/if}
</div>

<style>
  .waveform {
    position: relative;
    width: 100%;
  }

  canvas {
    display: block;
    cursor: crosshair;
  }

  .chip {
    position: absolute;
    top: 32px;
    display: flex;
    gap: calc(var(--space) / 2);
  }

  .chip button {
    font-size: 12px;
    padding: 2px 6px;
  }
</style>
