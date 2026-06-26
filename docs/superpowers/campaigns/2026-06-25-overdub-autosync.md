# Campaign: Overdub auto-sync

**Status:** CORE COMPLETE — verified on hardware 2026-06-26. Transport-locked
capture + auto RTL trim (64ms) work; count-in excluded, early-stop clamped,
from-playhead anchored ("seems ok"). Only optional AS-7 (loopback calibration)
remains, deferred. Started 2026-06-25.
**Spec:** `docs/superpowers/specs/2026-06-25-overdub-autosync-design.md`
**Plan:** `docs/superpowers/plans/2026-06-25-overdub-autosync.md`
**Research/reference (critical — read before touching alignment):** `docs/research/recording-latency-compensation.md`
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

| # | Task | Status |
|---|------|--------|
| — | spec written + committed | DONE |
| — | plan written + committed | DONE |
| 1 | Feasibility spike: PipeWire stream timing API (FFI needed?) | DONE — no FFI; `Stream::time()` exists |
| AS-1 | RollingRing monotonic frame index + absolute-range read | DONE (52dec02) |
| AS-2 | StreamClock timing snapshot + mapping math | DONE (b524c35, +fix) |
| AS-3 | Capture stream clock + input delay (device-bound) | DONE (5e73aa4) |
| AS-4 | Playback song-frame clock + output delay (device-bound) | DONE (ce5e514) |
| AS-5 | Transport-locked take extraction | DONE (b548249) |
| AS-5b | Count-in fix: pin anchor at playback start | DONE (3c7dcff) — **awaiting re-test** |
| — | Recordings UI: "default (follow devices)" + "from playhead" | DONE (35067c2) |
| AS-5c | Clamp take to captured frames (early-stop fallback re-included count-in) | DONE — verified on hardware |
| AS-6 | Part 2: PipeWire-reported latency baseline | DONE (ab0a83d) — VERIFIED (rtl=3072/64ms, "tighter") |
| AS-5d | Fix negative ring_start (from-playhead recording) | DONE — awaiting hardware test |
| — | Logging: DREDGE_DEBUG keeps stderr; breadcrumb; `just logs` | DONE — tested via daemon |
| AS-7 | Part 2: loopback ping calibration | DEFERRED — AS-6 auto-trim works; needs a cable to test |
| AS-8 | Full gate + cleanup | gate GREEN; spawn-args struct refactor deferred (don't churn working audio) |

**Part 1 = AS-1..5 + AS-5b: CODE-COMPLETE.** Blocking gate: user records a take
on the Focusrite (with count-in) and confirms it lands in time. Then Part 2.

## Risks / open questions

- ~~**PipeWire timing binding:** needs an FFI shim?~~ RESOLVED — `Stream::time()`
  exists in the crate; no FFI. Field names confirmed against bindgen output.
- **pw_time field semantics on hardware:** still unverified — `rate` direction
  (frames/sec = denom/num), `delay` units/sign, `ticks` advance. `DREDGE_DEBUG=1`
  prints them once per stream. If alignment is off, check these first.
- **Two-clock setups:** if the user ever outputs through a different device than
  the Focusrite input, the streams aren't sample-locked. Documented limitation;
  nudge covers it.
- **Device-bound verification:** the PipeWire timing read can't be unit-tested
  against hardware in CI — correspondence MATH is unit-tested; the live read is
  manually verified. UI/audio paths need a real-app + chrome-console smoke test
  (see memory: ui-runtime-smoke-test).

## Cleanup candidates (do in AS-8)

- `output::spawn`/`run` now take 8 args (`#[allow(too_many_arguments)]`). Bundle
  the slots (song_slot, click_slot, layer_slot, playback_clock) into a struct —
  per the keep-code-modular directive.
- AS-2's `stream_clock.rs` was missed by a `git commit -am` (fixed in a follow-up
  commit). Implementer dispatches must use explicit `git add` for new files.

## Field feedback (during testing)

- **Count-in delay bug — TWO causes, second was the real one:**
  - First attempt (3c7dcff): pin `ring_start` at the first real-playback tick
    instead of extrapolating back from span-end. Necessary but not sufficient.
  - **Real root cause** (next commit): the user picked "full song" but STOPPED
    EARLY (a few seconds). `extract_range(ring_start, full_song_len)` overran the
    ring (only seconds captured) → `None` → fell back to `snapshot_last`, which
    returns ALL captured audio INCLUDING the count-in. That's also why it
    "worked without a count-in" (captured-from-start happens to begin at song-0).
    Fix: clamp `take_len = min(len_frames, ring_total - ring_start)` so the take
    stays anchored at count-in-end and is simply shorter — never hits the
    count-in-including fallback. Awaiting re-test.
  - Note: `DREDGE_DEBUG=1 ./cmd` does NOT set the env in **fish** (user's shell);
    use `env DREDGE_DEBUG=1 ./target/release/dredge 2>log`. The empty debug log
    in the first re-test was this, not a stale binary (binary had the strings).
- **UI** (35067c2): recordings input now has "default (follow devices)" (mirrors
  the tuner via shared `resolveInputDevice` — renamed from `resolveTunerInput`),
  and a "from playhead" span option (maps to a selection from the playhead to
  song end). svelte-check clean; vite+chrome smoke test confirmed no crash.

## Progress log

(Newest first.)

- 2026-06-26: From-playhead anchor fix verified ("seems ok"). **Overdub
  auto-sync core feature COMPLETE & hardware-verified** across all paths:
  transport-locked capture, count-in exclusion, early-stop clamp, auto RTL trim
  (64ms), from-playhead. Full `just check` green. Optional AS-7 (loopback
  calibration) deferred — auto-trim + nudge are sufficient; build on request.
  Spawn 8-arg struct refactor left as a future cleanup (don't churn working
  audio). DREDGE_DEBUG diagnostic prints kept (gated, now reachable via `just
  logs`).

- 2026-06-26: **Logging side-quest (resolved + tested autonomously).** The
  "empty debug log" mystery was `redirect_if_headless` funnelling backend stderr
  into `~/.local/share/dredge/dredge.log` whenever stderr isn't a terminal — the
  user's `2>file` got overridden. Fix: `DREDGE_DEBUG` now skips the redirect (so
  `env DREDGE_DEBUG=1 ./dredge 2>file` works), a non-debug `2>file` gets a
  breadcrumb, `just logs` tails it; documented in DEVELOPMENT.md. Verified via
  the headless daemon (both redirect + no-redirect paths).
  **Bonus from reading the existing log — real values:** `rate.num=1
  rate.denom=48000` (frames/sec = denom/num CONFIRMED); `rtl[auto]:
  output_delay=1536 input_delay=1536 rtl=3072` (64 ms, sign correct — why it
  sounded "tighter"); count-in anchor correct (`ring_start≈count-in-end`).
  **Found bug:** recording **from a mid-song playhead** yields a NEGATIVE
  `ring_start` (capture starts after playback is already at the anchor) →
  falls back to `snapshot_last`. Needs a fix (clamp negative → 0, or start
  capture before seeking). AS-6 (auto RTL) VERIFIED working.

- 2026-06-25: Field testing → two changes. (1) Recordings UI: "default (follow
  devices)" input + "from playhead" span (35067c2). (2) Count-in delay fix
  (3c7dcff) — pin the take anchor at the first real-playback tick instead of
  extrapolating from span-end across the count-in. Rebuilt release; awaiting
  on-Focusrite re-test of the count-in case before Part 2.
- 2026-06-25: Diagnosed root cause (snapshot_last + stop-timing, not just RTL).
  Researched pro approach (Reaper/Audacity/Ardour: transport-lock + constant RTL,
  loopback ping). Brainstormed + got design approval. Spec + campaign written.
- 2026-06-25: Part 1 (AS-1..5) CODE-COMPLETE. Ring absolute indexing (52dec02),
  StreamClock math (b524c35 + fix), capture clock (5e73aa4), playback clock
  (ce5e514), transport-locked extraction + auto-finalize at span end (b548249).
  Needs on-Focusrite verification (DREDGE_DEBUG pw_time + a take that lands in
  time) before Part 2 (RTL calibration) builds on the validated timing.
  Wrote research/reference doc `docs/research/recording-latency-compensation.md`.
- 2026-06-25: Feasibility spike DONE. `pipewire` 0.10 exposes
  `Stream::time() -> Time` (wraps `pw_stream_get_time`); `pw_time` carries `now`
  (shared graph-clock ns), `ticks` (sample pos), `rate`, and `delay` (per-stream
  hardware latency). NO FFI shim needed. `pw_time.delay` also gives Part 2's
  reported-latency baseline for free. Note: engine has feature `v0_3_49`; `time()`
  falls back to the (deprecated but working) `pw_stream_get_time` — optionally bump
  to `v0_3_50` for the non-deprecated `_n` variant.
