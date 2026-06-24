# Click Track Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "section click" — a metronome click on every analyzed beat during user-marked sections, layered over playback and locked to the audible beat grid — and house it with the existing count-in in a new Click track control box.

**Architecture:** The per-section selection is a `click_guide` flag on each `Section`, persisted in the song bundle. The server intersects `Analysis.beats` with the marked sections' spans to build a click schedule (`Vec<ClickMark>`), pushed to the RT audio thread over a lock-free `ArcSwapOption` slot (the same mechanism the song uses). The pipeline tracks an audible source-position cursor and mixes a decaying click ping over the rendered audio at each scheduled beat.

**Tech Stack:** Rust (engine crate: RT audio + `arc_swap`; practice crate: model + bundle persistence; server crate: dispatcher), Svelte 5 + TypeScript frontend, vitest + `cargo test`.

**Execution note:** This repo works directly on `main` (no feature branch/worktree). Commit after each task.

---

## File structure

**Modified:**
- `crates/practice/src/model.rs` — add `Section.click_guide`.
- `crates/practice/src/library.rs` — add `set_section_click_guide`.
- `crates/engine/src/pipeline.rs` — `ClickMark`, `ClickVoice`, schedule slot consumer, overlay + audible cursor.
- `crates/engine/src/render_core.rs` — load the click slot, hand schedules to the pipeline.
- `crates/engine/src/engine.rs` — own the click slot, `set_click_schedule`.
- `crates/engine/src/output.rs`, `output_cpal.rs` — thread the click slot into `RenderCore::new`.
- `crates/server/src/control.rs` — `AudioControl::set_click_schedule` (+ MockEngine record).
- `crates/server/src/app.rs` — `section.click.set`, `sectionclick.set`, `push_section_click`, recompute wiring.
- `apps/desktop/src/lib/stores.ts` — `sectionClick` store, `Section.clickGuide`, actions, gating, hydration.
- `apps/desktop/src/components/Transport.svelte` — remove count-in controls (keep the pulse).
- `apps/desktop/src/components/Sections.svelte` — per-section click toggle.
- `apps/desktop/src/App.svelte` — mount the Click track box on the stage.

**Created:**
- `crates/server/src/section_click.rs` — the pure schedule builder.
- `apps/desktop/src/components/ClickTrack.svelte` — the new control box (count-in group + section-click group).

---

## Task 1: `click_guide` flag on Section

**Files:**
- Modify: `crates/practice/src/model.rs:20-29`
- Test: `crates/practice/src/model.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Add at the bottom of `crates/practice/src/model.rs`:

```rust
#[cfg(test)]
mod click_guide_tests {
    use super::*;

    #[test]
    fn click_guide_defaults_false_when_absent() {
        // Old bundles have no `click_guide` key — it must deserialize to false.
        let json = r#"{"id":1,"song_id":2,"name":"verse","start":0.0,"end":4.0,"position":0}"#;
        let s: Section = serde_json::from_str(json).unwrap();
        assert!(!s.click_guide);
    }

    #[test]
    fn click_guide_round_trips_when_true() {
        let json = r#"{"id":1,"song_id":2,"name":"verse","start":0.0,"end":4.0,"position":0,"click_guide":true}"#;
        let s: Section = serde_json::from_str(json).unwrap();
        assert!(s.click_guide);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p practice click_guide_tests`
Expected: FAIL — `Section` has no field `click_guide`.

- [ ] **Step 3: Add the field**

In `crates/practice/src/model.rs`, add to `struct Section` after `position`:

```rust
    /// 0-based order within the song.
    pub position: i32,
    /// When true, the section gets a per-beat click guide during playback.
    #[serde(default)]
    pub click_guide: bool,
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p practice click_guide_tests`
Expected: PASS. (If other constructors of `Section` fail to compile, add `click_guide: false` to them.)

- [ ] **Step 5: Commit**

```bash
git add crates/practice/src/model.rs
git commit -m "feat(click-track): add click_guide flag to Section"
```

---

## Task 2: ClickMark + ClickVoice in the engine

**Files:**
- Modify: `crates/engine/src/pipeline.rs:8-14` (constants) and add new items
- Test: `crates/engine/src/pipeline.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Add inside the existing `mod tests` in `crates/engine/src/pipeline.rs`:

```rust
    #[test]
    fn click_voice_decays_and_is_silent_until_triggered() {
        let mut v = ClickVoice::default();
        assert_eq!(v.sample(1.0), 0.0, "silent before trigger");
        v.trigger(false);
        let first = v.sample(1.0).abs();
        // advance through most of the envelope
        for _ in 0..(CLICK_LEN_FRAMES / 2) {
            v.sample(1.0);
        }
        let later = v.sample(1.0).abs();
        assert!(first > 0.0, "audible right after trigger");
        assert!(later < first, "envelope decays");
    }

    #[test]
    fn click_voice_accent_is_louder() {
        let mut normal = ClickVoice::default();
        normal.trigger(false);
        let mut accent = ClickVoice::default();
        accent.trigger(true);
        assert!(accent.sample(1.0).abs() > normal.sample(1.0).abs());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p engine click_voice`
Expected: FAIL — `ClickVoice` not found.

- [ ] **Step 3: Add ClickMark, the shared wave fn, and ClickVoice**

In `crates/engine/src/pipeline.rs`, just below the click constants (after line 14), add:

```rust
/// One scheduled click: a beat time (song seconds) and whether it's accented
/// (a downbeat). The section-click schedule is a sorted slice of these.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClickMark {
    pub secs: f64,
    pub accent: bool,
}

/// The click waveform sample for a given envelope age (frames since trigger),
/// shared by the count-in pre-roll and the section-click overlay so they sound
/// identical. Silent once the envelope has decayed.
fn click_wave(age: usize, accent: bool, volume: f32) -> f32 {
    if age >= CLICK_LEN_FRAMES {
        return 0.0;
    }
    let t = age as f64 / SAMPLE_RATE as f64;
    let f = if accent { CLICK_FREQ_ACCENT } else { CLICK_FREQ_NORMAL };
    let env = (-CLICK_DECAY * t).exp();
    let amp = if accent { CLICK_AMP } else { CLICK_AMP * 0.7 };
    ((2.0 * std::f64::consts::PI * f * t).sin() * env) as f32 * amp * volume
}

/// A one-shot click for the section-click overlay: retrigger on each beat, mix
/// its `sample()` over the music until the envelope decays.
#[derive(Default)]
pub struct ClickVoice {
    age: usize,
    accent: bool,
}

impl ClickVoice {
    fn trigger(&mut self, accent: bool) {
        self.age = 0;
        self.accent = accent;
    }
    /// The sample for the current frame, then advances the envelope by one frame.
    fn sample(&mut self, volume: f32) -> f32 {
        let s = click_wave(self.age, self.accent, volume);
        self.age = self.age.saturating_add(1);
        s
    }
}
```

A freshly `Default`-constructed `ClickVoice` has `age == 0`, which would sound. Fix that by starting it decayed:

```rust
impl Default for ClickVoice {
    fn default() -> Self {
        Self { age: CLICK_LEN_FRAMES, accent: false }
    }
}
```

Remove the `#[derive(Default)]` line above when you add the manual `impl Default`.

- [ ] **Step 4: Refactor `click_sample` to use `click_wave` (DRY)**

Replace the body of the existing `fn click_sample(&self) -> f32` (lines ~232-249) with:

```rust
    fn click_sample(&self) -> f32 {
        click_wave(self.ci_click_age, self.ci_accent, self.volume)
    }
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p engine click_voice && cargo test -p engine count_in`
Expected: PASS (count-in behavior unchanged by the refactor).

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/pipeline.rs
git commit -m "feat(click-track): add ClickMark + ClickVoice, share click waveform"
```

---

## Task 3: Pipeline overlay + audible cursor

**Files:**
- Modify: `crates/engine/src/pipeline.rs` (struct fields, `new`, `apply` for SeekSecs, `render_song`, new helpers)
- Test: `crates/engine/src/pipeline.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Write the failing test**

Add inside `mod tests` (the existing `sine` helper builds a `SongBuffer`; a flat/quiet song makes the click the only signal):

```rust
    #[test]
    fn section_click_mixes_over_audio_at_scheduled_beat() {
        use std::sync::Arc;
        // 1s of silence so the click overlay is the only output signal.
        let song = StemSet::new(vec![sine(1.0, 440.0, 0.0)]);
        let mut p = Pipeline::new(song);
        p.set_click_schedule(Arc::new(vec![ClickMark { secs: 0.5, accent: false }]));
        p.apply(EngineCmd::Play);

        let mut out = vec![0.0f32; 256 * CHANNELS];
        let mut events = Vec::new();
        let mut first_loud: Option<usize> = None;
        let mut frame = 0usize;
        // render ~0.7s in 256-frame blocks
        for _ in 0..(SAMPLE_RATE as usize / 256) {
            out.iter_mut().for_each(|s| *s = 0.0);
            p.render(&mut out, &mut events);
            for i in 0..256 {
                if first_loud.is_none() && out[i * CHANNELS].abs() > 0.05 {
                    first_loud = Some(frame + i);
                }
            }
            frame += 256;
            if frame as f64 / SAMPLE_RATE as f64 > 0.7 {
                break;
            }
        }
        let at = first_loud.expect("a click sounded");
        let expected = (0.5 * SAMPLE_RATE as f64) as usize;
        // within ~20ms of the scheduled beat
        assert!(
            (at as i64 - expected as i64).abs() < (SAMPLE_RATE as i64 / 50),
            "click at frame {at}, expected ~{expected}"
        );
    }

    #[test]
    fn no_clicks_when_schedule_empty() {
        let song = StemSet::new(vec![sine(1.0, 440.0, 0.0)]);
        let mut p = Pipeline::new(song);
        p.apply(EngineCmd::Play);
        let mut out = vec![0.0f32; 256 * CHANNELS];
        let mut events = Vec::new();
        for _ in 0..20 {
            out.iter_mut().for_each(|s| *s = 0.0);
            p.render(&mut out, &mut events);
            assert!(out.iter().all(|s| s.abs() < 1e-6), "silent with no schedule");
        }
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p engine section_click`
Expected: FAIL — `set_click_schedule` not found.

- [ ] **Step 3: Add fields to `Pipeline` and init in `new`**

Add `use std::sync::Arc;` at the top of the file if not present. Add fields to `struct Pipeline` (after `ci_pass_at_end`):

```rust
    // section-click overlay
    clicks: Arc<Vec<ClickMark>>,
    click_cursor: usize,
    click_voice: ClickVoice,
    /// Audible song position in source frames (advances by `rate` per output
    /// frame). Decoupled from the looper's *feed* position so clicks line up
    /// with what is actually heard, not what has been fed to the stretcher.
    audible_frame: f64,
```

In `Pipeline::new`, initialize them (inside the returned `Self { ... }`):

```rust
            ci_pass_at_end: false,
            clicks: Arc::new(Vec::new()),
            click_cursor: 0,
            click_voice: ClickVoice::default(),
            audible_frame: 0.0,
```

- [ ] **Step 4: Add the schedule setter + cursor helper**

Add as `Pipeline` methods (near `arm_count_in`):

```rust
    /// Install a new section-click schedule (sorted by `secs`). Re-seeks the
    /// cursor to the current audible position; a ringing click finishes.
    pub fn set_click_schedule(&mut self, clicks: Arc<Vec<ClickMark>>) {
        self.clicks = clicks;
        self.reseek_click_cursor();
    }

    /// Point the cursor at the first mark at or after the audible position.
    fn reseek_click_cursor(&mut self) {
        let pos = self.audible_frame;
        self.click_cursor = self
            .clicks
            .partition_point(|m| m.secs * SAMPLE_RATE as f64 <= pos);
    }
```

- [ ] **Step 5: Seed the audible position on seek**

In `apply`, extend the `SeekSecs` arm:

```rust
            EngineCmd::SeekSecs(secs) => {
                self.looper.seek(secs_to_frames(secs));
                self.stretch.reset();
                self.audible_frame = secs.max(0.0) * SAMPLE_RATE as f64;
                self.reseek_click_cursor();
            }
```

- [ ] **Step 6: Mix the overlay in `render_song`**

In `render_song`, after the per-frame gain/volume ramp loop (after the `for fr in out.chunks_exact_mut(CHANNELS)` block, before the pause/every-loop tails), add:

```rust
        // Section-click overlay: walk the audible position across the frames we
        // just produced and mix a click ping at each scheduled beat. The click
        // is added at full reference level (scaled by volume only, not the
        // play/pause gain) so it stays a steady metronome.
        if self.playing {
            let region = self.looper.region();
            if self.clicks.is_empty() {
                // OFF fast path: no per-frame work — bulk-advance the audible
                // cursor so it stays valid if a schedule is installed mid-play.
                self.audible_frame += filled as f64 * self.rate;
                if let Some((start, end)) = region {
                    while self.audible_frame >= end as f64 {
                        self.audible_frame -= (end - start) as f64;
                    }
                }
            } else {
                for fr in out[..filled * CHANNELS].chunks_exact_mut(CHANNELS) {
                    let cur = self.audible_frame;
                    while self.click_cursor < self.clicks.len() {
                        let mark = self.clicks[self.click_cursor].secs * SAMPLE_RATE as f64;
                        if mark < cur {
                            self.click_cursor += 1; // stale (e.g. after a resize)
                        } else if mark < cur + self.rate {
                            self.click_voice.trigger(self.clicks[self.click_cursor].accent);
                            self.click_cursor += 1;
                            break;
                        } else {
                            break;
                        }
                    }
                    let s = self.click_voice.sample(self.volume);
                    fr[0] += s;
                    fr[1] += s;
                    self.audible_frame += self.rate;
                    if let Some((start, end)) = region {
                        if self.audible_frame >= end as f64 {
                            self.audible_frame -= (end - start) as f64;
                            self.reseek_click_cursor();
                        }
                    }
                }
            }
        }
```

Note: `filled` is in scope from the pull loop above. When the schedule is empty the overlay takes the OFF fast path — a single bulk add, honoring the spec's "no per-frame work when off". The per-frame loop runs only when there are marks to place.

- [ ] **Step 7: Run tests to verify they pass**

Run: `cargo test -p engine section_click no_clicks_when_schedule_empty`
Expected: PASS.

- [ ] **Step 8: Run the full engine suite (no regressions)**

Run: `cargo test -p engine`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/engine/src/pipeline.rs
git commit -m "feat(click-track): mix section-click overlay on the audible beat grid"
```

---

## Task 4: Thread the click slot to the audio thread

**Files:**
- Modify: `crates/engine/src/engine.rs`, `render_core.rs`, `output.rs`, `output_cpal.rs`

- [ ] **Step 1: Add the slot to `Engine`**

In `crates/engine/src/engine.rs`, add the field and types:

```rust
use crate::pipeline::{ClickMark, EngineCmd, EngineEvent};
```

Add to `struct Engine`:

```rust
    click_slot: Arc<ArcSwapOption<Vec<ClickMark>>>,
```

In `Engine::start`, create it and pass a clone to `spawn`:

```rust
        let click_slot = Arc::new(ArcSwapOption::<Vec<ClickMark>>::empty());
        let audio_thread = crate::output::spawn(
            cmd_rx,
            evt_tx,
            song_slot.clone(),
            click_slot.clone(),
            None,
            stop.clone(),
        )?;
```

Add `click_slot,` to the returned `Self { ... }`. Add the public setter:

```rust
    /// Replace the section-click schedule; the audio thread picks it up next block.
    pub fn set_click_schedule(&self, marks: Vec<ClickMark>) {
        self.click_slot.store(Some(Arc::new(marks)));
    }
```

In `set_output_device`, pass `self.click_slot.clone()` to the re-`spawn` (same arg position as in `start`).

- [ ] **Step 2: Consume the slot in `RenderCore`**

In `crates/engine/src/render_core.rs`:

```rust
use crate::pipeline::{ClickMark, EngineCmd, EngineEvent, Pipeline};
```

Add fields:

```rust
    click_slot: Arc<ArcSwapOption<Vec<ClickMark>>>,
    current_clicks: Option<Arc<Vec<ClickMark>>>,
```

Extend `RenderCore::new` to accept `click_slot` and init `current_clicks: None`.

In `fill`, immediately after the song-swap block (after `self.current_song = song;` closes), add:

```rust
        // Click-schedule swap: detect by pointer like the song slot.
        let cguard = self.click_slot.load();
        let cswapped = match (cguard.as_ref(), self.current_clicks.as_ref()) {
            (Some(a), Some(b)) => !Arc::ptr_eq(a, b),
            (Some(_), None) | (None, Some(_)) => true,
            (None, None) => false,
        };
        // A fresh pipeline (song swap) needs the current schedule re-applied too.
        if cswapped || swapped {
            let clicks = (*cguard).clone();
            if let Some(p) = self.pipeline.as_mut() {
                p.set_click_schedule(clicks.clone().unwrap_or_default());
            }
            self.current_clicks = clicks;
        }
```

`Arc<Vec<ClickMark>>` implements `Default` (empty `Arc`), so `unwrap_or_default()` yields an empty schedule when the slot is `None`.

- [ ] **Step 3: Thread through both output backends**

In `crates/engine/src/output.rs`: add `click_slot: Arc<ArcSwapOption<Vec<ClickMark>>>` to both `spawn` and `run` signatures (next to `song_slot`), pass it from `spawn`→`run`→`RenderCore::new`, and import `ClickMark` (`use crate::pipeline::{ClickMark, EngineCmd, EngineEvent};`).

Do the same in `crates/engine/src/output_cpal.rs` (`spawn`, `run`, the `RenderCore::new` call at line ~68).

- [ ] **Step 4: Build to verify wiring**

Run: `cargo build -p engine`
Expected: compiles clean.

- [ ] **Step 5: Run the engine suite**

Run: `cargo test -p engine`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/engine.rs crates/engine/src/render_core.rs crates/engine/src/output.rs crates/engine/src/output_cpal.rs
git commit -m "feat(click-track): plumb the click schedule slot to the audio thread"
```

---

## Task 5: Pure schedule builder (server)

**Files:**
- Create: `crates/server/src/section_click.rs`
- Modify: `crates/server/src/lib.rs` (add `mod section_click;`)

- [ ] **Step 1: Write the builder with failing tests**

Create `crates/server/src/section_click.rs`:

```rust
//! Pure construction of the section-click schedule: intersect the analyzed beat
//! grid with the marked sections, accenting downbeats. No audio, no I/O.

use engine::pipeline::ClickMark;
use practice::model::{Analysis, Section};

/// Tolerance (seconds) for matching a beat to a downbeat — the two grids come
/// from the same analysis but are stored as separate float lists.
const DOWNBEAT_EPS: f64 = 0.001;

fn is_downbeat(beat: f64, downbeats: &[f64]) -> bool {
    downbeats.iter().any(|d| (d - beat).abs() <= DOWNBEAT_EPS)
}

/// Every beat that falls inside a `click_guide` section, accented on downbeats.
/// Empty when no section is marked.
pub fn build_schedule(analysis: &Analysis, sections: &[Section]) -> Vec<ClickMark> {
    let spans: Vec<(f64, f64)> = sections
        .iter()
        .filter(|s| s.click_guide)
        .map(|s| (s.start, s.end))
        .collect();
    if spans.is_empty() {
        return Vec::new();
    }
    analysis
        .beats
        .iter()
        .filter(|&&b| spans.iter().any(|&(s, e)| b >= s && b < e))
        .map(|&b| ClickMark { secs: b, accent: is_downbeat(b, &analysis.downbeats) })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn section(id: i64, start: f64, end: f64, click: bool) -> Section {
        Section {
            id: practice::model::SectionId(id),
            song_id: practice::model::SongId(1),
            name: "s".into(),
            start,
            end,
            position: 0,
            click_guide: click,
        }
    }

    fn analysis(beats: Vec<f64>, downbeats: Vec<f64>) -> Analysis {
        Analysis { bpm: Some(120.0), beats, downbeats, sections: vec![], engine: "test".into() }
    }

    #[test]
    fn empty_when_no_section_marked() {
        let a = analysis(vec![0.0, 0.5, 1.0], vec![0.0]);
        let secs = vec![section(1, 0.0, 2.0, false)];
        assert!(build_schedule(&a, &secs).is_empty());
    }

    #[test]
    fn includes_only_beats_inside_marked_spans() {
        let a = analysis(vec![0.0, 0.5, 1.0, 1.5, 2.0], vec![0.0, 2.0]);
        // mark [1.0, 2.0): beats 1.0 and 1.5 (2.0 is the exclusive end)
        let secs = vec![section(1, 0.0, 1.0, false), section(2, 1.0, 2.0, true)];
        let marks = build_schedule(&a, &secs);
        let times: Vec<f64> = marks.iter().map(|m| m.secs).collect();
        assert_eq!(times, vec![1.0, 1.5]);
    }

    #[test]
    fn accents_downbeats() {
        let a = analysis(vec![0.0, 0.5, 1.0], vec![0.0, 1.0]);
        let secs = vec![section(1, 0.0, 2.0, true)];
        let marks = build_schedule(&a, &secs);
        assert_eq!(marks[0].accent, true); // 0.0 downbeat
        assert_eq!(marks[1].accent, false); // 0.5 offbeat
        assert_eq!(marks[2].accent, true); // 1.0 downbeat
    }
}
```

- [ ] **Step 2: Register the module**

In `crates/server/src/lib.rs`, add with the other `mod` lines:

```rust
mod section_click;
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p server section_click`
Expected: PASS (3 tests).

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/section_click.rs crates/server/src/lib.rs
git commit -m "feat(click-track): pure section-click schedule builder"
```

---

## Task 6: `set_click_schedule` on the AudioControl trait

**Files:**
- Modify: `crates/server/src/control.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/server/src/control.rs` (inline test module, or extend an existing one):

```rust
#[cfg(test)]
mod click_schedule_tests {
    use super::*;
    use engine::pipeline::ClickMark;

    #[test]
    fn mock_records_last_schedule() {
        let mut m = MockEngine::default();
        m.set_click_schedule(vec![ClickMark { secs: 1.0, accent: true }]);
        assert_eq!(m.click_schedule.len(), 1);
        assert_eq!(m.click_schedule[0].secs, 1.0);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p server click_schedule_tests`
Expected: FAIL — trait method / field missing.

- [ ] **Step 3: Add the trait method + impls**

In `crates/server/src/control.rs`:

```rust
use engine::pipeline::{ClickMark, EngineCmd, EngineEvent};
```

Add to the `AudioControl` trait:

```rust
    fn set_click_schedule(&mut self, marks: Vec<ClickMark>);
```

`impl AudioControl for engine::Engine`:

```rust
    fn set_click_schedule(&mut self, marks: Vec<ClickMark>) {
        engine::Engine::set_click_schedule(self, marks);
    }
```

Add a field to `MockEngine`:

```rust
    pub click_schedule: Vec<ClickMark>,
```

`impl AudioControl for MockEngine`:

```rust
    fn set_click_schedule(&mut self, marks: Vec<ClickMark>) {
        self.click_schedule = marks;
    }
```

`impl AudioControl for Arc<Mutex<MockEngine>>`:

```rust
    fn set_click_schedule(&mut self, marks: Vec<ClickMark>) {
        self.lock().unwrap().set_click_schedule(marks);
    }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p server click_schedule_tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/control.rs
git commit -m "feat(click-track): AudioControl.set_click_schedule"
```

---

## Task 7: Server commands + push_section_click + library persistence

**Files:**
- Modify: `crates/practice/src/library.rs` (add `set_section_click_guide`)
- Modify: `crates/server/src/app.rs` (commands, `push_section_click`, dispatch, recompute wiring)
- Test: `crates/server/tests/` (an integration test against `App` + `MockEngine`) — follow the pattern in `crates/server/tests/app_position.rs`

- [ ] **Step 1: Library setter with a failing test**

Add to `crates/practice/src/library.rs` (after `set_section_notes`):

```rust
    /// Toggle a section's click guide and rewrite the bundle manifest.
    pub fn set_section_click_guide(
        &mut self,
        song_id: SongId,
        section_id: SectionId,
        on: bool,
    ) -> Result<()> {
        let entry = self.entry_mut(song_id)?;
        if let Some(s) = entry.manifest.sections.iter_mut().find(|s| s.id == section_id) {
            s.click_guide = on;
        }
        Self::persist(entry)?;
        Ok(())
    }
```

Add `use crate::model::SectionId;` if the module doesn't already import it (it imports `SongId`, `Section`; confirm `SectionId` is in scope). Add an inline test in `library.rs`:

```rust
    #[test]
    fn set_click_guide_persists() {
        // build a temp library with one song + section, toggle, reload, assert.
        // (Mirror the setup used by the existing set_section_notes test.)
    }
```

Fill the test body by copying the setup from the nearest existing `library.rs` test that creates a temp bundle with sections; assert `list_sections` reports `click_guide == true` after the call and after a fresh `Library` load of the same root.

- [ ] **Step 2: Run the library test**

Run: `cargo test -p practice set_click_guide_persists`
Expected: PASS.

- [ ] **Step 3: Add `push_section_click` to App**

In `crates/server/src/app.rs`, add near `push_count_in` (use the same imports already present: `section_click::build_schedule`):

```rust
    /// Recompute the section-click schedule from the persisted master switch,
    /// the open song's sections, and its analyzed beat grid; push it to the
    /// engine. Empty schedule when off, no song open, or no analysis.
    fn push_section_click(&mut self) {
        let enabled = self
            .store
            .get_setting("section_click")
            .ok()
            .flatten()
            .and_then(|v| v.get("enabled").and_then(|e| e.as_bool()))
            .unwrap_or(false);
        let marks = match &self.open_song {
            Some(o) if enabled => {
                let song_id = o.song.id;
                match self.library.get_analysis(song_id) {
                    Some(a) => {
                        let sections = self.library.list_sections(song_id);
                        crate::section_click::build_schedule(&a, &sections)
                    }
                    None => Vec::new(),
                }
            }
            _ => Vec::new(),
        };
        self.audio.set_click_schedule(marks);
    }
```

- [ ] **Step 4: Add the two commands**

In `app.rs`, add the handlers (model `section_click_set` after `section_notes_set`):

```rust
    fn section_click_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            section_id: SectionId,
            on: bool,
        }
        let p: P = from_params(p)?;
        let song_id = self
            .open_song
            .as_ref()
            .map(|o| o.song.id)
            .ok_or_else(|| "no song open".to_string())?;
        self.library
            .set_section_click_guide(song_id, p.section_id, p.on)
            .err_str()?;
        self.push_section_click();
        let (sections, orphan_notes) = self.sections_payload(song_id)?;
        Ok(json!({ "sections": sections, "orphan_notes": orphan_notes }))
    }

    fn section_click_enable(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            enabled: bool,
        }
        let p: P = from_params(p)?;
        self.store
            .set_setting("section_click", &json!({ "enabled": p.enabled }))
            .err_str()?;
        self.push_section_click();
        Ok(Value::Null)
    }
```

Register both in the `dispatch_inner` match (near `"countin.set"` / `"section.notes.set"`):

```rust
            "section.click.set" => self.section_click_set(p),
            "sectionclick.set" => self.section_click_enable(p),
```

- [ ] **Step 5: Recompute at the right moments**

Call `self.push_section_click();` immediately after each existing `self.push_count_in();` call site (song open at ~line 759, and the end-of-`open` path; plus wherever count-in is refreshed). Also add it right after the post-analysis section auto-commit (the `commit_analysis_sections` success branch near line 748-761), so a newly analyzed song with marked sections starts clicking without a reload.

- [ ] **Step 6: Integration test**

Create `crates/server/tests/app_section_click.rs`, mirroring `app_position.rs` setup (build an `App` with `Arc<Mutex<MockEngine>>`, import a song fixture, run analysis or inject analysis, mark a section). Assert:

```rust
// after enabling the master switch, marking a section, with analysis present,
// the MockEngine received a non-empty click schedule:
let sched = mock.lock().unwrap().click_schedule.clone();
assert!(!sched.is_empty());
// and disabling the master switch pushes an empty schedule:
// (call sectionclick.set {enabled:false}, then assert sched is empty)
```

Use the existing fixture helpers from the other `tests/` files for song import + analysis injection. If injecting analysis directly is hard, gate the assertion on `library.get_analysis` returning `Some` via the same path `app_position.rs` uses.

- [ ] **Step 7: Run the server suite**

Run: `cargo test -p server`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/practice/src/library.rs crates/server/src/app.rs crates/server/tests/app_section_click.rs
git commit -m "feat(click-track): section-click commands, persistence, and engine push"
```

---

## Task 8: Frontend store wiring

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts`

- [ ] **Step 1: Add the `clickGuide` field to the Section interface**

In `apps/desktop/src/lib/stores.ts`, add to `interface Section` (after `notes`):

```ts
  /** Per-section beat-click guide flag. */
  clickGuide?: boolean;
```

Note: the server serializes the Rust field as `click_guide` (snake_case). Add a normalization where sections arrive, OR change the field to `click_guide?: boolean` to match the wire shape directly. Match the existing convention in this file — the other section fields (`song_id`) are snake_case, so use:

```ts
  /** Per-section beat-click guide flag (wire field). */
  click_guide?: boolean;
```

- [ ] **Step 2: Add the master-arm store, setting key, and availability**

Add near the `countIn` store (~line 254):

```ts
export const SECTION_CLICK = "section_click";

export const sectionClick = writable<{ enabled: boolean }>({ enabled: false });

/** Section click needs an analyzed beat grid, same gate as count-in. */
export const sectionClickAvailable = countInAvailable;
```

(`countInAvailable` already derives `analysis?.bpm != null`; reuse it so the two stay in lockstep.)

- [ ] **Step 3: Hydrate the master arm at launch**

In `loadSettings` (after the `COUNT_IN` block, ~line 449):

```ts
    const sc = all[SECTION_CLICK];
    if (sc && typeof sc === "object") {
      const s = sc as { enabled?: unknown };
      sectionClick.set({ enabled: typeof s.enabled === "boolean" ? s.enabled : false });
    }
```

- [ ] **Step 4: Add the actions**

In the `actions` object (near `setCountIn`):

```ts
  async setSectionClick(enabled: boolean): Promise<void> {
    sectionClick.set({ enabled });
    await cmd("sectionclick.set", { enabled });
  },

  /** Toggle one section's beat-click guide; server returns refreshed sections. */
  async toggleSectionClick(sectionId: number, on: boolean): Promise<void> {
    const out = await cmd<{ sections: Section[]; orphan_notes: OrphanNote[] }>(
      "section.click.set",
      { section_id: sectionId, on },
    );
    openSong.update((o) =>
      o ? { ...o, sections: out.sections, orphan_notes: out.orphan_notes } : o,
    );
  },
```

- [ ] **Step 5: Typecheck**

Run: `cd apps/desktop && pnpm svelte-check`
Expected: no new errors.

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/lib/stores.ts
git commit -m "feat(click-track): frontend store wiring for section click"
```

---

## Task 9: ClickTrack control box + move count-in out of Transport

**Files:**
- Create: `apps/desktop/src/components/ClickTrack.svelte`
- Modify: `apps/desktop/src/components/Transport.svelte` (remove count-in controls; keep the playhead pulse)
- Modify: `apps/desktop/src/App.svelte` (mount the box on the stage)

- [ ] **Step 1: Build the box**

Create `apps/desktop/src/components/ClickTrack.svelte` using `lib/ui/Box.svelte` and `lib/ui/Group.svelte`. Move the count-in markup (the `{#if $countInAvailable}` block at `Transport.svelte:129-156` — on/off pill, loop-mode word, beats stepper) into a "count in" `Group`. Add a "section click" `Group` with a single on/off pill bound to `sectionClick`:

```svelte
<script lang="ts">
  import Box from "../lib/ui/Box.svelte";
  import Group from "../lib/ui/Group.svelte";
  import { actions, countIn, sectionClick, sectionClickAvailable } from "../lib/stores";
  import { stepCountInBeats } from "../lib/count-in";

  function stepCount(d: number) {
    void actions.setCountIn({ beats: stepCountInBeats($countIn.beats, d) });
  }
</script>

{#if $sectionClickAvailable}
  <Box label="click track">
    <Group label="count in">
      <!-- moved verbatim from Transport: on/off pill, loop-mode word, beats stepper -->
    </Group>
    <Group label="section click">
      <button
        class="pill"
        class:on={$sectionClick.enabled}
        onclick={() => actions.setSectionClick(!$sectionClick.enabled)}
        title="click every beat during marked sections"
      >{$sectionClick.enabled ? "on" : "off"}</button>
    </Group>
  </Box>
{/if}
```

Use the theme accent (`--accent`/`--accent-dim`) for the `.on` state — do not hardcode cyan/amber (see project conventions). Reuse the count-in pill/stepper CSS from `Transport.svelte` (copy the relevant rules into this component's `<style>`).

- [ ] **Step 2: Remove count-in controls from Transport (keep the pulse)**

In `Transport.svelte`, delete the count-in control block (lines ~129-156) and its now-unused count-in CSS (the pill/stepper/loop-mode rules at ~335-361). Leave untouched: the count-in **playhead pulse** logic (whatever reads `$position.countIn` to pulse the playhead) and the `position` import. Remove the now-unused `countIn` / `stepCountInBeats` imports from Transport if nothing else uses them.

- [ ] **Step 3: Mount the box on the stage**

In `App.svelte`, add `<ClickTrack />` to the stage's flowing row of control boxes (with `Isolation`, `Notes`, `Tuner`, `Drill`), importing it at the top. Place it after `Isolation` (it is a global playback aid, like isolation).

- [ ] **Step 4: Typecheck + build**

Run: `cd apps/desktop && pnpm svelte-check`
Expected: no new errors.

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/components/ClickTrack.svelte apps/desktop/src/components/Transport.svelte apps/desktop/src/App.svelte
git commit -m "feat(click-track): Click track control box; relocate count-in controls"
```

---

## Task 10: Per-section toggle in the structure tab

**Files:**
- Modify: `apps/desktop/src/components/Sections.svelte`

- [ ] **Step 1: Add the per-row toggle**

In `Sections.svelte`, for each section row, add a small click-guide toggle button gated on `$sectionClickAvailable`. Wire it to the action:

```svelte
<script lang="ts">
  import { actions, sectionClickAvailable } from "../lib/stores";
  // ...existing imports
</script>

<!-- inside the per-section row markup -->
{#if $sectionClickAvailable}
  <button
    class="click-toggle"
    class:on={section.click_guide}
    title="beat-click guide during this section"
    aria-label="toggle beat click for this section"
    onclick={() => actions.toggleSectionClick(section.id, !section.click_guide)}
  >♩</button>
{/if}
```

Style `.click-toggle.on` with the theme accent (`--accent`). Keep the glyph simple and CSS-sized; if a quarter-note glyph blobs at small sizes, use a small inline SVG following the Transport icon convention (viewBox 24, stroke ~2) instead of a Unicode glyph.

- [ ] **Step 2: Typecheck**

Run: `cd apps/desktop && pnpm svelte-check`
Expected: no new errors.

- [ ] **Step 3: Commit**

```bash
git add apps/desktop/src/components/Sections.svelte
git commit -m "feat(click-track): per-section beat-click toggle in the structure tab"
```

---

## Task 11: Full verification

- [ ] **Step 1: Run the whole suite**

Run: `just test`
Expected: `cargo test --workspace` + `pnpm vitest run` all PASS.

- [ ] **Step 2: Lint gate**

Run: `just lint`
Expected: clippy clean (`-D warnings`), `cargo fmt --check` clean, `svelte-check` clean.

- [ ] **Step 3: Build + manual smoke (human checklist)**

Run: `just build` then `just run`. The desktop WebKitGTK webview can't be driven by chrome-devtools, so verify by hand:
- Open an analyzed song → a **Click track** box appears on the stage with **count in** and **section click** groups; count-in is gone from the transport but the count-in playhead pulse still fires on play.
- Structure tab shows a click toggle per section; toggling one and enabling **section click**, then playing through that section, produces a beat click over the audio that stops at the section boundary.
- At 0.5× speed the clicks slow with the music and stay on the beats.
- With **section click** off (or no sections marked), playback is silent of clicks.

- [ ] **Step 4: Commit any fmt/lint fixups**

```bash
git add -A
git commit -m "chore(click-track): lint/test fixups"
```

---

## Self-review notes

- **Spec coverage:** per-section toggle in structure tab (Task 1, 7, 10); global master arm in box (Task 7, 8, 9); real-beat-grid timing + downbeat accent (Task 2, 3, 5); shared click voice (Task 2); audible-position alignment + speed-fader tracking + loop-boundary handoff (Task 3); lock-free slot transport (Task 4); no-analysis gating (Task 8 `sectionClickAvailable`, Task 7 empty schedule); persistence split global-setting vs per-song-flag (Task 1, 7, 8); performance early-out for empty schedule (Task 3 — `!self.clicks.is_empty()` gate). All spec sections map to a task.
- **Deferred (per spec, intentionally not in v1):** configurable accent on/off and click level; auto-detect drumless sections from the drums stem; any visual feedback during section click.
