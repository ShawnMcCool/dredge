<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { canvasSize } from "../lib/actions/canvasSize";
  import {
    actions,
    currentLoop,
    gridSnap,
    openingSong,
    openSong,
    position,
    selection,
    type LoopRegion,
    type OpenSong,
  } from "../lib/stores";
  import { labelColor } from "../lib/waveform-colors";
  import {
    adjustWindow,
    playheadSecs,
    secToX,
    snapToGrid,
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
  const SNAP_PX = 10; // grid-snap pull radius around a downbeat
  const MIN_TICK_PX = 6; // hide beat ticks when they'd sit closer than this

  let canvas: HTMLCanvasElement;
  let view: View = $state({ startSec: 0, endSec: 1, width: 1 });
  let lastSongId: number | null = null;
  /** Lane span whose bounds currently drive the transport loop (clicked). */
  let activeSpan: { start: number; end: number } | null = $state(null);

  /** One row in the structure lane: saved sections when any exist, analysis
   *  suggestions otherwise (never both — the Sections tab shows the rest). */
  interface LaneSpan {
    name: string;
    start: number;
    end: number;
    suggested: boolean;
  }

  function laneSpans(open: OpenSong): LaneSpan[] {
    if (open.sections.length > 0) {
      return open.sections.map((s) => ({ ...s, suggested: false }));
    }
    return (open.analysis?.sections ?? []).map((s) => ({
      name: s.label,
      start: s.start,
      end: s.end,
      suggested: true,
    }));
  }

  // pointer interaction state
  type Drag =
    | { mode: "select"; anchorX: number; moved: boolean }
    | { mode: "resize"; loop: LoopRegion; edge: "start" | "end"; start: number; end: number }
    | { mode: "lane"; anchor: { start: number; end: number }; moved: boolean };
  let drag: Drag | null = null;

  // reset the view when a different song opens
  $effect(() => {
    const open = $openSong;
    if (open && open.song.id !== lastSongId) {
      lastSongId = open.song.id;
      activeSpan = null;
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
      ctx.fillText(
        get(openingSong) !== null ? "opening…" : "no song open",
        8,
        LANE_H + WAVE_H / 2,
      );
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

    // beat grid — short ticks along the bottom edge, downbeats stronger;
    // hidden when beats would crowd (< MIN_TICK_PX apart)
    const analysis = open.analysis;
    if (analysis && analysis.beats.length > 1) {
      const pxPerSec = w / (view.endSec - view.startSec);
      const beatSpan =
        (analysis.beats[analysis.beats.length - 1] - analysis.beats[0]) /
        (analysis.beats.length - 1);
      if (beatSpan * pxPerSec >= MIN_TICK_PX) {
        const bottom = LANE_H + WAVE_H;
        const downs = new Set(analysis.downbeats);
        ctx.fillStyle = css("--line");
        for (const b of analysis.beats) {
          if (downs.has(b)) continue;
          const x = Math.round(secToX(view, b));
          if (x >= 0 && x <= w) ctx.fillRect(x, bottom - 6, 1, 6);
        }
        ctx.fillStyle = css("--muted");
        for (const d of analysis.downbeats) {
          const x = Math.round(secToX(view, d));
          if (x >= 0 && x <= w) ctx.fillRect(x, bottom - 11, 1, 11);
        }
      }
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
      if (get(currentLoop)?.id === l.id) {
        ctx.fillStyle = accent;
        ctx.fillRect(x0 - 2, LANE_H, 4, 10);
        ctx.fillRect(x1 - 2, LANE_H, 4, 10);
      }
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

    // structure lane — label-colored spans: saved sections solid, analysis
    // suggestions dashed/dimmer/italic; clicked span gets a second fill pass
    for (const s of laneSpans(open)) {
      const x0 = secToX(view, s.start);
      const x1 = secToX(view, s.end);
      if (x1 < 0 || x0 > w) continue;
      const { fill, edge } = labelColor(s.name);
      const active = activeSpan?.start === s.start && activeSpan?.end === s.end;
      ctx.globalAlpha = s.suggested && !active ? 0.6 : 1;
      ctx.fillStyle = fill;
      ctx.fillRect(x0, 2, x1 - x0 - 1, LANE_H - 4);
      if (active) ctx.fillRect(x0, 2, x1 - x0 - 1, LANE_H - 4);
      ctx.strokeStyle = edge;
      ctx.lineWidth = 1;
      ctx.setLineDash(s.suggested ? [3, 3] : []);
      ctx.strokeRect(x0 + 0.5, 2.5, x1 - x0 - 2, LANE_H - 5);
      ctx.setLineDash([]);
      ctx.globalAlpha = 1;
      ctx.fillStyle = css("--fg");
      ctx.font = (s.suggested ? "italic " : "") + "11px " + css("--mono");
      ctx.fillText(s.name, x0 + 4, LANE_H - 8, Math.max(x1 - x0 - 8, 0));
    }

    // playhead — 1 px accent line
    if (playheadX >= 0 && playheadX <= w) {
      ctx.fillStyle = accent;
      ctx.fillRect(Math.round(playheadX), 0, 1, LANE_H + WAVE_H);
    }
  }

  function applySize(w: number, _h: number, dpr: number) {
    if (!canvas) return;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round((LANE_H + WAVE_H) * dpr);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${LANE_H + WAVE_H}px`;
    view = { ...view, width: w };
  }

  onMount(() => {
    let raf = 0;
    const frame = () => {
      draw();
      raf = requestAnimationFrame(frame);
    };
    raf = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(raf);
  });

  /** Topmost loop whose body is under a canvas point (below the lane). */
  function hitLoopBody(x: number, y: number): LoopRegion | null {
    if (y < LANE_H) return null;
    const open = get(openSong);
    if (!open) return null;
    const sec = xToSec(view, x);
    for (let i = open.loops.length - 1; i >= 0; i--) {
      const l = open.loops[i];
      if (sec >= l.start && sec <= l.end) return l;
    }
    return null;
  }

  function hitLoopEdge(x: number): { loop: LoopRegion; edge: "start" | "end" } | null {
    const open = get(openSong);
    if (!open) return null;
    for (const l of open.loops) {
      if (Math.abs(secToX(view, l.start) - x) <= EDGE_PX) return { loop: l, edge: "start" };
      if (Math.abs(secToX(view, l.end) - x) <= EDGE_PX) return { loop: l, edge: "end" };
    }
    return null;
  }

  function canvasX(e: MouseEvent): number {
    return e.clientX - canvas.getBoundingClientRect().left;
  }

  function canvasY(e: MouseEvent): number {
    return e.clientY - canvas.getBoundingClientRect().top;
  }

  /** Lane span containing a time (used while dragging across headers). */
  function spanAtTime(sec: number): { start: number; end: number } | null {
    const open = get(openSong);
    if (!open) return null;
    const s = laneSpans(open).find((sp) => sec >= sp.start && sec <= sp.end);
    return s ? { start: s.start, end: s.end } : null;
  }

  /** Structure-lane span under a canvas point (lane y-band only). */
  function hitLaneSpan(x: number, y: number): LaneSpan | null {
    if (y >= LANE_H) return null;
    const open = get(openSong);
    if (!open) return null;
    const sec = xToSec(view, x);
    return laneSpans(open).find((s) => sec >= s.start && sec <= s.end) ?? null;
  }

  function onPointerDown(e: PointerEvent) {
    if (!get(openSong)) return;
    const x = canvasX(e);
    // lane click/drag: start a lane drag; single click handled on pointer-up
    const span = hitLaneSpan(x, canvasY(e));
    if (span) {
      canvas.setPointerCapture(e.pointerId);
      drag = { mode: "lane", anchor: { start: span.start, end: span.end }, moved: false };
      return;
    }
    canvas.setPointerCapture(e.pointerId);
    const edge = hitLoopEdge(x);
    drag = edge
      ? { mode: "resize", loop: edge.loop, edge: edge.edge, start: edge.loop.start, end: edge.loop.end }
      : { mode: "select", anchorX: x, moved: false };
  }

  /** Double-click on a *suggested* span seeds the selection (l/p work on it). */
  function onDblClick(e: MouseEvent) {
    const span = hitLaneSpan(canvasX(e), canvasY(e));
    if (span?.suggested) selection.set({ start: span.start, end: span.end });
  }

  /** Pull a time onto the nearest downbeat when grid snap applies. */
  function maybeSnap(secs: number): number {
    const downbeats = get(openSong)?.analysis?.downbeats;
    if (!downbeats?.length || !get(gridSnap)) return secs;
    return snapToGrid(secs, downbeats, view, SNAP_PX);
  }

  function onPointerMove(e: PointerEvent) {
    const x = canvasX(e);
    if (!drag) {
      const y = canvasY(e);
      let cursor = "crosshair";
      if (hitLoopEdge(x)) cursor = "ew-resize";
      else if (hitLaneSpan(x, y)) cursor = "grab";
      else if (hitLoopBody(x, y)) cursor = "pointer";
      canvas.style.cursor = cursor;
      return;
    }
    if (drag.mode === "lane") {
      const cur = spanAtTime(Math.min(Math.max(xToSec(view, x), 0), duration()));
      if (cur && (cur.start !== drag.anchor.start || cur.end !== drag.anchor.end)) {
        drag.moved = true;
      }
      if (drag.moved) {
        const lo = cur ? Math.min(drag.anchor.start, cur.start) : drag.anchor.start;
        const hi = cur ? Math.max(drag.anchor.end, cur.end) : drag.anchor.end;
        selection.set({ start: lo, end: hi });
      }
      return;
    }
    const secs = maybeSnap(Math.min(Math.max(xToSec(view, x), 0), duration()));
    if (drag.mode === "resize") {
      if (drag.edge === "start") drag.start = Math.min(secs, drag.end - 0.05);
      else drag.end = Math.max(secs, drag.start + 0.05);
    } else {
      if (Math.abs(x - drag.anchorX) >= CLICK_PX) drag.moved = true;
      if (drag.moved) {
        const a = maybeSnap(xToSec(view, drag.anchorX));
        selection.set({ start: Math.min(a, secs), end: Math.max(a, secs) });
      }
    }
  }

  function onPointerUp(e: PointerEvent) {
    const d = drag;
    drag = null;
    if (!d) return;
    if (d.mode === "lane") {
      if (!d.moved) {
        activeSpan = { start: d.anchor.start, end: d.anchor.end };
        void actions.setTransportLoop(d.anchor.start, d.anchor.end);
      }
      return;
    }
    if (d.mode === "resize") {
      void actions.updateLoop(d.loop.id, { start: d.start, end: d.end });
    } else if (!d.moved) {
      const cx = canvasX(e);
      const loop = hitLoopBody(cx, canvasY(e));
      if (loop) void actions.selectLoop(loop);
      else void actions.seek(Math.min(Math.max(xToSec(view, cx), 0), duration()));
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

  let dur = $derived($openSong?.song.duration_secs ?? 0);
  const MIN_WIN = 1; // seconds — min visible window

  let scrollEl: HTMLDivElement;
  type ScrollDrag = { mode: "pan" | "start" | "end"; px0: number; s0: number; e0: number };
  let scrollDrag: ScrollDrag | null = null;

  function scrollPx(e: PointerEvent): number {
    const rect = scrollEl.getBoundingClientRect();
    return e.clientX - rect.left;
  }
  function pxToDsec(dpx: number): number {
    const w = scrollEl?.clientWidth || 1;
    return (dpx / w) * dur;
  }
  function onScrollDown(e: PointerEvent) {
    if (dur <= 0) return;
    const w = scrollEl.clientWidth;
    const px = scrollPx(e);
    const x0 = (view.startSec / dur) * w;
    const x1 = (view.endSec / dur) * w;
    const EDGE = 6;
    let mode: "pan" | "start" | "end";
    if (Math.abs(px - x0) <= EDGE) mode = "start";
    else if (Math.abs(px - x1) <= EDGE) mode = "end";
    else if (px > x0 && px < x1) mode = "pan";
    else {
      // click outside the window: recenter the window there (keep width)
      const width = view.endSec - view.startSec;
      const c = (px / w) * dur;
      const win = adjustWindow("pan", c - width / 2, c + width / 2, dur, MIN_WIN);
      view = { ...view, startSec: win.startSec, endSec: win.endSec };
      return;
    }
    scrollEl.setPointerCapture(e.pointerId);
    scrollDrag = { mode, px0: px, s0: view.startSec, e0: view.endSec };
  }
  function onScrollMove(e: PointerEvent) {
    if (!scrollDrag) return;
    const d = pxToDsec(scrollPx(e) - scrollDrag.px0);
    let s = scrollDrag.s0;
    let en = scrollDrag.e0;
    if (scrollDrag.mode === "pan") { s += d; en += d; }
    else if (scrollDrag.mode === "start") { s += d; }
    else { en += d; }
    const win = adjustWindow(scrollDrag.mode, s, en, dur, MIN_WIN);
    view = { ...view, startSec: win.startSec, endSec: win.endSec };
  }
  function onScrollUp() { scrollDrag = null; }
  function resetView() { view = { ...view, startSec: 0, endSec: dur }; }
</script>

<div class="waveform" use:canvasSize={applySize}>
  <canvas
    id="waveform-canvas"
    bind:this={canvas}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    ondblclick={onDblClick}
    onwheel={onWheel}
  ></canvas>
  <div
    class="scrollbar"
    role="scrollbar"
    aria-label="waveform range selector"
    aria-controls="waveform-canvas"
    aria-valuenow={Math.round((view.startSec / (dur || 1)) * 100)}
    aria-valuemin={0}
    aria-valuemax={100}
    tabindex="0"
    bind:this={scrollEl}
    onpointerdown={onScrollDown}
    onpointermove={onScrollMove}
    onpointerup={onScrollUp}
    onpointercancel={onScrollUp}
    ondblclick={resetView}
    title="drag to scroll · drag edges to zoom · double-click to fit"
  >
    {#if dur > 0}
      <div
        class="sb-window"
        style="left: {(view.startSec / dur) * 100}%; width: {((view.endSec - view.startSec) / dur) * 100}%"
      ></div>
    {/if}
  </div>
  {#if $openingSong !== null && $openSong}
    <!-- song switch in flight: keep the old waveform, show progress on top -->
    <div class="loading-bar"></div>
  {/if}
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

  /* thin indeterminate bar across the top of the stage while a new song
     decodes — same accent + timing language as the prepare modal's bar */
  .loading-bar {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 2px;
    overflow: hidden;
  }

  .loading-bar::after {
    content: "";
    position: absolute;
    width: 30%;
    height: 100%;
    background: var(--accent);
    animation: indeterminate 1s ease-in-out infinite;
  }

  @keyframes indeterminate {
    from {
      left: -30%;
    }
    to {
      left: 100%;
    }
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

  .scrollbar {
    position: relative;
    height: 12px;
    margin-top: 4px;
    background: var(--bg-raised);
    border-radius: 3px;
    cursor: pointer;
    user-select: none;
  }

  .sb-window {
    position: absolute;
    top: 0;
    bottom: 0;
    min-width: 6px;
    background: var(--accent);
    opacity: 0.35;
    border-radius: 3px;
    box-sizing: border-box;
  }
</style>
