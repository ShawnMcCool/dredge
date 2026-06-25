# Overdub layers — design

## Concept

Record yourself playing along with a track and hold each take as an additional
waveform layered onto the mix. The canonical use: mute the bass stem, play bass
along with the rest of the band, then listen back to how it sat.

A **layer** is a recording of your own input (mic / instrument), captured during
one finite linear pass over a span. Layers are **additive overdubs**: each is
separable, has its own gain / mute / nudge / waveform, and they all play back
together on top of the stem-adjusted track. Record again over an existing layer
and you get a second layer playing simultaneously (record rhythm, then lead over
it).

This stays inside the ear-first practice-looper identity. It is **not** a DAW:
layers are locked to the song's timeline — they share its tempo, time-stretch,
and loop. There is one playhead and one stretcher, exactly as today.

## Why this fits the engine grain

dredge already mixes multiple sources (`StemSet`) and stretches the **mixed**
output with a single post-mix Rubber Band instance (`pipeline.rs`,
`render_core.rs`). The decisive consequence: if layers are summed into the
**pre-stretch** mix buffer at their source-time offset, then slowing the track
to 50% slows every layer identically and pitch-preserved, so alignment holds at
any tempo for free. One stretcher and one playhead are not limitations here —
they are *why* overdubs stay in sync.

The one place the current engine is rigid against this: `StemSet` pads every
source to equal length from frame 0 (`buffer.rs`). A layer starts at an
arbitrary frame and is shorter than the song, so layers are **not** folded into
`StemSet`. They are a separate source type with an offset and a length, mixed in
alongside the looper's output. That "offset source" is the only new engine
primitive this feature introduces.

## Recording model

A recording always captures **one pass** over a span:

1. Count-in plays (existing click pre-roll); **not captured**.
2. At count-in end: playback starts at the span's start frame **and** input
   capture starts.
3. Capture runs to the span's end frame (or until the user stops early), then
   the WAV is written.

The span is the user's explicit choice at record time, one of:

- **Full song** — span start → end of song.
- **Selection** — the current waveform selection span.
- **Loop** — the active loop region, one pass (no infinite repeat).

Looping repeat is suspended during recording; we capture a single linear pass.
Monitoring is **external only** — dredge plays the track + existing layers; you
hear yourself acoustically or through your interface's direct monitoring. No
software monitoring (it would add capture+playback latency to your own sound).

## Sync & calibration

A recording's sample 0 is anchored to the playback source-frame at capture start
(the span start). Round-trip latency means the captured transient lands late, so
effective timeline placement is:

```
effective_start_frame = anchor_frame - input_latency_frames - nudge_frames
```

- **`input_latency_frames`** — a global setting, seeded once by an **acoustic
  loopback calibration**: emit a click out the output, simultaneously record it
  back through the input, detect the click onset in the recording; onset frame =
  round-trip latency. Stored in the settings table; reused for every recording.
- **`nudge_frames`** — per-layer manual fine-tune (a ± control in ms) on top of
  the global offset, dialed in by ear.

We deliberately do not chase sample-accurate cross-thread position reporting:
anchoring to the known span start and correcting with the calibrated offset +
per-layer nudge is the pragmatic model the user chose.

## Engine changes (`crates/engine`)

- **New source type** mixed alongside the looper, e.g.
  `Layer { samples: Arc<SongBuffer>, start_frame: i64, gain: f32, muted: bool }`.
  The render core holds a `Vec<Layer>`. For the source-frame window being
  produced each block, layers that overlap the window are summed into the mix
  buffer **before** the stretcher. A negative or partial overlap is clamped.
  Because layers are read by source-frame, loop wrap and crossfade re-read them
  correctly with no extra logic.
- **New `EngineCmd` variants:** `AddLayer`, `RemoveLayer { id }`,
  `SetLayerGain { id, gain }`, `SetLayerMute { id, muted }`,
  `SetLayerOffset { id, start_frame }` (for nudge / calibration re-apply). Layer
  buffers are loaded on the control thread and handed over by `Arc` swap, never
  allocated on the RT thread.
- **Targeted modularity improvement:** to avoid fattening the already-large
  `Pipeline::render_song()` (1141 lines), extract the "sum active layers into the
  pre-stretch buffer at offset" step into a small dedicated helper rather than
  inlining it into the render chain. No broader refactor of the signal chain.
- **Recording orchestration is server-side** (see below); the RT engine only
  gains layer playback. Capture reuses `capture.rs`'s input stream, but drains
  the full stream to a growing buffer instead of the tuner's rolling window.

## Recording orchestration (`crates/server`)

New module `recording.rs`, mirroring `tuner.rs`: opens an input capture session
(reusing `engine::capture`), drains frames to a buffer for the span duration,
then writes the WAV. Calibration's click-detection lives here too. Both the
WAV-write+peak-compute on finish and the calibration routine run **outside the
`App` mutex** via the `*_phased` pattern, so they never block the tick pump.

## Data model & persistence (`crates/practice`)

- WAVs at `<bundle_dir>/recordings/<id>.wav` (48k stereo f32, reuse
  `capture::write_wav`). Travels with the bundle; the manifest stores a relative
  path so it rebases on copy, like the song audio.
- New wire type in `model.rs`:
  `Recording { id, name, file, anchor_frame, len_frames, nudge_frames, gain,
  muted, created_at }`.
- `BundleManifest` gains `#[serde(default)] recordings: Vec<Recording>` so
  existing bundles load unchanged (`bundle.rs`).
- Per-recording **peaks** computed on load (reuse `peaks.rs`), cached by file
  hash, surfaced to the frontend like the song's peaks. `gain` / `muted` /
  `nudge_frames` are persisted playback state; `name` defaults to "take 1",
  "take 2", … (occurrence-style, matching the section-notes naming feel).

## Command surface (`crates/server/src/app.rs`)

- `recording.start { span: "song"|"selection"|"loop", device_id }` — arm,
  count-in, record. Returns once started; completion via event.
- `recording.stop` — stop the current pass early.
- `recording.list` — recordings for the current song (also included in the
  `song.open` response).
- `recording.rename { id, name }`, `recording.delete { id }`.
- `recording.setGain { id, gain }`, `recording.setMute { id, muted }`,
  `recording.setNudge { id, nudge_ms }`.
- `recording.calibrate` — run loopback click calibration, store the measured
  offset, return the latency. (Reuse `device.inputs` for device enumeration.)
- **Events:** `recording.started`, `recording.finished { recording }`.

## UI (`apps/desktop/src`)

- **New recordings control box** (`Recordings.svelte`, built on `Box`) on the
  stage. Body:
  - a span selector `[ Full song · Selection · Loop ]`,
  - an input-device picker (reuse `device.inputs`),
  - a **Record** button (uses the existing count-in setting), with a recording
    indicator + early-stop while a pass is live,
  - one row per layer — name, level fader, **M**ute, nudge ± (ms), delete.

  Active / on states use the theme accent (`--accent` / `--accent-dim`), per the
  project's color convention. The box appears once the song has at least one
  recording or recording is available.
- **Stacked waveform lanes:** each layer renders as its own thin waveform lane
  beneath the main waveform, sharing the main waveform's zoom and scroll and
  time-aligned to it. Reuse the existing peak-rendering path.
- **Calibration** lives in the **settings tab** (one-time action + the resulting
  latency value), not in the control box.
- State mirrors the wire shapes in `lib/stores.ts`; no second source of truth.
  Components reach the backend only through `lib/ipc.ts`.

## Testing

- **Rust:** layer mix placement + gain math (offset window summed correctly,
  partial-overlap clamping); span resolution (song / selection / loop →
  `[start, end)`); calibration onset-detection on a synthetic click with a known
  injected offset → recovered offset within tolerance; `BundleManifest`
  round-trip with `recordings`; bundle-move keeps `recordings/`.
- **Frontend:** recordings store mirrors the wire shape; ms↔frames nudge
  conversion; layer-lane zoom/scroll alignment reuses the waveform-math tests.

## Out of scope (first version)

- Software monitoring (external monitoring only — decided).
- Trimming / fades / comping takes; per-layer effects or filters.
- Independent per-track tempo or loop region; per-track stretchers; multi-rate.
  (These are DAW features; explicitly not this product.)
- Mixdown export including layers — possible later in the export tab; noted, not
  built now.
- Recording over a repeating loop — one pass only.
