# Overdub Auto-Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Recorded overdub takes land in time automatically — record locked to the transport (kills seconds of drift), then compensate a measured constant round-trip latency (kills the ms residual). Manual nudge stays.

**Architecture:** Capture and playback are separate PipeWire streams sharing the interface graph clock. We read `Stream::time()` (`pw_time`: `now` graph-clock ns, `ticks` sample pos, `rate`, `delay` per-stream hardware latency — no FFI needed) on both, establish a capture-frame ↔ song-frame correspondence at record start, and extract the take anchored to the song timeline. RTL = output `delay` + input `delay` (auto baseline) or a loopback ping (sample-accurate), applied via the existing `start_frame = anchor − latency − nudge`.

**Tech Stack:** Rust (`engine`, `server`), `pipewire`/`libspa` 0.10, lock-free atomics for RT→control timing snapshots.

**Sequencing:** Part 1 (Tasks 1–5) ships the real fix and is independently testable/usable. Part 2 (Tasks 6–7) polishes the residual. Task 8 gates.

---

## File Structure

- `crates/engine/src/ring.rs` — *modify*. Monotonic frame index + absolute-range read.
- `crates/engine/src/stream_clock.rs` — *new*. Lock-free `(now_ns, ticks, rate, delay)` snapshot updated from an RT callback + read from control thread; pure mapping math.
- `crates/engine/src/capture.rs` — *modify*. Update the capture `StreamClock` per cycle; expose it + the input `delay`.
- `crates/engine/src/output.rs` / `engine.rs` / `pipeline.rs` — *modify*. Update a playback `StreamClock`; record song-frame↔now; expose output `delay`.
- `crates/server/src/recording.rs` — *modify*. Transport-locked take extraction; loopback ping orchestration.
- `crates/server/src/app.rs` — *modify*. Latency baseline wiring; keep `recording.calibrate`.

---

## Task 1: RollingRing monotonic index + absolute-range read

**Files:** Modify `crates/engine/src/ring.rs`; tests in-file.

- [ ] **Step 1: Write failing tests** for a monotonic counter and absolute read. Add to the `#[cfg(test)] mod tests` in `ring.rs`:

```rust
    #[test]
    fn tracks_total_frames_written_monotonically() {
        let mut r = RollingRing::new(4); // 4-frame window
        r.push(&vec![0.0; 2 * CHANNELS]); // 2 frames
        assert_eq!(r.total_frames_written(), 2);
        r.push(&vec![0.0; 5 * CHANNELS]); // 5 frames, wraps the 4-window
        assert_eq!(r.total_frames_written(), 7); // NOT capped at capacity
    }

    #[test]
    fn reads_an_absolute_frame_range_still_in_the_window() {
        let mut r = RollingRing::new(8);
        // write frames 0..6 with sample value = frame index (both channels)
        for f in 0..6u32 {
            r.push(&[f as f32, f as f32]);
        }
        // request absolute frames [2,5)
        let got = r.read_range(2, 5).expect("in window");
        assert_eq!(got, vec![2.0, 2.0, 3.0, 3.0, 4.0, 4.0]);
    }

    #[test]
    fn read_range_returns_none_when_evicted() {
        let mut r = RollingRing::new(4);
        for f in 0..10u32 {
            r.push(&[f as f32, f as f32]);
        }
        // frames 0..6 are gone (window holds last 4: 6,7,8,9)
        assert!(r.read_range(0, 4).is_none());
        assert!(r.read_range(6, 10).is_some());
    }
```

(Confirm `RollingRing::new(capacity_frames)` and `push(&[f32])` signatures against the real file; adapt the constructor call if it differs, e.g. takes seconds.)

- [ ] **Step 2: Run — expect failure** (`total_frames_written`/`read_range` missing). `cargo test -p engine ring::tests`.

- [ ] **Step 3: Implement.** Add `total_frames_written: u64` to `RollingRing`, increment it (uncapped) in `push` alongside the existing capped `filled_frames`. Add:

```rust
    pub fn total_frames_written(&self) -> u64 {
        self.total_frames_written
    }

    /// Interleaved samples for absolute frame range `[start, end)`, or `None`
    /// if any of it has already been evicted from the window (or `end` is past
    /// what's been written).
    pub fn read_range(&self, start: u64, end: u64) -> Option<Vec<f32>> {
        let total = self.total_frames_written;
        let cap = self.capacity_frames as u64;
        let oldest = total.saturating_sub(self.filled_frames as u64);
        if start < oldest || end > total || end < start {
            return None;
        }
        let mut out = Vec::with_capacity(((end - start) as usize) * CHANNELS);
        for f in start..end {
            // position in the ring of absolute frame f
            let idx = ((f % cap) as usize) * CHANNELS;
            out.extend_from_slice(&self.data[idx..idx + CHANNELS]);
        }
        Some(out)
    }
```

(Verify the ring's modulo geometry matches `snapshot_last`'s — `write_frame`/`capacity_frames`. The absolute→ring mapping must agree with how `push` lays samples down.)

- [ ] **Step 4: Run — expect pass.** `cargo test -p engine ring::tests`.
- [ ] **Step 5: Lint + commit.** `cargo clippy -p engine --all-targets -- -D warnings`; `git commit -am "feat(engine): RollingRing monotonic frame index + absolute-range read"`.

---

## Task 2: StreamClock — lock-free timing snapshot + mapping math

**Files:** Create `crates/engine/src/stream_clock.rs`; register in `lib.rs`; tests in-file.

A `StreamClock` holds the latest `pw_time`-derived `(now_ns, ticks, rate_hz, delay_frames)` written from an RT callback and read from the control thread, plus pure mapping helpers. Use an `AtomicU64`-packed seqlock or `arc_swap` of a small `Copy` struct (match the engine's existing lock-free style — `ArcSwapOption` is already used).

- [ ] **Step 1: Write failing tests** for the mapping math (pure, no PipeWire):

```rust
    // A clock snapshot says: at graph time `now_ns`, the stream was at sample
    // `ticks`, advancing at `rate_hz` samples/sec.
    #[test]
    fn maps_graph_time_to_stream_frame() {
        let s = ClockSnapshot { now_ns: 1_000_000_000, ticks: 48_000, rate_hz: 48_000 };
        // 0.5s later → +24000 frames
        assert_eq!(s.frame_at_ns(1_500_000_000), 48_000 + 24_000);
        // at the snapshot instant
        assert_eq!(s.frame_at_ns(1_000_000_000), 48_000);
    }

    #[test]
    fn maps_stream_frame_to_graph_time() {
        let s = ClockSnapshot { now_ns: 1_000_000_000, ticks: 48_000, rate_hz: 48_000 };
        assert_eq!(s.ns_at_frame(48_000), 1_000_000_000);
        assert_eq!(s.ns_at_frame(72_000), 1_500_000_000);
    }
```

- [ ] **Step 2: Run — expect failure.** `cargo test -p engine stream_clock::tests`.

- [ ] **Step 3: Implement** `ClockSnapshot { now_ns: i64, ticks: i64, rate_hz: i64 }` with:

```rust
    pub fn frame_at_ns(&self, t_ns: i64) -> i64 {
        self.ticks + (t_ns - self.now_ns) * self.rate_hz / 1_000_000_000
    }
    pub fn ns_at_frame(&self, frame: i64) -> i64 {
        self.now_ns + (frame - self.ticks) * 1_000_000_000 / self.rate_hz
    }
```

Plus a `StreamClock` wrapper (`ArcSwap<ClockSnapshot>` or a seqlock) with `store(snapshot)` (RT side) and `load() -> ClockSnapshot` (control side), and a `delay_frames` slot. Register `pub mod stream_clock;` in `lib.rs`.

- [ ] **Step 4: Run — expect pass; lint; commit.** `cargo test -p engine stream_clock::tests`; clippy; `git commit -am "feat(engine): StreamClock timing snapshot + graph-time mapping"`.

---

## Task 3: Feed the capture StreamClock from the capture callback

**Files:** Modify `crates/engine/src/capture.rs`. Device-bound — verified manually (Task 8); no hardware unit test.

- [ ] **Step 1:** Give `CaptureSession` a `clock: Arc<StreamClock>` (and the ring already there). In the capture process callback (`capture.rs` ~239), after dequeuing, call `stream.time()`; if `Ok(t)`, build a `ClockSnapshot { now_ns: t.now(), ticks: t.as_raw().ticks as i64, rate_hz: (t.as_raw().rate.denom as i64)/(t.as_raw().rate.num as i64) ... }` — **derive `rate_hz` correctly from the `pw_time.rate` SPA fraction** (it is `num/denom` seconds per tick, so samples/sec = `denom/num`; confirm against `libspa` and a debug print on-device). Also record the ring's `total_frames_written` at this instant so a graph-time can map to a ring frame: store both in the snapshot or a paired struct. Store `t.as_raw().delay` as the input `delay_frames`.
- [ ] **Step 2:** Expose `CaptureSession::clock()` and `input_delay_frames()` to the server.
- [ ] **Step 3:** Build + clippy (`cargo build -p engine`, `cargo clippy -p engine --all-targets -- -D warnings`). Commit `feat(engine): capture stream clock + input delay`.

Note: the mapping that matters is **ring frame ↔ graph time**. Because the ring's `total_frames_written` and the stream `ticks` both advance at the device rate, capture them together in one callback so `ring_frame_at_ns(t)` = `total_at_snapshot + (frame_at_ns(t) - ticks_at_snapshot)`. Put that helper (pure) in `stream_clock.rs` with a unit test if it can be expressed without PipeWire.

---

## Task 4: Expose playback song-frame ↔ graph time + output delay

**Files:** Modify `crates/engine/src/output.rs`, `pipeline.rs`, `engine.rs`. Device-bound — manual verification.

- [ ] **Step 1:** In the output process callback (`output.rs` ~92), after `core.fill(out)`, read `stream.time()` and store a `ClockSnapshot` into a playback `StreamClock` owned by the engine, plus `delay` as output `delay_frames`. The snapshot's `ticks` is output-stream samples; we must relate it to the **song audible frame**. The pipeline knows the audible song frame produced this block (`audible_frame`). Store the pair `(audible_song_frame, graph_now_ns)` atomically each block (a second small snapshot: `SongClock { song_frame: i64, now_ns: i64, rate_hz: i64 }`) so the control thread can compute `now_ns_at_song_frame(start)`.
- [ ] **Step 2:** Expose from `Engine`: `playback_song_clock() -> SongClock`, `output_delay_frames() -> i64`.
- [ ] **Step 3:** Build + clippy; commit `feat(engine): playback song-frame clock + output delay`.

---

## Task 5: Transport-locked take extraction

**Files:** Modify `crates/server/src/recording.rs` (`RealRecorder`), `crates/server/src/app.rs` (`recording_start`/`recording_stop`). Extraction-selection math unit-tested; live timing manual.

- [ ] **Step 1: Write a failing unit test** for the frame-selection (pure), in `recording.rs`:

```rust
    // Given the song-frame→graph-time clock and the capture ring→graph-time
    // clock, the take's first ring frame for song frame `start` is computed by
    // round-tripping through graph time.
    #[test]
    fn take_start_ring_frame_round_trips_through_graph_time() {
        // song frame `start` was output at graph ns T; capture ring frame R was
        // at graph ns T. With both clocks at unity rate, picking start maps back.
        let song = ClockSnapshot { now_ns: 0, ticks: 0, rate_hz: 48_000 };      // song frame == ns*48k/1e9
        let cap_ticks_at0 = 1000; let cap_total_at0 = 1000; // ring frame == ticks here
        let cap = ClockSnapshot { now_ns: 0, ticks: cap_ticks_at0, rate_hz: 48_000 };
        let start_song_frame = 48_000; // 1.0s in
        let t = song.ns_at_frame(start_song_frame);
        let cap_frame = cap.frame_at_ns(t);
        let ring_frame = cap_total_at0 as i64 + (cap_frame - cap_ticks_at0);
        assert_eq!(ring_frame, cap_total_at0 as i64 + 48_000);
    }
```

(This documents/locks the round-trip; the real handler performs the same arithmetic with live snapshots.)

- [ ] **Step 2:** Implement transport-locked capture. In `recording_start`: capture both clocks' snapshots (song clock + capture clock + capture `total_frames_written`) at the moment playback reaches the span start — i.e., gate on the first `Position` with `count_in == None` at/after `start` (the app already sees Position events), then snapshot. Store these in `PendingRecording`. In `recording_stop`: compute `ring_start = ring_frame_for_song_frame(start)` via the round-trip above, then `samples = ring.read_range(ring_start, ring_start + len)`; if `None` (evicted — recording longer than the ring), fall back to `snapshot_last(len)` and log. Replace the `snapshot_last`-only path. The take is now anchored to song frame `start`; RTL is applied later by `refresh_layers`.
- [ ] **Step 3:** `cargo test -p server`, `cargo build -p server`, clippy. Commit `feat(server): transport-locked overdub capture`.
- [ ] **Step 4: Manual check** (Task 8 covers it): record a take, confirm it lands within a few ms regardless of when stop is pressed.

---

## Task 6: PipeWire-reported latency baseline (Part 2)

**Files:** `crates/server/src/app.rs`.

- [ ] **Step 1:** When recording (or on song open / device change), set the `input_latency_frames` baseline = `engine.output_delay_frames() + capture.input_delay_frames()` **if** the user has not set an explicit calibration value (track a `latency_source: auto|loopback|manual`). Apply via the existing `refresh_layers` formula. Unit-test the sum/selection logic with a fake engine returning known delays.
- [ ] **Step 2:** `cargo test -p server`, clippy. Commit `feat(server): auto RTL baseline from PipeWire delays`.

---

## Task 7: Loopback ping calibration (Part 2)

**Files:** `crates/server/src/recording.rs`, `app.rs`. Reuses `detect_click_onset` (already present + tested).

- [ ] **Step 1:** Implement `recording.calibrate` properly (currently an honest-error stub): emit a short impulse out the output while capturing (orchestrated outside the App lock, as the stub already is), detect the onset in the recorded buffer, subtract the known internal offset, store the result as `input_latency_frames` with `latency_source = loopback`. Keep the honest error when no onset is detected.
- [ ] **Step 2:** Engine needs a one-shot impulse out the output. Add a minimal `EngineCmd::Impulse` (or reuse the metronome for one accented click) emitted at a known output frame; the calibration reads its arrival. Unit-test onset math on a synthetic buffer (already covered); the live loop is manual.
- [ ] **Step 3:** `cargo test -p server`, clippy. Commit `feat(server): loopback ping latency calibration`.

---

## Task 8: Full gate + manual + smoke

- [ ] **Step 1:** `just check` (workspace tests + vitest + clippy --all-targets + fmt + svelte-check). Run `cargo fmt --all` first.
- [ ] **Step 2: UI runtime smoke test** (per memory ui-runtime-smoke-test): `pnpm vite --port 5173` (Bash `run_in_background`), load in chrome-devtools MCP, confirm no console errors.
- [ ] **Step 3: Manual device verification** (`just build && just run`): record a take on the Focusrite → it lands in time without nudging, regardless of stop timing → run loopback calibrate, confirm a plausible sample value that persists and applies → record again, confirm tighter → manual nudge still works.
- [ ] **Step 4:** Update the campaign file to DONE; note residual limitations.

---

## Notes for the implementer

- **`pw_time` field semantics must be confirmed on-device** with a debug print the first time (the `rate` fraction direction and `delay` units are the two things to verify). Don't trust the plan's arithmetic blind — print `t.as_raw()` once and check.
- **Two-clock setups** (input/output different devices) make the streams drift; the round-trip still gives a good anchor at record start. Don't try to track drift in v1 — document it; nudge covers residual.
- **RT-safety:** `stream.time()` in the process callback is allowed; the `StreamClock` store must be lock-free (atomics/ArcSwap), never a Mutex on the RT path.
