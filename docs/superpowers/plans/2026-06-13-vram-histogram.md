# VRAM Histogram (live readout) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a VRAM-used histogram (y-axis = total VRAM) with a peak high-water line to the live work readout, so headroom is visible in real time.

**Architecture:** Frontend-only — every `work_sample` event already carries `gpu_mem_used_mb`/`gpu_mem_total_mb`. A new `vram` store accumulates the run's rolling series + peak inside `recordWorkSample`; `LiveProgress.svelte` renders a small inline SVG histogram with an amber peak line.

**Tech Stack:** Svelte 5, TypeScript, Vitest, svelte-check. Frontend under `apps/desktop` (pnpm).

**Depends on:** the live-readout feature (through `9635d00`): the `work_sample` event, `workSample` store, `recordWorkSample` action, and `LiveProgress.svelte`.

**Scope:** Two tasks, both frontend. Task 1 (store) is Vitest-tested; Task 2 (component) is verified via svelte-check + build.

---

## File Structure

- `apps/desktop/src/lib/stores.ts` — add the `vram` store, accumulate it in `recordWorkSample`, clear it at the three run-end spots.
- `apps/desktop/src/lib/livesample.test.ts` — extend with `vram` accumulation tests.
- `apps/desktop/src/components/LiveProgress.svelte` — add the `vram` histogram meter row; trim the redundant GB text off the `gpu` row.

---

## Task 1: `vram` store + accumulation

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts`
- Modify: `apps/desktop/src/lib/livesample.test.ts`

- [ ] **Step 1: Add the failing tests**

In `apps/desktop/src/lib/livesample.test.ts`, add `vram` to the import from `./stores`:
```ts
import { actions, workSample, vram, type WorkSample } from "./stores";
```
Add `vram.set(null);` to the existing `beforeEach`. Then append:
```ts
describe("vram accumulation", () => {
  const s = (used: number, total = 16000): WorkSample => ({
    op: "analysis", stage: "GPU attempt", elapsed_ms: 1000, cpu_pct: 100,
    gpu_util: 40, gpu_mem_used_mb: used, gpu_mem_total_mb: total,
  });

  it("accumulates used, total, and peak", () => {
    actions.recordWorkSample(s(4000));
    actions.recordWorkSample(s(6000));
    actions.recordWorkSample(s(5000));
    const v = get(vram)!;
    expect(v.used).toEqual([4000, 6000, 5000]);
    expect(v.total).toBe(16000);
    expect(v.peak).toBe(6000);
  });

  it("keeps only the last 60 samples but peak persists after the window slides", () => {
    for (let i = 1; i <= 70; i++) actions.recordWorkSample(s(i * 100)); // 100..7000
    const v = get(vram)!;
    expect(v.used).toHaveLength(60);
    expect(v.used[0]).toBe(1100); // sample #11 (first 10 slid off)
    expect(v.used[v.used.length - 1]).toBe(7000);
    expect(v.peak).toBe(7000); // high-water mark survives the slide
  });

  it("ignores samples with no GPU memory", () => {
    const noGpu: WorkSample = { op: "analysis", stage: "x", elapsed_ms: 1, cpu_pct: 1 };
    actions.recordWorkSample(noGpu);
    expect(get(vram)).toBeNull();
  });
});
```

- [ ] **Step 2: Confirm it fails**

Run: `cd apps/desktop && pnpm vitest run lib/livesample.test.ts`
Expected: FAIL — `vram` not exported.

- [ ] **Step 3: Add the `vram` store**

In `stores.ts`, immediately after the `workSample` writable (around line 249):
```ts
/** VRAM series for the active run: used-MB samples (rolling last 60), the run's
 *  peak used-MB (high-water mark), and total VRAM. Null when idle / no GPU. */
export const vram = writable<{ used: number[]; peak: number; total: number } | null>(null);
```

- [ ] **Step 4: Accumulate in `recordWorkSample`**

Replace the existing `recordWorkSample` action:
```ts
  recordWorkSample(sample: WorkSample): void {
    workSample.set(sample);
  },
```
with:
```ts
  recordWorkSample(sample: WorkSample): void {
    workSample.set(sample);
    if (sample.gpu_mem_used_mb != null && sample.gpu_mem_total_mb != null) {
      const used = sample.gpu_mem_used_mb;
      const total = sample.gpu_mem_total_mb;
      vram.update((v) => ({
        used: [...(v?.used ?? []), used].slice(-60),
        peak: Math.max(v?.peak ?? 0, used),
        total,
      }));
    }
  },
```

- [ ] **Step 5: Clear `vram` at the three run-end spots**

`vram` must reset wherever `workSample` does. Add `vram.set(null);` at each:
1. Around line 706 (start of `prepare()`), next to the existing `workSample.set(null);`:
   ```ts
    workSample.set(null);
    vram.set(null);
   ```
2. Around line 754 (the success linger) — change
   ```ts
      setTimeout(() => { prepareState.set(null); workSample.set(null); }, 1500);
   ```
   to
   ```ts
      setTimeout(() => { prepareState.set(null); workSample.set(null); vram.set(null); }, 1500);
   ```
3. Around line 761 (`closePrepare()`), next to its `workSample.set(null);`:
   ```ts
    workSample.set(null);
    vram.set(null);
   ```
(Read the file to confirm exact line numbers; each is paired with an existing `workSample.set(null)`.)

- [ ] **Step 6: Confirm pass + type-check**

Run: `cd apps/desktop && pnpm vitest run lib/livesample.test.ts`
Expected: PASS (the new `vram accumulation` describe + the existing tests).
Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: no new errors.

- [ ] **Step 7: Commit**

```bash
git add apps/desktop/src/lib/stores.ts apps/desktop/src/lib/livesample.test.ts
git commit -m "feat(desktop): vram store — rolling used series + peak high-water mark"
```

---

## Task 2: VRAM histogram row in LiveProgress

**Files:**
- Modify: `apps/desktop/src/components/LiveProgress.svelte`

- [ ] **Step 1: Import `vram`**

In the `<script>`, add `vram` to the import from `../lib/stores`:
```ts
  import {
    prepareState,
    workSample,
    vram,
    profiles,
    type PrepareStepState,
  } from "../lib/stores";
```

- [ ] **Step 2: Trim the redundant GB text off the `gpu` row**

The `gpu` meter currently appends the GB figure; the new `vram` row will own it. Change the gpu `mval` span (the line reading
`<span class="mval mono">{$workSample.gpu_util}%{#if $workSample.gpu_mem_total_mb} · ...GB{/if}</span>`)
to just:
```svelte
              <span class="mval mono">{$workSample.gpu_util}%</span>
```

- [ ] **Step 3: Add the `vram` histogram meter row**

Inside the `.meters` block, AFTER the closing `{/if}` of the gpu meter (the `{#if $workSample.gpu_util != null}...{/if}`), add:
```svelte
          {#if $vram && $vram.used.length}
            <div class="meter">
              <span class="mlabel mono">vram</span>
              <span class="hist">
                <svg viewBox="0 0 60 100" preserveAspectRatio="none">
                  {#each $vram.used as u, i (i)}
                    <rect x={i} y={100 - (u / $vram.total) * 100} width="1" height={(u / $vram.total) * 100} />
                  {/each}
                  <line
                    x1="0"
                    x2="60"
                    y1={100 - ($vram.peak / $vram.total) * 100}
                    y2={100 - ($vram.peak / $vram.total) * 100}
                    class="peak"
                  />
                </svg>
              </span>
              <span class="mval mono">{($vram.used[$vram.used.length - 1] / 1024).toFixed(1)} / {Math.round($vram.total / 1024)} GB</span>
            </div>
          {/if}
```

- [ ] **Step 4: Add styles**

In the `<style>` block, after the existing `.bar`/`.fill`/`.mval` rules, add:
```css
  .hist { flex: 1; height: 24px; max-width: 220px; background: var(--bg-raised); border-radius: 2px; overflow: hidden; }
  .hist svg { width: 100%; height: 100%; display: block; }
  .hist rect { fill: var(--accent); }
  .hist line.peak { stroke: var(--shaky); stroke-width: 1; vector-effect: non-scaling-stroke; }
```
(`vector-effect: non-scaling-stroke` keeps the peak line 1 px despite the `preserveAspectRatio="none"` viewBox stretch. `--accent`, `--shaky`, `--bg-raised` already exist.)

- [ ] **Step 5: Type-check + build**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 errors.
Run: `cd apps/desktop && pnpm build`
Expected: clean.
Run: `cd apps/desktop && pnpm vitest run`
Expected: all pass.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/components/LiveProgress.svelte
git commit -m "feat(desktop): VRAM histogram with peak line in the live readout"
```

---

## Manual verification (after Task 2)

- [ ] `just build` then `just run`.
- [ ] RE-PREPARE a song while a GPU is present. Below the cpu/gpu bars, a `vram` row shows a histogram filling left→right as samples arrive (y-axis = total VRAM, so a near-full bar means VRAM is nearly full), an amber horizontal **peak line** at the high-water mark, and a `5.1 / 16 GB` label. With no GPU (`nvidia-smi` absent) the vram row simply doesn't appear.

---

## Self-review checklist (done while writing)

- **Spec coverage:** `vram` store with rolling-60 used + run-peak + total (T1) · accumulation in `recordWorkSample` ignoring null-GPU samples (T1) · cleared at the three run-end spots (T1) · SVG histogram y-axis = total VRAM (T2) · amber peak line at run max (T2) · GB label (T2) · cpu/gpu bars kept (gpu GB text moved to the vram row) (T2) · Vitest for accumulation/cap/peak/null-skip (T1). All covered.
- **Placeholder scan:** none — full code each step; the three clear-spot line numbers are flagged "confirm by reading."
- **Type consistency:** `vram` store shape `{ used: number[]; peak: number; total: number } | null` used identically in T1 (store + action + tests) and T2 (`$vram.used`, `$vram.peak`, `$vram.total`); `recordWorkSample` signature unchanged.
