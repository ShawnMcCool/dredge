# VRAM histogram with peak line in the live readout

Date: 2026-06-13

## Problem

The live work readout (`LiveProgress.svelte`) shows GPU **utilization** as a bar and
VRAM as a text figure (`5.1 / 16 GB`). But the number that actually decides
GPU-vs-CPU is **VRAM headroom**: SongFormer's ~6 GB peak failing to fit triggers
the slow CPU recovery. A single instantaneous figure doesn't show the *trend* or
how close a run came to filling VRAM.

## Goal

Add a small **VRAM histogram** to the live readout, y-axis scaled to **total
VRAM**, showing the recent series plus a **peak high-water line** — so you can
watch headroom in real time and see how close the run got to the OOM cliff.

## Non-goals

- Backend changes — every `work_sample` already carries `gpu_mem_used_mb` and
  `gpu_mem_total_mb`. This is frontend-only.
- Historical VRAM across runs, or a VRAM chart in the Profile panel.
- Charting CPU or utilization as histograms — they stay as bars.

## Design (frontend only)

### Data (`apps/desktop/src/lib/stores.ts`)

A new store accumulates the current run's series:
```ts
export const vram = writable<{ used: number[]; peak: number; total: number } | null>(null);
```

Inside the existing `recordWorkSample(sample)` action, after `workSample.set(sample)`,
when both `sample.gpu_mem_used_mb` and `sample.gpu_mem_total_mb` are present:
- append `gpu_mem_used_mb` to `used`, keep the **last 60** entries (rolling window,
  ~45 s at one sample / 750 ms),
- `peak = max(prev peak, gpu_mem_used_mb)` — the run's overall high-water mark, so
  it persists even after early samples scroll out of the window,
- `total = gpu_mem_total_mb`.

`vram` is cleared (`set(null)`) at the **same three places** `workSample` is
already cleared: at the start of `prepare()`, inside the 1.5 s success-linger
`setTimeout`, and in `closePrepare()`. So it resets per run and disappears when
idle.

### View (`apps/desktop/src/components/LiveProgress.svelte`)

Keep the existing `cpu` and `gpu` (utilization) bars unchanged. Add a third
`vram` meter row, rendered only while a run is active and `$vram` is non-null:
- A small inline **SVG** (container ~220 × 28 px, `preserveAspectRatio="none"`):
  - `viewBox="0 0 60 100"`. One `<rect>` per `used` sample, `width=1`, anchored at
    the bottom, `height = used / total * 100`, `x = index`. Accent fill. (Fewer
    than 60 samples simply leaves the left side empty — bars grow in from x=0.)
  - A horizontal **peak line** (`<line>`, color `--shaky`/amber) at
    `y = 100 - peak / total * 100`, spanning x=0..60.
- A text label: latest used / total in GB, e.g. `5.1 / 16 GB`
  (`(used_mb/1024).toFixed(1)` / `round(total_mb/1024)`).

Layout mirrors the existing `.meter` rows (`mlabel` "vram" + the SVG in place of
the bar + the GB label), so it sits directly under the cpu/gpu bars.

### Testing

- **Vitest** (`apps/desktop/src/lib/livesample.test.ts`, extend it): feeding
  samples through `recordWorkSample` accumulates `vram.used` (rolling, capped at
  60), tracks `peak` as the running max (including after the window slides), sets
  `total`, and ignores samples with null GPU memory; `vram` is null after a
  clear.
- **`LiveProgress.svelte`** verified via `svelte-check` + `pnpm build` (no
  render tests in this repo).

## Open questions (resolved)

- **Peak scope:** run overall max, not just within the visible window.
  *Resolved.*
- **Utilization bar:** kept; the histogram is added alongside, not a
  replacement. *Resolved.*
- **Window size:** last 60 samples (~45 s). *Resolved.*
