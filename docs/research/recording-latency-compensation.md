# Research: recording latency compensation for overdub

Reference notes behind dredge's overdub auto-sync work (2026-06). Read this
before touching recording alignment — it captures hard-won knowledge that is not
obvious from the code and that the on-hardware-only verification depends on.

Related: spec `docs/superpowers/specs/2026-06-25-overdub-autosync-design.md`,
plan `docs/superpowers/plans/2026-06-25-overdub-autosync.md`, campaign
`docs/superpowers/campaigns/2026-06-25-overdub-autosync.md`.

---

## 1. The problem

When you record yourself playing along with a track (overdub), the recorded take
lands out of time with the track. Two **distinct, stacked** causes — do not
conflate them:

1. **Gross drift (seconds) — capture anchoring.** If the take is captured as
   "the last N seconds of audio ending when the user pressed stop" and pinned to
   the song start (dredge's original `RealRecorder::stop` → `snapshot_last`), its
   alignment is governed by stop-button timing and the count-in length, not the
   song timeline. This is the *dominant* error and the originally-deferred
   latency calibration could never have fixed it.
2. **Fine offset (milliseconds) — round-trip latency (RTL).** The fixed system
   delay: output buffering + DAC + (the player's analog path) + ADC + input
   buffering. Constant for a given interface + sample rate + buffer size.

A latency calibration only addresses (2). You must fix (1) first by recording
**locked to the transport**.

## 2. How professional DAWs solve it (the standard, and what we copy)

Confirmed against Reaper, Audacity, and Ardour. The consensus:

- **They do NOT auto-align the performance to the track.** With a clean DI signal
  and headphone monitoring there is no backing-track bleed in the take; sliding
  it to "match" the song by cross-correlation would shift the player's *timing
  and feel*. Pros refuse to do this. (So: cross-correlation / "snap to grid" is
  the WRONG approach for tracking. We rejected it.)
- **The take is recorded locked to the transport position**, so it sits at the
  right place on the timeline by construction. The only residual is the constant
  RTL.
- **RTL is a measured constant**, applied as a fixed sample offset (shift the
  recorded take earlier by RTL — or Ardour's read-ahead: play *early* by RTL so
  the take lands aligned). Measured once via a **loopback ping**: patch an
  interface output back into an input, play an impulse/click, record it, measure
  the sample delay between sent and received.
  - **Reaper:** ReaInsert "ping detect" automates the loopback measurement and
    writes a sample value into the recording "input manual offset". Also offers
    "use audio driver reported latency" as a baseline.
  - **Audacity:** a documented latency-correction procedure (record a click
    through the monitoring/loopback path, measure the offset, enter the
    correction); applies a constant shift.
  - **Ardour:** uses JACK's `jack_iodelay`, which emits tones, captures them after
    a round-trip, and measures phase difference for **sub-sample** precision.
    Compensates via read-ahead.
- **Caveat they all note:** on Linux/USB the RTL can change on reconnect, reboot,
  or xrun, and whenever sample rate or buffer/block size changes. So it is
  re-measurable, not a write-once constant.

**Sources:**
- Audacity – Latency Compensation: https://support.audacityteam.org/troubleshooting/solving-recording-problems/latency-compensation
- Reaper – Recording Latency Offset: https://reaper.blog/2018/11/rec_latency_offset/
- Ardour – Latency and Latency Compensation: https://manual.ardour.org/synchronization/latency-and-latency-compensation/
- Oblique Audio – RTL Utility (standalone round-trip latency measurement): https://oblique-audio.com/rtl-utility.php

## 3. The PipeWire timing model (the key enabler on Linux)

dredge runs capture and playback as **two separate PipeWire streams**, each on
its own mainloop/context/thread. They expose no timing by default. The fix uses
PipeWire's per-stream timing against the shared graph clock.

### `pw_time` (the C struct, via `pw_stream_get_time[_n]`)

Fields used (confirmed against the generated bindgen output for this repo):

| field | type | meaning |
|-------|------|---------|
| `now` | `i64` | graph-clock timestamp (nanoseconds, monotonic) of the last cycle update — **shared across all streams on one graph** |
| `rate` | `spa_fraction { num: u32, denom: u32 }` | **seconds per tick** → frames/sec = `denom / num` (for 48 kHz: num=1, denom=48000) |
| `ticks` | `u64` | the stream's sample position at `now` |
| `delay` | `i64` | the stream↔hardware latency **in ticks/frames** — this is the per-stream latency for the RTL baseline (output delay + input delay) |
| `queued`, `buffered` | `u64` | buffered samples (not currently used) |

### The Rust binding (`pipewire` 0.10)

- `Stream::time() -> Result<Time, Error>` wraps `pw_stream_get_time` (and
  `pw_stream_get_time_n` with the `v0_3_50` feature; this repo has `v0_3_49`, so
  it uses the deprecated-but-working non-`_n` call — bump the feature to silence
  that if desired).
- `Time::now() -> i64`; `Time::as_raw() -> &pw_sys::pw_time` for the fields above.
- `spa_sys::spa_fraction { num: u32, denom: u32 }`.
- `Stream::time()` is callable inside the process callback (signature
  `FnMut(&Stream, &mut D)`), so `stream.time()` works in-place.

### Shared graph clock — why this works

PipeWire schedules all linked nodes in one graph from a single driver clock, so
`now` is a common time reference across the two streams. **Crucial assumption:**
when the input and output are the **same interface** (e.g. a Focusrite used for
both DI input and headphone output), both streams run on that interface's clock
and are sample-locked — the correspondence is stable. If input and output are on
**different devices**, PipeWire follows one driver and adaptively resamples the
other; the streams aren't sample-locked and the anchor can drift over a long
take. That two-clock case is a documented degraded path (manual nudge covers the
residual); we did not solve it in v1.

## 4. dredge's design (how the pieces map)

- **`engine::stream_clock`** — pure math: `ClockSnapshot { now_ns, ticks,
  rate_hz }` with `frame_at_ns` / `ns_at_frame` (linear interpolation from one
  reading), and `ring_frame_at_ns(cap, ring_total_at_snapshot, t_ns)` mapping a
  graph time to a capture-ring absolute frame. Plus `StreamClock`: a lock-free
  arm-gated publisher (an `AtomicBool armed` + `ArcSwapOption<ClockSnapshot>` +
  atomics for `ring_total` and `delay`). **Arm-gating** means the RT callback
  allocates/publishes only while the control thread has armed it around a
  recording — the steady audio path pays nothing.
- **Capture clock** (`engine::capture`) publishes `(now_ns, capture stream ticks,
  ring_total_at_snapshot)` + input `delay`. The ring (`RollingRing`) gained a
  monotonic `total_frames_written()` and `read_range(start,end)` (absolute-frame
  read, counting back from the write head so it survives oversized pushes).
- **Playback clock** (`engine` owns it, output RT thread publishes) publishes
  `(now_ns, audible **song** frame, song rate)` + output `delay`. Note: it
  publishes the SONG frame (from `Pipeline::audible_frame`), not the output
  stream's raw ticks, because we need the song-timeline position.
- **Anchor math at finalize** (`server::app::finalize_recording`):
  ```
  t       = playback_snapshot.ns_at_frame(anchor_song_frame)   // graph time the song was at `anchor`
  ring0   = ring_frame_at_ns(capture_snapshot, ring_total, t)  // capture ring frame acquired then
  samples = ring.read_range(ring0, ring0 + len)                // the take, anchored to the song
  ```
  Falls back to `snapshot_last` if the range was evicted (recording longer than
  the ring) or if snapshots are missing (e.g. cpal / non-PipeWire backend).
- **RTL application:** the existing layer formula `start_frame = anchor − latency
  − nudge`. `latency` = RTL (Part 2). Once the take is transport-anchored, this
  formula is finally *correct*.
- **Auto-finalize at span end:** the recording finalizes when playback reaches
  the span end (tick/Position path), so the take equals the span regardless of
  when the user clicks stop — this is what robustly defeats the late-stop drift.

## 5. RTL measurement (Part 2 — chosen approach)

- **Auto baseline (no cable):** sum the two streams' `pw_time.delay` (output +
  input) → `input_latency_frames`. Applied automatically. This is the
  PipeWire-reported equivalent of Reaper's "use driver reported latency".
- **Optional loopback ping (sample-accurate):** patch a Focusrite output into an
  input, play an impulse, capture it, `detect_click_onset` (already implemented +
  tested) → exact RTL, overriding the baseline. This is the Reaper/Audacity/Ardour
  method. Reuses the `recording.calibrate` command (currently an honest-error
  stub).
- **Manual nudge** remains the final human fine-tune (and the fallback for
  two-clock setups).

## 6. What is CONFIRMED vs. what still needs on-hardware verification

**Confirmed (compile-time / source):**
- `pw_time` / `spa_fraction` field names and types (bindgen output).
- `Stream::time()` is callable in the process callback.
- The pure anchor/ring math (unit-tested in `stream_clock` and at the app layer).

**NOT yet verified — REQUIRES the real interface (e.g. Focusrite), via
`DREDGE_DEBUG=1` which prints the raw `pw_time` once per stream:**
- That `rate` really yields the device rate as `denom/num` (i.e. the fraction
  direction is as assumed).
- That `delay` is in ticks/frames with the expected sign and magnitude (this is
  the RTL baseline — if the sign or unit is wrong, the baseline is wrong).
- That `ticks` advances monotonically at the device rate against `now`.
- That pairing a playback snapshot with a capture snapshot yields a sane
  capture-ring ↔ song-frame mapping (the actual cross-stream alignment — the
  whole point). Verify empirically: record a take and confirm it lands in time
  without nudging, independent of when stop is pressed.

If any of the four are off, fix the derivation in `engine::capture` /
`engine::output` (the field reads), not the pure math.

## 7. Gotchas / lessons

- **Same-interface in+out** is the easy, sample-locked case and what the design
  targets. Different devices = two clocks = degraded.
- **Arm-gate the RT publish** — never allocate or lock on the steady audio path.
- **`stream.time()` per cycle** is a syscall; gate it behind `is_armed()` so the
  steady path (e.g. the tuner) doesn't pay it.
- **Integer math, not f64**, for the frame/ns mapping — keep it exact (deltas fit
  i64 for realistic durations).
- The originally-shipped overdub recording used `snapshot_last(span)` ending at
  stop-press time; that is the gross-drift bug. Transport-locking replaces it.
