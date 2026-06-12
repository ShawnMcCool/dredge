# earworm v2 — Plan 5: PipeWire capture-anything

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Tap any application's audio (Spotify, Firefox, anything) into a rolling buffer, then "grab what just played" — snapshot the last N seconds to a WAV, import it as a song, and loop it. The workflow Spotify's SDK shutdown killed industry-wide, legitimately resurrected via PipeWire.

**Architecture:** `engine::ring` is a pure rolling buffer (TDD). `engine::capture` runs its own PipeWire thread per capture session: a capture stream targeting the chosen app node feeds the ring (ring behind a Mutex — capture path is not latency-critical). Node discovery is a short-lived registry scan. `server` gets a `CaptureControl` trait (mockable, like `AudioControl`) and `capture.*` commands; snapshot writes a WAV under `~/music/earworm-captures/` and funnels through the existing `song.import` path so hashing/sidecar/peaks all just work. UI gets a Capture tab.

**Tech Stack:** pipewire/libspa 0.10 (already in tree), hound (promoted from dev-dep — WAV writing), existing import pipeline.

**Spec:** `docs/superpowers/specs/2026-06-12-earworm-design.md` (v2 section)

---

### Task 1: Rolling ring buffer (`engine::ring`)

**Files:**
- Create: `crates/engine/src/ring.rs`
- Modify: `crates/engine/src/lib.rs` (add `pub mod ring;`)

- [x] **Step 1: Write failing tests**

`crates/engine/src/ring.rs`:
```rust
use crate::buffer::{CHANNELS, SAMPLE_RATE};

/// Rolling buffer of the last `capacity_frames` of interleaved stereo audio.
pub struct RollingRing {
    data: Vec<f32>, // capacity_frames * CHANNELS, allocated once
    capacity_frames: usize,
    write_frame: usize, // next write position (frame index, wraps)
    filled_frames: usize,
}

impl RollingRing {
    pub fn with_secs(secs: f64) -> Self {
        let capacity_frames = (secs * SAMPLE_RATE as f64) as usize;
        Self {
            data: vec![0.0; capacity_frames * CHANNELS],
            capacity_frames,
            write_frame: 0,
            filled_frames: 0,
        }
    }

    pub fn filled_secs(&self) -> f64 {
        self.filled_frames as f64 / SAMPLE_RATE as f64
    }

    /// Push interleaved stereo samples (any length; may exceed capacity).
    pub fn push(&mut self, interleaved: &[f32]) {
        todo!()
    }

    /// Last `secs` (clamped to what's filled), chronological, interleaved.
    pub fn snapshot_last(&self, secs: f64) -> Vec<f32> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(vals: &[f32]) -> Vec<f32> {
        vals.iter().flat_map(|v| [*v, *v]).collect() // mono value -> stereo frame
    }

    fn firsts(inter: &[f32]) -> Vec<f32> {
        inter.iter().step_by(CHANNELS).copied().collect()
    }

    #[test]
    fn fills_and_snapshots_chronologically() {
        let mut r = RollingRing::with_secs(1.0); // 48_000 frames
        r.push(&frames(&[1.0, 2.0, 3.0]));
        assert_eq!(firsts(&r.snapshot_last(1.0)), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn overwrites_oldest_when_full() {
        let mut r = RollingRing::with_secs(4.0 / SAMPLE_RATE as f64); // 4 frames
        r.push(&frames(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]));
        assert_eq!(firsts(&r.snapshot_last(10.0)), vec![3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn snapshot_subset_returns_most_recent() {
        let mut r = RollingRing::with_secs(1.0);
        let vals: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        r.push(&frames(&vals));
        let last = r.snapshot_last(10.0 / SAMPLE_RATE as f64); // last 10 frames
        assert_eq!(firsts(&last), (990..1000).map(|i| i as f32).collect::<Vec<_>>());
    }

    #[test]
    fn push_larger_than_capacity_keeps_tail() {
        let mut r = RollingRing::with_secs(3.0 / SAMPLE_RATE as f64); // 3 frames
        r.push(&frames(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]));
        assert_eq!(firsts(&r.snapshot_last(1.0)), vec![5.0, 6.0, 7.0]);
    }

    #[test]
    fn filled_secs_caps_at_capacity() {
        let mut r = RollingRing::with_secs(2.0 / SAMPLE_RATE as f64);
        r.push(&frames(&[1.0]));
        assert!(r.filled_secs() < 2.0 / SAMPLE_RATE as f64 + f64::EPSILON);
        r.push(&frames(&[2.0, 3.0, 4.0]));
        assert_eq!(r.filled_frames, 2);
    }
}
```

- [x] **Step 2: Run (fail), implement (straightforward modular arithmetic), run (pass), commit**

Run: `cargo test -p engine ring` — 5 PASS.

```bash
git add -A && git commit -m "feat(engine): rolling capture ring buffer"
```

---

### Task 2: Node discovery + capture session (`engine::capture`)

**Files:**
- Create: `crates/engine/src/capture.rs`
- Modify: `crates/engine/src/lib.rs`, `crates/engine/Cargo.toml` (move `hound` to `[dependencies]`)

PipeWire-coupled — no unit tests; verified by Task 6's live smoke. Keep all
PipeWire code in this module.

- [x] **Step 1: Implement discovery**

```rust
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
pub struct CaptureNode {
    pub id: u32,
    pub app: String,   // application.name or node.name fallback
    pub media: String, // media.name (song title in Spotify/Firefox!) or ""
}

/// One-shot registry scan for application output streams
/// (media.class == "Stream/Output/Audio"). Runs its own mainloop thread,
/// collects for ~300 ms, returns.
pub fn list_output_streams() -> crate::error::Result<Vec<CaptureNode>> { ... }
```

Implementation: mainloop + context + core, registry listener `global` callback; filter `props["media.class"] == "Stream/Output/Audio"`; collect (id, application.name, media.name) into an `Rc<RefCell<Vec<_>>>`; quit the loop via a 300 ms timer source; join and return. (Same MainLoopRc/ContextRc API family as `output.rs` — follow that module's idioms for 0.10.)

- [x] **Step 2: Implement capture session**

```rust
pub struct CaptureSession {
    pub ring: std::sync::Arc<std::sync::Mutex<crate::ring::RollingRing>>,
    pub node: CaptureNode,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

/// Capture an application node's output into a rolling ring.
/// `buffer_secs` default 180.0 (≈66 MB — three minutes of grab-back).
pub fn start_capture(node: CaptureNode, buffer_secs: f64) -> crate::error::Result<CaptureSession> { ... }

impl CaptureSession {
    pub fn stop(mut self) { /* flag + wake loop + join */ }
}
impl Drop for CaptureSession { /* same, idempotent */ }
```

Stream details: capture stream, `target.object` = node id (as string prop, `TARGET_OBJECT`), format F32 interleaved (or planar per what connect negotiates — convert in the process callback), 48 kHz stereo (PipeWire converts/resamples to the requested format), flags `AUTOCONNECT | MAP_BUFFERS | RT_PROCESS`, `stream.capture.sink` NOT set (we target a specific output stream node, not a sink monitor). Process callback: lock ring, `push` the datas (convert channel count if the negotiated format differs — request stereo and trust the graph's channelmix). Mainloop on the session thread; stop via `AtomicBool` checked in a 100 ms timer that calls `quit()`.

If targeting a Stream/Output/Audio node by `target.object` fails to negotiate on this PipeWire version (it links to the stream's monitor implicitly; behavior verified in Task 6), the documented fallback is capturing the node's peer **sink monitor** filtered to that stream — but try direct stream targeting first; `pw-record --target <id>` does exactly this and works on PipeWire 1.6.

- [x] **Step 3: WAV snapshot helper**

```rust
/// Write interleaved stereo f32 to a 16-bit WAV at 48 kHz. Returns the path.
pub fn write_wav(path: &std::path::Path, interleaved: &[f32]) -> crate::error::Result<()> { ... }
```
(hound, i16 conversion with clamp; create parent dirs.)

- [x] **Step 4: Compile gate + commit**

Run: `cargo build -p engine && cargo clippy -p engine -- -D warnings`

```bash
git add -A && git commit -m "feat(engine): pipewire app-node discovery and rolling capture session"
```

---

### Task 3: `CaptureControl` trait + server commands

**Files:**
- Create: `crates/server/src/capture_control.rs`
- Modify: `crates/server/src/app.rs`, `crates/server/src/lib.rs`
- Test: `crates/server/tests/app_capture.rs`

- [x] **Step 1: Trait (mirrors AudioControl pattern)**

```rust
pub trait CaptureControl: Send {
    fn list_nodes(&mut self) -> Result<Vec<engine::capture::CaptureNode>, String>;
    fn start(&mut self, node_id: u32, buffer_secs: f64) -> Result<(), String>;
    fn stop(&mut self);
    /// (filled_secs, node) if a session is running.
    fn status(&self) -> Option<(f64, engine::capture::CaptureNode)>;
    /// Snapshot last `secs`, chronological interleaved samples.
    fn snapshot(&mut self, secs: f64) -> Result<Vec<f32>, String>;
}
```
`RealCapture` implements it over `engine::capture` (owns `Option<CaptureSession>`); `MockCapture` (in the same module, like MockEngine) returns scripted nodes and a scripted snapshot buffer.

`App::new` gains a `Box<dyn CaptureControl>` parameter — update earwormd + desktop `main.rs` construction (`RealCapture::default()`), and all existing server tests (`MockCapture::default()`); keep the change mechanical.

- [x] **Step 2: Dispatch arms + tests**

Commands:
- `"capture.nodes"` → list
- `"capture.start" {node_id, buffer_secs?}` (default 180)
- `"capture.stop"`
- `"capture.status"` → `{running, filled_secs?, app?, media?}`
- `"capture.grab" {last_secs}` → snapshot; error if empty; write WAV to `~/music/earworm-captures/<app>-<media>-<unix_ts>.wav` (sanitize filename chars, `XDG_MUSIC_DIR` not worth parsing — use `~/music`, matching this user's lowercase dirs); then run the **existing** `song.import` path on it and return the imported Song. Title for the import: `<app> — <media>` if media nonempty (requires `song.import` to accept an optional `title` override param — add it: explicit title wins over file-stem).

`tests/app_capture.rs` (MockCapture + MockEngine):
1. `nodes_and_status_roundtrip` — scripted two nodes; `capture.nodes` returns them; `capture.status` shows not running, then running after `capture.start`.
2. `grab_writes_wav_and_imports` — scripted snapshot = 1 s 440 Hz sine; `capture.grab {last_secs: 1}` → response is a Song with title `"Spotify — Some Song"`; the WAV exists at the returned song's path (point the captures dir at a tempdir via an `App` config field `captures_dir: PathBuf`, default `~/music/earworm-captures`, overridden in tests); `song.open` on it succeeds (decodes).
3. `grab_with_no_capture_errors` — ok:false.

- [x] **Step 3: Run (fail→pass), full server suite, commit**

Run: `cargo test -p server` — all green including the 3 new.

```bash
git add -A && git commit -m "feat(server): capture commands — discover, roll, grab-to-song"
```

---

### Task 4: Capture UI tab

**Files:**
- Create: `apps/desktop/src/components/Capture.svelte`
- Modify: `apps/desktop/src/lib/stores.ts` (capture actions), right-rail tabs

- [x] **Step 1: Implement**

Tab `capture`: refresh button → node list (`app` bold, `media` muted — media is often the *song title currently playing*); click a node → `capture.start`; status line while rolling: `● REC <app> — <filled>s buffered` (accent dot, mono); grab buttons `last 30s · 60s · 2m · all` → `capture.grab` → imported song auto-opens (reuse `openSong(id)`); stop button. Poll `capture.status` every 2 s while the tab is visible (no new event plumbing needed).

- [x] **Step 2: Verify + commit**

Run: `pnpm build && pnpm vitest run` — clean.

```bash
git add -A && git commit -m "feat(desktop): capture tab — pick app, roll, grab to song"
```

---

### Task 5: earwormd parity

**Files:**
- Modify: `crates/server/src/bin/earwormd.rs` (construct `RealCapture`)

- [x] **Step 1: Wire + gate**

Run: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt`

```bash
git add -A && git commit -m "feat(server): earwormd capture parity"
```

---

### Task 6: Live smoke test (the proof)

No files — verification task. PipeWire is live on this machine.

- [x] **Step 1: End-to-end capture of a real app node**

```bash
ffmpeg -f lavfi -i "sine=frequency=330:duration=60" -ac 2 /tmp/cap-src.flac  # if missing
pw-play /tmp/cap-src.flac &   # creates a Stream/Output/Audio node
sleep 2
# via the socket (earwormd or desktop running):
# capture.nodes  -> expect a node with app/media mentioning pw-play/cap-src
# capture.start {node_id}
# sleep 8
# capture.grab {last_secs: 5} -> Song response
# verify: WAV file exists, >4s long, non-silent (decode + RMS via a tiny rust example or sox stat)
kill %1
```
Drive it with a small python3 socket script (pattern from Plan 3's smoke). Assert the grabbed WAV's RMS > 0.1 (`sox <wav> -n stat` or a 10-line rust example). If `target.object` linking fails, implement the documented fallback in Task 2 and re-run.

- [x] **Step 2: Record results + commit any fixes**

```bash
git add -A && git commit -m "test(capture): live pipewire capture smoke verified"
```
(Commit only if fixes/changes; otherwise note results in the final report.)

---

## Self-review checklist

- Spec v2 coverage: tap any app node ✔ (discovery + targeted capture), rolling "loop what just played" ✔ (ring + grab), promote capture to library song ✔ (WAV → song.import → hash/sidecar/peaks all reused).
- Pattern consistency: CaptureControl mirrors AudioControl; commands follow `noun.verb`; UI follows existing tab/store idioms.
- Risk + mitigation: `target.object` semantics against a stream node — Task 6 is a real end-to-end proof with a synthetic app node, with a named fallback strategy.
