# Guitar Tuner — Design Spec

**Date:** 2026-06-14
**Status:** Approved, ready for implementation planning

## Context

Dredge is an ear-first practice looper. Tuning up is the first thing you do
before a practice session, so a tuner belongs *on the practice surface*, not in
a separate utility. This spec adds a chromatic tuner as a first-class **box** in
the stage stack, listening to a live instrument input, showing the detected note
and how many cents sharp/flat it is.

There is no pitch detection or live instrument-input capture in the codebase
today: existing capture (`crates/engine/src/capture.rs`) is output-only (it taps
app audio like Spotify via `media.class == "Stream/Output/Audio"`), and the only
DSP is biquad filters + waveform peaks. Both are net-new here; everything else
follows existing patterns.

## Goals

- A chromatic tuner (auto-detect nearest note + cents) for any instrument,
  driven by a live audio input chosen by the user.
- Make it *feel* good: calm when idle, stable readings, confident "you nailed
  it" feedback.
- Fit dredge's architecture — one shared data path, a pluggable view, state
  derived from dispatch events, settings in the SQLite settings table.

## Non-goals (v1)

- Multiple gauge styles shipped at once (architect pluggable; ship one).
- Guitar-string presets / alternate tunings (chromatic only).
- Adjustable reference pitch (fixed A4 = 440 Hz).
- Handling the mic-hears-speakers case (user tunes via a fixed direct input).

## UX / behavior

**Placement.** A new **tuner box** in the stage stack, positioned **below the
stems and structure boxes**. Same `box` vocabulary as controls/stems/structure/
analyze (label header + body).

**Power model.** The box is **disabled by default**, showing a dim header with a
**power button** (⏻). Clicking power enables the box: it starts listening and
shows the live readout. Clicking power again disables it and stops capture. The
power button *is* the listen control — no separate start/stop.

**Input device.** Selected behind a **gear icon** in the header (keeps the box
clean). The choice is **maximally sticky**: persisted to settings, restored on
launch, and reused indefinitely. The system default input is used only on the
very first run before any choice exists.

**Live states** (confidence-gated so the readout stays calm on noise/silence):

1. **Off** — dim box, power button only.
2. **On, no steady pitch** — "listening… play a note", meter parked at center,
   neutral colour. The detector only commits to a note once the pitch is steady
   (confidence above threshold), so noise/transients never flail the gauge.
3. **Holding a note, out of tune** — shows note + octave (e.g. `A2`), cents as
   `+27¢ sharp` / `−12¢ flat`, marker off-center, **amber**.
4. **In tune** — marker centered, **green**, `+2¢ ✓`.

**Hold-to-lock feedback.** When the note sits in tune (within the in-tune
threshold, ~±5¢) for ~½ second, fire a brief pulse + a "locked" badge —
confirming the note was actually tuned, not just swept past. Thresholds and
timing are tunable later.

**Display details.** Always show **octave** (E2 vs E3 distinguishes strings).
Cents read as signed integers with a sharp/flat word.

**Gauge style.** Ship **one** style — a **linear center-zero meter** (reuses
dredge's existing meter/fader visual language, least custom code). The view is
**pluggable**: the gauge is a presentational component taking
`{ note, octave, cents, hz, confidence, inTune, locked }`, so needle / strobe /
other styles can be added later as sibling components selected by a setting.
That setting is not exposed in v1 (only the meter exists), but the seam is built.

## Architecture

Everything expensive is shared and built once; only the view multiplies later.

```
mic/input capture → pitch detection + smoothing → note/cents math
   → tuner_pitch event stream → tunerData store → pluggable gauge view
```

**Streaming approach: dedicated sampler thread** (chosen over emitting from the
engine render loop). It mirrors the existing `work_sample` sampler
(`crates/server/src/sampler.rs`): a thread reads a short live input ring every
~50 ms, runs detection + smoothing off the audio thread, and sends readings over
an mpsc channel that `App::tick()` drains into `tuner_pitch` events. This keeps
the tuner fully decoupled from the real-time playback engine and makes
start/stop trivial (tied to the power button).

### Engine (`crates/engine`)
- **Input capture.** Extend node enumeration to surface **input/source** nodes
  (`Audio/Source`), alongside today's output-only discovery in `capture.rs`. Add
  a capture session targeting the chosen source serial, feeding a small
  `RollingRing` (~100–200 ms window — no large buffer needed). Reuse the existing
  `RollingRing` (`ring.rs`) and the 48 kHz f32 format.
- **`pitch.rs` (new).** Pitch detection via the **`pitch-detection` crate**
  (McLeod/YIN; pure Rust, pulls `rustfft` transitively, no system/`-sys` deps).
  Input: a window of mono f32 samples (downmix the captured stereo). Output:
  fundamental Hz + a confidence/clarity value. Includes **smoothing/damping**
  (e.g. exponential smoothing + a confidence gate + note hysteresis) — this is
  the shared quality work that determines how good *any* gauge feels.

### Server (`crates/server`)
- **Commands** (follow the `capture.*` pattern in `app.rs`):
  - `tuner.inputs` — list available input/source devices.
  - `tuner.start` — begin capture + detection on a device id; starts the sampler.
  - `tuner.stop` — stop capture + sampler.
- **Tuner sampler thread** — new module modeled on `sampler.rs`; produces
  readings; `App::tick()` drains them into `Event { event: "tuner_pitch", … }`.
- **Settings** — persist the sticky input device + (future) gauge style in the
  existing **settings table** (V3 JSON settings pattern in `store.rs`). No schema
  bump needed if it fits the existing settings JSON; otherwise add a settings
  key, not a new table.

### Frontend (`apps/desktop/src`)
- **`components/Tuner.svelte` (new)** — the box; renders in the stage stack below
  the stems/structure boxes (wire into the stage box list in the stage component
  / `App.svelte` region that renders boxes). Header with power button + gear;
  body with the gauge. Powers `tuner.start`/`tuner.stop` on toggle.
- **Gauge component** — `MeterGauge.svelte` (or similar) taking the reading
  props; the pluggable seam for future styles.
- **Store** — `tunerData` in `lib/stores.ts` mirroring the wire shape; a case for
  `tuner_pitch` in `initEvents()`; actions for inputs/start/stop via `cmd()`.
- **Pure logic** — Hz→note+octave+cents in `lib/` (e.g. `tuner-math.ts`) with a
  colocated `*.test.ts`. Reference A4 = 440. This is the unit-testable core.

## Data shapes

Event pushed ~20 Hz while listening:

```
{ "event": "tuner_pitch",
  "data": { "hz": 110.3, "note": "A", "octave": 2, "cents": 5,
            "confidence": 0.94, "inTune": true } }
```

`locked` is derived UI-side from a sustained `inTune` run (hold-to-lock timing
lives in the component, not the wire).

## Component boundaries

- **Capture** — owns the input stream + ring; knows nothing about pitch.
- **Pitch detector** (`pitch.rs`) — pure-ish: samples in, Hz + confidence out;
  no I/O, unit-testable on synthesized sines.
- **Sampler thread** — orchestration: ring → detector → smoothing → channel.
- **Tuner-math (`lib/`)** — pure Hz→note/cents; unit-tested in isolation.
- **Gauge component** — presentational only; same props regardless of style.

## Testing / verification

- **Unit (frontend):** Hz→note+cents table — 440→A4/0¢, 466.16→A#4/0¢, plus a
  few sharp/flat offsets and octave boundaries.
- **Unit (Rust):** detector returns expected Hz (±tolerance) on synthesized
  sine waves across the guitar range (82 Hz low E → ~1.2 kHz); confidence is
  high on clean tones and low on noise/silence.
- **Rust:** input/source enumeration returns source nodes.
- **End-to-end:** `just dev`, power the box on, play open A (110 Hz) and low E
  (82 Hz) on the interface; confirm correct note naming, the meter tracking
  bends, confidence-gating staying calm on silence, and hold-to-lock firing.

## Dependency impact

- **Compile-time:** one new Rust crate (`pitch-detection`), pure Rust; pulls
  `rustfft` transitively — no `build.rs` C compilation, no new system/`-sys`
  deps, no new bindgen/clang surface beyond the engine's existing requirement.
  No new pnpm packages (gauge is Svelte + SVG/CSS).
- **Runtime:** none new. PipeWire is already required (playback + output
  capture); input capture uses the same daemon. No Python, nothing downloaded
  on first run; detection is statically linked.
