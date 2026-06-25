# Overdub Layers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Record yourself playing along with a track and hold each take as an additive, separable layer that plays back over the stem-adjusted mix, time-aligned through tempo changes.

**Architecture:** A layer is an "offset source" (`samples`, `start_frame`, `gain`, `muted`) summed into the **pre-stretch** mix buffer so it rides the song's single stretcher and stays aligned + pitch-preserved at any speed. Layers cross the audio-thread boundary via a new `layer_slot: ArcSwapOption<Vec<Layer>>`, mirroring the existing `song_slot`/`click_slot` — **no new `Copy` `EngineCmd` variants** (an `EngineCmd` can't carry an `Arc`). Recording orchestration lives server-side in a new `recording.rs` behind a `RecordingControl` trait (real PipeWire impl + a fake for tests), persisting each take as a WAV in the song bundle. The UI adds a `Recordings` control box, stacked waveform lanes, and a settings-tab calibration action.

**Tech Stack:** Rust (engine/server/practice crates, `rtrb` lock-free rings, `arc-swap`, `hound` WAV, `rusqlite` settings), Svelte 5 + Tauri frontend (vitest), `just` task runner.

---

## File Structure

**Engine (`crates/engine/src`):**
- `layers.rs` — *new*. `Layer` struct + pure `mix_layers()`. One responsibility: place layer audio into a pre-stretch buffer by absolute source frame.
- `lib.rs` — *modify*. Register `pub mod layers;`.
- `pipeline.rs` — *modify*. `Pipeline` holds `layers: Arc<Vec<Layer>>`; `set_layers()`; mix layers into the feed buffer after each looper read.
- `render_core.rs` — *modify*. Hold `layer_slot`, detect swaps, forward to `pipeline.set_layers()` (exactly like the click schedule).
- `engine.rs` — *modify*. Own `layer_slot`; `set_layers()` public method; pass slot to `output::spawn`.
- `output.rs` / `output_cpal.rs` — *modify*. `spawn()` takes `layer_slot` and passes it to `RenderCore::new`.

**Practice (`crates/practice/src`):**
- `model.rs` — *modify*. `RecordingId` + `Recording` wire type.
- `bundle.rs` — *modify*. `recordings: Vec<Recording>` on `BundleManifest`.

**Server (`crates/server/src`):**
- `recording.rs` — *new*. `RecordingControl` trait, `RealRecorder`, pure `detect_click_onset()`, pure `resolve_span()`.
- `app.rs` — *modify*. `recording.*` dispatch arms, layer-rebuild + WAV/peaks/manifest write, events, `recordings` in `song.open`.

**Frontend (`apps/desktop/src`):**
- `lib/recording-math.ts` — *new*. Pure ms↔frames helpers.
- `lib/recording-math.test.ts` — *new*.
- `lib/stores.ts` — *modify*. `Recording` interface, `recordings` store, actions, event wiring, `song.open` consumption.
- `components/Recordings.svelte` — *new*. Control box.
- `components/Waveform.svelte` — *modify*. Stacked layer lanes.
- `components/SettingsPanel.svelte` — *modify*. Calibration action.
- `App.svelte` — *modify*. Mount `Recordings` in the stage.

---

## Task 1: `Layer` source type + `mix_layers` (engine)

**Files:**
- Create: `crates/engine/src/layers.rs`
- Modify: `crates/engine/src/lib.rs`
- Test: in-file `#[cfg(test)] mod tests` in `layers.rs`

- [ ] **Step 1: Register the module**

In `crates/engine/src/lib.rs`, add the module declaration alongside the other `pub mod` lines (e.g. next to `pub mod buffer;`):

```rust
pub mod layers;
```

- [ ] **Step 2: Write the failing test file**

Create `crates/engine/src/layers.rs` with the struct, a stub, and tests:

```rust
//! Overdub layers: recorded performances placed on the song timeline by
//! absolute source frame and summed into the *pre-stretch* mix buffer, so they
//! ride the song's single stretcher (aligned + pitch-preserved at any tempo).

use crate::buffer::{SongBuffer, CHANNELS};
use std::sync::Arc;

/// One recorded take placed on the song timeline.
#[derive(Debug, Clone)]
pub struct Layer {
    /// Interleaved stereo f32 at 48 kHz (the recorded audio).
    pub samples: Arc<SongBuffer>,
    /// Absolute source frame where `samples` frame 0 sits. May be negative
    /// after latency/nudge compensation; the leading portion is then clipped.
    pub start_frame: i64,
    /// Playback gain (0.0..=1.5).
    pub gain: f32,
    /// When true the layer contributes nothing.
    pub muted: bool,
}

/// Add every active layer's audio for the contiguous source-frame window
/// `[src_start, src_start + out.len()/CHANNELS)` into `out` (which already holds
/// the looper's mixed track frames for the same window). Frames outside a
/// layer's extent contribute nothing.
pub fn mix_layers(layers: &[Layer], src_start: usize, out: &mut [f32]) {
    let frames = out.len() / CHANNELS;
    for layer in layers {
        if layer.muted || layer.gain == 0.0 {
            continue;
        }
        let len = layer.samples.frames() as i64;
        for i in 0..frames {
            let abs = src_start as i64 + i as i64;
            let local = abs - layer.start_frame;
            if local < 0 || local >= len {
                continue;
            }
            let li = (local as usize) * CHANNELS;
            let oi = i * CHANNELS;
            out[oi] += layer.samples.data[li] * layer.gain;
            out[oi + 1] += layer.samples.data[li + 1] * layer.gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constant(frames: usize, v: f32) -> Arc<SongBuffer> {
        Arc::new(SongBuffer {
            data: vec![v; frames * CHANNELS],
        })
    }

    fn layer(start_frame: i64, frames: usize, v: f32) -> Layer {
        Layer {
            samples: constant(frames, v),
            start_frame,
            gain: 1.0,
            muted: false,
        }
    }

    #[test]
    fn places_layer_at_its_offset_and_leaves_the_rest_untouched() {
        let layers = vec![layer(100, 10, 0.5)];
        let mut out = vec![0.0f32; 6 * CHANNELS]; // window [98, 104)
        mix_layers(&layers, 98, &mut out);
        // frames 98,99 are before the layer → silent
        assert_eq!(out[0], 0.0);
        assert_eq!(out[1 * CHANNELS], 0.0);
        // frames 100..104 carry the layer
        for f in 2..6 {
            assert!((out[f * CHANNELS] - 0.5).abs() < 1e-6, "frame {f}");
            assert!((out[f * CHANNELS + 1] - 0.5).abs() < 1e-6, "frame {f}");
        }
    }

    #[test]
    fn adds_onto_existing_track_audio() {
        let layers = vec![layer(0, 4, 0.25)];
        let mut out = vec![0.1f32; 4 * CHANNELS]; // pretend the track is at 0.1
        mix_layers(&layers, 0, &mut out);
        for s in &out {
            assert!((s - 0.35).abs() < 1e-6, "got {s}");
        }
    }

    #[test]
    fn muted_layer_contributes_nothing() {
        let mut l = layer(0, 4, 0.5);
        l.muted = true;
        let mut out = vec![0.0f32; 4 * CHANNELS];
        mix_layers(&[l], 0, &mut out);
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn gain_scales_contribution() {
        let mut l = layer(0, 4, 0.4);
        l.gain = 0.5;
        let mut out = vec![0.0f32; 4 * CHANNELS];
        mix_layers(&[l], 0, &mut out);
        for s in &out {
            assert!((s - 0.2).abs() < 1e-6, "got {s}");
        }
    }

    #[test]
    fn negative_start_frame_clips_the_leading_portion() {
        // layer starts 2 frames before 0; window [0,4) sees its frames 2..6
        let layers = vec![layer(-2, 8, 0.5)];
        let mut out = vec![0.0f32; 4 * CHANNELS];
        mix_layers(&layers, 0, &mut out);
        for s in &out {
            assert!((s - 0.5).abs() < 1e-6, "got {s}");
        }
    }
}
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cargo test -p engine layers::tests`
Expected: all 5 tests PASS (the implementation is included above; if a test fails, fix `mix_layers`, not the test).

- [ ] **Step 4: Lint**

Run: `cargo clippy -p engine -- -D warnings`
Expected: no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/layers.rs crates/engine/src/lib.rs
git commit -m "feat(engine): Layer source type + pre-stretch mix_layers"
```

---

## Task 2: Plumb layers through the engine to audible output

**Files:**
- Modify: `crates/engine/src/pipeline.rs` (struct field, `set_layers`, feed-loop mix)
- Modify: `crates/engine/src/render_core.rs` (`layer_slot`, swap detection, forward)
- Modify: `crates/engine/src/engine.rs` (`layer_slot`, `set_layers`, pass to spawn)
- Modify: `crates/engine/src/output.rs` and `crates/engine/src/output_cpal.rs` (`spawn` signature)
- Test: in-file `#[cfg(test)] mod tests` in `render_core.rs`

- [ ] **Step 1: Add the layer field + setter to `Pipeline`**

In `crates/engine/src/pipeline.rs`, add the import near the top:

```rust
use crate::layers::{mix_layers, Layer};
```

Add a field to the `Pipeline` struct (after the `clicks: Arc<Vec<ClickMark>>` field):

```rust
    /// Overdub layers, mixed into the feed buffer before the stretcher.
    layers: Arc<Vec<Layer>>,
```

In `Pipeline::new`, initialise it (alongside the other field initialisers):

```rust
            layers: Arc::new(Vec::new()),
```

Add the setter method on `impl Pipeline` (next to `set_click_schedule`):

```rust
    pub fn set_layers(&mut self, layers: Arc<Vec<Layer>>) {
        self.layers = layers;
    }
```

- [ ] **Step 2: Mix layers into the feed buffer in `render_song`**

In `pipeline.rs`, in the feed loop of `render_song` (the `while self.playing && self.stretch.available() < frames_req` block), mix layers immediately after **each** read, before `self.stretch.feed(...)`. There are two read sites — the capping branch (`read_contiguous`) and the normal branch (`self.looper.read`). Capture the source start before each read and mix over the frames produced.

Capping branch — change:

```rust
                let cap = (end - pos).min(want);
                let n = self
                    .looper
                    .read_contiguous(&mut self.feed_buf[..cap * CHANNELS], cap);
                if n > 0 {
                    self.stretch.feed(&self.feed_buf[..n * CHANNELS]);
                }
                continue;
```

to:

```rust
                let cap = (end - pos).min(want);
                let n = self
                    .looper
                    .read_contiguous(&mut self.feed_buf[..cap * CHANNELS], cap);
                if n > 0 {
                    mix_layers(&self.layers, pos, &mut self.feed_buf[..n * CHANNELS]);
                    self.stretch.feed(&self.feed_buf[..n * CHANNELS]);
                }
                continue;
```

Normal branch — change:

```rust
            let info = self.looper.read(&mut self.feed_buf[..want * CHANNELS]);
            if info.wrapped {
                events.push(EngineEvent::LoopWrapped);
            }
            if info.frames > 0 {
                self.stretch.feed(&self.feed_buf[..info.frames * CHANNELS]);
            }
```

to:

```rust
            let src_start = self.looper.pos_frames();
            let info = self.looper.read(&mut self.feed_buf[..want * CHANNELS]);
            if info.wrapped {
                events.push(EngineEvent::LoopWrapped);
            }
            if info.frames > 0 {
                mix_layers(&self.layers, src_start, &mut self.feed_buf[..info.frames * CHANNELS]);
                self.stretch.feed(&self.feed_buf[..info.frames * CHANNELS]);
            }
```

(Within a single `read`, `pos` advances contiguously from `src_start`, including across a crossfade — the looper returns at the wrap boundary — so `src_start + i` is the correct absolute source frame for output frame `i`.)

- [ ] **Step 3: Add `layer_slot` to `RenderCore` and forward swaps**

In `crates/engine/src/render_core.rs`:

Add the import:

```rust
use crate::layers::Layer;
```

Add the field to `RenderCore` (after `click_slot`):

```rust
    layer_slot: Arc<ArcSwapOption<Vec<Layer>>>,
    current_layers: Option<Arc<Vec<Layer>>>,
```

Add the `new` parameter (after `click_slot: Arc<ArcSwapOption<Vec<ClickMark>>>,`) and initialiser:

```rust
        layer_slot: Arc<ArcSwapOption<Vec<Layer>>>,
```

```rust
            layer_slot,
            current_layers: None,
```

In `fill`, after the click-schedule swap block and before the command drain, add a layer swap block mirroring the click one (also re-apply on `swapped`, since a song swap builds a fresh pipeline):

```rust
        // Layer-set swap: detect by pointer like the click slot; re-apply on a
        // song swap too, since that built a fresh pipeline.
        let lguard = self.layer_slot.load();
        let lswapped = match (lguard.as_ref(), self.current_layers.as_ref()) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        };
        if lswapped || swapped {
            let layers = (*lguard).clone();
            if let Some(p) = self.pipeline.as_mut() {
                p.set_layers(layers.clone().unwrap_or_default().into());
            }
            self.current_layers = layers;
        }
```

- [ ] **Step 4: Thread `layer_slot` through `engine.rs` and both `spawn`s**

In `crates/engine/src/engine.rs`:

Add the field to `Engine` (after `click_slot`):

```rust
    layer_slot: Arc<ArcSwapOption<Vec<crate::layers::Layer>>>,
```

In `start`, create the slot and pass it to `spawn` and store it:

```rust
        let layer_slot = Arc::new(ArcSwapOption::<Vec<crate::layers::Layer>>::empty());
```

Update the `crate::output::spawn(...)` call to pass `layer_slot.clone()` after `click_slot.clone()`, and add `layer_slot` to the returned `Self { ... }`.

Add the public setter (next to `load`):

```rust
    /// Replace the active overdub layer set (atomic pointer swap; the audio
    /// thread picks it up on its next block).
    pub fn set_layers(&self, layers: Vec<crate::layers::Layer>) {
        self.layer_slot.store(Some(Arc::new(layers)));
    }
```

In **both** `crates/engine/src/output.rs` and `crates/engine/src/output_cpal.rs`, add a `layer_slot: Arc<ArcSwapOption<Vec<crate::layers::Layer>>>` parameter to `spawn` (positioned after the `click_slot` parameter) and pass it through to `RenderCore::new(...)` (after `click_slot`).

- [ ] **Step 5: Write the failing integration test in `render_core.rs`**

Add to `crates/engine/src/render_core.rs` a `#[cfg(test)]` module. This drives the whole slot→pipeline→layer→output path deterministically (no audio device): a silent song plus one loud layer must produce nonzero output.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::{SongBuffer, StemSet, CHANNELS, SAMPLE_RATE};
    use crate::layers::Layer;
    use crate::pipeline::EngineCmd;

    fn core_with(song: StemSet, layers: Option<Vec<Layer>>) -> (RenderCore, rtrb::Producer<EngineCmd>) {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::<EngineCmd>::new(16);
        let (evt_tx, _evt_rx) = rtrb::RingBuffer::<EngineEvent>::new(64);
        let song_slot = Arc::new(ArcSwapOption::new(Some(Arc::new(song))));
        let click_slot = Arc::new(ArcSwapOption::<Vec<ClickMark>>::empty());
        let layer_slot = Arc::new(ArcSwapOption::new(layers.map(Arc::new)));
        let core = RenderCore::new(cmd_rx, evt_tx, song_slot, click_slot, layer_slot);
        (core, cmd_tx)
    }

    fn peak(core: &mut RenderCore) -> f32 {
        // prime the stretcher across several blocks, then measure the loudest
        let mut max = 0.0f32;
        let mut out = vec![0.0f32; 1024 * CHANNELS];
        for _ in 0..32 {
            core.fill(&mut out);
            for s in &out {
                max = max.max(s.abs());
            }
        }
        max
    }

    #[test]
    fn a_loud_layer_becomes_audible_over_a_silent_song() {
        let silent = StemSet::single(SongBuffer {
            data: vec![0.0; SAMPLE_RATE as usize * CHANNELS], // 1s of silence
        });
        let layer = Layer {
            samples: Arc::new(SongBuffer {
                data: vec![0.5; SAMPLE_RATE as usize * CHANNELS],
            }),
            start_frame: 0,
            gain: 1.0,
            muted: false,
        };
        let (mut core, mut tx) = core_with(silent, Some(vec![layer]));
        tx.push(EngineCmd::Play).unwrap();
        assert!(peak(&mut core) > 0.1, "layer should be audible");
    }

    #[test]
    fn silent_song_with_no_layers_stays_silent() {
        let silent = StemSet::single(SongBuffer {
            data: vec![0.0; SAMPLE_RATE as usize * CHANNELS],
        });
        let (mut core, mut tx) = core_with(silent, None);
        tx.push(EngineCmd::Play).unwrap();
        assert!(peak(&mut core) < 1e-3, "should be silent");
    }
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p engine render_core::tests`
Expected: both PASS.

- [ ] **Step 7: Build + lint the whole engine (both backends)**

Run: `cargo test -p engine` then `cargo clippy -p engine -- -D warnings`
Expected: full engine suite passes; no clippy warnings. (Confirms both `output.rs` and `output_cpal.rs` `spawn` signatures compile.)

- [ ] **Step 8: Commit**

```bash
git add crates/engine/src/pipeline.rs crates/engine/src/render_core.rs crates/engine/src/engine.rs crates/engine/src/output.rs crates/engine/src/output_cpal.rs
git commit -m "feat(engine): play overdub layers via a layer_slot, mixed pre-stretch"
```

---

## Task 3: `Recording` model + manifest persistence (practice)

**Files:**
- Modify: `crates/practice/src/model.rs`
- Modify: `crates/practice/src/bundle.rs`
- Test: in-file `#[cfg(test)] mod tests` in `bundle.rs`

- [ ] **Step 1: Add the `RecordingId` and `Recording` types**

In `crates/practice/src/model.rs`, add the id type next to the existing id structs:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RecordingId(pub i64);
```

Add the wire type (near `SectionNote`):

```rust
/// An overdub take: your own input recorded over one pass of a span, held as an
/// additive layer. Audio lives at `<bundle>/recordings/<file>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recording {
    pub id: RecordingId,
    pub name: String,
    /// Path relative to the bundle dir, e.g. "recordings/1.wav".
    pub file: String,
    /// Source frame where capture began (the span start).
    pub anchor_frame: i64,
    /// Recorded length in frames.
    pub len_frames: i64,
    /// Per-layer manual alignment offset in frames (added to global latency).
    #[serde(default)]
    pub nudge_frames: i64,
    /// Playback gain, 0.0..=1.5.
    pub gain: f32,
    /// Muted in the layer mix.
    #[serde(default)]
    pub muted: bool,
    /// ISO-8601 creation time (set by the server when written).
    pub created_at: String,
}
```

- [ ] **Step 2: Add `recordings` to the manifest**

In `crates/practice/src/bundle.rs`, add the field to `BundleManifest` (after `analysis`):

```rust
    #[serde(default)]
    pub recordings: Vec<Recording>,
```

Add `Recording` to the `use crate::model::{...}` import line at the top of `bundle.rs`.

- [ ] **Step 3: Write the failing round-trip test**

The existing `manifest_json_roundtrips` test builds `sample_manifest()`. Add a focused test below it that an existing (recordings-free) manifest still deserialises, and that recordings round-trip:

```rust
    #[test]
    fn manifest_without_recordings_field_still_loads() {
        // Older bundles have no `recordings` key; #[serde(default)] must apply.
        let json = r#"{"version":1,"song":{"id":1,"title":"T","artist":null,
            "path":"/tmp/a.flac","file_hash":"h","duration_secs":1.0}}"#;
        let m: BundleManifest = serde_json::from_str(json).unwrap();
        assert!(m.recordings.is_empty());
    }

    #[test]
    fn recordings_roundtrip_in_manifest() {
        let mut m = sample_manifest();
        m.recordings.push(crate::model::Recording {
            id: crate::model::RecordingId(1),
            name: "take 1".into(),
            file: "recordings/1.wav".into(),
            anchor_frame: 48_000,
            len_frames: 240_000,
            nudge_frames: -120,
            gain: 1.0,
            muted: false,
            created_at: "2026-06-25T12:00:00Z".into(),
        });
        let bytes = serde_json::to_vec(&m).unwrap();
        let back: BundleManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(m, back);
    }
```

If `sample_manifest()` constructs `BundleManifest { ... }` with explicit fields (no `..Default`), add `recordings: vec![]` to that literal so it compiles.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p practice bundle::tests`
Expected: PASS.

- [ ] **Step 5: Lint + commit**

Run: `cargo clippy -p practice -- -D warnings`

```bash
git add crates/practice/src/model.rs crates/practice/src/bundle.rs
git commit -m "feat(practice): Recording model + recordings field on bundle manifest"
```

---

## Task 4: Recording orchestration primitives (server, pure + trait)

**Files:**
- Create: `crates/server/src/recording.rs`
- Modify: `crates/server/src/lib.rs` (register `mod recording;`)
- Test: in-file `#[cfg(test)] mod tests` in `recording.rs`

- [ ] **Step 1: Register the module**

In `crates/server/src/lib.rs` (or wherever sibling modules like `tuner` are declared), add:

```rust
pub mod recording;
```

- [ ] **Step 2: Write `recording.rs` with pure helpers + trait + fake, and tests**

Create `crates/server/src/recording.rs`:

```rust
//! Overdub recording orchestration. Pure helpers (span resolution, calibration
//! click detection) are unit-tested here; device capture lives behind the
//! `RecordingControl` trait so the dispatcher can be tested with a fake.

use engine::buffer::{CHANNELS, SAMPLE_RATE};

/// Which region a recording pass covers, chosen by the user at record time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Span {
    Song,
    Selection { start: f64, end: f64 },
    Loop { start: f64, end: f64 },
}

/// Resolve a span to a `[start_frame, end_frame)` source-frame window, clamped
/// to the song. Returns `None` if the window is empty.
pub fn resolve_span(span: Span, song_frames: i64) -> Option<(i64, i64)> {
    let to_frame = |s: f64| (s.max(0.0) * SAMPLE_RATE as f64).round() as i64;
    let (start, end) = match span {
        Span::Song => (0, song_frames),
        Span::Selection { start, end } | Span::Loop { start, end } => {
            (to_frame(start), to_frame(end))
        }
    };
    let start = start.clamp(0, song_frames);
    let end = end.clamp(0, song_frames);
    if end > start {
        Some((start, end))
    } else {
        None
    }
}

/// Find the first frame whose absolute sample exceeds `threshold` in an
/// interleaved stereo recording. Used by latency calibration: emit a click at
/// recording frame 0, and the detected onset is the round-trip latency.
pub fn detect_click_onset(interleaved: &[f32], threshold: f32) -> Option<usize> {
    interleaved
        .chunks_exact(CHANNELS)
        .position(|f| f.iter().any(|s| s.abs() > threshold))
}

/// Capture backend. The real implementation taps a PipeWire/cpal input; the
/// fake returns canned audio so the dispatcher is testable.
pub trait RecordingControl: Send {
    /// Begin capturing from `device_id`, sizing the buffer for `len_frames`.
    fn start(&mut self, device_id: &str, len_frames: i64) -> Result<(), String>;
    /// Stop and return the captured interleaved-stereo f32 (up to `len_frames`).
    fn stop(&mut self) -> Result<Vec<f32>, String>;
    /// Run an acoustic loopback: emit a click out the default output while
    /// capturing `device_id`, return the captured recording for onset analysis.
    fn calibrate_capture(&mut self, device_id: &str) -> Result<Vec<f32>, String>;
}

#[cfg(test)]
pub struct FakeRecorder {
    pub canned: Vec<f32>,
    pub started: Option<(String, i64)>,
}

#[cfg(test)]
impl RecordingControl for FakeRecorder {
    fn start(&mut self, device_id: &str, len_frames: i64) -> Result<(), String> {
        self.started = Some((device_id.to_string(), len_frames));
        Ok(())
    }
    fn stop(&mut self) -> Result<Vec<f32>, String> {
        Ok(self.canned.clone())
    }
    fn calibrate_capture(&mut self, _device_id: &str) -> Result<Vec<f32>, String> {
        Ok(self.canned.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_song_span_is_whole_song() {
        assert_eq!(resolve_span(Span::Song, 1000), Some((0, 1000)));
    }

    #[test]
    fn resolve_selection_converts_seconds_and_clamps() {
        let s = Span::Selection { start: 1.0, end: 2.0 };
        assert_eq!(
            resolve_span(s, 10 * SAMPLE_RATE as i64),
            Some((SAMPLE_RATE as i64, 2 * SAMPLE_RATE as i64))
        );
    }

    #[test]
    fn resolve_empty_span_is_none() {
        let s = Span::Selection { start: 2.0, end: 2.0 };
        assert_eq!(resolve_span(s, 10 * SAMPLE_RATE as i64), None);
    }

    #[test]
    fn detect_onset_finds_the_click() {
        // 50 silent frames, then a spike
        let mut buf = vec![0.0f32; 50 * CHANNELS];
        buf.extend_from_slice(&[0.9, 0.9]);
        assert_eq!(detect_click_onset(&buf, 0.5), Some(50));
    }

    #[test]
    fn detect_onset_none_when_below_threshold() {
        let buf = vec![0.1f32; 100 * CHANNELS];
        assert_eq!(detect_click_onset(&buf, 0.5), None);
    }
}
```

- [ ] **Step 3: Run the tests**

Run: `cargo test -p server recording::tests`
Expected: 5 PASS.

- [ ] **Step 4: Add the real recorder (device capture; verified manually later)**

Append the real implementation. It reuses `engine::capture::start_capture_by_id` with a buffer sized to the take, mirroring `tuner.rs`. Audio-device timing is not unit-testable; it is covered by the manual checklist in Task 10.

```rust
use engine::ring::RollingRing;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub struct RealRecorder {
    capture: Option<engine::capture::CaptureSession>,
    len_frames: i64,
}

impl RecordingControl for RealRecorder {
    fn start(&mut self, device_id: &str, len_frames: i64) -> Result<(), String> {
        let secs = (len_frames as f64 / SAMPLE_RATE as f64) + 1.0; // +1s margin
        let cap = engine::capture::start_capture_by_id(device_id, secs)
            .map_err(|e| e.to_string())?;
        self.capture = Some(cap);
        self.len_frames = len_frames;
        Ok(())
    }

    fn stop(&mut self) -> Result<Vec<f32>, String> {
        let cap = self.capture.take().ok_or("not recording")?;
        let secs = self.len_frames as f64 / SAMPLE_RATE as f64;
        let snap = cap
            .ring
            .lock()
            .map_err(|_| "capture ring poisoned")?
            .snapshot_last(secs);
        cap.stop();
        Ok(snap)
    }

    fn calibrate_capture(&mut self, device_id: &str) -> Result<Vec<f32>, String> {
        // Emit a click out the default output while capturing ~1s of input.
        // (Implementation: reuse engine click synthesis + a short capture.)
        let cap = engine::capture::start_capture_by_id(device_id, 2.0)
            .map_err(|e| e.to_string())?;
        // ... emit click via the engine, sleep ~1s, snapshot ...
        let snap = cap
            .ring
            .lock()
            .map_err(|_| "capture ring poisoned")?
            .snapshot_last(1.5);
        cap.stop();
        Ok(snap)
    }
}

// Silence unused-import warnings on platforms where the real path differs.
#[allow(unused_imports)]
use {AtomicBool as _AtomicBool};
```

Note: confirm `RollingRing::snapshot_last(secs)` exists (used by `tuner.rs` as `r.snapshot_last(SNAPSHOT_SECS)`); reuse it verbatim. Remove the trailing `#[allow]`/unused `use` lines if the compiler does not flag them.

- [ ] **Step 5: Build + lint + commit**

Run: `cargo test -p server recording::tests` and `cargo clippy -p server -- -D warnings`
Expected: pass.

```bash
git add crates/server/src/recording.rs crates/server/src/lib.rs
git commit -m "feat(server): recording orchestration primitives (span, onset, capture trait)"
```

---

## Task 5: Recording command surface + layer rebuild (server `app.rs`)

**Files:**
- Modify: `crates/server/src/app.rs`
- Test: in-file `#[cfg(test)] mod tests` in `app.rs`

This task wires dispatch arms, persists takes to the bundle, rebuilds the engine layer set from the manifest, and emits events. The `recording.start`/`stop` device timing uses `RealRecorder`; tests use a fake injected the same way `MockEngine`/`FakeSeparator` are.

- [ ] **Step 1: Hold a recorder + input-latency, and a layer-rebuild helper**

Add a field to `App` for the recorder (boxed trait, like the separator) and a constant key for the calibration setting. In `App::new`, default it to `Box::new(recording::RealRecorder::default())`; add a `with_recorder` test seam if the existing constructor doesn't already accept injected collaborators (follow how `Box<dyn ...>` collaborators are passed today).

Add a private helper that reads the open song's recordings, computes each layer's compensated `start_frame`, loads the WAVs, and pushes them to the engine:

```rust
const INPUT_LATENCY_KEY: &str = "input_latency_frames";

impl App {
    fn input_latency_frames(&self) -> i64 {
        self.store
            .get_setting(INPUT_LATENCY_KEY)
            .ok()
            .flatten()
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
    }

    /// Rebuild the engine's layer set from the open song's recordings and push
    /// it across the layer slot. Called after any change to recordings.
    fn refresh_layers(&mut self) -> Result<(), String> {
        let Some(open) = self.open_song.as_ref() else {
            self.audio.set_layers(Vec::new());
            return Ok(());
        };
        let song_id = open.song.id;
        let Some(dir) = self.library.bundle_dir(song_id) else {
            return Ok(());
        };
        let latency = self.input_latency_frames();
        let recs = self.library.recordings(song_id); // see Step 2
        let mut layers = Vec::new();
        for r in recs {
            let path = dir.join(&r.file);
            let buf = engine::decode::decode_file(&path).map_err(|e| e.to_string())?;
            layers.push(engine::layers::Layer {
                samples: std::sync::Arc::new(buf.into_stem_buffer()), // adapt to decode's return type
                start_frame: r.anchor_frame - latency - r.nudge_frames,
                gain: r.gain,
                muted: r.muted,
            });
        }
        self.audio.set_layers(layers);
        Ok(())
    }
}
```

Adapt `decode_file(...)` and `into_stem_buffer()` to the actual `engine::decode` API used by `open_decode` in the phased open path (reuse exactly what produces a `SongBuffer` there — do not invent a new decode function). The decoded recording must be a single `SongBuffer` (stereo, 48k); recordings are written at 48k by `write_wav`, so no resampling is needed.

- [ ] **Step 2: Add library access to a song's recordings**

The bundle manifest is the source of truth. Add a `recordings(song_id) -> Vec<Recording>` reader and a `set_recordings(song_id, Vec<Recording>)` writer to `library.rs` that mutate the in-memory manifest and rewrite `dredge.json` atomically (follow the existing pattern used by `set_sections`/loops edits). Allocate new ids as `max(existing ids) + 1`.

- [ ] **Step 3: Write the failing dispatcher test (with a fake recorder)**

Add to `app.rs` tests. The fake returns a canned 1-second tone; `recording.start` then `recording.stop` must write a WAV under the bundle, append a `Recording` to the manifest, and return it.

```rust
    #[test]
    fn record_start_stop_persists_a_take_and_pushes_a_layer() {
        let (mock, mut app) = make_shared_mock_with_recorder(FakeRecorder {
            canned: vec![0.3f32; SAMPLE_RATE as usize * CHANNELS], // 1s
            started: None,
        });
        let song_id = import_and_open_a_test_song(&mut app); // existing test helper pattern

        let started = app.dispatch(Request {
            id: 1,
            cmd: "recording.start".into(),
            params: json!({ "span": "song", "device_id": "0" }),
        });
        assert!(started.ok, "start failed: {:?}", started.error);

        let finished = app.dispatch(Request {
            id: 2,
            cmd: "recording.stop".into(),
            params: json!(null),
        });
        assert!(finished.ok, "stop failed: {:?}", finished.error);

        // a recording is now in the manifest
        let recs = app.library.recordings(song_id);
        assert_eq!(recs.len(), 1);
        // its WAV exists on disk in the bundle
        let dir = app.song_bundle_dir(song_id).unwrap();
        assert!(dir.join(&recs[0].file).exists());
        // the engine received a layer
        assert_eq!(mock.lock().unwrap().layers_len, 1);
    }
```

This requires: (a) a `make_shared_mock_with_recorder` test helper (clone of `make_shared_mock` that injects the recorder), (b) the `MockEngine` to record `set_layers` calls into a `layers_len` field, (c) a small `import_and_open_a_test_song` helper if one does not already exist (reuse whatever existing app tests use to get an open song with a bundle dir).

- [ ] **Step 4: Implement the dispatch arms**

Add to the `match cmd` block in `dispatch_inner`:

```rust
        "recording.start" => self.recording_start(p),
        "recording.stop" => self.recording_stop(p),
        "recording.list" => serde_json::to_value(
            self.open_song
                .as_ref()
                .map(|o| self.library.recordings(o.song.id))
                .unwrap_or_default(),
        )
        .err_str(),
        "recording.rename" => self.recording_rename(p),
        "recording.delete" => self.recording_delete(p),
        "recording.setGain" => self.recording_set_gain(p),
        "recording.setMute" => self.recording_set_mute(p),
        "recording.setNudge" => self.recording_set_nudge(p),
        "recording.calibrate" => self.recording_calibrate(p),
```

Implement the handlers. `recording_start` resolves the span (Song uses `open.song.duration_secs`; Selection/Loop come from params), starts the recorder for the resolved length, sends `EngineCmd::Play` (count-in fires from the existing setting), and stores the pending `(song_id, anchor_frame, len_frames)`:

```rust
    fn recording_start(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            span: String,
            #[serde(default)]
            start: Option<f64>,
            #[serde(default)]
            end: Option<f64>,
            device_id: String,
        }
        let p: P = from_params(p)?;
        let open = self.open_song.as_ref().ok_or("no song open")?;
        let song_frames = (open.song.duration_secs * SAMPLE_RATE as f64).round() as i64;
        let span = match p.span.as_str() {
            "song" => recording::Span::Song,
            "selection" => recording::Span::Selection {
                start: p.start.ok_or("selection needs start/end")?,
                end: p.end.ok_or("selection needs start/end")?,
            },
            "loop" => recording::Span::Loop {
                start: p.start.ok_or("loop needs start/end")?,
                end: p.end.ok_or("loop needs start/end")?,
            },
            other => return Err(format!("unknown span: {other}")),
        };
        let (start, end) = recording::resolve_span(span, song_frames).ok_or("empty span")?;
        self.recorder.start(&p.device_id, end - start)?;
        self.pending_recording = Some(PendingRecording {
            song_id: open.song.id,
            anchor_frame: start,
            len_frames: end - start,
        });
        self.audio.send(EngineCmd::SeekSecs(start as f64 / SAMPLE_RATE as f64));
        self.audio.send(EngineCmd::Play);
        Ok(Value::Null)
    }
```

`recording_stop` drains the captured audio, writes the WAV, appends to the manifest, refreshes layers, and emits an event:

```rust
    fn recording_stop(&mut self, _p: Value) -> Result<Value, String> {
        let pending = self.pending_recording.take().ok_or("not recording")?;
        let samples = self.recorder.stop()?;
        self.audio.send(EngineCmd::Pause);
        let dir = self
            .library
            .bundle_dir(pending.song_id)
            .ok_or("no bundle dir")?;
        let mut recs = self.library.recordings(pending.song_id);
        let id = recs.iter().map(|r| r.id.0).max().unwrap_or(0) + 1;
        let file = format!("recordings/{id}.wav");
        engine::capture::write_wav(&dir.join(&file), &samples).map_err(|e| e.to_string())?;
        let rec = practice::model::Recording {
            id: practice::model::RecordingId(id),
            name: format!("take {id}"),
            file,
            anchor_frame: pending.anchor_frame,
            len_frames: pending.len_frames,
            nudge_frames: 0,
            gain: 1.0,
            muted: false,
            created_at: now_iso8601(), // reuse existing time helper, or chrono if present
        };
        recs.push(rec.clone());
        self.library.set_recordings(pending.song_id, recs)?;
        self.refresh_layers()?;
        self.push_event("recording.finished", serde_json::to_value(&rec).err_str()?);
        serde_json::to_value(&rec).err_str()
    }
```

`recording_rename`/`set_gain`/`set_mute`/`set_nudge` each load recordings, mutate the matching id, `set_recordings`, then `refresh_layers()`. `recording_delete` also removes the WAV file from disk. `recording_calibrate` calls `self.recorder.calibrate_capture(device_id)`, runs `recording::detect_click_onset`, stores the result under `INPUT_LATENCY_KEY`, calls `refresh_layers()`, and returns `{ "latency_frames": n }`.

Add `pending_recording: Option<PendingRecording>` and the `PendingRecording { song_id, anchor_frame, len_frames }` struct to `app.rs`. Use the existing event-push mechanism (`push_event`/`push_*` helpers already used by `push_count_in`) — match the actual helper name in the file.

- [ ] **Step 5: Include `recordings` in the `song.open` response**

In `finish_open`, add to the `json!` payload and call `refresh_layers()` after loading:

```rust
        "recordings": self.library.recordings(song_id),
```

Add `self.refresh_layers().ok();` after `self.audio.load(decoded.set);` so layers attach when a song with existing recordings opens.

- [ ] **Step 6: Run the test + full server suite**

Run: `cargo test -p server` then `cargo clippy -p server -- -D warnings`
Expected: the new test passes and nothing regresses.

- [ ] **Step 7: Commit**

```bash
git add crates/server/src/app.rs crates/practice/src/library.rs
git commit -m "feat(server): recording.* commands, bundle persistence, layer rebuild"
```

---

## Task 6: Frontend store + pure nudge math

**Files:**
- Create: `apps/desktop/src/lib/recording-math.ts`
- Create: `apps/desktop/src/lib/recording-math.test.ts`
- Modify: `apps/desktop/src/lib/stores.ts`

- [ ] **Step 1: Write the failing ms↔frames test**

Create `apps/desktop/src/lib/recording-math.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import { framesToMs, msToFrames } from "./recording-math";

describe("nudge ms<->frames", () => {
  it("converts ms to frames at 48kHz", () => {
    expect(msToFrames(10)).toBe(480);
    expect(msToFrames(-10)).toBe(-480);
  });
  it("converts frames to ms", () => {
    expect(framesToMs(480)).toBeCloseTo(10, 9);
  });
  it("round-trips whole-ms values", () => {
    for (const ms of [-50, -5, 0, 5, 50]) {
      expect(framesToMs(msToFrames(ms))).toBeCloseTo(ms, 6);
    }
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd apps/desktop && pnpm vitest run lib/recording-math.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the helpers**

Create `apps/desktop/src/lib/recording-math.ts`:

```typescript
// Recording nudge conversions. The engine works in source frames at 48 kHz;
// the UI shows milliseconds.
export const SAMPLE_RATE = 48_000;

export const msToFrames = (ms: number): number => Math.round((ms / 1000) * SAMPLE_RATE);

export const framesToMs = (frames: number): number => (frames / SAMPLE_RATE) * 1000;
```

- [ ] **Step 4: Run it to verify it passes**

Run: `cd apps/desktop && pnpm vitest run lib/recording-math.test.ts`
Expected: PASS.

- [ ] **Step 5: Add the `Recording` interface + store + actions**

In `apps/desktop/src/lib/stores.ts`:

Add the wire interface (near `StemMix`):

```typescript
export interface Recording {
  id: number;
  name: string;
  file: string;
  anchor_frame: number;
  len_frames: number;
  nudge_frames: number;
  gain: number;
  muted: boolean;
  created_at: string;
}
```

Add stores (near the other `writable` declarations):

```typescript
export const recordings = writable<Recording[]>([]);
export const recordingActive = writable<boolean>(false);
export const inputLatencyFrames = writable<number>(0);
```

Extend `OpenSong` to include `recordings: Recording[]` (matching the new `song.open` payload key) and set the store in `openSong()`:

```typescript
    recordings.set(data.recordings ?? []);
```

Add actions (on the `actions` object):

```typescript
  async startRecording(span: "song" | "selection" | "loop", deviceId: string, range?: { start: number; end: number }): Promise<void> {
    await cmd("recording.start", { span, device_id: deviceId, ...(range ?? {}) });
    recordingActive.set(true);
  },
  async stopRecording(): Promise<void> {
    const rec = await cmd<Recording>("recording.stop");
    recordingActive.set(false);
    recordings.update((rs) => [...rs, rec]);
  },
  async deleteRecording(id: number): Promise<void> {
    await cmd("recording.delete", { id });
    recordings.update((rs) => rs.filter((r) => r.id !== id));
  },
  async renameRecording(id: number, name: string): Promise<void> {
    await cmd("recording.rename", { id, name });
    recordings.update((rs) => rs.map((r) => (r.id === id ? { ...r, name } : r)));
  },
  async setRecordingGain(id: number, gain: number): Promise<void> {
    recordings.update((rs) => rs.map((r) => (r.id === id ? { ...r, gain } : r)));
    await cmd("recording.setGain", { id, gain });
  },
  async toggleRecordingMute(id: number): Promise<void> {
    let muted = false;
    recordings.update((rs) => rs.map((r) => (r.id === id ? ((muted = !r.muted), { ...r, muted }) : r)));
    await cmd("recording.setMute", { id, muted });
  },
  async setRecordingNudge(id: number, nudgeFrames: number): Promise<void> {
    recordings.update((rs) => rs.map((r) => (r.id === id ? { ...r, nudge_frames: nudgeFrames } : r)));
    await cmd("recording.setNudge", { id, nudge_ms: framesToMs(nudgeFrames) });
  },
  async calibrateLatency(deviceId: string): Promise<void> {
    const { latency_frames } = await cmd<{ latency_frames: number }>("recording.calibrate", { device_id: deviceId });
    inputLatencyFrames.set(latency_frames);
  },
```

Import `framesToMs` at the top of `stores.ts` from `./recording-math`.

- [ ] **Step 6: Wire the `recording.finished` event**

In `initEvents`, add a case (the server also pushes this after a stop; keep the store idempotent by id):

```typescript
      case "recording.finished": {
        const r = ev.data as Recording;
        recordings.update((rs) => (rs.some((x) => x.id === r.id) ? rs : [...rs, r]));
        recordingActive.set(false);
        break;
      }
```

- [ ] **Step 7: Run frontend tests + svelte-check**

Run: `cd apps/desktop && pnpm vitest run lib/recording-math.test.ts && pnpm svelte-check`
Expected: tests PASS; svelte-check clean.

- [ ] **Step 8: Commit**

```bash
git add apps/desktop/src/lib/recording-math.ts apps/desktop/src/lib/recording-math.test.ts apps/desktop/src/lib/stores.ts
git commit -m "feat(ui): recordings store, actions, nudge math, event wiring"
```

---

## Task 7: `Recordings` control box

**Files:**
- Create: `apps/desktop/src/components/Recordings.svelte`
- Modify: `apps/desktop/src/App.svelte`

- [ ] **Step 1: Create the control box component**

Create `apps/desktop/src/components/Recordings.svelte`, built on `Box`/`Button`/`Fader`, following `Isolation.svelte`'s structure. It needs an input-device picker (fetch via `cmd("device.inputs")`), a span selector, a Record/Stop button, and one row per recording:

```svelte
<script lang="ts">
  // Recordings box: capture your own input over the track as additive layers.
  // One row per take — name, level, mute, nudge, delete. Recording always
  // covers one pass over the chosen span, after the count-in.
  import { actions, openSong, recordingActive, recordings, selection, currentLoop } from "../lib/stores";
  import { framesToMs, msToFrames } from "../lib/recording-math";
  import { cmd } from "../lib/ipc";
  import Box from "../lib/ui/Box.svelte";
  import Button from "../lib/ui/Button.svelte";
  import Fader from "../lib/ui/Fader.svelte";

  type Span = "song" | "selection" | "loop";
  let span = $state<Span>("song");
  let devices = $state<{ id: string; name: string }[]>([]);
  let deviceId = $state<string>("");

  $effect(() => {
    void cmd<{ id: string; name: string }[]>("device.inputs").then((d) => {
      devices = d;
      if (!deviceId && d.length) deviceId = d[0].id;
    });
  });

  async function record() {
    if ($recordingActive) {
      await actions.stopRecording();
      return;
    }
    const sel = $selection;
    const lp = $currentLoop;
    const range =
      span === "selection" && sel ? { start: sel.start, end: sel.end }
      : span === "loop" && lp ? { start: lp.start, end: lp.end }
      : undefined;
    await actions.startRecording(span, deviceId, range);
  }
</script>

{#if $openSong}
  <Box label="recordings">
    <div class="bar">
      <select bind:value={span} disabled={$recordingActive}>
        <option value="song">full song</option>
        <option value="selection" disabled={!$selection}>selection</option>
        <option value="loop" disabled={!$currentLoop}>loop</option>
      </select>
      <select bind:value={deviceId} disabled={$recordingActive}>
        {#each devices as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
      </select>
      <Button variant="toggle" active={$recordingActive} onclick={() => void record()}>
        {$recordingActive ? "stop" : "record"}
      </Button>
    </div>

    {#each $recordings as r (r.id)}
      <div class="row">
        <span class="name mono">{r.name}</span>
        <div class="fader">
          <Fader orientation="horizontal" value={r.gain} min={0} max={1.5} step={0.01}
            onchange={(v) => void actions.setRecordingGain(r.id, v)}
            format={(v) => `${r.name} ${Math.round(v * 100)}%`} />
        </div>
        <Button variant="chip" active={r.muted} onclick={() => void actions.toggleRecordingMute(r.id)} title="mute">M</Button>
        <input class="nudge mono" type="number" step="1" value={Math.round(framesToMs(r.nudge_frames))}
          onchange={(e) => void actions.setRecordingNudge(r.id, msToFrames(+e.currentTarget.value))}
          title="nudge (ms)" />
        <button class="del" onclick={() => void actions.deleteRecording(r.id)} title="delete">✕</button>
      </div>
    {/each}
  </Box>
{/if}

<style>
  .bar { display: flex; gap: 8px; align-items: center; }
  .row { display: flex; gap: 8px; align-items: center; margin-top: 6px; }
  .name { font-size: 11px; color: var(--muted); min-width: 6ch; }
  .fader { flex: 1; }
  .nudge { width: 6ch; background: var(--bg); color: var(--fg); border: 1px solid var(--line); }
  .del { color: var(--muted); background: none; border: none; cursor: pointer; }
</style>
```

Match `Fader`/`Button` prop names to their actual definitions (cross-check against `Isolation.svelte`'s usage, shown to use `orientation`, `value`, `min`, `max`, `step`, `onchange`, `format`, and `variant`/`active`/`onclick`). Use the theme accent for active states via the `Button` `active` prop — do not hardcode colors.

- [ ] **Step 2: Mount it in the stage**

In `apps/desktop/src/App.svelte`, import it next to the other component imports:

```svelte
  import Recordings from "./components/Recordings.svelte";
```

Add it inside the `{#if $openSong}` block in `.boxes`, after `<Notes />`:

```svelte
        <Notes />
        <Recordings />
```

- [ ] **Step 3: Verify build + svelte-check**

Run: `cd apps/desktop && pnpm svelte-check`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/Recordings.svelte apps/desktop/src/App.svelte
git commit -m "feat(ui): recordings control box on the stage"
```

---

## Task 8: Stacked layer waveform lanes

**Files:**
- Modify: `apps/desktop/src/components/Waveform.svelte`

Layers draw as thin lanes beneath the main waveform, sharing the same `view` (zoom/scroll). Peaks for each recording are not yet sent from the backend; the simplest correct first version derives a lane's extent from `anchor_frame`/`len_frames` and draws the recorded WAV peaks once available. For this task, render each layer as a labeled block spanning its time extent (a visible, time-aligned lane) and leave per-sample peak rendering as a follow-up — this keeps the task self-contained and testable by eye.

- [ ] **Step 1: Add a pure helper for a layer's time extent + test**

Add to `apps/desktop/src/lib/recording-math.ts`:

```typescript
// A layer occupies [anchor, anchor+len) in source frames, shifted by the
// effective offset (latency + nudge handled server-side for audio; for the
// visual lane we show the recorded extent at its anchor).
export const layerSpanSecs = (anchorFrame: number, lenFrames: number) => ({
  start: anchorFrame / SAMPLE_RATE,
  end: (anchorFrame + lenFrames) / SAMPLE_RATE,
});
```

Add to `recording-math.test.ts`:

```typescript
import { layerSpanSecs } from "./recording-math";

describe("layerSpanSecs", () => {
  it("converts anchor/len frames to a seconds span", () => {
    expect(layerSpanSecs(48000, 96000)).toEqual({ start: 1, end: 3 });
  });
});
```

Run: `cd apps/desktop && pnpm vitest run lib/recording-math.test.ts` — expect PASS.

- [ ] **Step 2: Draw the lanes in `Waveform.svelte`**

In the canvas draw routine, after the main waveform and section lane are drawn, iterate `recordings` (import the store) and draw one lane per layer below the waveform, using `secToX(view, span.start)`/`secToX(view, span.end)` for x-extent. Reuse the section-lane drawing approach (a tinted rect + a label), offset vertically below `LANE_H + WAVE_H`. Use `var(--accent)`-derived tint via the existing `c.accent`/`labelColor` helpers so it re-tints with the theme. Stack multiple layers by index (each lane a fixed height, e.g. 18px).

This is canvas rendering driven by reactive store state; it is verified visually (Task 10), not by a unit test.

- [ ] **Step 3: svelte-check + commit**

Run: `cd apps/desktop && pnpm svelte-check`

```bash
git add apps/desktop/src/components/Waveform.svelte apps/desktop/src/lib/recording-math.ts apps/desktop/src/lib/recording-math.test.ts
git commit -m "feat(ui): stacked layer lanes beneath the waveform"
```

---

## Task 9: Latency calibration in the settings tab

**Files:**
- Modify: `apps/desktop/src/components/SettingsPanel.svelte`

- [ ] **Step 1: Add a calibration control**

In `SettingsPanel.svelte`, add a row that lets the user pick an input device and run calibration, showing the resulting latency. Reuse `cmd("device.inputs")` and `actions.calibrateLatency(deviceId)`; bind the displayed value to the `inputLatencyFrames` store (show `framesToMs(...)` ms). Load the persisted value on mount via the settings the panel already fetches (the backend stores `input_latency_frames`; surface it in whatever settings payload the panel reads, or fetch it once).

```svelte
  import { actions, inputLatencyFrames } from "../lib/stores";
  import { framesToMs } from "../lib/recording-math";
  // ...
  <div class="setting">
    <span>recording latency</span>
    <span class="mono">{framesToMs($inputLatencyFrames).toFixed(1)} ms</span>
    <button onclick={() => void actions.calibrateLatency(inputDeviceId)}>calibrate…</button>
  </div>
```

Provide an input-device selection in the panel (reuse the pattern from the devices tab / `device.inputs`).

- [ ] **Step 2: svelte-check + commit**

Run: `cd apps/desktop && pnpm svelte-check`

```bash
git add apps/desktop/src/components/SettingsPanel.svelte
git commit -m "feat(ui): latency calibration in settings"
```

---

## Task 10: Full-suite gate + manual verification

**Files:** none (verification only)

- [ ] **Step 1: Full automated suite**

Run: `just check`
Expected: `cargo test --workspace` + `pnpm vitest run` pass; clippy `-D warnings`, `cargo fmt --check`, `svelte-check` clean. Fix anything that fails before proceeding.

- [ ] **Step 2: Manual checklist (device + UI paths not unit-testable)**

Build and run the real app (`just build && just run`). Per the project's webview constraint, verify by hand:

- Open a song with stems; mute the bass stem in the isolation box.
- In the recordings box, pick an input device, span = full song, hit **record**. Confirm the count-in plays and is **not** captured, playback runs, and a take appears on stop.
- Play back: the recorded layer is audible over the track; muting it silences only your part.
- Slow the rate to 0.5×: the layer slows **and stays aligned** with the track (no drift), pitch preserved.
- Record a second take over the first: both play together.
- Adjust a layer's level fader and mute — audible and immediate.
- Nudge a layer by ±20 ms — alignment shifts audibly.
- Run settings → calibrate; confirm a plausible latency (a few ms to tens of ms) is stored and persists across restart.
- Delete a take: its row, its lane, and its `recordings/<id>.wav` are gone; reopening the song does not resurrect it.
- Copy the bundle folder to a new path and open it there: takes load and play (relative paths rebase).

- [ ] **Step 3: Update the changelog and docs if the repo tracks them**

Follow the repo's existing release/changelog convention (see recent commits like `docs: changelog for v0.7.0`). Add a user-facing line describing overdub recording layers. Do not push (local-only main).

```bash
git add -A
git commit -m "docs: changelog for overdub recording layers"
```

---

## Self-Review Notes (for the implementer)

- **Spec coverage:** recording model (T4 span/onset, T5 commands), pre-stretch layered playback (T1–T2), calibration + nudge (T5 calibrate, T6 math, T9 UI), persistence in bundle (T3, T5), recordings box + lanes (T7–T8), out-of-scope items (monitoring, trimming, mixdown, per-track tempo) are intentionally absent.
- **Deviation from spec:** the spec named `AddLayer`/`SetLayer*` `EngineCmd` variants; this plan uses a `layer_slot` `ArcSwapOption` instead because `EngineCmd: Copy` cannot carry an `Arc<SongBuffer>`. The slot mirrors `song_slot`/`click_slot` and is the more consistent design. The spec has been annotated to match.
- **Adapt-to-reality points** (the implementer must cross-check against the live code, not invent): the exact `engine::decode` function + its return type (reuse what `open_decode` uses), the `App` constructor's collaborator-injection style (for the recorder seam), the event-push helper name in `app.rs`, `RollingRing::snapshot_last`, the ISO-8601 time helper, and `library.rs`'s atomic-manifest-edit pattern (mirror `set_sections`).
