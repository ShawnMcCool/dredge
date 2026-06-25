# Campaign: Overdub auto-sync

**Status:** IN PROGRESS — started 2026-06-25
**Spec:** `docs/superpowers/specs/2026-06-25-overdub-autosync-design.md`
**Plan:** `docs/superpowers/plans/2026-06-25-overdub-autosync.md`
**Branch:** main (local; not pushed)

## Goal

Make recorded overdub takes land in time with the track automatically, the way
pro DAWs do: record locked to the transport (kills seconds of drift), then
compensate a measured constant round-trip latency (kills the ms residual).
Manual nudge stays as the final fine-tune.

## Key decisions

- Match pro tools (Reaper/Audacity/Ardour): transport-locked recording + a fixed
  RTL constant. NOT cross-correlation (would distort a clean DI performance).
- RTL = PipeWire-reported baseline (auto, no cable) + optional loopback ping
  (sample-accurate). Manual nudge retained.
- User setup: DI into Focusrite, headphone monitoring. Input+output assumed on
  the same interface clock (sample-locked streams); two-clock case is a
  documented degraded path (nudge fallback).
- Sequenced: Part 1 (anchoring) ships first — it alone solves the main complaint.

## Root cause (confirmed in code)

`RealRecorder::stop` uses `snapshot_last(span)` ending at stop-press time, pinned
to span start → alignment depends on stop timing + count-in (seconds of drift).
Plus fixed system RTL (ms). The capture and playback PipeWire streams expose no
timing today; the ring has no absolute frame index.

## Task status

(Filled in once the plan is written. Updated as tasks complete — newest progress
at the bottom of the Progress log.)

| # | Task | Status |
|---|------|--------|
| — | spec written + committed | done |
| — | campaign file created | done |
| — | plan written + committed | pending |
| 1 | Feasibility spike: PipeWire stream timing API in the `pipewire` Rust crate (or FFI needed?) | DONE — no FFI; `Stream::time()` exists |
| 2 | RollingRing monotonic frame index + absolute-range read | pending |
| 3 | Capture↔playback frame correspondence (transport-locked extraction) | pending |
| 4 | Wire transport-locked take into recording_start/stop | pending |
| 5 | Part 2: PipeWire-reported latency baseline | pending |
| 6 | Part 2: loopback ping calibration (recording.calibrate) | pending |
| 7 | Full gate + manual device verification | pending |

## Risks / open questions

- **PipeWire timing binding:** does the `pipewire` Rust crate expose
  `pw_stream_get_time`? If not, needs a small FFI shim (Task 1 settles this).
- **Two-clock setups:** if the user ever outputs through a different device than
  the Focusrite input, the streams aren't sample-locked. Documented limitation;
  nudge covers it.
- **Device-bound verification:** the PipeWire timing read can't be unit-tested
  against hardware in CI — correspondence MATH is unit-tested; the live read is
  manually verified. UI/audio paths need a real-app + chrome-console smoke test
  (see memory: ui-runtime-smoke-test).

## Progress log

- 2026-06-25: Diagnosed root cause (snapshot_last + stop-timing, not just RTL).
  Researched pro approach (Reaper/Audacity/Ardour: transport-lock + constant RTL,
  loopback ping). Brainstormed + got design approval. Spec + campaign written.
- 2026-06-25: Feasibility spike DONE. `pipewire` 0.10 exposes
  `Stream::time() -> Time` (wraps `pw_stream_get_time`); `pw_time` carries `now`
  (shared graph-clock ns), `ticks` (sample pos), `rate`, and `delay` (per-stream
  hardware latency). NO FFI shim needed. `pw_time.delay` also gives Part 2's
  reported-latency baseline for free. Note: engine has feature `v0_3_49`; `time()`
  falls back to the (deprecated but working) `pw_stream_get_time` — optionally bump
  to `v0_3_50` for the non-deprecated `_n` variant.
