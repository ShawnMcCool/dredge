# macOS Port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Earworm build and run on macOS (Apple Silicon) for the load-a-file practice workflow — playback, looping/stretch, tuner, stem separation, and structure analysis — by abstracting the PipeWire-specific audio I/O behind a cross-platform backend and scrubbing the system-audio capture feature.

**Architecture:** The DSP core (`pipeline`, `looper`, `stretch`, `filter`, `export`, `decode`) is already OS-agnostic. PipeWire touches only three engine files (`output.rs`, `capture.rs`, deps). We extract a platform-agnostic `RenderCore` that both a PipeWire (Linux) and a cpal (macOS/other) backend drive, `cfg`-gate the PipeWire crates to Linux, delete the system-audio capture feature end-to-end, and fix three small macOS shims in the out-of-process Python analysis path.

**Tech Stack:** Rust (engine/server crates), `cpal` 0.15 (CoreAudio/cross-platform audio I/O), `pipewire`/`libspa` 0.10 (Linux only, now `cfg`-gated), Tauri + Svelte 5 (desktop), Python (`torch`/Demucs/beat_this/SongFormer) out-of-process.

---

## Status — updated 2026-06-16

Executed subagent-driven on `main` (per repo convention; commits local, not
pushed). Every Rust task passed independent spec + code-quality review; the full
Linux gate (`just check`) is green.

| Task | State | Commit(s) |
|------|-------|-----------|
| 1 — deps + `Error::Audio` | ✅ done | `a6f3b2b` |
| 2 — extract `RenderCore` | ✅ done | `693f11f` |
| 3 — cpal output backend | ✅ done | `0a35a44` |
| 4 — scrub capture + grab-back (D1) | ✅ done | `a08698a`, `b14ae7e` (doc), `2d0d7e5` (fmt) |
| — lockfile for cpal deps | ✅ done | `1ad0dc1` |
| 5 — cpal input backend (tuner) | ✅ done | `c9adedd`, `f73dfe7` (doc) |
| 6 — Python MPS device branch | ✅ done | `606be4a` |
| 7 — bash `readlink` portability | ✅ done | `05d69d8` |
| 8 — `die_with_parent` macOS shim | ✅ done | `78d238a` |
| 9 — packaging | ◐ **partial** | `e711a40` (Linux-doable parts) |

**Task 9 is PARKED — no Apple hardware available.** The config-only parts that
are Linux-safe are committed (Tauri `app`/`dmg` targets, `bundle.macOS`,
`Info.plist` mic entitlement, `_pw_thread`→`_audio_thread` rename, neutral
`build.rs` message, park-loop comment). Everything that needs a real Mac is
outstanding and **unverified**: the cpal/CoreAudio code has never been compiled
(coreaudio-sys needs the macOS SDK). When hardware is available, resume at
**Task 9 Step 1** and work the carried cleanups (Step 7) + the follow-ups below.
Setup is captured in `docs/macos-build.md`.

---

## Key Decisions (review before executing)

**D1 — Scrub system-audio capture + grab-back entirely (both platforms).** The user authorized removing the capture tab and grab-back ("I'd be fine scrubbing that from the app"). This deletes `list_output_streams`, `capture_control.rs`, the `capture.*` socket commands, `Capture.svelte`, and the capture tab. **The tuner's *input* (mic/instrument) capture is kept** — it ports cleanly to CoreAudio. This is the only destructive/irreversible part of the plan; if you'd rather keep capture Linux-only behind `cfg`, stop and re-scope before Task 4.

**D2 — cpal for non-Linux, keep PipeWire on Linux.** Selected by `#[cfg(target_os = "linux")]`. The tuned Linux PipeWire path (RT_PROCESS, monitor latency) is left untouched to avoid regressing the primary platform; macOS gets a fresh cpal backend. Both expose the identical `output::spawn(...)` / `capture::{list_input_sources, start_capture}` signatures, so `engine.rs`, `tuner.rs`, and the server are unchanged. A shared `RenderCore` holds the song-swap + command-drain + render logic so neither backend duplicates it.

**D3 — Work on `main`, no worktree.** Per repo convention (commit directly on main; never auto-push). Commit after each task.

**D4 — CPU-correct first, MPS as opt-in.** The Python models run on CPU on macOS out of the box; the MPS (Metal) branch is an optimization whose op-coverage must be verified per model. Ship CPU-correct, prefer MPS where it proves stable.

---

## File Structure

**Engine (`crates/engine`):**
- Create `src/render_core.rs` — platform-agnostic per-callback logic (song-swap, command drain, render, event emit). Owned by both backends.
- Create `src/output_cpal.rs` — cpal output backend; `output::spawn` for non-Linux.
- Modify `src/output.rs` — PipeWire output, now Linux-only; delegates per-callback work to `RenderCore`.
- Create `src/capture_cpal.rs` — cpal input enumeration + capture; non-Linux.
- Modify `src/capture.rs` — keep shared types (`CaptureNode`, `CaptureSession`, `write_wav`, `wav_header_rate`); `cfg`-gate the PipeWire impls; **delete** `list_output_streams`/`scan_output_streams`.
- Modify `src/lib.rs` — `cfg`-select the `output`/capture impls; declare `render_core`.
- Modify `Cargo.toml` — add `cpal`; move `pipewire`/`libspa` under `[target.'cfg(target_os = "linux")'.dependencies]`.
- Modify `src/error.rs` — add `Audio(String)` variant.
- Modify `examples/play.rs` — `cfg`-gate or drop PipeWire-specific bits if any.

**Server (`crates/server`):**
- Delete `src/capture_control.rs`.
- Modify `src/app.rs` — remove `capture.nodes/start/stop/status/grab` commands and their helpers; keep `tuner.*` and `export.*`.
- Modify `src/stems.rs` — `cfg`-split `die_with_parent` (Linux `PR_SET_PDEATHSIG`; macOS shim).
- Modify `src/lib.rs` (or `mod.rs`) — drop `capture_control` module.

**Frontend (`apps/desktop`):**
- Delete `src/components/Capture.svelte`.
- Modify `src/App.svelte` — remove `capture` from `ALL_TABS`, `TAB_VIEWS`, imports.
- Modify `src/lib/stores.ts` — remove capture state/actions.
- Modify `src/components/Guide.svelte`, `src/components/SettingsPanel.svelte` — remove capture references.
- Modify `src-tauri/tauri.conf.json` + bundle config — macOS bundle target, entitlements (microphone for tuner).

**Python (`scripts/`):**
- Modify `analyze_impl.py`, `songformer_impl.py` — MPS device branch.
- Modify `analyze`, `earworm-enable-ml` — `readlink -f` portability; macOS impl-search path.

---

## Task 1: Add `Audio` error variant + `cpal` dependency

**Files:**
- Modify: `crates/engine/src/error.rs:1-11`
- Modify: `crates/engine/Cargo.toml:6-14`

- [ ] **Step 1: Add the error variant**

In `crates/engine/src/error.rs`, add to the `Error` enum:

```rust
    #[error("audio error: {0}")]
    Audio(String),
```

- [ ] **Step 2: Restructure dependencies**

In `crates/engine/Cargo.toml`, remove the two unconditional lines:

```toml
pipewire = { version = "0.10", features = ["v0_3_49"] }
libspa = "0.10"
```

Add `cpal` to `[dependencies]`:

```toml
cpal = "0.15"
```

Add a new target table after `[dependencies]` (before `[dev-dependencies]`):

```toml
[target.'cfg(target_os = "linux")'.dependencies]
pipewire = { version = "0.10", features = ["v0_3_49"] }
libspa = "0.10"
```

- [ ] **Step 3: Verify it still builds on Linux**

Run: `cargo build -p engine`
Expected: PASS (pipewire still present on Linux; cpal compiles).

- [ ] **Step 4: Commit**

```bash
git add crates/engine/src/error.rs crates/engine/Cargo.toml
git commit -m "build(engine): cfg-gate pipewire to linux, add cpal + Audio error"
```

---

## Task 2: Extract `RenderCore` from the output callback

This is a pure refactor — no behavior change. It moves the song-swap / command-drain / render logic out of `output.rs` so both backends share it.

**Files:**
- Create: `crates/engine/src/render_core.rs`
- Modify: `crates/engine/src/lib.rs:3-18`
- Modify: `crates/engine/src/output.rs:7-164`

- [ ] **Step 1: Create the shared core**

Create `crates/engine/src/render_core.rs`:

```rust
//! Platform-agnostic audio render core. Both the PipeWire (Linux) and cpal
//! (non-Linux) output backends drive this: it detects song swaps, drains the
//! command ring into the pipeline, renders one interleaved-stereo block, and
//! pushes engine events out. Never allocates or locks on the steady path.

use crate::buffer::StemSet;
use crate::pipeline::{EngineCmd, EngineEvent, Pipeline};
use arc_swap::ArcSwapOption;
use std::sync::Arc;

pub struct RenderCore {
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<StemSet>>,
    pipeline: Option<Pipeline>,
    current_song: Option<Arc<StemSet>>,
    events: Vec<EngineEvent>,
    /// User volume, held here (not just in the Pipeline) so it survives song
    /// swaps and a SetVolume that arrives before any song is loaded.
    volume: f32,
}

impl RenderCore {
    pub fn new(
        cmd_rx: rtrb::Consumer<EngineCmd>,
        evt_tx: rtrb::Producer<EngineEvent>,
        song_slot: Arc<ArcSwapOption<StemSet>>,
    ) -> Self {
        Self {
            cmd_rx,
            evt_tx,
            song_slot,
            pipeline: None,
            current_song: None,
            events: Vec::with_capacity(64),
            volume: 1.0,
        }
    }

    /// Render `out.len() / CHANNELS` interleaved stereo frames into `out`.
    pub fn fill(&mut self, out: &mut [f32]) {
        // Song swap detection: compare the slot against the buffer the current
        // pipeline was built from. `load()` gives a guard (no refcount clone)
        // for the common no-swap path; only clone the Arc out on an actual swap.
        let guard = self.song_slot.load();
        let swapped = match (guard.as_ref(), self.current_song.as_ref()) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        };
        if swapped {
            let song = (*guard).clone();
            // Seed the fresh pipeline with the current user volume so swaps
            // don't reset it to the Pipeline default.
            self.pipeline = song.clone().map(|s| {
                let mut p = Pipeline::new((*s).clone());
                p.apply(EngineCmd::SetVolume(self.volume));
                p
            });
            self.current_song = song;
        }

        // Drain control commands. SetVolume is latched into self.volume so it
        // persists across song swaps and survives arriving before any pipeline.
        while let Ok(cmd) = self.cmd_rx.pop() {
            if let EngineCmd::SetVolume(v) = cmd {
                self.volume = v;
            }
            if let Some(p) = self.pipeline.as_mut() {
                p.apply(cmd);
            }
        }

        match self.pipeline.as_mut() {
            Some(p) => {
                self.events.clear();
                p.render(out, &mut self.events);
                for ev in self.events.drain(..) {
                    let _ = self.evt_tx.push(ev); // drop on full
                }
            }
            None => out.fill(0.0),
        }
    }
}
```

- [ ] **Step 2: Declare the module**

In `crates/engine/src/lib.rs`, add alongside the other `pub mod` lines (keep alphabetical-ish grouping):

```rust
pub mod render_core;
```

- [ ] **Step 3: Rewrite `output.rs` to delegate to `RenderCore`**

In `crates/engine/src/output.rs`, replace the `State` struct (lines 23-34) with:

```rust
struct State {
    core: crate::render_core::RenderCore,
    render_buf: Vec<f32>,
}
```

Replace the `let state = State { ... };` block (lines 75-84) with:

```rust
    let state = State {
        core: crate::render_core::RenderCore::new(cmd_rx, evt_tx, song_slot),
        render_buf: vec![0.0; MAX_QUANTUM_FRAMES * CHANNELS],
    };
```

Replace the body of the `.process(...)` closure that currently does swap-detect /
command-drain / render (lines 92-156, i.e. everything from `// Song swap detection`
down to the `n_frames` produced by the `if let Some(slice)` block) so the render
step calls the core. The closure becomes:

```rust
        .process(|stream, state| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };

            let stride = std::mem::size_of::<f32>() * CHANNELS;
            let requested = buffer.requested() as usize;
            let datas = buffer.datas_mut();
            let data = &mut datas[0];
            let n_frames = if let Some(slice) = data.data() {
                let mut n_frames = (slice.len() / stride).min(MAX_QUANTUM_FRAMES);
                if requested > 0 {
                    n_frames = n_frames.min(requested);
                }
                let out = &mut state.render_buf[..n_frames * CHANNELS];
                state.core.fill(out);
                // F32LE device buffer + little-endian host (asserted at module
                // load): render_buf bytes are already in destination layout.
                let bytes: &[u8] = bytemuck::cast_slice(&out[..]);
                slice[..bytes.len()].copy_from_slice(bytes);
                n_frames
            } else {
                0
            };
            let chunk = data.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = stride as _;
            *chunk.size_mut() = (stride * n_frames) as _;
        })
```

Remove the now-unused imports at the top of `output.rs`: the
`use crate::pipeline::{EngineCmd, EngineEvent, Pipeline};` line and
`use arc_swap::ArcSwapOption;` (the closure no longer references them directly;
`run`'s signature still needs `EngineCmd`/`EngineEvent`/`ArcSwapOption` for its
parameters, so keep those types imported — adjust to
`use crate::pipeline::{EngineCmd, EngineEvent};` and keep `use arc_swap::ArcSwapOption;`).
Keep `use crate::buffer::{StemSet, CHANNELS, SAMPLE_RATE};`.

- [ ] **Step 4: Build + run existing tests (behavior unchanged)**

Run: `cargo build -p engine && cargo test -p engine`
Expected: PASS. The refactor is behavior-preserving; existing engine tests stay green.

- [ ] **Step 5: Smoke-test playback on Linux**

Run: `just dev`, load a song, press play.
Expected: audio plays exactly as before; volume and song-swap behave normally.

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/render_core.rs crates/engine/src/lib.rs crates/engine/src/output.rs
git commit -m "refactor(engine): extract RenderCore shared by output backends"
```

---

## Task 3: cpal output backend + `cfg`-select it

**Files:**
- Create: `crates/engine/src/output_cpal.rs`
- Modify: `crates/engine/src/lib.rs` (the `output` module declaration)

- [ ] **Step 1: Write the cpal backend**

Create `crates/engine/src/output_cpal.rs`:

```rust
//! cpal output backend (non-Linux, e.g. CoreAudio on macOS). Mirrors the
//! PipeWire backend: a dedicated thread owns a cpal output stream whose data
//! callback drives the shared `RenderCore`. The thread parks to keep the
//! stream (which is `!Send` on some hosts) alive and on one thread.

use crate::buffer::{CHANNELS, SAMPLE_RATE};
use crate::error::Error;
use crate::pipeline::{EngineCmd, EngineEvent};
use crate::render_core::RenderCore;
use arc_swap::ArcSwapOption;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::Arc;
use std::thread::JoinHandle;

pub fn spawn(
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<crate::buffer::StemSet>>,
) -> crate::error::Result<JoinHandle<()>> {
    let handle = std::thread::Builder::new()
        .name("earworm-audio".into())
        .spawn(move || {
            if let Err(e) = run(cmd_rx, evt_tx, song_slot) {
                eprintln!("earworm audio thread failed: {e}");
            }
        })?;
    Ok(handle)
}

fn run(
    cmd_rx: rtrb::Consumer<EngineCmd>,
    evt_tx: rtrb::Producer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<crate::buffer::StemSet>>,
) -> crate::error::Result<()> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or_else(|| Error::Audio("no default output device".into()))?;

    // Request the engine's native format (48 kHz stereo f32). CoreAudio
    // devices support 48 kHz; if a host doesn't, build_output_stream errors
    // and we surface it rather than silently resampling (see follow-up task).
    let config = cpal::StreamConfig {
        channels: CHANNELS as u16,
        sample_rate: cpal::SampleRate(SAMPLE_RATE),
        buffer_size: cpal::BufferSize::Default,
    };

    let mut core = RenderCore::new(cmd_rx, evt_tx, song_slot);

    let stream = device
        .build_output_stream(
            &config,
            move |out: &mut [f32], _: &cpal::OutputCallbackInfo| {
                // cpal hands us an interleaved f32 buffer sized to the device
                // request; RenderCore fills exactly out.len() samples.
                core.fill(out);
            },
            move |err| eprintln!("earworm cpal stream error: {err}"),
            None,
        )
        .map_err(|e| Error::Audio(format!("build output stream: {e}")))?;

    stream
        .play()
        .map_err(|e| Error::Audio(format!("play stream: {e}")))?;

    // The Engine owns this JoinHandle and never joins it; park forever so the
    // stream stays alive on this thread.
    loop {
        std::thread::park();
    }
}
```

- [ ] **Step 2: `cfg`-select the output module in `lib.rs`**

In `crates/engine/src/lib.rs`, replace the line `pub mod output;` with:

```rust
#[cfg(target_os = "linux")]
pub mod output;
#[cfg(not(target_os = "linux"))]
#[path = "output_cpal.rs"]
pub mod output;
```

(`engine.rs` keeps calling `crate::output::spawn(...)` unchanged.)

- [ ] **Step 3: Verify the Linux build is unaffected**

Run: `cargo build -p engine && cargo test -p engine`
Expected: PASS (on Linux, `output_cpal.rs` is not compiled into the `output` module; cpal still compiles as a dep but is unused — that is fine).

- [ ] **Step 4: Verify the non-Linux code compiles (cross-check)**

Run: `cargo build -p engine --target aarch64-apple-darwin 2>&1 | head -40`
Expected: if the macOS target/toolchain isn't installed locally this errors at the linker/target-not-found stage — that's acceptable here; the goal is that no *engine* source under `cfg(not(target_os="linux"))` has a type/name error. If you have a Mac, build there instead. Record the outcome.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/output_cpal.rs crates/engine/src/lib.rs
git commit -m "feat(engine): cpal output backend for non-linux hosts"
```

---

## Task 4: Scrub system-audio capture (engine + server + frontend)

Implements **D1**. Deletes the capture tab, grab-back, and system-audio
enumeration; keeps the tuner's input capture path.

**Files:**
- Modify: `crates/engine/src/capture.rs:35-104` (delete `list_output_streams` + `scan_output_streams`)
- Delete: `crates/server/src/capture_control.rs`
- Modify: `crates/server/src/app.rs` (remove `capture.*` commands + helpers)
- Modify: `crates/server/src/lib.rs` (drop `capture_control` module)
- Delete: `apps/desktop/src/components/Capture.svelte`
- Modify: `apps/desktop/src/App.svelte`, `src/lib/stores.ts`, `src/components/Guide.svelte`, `src/components/SettingsPanel.svelte`

- [ ] **Step 1: Delete the system-audio enumeration from the engine**

In `crates/engine/src/capture.rs`, delete `pub fn list_output_streams()`
(lines ~38-46) and `fn scan_output_streams()` (lines ~48-104). Leave
`list_input_sources`, `scan_input_sources`, `start_capture`, `run_capture`,
`CaptureNode`, `CaptureSession`, `RollingRing` usage, `write_wav`, and
`wav_header_rate` intact.

- [ ] **Step 2: Remove the capture commands from the dispatcher**

In `crates/server/src/app.rs`, delete the match arms for `capture.nodes`,
`capture.start`, `capture.stop`, `capture.status`, and `capture.grab`
(lines ~453-460), the `"capture.grab" => grab_phased(...)` phased arm
(line ~53), and their helper fns: `grab_phased`, `capture_start`,
`capture_status`, `capture_grab`, and the grab phase structs/helpers
(the `capture.grab` lock-phase product around lines ~256-305 and ~1136-1141).
Remove the `self.capture` field and its construction, and the `use` of
`capture_control`. Keep all `tuner.*` and `export.*` arms.

- [ ] **Step 3: Delete `capture_control.rs` and its module declaration**

```bash
git rm crates/server/src/capture_control.rs
```

Remove `mod capture_control;` / `pub mod capture_control;` from
`crates/server/src/lib.rs`.

- [ ] **Step 4: Build the server**

Run: `cargo build -p server`
Expected: PASS. If the compiler flags a leftover `capture` reference, delete it.

- [ ] **Step 5: Run server tests**

Run: `cargo test -p server`
Expected: PASS. Delete any capture-specific tests that no longer apply
(e.g. grab snapshot tests); keep tuner/export/stems tests.

- [ ] **Step 6: Remove the capture tab from the frontend**

```bash
git rm apps/desktop/src/components/Capture.svelte
```

In `apps/desktop/src/App.svelte`:
- Remove `import Capture from "./components/Capture.svelte";` (line ~4).
- Remove `"capture"` from `ALL_TABS` (line ~34).
- Remove the `capture: Capture,` entry from `TAB_VIEWS` (line ~40).

In `apps/desktop/src/lib/stores.ts`, remove capture-related state and actions
(the capture node list, capture status, grab action — whatever the capture
component imported). In `Guide.svelte` and `SettingsPanel.svelte`, remove
capture references (help text / settings rows).

- [ ] **Step 7: Typecheck + frontend tests**

Run: `cd apps/desktop && pnpm svelte-check && pnpm vitest run`
Expected: PASS, no dangling imports or references to capture.

- [ ] **Step 8: Smoke-test on Linux**

Run: `just dev`. Confirm the panel tabs are structure/loops/export/profile/settings/guide
(no capture tab), the tuner still works, and playback is unaffected.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "feat: remove system-audio capture + grab-back (keep tuner input)"
```

---

## Task 5: cpal input backend for the tuner

The tuner calls `engine::capture::list_input_sources()` and
`engine::capture::start_capture(node, RING_SECS)`. Provide cpal implementations
for non-Linux that keep the exact same signatures, so `tuner.rs` is untouched.

**Files:**
- Create: `crates/engine/src/capture_cpal.rs`
- Modify: `crates/engine/src/capture.rs` (split: shared types vs. PipeWire impls)
- Modify: `crates/engine/src/lib.rs` (the `capture` module wiring)

- [ ] **Step 1: `cfg`-gate the PipeWire capture impls**

In `crates/engine/src/capture.rs`, gate the PipeWire-specific items with
`#[cfg(target_os = "linux")]`: the `use pipewire ...` imports (lines ~10-12),
`pw_err`, `list_input_sources`, `scan_input_sources`, `start_capture`,
`run_capture`, `CapState`, and the `pw` references inside `Drop`/`CaptureSession`
shutdown if any. Leave **shared, platform-agnostic** items un-gated:
`CaptureNode`, `CaptureSession` (the struct: `ring`, `node`, `stop`, `thread`),
its `stop`/`shutdown`/`Drop` impls, `write_wav`, `wav_header_rate`, and the
`RollingRing`/`std` imports they need.

Note: `CaptureSession` and its thread/stop/Drop machinery are already
platform-neutral (a `JoinHandle` + `AtomicBool`), so both backends reuse them.

- [ ] **Step 2: Write the cpal input backend**

Create `crates/engine/src/capture_cpal.rs`:

```rust
//! cpal input capture (non-Linux). Enumerates input devices and taps the
//! chosen one into a rolling ring — the tuner's source on macOS. A dedicated
//! thread owns the cpal input stream (kept on one thread; parks until stopped),
//! mirroring the PipeWire capture thread model so `CaptureSession` is shared.

use crate::buffer::{CHANNELS, SAMPLE_RATE};
use crate::capture::{CaptureNode, CaptureSession};
use crate::error::Error;
use crate::ring::RollingRing;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Enumerate input devices. `serial` carries the device index (used by
/// `start_capture` to re-select it); `app` is the device name.
pub fn list_input_sources() -> crate::error::Result<Vec<CaptureNode>> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| Error::Audio(format!("enumerate input devices: {e}")))?;
    let mut out = Vec::new();
    for (idx, dev) in devices.enumerate() {
        let name = dev.name().unwrap_or_else(|_| format!("input {idx}"));
        out.push(CaptureNode {
            id: idx as u32,
            serial: idx as u64,
            app: name,
            media: String::new(),
        });
    }
    Ok(out)
}

/// Tap the chosen input device (`node.serial` == device index) into a rolling
/// ring of `buffer_secs`.
pub fn start_capture(node: CaptureNode, buffer_secs: f64) -> crate::error::Result<CaptureSession> {
    let ring = Arc::new(Mutex::new(RollingRing::with_secs(buffer_secs)));
    let stop = Arc::new(AtomicBool::new(false));
    let thread = {
        let ring = ring.clone();
        let stop = stop.clone();
        let target = node.serial as usize;
        std::thread::Builder::new()
            .name("earworm-cap".into())
            .spawn(move || {
                if let Err(e) = run_capture(target, ring, stop) {
                    eprintln!("earworm capture thread failed: {e}");
                }
            })?
    };
    Ok(CaptureSession::from_parts(ring, node, stop, thread))
}

fn run_capture(
    target: usize,
    ring: Arc<Mutex<RollingRing>>,
    stop: Arc<AtomicBool>,
) -> crate::error::Result<()> {
    let host = cpal::default_host();
    let device = host
        .input_devices()
        .map_err(|e| Error::Audio(format!("enumerate input devices: {e}")))?
        .nth(target)
        .or_else(|| host.default_input_device())
        .ok_or_else(|| Error::Audio("no input device".into()))?;

    let config = cpal::StreamConfig {
        channels: CHANNELS as u16,
        sample_rate: cpal::SampleRate(SAMPLE_RATE),
        buffer_size: cpal::BufferSize::Default,
    };

    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                // Non-blocking like the PipeWire path: drop a buffer on the
                // rare contention rather than risk an xrun on the audio thread.
                if let Ok(mut r) = ring.try_lock() {
                    r.push(data);
                }
            },
            move |err| eprintln!("earworm cpal capture error: {err}"),
            None,
        )
        .map_err(|e| Error::Audio(format!("build input stream: {e}")))?;

    stream
        .play()
        .map_err(|e| Error::Audio(format!("play input stream: {e}")))?;

    while !stop.load(Ordering::Relaxed) {
        std::thread::park_timeout(Duration::from_millis(100));
    }
    Ok(())
}
```

- [ ] **Step 3: Add a `CaptureSession` constructor for backends**

The cpal backend builds a `CaptureSession` from parts. In
`crates/engine/src/capture.rs`, add (un-gated, near the `CaptureSession`
struct) a constructor so backend modules don't touch private fields:

```rust
impl CaptureSession {
    /// Assemble a session from a backend's ring/stop/thread. Backend-agnostic.
    pub(crate) fn from_parts(
        ring: std::sync::Arc<std::sync::Mutex<crate::ring::RollingRing>>,
        node: CaptureNode,
        stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
        thread: std::thread::JoinHandle<()>,
    ) -> Self {
        Self {
            ring,
            node,
            stop,
            thread: Some(thread),
        }
    }
}
```

Then refactor the Linux `start_capture` (in `capture.rs`) to build its session
via `CaptureSession::from_parts(...)` too, so both backends use one constructor
(DRY). Confirm `CaptureSession`'s fields are exactly `ring`, `node`, `stop`,
`thread: Option<JoinHandle<()>>` (per capture.rs:179-184); adjust the
constructor if the field set differs.

- [ ] **Step 4: `cfg`-select the input-capture functions**

In `crates/engine/src/lib.rs`, after `pub mod capture;`, add the cpal module and
re-export so callers keep using `engine::capture::{list_input_sources, start_capture}`:

```rust
#[cfg(not(target_os = "linux"))]
#[path = "capture_cpal.rs"]
mod capture_cpal;
```

Then in `crates/engine/src/capture.rs`, at the bottom, re-export the cpal impls
under the `capture` namespace for non-Linux:

```rust
#[cfg(not(target_os = "linux"))]
pub use crate::capture_cpal::{list_input_sources, start_capture};
```

(Confirm `RollingRing` exposes `with_secs` and `push(&[f32])` — it does, per the
PipeWire path's `RollingRing::with_secs` and `ring.push(&state.scratch)` at
capture.rs:189,295.)

- [ ] **Step 5: Build + test on Linux**

Run: `cargo build -p engine && cargo build -p server && cargo test -p engine -p server`
Expected: PASS — Linux still uses the PipeWire `list_input_sources`/`start_capture`;
the cpal module is excluded.

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/capture_cpal.rs crates/engine/src/capture.rs crates/engine/src/lib.rs
git commit -m "feat(engine): cpal input-capture backend for the tuner (non-linux)"
```

---

## Task 6: Python device selection — MPS branch

CPU is correct today on macOS; this adds the Metal fast path. Defensive
(`getattr`) so it's a no-op on builds of torch without MPS.

**Files:**
- Modify: `scripts/analyze_impl.py:59`
- Modify: `scripts/songformer_impl.py:43`

- [ ] **Step 1: Add a shared device helper to `analyze_impl.py`**

In `scripts/analyze_impl.py`, replace line 59
(`device = "cuda" if torch.cuda.is_available() else "cpu"`) with:

```python
    if torch.cuda.is_available():
        device = "cuda"
    elif getattr(torch.backends, "mps", None) is not None and torch.backends.mps.is_available():
        device = "mps"
    else:
        device = "cpu"
```

- [ ] **Step 2: Same change in `songformer_impl.py`**

In `scripts/songformer_impl.py`, replace line 43
(`device = "cuda" if torch.cuda.is_available() else "cpu"`) with the identical
block from Step 1.

- [ ] **Step 3: Verify the scripts still parse**

Run: `python3 -c "import ast; ast.parse(open('scripts/analyze_impl.py').read()); ast.parse(open('scripts/songformer_impl.py').read()); print('ok')"`
Expected: `ok`.

- [ ] **Step 4: Note the verification gap**

MPS op-coverage for beat_this / SongFormer is unverified. On the first Mac run,
if a model raises a `NotImplementedError` for an MPS op, set the env override
`PYTORCH_ENABLE_MPS_FALLBACK=1` (CPU fallback per-op) or force `device="cpu"`
for that model. Record which models run clean on MPS. **This is expected
follow-up, not a blocker** (per decision D4).

- [ ] **Step 5: Commit**

```bash
git add scripts/analyze_impl.py scripts/songformer_impl.py
git commit -m "feat(analyze): prefer MPS device on apple silicon, fall back to cpu"
```

---

## Task 7: Bash wrapper portability (`readlink -f`, impl search path)

**Files:**
- Modify: `scripts/analyze:13`
- Modify: `scripts/analyze:28-32` (impl search dirs)

- [ ] **Step 1: Replace BSD-incompatible `readlink -f`**

In `scripts/analyze`, replace line 13
(`HERE="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"`) with a portable
resolver that works on macOS (BSD `readlink` lacks `-f`):

```bash
# Portable absolute-dir of this script (BSD readlink has no -f).
SOURCE="$0"
while [ -h "$SOURCE" ]; do
  DIR="$(cd -P "$(dirname "$SOURCE")" && pwd)"
  SOURCE="$(readlink "$SOURCE")"
  case "$SOURCE" in /*) ;; *) SOURCE="$DIR/$SOURCE" ;; esac
done
HERE="$(cd -P "$(dirname "$SOURCE")" && pwd)"
```

- [ ] **Step 2: Add macOS bundle/Homebrew impl-search dirs**

In `scripts/analyze`, extend the impl-search loop (line ~30,
`for d in "$HERE" /usr/lib/earworm /usr/local/lib/earworm; do`) to include
Homebrew and an env override:

```bash
for d in "$HERE" "${EARWORM_IMPL_DIR:-}" /usr/lib/earworm /usr/local/lib/earworm /opt/homebrew/lib/earworm; do
  [ -n "$d" ] || continue
  if [ -f "$d/analyze_impl.py" ]; then IMPL="$d/analyze_impl.py"; break; fi
done
```

- [ ] **Step 3: Verify the wrapper still runs on Linux**

Run: `bash -n scripts/analyze && echo "syntax ok"`
Then (if a venv exists) a real invocation on a short clip to confirm `HERE`
still resolves: `scripts/analyze <some-test.wav> >/dev/null` and confirm one
JSON object on stdout.
Expected: `syntax ok`; analysis runs as before.

- [ ] **Step 4: Commit**

```bash
git add scripts/analyze
git commit -m "fix(analyze): portable script-dir resolution + macos impl paths"
```

---

## Task 8: macOS shim for `die_with_parent`

`PR_SET_PDEATHSIG` is Linux-only. Provide a `cfg`-split so the server builds on
macOS; the macOS variant is a documented best-effort no-op (orphaned analyzer/
Demucs on abrupt parent exit is acceptable — they're short-lived and the user
isn't capturing).

**Files:**
- Modify: `crates/server/src/stems.rs:125-146`

- [ ] **Step 1: `cfg`-gate the Linux implementation**

In `crates/server/src/stems.rs`, add `#[cfg(target_os = "linux")]` above the
existing `die_with_parent` (line ~129) and keep its body as-is.

- [ ] **Step 2: Add the non-Linux variant**

Immediately after, add:

```rust
/// macOS / other: PR_SET_PDEATHSIG has no portable equivalent. Best-effort
/// no-op — a child orphaned by an abrupt parent exit is acceptable here
/// (analyzer/Demucs runs are short-lived and bounded).
#[cfg(not(target_os = "linux"))]
pub(crate) fn die_with_parent(_cmd: &mut std::process::Command) {}
```

- [ ] **Step 3: Verify Linux build + the stems CUDA test still pass**

Run: `cargo build -p server && cargo test -p server stems`
Expected: PASS (the `die_with_parent` Linux path and existing stems tests
unchanged).

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/stems.rs
git commit -m "fix(server): cfg-split die_with_parent for non-linux hosts"
```

---

## Task 9: macOS app packaging (Tauri bundle + entitlements)

This makes a runnable `.app`. Do this on a Mac (or CI macOS runner); the steps
are config-only here.

**Files:**
- Modify: `apps/desktop/src-tauri/tauri.conf.json`
- Possibly add: `apps/desktop/src-tauri/Entitlements.plist`
- Modify: `crates/engine/build.rs` (rubberband via Homebrew pkg-config — usually
  works unmodified; verify)

- [ ] **Step 1: Confirm Rubber Band resolves via Homebrew**

On the Mac: `brew install rubber-band pkg-config ffmpeg uv` then
`PKG_CONFIG_PATH="$(brew --prefix)/lib/pkgconfig" pkg-config --modversion rubberband`
Expected: prints a version. `crates/engine/build.rs` already probes `rubberband`
via `pkg_config`; if it isn't found, export `PKG_CONFIG_PATH` as above before
building (document this in the Mac build notes).

- [ ] **Step 2: Add macOS bundle config + microphone entitlement**

In `apps/desktop/src-tauri/tauri.conf.json`, ensure `bundle.targets` includes
`"app"`/`"dmg"` for macOS and add a microphone usage description (the tuner
needs input access) under the macOS bundle settings, e.g.:

```json
"bundle": {
  "macOS": {
    "minimumSystemVersion": "11.0"
  }
}
```

Add to the Info.plist (via Tauri's `bundle.macOS.infoPlist` or an
`Info.plist` fragment):

```xml
<key>NSMicrophoneUsageDescription</key>
<string>Earworm uses the microphone for the live tuner.</string>
```

- [ ] **Step 3: Ensure the analyze impls ship with/near the bundle**

The `analyze` wrapper resolves impls via `$EARWORM_IMPL_DIR` or the search dirs
from Task 7. For the `.app`, set `EARWORM_IMPL_DIR` (or place `analyze_impl.py`
+ `songformer_impl.py` in a resolved dir) so analysis works from the bundle.
Document this in the Mac build notes / README.

- [ ] **Step 4: Build the desktop app on macOS**

Run (on Mac): `just build` (or `pnpm tauri build` from `apps/desktop`).
Expected: produces `target/release/earworm` / `.app` with no compile errors.
The engine compiles the cpal backends; pipewire/libspa are absent (cfg-gated).

- [ ] **Step 5: Smoke-test the core workflow on macOS**

Launch the `.app`. Verify: load a file → waveform renders → play → loop a
section → slow it down (stretch) → tuner reads pitch from the mic → run analysis
(beats/sections appear) → run stem separation (stems box populates).
Expected: all work. Note any MPS op failures (Task 6, Step 4) and whether to
pin a model to CPU.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src-tauri/tauri.conf.json apps/desktop/src-tauri/Entitlements.plist
git commit -m "build(desktop): macos bundle config + microphone entitlement"
```

- [ ] **Step 7: Carried cleanup from earlier reviews (apply on the Mac, where they can be compiled)**

  - In `crates/engine/build.rs`, the `pkg_config` probe's `.expect(...)` message names `pacman`; make it platform-neutral (e.g. mention both `pacman -S rubberband` and `brew install rubber-band`) so the failure message isn't misleading on macOS.
  - In `crates/engine/src/engine.rs`, rename the field `_pw_thread` to `_audio_thread` (it holds the cpal thread on non-Linux now). One-word rename, no behavior change.
  - In `crates/engine/src/output_cpal.rs`, expand the park-loop comment to note `park()` may wake spuriously and re-parking is harmless; if `cargo clippy` on the Mac fires `clippy::infinite_loop`, add `#[allow(clippy::infinite_loop)]` with a one-line justification.
  - Commit these as `chore: macos cleanups carried from review`.

---

## Milestone Verification

- [ ] **Linux regression gate (run on Linux):** `just check`
  Expected: full test + lint suite green. Playback, tuner, analysis, stems,
  export all behave as before; the only user-visible change is the removed
  capture tab.

- [ ] **macOS acceptance (run on Mac):** the Task 9 Step 5 smoke test passes —
  load-a-file practice loop works end to end (playback, loop, stretch, tuner,
  analysis, stems, export).

- [ ] **Document the MPS outcome:** record in the README / Mac build notes which
  analysis models run on MPS vs. need CPU, plus the Homebrew deps
  (`rubber-band pkg-config ffmpeg uv`) and `EARWORM_IMPL_DIR`.

---

## Follow-ups (out of scope for this plan)

- **Sample-rate fallback:** if a CoreAudio device rejects 48 kHz, the cpal
  backends error rather than resample. `rubato` is already a dependency — add a
  resampling adapter in `output_cpal.rs` / `capture_cpal.rs` that queries
  `supported_output_configs()` and resamples when the device's rate ≠ 48 kHz.
- **Mono input-device negotiation (tuner).** `capture_cpal.rs` requests a 2-ch
  (stereo) input config, but many mics/interfaces are mono. On CoreAudio a
  built-in mic usually upmixes fine, but a strictly-mono device may make
  `build_input_stream` error (tuner won't start) or, on an unusual host, feed
  mono data into the stereo-assuming `RollingRing` and skew pitch detection.
  Same fix pattern as sample-rate fallback: query `default_input_config()`,
  handle the channel count, add a mono→stereo upmix shim in the callback.
  Surfaced in the Task 5 review. Validate on-device in Task 9 Step 5.
- **Output device hot-swap:** cpal doesn't auto-follow the default-device change
  the way PipeWire does; add device-change handling if users hit it.
- **Code signing / notarization** for distributing the `.app` outside your own
  machine.
- **MPS-stable model pinning** once Task 6 Step 4's results are known.
- **Audio-thread-death signalling (both backends).** Today, if the output
  stream errors *after* `play()` (device unplugged, runtime format collapse),
  the audio thread logs to stderr and the app keeps running silently with no
  signal to `Engine`. The cpal park loop is slightly worse than PipeWire here
  (it stays parked post-error). Add an `AtomicBool` status or an
  `EngineEvent::AudioError` so the UI can surface "audio device lost". Shared
  gap, not macOS-specific — surfaced in the Task 3 review.
