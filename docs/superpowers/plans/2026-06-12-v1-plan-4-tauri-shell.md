# earworm v1 — Plan 4: Tauri shell

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The desktop UI: waveform-centred, keyboard-first, deliberately spare. Embeds the Plan-3 `App` dispatcher (single `dispatch` Tauri command + event emission) and keeps the control socket running alongside the UI.

**Architecture:** `apps/desktop/src-tauri` is a workspace crate hosting `Arc<Mutex<server::app::App>>`; the webview talks through one Tauri command (`dispatch(Request) -> Response`) and listens to one event channel (`earworm://event`). Frontend: Vite + Svelte 5 + TypeScript. All UI state derives from dispatch responses + events; no second source of truth. Playhead animates client-side (rAF) from the last `position` event extrapolated by rate — LiveView-hook-style discipline: the canvas owns per-frame work, events only sync state.

**Tech Stack:** Tauri 2, @tauri-apps/api, tauri-plugin-dialog (file picker), Vite, Svelte 5, TypeScript, vitest (pure-logic UI tests).

**Design language (apply throughout, no component library):** near-black `#101014` background, `#e8e6e3` text, one accent `#e0a458` (amber), muted `#6b7280` for chrome; mono font (`ui-monospace`) for times/rates; 8 px spacing grid; no shadows, no rounded-corner soup (2 px radius max), no animations except the playhead and a 120 ms panel fade. The waveform is the hero; everything else is quiet lists.

**Spec:** `docs/superpowers/specs/2026-06-12-earworm-design.md`

---

### Task 1: Scaffold

**Files:**
- Create: `apps/desktop/` — Vite+Svelte+TS app (`package.json`, `vite.config.ts`, `tsconfig.json`, `svelte.config.js`, `index.html`, `src/main.ts`, `src/App.svelte`, `src/app.css`)
- Create: `apps/desktop/src-tauri/` — `Cargo.toml`, `tauri.conf.json`, `build.rs`, `src/main.rs`, `icons/` (generate via `pnpm tauri icon` from a plain 512×512 PNG; a solid-color placeholder generated with ImageMagick `magick -size 512x512 xc:'#e0a458' icon.png` is fine for v1)
- Modify: root `Cargo.toml` (add `"apps/desktop/src-tauri"` to members)

- [x] **Step 1: Frontend scaffold**

`pnpm create vite apps/desktop --template svelte-ts` (pnpm 11 syntax may be `pnpm create vite@latest`), then `pnpm add -D @tauri-apps/cli vitest` and `pnpm add @tauri-apps/api @tauri-apps/plugin-dialog`. Strip demo content; `App.svelte` renders an empty three-column shell (`aside.library / main.stage / aside.panels`) with the design tokens in `app.css` as CSS custom properties.

- [x] **Step 2: Tauri scaffold**

`pnpm tauri init` (or hand-write) with: `productName: "earworm"`, identifier `dev.shawn.earworm`, `frontendDist: "../dist"`, `devUrl: "http://localhost:5173"`, window 1280×800, title "earworm". `src-tauri/Cargo.toml` joins the workspace (`edition.workspace = true`), deps: `tauri = { version = "2", features = [] }`, `tauri-plugin-dialog = "2"`, `server/practice/engine` path deps, serde/serde_json. Keep the generated `main.rs` minimal for now (default builder + dialog plugin) — state wiring is Task 2.

- [x] **Step 3: Verify both builds**

Run: `pnpm install && pnpm build` (in apps/desktop) — vite build succeeds.
Run: `cargo check -p earworm-desktop` (name the crate that) — compiles.

- [x] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(desktop): tauri 2 + svelte 5 scaffold with design tokens"
```

---

### Task 2: Rust host — dispatch command, event pump, socket alongside

**Files:**
- Modify: `apps/desktop/src-tauri/src/main.rs`
- Create: `apps/desktop/src-tauri/src/host.rs`

- [x] **Step 1: Implement**

> As-built note: `serve` already owned the tick-pump, so the plan's
> "simplest correct resolution" was taken — `serve(app, path, on_events)`
> now accepts a hook called with each non-empty tick batch; desktop forwards
> those to the webview (`host::start_server`), earwormd passes `|_| {}`.
> No `start_pump` thread exists in the desktop crate; there is exactly one pump.

`host.rs`:
```rust
use server::app::App;
use server::protocol::{Request, Response};
use std::sync::{Arc, Mutex};
use tauri::Emitter;

pub struct AppState(pub Arc<Mutex<App>>);

#[tauri::command]
pub fn dispatch(state: tauri::State<AppState>, req: Request) -> Response {
    state.0.lock().unwrap().dispatch(req)
}

/// 50 ms pump: tick() the app, emit each event to the webview.
pub fn start_pump(handle: tauri::AppHandle, app: Arc<Mutex<App>>) {
    std::thread::spawn(move || loop {
        let events = app.lock().unwrap().tick();
        for ev in events {
            let _ = handle.emit("earworm://event", &ev);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    });
}
```

`main.rs` setup: open Store at `~/.local/share/earworm/earworm.db` (create dirs), `engine::Engine::start()` (on failure, show a dialog/panic with a clear message), build `App`, wrap in `Arc<Mutex<_>>`; `server::socket::serve(app.clone(), &server::socket::default_socket_path())` (keep the handle alive in managed state — UI and scripts share one session); `.manage(AppState(app.clone()))`, `.invoke_handler(tauri::generate_handler![host::dispatch])`, and in `.setup()` call `start_pump`.

**Check against the as-built Plan-3 API first:** `serve`'s exact signature/handle type, and whether `App::tick` already broadcasts to socket subscribers internally (if the socket layer owns the pump, desktop must NOT double-tick — in that case have `serve` return events or add a `tick_hook`; simplest correct resolution: move the tick-pump INTO `server::socket` behind a callback parameter `serve(app, path, on_events: impl Fn(&[Event]))` so there is exactly one pump wherever App lives. Refactor minimally if needed and keep server tests green).

- [x] **Step 2: Verify**

Run: `cargo check -p earworm-desktop && cargo test -p server`
Expected: compiles; server suite still green (especially if `serve` was refactored).

- [x] **Step 3: Commit**

```bash
git add -A && git commit -m "feat(desktop): embed app dispatcher, event pump, shared control socket"
```

---

### Task 3: IPC layer + stores

**Files:**
- Create: `apps/desktop/src/lib/ipc.ts`, `apps/desktop/src/lib/stores.ts`
- Test: `apps/desktop/src/lib/ipc.test.ts` (vitest, mock invoke)

- [ ] **Step 1: Implement**

`ipc.ts`:
```typescript
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

let nextId = 1;

export interface Resp<T = unknown> { id: number; ok: boolean; data?: T; error?: string }

export async function cmd<T = unknown>(cmd: string, params: unknown = null): Promise<T> {
  const req = { id: nextId++, cmd, params };
  const resp = (await invoke("dispatch", { req })) as Resp<T>;
  if (!resp.ok) throw new Error(resp.error ?? `command ${cmd} failed`);
  return resp.data as T;
}

export type EwEvent = { event: string; data: any };

export function onEvent(handler: (e: EwEvent) => void): Promise<() => void> {
  return listen<EwEvent>("earworm://event", (e) => handler(e.payload));
}
```

`stores.ts` — Svelte stores + types mirroring the wire shapes (Song, Section, LoopRegion `{id, song_id, name, start, end, kind}`, Plan, Peaks `{frames_per_bucket, buckets}`):
- `songs`, `openSong` (`{song, sections, loops, plans, peaks} | null`)
- `position` (`{secs, rate, playing, at: number}` — `at = performance.now()` stamped on receipt, for extrapolation)
- `planStatus` (`{plan_id, step_idx, rep_idx, mode, loop_id} | null`)
- `selection` (`{start, end} | null` — waveform drag selection, seconds)
- `actions`: `importSong(path)`, `openSong(id)`, `refreshLoops()`, `play/pause/seek/setRate/setPitch/bassFocus`, `createLoop(name, start, end)`, `deleteLoop(id)`, `savePlan(...)`, `startPlan(id)`, `stopPlan()`, `skipStep()`, `rate(loopId, rating, isRetest)` — each a thin `cmd()` call updating stores from the response.
- `initEvents()`: wires `onEvent` → `position`, `planStatus` (from `rep_changed`/`plan_finished`), and refreshes due/retention panels on `plan_finished`.

`ipc.test.ts` (vitest with `vi.mock("@tauri-apps/api/core")`): `cmd` resolves data on ok:true; throws with the error message on ok:false; ids increment.

- [ ] **Step 2: Run tests, pass; commit**

Run: `pnpm vitest run` — green.

```bash
git add -A && git commit -m "feat(desktop): ipc layer and stores over single dispatch command"
```

---

### Task 4: Waveform component

**Files:**
- Create: `apps/desktop/src/lib/waveform-math.ts`, `apps/desktop/src/lib/waveform-math.test.ts`
- Create: `apps/desktop/src/components/Waveform.svelte`

- [ ] **Step 1: Pure math first (TDD with vitest)**

`waveform-math.ts`:
```typescript
export interface View { startSec: number; endSec: number; width: number } // px

export const secToX = (v: View, s: number) => ((s - v.startSec) / (v.endSec - v.startSec)) * v.width;
export const xToSec = (v: View, x: number) => v.startSec + (x / v.width) * (v.endSec - v.startSec);

/** Extrapolate playhead: position event + elapsed wall time × rate. */
export function playheadSecs(
  pos: { secs: number; rate: number; playing: boolean; at: number },
  now: number,
): number {
  if (!pos.playing) return pos.secs;
  return pos.secs + ((now - pos.at) / 1000) * pos.rate;
}

/** Zoom around an anchor (e.g. cursor), clamped to [0, duration] and a 2 s minimum span. */
export function zoom(v: View, anchorSec: number, factor: number, duration: number): View;

/** Bucket range of the peaks array visible in the view (for drawing). */
export function visibleBuckets(
  v: View, framesPerBucket: number, sampleRate: number, totalBuckets: number,
): { first: number; last: number };
```

Tests: round-trip `secToX`/`xToSec`; playhead extrapolates at 0.75× (`pos.secs=10, at=t0, now=t0+2000` → 11.5) and freezes when paused; zoom keeps the anchor's px position stable and clamps at song bounds and min span; visibleBuckets clamps to `[0, totalBuckets-1]`.

- [ ] **Step 2: Component**

`Waveform.svelte` contract:
- Props: none (reads stores). Canvas fills `main.stage` width, ~200 px tall, plus a 24 px section lane above.
- Draw (single rAF loop, redraw every frame — simple and fast enough at one canvas): background; per visible bucket a vertical min/max line in `#3f4250`, the portion under the playhead in accent-dim; loop regions as translucent accent rectangles with 1 px edges (junction loops dashed edges); current `selection` brighter; sections as labelled spans in the lane; playhead as 1 px accent line using `playheadSecs(...)`.
- Interactions: drag on empty waveform → set `selection` (live); click (no drag, <5 px) → `seek(xToSec(...))`; scroll wheel → `zoom` around cursor; drag a loop edge (±4 px hit zone) → resize that loop (dispatch update: v1 = delete+recreate with same name since there's no loop.update command — fine) ... **simpler and better: add a `loop.update {loop_id, start, end}` command to server** (tiny dispatch arm + store method `update_loop`; add a server test mirroring loop tests' style). Do that rather than delete+recreate.
- Buttons next to selection (small floating chip): "Loop selection" → `createLoop(auto-name, sel.start, sel.end)`, "Play selection" → `loop.set` transport command without persisting.
- DPR-aware canvas sizing (`devicePixelRatio`), resize observer.

- [ ] **Step 3: Verify**

Run: `pnpm vitest run && pnpm build && cargo test -p server` (server gained loop.update).
Expected: green.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(desktop): waveform canvas with selection, zoom, loops, client-side playhead"
```

---

### Task 5: Transport + keyboard

**Files:**
- Create: `apps/desktop/src/components/Transport.svelte`, `apps/desktop/src/lib/keys.ts`

- [ ] **Step 1: Implement**

Transport bar under the waveform: play/pause button; time readout `mm:ss.t / mm:ss` (mono font); rate readout + slider 0.25–2.0 step 0.05 with preset chips `50 70 85 100 %`; pitch chips `-2 -1 0 +1 +2` semitones + cents drag-number; toggles: `BASS FOCUS` (octave-up + low-pass — sends `bass_focus {on}` AND `pitch {octave_up: true}` together), `MUTE`.

`keys.ts` — global keydown (ignore when target is input/textarea): Space play/pause; `r` restart current loop (`loop.set` again with current loop bounds → engine jumps to start; if no loop, `seek 0`); `[` / `]` rate −/+5%; `l` create loop from selection; `Escape` clear selection; `b` bass focus toggle; `1/2/3` rate Miss/Shaky/Solid when the rating prompt is visible (wired in Task 7). Show the map in a help footer line.

- [ ] **Step 2: Verify + commit**

Run: `pnpm build` (svelte-check via build) — clean.

```bash
git add -A && git commit -m "feat(desktop): transport bar and keyboard-first controls"
```

---

### Task 6: Library, sections, loops, plan builder panels

**Files:**
- Create: `apps/desktop/src/components/Library.svelte`, `Sections.svelte`, `Loops.svelte`, `PlanBuilder.svelte`

- [ ] **Step 1: Implement**

- **Library** (left rail): song list (title, artist, duration); click → `openSong`; "+ import" → tauri dialog open (audio filter: mp3 flac ogg wav m4a) → `importSong(path)` → auto-open.
- **Sections** (right panel, tab 1): ordered list; add (name + start/end prefilled from selection), edit times inline, delete, reorder (↑↓ buttons); save = `section.replace` with the whole lane (server re-derives junction loops — refresh loops store after).
- **Loops** (tab 2): list manual + junction loops (junction badged `J`); click → `loop.set` transport + select; rename inline (loop.update); delete; "derive junctions" button with tail/head inputs (defaults 2.0).
- **PlanBuilder** (tab 3): steps list builder over the open song's loops. Add-step chooser: Listen first (loop, reps), Play reps (loop, reps, curve: dwell rate | ladder start/step/target | oscillate low/high/period), Rotation (multi-select loops, rounds, reps/visit, curve), Recall test (loop, alternations, rate). Reorder/delete steps. Name + save → `plan.save`. Existing plans listed with "▶ start".
- Wire panels into `App.svelte` right rail with the 120 ms fade between tabs.

A "suggested plan" button: given selected loops A,B,…: generates the evidence-based default — Listen(A,2) → Play(A,4, oscillate 0.7/1.0/3) → repeat per loop → Rotation(all, 2 rounds, 2/visit, dwell 0.85) → Recall(first loop, 2) — one click from sections to a research-shaped session.

- [ ] **Step 2: Verify + commit**

Run: `pnpm build` — clean.

```bash
git add -A && git commit -m "feat(desktop): library, sections, loops and plan builder panels"
```

---

### Task 7: Plan runner UI + ratings + due panel

**Files:**
- Create: `apps/desktop/src/components/PlanRunner.svelte`, `DuePanel.svelte`

- [ ] **Step 1: Implement**

- **PlanRunner** (replaces right rail while a plan is active): big current-mode word — `LISTEN` / `PLAY` / `FROM MEMORY` (accent color for recall) — loop name, rate %, `rep i/n · step j/k`; controls: skip step, stop. On `step_finished {loop_id}`: show an inline rating prompt "How was <loop>? 1 Miss · 2 Shaky · 3 Solid" → `rep.rate {loop_id, rating}` (not retest). On `plan_finished`: session summary — reps done, steps, then for each loop practiced a rating prompt flagged `is_retest: false`… **retest semantics:** the *first* rep of a loop that `due.list` contained today is the retention probe — when a plan starts, fetch `due.list`; loops in it get their end-of-step rating sent with `is_retest: true`. Keep that rule in `stores.ts`.
- **DuePanel** (right rail, tab 4 + shown on app start): "Due today" list from `due.list` (loop name, song); click → opens song, sets loop; retention table from `retention {song_id}` for the open song (loop / last retest rating / when) — ratings colored Miss red, Shaky amber, Solid green. One quiet line under it: "rotating sections and next-day retests feel worse and work better."
- No per-rep correctness meters anywhere — summary at step/plan end only (spec's faded-feedback principle).

- [ ] **Step 2: Verify + commit**

Run: `pnpm vitest run && pnpm build` — green.

```bash
git add -A && git commit -m "feat(desktop): plan runner with ratings, due panel, retention view"
```

---

### Task 8: Full gate + launch script + desktop entry

**Files:**
- Create: `apps/desktop/src-tauri/` release config as needed
- Create: `~/.local/share/applications/earworm.desktop` (Exec=the built binary) — follow the user's conventions: extensionless scripts, XDG paths

- [ ] **Step 1: Full build**

Run: `pnpm tauri build --debug` (debug avoids long LTO; produces `src-tauri/target/.../earworm` — workspace target dir: `target/debug/earworm`).
Expected: bundle builds. Then `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt && pnpm vitest run`.

- [ ] **Step 2: Smoke run**

Launch the binary with a 5 s timeout under the current session (`timeout 5 target/debug/earworm`; Wayland/Hyprland session is live). Verify: process starts, no panic on stderr, socket `$XDG_RUNTIME_DIR/earworm.sock` exists while running, and `song.list` over the socket answers while the UI is up (proves UI+socket share one App).

- [ ] **Step 3: Desktop entry**

`earworm.desktop` with Exec pointing at the release binary path (build release: `pnpm tauri build`), icon from the generated icons dir, `Categories=AudioVideo;Audio;`.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(desktop): build gate, smoke run, desktop entry"
```

---

## Self-review checklist

- Spec coverage: canvas waveform with drag-to-loop/zoom/section lane ✔, client-side playhead from timebase ✔, panels (library/sections/loops/plan builder/runner/journal-due) ✔, keyboard-first ✔, spare visual language ✔, no live correctness HUD (faded feedback) ✔, bass-focus one-key ✔, suggested evidence-based plan ✔, UI and socket share one App ✔.
- New server surface added here: `loop.update` (+ test), possible `serve` pump refactor (keep tests green).
- Deferred: v2 capture, v3 stems.
