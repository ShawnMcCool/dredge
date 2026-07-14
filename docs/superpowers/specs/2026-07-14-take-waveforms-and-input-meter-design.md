# Take waveforms + input level meter — design

## Context

The overdub-layers feature (record yourself over the track, `2026-06-25-overdub-layers-design.md`)
shipped, but two pieces the original design called for were never built, and
together they let a user record a **silent take without any feedback**:

1. **Take lanes render as placeholder blocks, not waveforms.** `Waveform.svelte`
   draws each take as a labelled tinted block (`// No per-sample peaks yet (v1)`).
   A silent take looks identical to a good one. No per-take peaks exist anywhere
   (neither `model::Recording` nor the TS `Recording` carry peaks; the server
   never computes them).
2. **No input signal feedback.** The Recordings box has a device picker but no
   level meter, so you arm and record blind. A dead/wrong input yields a WAV of
   digital silence (observed: a take at −90 dBFS, RMS 0.0).

Both are the same root problem — you can't *see* whether audio is present. This
design adds the two missing pieces.

## Feature 1 — real waveform lanes

**Peaks are computed server-side, reusing the song path.** `engine::peaks::compute_peaks(&SongBuffer) -> Peaks { frames_per_bucket, buckets: Vec<(min,max)> }`
already exists and is pure. `refresh_layers` (app.rs) already decodes each take's
WAV into an `Arc<SongBuffer>` and caches it (`layer_cache`); alongside it we
populate a `layer_peaks: HashMap<RecordingId, Peaks>` cache. Peaks are a
recomputable cache — **never persisted** to the manifest (exactly like the song's
own peaks).

**Delivery.** A take's peaks ride its JSON wherever recordings are emitted to the
frontend — `song.open`, `recording.list`, and the `recording.finished` event /
`recording.stop` return — via a `peaks` field added to the serialized recording
object (flatten Recording + peaks). The on-disk `BundleManifest` stays bare
`Recording`. A take whose WAV failed to decode carries `peaks: null`.

**Frontend.** The TS `Recording` interface gains `peaks: Peaks | null`. In
`Waveform.svelte` the placeholder block (line ~527) is replaced with a real
min/max waveform: for each peak bucket in view, draw a vertical span within the
lane, horizontally placed by `layerSpanSecs(anchor_frame, len_frames)` against the
shared `view` zoom/scroll — the same per-bucket loop the main waveform uses. The
take name stays as a small left-aligned caption; muted takes render dimmed. Lane
height grows from 18px so the shape is legible. Existing peaks pass through the
current store plumbing unchanged once the type includes `peaks`.

## Feature 2 — live input level meter

**Backend.** A new `crates/server/src/input_monitor.rs` mirrors `tuner.rs`: an
`InputMonitorControl` trait (Real + Mock), where `start(device_id, tx)` opens a
capture via `engine::capture::start_capture_by_id` and spawns a sampler thread
that, every ~50 ms, snapshots the ring and sends `InputLevel { peak, rms }`. The
App drains the channel in `tick()` and emits `input_level` events, exactly like
`tuner_pitch`. New commands `input.monitorStart { device_id }` /
`input.monitorStop`. `recording_start` stops the monitor first so it never
contends with the recorder for the device.

**Frontend.** A store `inputLevel` (set from the `input_level` event) and
`startInputMonitor`/`stopInputMonitor` actions. The Recordings box runs the
monitor **while armed and not recording** (bounded lifetime — you arm, watch the
meter, then hit record), on the currently-selected input; it restarts when the
input changes and stops on disarm / record / unmount. A compact horizontal meter
(accent-colored, clip indication near unity) renders in the arm area.

## Testing

- **Rust:** `layer_peaks` computed for a decoded take; `RecordingView` JSON
  round-trips with a `peaks` field; input-monitor level math (peak = max|s|, rms).
- **Frontend:** peak-bucket → lane x/y mapping (pure, in `recording-math` or a
  waveform-math helper); `Recording` store carries `peaks`.

## Out of scope

- Per-take solo / independent playback control (takes are layers, by design).
- Metering during the actual recording pass (recorder owns the device then).
- Persisted take peaks (recomputable cache only).

## Also fixed alongside

The Recordings box input/span dropdowns were wrapped in `<label>`, which in
WebKitGTK forwards an option click to the trigger button and re-opens the menu.
Replaced with plain `<div>` wrappers (a custom button widget is not a labelable
control). Committed with this work.
