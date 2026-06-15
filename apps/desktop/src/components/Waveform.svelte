<script lang="ts">
  import { onMount } from "svelte";
  import { fade } from "svelte/transition";
  import { get } from "svelte/store";
  import { canvasSize } from "../lib/actions/canvasSize";
  import {
    actions,
    currentLoop,
    drillSpan,
    gridLines,
    gridSnap,
    gridSubdivision,
    gridVisible,
    libraryCollapsed,
    openingSong,
    openSong,
    position,
    selection,
    workspaceReset,
    type LoopRegion,
    type OpenSong,
  } from "../lib/stores";
  import HoverActions from "../lib/ui/HoverActions.svelte";
  import {
    hitLaneSpan as hitLane,
    hitLoopBody as hitBody,
    hitLoopEdge as hitEdge,
    laneSpans,
    nearestLoopEdge as nearestEdge,
    spanAtTime as spanAt,
    type LaneSpan,
    type LoopEdge,
  } from "../lib/waveform-hit";
  import { hexToHue, labelColor } from "../lib/waveform-colors";
  import {
    adjustWindow,
    followView,
    makePlayheadClock,
    secToX,
    snapToGrid,
    subdivisionTimes,
    tickPlayhead,
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
  const DBLCLICK_MS = 300; // two lane clicks within this = double-click

  let canvas: HTMLCanvasElement;
  let view: View = $state({ startSec: 0, endSec: 1, width: 1 });
  let lastSongId: number | null = null;
  /** Lane span whose bounds currently drive the transport loop (clicked). */
  let activeSpan: { start: number; end: number } | null = $state(null);

  // "lock viewport to playhead": while playing, scroll the window so the
  // playhead stays within the centre dead-zone band. Ephemeral local state —
  // off by default, reset on new song / workspace reset. A manual pan turns it
  // off (the pointer handlers below); zoom stays follow-aware.
  const FOLLOW_MARGIN = 0.2; // free-roam middle 60%, scroll past the edge 20%
  let follow = $state(false);
  // last playhead drawn — used as the zoom anchor while following so a zoom
  // can't shove the playhead out of view (recomputing the smoothed clock here
  // would perturb its interpolation, so reuse what draw() last produced).
  let lastPlayhead = 0;

  // pointer interaction state
  type Drag =
    | { mode: "select"; anchorX: number; moved: boolean }
    | { mode: "resize"; loop: LoopRegion; edge: "start" | "end"; start: number; end: number }
    | { mode: "lane"; anchor: { start: number; end: number }; moved: boolean; double: boolean }
    | { mode: "zoom"; anchorX: number; curX: number };
  let drag: Drag | null = null;
  // timestamp of the last lane-header pointerdown — for double-click detection
  let lastLaneDownAt = 0;
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
      follow = false;
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
      follow = false;
      const open = get(openSong);
      if (open) fitToSong(open);
    }
  });

  function duration(): number {
    return get(openSong)?.song.duration_secs ?? 0;
  }

  // laneSpans is a pure function of the open song; memoize on the song object's
  // identity so the per-redraw draw and the per-pointer-move hit-tests don't
  // rebuild the array every call.
  let laneSpansCache: { open: OpenSong; spans: LaneSpan[] } | null = null;
  function spansFor(open: OpenSong): LaneSpan[] {
    if (laneSpansCache?.open !== open) {
      laneSpansCache = { open, spans: laneSpans(open) };
    }
    return laneSpansCache.spans;
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

  // Demand-driven redraw: instead of an unconditional 60fps RAF (which repaints
  // an identical canvas while paused/idle), schedule a single frame on demand
  // and keep animating ONLY while playing (so the interpolated playhead stays
  // smooth). Everything draw() reads reactively is wired to requestRedraw via
  // the $effect below; the non-reactive `drag`/`pendingResize` are nudged from
  // the pointer handlers.
  let rafPending = false;
  let rafId = 0;
  const playClock = makePlayheadClock();
  function paint() {
    rafPending = false;
    draw();
    if (get(position).playing) {
      rafPending = true;
      rafId = requestAnimationFrame(paint);
    }
  }
  function requestRedraw() {
    if (rafPending) return;
    rafPending = true;
    rafId = requestAnimationFrame(paint);
  }

  // Repaint whenever any reactive input to draw() changes. While playing,
  // $position ticks keep arriving but the paint loop is already self-sustaining,
  // so requestRedraw is a no-op; while paused, position is steady (server gates
  // unchanged positions) so this settles to zero repaints.
  $effect(() => {
    void $openSong;
    void $position;
    void $selection;
    void $currentLoop;
    void $drillSpan;
    void $gridVisible;
    void $gridLines;
    void $gridSubdivision;
    void view;
    void activeSpan;
    requestRedraw();
  });

  function draw() {
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const w = view.width;
    const open = get(openSong);

    // theme colors, read once per frame from a single getComputedStyle (each
    // css() call is its own getComputedStyle, and some reads sit inside
    // per-beat / per-section loops — keep it to one computed-style lookup).
    const cs = getComputedStyle(document.documentElement);
    const v = (name: string) => cs.getPropertyValue(name).trim();
    const c = {
      bg: v("--bg"),
      wave: v("--wave"),
      muted: v("--muted"),
      line: v("--line"),
      accent: v("--accent"),
      accentDim: v("--accent-dim"),
      fg: v("--fg"),
      mono: v("--mono"),
    };

    ctx.fillStyle = c.bg;
    ctx.fillRect(0, 0, w, LANE_H + WAVE_H);

    if (!open) return; // empty state is the .wave-empty HTML overlay

    const playhead = tickPlayhead(playClock, get(position), performance.now());
    lastPlayhead = playhead;
    // lock viewport to playhead: shift the window before everything else reads
    // `view` this frame, so the wave, lane and playhead all draw against the
    // scrolled window. followView returns the same ref when no shift is needed.
    if (follow && get(position).playing) {
      const next = followView(view, playhead, duration(), FOLLOW_MARGIN);
      if (next !== view) view = next;
    }
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
    const wave = c.wave;
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
          ctx.fillStyle = strong ? c.muted : c.line;
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
    const accent = c.accent;
    const accentDim = c.accentDim;
    const drill = get(drillSpan);
    for (const l of open.loops) {
      const saved = loopBounds(l);
      const isSel = get(currentLoop)?.id === l.id;
      // while a loop is active the bold highlight tracks the scratch span (so
      // isolate / run-up are visible); the saved bounds show as a faint ghost
      // when the two diverge, marking where "reset span" returns to.
      const { start, end } = isSel && drill ? drill : saved;
      const diverged = !!(isSel && drill && (drill.start !== saved.start || drill.end !== saved.end));
      const x0 = secToX(view, start);
      const x1 = secToX(view, end);
      if (x1 < 0 || x0 > w) continue;
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
      // ghost the saved bounds when the drill span has diverged from them
      if (diverged) {
        const gx0 = secToX(view, saved.start);
        const gx1 = secToX(view, saved.end);
        ctx.strokeStyle = accentDim;
        ctx.lineWidth = 1;
        ctx.setLineDash([2, 4]);
        ctx.strokeRect(gx0 + 0.5, LANE_H + 0.5, gx1 - gx0 - 1, WAVE_H - 1);
        ctx.setLineDash([]);
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
      ctx.strokeStyle = c.fg;
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

    // playhead — 1 px accent line, full height. Drawn before the structure lane
    // so the section headings paint over it in the lane.
    if (playheadX >= 0 && playheadX <= w) {
      ctx.fillStyle = accent;
      ctx.fillRect(Math.round(playheadX), 0, 1, LANE_H + WAVE_H);
    }

    // structure lane — label-colored spans: saved sections solid, analysis
    // suggestions dashed/dimmer/italic; clicked span gets a second fill pass.
    // Hues are derived from the live accent so the lane re-tints with the theme.
    const baseHue = hexToHue(c.accent);
    for (const s of spansFor(open)) {
      const x0 = secToX(view, s.start);
      const x1 = secToX(view, s.end);
      if (x1 < 0 || x0 > w) continue;
      const { fill, edge } = labelColor(s.name, baseHue);
      const active = activeSpan?.start === s.start && activeSpan?.end === s.end;
      // translucent backing so the box dims (but doesn't fully hide) the
      // playhead passing behind it; the tint below is only ~16% opaque.
      ctx.globalAlpha = 0.8;
      ctx.fillStyle = c.bg;
      ctx.fillRect(x0, 2, x1 - x0 - 1, LANE_H - 4);
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
      ctx.fillStyle = c.fg;
      ctx.font = (s.suggested ? "italic " : "") + "11px " + c.mono;
      // sticky label: pin to the visible left edge while any of the span is on
      // screen (but never past its right edge), and truncate against what's left
      const lpad = 4;
      const lx = Math.min(Math.max(x0 + lpad, lpad), x1 - lpad);
      ctx.fillText(s.name, lx, LANE_H - 8, Math.max(x1 - lx - lpad, 0));
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
    requestRedraw(); // initial paint; subsequent ones are demand-driven
    return () => cancelAnimationFrame(rafId);
  });

  // store-reading wrappers around the pure hit-testers in lib/waveform-hit.ts
  /** Topmost loop whose body is under a canvas point (below the lane). */
  function hitLoopBody(x: number, y: number): LoopRegion | null {
    const open = get(openSong);
    return open ? hitBody(open.loops, view, x, y, LANE_H) : null;
  }

  function hitLoopEdge(x: number): LoopEdge | null {
    const open = get(openSong);
    return open ? hitEdge(open.loops, view, x, EDGE_PX) : null;
  }

  /** The loop edge (across all loops) nearest to canvas x. Right-drag grabs this
   *  from anywhere — like Hyprland's super+right-drag snapping to the nearest
   *  tile border instead of requiring a pixel-perfect hit. */
  function nearestLoopEdge(x: number): LoopEdge | null {
    const open = get(openSong);
    return open ? nearestEdge(open.loops, view, x) : null;
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
    return open ? spanAt(spansFor(open), sec) : null;
  }

  /** Structure-lane span under a canvas point (lane y-band only). */
  function hitLaneSpan(x: number, y: number): LaneSpan | null {
    const open = get(openSong);
    return open ? hitLane(spansFor(open), view, x, y, LANE_H) : null;
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
    // lane click/drag: single click plays the section through; double click (or
    // double-click-drag) loops it — resolved on pointer-up using `double`.
    const span = hitLaneSpan(x, canvasY(e));
    if (span) {
      const double = e.timeStamp - lastLaneDownAt < DBLCLICK_MS;
      lastLaneDownAt = e.timeStamp;
      canvas.setPointerCapture(e.pointerId);
      drag = { mode: "lane", anchor: { start: span.start, end: span.end }, moved: false, double };
      return;
    }
    lastLaneDownAt = 0; // a non-lane click breaks any double-click sequence
    canvas.setPointerCapture(e.pointerId);
    drag = { mode: "select", anchorX: x, moved: false };
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
      requestRedraw(); // zoom preview box lives in non-reactive `drag`
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
      requestRedraw(); // resize bounds live in non-reactive `drag`
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
      if (d.double) {
        // double click (± drag across headers) → SELECT the section / span. No
        // loop is created here; the ⟳ button on the selection does that.
        if (!d.moved) {
          selection.set({ start: d.anchor.start, end: d.anchor.end });
          activeSpan = { start: d.anchor.start, end: d.anchor.end };
        } else {
          activeSpan = null; // multi-section: the selection box shows the range
        }
      } else if (!d.moved) {
        // single click → move the playhead to the section start (no auto-play)
        void actions.seek(d.anchor.start);
      }
      // single-click drag (!double && moved): selection was set during the drag;
      // leave it for the user to loop/save by hand — no auto-loop.
      return;
    }
    if (d.mode === "resize") {
      if (d.start !== d.loop.start || d.end !== d.loop.end) {
        // pin the new bounds visually until the store reflects them, then release
        pendingResize = { id: d.loop.id, start: d.start, end: d.end };
        requestRedraw(); // pinned bounds live in non-reactive `pendingResize`
        const id = d.loop.id;
        void actions.updateLoop(id, { start: d.start, end: d.end }).finally(() => {
          if (pendingResize?.id === id) pendingResize = null;
          requestRedraw(); // repaint once the store round-trip releases the pin
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
      // pan — same clamp as the scrollbar, via the shared window helper. A
      // manual pan means the user is taking control → drop follow.
      follow = false;
      const span = view.endSec - view.startSec;
      const shift = (e.deltaY / view.width) * span;
      const win = adjustWindow("pan", view.startSec + shift, view.endSec + shift, duration(), MIN_WIN);
      view = { ...view, ...win };
    } else {
      // zoom stays follow-aware: anchor on the playhead while following so the
      // zoom can't push it out of the window (else anchor on the cursor).
      const factor = e.deltaY > 0 ? 1.25 : 0.8;
      const anchor = follow ? lastPlayhead : xToSec(view, canvasX(e));
      view = zoom(view, anchor, factor, duration());
    }
  }

  /** Loop the selection: save it as a loop (or reuse a matching one), make it the
   *  active loop, and play — which opens the drill box on it. */
  async function loopSelection() {
    const sel = get(selection);
    if (!sel) return;
    selection.set(null);
    await actions.saveAndSelectLoop(sel.start, sel.end);
  }

  /** Loop glyph on the selected loop: point the transport at it and play. */
  async function playCurrentLoop() {
    const l = get(currentLoop);
    if (!l) return;
    const b = get(drillSpan) ?? loopBounds(l);
    await actions.setTransportLoop(b.start, b.end);
    await actions.seek(b.start);
    await actions.play();
  }

  /** ✕ glyph on the selected loop: delete it (clears the transport loop too). */
  async function deleteCurrentLoop() {
    const l = get(currentLoop);
    if (l) await actions.deleteLoop(l.id);
  }

  /** Empty-state "library" link: reveal the library pane if it's collapsed. */
  function openLibrary() {
    if (get(libraryCollapsed)) void actions.toggleLibrary();
  }

  // Cursor position in waveform px (or null off-canvas) — drives the hover-reveal
  // action clusters for the selection and the selected loop.
  let hoverPt = $state<{ x: number; y: number } | null>(null);

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
    follow = false; // any scrollbar pan/resize/recenter is a manual override
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

<!-- pointer tracked at the container (not the canvas) so moving onto the overlay
     action buttons — children of .waveform — never reads as leaving, which would
     flicker the hover clusters at the canvas/button seam -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="waveform"
  use:canvasSize={applySize}
  onpointermove={(e) => (hoverPt = { x: canvasX(e), y: canvasY(e) })}
  onpointerleave={() => (hoverPt = null)}
>
  <canvas
    id="waveform-canvas"
    bind:this={canvas}
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onwheel={onWheel}
    oncontextmenu={(e) => e.preventDefault()}
  ></canvas>
  {#if !$openSong}
    <div class="wave-empty" style="top: {LANE_H}px; height: {WAVE_H}px;">
      {#if $openingSong !== null}
        <span class="we-title">opening…</span>
      {:else}
        <svg
          class="we-glyph"
          width="40"
          height="26"
          viewBox="0 0 40 26"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
        >
          <line x1="4" y1="11" x2="4" y2="15" />
          <line x1="9" y1="8" x2="9" y2="18" />
          <line x1="14" y1="3" x2="14" y2="23" />
          <line x1="19" y1="9" x2="19" y2="17" />
          <line x1="24" y1="5" x2="24" y2="21" />
          <line x1="29" y1="10" x2="29" y2="16" />
          <line x1="34" y1="7" x2="34" y2="19" />
        </svg>
        <span class="we-title">no song open</span>
        <span class="we-hint">
          pick a track in the <button class="we-link" onclick={openLibrary}>library</button>
        </span>
      {/if}
    </div>
  {/if}
  {#if $openSong?.analysis && hoverPt}
    <div class="grid-ctl" transition:fade={{ duration: 120 }}>
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
  {#if $openSong && (hoverPt || follow)}
    <!-- lock viewport to playhead: hover-revealed, stays lit (cyan) while on -->
    <button
      class="follow-toggle"
      class:on={follow}
      style="top: {LANE_H + 4}px;"
      onclick={() => (follow = !follow)}
      title={follow ? "following playhead — click to unlock" : "lock viewport to playhead"}
      aria-label="lock viewport to playhead"
      aria-pressed={follow}
      transition:fade={{ duration: 120 }}
    >
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1" stroke-linecap="round" aria-hidden="true">
        <circle cx="12" cy="12" r="5.5" />
        <line x1="12" y1="2.5" x2="12" y2="21.5" />
      </svg>
    </button>
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
  {#if $selection}
    <HoverActions
      left={secToX(view, $selection.start)}
      right={secToX(view, $selection.end)}
      bandTop={LANE_H}
      bandHeight={WAVE_H}
      viewWidth={view.width}
      pointer={hoverPt}
      count={1}
    >
      <button class="sa-btn" onclick={loopSelection} title="loop — saves it & opens the drill" aria-label="loop selection">⟳</button>
    </HoverActions>
  {/if}
  {#if $currentLoop}
    <HoverActions
      left={secToX(view, ($drillSpan ?? loopBounds($currentLoop)).start)}
      right={secToX(view, ($drillSpan ?? loopBounds($currentLoop)).end)}
      bandTop={LANE_H}
      bandHeight={WAVE_H}
      viewWidth={view.width}
      pointer={hoverPt}
      count={2}
    >
      <button class="sa-btn" onclick={playCurrentLoop} title="play loop" aria-label="play loop">⟳</button>
      <button class="sa-btn danger" onclick={deleteCurrentLoop} title="delete loop" aria-label="delete loop">✕</button>
    </HoverActions>
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

  .follow-toggle {
    position: absolute;
    right: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 3px;
    line-height: 0;
    background: color-mix(in srgb, var(--bg) 80%, transparent);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    color: var(--muted);
    cursor: pointer;
  }
  .follow-toggle svg {
    width: 16px;
    height: 16px;
  }
  .follow-toggle:hover {
    color: var(--fg);
  }
  .follow-toggle.on {
    color: var(--accent);
    border-color: var(--accent-dim);
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

  /* glyph buttons rendered inside HoverActions (selection + selected loop) */
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
  }
  .sa-btn.danger:hover {
    color: var(--shaky);
  }

  /* empty state, centered over the wave area (canvas draws nothing when no song) */
  .wave-empty {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    text-align: center;
    /* let pointer tracking / interactions fall through to the canvas; only the
       link opts back in */
    pointer-events: none;
  }
  .we-glyph {
    color: var(--muted);
    opacity: 0.5;
  }
  .we-title {
    color: var(--muted);
    font-size: 14px;
    letter-spacing: 0.04em;
  }
  .we-hint {
    color: var(--muted);
    opacity: 0.75;
    font-size: 11px;
  }
  .we-link {
    pointer-events: auto;
    background: none;
    border: none;
    font: inherit;
    color: var(--accent-dim);
    cursor: pointer;
    padding: 0;
    text-decoration: underline;
  }
  .we-link:hover {
    color: var(--accent);
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
