# Overdub auto-sync — design

## Problem

Recorded overdub takes land badly out of time with the track. Diagnosis of the
existing code (`crates/server/src/recording.rs`, `app.rs`) found **two stacked
causes**:

1. **Gross drift (seconds) — the capture mechanism.** Capture starts before the
   count-in; on stop, `RealRecorder::stop` keeps the *last `span_seconds` of
   audio ending at the moment stop was pressed* (`snapshot_last`) and pins that
   to the span start. So the take's alignment is determined by when the user
   presses stop and the count-in length, not by the song timeline. This is the
   dominant error.
2. **Fine offset (milliseconds) — system round-trip latency (RTL).** Output
   buffering + DAC + (instrument) + ADC + input buffering. A fixed constant for
   a given interface + sample rate + buffer size.

The deferred "acoustic click calibration" would only have addressed (2), so it
could never have fixed the observed problem.

## What professional tools do (and we will match)

Confirmed against Reaper, Audacity, and Ardour: they do **not** auto-align the
performance to the track (that would distort the player's timing/feel — wrong
for a clean DI signal). Instead:

- The take is recorded **locked to the transport position**, so it sits at the
  right place on the timeline by construction.
- **RTL is a measured constant**, applied as a fixed offset to every take.
  Measured precisely via a **loopback ping** (patch an output back to an input,
  play an impulse, measure the sample delay) — Reaper's ReaInsert "ping detect",
  Audacity's latency-correction procedure, Ardour's `jack_iodelay`. Drivers also
  *report* an I/O latency that serves as a no-cable baseline.
- The performance content is preserved exactly; only the constant is removed.

## Decisions (from brainstorming)

- **Signal path:** direct instrument (DI) into a USB interface (Focusrite
  Scarlett 2i2), headphone monitoring. The take is a clean instrument signal
  Scarlett 2i2), headphone monitoring. The take is a clean instrument signal
  with **no backing-track bleed**, so cross-correlation against the track is
  rejected (it would shift the performance's feel). Pro-style constant-offset
  compensation is the approach.
- **RTL measurement:** PipeWire-reported latency as an automatic baseline, **plus
  an optional loopback ping** for sample-accurate precision. Manual per-layer
  nudge stays as the final human fine-tune.
- **Clock assumption:** input and output on the **same interface** (Focusrite in
  and out) share one hardware clock, so the two PipeWire streams are sample-
  locked. The two-clock case (input and output on different devices) is a
  documented degraded path — anchoring still works but may drift slightly over a
  long take; out of scope to fully solve in v1.

## Architecture

dredge runs capture and playback as **two separate PipeWire streams**, each on
its own mainloop/context/thread, exposing **no timing info today**: the playback
position is a software estimate (`Pipeline::audible_frame`) and the capture
`RollingRing` keeps only a rolling window with no absolute frame index. The fix
adds the missing timing relationship.

### Part 1 — Transport-locked capture (the core fix)

Goal: the captured take's frame 0 corresponds to playback frame = span `start`,
independent of count-in and stop timing. Then only the constant RTL remains.

- **`RollingRing` gains a monotonic counter** (`total_frames_written: u64`) and
  the ability to read an absolute frame range, not just `snapshot_last`.
- **PipeWire stream timing** is read on both streams (`pw_stream_get_time` /
  the `pipewire` crate equivalent; an FFI shim like `engine/src/ffi.rs` if the
  safe binding lacks it). At record start we establish one correspondence —
  "capture frame C ↔ playback frame P" — against the shared graph clock.
- On stop, the take is extracted from the capture frame that maps to span
  `start`, for the span length. Anchoring no longer depends on stop timing.
- The engine exposes the playback stream's true sample position (from PipeWire)
  so the correspondence is sample-accurate on a shared-clock device.

Part 1 alone aligns the take to within the RTL (a few ms on a same-device
interface), which already resolves the user's complaint.

### Part 2 — RTL compensation

- **Auto baseline:** query PipeWire output + input node latencies, sum =
  `input_latency_frames` baseline, applied automatically (no cable).
- **Optional loopback ping:** a calibration that patches an interface output to
  an input; dredge plays a short impulse out and records it; the measured sample
  delay (via `detect_click_onset`, already present) = sample-accurate RTL,
  stored as `input_latency_frames`, overriding the baseline. Reuses the existing
  `recording.calibrate` command surface (currently a stub returning an honest
  error).
- Applied through the existing engine layer formula
  `start_frame = anchor − latency − nudge`, which becomes correct once Part 1
  anchors the take. `latency` = stored RTL.

### Part 3 — Manual nudge

Unchanged. The per-layer `nudge_frames` control remains for any residual or for
two-clock setups; users should rarely need it after Parts 1–2.

## Data flow (a take, end to end)

record start → mark capture↔playback frame correspondence → (count-in plays,
not captured) → capture runs locked to transport → stop → extract take from the
span-start capture frame for span length → write WAV → manifest → `refresh_layers`
places the layer at `anchor − RTL − nudge` → engine mixes pre-stretch.

## Components & boundaries

- `engine::ring::RollingRing` — add monotonic frame index + absolute-range read.
  One responsibility: a timestamped rolling buffer.
- `engine::capture` — expose capture stream timing + absolute frame mapping.
- `engine::output` / `engine` — expose playback stream's PipeWire sample position
  and a way to query node latencies (Part 2 baseline).
- A small PipeWire-timing shim (safe wrapper or `ffi`) — isolates the unsafe
  timing calls. One responsibility: "given a stream, return (sample position,
  graph clock time)".
- `server::recording` — transport-locked take extraction; loopback calibration
  orchestration (impulse out + capture + onset detect), run outside the App lock.
- `server::app` — wire latency baseline on song open / device change; keep the
  `recording.calibrate` command; `refresh_layers` already applies the formula.

## Testing

- **Pure/unit (Rust):** `RollingRing` monotonic counter + absolute-range read
  (write N, read range [a,b)); span-start→capture-frame mapping math given a
  known correspondence; `detect_click_onset` already tested; latency-sum math.
- **Engine timing:** a deterministic test of the correspondence math with a
  synthetic (capture_frame, playback_frame, clock) sample — the PipeWire call
  itself is device-bound and verified manually.
- **Manual (device):** record a take on the Focusrite, confirm it lands in time
  without nudging; run loopback ping, confirm a plausible sample value; verify
  the value persists and is applied.

## Out of scope (v1)

- Fully solving the two-clock case (input/output on different interfaces) beyond
  the manual-nudge fallback and a documented note.
- Sub-sample (phase) precision (Ardour's `jack_iodelay` level). Whole-sample RTL
  is enough here.
- Cross-correlation / performance auto-align (explicitly rejected).
- Auto-stop at span end (separate known issue; not required once anchoring is
  transport-locked, but tracked).
