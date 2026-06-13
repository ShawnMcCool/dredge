# Live Readout v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Persist per-run max CPU/GPU/VRAM into each profile (schema V5) and redesign the live readout: model name per step, one shared meters block with the VRAM histogram in its own column, and a per-effort completion summary.

**Architecture:** The sampler folds each tick's metrics into running maxima held in the shared `WorkState`; the worker stamps them onto the `ProfileRun` (new nullable columns, schema V5). The Svelte `LiveProgress` reads the persisted `profiles` for its completion summary and the live `workSample`/`vram` for the active meters.

**Tech Stack:** Rust (`practice`, `server`), `rusqlite`, Svelte 5, Vitest, svelte-check. No new dependencies.

**Depends on:** the live-readout + VRAM-histogram features (through `ccb4e5b`).

---

## File Structure

- `crates/practice/src/model.rs` — 4 new `ProfileRun` fields (Task 1).
- `crates/server/src/profile.rs` — `Timer::finish` sets the new fields to None (Task 1).
- `crates/practice/src/store.rs` — schema V5 columns + save/list (Task 2).
- `crates/server/src/sampler.rs` — `WorkState` maxima + `observe`/`maxes` + sampler folds (Task 3).
- `crates/server/src/app.rs` — workers stamp maxima onto the `ProfileRun` (Task 4).
- `crates/server/tests/app_profiling.rs` — regression for max in the event (Task 4).
- `apps/desktop/src/lib/stores.ts` — `ProfileRun` TS fields (Task 5).
- `apps/desktop/src/lib/livesummary.ts` (new) + `livesummary.test.ts` (new) — completion-summary helper (Task 5).
- `apps/desktop/src/components/LiveProgress.svelte` — redesign (Task 6).

---

## Task 1: ProfileRun max fields (compile-only)

**Files:** `crates/practice/src/model.rs`, `crates/server/src/profile.rs`, `crates/practice/src/store.rs` (test literal).

- [ ] **Step 1: Add the fields to `ProfileRun`**

In `crates/practice/src/model.rs`, in `struct ProfileRun`, immediately BEFORE `pub stages: Vec<ProfileStage>,`:
```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cpu_pct: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_gpu_util: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_vram_used_mb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vram_total_mb: Option<u32>,
```

- [ ] **Step 2: Fix `Timer::finish` (server won't compile otherwise)**

In `crates/server/src/profile.rs`, in `Timer::finish`'s `ProfileRun { ... }` literal, add (anywhere among the fields):
```rust
            max_cpu_pct: None,
            max_gpu_util: None,
            max_vram_used_mb: None,
            vram_total_mb: None,
```

- [ ] **Step 3: Fix the store test literal (practice won't compile otherwise)**

In `crates/practice/src/store.rs`, the `profiles_roundtrip_and_trim` test builds a `ProfileRun { ... }`. Add the four fields to that literal so it compiles (Task 2 will set them to real values):
```rust
        max_cpu_pct: None,
        max_gpu_util: None,
        max_vram_used_mb: None,
        vram_total_mb: None,
```

- [ ] **Step 4: Verify both crates compile + pass**

Run: `cargo test -p practice && cargo test -p server`
Expected: builds and all existing tests pass (the new fields are serialized by serde, so no dead-code warnings).

- [ ] **Step 5: Commit**

```bash
git add crates/practice/src/model.rs crates/server/src/profile.rs crates/practice/src/store.rs
git commit -m "feat(practice): ProfileRun gains max cpu/gpu/vram fields"
```

---

## Task 2: persist max metrics (schema V5)

**Files:** `crates/practice/src/store.rs`

- [ ] **Step 1: Strengthen the round-trip test to assert maxes persist**

In `crates/practice/src/store.rs`'s `profiles_roundtrip_and_trim` test, change the four `max_*`/`vram_*` fields in the constructed `ProfileRun` from `None` to real values:
```rust
        max_cpu_pct: Some(496),
        max_gpu_util: Some(41),
        max_vram_used_mb: Some(6100),
        vram_total_mb: Some(16000),
```
and after the existing `listed[0]` assertions, add:
```rust
    assert_eq!(listed[0].max_cpu_pct, Some(496));
    assert_eq!(listed[0].max_gpu_util, Some(41));
    assert_eq!(listed[0].max_vram_used_mb, Some(6100));
    assert_eq!(listed[0].vram_total_mb, Some(16000));
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p practice profiles_roundtrip_and_trim`
Expected: FAIL — listed maxes are `None` (save/list don't persist them yet).

- [ ] **Step 3: Add schema V5 + bump migration**

After the `SCHEMA_V4` const in `store.rs`:
```rust
/// v5: per-run max resource metrics on profiles.
const SCHEMA_V5: &str = "
ALTER TABLE profiles ADD COLUMN max_cpu_pct INTEGER;
ALTER TABLE profiles ADD COLUMN max_gpu_util INTEGER;
ALTER TABLE profiles ADD COLUMN max_vram_used_mb INTEGER;
ALTER TABLE profiles ADD COLUMN vram_total_mb INTEGER;
";
```
In `migrate()`, after the `if version < 4 { ... }` block:
```rust
        if version < 5 {
            self.conn.execute_batch(SCHEMA_V5)?;
            self.conn.pragma_update(None, "user_version", 5)?;
        }
```

- [ ] **Step 4: Persist + read the columns**

In `save_profile`, change the INSERT to include the four columns and params (note the `as i64` cast on the optional u32s):
```rust
        let started: String = self.conn.query_row(
            "INSERT INTO profiles (op, song_id, total_ms, ok, error, device, engine, stages_json,
                max_cpu_pct, max_gpu_util, max_vram_used_mb, vram_total_mb)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
             RETURNING started_at",
            params![
                run.op,
                run.song_id.map(|s| s.0),
                run.total_ms as i64,
                run.ok as i64,
                run.error,
                run.device,
                run.engine,
                serde_json::to_string(&run.stages)?,
                run.max_cpu_pct.map(|v| v as i64),
                run.max_gpu_util.map(|v| v as i64),
                run.max_vram_used_mb.map(|v| v as i64),
                run.vram_total_mb.map(|v| v as i64),
            ],
            |row| row.get(0),
        )?;
```

In `list_profiles`, extend the SELECT and the row reconstruction:
```rust
        let mut stmt = self.conn.prepare(
            "SELECT op, song_id, started_at, total_ms, ok, error, device, engine, stages_json,
                max_cpu_pct, max_gpu_util, max_vram_used_mb, vram_total_mb
             FROM profiles ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |row| {
                let stages: String = row.get(8)?;
                Ok(crate::model::ProfileRun {
                    op: row.get(0)?,
                    song_id: row.get::<_, Option<i64>>(1)?.map(crate::model::SongId),
                    started_at: row.get(2)?,
                    total_ms: row.get::<_, i64>(3)? as u64,
                    ok: row.get::<_, i64>(4)? != 0,
                    error: row.get(5)?,
                    device: row.get(6)?,
                    engine: row.get(7)?,
                    max_cpu_pct: row.get::<_, Option<i64>>(9)?.map(|v| v as u32),
                    max_gpu_util: row.get::<_, Option<i64>>(10)?.map(|v| v as u32),
                    max_vram_used_mb: row.get::<_, Option<i64>>(11)?.map(|v| v as u32),
                    vram_total_mb: row.get::<_, Option<i64>>(12)?.map(|v| v as u32),
                    stages: serde_json::from_str(&stages).map_err(json_err)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
```
(The `stages` field stays last in the struct literal; field order in a literal doesn't matter, but keep it valid Rust.)

- [ ] **Step 5: Run to confirm pass**

Run: `cargo test -p practice`
Expected: PASS (round-trip incl. maxes; the 200-row trim still works).

- [ ] **Step 6: Commit**

```bash
git add crates/practice/src/store.rs
git commit -m "feat(practice): profiles schema V5 — persist max cpu/gpu/vram"
```

---

## Task 3: sampler maxima

**Files:** `crates/server/src/sampler.rs`

- [ ] **Step 1: Add the failing test**

Add to the `#[cfg(test)] mod tests` in `sampler.rs`:
```rust
    #[test]
    fn reporter_observe_tracks_maxima() {
        let state = std::sync::Arc::new(std::sync::Mutex::new(None));
        let r = WorkReporter::new(state);
        r.begin("analysis", "x");
        assert_eq!(r.maxes(), Some((0, None, None, None)));
        r.observe(100, Some((40, 5000, 16000)));
        r.observe(80, Some((50, 6000, 16000)));
        r.observe(120, None);
        assert_eq!(r.maxes(), Some((120, Some(50), Some(6000), Some(16000))));
        r.end();
        assert_eq!(r.maxes(), None);
    }
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p server --lib reporter_observe_tracks_maxima`
Expected: FAIL — `observe`/`maxes` not found; `WorkState` lacks max fields.

- [ ] **Step 3: Add max fields to `WorkState`**

In `WorkState`, add after `started`:
```rust
    pub max_cpu: u32,
    pub max_gpu_util: Option<u32>,
    pub max_vram_used_mb: Option<u32>,
    pub vram_total_mb: Option<u32>,
```

- [ ] **Step 4: Init them in `begin`, add `observe` + `maxes`**

In `WorkReporter::begin`, the `Some(WorkState { ... })` literal gains:
```rust
            max_cpu: 0,
            max_gpu_util: None,
            max_vram_used_mb: None,
            vram_total_mb: None,
```
Add these methods to `impl WorkReporter`:
```rust
    /// Fold one sample's metrics into the run's running maxima.
    pub fn observe(&self, cpu: u32, gpu: Option<(u32, u32, u32)>) {
        if let Some(ws) = self.state.lock().unwrap().as_mut() {
            ws.max_cpu = ws.max_cpu.max(cpu);
            if let Some((util, used, total)) = gpu {
                ws.max_gpu_util = Some(ws.max_gpu_util.unwrap_or(0).max(util));
                ws.max_vram_used_mb = Some(ws.max_vram_used_mb.unwrap_or(0).max(used));
                ws.vram_total_mb = Some(total);
            }
        }
    }

    /// Read the run's maxima (cpu, gpu_util, vram_used, vram_total), or None if idle.
    pub fn maxes(&self) -> Option<(u32, Option<u32>, Option<u32>, Option<u32>)> {
        self.state
            .lock()
            .unwrap()
            .as_ref()
            .map(|ws| (ws.max_cpu, ws.max_gpu_util, ws.max_vram_used_mb, ws.vram_total_mb))
    }
```

- [ ] **Step 5: Fold maxima in the sampler loop**

In `run()`, after `let mut gpu_ok = true;`, add a reporter over the shared state:
```rust
    let reporter = WorkReporter::new(state.clone());
```
Then in the loop, after the `let gpu = ...;` block and BEFORE the `let _ = tx.send(...)`, add:
```rust
        reporter.observe(cpu, gpu);
```

- [ ] **Step 6: Run tests**

Run: `cargo test -p server --lib sampler`
Expected: PASS (the new test + existing sampler tests).

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/sampler.rs
git commit -m "feat(server): sampler folds per-run max cpu/gpu/vram into WorkState"
```

---

## Task 4: workers stamp maxima onto the ProfileRun

**Files:** `crates/server/src/app.rs`, `crates/server/tests/app_profiling.rs`

- [ ] **Step 1: Add the regression test**

Append to `crates/server/tests/app_profiling.rs`:
```rust
#[test]
fn analysis_profile_includes_max_metrics() {
    let mut ctx = setup(Arc::new(DeviceAwareAnalyzer));
    req(&mut ctx.app, "settings.set", json!({"key":"analysis_device","value":"cpu"}));
    req(&mut ctx.app, "analysis.run", json!({"song_id": ctx.song_id, "force": true}));
    let data = wait_for_event(&mut ctx.app, "profile_run");
    // sampler isn't spawned in tests, so observe() never runs → max_cpu_pct is 0,
    // but it must be present (the worker stamped the maxima onto the profile).
    assert!(data["max_cpu_pct"].is_number(), "max_cpu_pct present: {data}");
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cargo test -p server --test app_profiling analysis_profile_includes_max_metrics`
Expected: FAIL — `max_cpu_pct` absent (worker doesn't stamp maxima yet).

- [ ] **Step 3: Stamp maxima in the analysis worker**

In `analysis_run`'s spawned closure, replace the section from `let (result, device) = ...` through the `timer.finish(...)` line with:
```rust
            let (result, device) = crate::analysis::analyze_with_recovery(
                analyzer.as_ref(),
                &audio_path,
                &device_setting,
                &mut timer,
                &reporter,
            );
            let m = reporter.maxes();
            reporter.end();
            let engine = result.as_ref().ok().map(|a| a.engine.clone());
            let err = result.as_ref().err().cloned();
            let mut run = timer.finish(result.is_ok(), err, device, engine);
            if let Some((cpu, gpu, vram_used, vram_total)) = m {
                run.max_cpu_pct = Some(cpu);
                run.max_gpu_util = gpu;
                run.max_vram_used_mb = vram_used;
                run.vram_total_mb = vram_total;
            }
            let _ = tx.send((song_id, result));
            let _ = profile_tx.send(run);
```
(The previous code called `reporter.end()` right after `analyze_with_recovery`; now `reporter.maxes()` is read FIRST, then `end()`. `run` is now `mut`.)

- [ ] **Step 4: Stamp maxima in the stems worker**

In `stems_separate`'s spawned closure, replace the section from `let result = timer.stage("demucs", ...)` through the `timer.finish(...)` line with:
```rust
            let result = timer.stage("demucs", || separator.separate(&audio_path, &cache, force_cpu));
            let m = reporter.maxes();
            reporter.end();
            separating.lock().unwrap().remove(&song_id.0);
            let err = result.as_ref().err().cloned();
            let mut run = timer.finish(result.is_ok(), err.clone(), Some(device), None);
            if let Some((cpu, gpu, vram_used, vram_total)) = m {
                run.max_cpu_pct = Some(cpu);
                run.max_gpu_util = gpu;
                run.max_vram_used_mb = vram_used;
                run.vram_total_mb = vram_total;
            }
```
(Keep the rest of the closure — the `stems_progress` event send and `profile_tx.send(run)` — unchanged. `reporter.end()` now runs before `separating.remove`, which is fine; `maxes()` is read before `end()`.)

- [ ] **Step 5: Run tests + clippy**

Run: `cargo test -p server` then `cargo clippy -p server --all-targets -- -D warnings`
Expected: all PASS, no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/server/src/app.rs crates/server/tests/app_profiling.rs
git commit -m "feat(server): workers stamp max cpu/gpu/vram onto each ProfileRun"
```

---

## Task 5: frontend ProfileRun fields + completion-summary helper

**Files:** `apps/desktop/src/lib/stores.ts`, `apps/desktop/src/lib/livesummary.ts` (new), `apps/desktop/src/lib/livesummary.test.ts` (new)

- [ ] **Step 1: Add the max fields to the TS `ProfileRun`**

In `stores.ts`, in the `ProfileRun` interface, after `engine?: string;`:
```ts
  max_cpu_pct?: number;
  max_gpu_util?: number;
  max_vram_used_mb?: number;
  vram_total_mb?: number;
```

- [ ] **Step 2: Write the failing test**

Create `apps/desktop/src/lib/livesummary.test.ts`:
```ts
import { describe, expect, it } from "vitest";
import { effortSummaries } from "./livesummary";
import type { ProfileRun } from "./stores";

const run = (op: string, over: Partial<ProfileRun> = {}): ProfileRun => ({
  op, started_at: "", total_ms: 1000, ok: true, stages: [], ...over,
});

describe("effortSummaries", () => {
  it("picks the newest profile per op and formats the max line", () => {
    const profiles: ProfileRun[] = [
      run("stems", { total_ms: 15000, stages: [{ name: "demucs", ms: 15000 }], max_cpu_pct: 180, max_gpu_util: 62, max_vram_used_mb: 3200, vram_total_mb: 16000 }),
      run("analysis", { total_ms: 217000, device: "cpu", engine: "songformer",
        stages: [{ name: "GPU attempt", ms: 22600 }, { name: "CPU recovery", ms: 194400 }],
        max_cpu_pct: 496, max_gpu_util: 41, max_vram_used_mb: 6100, vram_total_mb: 16000 }),
      run("analysis", { total_ms: 999999 }), // older analysis — must be ignored
    ];
    const s = effortSummaries(profiles);
    expect(s.map((e) => e.op)).toEqual(["analysis", "stems"]);
    const a = s[0];
    expect(a.total_ms).toBe(217000);
    expect(a.engine).toBe("songformer");
    expect(a.stages).toHaveLength(2);
    expect(a.maxLine).toBe("cpu 496% · gpu 41% · vram 6.0/16 GB");
    expect(s[1].maxLine).toBe("cpu 180% · gpu 62% · vram 3.1/16 GB");
  });

  it("omits the max line when absent and skips missing ops", () => {
    const s = effortSummaries([run("analysis", { total_ms: 100 })]);
    expect(s).toHaveLength(1);
    expect(s[0].maxLine).toBeNull();
  });
});
```

- [ ] **Step 3: Confirm it fails**

Run: `cd apps/desktop && pnpm vitest run lib/livesummary.test.ts`
Expected: FAIL — `./livesummary` not found.

- [ ] **Step 4: Implement the helper**

Create `apps/desktop/src/lib/livesummary.ts`:
```ts
import type { ProfileRun, ProfileStage } from "./stores";

export interface EffortSummary {
  op: string;
  total_ms: number;
  device?: string;
  engine?: string;
  stages: ProfileStage[];
  maxLine: string | null;
}

/** Newest profile per op (analysis, then stems) from a most-recent-first list,
 *  shaped for the completion summary. */
export function effortSummaries(profiles: ProfileRun[]): EffortSummary[] {
  const out: EffortSummary[] = [];
  for (const op of ["analysis", "stems"]) {
    const p = profiles.find((r) => r.op === op);
    if (!p) continue;
    const maxLine =
      p.max_cpu_pct != null
        ? [
            `cpu ${p.max_cpu_pct}%`,
            p.max_gpu_util != null ? `gpu ${p.max_gpu_util}%` : null,
            p.vram_total_mb != null
              ? `vram ${((p.max_vram_used_mb ?? 0) / 1024).toFixed(1)}/${Math.round(p.vram_total_mb / 1024)} GB`
              : null,
          ]
            .filter(Boolean)
            .join(" · ")
        : null;
    out.push({ op: p.op, total_ms: p.total_ms, device: p.device, engine: p.engine, stages: p.stages, maxLine });
  }
  return out;
}
```

- [ ] **Step 5: Confirm pass + type-check**

Run: `cd apps/desktop && pnpm vitest run lib/livesummary.test.ts`
Expected: PASS.
Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: no new errors.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/lib/stores.ts apps/desktop/src/lib/livesummary.ts apps/desktop/src/lib/livesummary.test.ts
git commit -m "feat(desktop): ProfileRun max fields + effortSummaries helper"
```

---

## Task 6: LiveProgress redesign

**Files:** `apps/desktop/src/components/LiveProgress.svelte`

- [ ] **Step 1: Replace the component with the v2 layout**

Overwrite `apps/desktop/src/components/LiveProgress.svelte` with:
```svelte
<script lang="ts">
  import { prepareState, workSample, vram, profiles, type PrepareStepState } from "../lib/stores";
  import { effortSummaries } from "../lib/livesummary";

  const STEPS = [
    { key: "analysis", label: "analyzing structure", op: "analysis", model: "SongFormer" },
    { key: "stems", label: "separating stems", op: "stems", model: "Demucs" },
  ] as const;

  const GLYPHS: Record<PrepareStepState, string> = {
    pending: "·", running: "◌", done: "✓", cached: "✓", failed: "✗",
  };

  function fmt(ms: number): string {
    if (ms < 1000) return `${ms} ms`;
    const s = ms / 1000;
    return s < 60 ? `${s.toFixed(1)} s` : `${Math.floor(s / 60)}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
  }

  let summaries = $derived(effortSummaries($profiles));
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
          <span class="elapsed mono">{fmt($workSample.elapsed_ms)}</span>
        {:else if s === "cached"}
          <span class="muted mono">cached</span>
        {/if}
        {#if $prepareState.errors[step.key]}
          <span class="error">{$prepareState.errors[step.key]}</span>
        {/if}
      </div>
    {/each}
    {#if $workSample}
      <div class="meters">
        <div class="bars">
          <div class="meter">
            <span class="mlabel mono">cpu</span>
            <span class="bar"><span class="fill" style="width: {Math.min(100, $workSample.cpu_pct / 8)}%"></span></span>
            <span class="mval mono">{$workSample.cpu_pct}%</span>
          </div>
          {#if $workSample.gpu_util != null}
            <div class="meter">
              <span class="mlabel mono">gpu</span>
              <span class="bar"><span class="fill" style="width: {$workSample.gpu_util}%"></span></span>
              <span class="mval mono">{$workSample.gpu_util}%</span>
            </div>
          {/if}
        </div>
        {#if $vram && $vram.used.length}
          <div class="vramcol">
            <span class="hist">
              <svg viewBox="0 0 60 100" preserveAspectRatio="none">
                {#each $vram.used as u, i (i)}
                  <rect x={i} y={100 - (u / $vram.total) * 100} width="1" height={(u / $vram.total) * 100} />
                {/each}
                <line x1="0" x2="60" y1={100 - ($vram.peak / $vram.total) * 100} y2={100 - ($vram.peak / $vram.total) * 100} class="peak" />
              </svg>
            </span>
            <span class="mval mono">{($vram.used[$vram.used.length - 1] / 1024).toFixed(1)} / {Math.round($vram.total / 1024)} GB</span>
          </div>
        {/if}
      </div>
    {/if}
  </section>
{:else if summaries.length}
  <section class="live idle">
    {#each summaries as e (e.op)}
      <div class="effort">
        <div class="ehead mono">{e.op} · {fmt(e.total_ms)}{#if e.device} · {e.device}{/if}{#if e.engine} · {e.engine}{/if}</div>
        {#if e.stages.length}
          <div class="esub mono">{e.stages.map((st) => `${st.name} ${fmt(st.ms)}`).join(" · ")}</div>
        {/if}
        {#if e.maxLine}
          <div class="esub mono">{e.maxLine}</div>
        {/if}
      </div>
    {/each}
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
  .name { font-size: 13px; }
  .model { font-size: 10px; color: var(--muted); }
  .stage { font-size: 11px; color: var(--accent); }
  .elapsed { margin-left: auto; font-size: 11px; color: var(--muted); }
  .muted { color: var(--muted); font-size: 11px; }
  .error { color: var(--miss); font-size: 11px; }
  .meters { display: flex; gap: var(--space); align-items: flex-start; margin: 6px 0 6px 1.2em; }
  .bars { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
  .meter { display: flex; align-items: center; gap: 6px; }
  .mlabel { font-size: 10px; color: var(--muted); width: 2em; }
  .bar { flex: 1; height: 4px; background: var(--bg-raised); border-radius: 2px; overflow: hidden; max-width: 220px; }
  .fill { display: block; height: 100%; background: var(--accent); }
  .mval { font-size: 10px; color: var(--muted); }
  .vramcol { display: flex; flex-direction: column; gap: 2px; flex: 0 0 auto; align-items: flex-start; }
  .hist { height: 28px; width: 160px; background: var(--bg-raised); border-radius: 2px; overflow: hidden; }
  .hist svg { width: 100%; height: 100%; display: block; }
  .hist rect { fill: var(--accent); }
  .hist line.peak { stroke: var(--shaky); stroke-width: 1; vector-effect: non-scaling-stroke; }
  .effort { margin-bottom: 6px; }
  .ehead { font-size: 11px; }
  .esub { font-size: 10px; color: var(--muted); margin-left: 1em; }
  .idle { color: var(--muted); }
  @keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  @media (prefers-reduced-motion: reduce) { .glyph.running { animation: none; } }
</style>
```

- [ ] **Step 2: Type-check + build + full frontend gate**

Run: `cd apps/desktop && pnpm exec svelte-check --tsconfig ./tsconfig.json`
Expected: 0 errors.
Run: `cd apps/desktop && pnpm build`
Expected: clean.
Run: `cd apps/desktop && pnpm vitest run`
Expected: all pass.

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/components/LiveProgress.svelte
git commit -m "feat(desktop): live readout v2 — model per step, shared meters w/ vram column, completion summary"
```

---

## Manual verification (after Task 6)

- [ ] `just build` then `just run`.
- [ ] ANALYZE TRACK a fresh song. The readout header reads `ANALYZING`; each step shows its model (`· SongFormer`, `· Demucs`); one meters block sits below the list with cpu/gpu bars on the left and the VRAM histogram in its own column on the right.
- [ ] After it finishes, the readout collapses to a per-effort summary: `analysis · 217.0 s · cpu · songformer`, the stage line, and `cpu 496% · gpu 41% · vram 6.1/16 GB`; plus a `stems` block.
- [ ] Open the Profile tab — the analysis/stems rows now also carry the max metrics (persisted), and they survive an app restart.

---

## Self-review checklist (done while writing)

- **Spec coverage:** ProfileRun max fields (T1) · schema V5 persist (T2) · sampler folds maxima + WorkState fields (T3) · workers stamp maxima (T4) · TS fields + per-effort summary helper newest-per-op (T5) · LiveProgress v2: ANALYZING header, model per step, shared meters with vram own column, completion summary (T6). All covered.
- **Placeholder scan:** none — full code each step. The two struct-literal fixes (T1) and the worker-block replacements (T4) quote the surrounding code.
- **Type consistency:** Rust `max_cpu_pct/max_gpu_util/max_vram_used_mb/vram_total_mb` (model + store + reporter `maxes()` tuple order cpu, gpu, vram_used, vram_total) match the worker stamping in T4; TS `ProfileRun` fields (T5) match `effortSummaries` usage (T5) and `LiveProgress` (T6); `effortSummaries` signature identical across T5 test/impl and T6 import.
