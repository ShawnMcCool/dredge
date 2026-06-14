<script lang="ts">
  import { onMount } from "svelte";
  import { get } from "svelte/store";
  import { canvasSize } from "../lib/actions/canvasSize";
  import {
    actions,
    currentLoop,
    gridLines,
    gridSnap,
    gridSubdivision,
    gridVisible,
    openingSong,
    openSong,
    position,
    selection,
    workspaceReset,
    type LoopRegion,
    type OpenSong,
  } from "../lib/stores";
  import { labelColor } from "../lib/waveform-colors";
  import {
    adjustWindow,
    playheadSecs,
    secToX,
    snapToGrid,
    subdivisionTimes,
    visibleBuckets,
    xToSec,
    zoom,
    type View,
  } from "../lib/waveform-math";

  const GRID_SUBDIVS = ["bar", "beat", "eighth"] as const;
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
    | { mode: "lane"; anchor: { start: number; end: number }; moved: boolean }
    | { mode: "zoom"; anchorX: number; curX: number };
  let drag: Drag | null = null;
  // after a resize is released, hold the new bounds visually until the
  // loop.update round-trip lands in the store — otherwise the wave flashes back
  // to the old bounds for the frames between release and refresh.
  let pendingResize: { id: number; start: number; end: number } | null = null;

  /** Zoom out to frame the whole song (keeps the current canvas width). */
  function fitToSong(open: OpenSong) {
    view = { startSec: 0, endSec: Math.max(open.song.duration_secs, 2), width: view.width };
  }

  // reset the view when a different song opens
  $effect(() => {
    const open = $openSong;
    if (open && open.song.id !== lastSongId) {
      lastSongId = open.song.id;
      activeSpan = null;
      fitToSong(open);
    }
  });

  // workspace reset (controls box) — refit zoom + drop the clicked active span;
  // the selection/loop/playhead are cleared store-side by resetWorkspace().
  let lastReset = 0;
  $effect(() => {
    const n = $workspaceReset;
    if (n !== lastReset) {
      lastReset = n;
      activeSpan = null;
      const open = get(openSong);
      if (open) fitToSong(open);
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
    if (pendingResize?.id === l.id) {
      return { start: pendingResize.start, end: pendingResize.end };
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
      ctx.fillStyle = wave;
      const y0 = mid - hi * (WAVE_H / 2 - 2);
      const y1 = mid - lo * (WAVE_H / 2 - 2);
      ctx.fillRect(x, y0, 1, Math.max(y1 - y0, 1));
    }

    // beat grid — subdivision-aware; bottom ticks, or full vertical lines.
    // downbeats render stronger. Hidden when ticks would crowd (< MIN_TICK_PX).
    const analysis = open.analysis;
    if (get(gridVisible) && analysis && analysis.beats.length > 1) {
      const pxPerSec = w / (view.endSec - view.startSec);
      const sub = get(gridSubdivision);
      const times = subdivisionTimes(analysis.beats, analysis.downbeats, sub);
      const span =
        times.length > 1 ? (times[times.length - 1] - times[0]) / (times.length - 1) : Infinity;
      if (span * pxPerSec >= MIN_TICK_PX) {
        const lines = get(gridLines);
        const top = LANE_H;
        const bottom = LANE_H + WAVE_H;
        const downs = new Set(analysis.downbeats);
        for (const t of times) {
          const x = Math.round(secToX(view, t));
          if (x < 0 || x > w) continue;
          const strong = downs.has(t);
          ctx.fillStyle = strong ? css("--muted") : css("--line");
          if (lines) {
            ctx.globalAlpha = strong ? 0.5 : 0.28;
            ctx.fillRect(x, top, 1, WAVE_H);
            ctx.globalAlpha = 1;
          } else {
            const h = strong ? 11 : 6;
            ctx.fillRect(x, bottom - h, 1, h);
          }
        }
      }
    }

    // loop regions — the selected loop (the Delete target) reads boldly: bright
    // 2px edges, a solid top cap, taller grab handles, denser fill. Unselected
    // loops sit faint in the background. junction edges stay dashed.
    const accent = css("--accent");
    const accentDim = css("--accent-dim");
    for (const l of open.loops) {
      const { start, end } = loopBounds(l);
      const x0 = secToX(view, start);
      const x1 = secToX(view, end);
      if (x1 < 0 || x0 > w) continue;
      const isSel = get(currentLoop)?.id === l.id;
      ctx.globalAlpha = isSel ? 0.2 : 0.07;
      ctx.fillStyle = accent;
      ctx.fillRect(x0, LANE_H, x1 - x0, WAVE_H);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = isSel ? accent : accentDim;
      ctx.lineWidth = isSel ? 2 : 1;
      const off = isSel ? 1 : 0.5;
      ctx.setLineDash(l.kind.kind === "junction" ? [3, 3] : []);
      ctx.beginPath();
      ctx.moveTo(x0 + off, LANE_H);
      ctx.lineTo(x0 + off, LANE_H + WAVE_H);
      ctx.moveTo(x1 - off, LANE_H);
      ctx.lineTo(x1 - off, LANE_H + WAVE_H);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.lineWidth = 1;
      if (isSel) {
        ctx.fillStyle = accent;
        ctx.fillRect(x0, LANE_H, x1 - x0, 3); // solid cap across the top
        ctx.fillRect(x0 - 1, LANE_H, 3, 14); // grab handles, taller than before
        ctx.fillRect(x1 - 2, LANE_H, 3, 14);
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

    // middle-drag zoom preview — dashed accent box over the range to zoom into
    if (drag?.mode === "zoom") {
      const zx0 = Math.min(drag.anchorX, drag.curX);
      const zw = Math.abs(drag.curX - drag.anchorX);
      ctx.globalAlpha = 0.15;
      ctx.fillStyle = accent;
      ctx.fillRect(zx0, LANE_H, zw, WAVE_H);
      ctx.globalAlpha = 1;
      ctx.strokeStyle = accent;
      ctx.setLineDash([4, 3]);
      ctx.strokeRect(zx0 + 0.5, LANE_H + 0.5, Math.max(zw - 1, 0), WAVE_H - 1);
      ctx.setLineDash([]);
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
      // sticky label: pin to the visible left edge while any of the span is on
      // screen (but never past its right edge), and truncate against what's left
      const lpad = 4;
      const lx = Math.min(Math.max(x0 + lpad, lpad), x1 - lpad);
      ctx.fillText(s.name, lx, LANE_H - 8, Math.max(x1 - lx - lpad, 0));
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

  /** The loop edge (across all loops) nearest to canvas x. Right-drag grabs this
   *  from anywhere — like Hyprland's super+right-drag snapping to the nearest
   *  tile border instead of requiring a pixel-perfect hit. */
  function nearestLoopEdge(x: number): { loop: LoopRegion; edge: "start" | "end" } | null {
    const open = get(openSong);
    if (!open) return null;
    let best: { loop: LoopRegion; edge: "start" | "end" } | null = null;
    let bestDist = Infinity;
    for (const l of open.loops) {
      const ds = Math.abs(secToX(view, l.start) - x);
      if (ds < bestDist) ((bestDist = ds), (best = { loop: l, edge: "start" }));
      const de = Math.abs(secToX(view, l.end) - x);
      if (de < bestDist) ((bestDist = de), (best = { loop: l, edge: "end" }));
    }
    return best;
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
    // middle button: drag a range to zoom into it (click with no drag = fit)
    if (e.button === 1) {
      e.preventDefault();
      canvas.setPointerCapture(e.pointerId);
      drag = { mode: "zoom", anchorX: x, curX: x };
      return;
    }
    // right button is the ONLY resize: grab the nearest loop edge from anywhere
    // (Hyprland super+right-drag feel) and drag it. inert if there are no loops.
    if (e.button === 2) {
      const edge = nearestLoopEdge(x);
      if (edge) {
        e.preventDefault();
        canvas.setPointerCapture(e.pointerId);
        drag = { mode: "resize", loop: edge.loop, edge: edge.edge, start: edge.loop.start, end: edge.loop.end };
      }
      return;
    }
    if (e.button !== 0) return;
    // left button = select & drag only — it never resizes.
    // lane click/drag: start a lane drag; single click handled on pointer-up
    const span = hitLaneSpan(x, canvasY(e));
    if (span) {
      canvas.setPointerCapture(e.pointerId);
      drag = { mode: "lane", anchor: { start: span.start, end: span.end }, moved: false };
      return;
    }
    canvas.setPointerCapture(e.pointerId);
    drag = { mode: "select", anchorX: x, moved: false };
  }

  /** Double-click on a *suggested* span seeds the selection (l/p work on it). */
  function onDblClick(e: MouseEvent) {
    const span = hitLaneSpan(canvasX(e), canvasY(e));
    if (span?.suggested) selection.set({ start: span.start, end: span.end });
  }

  /** Pull a time onto the nearest grid line (at the chosen subdivision) when
   *  grid snap applies. */
  function maybeSnap(secs: number): number {
    const a = get(openSong)?.analysis;
    if (!a || !get(gridSnap)) return secs;
    const times = subdivisionTimes(a.beats, a.downbeats, get(gridSubdivision));
    if (!times.length) return secs;
    return snapToGrid(secs, times, view, SNAP_PX);
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
    if (drag.mode === "zoom") {
      drag.curX = x;
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
    if (d.mode === "zoom") {
      const a = xToSec(view, d.anchorX);
      const b = xToSec(view, d.curX);
      const lo = Math.max(0, Math.min(a, b));
      const hi = Math.min(duration(), Math.max(a, b));
      if (hi - lo >= 0.2 && Math.abs(d.curX - d.anchorX) >= CLICK_PX) {
        view = { ...view, startSec: lo, endSec: hi };
      } else {
        view = { ...view, startSec: 0, endSec: Math.max(duration(), 2) };
      }
      return;
    }
    if (d.mode === "lane") {
      if (!d.moved) {
        activeSpan = { start: d.anchor.start, end: d.anchor.end };
        void actions.setTransportLoop(d.anchor.start, d.anchor.end);
      }
      return;
    }
    if (d.mode === "resize") {
      if (d.start !== d.loop.start || d.end !== d.loop.end) {
        // pin the new bounds visually until the store reflects them, then release
        pendingResize = { id: d.loop.id, start: d.start, end: d.end };
        const id = d.loop.id;
        void actions.updateLoop(id, { start: d.start, end: d.end }).finally(() => {
          if (pendingResize?.id === id) pendingResize = null;
        });
      }
    } else if (!d.moved) {
      const cx = canvasX(e);
      // a plain click dismisses any drag-selection box + its Loop/Play chip
      selection.set(null);
      // clicking inside a loop selects it (for handles / Delete); clicking away
      // deselects, so it's always clear which loop Delete would remove. either
      // way the click still seeks — a plain click never engages the transport loop
      const loop = hitLoopBody(cx, canvasY(e));
      currentLoop.set(loop);
      void actions.seek(Math.min(Math.max(xToSec(view, cx), 0), duration()));
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

  /** Primary glyph: loop the selection now — transient, nothing saved. */
  async function loopSelection() {
    const sel = get(selection);
    if (!sel) return;
    await actions.setTransportLoop(sel.start, sel.end);
    await actions.seek(sel.start);
    await actions.play();
  }

  /** Secondary glyph: save the selection to the loops list (server names it),
   *  then surface the loops tab. Does not change playback. */
  async function saveSelection() {
    const sel = get(selection);
    if (!sel) return;
    await actions.saveLoop(sel.start, sel.end);
    selection.set(null);
  }

  // Loop/save glyph buttons for the current selection. Placement is dynamic:
  // tucked inside the loop's bottom-right when it's wide enough to hold them,
  // otherwise just outside the right edge — or the left edge if the right would
  // spill past the viewport.
  const SA_BTN = 24; // glyph button size (px)
  const SA_GAP = 4; // gap between the two buttons
  const SA_PAD = 6; // breathing room from the loop edge / bottom
  const SA_GROUP_W = SA_BTN * 2 + SA_GAP;

  let selActions = $derived.by(() => {
    const sel = $selection;
    if (!sel) return null;
    const x0 = secToX(view, sel.start);
    const x1 = secToX(view, sel.end);
    const top = LANE_H + WAVE_H - SA_BTN - SA_PAD; // bottom of the loop, padded
    let left: number;
    if (x1 - x0 >= SA_GROUP_W + SA_PAD * 2) {
      // fits inside — right-aligned against the loop's right edge
      left = x1 - SA_PAD - SA_GROUP_W;
    } else if (x1 + SA_PAD + SA_GROUP_W <= view.width) {
      // too narrow — sit just outside the right edge
      left = x1 + SA_PAD;
    } else {
      // no room on the right either — fall to just outside the left edge
      left = x0 - SA_PAD - SA_GROUP_W;
    }
    return { left, top };
  });

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
    // right button is the ONLY resize: grab the nearest window edge from anywhere
    // (Hyprland super+right-drag feel) and drag it.
    if (e.button === 2) {
      const mode = Math.abs(px - x0) <= Math.abs(px - x1) ? "start" : "end";
      e.preventDefault();
      scrollEl.setPointerCapture(e.pointerId);
      scrollDrag = { mode, px0: px, s0: view.startSec, e0: view.endSec };
      return;
    }
    if (e.button !== 0) return;
    // left = pan the window (drag) or recenter (click outside) — never resizes.
    if (px >= x0 && px <= x1) {
      scrollEl.setPointerCapture(e.pointerId);
      scrollDrag = { mode: "pan", px0: px, s0: view.startSec, e0: view.endSec };
    } else {
      // click outside the window: recenter the window there (keep width)
      const width = view.endSec - view.startSec;
      const c = (px / w) * dur;
      const win = adjustWindow("pan", c - width / 2, c + width / 2, dur, MIN_WIN);
      view = { ...view, startSec: win.startSec, endSec: win.endSec };
    }
  }
  function onScrollMove(e: PointerEvent) {
    if (!scrollDrag) {
      // hover feedback: resize cursor over the window edges, grab over its body
      if (dur <= 0) return;
      const w = scrollEl.clientWidth;
      const px = scrollPx(e);
      const x0 = (view.startSec / dur) * w;
      const x1 = (view.endSec / dur) * w;
      const EDGE = 6;
      scrollEl.style.cursor =
        Math.abs(px - x0) <= EDGE || Math.abs(px - x1) <= EDGE
          ? "ew-resize"
          : px > x0 && px < x1
            ? "grab"
            : "default";
      return;
    }
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
    oncontextmenu={(e) => e.preventDefault()}
  ></canvas>
  {#if $openSong?.analysis}
    <div class="grid-ctl">
      <button class:on={$gridSnap} onclick={() => void actions.setGridSnap(!$gridSnap)} title="snap to grid (g)">snap</button>
      <button class:on={$gridVisible} onclick={() => void actions.setGridVisible(!$gridVisible)} title="show grid">grid</button>
      <button class:on={$gridLines} onclick={() => void actions.setGridLines(!$gridLines)} title="full gridlines vs bottom ticks">lines</button>
      <span class="seg">
        {#each GRID_SUBDIVS as s (s)}
          <button class:on={$gridSubdivision === s} onclick={() => void actions.setGridSubdivision(s)}>{s}</button>
        {/each}
      </span>
    </div>
  {/if}
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
    oncontextmenu={(e) => e.preventDefault()}
    title="left-drag to scroll · right-drag to zoom (grabs nearest edge) · double-click to fit"
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
  {#if $selection && selActions}
    <div class="sel-actions fade-in" style="left: {selActions.left}px; top: {selActions.top}px">
      <button class="sa-btn" onclick={loopSelection} title="loop selection" aria-label="loop selection">⟳</button>
      <button class="sa-btn" onclick={saveSelection} title="save loop" aria-label="save loop">🖫</button>
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

  /* grid control overlay — lower-right of the canvas, above the scrollbar
     (kept clear of the section lane along the top) */
  .grid-ctl {
    position: absolute;
    bottom: 20px;
    right: 4px;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px;
    background: color-mix(in srgb, var(--bg) 80%, transparent);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }

  .grid-ctl button {
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius);
    color: var(--muted);
    font-size: 10px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    padding: 1px 5px;
    cursor: pointer;
  }
  .grid-ctl button:hover {
    color: var(--fg);
  }
  .grid-ctl button.on {
    color: var(--accent);
    border-color: var(--accent-dim);
  }
  .grid-ctl .seg {
    display: inline-flex;
    gap: 2px;
    border-left: 1px solid var(--line);
    padding-left: 4px;
    margin-left: 1px;
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

  /* loop/play glyph buttons over the selection — see selActions for placement */
  .sel-actions {
    position: absolute;
    display: flex;
    gap: 4px;
  }

  .sa-btn {
    width: 24px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 13px;
    line-height: 1;
    color: var(--fg);
    background: color-mix(in srgb, var(--bg) 80%, transparent);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    cursor: pointer;
    padding: 0;
  }
  .sa-btn:hover {
    color: var(--accent);
    border-color: var(--accent-dim);
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
