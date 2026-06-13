# Operation Profiling — Frontend Implementation Plan (Plan B)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface the profiling backend in the desktop UI — an analysis-device setting toggle, an engine caption on the Sections panel (no more silent novelty fallback), a Profiling panel with run history, and a per-step "last run" line in the prepare modal.

**Architecture:** All UI state derives from dispatch responses + events (`lib/stores.ts` is the only state). A `profiles` writable mirrors the `profile_run` event stream and a `profiles.list` fetch at launch. Components are thin Svelte 5 views over the stores, following the existing `DuePanel`/`SettingsModal` patterns.

**Tech Stack:** Svelte 5 (runes), TypeScript, Tauri IPC (`lib/ipc.ts`), Vitest (pure-logic tests in `lib/*.test.ts`), `svelte-check` for component/type verification. Frontend uses **pnpm** under `apps/desktop`.

**Depends on:** the profiling backend (commits through `78f1b82`): the `profile_run` event, `profiles.list` command, and the `analysis_device` setting all exist server-side.

**Verification note:** This project unit-tests **pure store logic** in `lib/*.test.ts` (see `library.test.ts`) but does NOT render-test `.svelte` components. So store changes (Task 1) are TDD via Vitest; component tasks (2–5) are verified with `svelte-check` + build. All Vitest commands run from `apps/desktop`.

---

## File Structure

- `apps/desktop/src/lib/stores.ts` — `ProfileRun`/`ProfileStage` types, `ANALYSIS_DEVICE` const, `profiles` store, `profile_run` event case, `loadProfiles` action, launch fetch (Task 1).
- `apps/desktop/src/lib/profiles.test.ts` — Vitest for the store slice (Task 1).
- `apps/desktop/src/components/SettingsModal.svelte` — device toggle row (Task 2).
- `apps/desktop/src/components/Sections.svelte` — engine caption (Task 3).
- `apps/desktop/src/components/ProfilingPanel.svelte` — new panel (Task 4).
- `apps/desktop/src/App.svelte` — register the `profile` tab (Task 4).
- `apps/desktop/src/components/PrepareModal.svelte` — per-step last-run line (Task 5).

---

## Task 1: profiles store slice, types, event, and launch fetch

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts`
- Create: `apps/desktop/src/lib/profiles.test.ts`

- [ ] **Step 1: Write the failing test**

Create `apps/desktop/src/lib/profiles.test.ts` (mirrors `library.test.ts`'s `ipc` mock):

```ts
import { beforeEach, describe, expect, it, vi } from "vitest";
import { get } from "svelte/store";

const cmdMock = vi.fn();
vi.mock("./ipc", () => ({
  cmd: (...args: unknown[]) => cmdMock(...args),
  onEvent: () => () => {},
  initialSong: () => null,
}));

import { actions, profiles, type ProfileRun } from "./stores";

beforeEach(() => {
  cmdMock.mockReset();
  cmdMock.mockResolvedValue([]);
  profiles.set([]);
});

const run = (op: string): ProfileRun => ({
  op,
  total_ms: 1500,
  ok: true,
  device: "cpu",
  engine: "songformer",
  started_at: "2026-06-13 10:00:00",
  stages: [{ name: "analyze", ms: 1500 }],
});

describe("loadProfiles", () => {
  it("fetches profiles.list and fills the store", async () => {
    cmdMock.mockResolvedValue([run("analysis")]);
    await actions.loadProfiles();
    expect(cmdMock).toHaveBeenCalledWith("profiles.list", { limit: 50 });
    expect(get(profiles)).toHaveLength(1);
    expect(get(profiles)[0].op).toBe("analysis");
  });
});

describe("recordProfile", () => {
  it("prepends a run (most-recent-first)", () => {
    profiles.set([run("stems")]);
    actions.recordProfile(run("analysis"));
    const list = get(profiles);
    expect(list).toHaveLength(2);
    expect(list[0].op).toBe("analysis");
    expect(list[1].op).toBe("stems");
  });
});
```

- [ ] **Step 2: Run it to confirm it fails**

Run: `cd apps/desktop && pnpm vitest run lib/profiles.test.ts`
Expected: FAIL — `profiles`/`actions.loadProfiles`/`actions.recordProfile` not exported.

- [ ] **Step 3: Add the types + store + const**

In `apps/desktop/src/lib/stores.ts`, near the `Analysis` interface (around line 89), add:

```ts
export interface ProfileStage {
  name: string;
  ms: number;
  note?: string;
}

export interface ProfileRun {
  op: string;
  song_id?: number;
  started_at: string;
  total_ms: number;
  ok: boolean;
  error?: string;
  device?: string;
  engine?: string;
  stages: ProfileStage[];
}
```

In the durable-settings block (near `PLAYBACK_VOLUME`), add the key:

```ts
export const ANALYSIS_DEVICE = "analysis_device";
```

Near the other top-level `writable`s (e.g. after `settings`), add the store:

```ts
/** Recent profiling runs, most-recent-first. Mirrors `profile_run` events
 *  plus a `profiles.list` fetch at launch. */
export const profiles = writable<ProfileRun[]>([]);
```

- [ ] **Step 4: Add the actions**

In the `actions` object (near `loadSettings`), add:

```ts
  /** Pull recent profiling runs (most-recent-first) at launch. */
  async loadProfiles(): Promise<void> {
    profiles.set(await cmd<ProfileRun[]>("profiles.list", { limit: 50 }));
  },

  /** Prepend a freshly finished run (from a `profile_run` event). */
  recordProfile(run: ProfileRun): void {
    profiles.update((list) => [run, ...list].slice(0, 100));
  },
```

- [ ] **Step 5: Handle the `profile_run` event**

In the `switch (ev.event)` inside `initEvents()` (around line 747), add a case alongside `analysis_progress`:

```ts
      case "profile_run":
        actions.recordProfile(ev.data as ProfileRun);
        break;
```

- [ ] **Step 6: Fetch at launch**

In `actions.loadSettings()`, after `settings.set(all);`, add a fire-and-forget fetch so the panel is populated on open:

```ts
    void actions.loadProfiles();
```

- [ ] **Step 7: Run the test to confirm it passes**

Run: `cd apps/desktop && pnpm vitest run lib/profiles.test.ts`
Expected: PASS (both describes).

- [ ] **Step 8: Type-check**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 errors (warnings about unrelated pre-existing code are fine; no NEW errors from these changes).

- [ ] **Step 9: Commit**

```bash
git add apps/desktop/src/lib/stores.ts apps/desktop/src/lib/profiles.test.ts
git commit -m "feat(desktop): profiles store + profile_run event + loadProfiles"
```

---

## Task 2: analysis-device toggle in Settings

**Files:**
- Modify: `apps/desktop/src/components/SettingsModal.svelte`

- [ ] **Step 1: Add the device row**

In `SettingsModal.svelte`, add `ANALYSIS_DEVICE` to the imports from `../lib/stores` (the import list currently has `CAPTURE_BUFFER_SECS, GRID_SNAP_DEFAULT, ...`):

```ts
    ANALYSIS_DEVICE,
```

In the `<script>`, add a derived current value (near the other `$derived` lines):

```ts
  let device = $derived(($settings[ANALYSIS_DEVICE] as string) ?? "auto");
```

Add this row inside the `<Modal>`, after the `capture buffer` row:

```svelte
  <div class="row">
    <span class="label">analysis device</span>
    <div class="chips">
      <Button
        variant="chip"
        active={device === "auto"}
        onclick={() => void actions.setSetting(ANALYSIS_DEVICE, "auto")}
      >
        auto
      </Button>
      <Button
        variant="chip"
        active={device === "cpu"}
        onclick={() => void actions.setSetting(ANALYSIS_DEVICE, "cpu")}
      >
        cpu
      </Button>
    </div>
  </div>
  <p class="hint mono">auto = GPU when it fits, else CPU · cpu = slower, never out of VRAM</p>
```

Add a small style for `.hint` in the `<style>` block:

```css
  .hint {
    font-size: 10px;
    color: var(--muted);
    margin-top: calc(var(--space) * -0.5);
  }
```

- [ ] **Step 2: Type-check**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 new errors.

- [ ] **Step 3: Visual/behavior sanity (build)**

Run: `cd apps/desktop && pnpm build`
Expected: builds clean (vite bundles without errors).

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/SettingsModal.svelte
git commit -m "feat(desktop): analysis device (auto/cpu) toggle in settings"
```

---

## Task 3: engine caption on the Sections panel

**Files:**
- Modify: `apps/desktop/src/components/Sections.svelte`

- [ ] **Step 1: Add the caption**

In `Sections.svelte`'s `<script>`, add a derived label from the open song's analysis engine (the `openSong` store is already imported; `openSong.analysis` has an `engine: string`):

```ts
  let engineLabel = $derived.by(() => {
    const e = $openSong?.analysis?.engine;
    if (!e) return null;
    if (e === "songformer") return "SongFormer";
    if (e.includes("novelty")) return "novelty (SongFormer unavailable)";
    return e;
  });
```

In the template, render the caption near the section list. Add it right after the opening of the sections block (find where the rows/list render; place it as a small caption line above the list — e.g. just inside the top of the component's main markup, after the `{#if $openSong}` guard if present, or above the `<ul>`/rows). Use:

```svelte
  {#if engineLabel}
    <p class="engine mono" class:fallback={engineLabel.startsWith("novelty")}>
      sections: {engineLabel}
    </p>
  {/if}
```

Add styles in the `<style>` block:

```css
  .engine {
    font-size: 10px;
    color: var(--muted);
    margin-bottom: calc(var(--space) / 2);
  }
  .engine.fallback {
    color: var(--shaky);
  }
```

(`--shaky` is the existing amber rating color, used elsewhere in the app for "needs attention".)

- [ ] **Step 2: Type-check**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 new errors.

- [ ] **Step 3: Build**

Run: `cd apps/desktop && pnpm build`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/Sections.svelte
git commit -m "feat(desktop): show which engine produced the sections"
```

---

## Task 4: Profiling panel + tab

**Files:**
- Create: `apps/desktop/src/components/ProfilingPanel.svelte`
- Modify: `apps/desktop/src/App.svelte`

- [ ] **Step 1: Create the panel component**

Create `apps/desktop/src/components/ProfilingPanel.svelte` (modeled on `DuePanel.svelte`):

```svelte
<script lang="ts">
  import { profiles, songs, type ProfileRun } from "../lib/stores";

  function songTitle(id?: number): string {
    if (id == null) return "";
    return $songs.find((s) => s.id === id)?.title ?? `song ${id}`;
  }

  function secs(ms: number): string {
    return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
  }

  function maxStage(run: ProfileRun): number {
    return Math.max(1, ...run.stages.map((s) => s.ms));
  }
</script>

<h2>profiling</h2>
{#if $profiles.length === 0}
  <p class="empty">no runs yet</p>
{:else}
  <ul>
    {#each $profiles as run, i (i)}
      <li class="run" class:failed={!run.ok}>
        <div class="head">
          <span class="op">{run.op}</span>
          <span class="title">{songTitle(run.song_id)}</span>
          <span class="total mono">{secs(run.total_ms)}</span>
        </div>
        <div class="badges">
          {#if run.device}<span class="badge dev">{run.device}</span>{/if}
          {#if run.engine}<span class="badge eng">{run.engine}</span>{/if}
          {#if !run.ok}<span class="badge err">failed</span>{/if}
        </div>
        {#if run.stages.length}
          <div class="stages">
            {#each run.stages as st (st.name)}
              <div class="stage" title={`${st.name}: ${secs(st.ms)}${st.note ? ` — ${st.note}` : ""}`}>
                <span class="sname mono">{st.name}</span>
                <span class="bar"><span class="fill" style="width: {(st.ms / maxStage(run)) * 100}%"></span></span>
                <span class="sms mono">{secs(st.ms)}</span>
              </div>
            {/each}
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .empty { font-size: 11px; color: var(--muted); }
  .run { padding: calc(var(--space) / 2) 0; border-bottom: 1px solid var(--bg-raised); }
  .head { display: flex; align-items: baseline; gap: var(--space); }
  .op { font-size: 12px; }
  .title { flex: 1; min-width: 0; font-size: 11px; color: var(--muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .total { font-size: 11px; }
  .badges { display: flex; gap: 4px; margin-top: 2px; }
  .badge { font-size: 9px; padding: 1px 5px; border-radius: 8px; background: var(--bg-raised); color: var(--muted); }
  .badge.eng { color: var(--accent); }
  .badge.err { color: var(--miss); }
  .stages { margin-top: 4px; display: flex; flex-direction: column; gap: 2px; }
  .stage { display: flex; align-items: center; gap: 6px; }
  .sname { font-size: 10px; color: var(--muted); width: 7em; flex: 0 0 auto; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bar { flex: 1; height: 4px; background: var(--bg-raised); border-radius: 2px; overflow: hidden; }
  .fill { display: block; height: 100%; background: var(--accent); }
  .sms { font-size: 10px; color: var(--muted); width: 4em; text-align: right; flex: 0 0 auto; }
</style>
```

- [ ] **Step 2: Register the tab in `App.svelte`**

Add the import (alongside the other component imports):

```ts
  import ProfilingPanel from "./components/ProfilingPanel.svelte";
```

Add `"profile"` to the `TABS` tuple:

```ts
  const TABS = ["sections", "loops", "plan", "capture", "due", "profile"] as const;
```

In the tab render block, change the final `{:else}` (which renders `DuePanel`) into explicit branches so `profile` has its own:

```svelte
          {:else if tab === "due"}
            <DuePanel />
          {:else}
            <ProfilingPanel />
          {/if}
```

(The existing block ends `{:else}\n<DuePanel />\n{/if}` — replace that tail with the two branches above.)

- [ ] **Step 3: Type-check + build**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 new errors.
Run: `cd apps/desktop && pnpm build`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/ProfilingPanel.svelte apps/desktop/src/App.svelte
git commit -m "feat(desktop): profiling panel + tab"
```

---

## Task 5: per-step "last run" line in the prepare modal

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts` (add `song_id` to `PrepareState`)
- Modify: `apps/desktop/src/components/PrepareModal.svelte`

- [ ] **Step 1: Thread the prepared song id into `PrepareState`**

In `stores.ts`, add `song_id` to the `PrepareState` interface:

```ts
export interface PrepareState {
  open: boolean;
  song_id: number;
  steps: { analysis: PrepareStepState; stems: PrepareStepState };
  errors: { analysis?: string; stems?: string };
}
```

Find where `prepareState.set({...})` initializes the modal inside the `prepare()` action (it sets `open`, `steps`, `errors`, and assigns `prepareSongId`). Add `song_id` to that object literal using the same song id already in scope there (the song being prepared — the value assigned to `prepareSongId`). For example if the code reads `prepareSongId = id;` then the set becomes:

```ts
    prepareState.set({
      open: true,
      song_id: id,
      steps: { analysis: "pending", stems: "pending" },
      errors: {},
    });
```

(Match the EXACT existing field names and the id variable in that function — read it first; only ADD `song_id`.)

- [ ] **Step 2: Show the line in `PrepareModal.svelte`**

Add `profiles` to the imports from `../lib/stores`:

```ts
  import { actions, prepareState, profiles, type PrepareStepState } from "../lib/stores";
```

In the `<script>`, add a helper that finds the latest profile for a step+song:

```ts
  function lastRun(step: string): string | null {
    const s = $prepareState;
    if (!s) return null;
    const op = step === "analysis" ? "analysis" : "stems";
    const run = $profiles.find((p) => p.op === op && p.song_id === s.song_id);
    if (!run) return null;
    const ms = run.total_ms;
    const t = ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} s`;
    return [t, run.device, run.engine].filter(Boolean).join(" · ");
  }
```

In the step `<li>`, after the `{#if s === "cached"}...{/if}` line, add:

```svelte
          {#if terminal(s)}
            {@const summary = lastRun(step.key)}
            {#if summary}<span class="note mono">{summary}</span>{/if}
          {/if}
```

(`terminal` and the `note` style already exist in this component.)

- [ ] **Step 3: Type-check + build**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 new errors.
Run: `cd apps/desktop && pnpm build`
Expected: clean.

- [ ] **Step 4: Full frontend gate**

Run: `cd apps/desktop && pnpm vitest run` (all frontend tests) and `pnpm exec svelte-check --tsconfig ./tsconfig.json`.
Expected: all green.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/stores.ts apps/desktop/src/components/PrepareModal.svelte
git commit -m "feat(desktop): prepare modal shows last-run time/device/engine per step"
```

---

## Manual verification (after Task 5)

- [ ] `just build` (full desktop build) then `just run`.
- [ ] Open Settings (`,`) → confirm the **analysis device** auto/cpu toggle; set it to `cpu`.
- [ ] Open a song, hit PREPARE → after analysis, the prepare modal shows a "last run" line (e.g. `229.0 s · cpu · songformer`), and the **profile** tab lists the run with stage bars.
- [ ] Sections panel shows the engine caption (`sections: SongFormer`, or amber `novelty (SongFormer unavailable)` on a fallback).

---

## Self-review checklist (done while writing)

- **Spec coverage:** settings device toggle (T2), engine surfacing (T3), profiling panel (T4), prepare-modal last-run line (T5), profiles store + `profile_run` event + `profiles.list` fetch (T1). All four frontend bullets of the spec covered.
- **Placeholder scan:** none — full code in every step. Two spots require reading the existing file to match exact names (the `prepareState.set` literal in T5-Step1, and the tab-render tail in T4-Step2); both are called out explicitly with the surrounding context.
- **Type consistency:** `ProfileRun`/`ProfileStage` shapes match the backend wire types (`op`, `song_id?`, `total_ms`, `ok`, `device?`, `engine?`, `stages[]`); `profiles` store, `loadProfiles`/`recordProfile` actions, `ANALYSIS_DEVICE` const, and the `profile_run` event name are consistent across tasks.
- **No render-tests:** consistent with the repo — only Task 1 (pure store logic) is Vitest-tested; components verified via `svelte-check` + build.
