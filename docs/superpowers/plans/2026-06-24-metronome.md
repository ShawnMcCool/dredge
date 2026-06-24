# Metronome Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A standalone practice metronome — set a tempo and it clicks, with or without a song loaded — as its own always-visible stage control box, with time signature, cadence, downbeat accent, selectable sound kits, tap tempo, sync-to-song, and a live bar indicator.

**Architecture:** A `Metronome` generator lives in the engine **render core** (not the song pipeline), so it runs even when no song/pipeline exists; the render core mixes its synthesized voices over whatever the pipeline produced (song audio or silence) and emits a `MetronomeBeat` event per beat. It's driven by one `Copy` `EngineCmd::SetMetronome` over the existing command ring (no new slot plumbing). Sounds are synthesized (click reuses the existing `click_wave`; kick/snare/cowbell are small inline synths). The server persists a `metronome` setting and broadcasts beat events; the frontend renders a `MetronomeBox` and computes tap tempo client-side.

**Tech Stack:** Rust (engine: RT audio synthesis; server: dispatcher + settings), Svelte 5 + TypeScript, vitest + `cargo test`.

**Execution note:** This repo works directly on `main` (no feature branch/worktree). `git add` only the named files per task — never `git add -A` (unrelated untracked files exist). Commit after each task.

---

## File structure

**Created:**
- `crates/engine/src/metronome.rs` — `Cadence`, `Kit`, `MetronomeBeat`, the voice synths (kick/snare/cowbell), and the `Metronome` generator.
- `apps/desktop/src/lib/metronome.ts` — pure frontend logic: tap-tempo, BPM clamp, bar-indicator mapping.
- `apps/desktop/src/components/MetronomeBox.svelte` — the control box UI.

**Modified:**
- `crates/engine/src/pipeline.rs` — make `click_wave` `pub(crate)`; add `EngineCmd::SetMetronome` and `EngineEvent::MetronomeBeat`.
- `crates/engine/src/lib.rs` — `pub mod metronome;`.
- `crates/engine/src/render_core.rs` — own a `Metronome`, intercept `SetMetronome`, mix + emit beat events.
- `crates/server/src/control.rs` — `AudioControl::set_metronome` (+ MockEngine record).
- `crates/server/src/app.rs` — `metronome.set` command, `push_metronome`, `metronome` setting, broadcast `MetronomeBeat`.
- `apps/desktop/src/lib/stores.ts` — `metronome` store, actions, event handling, hydration.
- `apps/desktop/src/App.svelte` — mount `<MetronomeBox />` unconditionally on the stage.

---

## Task 1: Engine — sound kits and voice synthesis

**Files:**
- Create: `crates/engine/src/metronome.rs`
- Modify: `crates/engine/src/pipeline.rs` (make `click_wave` `pub(crate)`), `crates/engine/src/lib.rs` (add `pub mod metronome;`)

- [ ] **Step 1: Make `click_wave` reachable + register the module**

In `crates/engine/src/pipeline.rs`, change the `click_wave` signature from `fn click_wave(` to `pub(crate) fn click_wave(` (it's the shared sine-ping synth; the Click kit reuses it). Leave its body unchanged.

In `crates/engine/src/lib.rs`, add alongside the other `pub mod` lines:

```rust
pub mod metronome;
```

- [ ] **Step 2: Write the failing tests**

Create `crates/engine/src/metronome.rs` with these tests at the bottom (they reference items added in Step 3):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_is_silent_until_triggered_then_decays() {
        let mut v = Voice::default();
        let mut rng = 0x1234_5678u32;
        assert_eq!(v.sample(1.0, &mut rng), 0.0);
        v.trigger(Sound::Kick);
        // a few frames in, the kick is audible
        let mut peak = 0.0f32;
        for _ in 0..200 {
            peak = peak.max(v.sample(1.0, &mut rng).abs());
        }
        assert!(peak > 0.0, "kick is audible after trigger");
        // well past the kick length (0.15s ≈ 6615 frames) it is silent
        for _ in 0..7000 {
            v.sample(1.0, &mut rng);
        }
        assert_eq!(v.sample(1.0, &mut rng), 0.0, "decayed to silence");
    }

    #[test]
    fn every_kit_produces_sound_for_both_roles() {
        let mut rng = 0x9e37_79b9u32;
        for kit in [Kit::Click, Kit::KickSnare, Kit::Cowbell] {
            for accent in [true, false] {
                let mut v = Voice::default();
                v.trigger(sound_for(kit, accent));
                let mut peak = 0.0f32;
                for _ in 0..400 {
                    peak = peak.max(v.sample(1.0, &mut rng).abs());
                }
                assert!(peak > 0.0, "{kit:?} accent={accent} is audible");
            }
        }
    }

    #[test]
    fn click_kit_accent_is_louder_than_normal() {
        let mut rng = 1u32;
        let mut hi = Voice::default();
        hi.trigger(sound_for(Kit::Click, true));
        let mut lo = Voice::default();
        lo.trigger(sound_for(Kit::Click, false));
        let mut hp = 0.0f32;
        let mut lp = 0.0f32;
        for _ in 0..400 {
            hp = hp.max(hi.sample(1.0, &mut rng).abs());
            lp = lp.max(lo.sample(1.0, &mut rng).abs());
        }
        assert!(hp > lp, "click accent louder: {hp} > {lp}");
    }
}
```

- [ ] **Step 3: Implement the enums + voices**

At the top of `crates/engine/src/metronome.rs`:

```rust
use crate::buffer::SAMPLE_RATE;
use crate::pipeline::click_wave;

/// Which beats of the bar actually click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    EveryBeat,
    HalfBar,
    EveryBar,
}

/// A sound kit: a downbeat sound paired with an other-beats sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kit {
    Click,
    KickSnare,
    Cowbell,
}

/// One concrete voice the metronome can sound on a beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Sound {
    ClickHi,
    ClickLo,
    Kick,
    Snare,
    CowbellHi,
    CowbellLo,
}

/// The voice a kit uses for a beat: accent (downbeat) vs the rest.
fn sound_for(kit: Kit, accent: bool) -> Sound {
    match (kit, accent) {
        (Kit::Click, true) => Sound::ClickHi,
        (Kit::Click, false) => Sound::ClickLo,
        (Kit::KickSnare, true) => Sound::Kick,
        (Kit::KickSnare, false) => Sound::Snare,
        (Kit::Cowbell, true) => Sound::CowbellHi,
        (Kit::Cowbell, false) => Sound::CowbellLo,
    }
}

/// Bipolar white noise in [-1, 1) from a tiny xorshift RNG (no allocation).
fn noise(rng: &mut u32) -> f32 {
    let mut x = *rng;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *rng = x;
    (x as f32 / u32::MAX as f32) * 2.0 - 1.0
}

const SR: f64 = SAMPLE_RATE as f64;

/// Sample for a synthesized drum sound at envelope age `age` (frames since
/// trigger), or 0.0 once it has decayed. Self-contained per frame (instantaneous
/// frequency for the kick sweep — exact enough for a practice click).
fn synth(sound: Sound, age: usize, rng: &mut u32, volume: f32) -> f32 {
    let t = age as f64 / SR;
    let (len_s, raw) = match sound {
        Sound::ClickHi => return click_wave(age, true, volume),
        Sound::ClickLo => return click_wave(age, false, volume),
        Sound::Kick => {
            let f = 55.0 + 90.0 * (-t * 50.0).exp(); // 145 → 55 Hz sweep
            let env = (-t * 22.0).exp();
            (0.15, (2.0 * std::f64::consts::PI * f * t).sin() * env * 0.9)
        }
        Sound::Snare => {
            let tone = (2.0 * std::f64::consts::PI * 180.0 * t).sin();
            let env = (-t * 30.0).exp();
            let n = noise(rng) as f64;
            (0.12, (0.7 * n + 0.3 * tone) * env * 0.6)
        }
        Sound::CowbellHi => (0.30, cowbell(t, 540.0, 800.0)),
        Sound::CowbellLo => (0.30, cowbell(t, 430.0, 640.0)),
    };
    if t >= len_s {
        return 0.0;
    }
    raw as f32 * volume
}

fn cowbell(t: f64, f1: f64, f2: f64) -> f64 {
    let env = (-t * 12.0).exp();
    let tau = 2.0 * std::f64::consts::PI;
    0.5 * ((tau * f1 * t).sin() + (tau * f2 * t).sin()) * env * 0.45
}

/// A one-shot retriggerable voice. The metronome holds exactly one (beats never
/// overlap audibly at ≤300 BPM), retriggered on each sounding beat.
struct Voice {
    age: usize,
    sound: Sound,
    silent: bool,
}

impl Default for Voice {
    fn default() -> Self {
        Self { age: 0, sound: Sound::ClickLo, silent: true }
    }
}

impl Voice {
    fn trigger(&mut self, sound: Sound) {
        self.sound = sound;
        self.age = 0;
        self.silent = false;
    }
    /// Current sample then advance one frame. 0.0 while silent/decayed.
    fn sample(&mut self, volume: f32, rng: &mut u32) -> f32 {
        if self.silent {
            return 0.0;
        }
        let s = synth(self.sound, self.age, rng, volume);
        self.age = self.age.saturating_add(1);
        if s == 0.0 && self.age > 1 {
            self.silent = true; // decayed past its length
        }
        s
    }
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p engine metronome::tests`
Expected: 3 pass. (`Voice`/`Sound`/`sound_for`/`Kit`/`Cadence` are crate-internal; `Kit`/`Cadence` are `pub` for later tasks, `Voice`/`Sound` stay private.)

- [ ] **Step 5: Confirm no clippy/build regressions**

Run: `cargo clippy -p engine -- -D warnings`
Expected: clean. (Some items — `Cadence`, the `Metronome` generator — are added in Task 2; if `Kit`/`Cadence`/`sound_for` are flagged dead-code now, that clears in Task 2 which consumes them. If clippy fails the `-D warnings` gate on dead-code for `Cadence` only, add a temporary `#[allow(dead_code)]` on `enum Cadence` with a `// consumed in Task 2` comment and REMOVE it in Task 2.)

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/metronome.rs crates/engine/src/pipeline.rs crates/engine/src/lib.rs
git commit -m "feat(metronome): synthesized sound kits (click/kick/snare/cowbell)"
```

---

## Task 2: Engine — the Metronome generator

**Files:**
- Modify: `crates/engine/src/metronome.rs`

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/engine/src/metronome.rs`:

```rust
    use crate::buffer::CHANNELS;

    fn render_secs(m: &mut Metronome, secs: f64) -> (Vec<f32>, Vec<MetronomeBeat>) {
        let frames = (secs * SR) as usize;
        let mut out = vec![0.0f32; frames * CHANNELS];
        let mut beats = Vec::new();
        // render in 256-frame blocks like the audio callback
        let mut done = 0;
        while done < frames {
            let n = (frames - done).min(256);
            m.render(&mut out[done * CHANNELS..(done + n) * CHANNELS], 1.0, &mut beats);
            done += n;
        }
        (out, beats)
    }

    #[test]
    fn fires_beats_at_the_bpm_interval() {
        let mut m = Metronome::default();
        // 120 BPM = 0.5 s/beat, 4/4, every beat, click kit
        m.configure(true, 0.5, 4, Cadence::EveryBeat, Kit::Click);
        let (_out, beats) = render_secs(&mut m, 2.1); // ~4 beats in 2s
        let sounded: Vec<_> = beats.iter().filter(|b| b.sounded).collect();
        assert!(sounded.len() >= 4 && sounded.len() <= 5, "got {} beats", sounded.len());
        // first beat is the downbeat
        assert_eq!(beats[0].beat, 1);
        assert_eq!(beats[0].of, 4);
    }

    #[test]
    fn downbeat_recurs_every_bar() {
        let mut m = Metronome::default();
        m.configure(true, 0.25, 4, Cadence::EveryBeat, Kit::Click); // fast
        let (_o, beats) = render_secs(&mut m, 2.1);
        // beats cycle 1,2,3,4,1,2,3,4,...
        let labels: Vec<u32> = beats.iter().map(|b| b.beat).take(8).collect();
        assert_eq!(labels, vec![1, 2, 3, 4, 1, 2, 3, 4]);
    }

    #[test]
    fn every_bar_cadence_sounds_only_the_downbeat() {
        let mut m = Metronome::default();
        m.configure(true, 0.25, 4, Cadence::EveryBar, Kit::Click);
        let (_o, beats) = render_secs(&mut m, 2.1);
        for b in &beats {
            assert_eq!(b.sounded, b.beat == 1, "beat {} sounded={}", b.beat, b.sounded);
        }
    }

    #[test]
    fn half_bar_cadence_sounds_one_and_mid() {
        let mut m = Metronome::default();
        m.configure(true, 0.25, 4, Cadence::HalfBar, Kit::Click); // N=4 → beats 1 and 3
        let (_o, beats) = render_secs(&mut m, 2.1);
        for b in &beats {
            let want = b.beat == 1 || b.beat == 3;
            assert_eq!(b.sounded, want, "beat {} sounded={}", b.beat, b.sounded);
        }
    }

    #[test]
    fn stopped_metronome_is_silent_and_emits_no_beats() {
        let mut m = Metronome::default();
        m.configure(false, 0.5, 4, Cadence::EveryBeat, Kit::Click);
        let (out, beats) = render_secs(&mut m, 1.0);
        assert!(beats.is_empty());
        assert!(out.iter().all(|s| *s == 0.0));
    }

    #[test]
    fn audible_output_when_running() {
        let mut m = Metronome::default();
        m.configure(true, 0.5, 4, Cadence::EveryBeat, Kit::KickSnare);
        let (out, _b) = render_secs(&mut m, 1.0);
        assert!(out.iter().any(|s| s.abs() > 0.01), "metronome produced audio");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p engine metronome::tests::fires_beats_at_the_bpm_interval`
Expected: FAIL — `Metronome` / `MetronomeBeat` not found.

- [ ] **Step 3: Implement `MetronomeBeat` + `Metronome`**

Add to `crates/engine/src/metronome.rs` (above the tests):

```rust
use crate::buffer::CHANNELS;

/// Emitted once per beat while running, for the UI bar indicator. 1-based beat
/// within the bar; `of` is beats-per-bar; `sounded` reflects the cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetronomeBeat {
    pub beat: u32,
    pub of: u32,
    pub sounded: bool,
}

/// Free-running click generator, owned by the render core so it runs with or
/// without a song. Mixes its voice over the output buffer (add) and pushes a
/// `MetronomeBeat` per beat.
pub struct Metronome {
    running: bool,
    interval: usize, // frames between beats
    beats_per_bar: u32,
    cadence: Cadence,
    kit: Kit,
    to_next: usize, // frames until the next beat boundary
    beat_idx: u32,  // 0-based running beat counter
    voice: Voice,
    rng: u32,
}

impl Default for Metronome {
    fn default() -> Self {
        Self {
            running: false,
            interval: (0.5 * SR) as usize,
            beats_per_bar: 4,
            cadence: Cadence::EveryBeat,
            kit: Kit::Click,
            to_next: 0,
            beat_idx: 0,
            voice: Voice::default(),
            rng: 0x2545_f491,
        }
    }
}

/// Does beat `beat` (0-based) of an `n`-beat bar sound under `cadence`?
fn cadence_sounds(cadence: Cadence, beat: u32, n: u32) -> bool {
    match cadence {
        Cadence::EveryBeat => true,
        Cadence::EveryBar => beat == 0,
        // beat 0 and the mid-bar beat; for odd n the split is approximate.
        Cadence::HalfBar => beat == 0 || beat == n / 2,
    }
}

impl Metronome {
    /// Apply config. Re-deriving the interval from `beat_secs`; starting (a
    /// stopped→running edge) resets the bar to beat 1 immediately. A config
    /// change while already running keeps the running phase (no bar restart).
    pub fn configure(
        &mut self,
        running: bool,
        beat_secs: f64,
        beats_per_bar: u32,
        cadence: Cadence,
        kit: Kit,
    ) {
        let starting = running && !self.running;
        self.running = running;
        self.interval = ((beat_secs * SR).round() as usize).max(1);
        self.beats_per_bar = beats_per_bar.max(1);
        self.cadence = cadence;
        self.kit = kit;
        if starting {
            self.beat_idx = 0;
            self.to_next = 0; // first beat fires on the next frame
        } else if self.running {
            self.to_next = self.to_next.min(self.interval); // keep phase, clamp
        }
    }

    /// Mix the metronome over `out` (interleaved stereo, additive) and push beat
    /// events. `volume` is the user playback volume.
    pub fn render(&mut self, out: &mut [f32], volume: f32, events: &mut Vec<MetronomeBeat>) {
        let frames = out.len() / CHANNELS;
        for i in 0..frames {
            if self.running {
                if self.to_next == 0 {
                    let beat = self.beat_idx % self.beats_per_bar;
                    let sounded = cadence_sounds(self.cadence, beat, self.beats_per_bar);
                    if sounded {
                        self.voice.trigger(sound_for(self.kit, beat == 0));
                    }
                    events.push(MetronomeBeat {
                        beat: beat + 1,
                        of: self.beats_per_bar,
                        sounded,
                    });
                    self.beat_idx = self.beat_idx.wrapping_add(1);
                    self.to_next = self.interval;
                }
                self.to_next -= 1;
            }
            let s = self.voice.sample(volume, &mut self.rng);
            out[i * CHANNELS] += s;
            out[i * CHANNELS + 1] += s;
        }
    }
}
```

Now remove the temporary `#[allow(dead_code)]` on `Cadence` if Task 1 added one.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p engine metronome`
Expected: all metronome tests pass (Task 1 + Task 2).

- [ ] **Step 5: Clippy**

Run: `cargo clippy -p engine -- -D warnings`
Expected: clean (the generator now consumes `Cadence`/`Kit`/`sound_for`/`Voice`).

- [ ] **Step 6: Commit**

```bash
git add crates/engine/src/metronome.rs
git commit -m "feat(metronome): free-running beat generator with cadence + accent"
```

---

## Task 3: Engine — command, event, and render-core integration

**Files:**
- Modify: `crates/engine/src/pipeline.rs` (add `EngineCmd::SetMetronome`, `EngineEvent::MetronomeBeat`)
- Modify: `crates/engine/src/render_core.rs` (own + drive the metronome)

- [ ] **Step 1: Add the command and event variants**

In `crates/engine/src/pipeline.rs`:

Add an import near the top: `use crate::metronome::{Cadence, Kit};`

Add to `enum EngineCmd` (it derives `Copy`; `Cadence`/`Kit` are `Copy` enums, so this stays `Copy`):

```rust
    /// Configure the free-running metronome (handled by the render core, not the
    /// pipeline). `beat_secs` is the beat interval (60 / bpm).
    SetMetronome {
        running: bool,
        beat_secs: f64,
        beats_per_bar: u32,
        cadence: Cadence,
        kit: Kit,
    },
```

Add to `enum EngineEvent`:

```rust
    /// One metronome beat (1-based within the bar). Drives the UI bar indicator.
    MetronomeBeat {
        beat: u32,
        of: u32,
        sounded: bool,
    },
```

The `Pipeline::apply` match must stay exhaustive: add an arm that ignores it (the metronome is not a pipeline concern):

```rust
            EngineCmd::SetMetronome { .. } => {} // handled by the render core
```

- [ ] **Step 2: Write the failing test (render core drives the metronome with no song)**

In `crates/engine/src/render_core.rs`, add an inline test module. First check whether `RenderCore::new` can be constructed in a test — it needs `rtrb` rings + the `ArcSwapOption` slots. Build them inline:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::CHANNELS;
    use crate::metronome::{Cadence, Kit};
    use crate::pipeline::EngineCmd;

    fn core() -> (RenderCore, rtrb::Producer<EngineCmd>) {
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::<EngineCmd>::new(64);
        let (evt_tx, _evt_rx) = rtrb::RingBuffer::<EngineEvent>::new(256);
        let song_slot = Arc::new(ArcSwapOption::<StemSet>::empty());
        let click_slot = Arc::new(ArcSwapOption::<Vec<crate::pipeline::ClickMark>>::empty());
        (RenderCore::new(cmd_rx, evt_tx, song_slot, click_slot), cmd_tx)
    }

    #[test]
    fn metronome_sounds_with_no_song_loaded() {
        let (mut rc, mut cmd_tx) = core();
        cmd_tx
            .push(EngineCmd::SetMetronome {
                running: true,
                beat_secs: 0.5,
                beats_per_bar: 4,
                cadence: Cadence::EveryBeat,
                kit: Kit::Click,
            })
            .unwrap();
        // render ~0.6s in blocks; no song slot set → would be silence without the metronome
        let mut any = false;
        let mut out = vec![0.0f32; 256 * CHANNELS];
        for _ in 0..((0.6 * crate::buffer::SAMPLE_RATE as f64) as usize / 256) {
            out.iter_mut().for_each(|s| *s = 0.0);
            rc.fill(&mut out);
            if out.iter().any(|s| s.abs() > 0.01) {
                any = true;
            }
        }
        assert!(any, "metronome produced audio with no song loaded");
    }
}
```

Confirm the `RenderCore::new` argument order matches the real signature (Task 4 of the click-track feature added `click_slot` after `song_slot`). Adjust the test's `core()` to the real order if needed. If `EngineEvent`/`StemSet`/`ArcSwapOption`/`Arc` aren't already imported in `render_core.rs`, the test's `use super::*;` should cover those that are in scope; add explicit `use` lines in the test module for any that aren't.

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p engine render_core`
Expected: FAIL — `RenderCore` has no metronome; output is silent with no song.

- [ ] **Step 4: Integrate the metronome into `RenderCore`**

In `crates/engine/src/render_core.rs`:

Add imports: `use crate::metronome::{Metronome, MetronomeBeat};`

Add fields to `struct RenderCore`:

```rust
    metronome: Metronome,
    metro_beats: Vec<MetronomeBeat>,
```

In `RenderCore::new`, initialize them: `metronome: Metronome::default(), metro_beats: Vec::with_capacity(16),`.

In `fill`, the command-drain loop currently latches `SetVolume` and forwards every command to the pipeline. Restructure it so `SetMetronome` is applied to the metronome and NOT forwarded to the pipeline (the pipeline ignores it anyway, but intercepting is cleaner and avoids a pointless apply):

```rust
        while let Ok(cmd) = self.cmd_rx.pop() {
            if let EngineCmd::SetVolume(v) = cmd {
                self.volume = v;
            }
            if let EngineCmd::SetMetronome {
                running,
                beat_secs,
                beats_per_bar,
                cadence,
                kit,
            } = cmd
            {
                self.metronome.configure(running, beat_secs, beats_per_bar, cadence, kit);
                continue; // not a pipeline command
            }
            if let Some(p) = self.pipeline.as_mut() {
                p.apply(cmd);
            }
        }
```

After the pipeline render / silence-fill block (the `match self.pipeline.as_mut() { Some(p) => {...} None => out.fill(0.0) }`), and before returning, mix the metronome over `out` and push its beats as engine events:

```rust
        // Metronome runs regardless of song/pipeline; mix it over whatever the
        // pipeline produced (audio or the silence fill above).
        self.metro_beats.clear();
        self.metronome.render(out, self.volume, &mut self.metro_beats);
        for b in self.metro_beats.drain(..) {
            let _ = self.evt_tx.push(EngineEvent::MetronomeBeat {
                beat: b.beat,
                of: b.of,
                sounded: b.sounded,
            });
        }
```

IMPORTANT: place this AFTER the pipeline's own event pushes (so ordering is sane) but it must run on EVERY `fill`, including the `None` (no pipeline) branch. If the current code `return`s early inside the match, refactor so the metronome mix runs unconditionally at the end of `fill`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p engine` (the new render_core test + full suite)
Expected: all pass.

- [ ] **Step 6: Clippy**

Run: `cargo clippy -p engine -- -D warnings`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add crates/engine/src/pipeline.rs crates/engine/src/render_core.rs
git commit -m "feat(metronome): SetMetronome command, MetronomeBeat event, render-core mix"
```

---

## Task 4: Server — AudioControl.set_metronome

**Files:**
- Modify: `crates/server/src/control.rs`

- [ ] **Step 1: Write the failing test**

Add to `crates/server/src/control.rs` (extend the test module added by the click-track feature, or add a new one):

```rust
#[cfg(test)]
mod metronome_tests {
    use super::*;
    use engine::metronome::{Cadence, Kit};
    use engine::pipeline::EngineCmd;

    #[test]
    fn mock_records_set_metronome() {
        let mut m = MockEngine::default();
        m.set_metronome(EngineCmd::SetMetronome {
            running: true,
            beat_secs: 0.5,
            beats_per_bar: 4,
            cadence: Cadence::EveryBeat,
            kit: Kit::Click,
        });
        assert!(matches!(m.sent.last(), Some(EngineCmd::SetMetronome { running: true, .. })));
    }
}
```

(The metronome command is a plain `EngineCmd`, so the existing `MockEngine.sent` vec records it — no new mock field needed. `set_metronome` just forwards a command.)

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p server metronome_tests`
Expected: FAIL — `set_metronome` not on the trait.

- [ ] **Step 3: Add the trait method + impls**

In `crates/server/src/control.rs`, add to the `AudioControl` trait:

```rust
    fn set_metronome(&mut self, cmd: EngineCmd);
```

`impl AudioControl for engine::Engine`:

```rust
    fn set_metronome(&mut self, cmd: EngineCmd) {
        engine::Engine::send(self, cmd);
    }
```

`impl AudioControl for MockEngine`:

```rust
    fn set_metronome(&mut self, cmd: EngineCmd) {
        self.sent.push(cmd);
    }
```

`impl AudioControl for Arc<Mutex<MockEngine>>`:

```rust
    fn set_metronome(&mut self, cmd: EngineCmd) {
        self.lock().unwrap().set_metronome(cmd);
    }
```

(Using a dedicated trait method rather than the generic `send` keeps the App call site self-documenting and consistent with `set_click_schedule`. `EngineCmd` is already imported in this file.)

- [ ] **Step 4: Run the test**

Run: `cargo test -p server metronome_tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/server/src/control.rs
git commit -m "feat(metronome): AudioControl.set_metronome"
```

---

## Task 5: Server — command, push, persistence, broadcast

**Files:**
- Modify: `crates/server/src/app.rs`
- Test: `crates/server/tests/app_metronome.rs` (new)

- [ ] **Step 1: Add `push_metronome`, the command, dispatch, and the event broadcast**

In `crates/server/src/app.rs`:

Add imports near the engine imports: `use engine::metronome::{Cadence, Kit};`

Add `push_metronome` near `push_count_in`:

```rust
    /// Send the persisted metronome config to the engine. `running` is carried
    /// separately (transient) so launch never auto-starts the click.
    fn push_metronome(&mut self, running: bool) {
        let cfg = self
            .store
            .get_setting("metronome")
            .ok()
            .flatten()
            .unwrap_or(Value::Null);
        let bpm = cfg.get("bpm").and_then(|v| v.as_f64()).unwrap_or(120.0).clamp(30.0, 300.0);
        let beats_per_bar = cfg.get("beats_per_bar").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
        let cadence = match cfg.get("cadence").and_then(|v| v.as_str()) {
            Some("bar") => Cadence::EveryBar,
            Some("half") => Cadence::HalfBar,
            _ => Cadence::EveryBeat,
        };
        let kit = match cfg.get("kit").and_then(|v| v.as_str()) {
            Some("kick_snare") => Kit::KickSnare,
            Some("cowbell") => Kit::Cowbell,
            _ => Kit::Click,
        };
        self.audio.set_metronome(EngineCmd::SetMetronome {
            running,
            beat_secs: 60.0 / bpm,
            beats_per_bar: beats_per_bar.max(1),
            cadence,
            kit,
        });
    }
```

Add the command handler:

```rust
    fn metronome_set(&mut self, p: Value) -> Result<Value, String> {
        #[derive(Deserialize)]
        struct P {
            running: bool,
            bpm: f64,
            beats_per_bar: u32,
            cadence: String,
            kit: String,
        }
        let p: P = from_params(p)?;
        // Persist everything EXCEPT running (transient).
        self.store
            .set_setting(
                "metronome",
                &json!({
                    "bpm": p.bpm.clamp(30.0, 300.0),
                    "beats_per_bar": p.beats_per_bar.max(1),
                    "cadence": p.cadence,
                    "kit": p.kit,
                }),
            )
            .err_str()?;
        self.push_metronome(p.running);
        Ok(Value::Null)
    }
```

Register in `dispatch_inner` (near `"countin.set"`):

```rust
            "metronome.set" => self.metronome_set(p),
```

In the engine-event drain loop (the `for ev in self.audio.poll_events()` match, ~line 867), add an arm:

```rust
                EngineEvent::MetronomeBeat { beat, of, sounded } => {
                    events.push(Event {
                        event: "metronome_beat".into(),
                        data: json!({ "beat": beat, "of": of, "sounded": sounded }),
                    });
                }
```

- [ ] **Step 2: Integration test**

Create `crates/server/tests/app_metronome.rs`, mirroring the setup of `crates/server/tests/app_section_click.rs` (build an `App` over `Arc<Mutex<MockEngine>>`, keep a mock clone). It does NOT need a song. Assert:

```rust
// (mirror the App + MockEngine construction + `req` helper from app_section_click.rs)

// metronome.set with running:true forwards a SetMetronome to the engine and
// persists everything but `running`.
req(&mut app, "metronome.set", json!({
    "running": true, "bpm": 100.0, "beats_per_bar": 3, "cadence": "bar", "kit": "kick_snare"
}));
let sent = mock.lock().unwrap().sent.clone();
assert!(sent.iter().any(|c| matches!(c,
    engine::pipeline::EngineCmd::SetMetronome { running: true, beats_per_bar: 3, .. })));

// the persisted setting omits `running`
let setting = req(&mut app, "settings.get_all", json!({}));
let metro = &setting["metronome"];
assert_eq!(metro["bpm"], 100.0);
assert!(metro.get("running").is_none(), "running is transient, not persisted");
```

If `settings.get_all` returns under a different shape, adapt; the key assertions are (a) a `SetMetronome{running:true,...}` reached the mock and (b) the persisted `metronome` object has no `running` key. Use the real `req`/fixture helpers from the sibling test file.

- [ ] **Step 3: Run tests + clippy + fmt**

Run: `cargo test -p server` and `cargo clippy -p server -- -D warnings` and `cargo fmt -p server -- --check` (run `cargo fmt` first).
Expected: all pass/clean.

- [ ] **Step 4: Commit**

```bash
git add crates/server/src/app.rs crates/server/tests/app_metronome.rs
git commit -m "feat(metronome): metronome.set command, push, persistence, beat broadcast"
```

---

## Task 6: Frontend — pure metronome logic

**Files:**
- Create: `apps/desktop/src/lib/metronome.ts`
- Create: `apps/desktop/src/lib/metronome.test.ts`

- [ ] **Step 1: Write the failing tests**

Create `apps/desktop/src/lib/metronome.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { clampBpm, tapTempo, type TapState } from "./metronome";

describe("clampBpm", () => {
  it("clamps to 30..300 and rounds", () => {
    expect(clampBpm(12)).toBe(30);
    expect(clampBpm(999)).toBe(300);
    expect(clampBpm(120.4)).toBe(120);
  });
});

describe("tapTempo", () => {
  it("returns no bpm on the first tap", () => {
    const s: TapState = { taps: [] };
    const r = tapTempo(s, 1000);
    expect(r.bpm).toBeNull();
    expect(r.state.taps).toEqual([1000]);
  });

  it("computes bpm from steady 500ms taps (120 bpm)", () => {
    let s: TapState = { taps: [] };
    let bpm: number | null = null;
    for (const t of [0, 500, 1000, 1500]) {
      const r = tapTempo(s, t);
      s = r.state;
      bpm = r.bpm;
    }
    expect(bpm).toBe(120);
  });

  it("resets the window after a long gap", () => {
    let s: TapState = { taps: [0, 500, 1000] };
    const r = tapTempo(s, 1000 + 5000); // 5s later → fresh window
    expect(r.bpm).toBeNull();
    expect(r.state.taps).toEqual([6000]);
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd apps/desktop && pnpm vitest run lib/metronome.test.ts`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement**

Create `apps/desktop/src/lib/metronome.ts`:

```ts
/** Clamp a BPM to the supported range and round to an integer. */
export function clampBpm(bpm: number): number {
  return Math.max(30, Math.min(300, Math.round(bpm)));
}

export interface TapState {
  /** Tap timestamps (ms), oldest→newest, within the current window. */
  taps: number[];
}

const TAP_GAP_MS = 2000; // a gap longer than this starts a fresh tap window
const TAP_WINDOW = 4; // average over at most this many taps

/** Fold a tap at time `now` (ms) into the state, returning a BPM when derivable.
 *  Resets the window if the gap since the last tap exceeds TAP_GAP_MS. */
export function tapTempo(state: TapState, now: number): { state: TapState; bpm: number | null } {
  const last = state.taps[state.taps.length - 1];
  const taps =
    last != null && now - last > TAP_GAP_MS ? [now] : [...state.taps, now].slice(-TAP_WINDOW);
  if (taps.length < 2) {
    return { state: { taps }, bpm: null };
  }
  const span = taps[taps.length - 1] - taps[0];
  const avgInterval = span / (taps.length - 1);
  return { state: { taps }, bpm: clampBpm(60000 / avgInterval) };
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd apps/desktop && pnpm vitest run lib/metronome.test.ts`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add apps/desktop/src/lib/metronome.ts apps/desktop/src/lib/metronome.test.ts
git commit -m "feat(metronome): tap-tempo + bpm clamp pure logic"
```

---

## Task 7: Frontend — store wiring

**Files:**
- Modify: `apps/desktop/src/lib/stores.ts`

- [ ] **Step 1: Add the store, setting key, and types**

In `apps/desktop/src/lib/stores.ts`, near the `countIn` store, add:

```ts
export const METRONOME = "metronome";

export type Cadence = "beat" | "half" | "bar";
export type Kit = "click" | "kick_snare" | "cowbell";

export interface MetronomeState {
  running: boolean;
  bpm: number;
  beatsPerBar: number;
  cadence: Cadence;
  kit: Kit;
}

export const metronome = writable<MetronomeState>({
  running: false,
  bpm: 120,
  beatsPerBar: 4,
  cadence: "beat",
  kit: "click",
});

/** Live beat from the engine: 1-based beat in the bar, or null when stopped. */
export const metronomeBeat = writable<{ beat: number; of: number; sounded: boolean } | null>(null);
```

Add a local tap-state holder near the other module-local lets (e.g. `volumeSaveTimer`):

```ts
import { tapTempo, clampBpm, type TapState } from "./metronome";
let tapState: TapState = { taps: [] };
```

- [ ] **Step 2: Hydrate in loadSettings**

In `loadSettings` (after the `SECTION_CLICK` block), add:

```ts
    const mt = all[METRONOME];
    if (mt && typeof mt === "object") {
      const m = mt as Partial<MetronomeState> & { beats_per_bar?: number };
      metronome.update((s) => ({
        ...s,
        bpm: typeof m.bpm === "number" ? clampBpm(m.bpm) : s.bpm,
        beatsPerBar: typeof m.beats_per_bar === "number" ? m.beats_per_bar : s.beatsPerBar,
        cadence: (m.cadence as Cadence) ?? s.cadence,
        kit: (m.kit as Kit) ?? s.kit,
        running: false, // never auto-start
      }));
    }
```

- [ ] **Step 3: Add actions**

In the `actions` object:

```ts
  /** Push the current metronome config to the server (persists all but running). */
  async pushMetronome(): Promise<void> {
    const m = get(metronome);
    await cmd("metronome.set", {
      running: m.running,
      bpm: m.bpm,
      beats_per_bar: m.beatsPerBar,
      cadence: m.cadence,
      kit: m.kit,
    });
  },

  /** Patch the metronome config and push. */
  async setMetronome(patch: Partial<MetronomeState>): Promise<void> {
    metronome.update((s) => ({ ...s, ...patch }));
    await this.pushMetronome();
  },

  /** Toggle running. */
  async toggleMetronome(): Promise<void> {
    const running = !get(metronome).running;
    if (!running) metronomeBeat.set(null);
    await this.setMetronome({ running });
  },

  /** Register a tap; when a BPM is derivable, apply it. */
  async tapTempo(now: number): Promise<void> {
    const r = tapTempo(tapState, now);
    tapState = r.state;
    if (r.bpm != null) await this.setMetronome({ bpm: r.bpm });
  },

  /** Seed BPM from the open song's analyzed tempo, if any. */
  async syncMetronomeToSong(): Promise<void> {
    const bpm = get(openSong)?.analysis?.bpm;
    if (bpm != null) await this.setMetronome({ bpm: clampBpm(bpm) });
  },
```

- [ ] **Step 4: Handle the beat event**

Find the event subscription (`onEvent`/`initEvents` in stores.ts where `"position"`, `"loop_wrapped"` etc. are handled) and add a case:

```ts
        } else if (e.event === "metronome_beat") {
          const d = e.data as { beat: number; of: number; sounded: boolean };
          metronomeBeat.set(d);
```

Match the exact shape of the existing event dispatch (it may be a `switch` or `if/else` on `e.event`); follow the surrounding style.

- [ ] **Step 5: Typecheck + test**

Run: `cd apps/desktop && pnpm svelte-check` (0 new errors) and `pnpm vitest run` (all pass).

- [ ] **Step 6: Commit**

```bash
git add apps/desktop/src/lib/stores.ts
git commit -m "feat(metronome): frontend store wiring + beat event"
```

---

## Task 8: Frontend — the Metronome box

**Files:**
- Create: `apps/desktop/src/components/MetronomeBox.svelte`
- Modify: `apps/desktop/src/App.svelte`

- [ ] **Step 1: Build the box**

Create `apps/desktop/src/components/MetronomeBox.svelte`. Read `apps/desktop/src/components/ClickTrack.svelte` and `Tuner.svelte` first for the `Box`/`Group`/`NumberField`/`Button` idioms and the theme-accent on-state convention. Implement:

```svelte
<script lang="ts">
  import Box from "../lib/ui/Box.svelte";
  import Group from "../lib/ui/Group.svelte";
  import NumberField from "../lib/ui/NumberField.svelte";
  import { actions, metronome, metronomeBeat, openSong, type Cadence, type Kit } from "../lib/stores";

  const KITS: { id: Kit; label: string }[] = [
    { id: "click", label: "click" },
    { id: "kick_snare", label: "kick/snare" },
    { id: "cowbell", label: "cowbell" },
  ];
  const SIGS = [2, 3, 4, 5, 6, 7];
  const CADENCES: { id: Cadence; label: string }[] = [
    { id: "beat", label: "beat" },
    { id: "half", label: "½ bar" },
    { id: "bar", label: "bar" },
  ];

  let canSync = $derived($openSong?.analysis?.bpm != null);
  let beat = $derived($metronomeBeat);
  // dots for the current bar; lit = current beat; accent = beat 1
  let dots = $derived(Array.from({ length: $metronome.beatsPerBar }, (_, i) => i + 1));
</script>

<Box label="metronome">
  <div class="bar">
    {#each dots as d}
      <span class="dot" class:accent={d === 1} class:lit={beat?.beat === d && $metronome.running}></span>
    {/each}
  </div>

  <Group label="tempo">
    <button class="pill primary" class:on={$metronome.running} onclick={() => actions.toggleMetronome()}>
      {$metronome.running ? "stop" : "start"}
    </button>
    <NumberField
      value={$metronome.bpm}
      min={30}
      max={300}
      onchange={(v) => actions.setMetronome({ bpm: v })}
    />
    <span class="unit">bpm</span>
    <button class="pill" onclick={() => actions.tapTempo(performance.now())} title="tap to set tempo">tap</button>
    {#if canSync}
      <button class="pill" onclick={() => actions.syncMetronomeToSong()} title="use the song's tempo">sync</button>
    {/if}
  </Group>

  <Group label="feel">
    <select value={$metronome.beatsPerBar} onchange={(e) => actions.setMetronome({ beatsPerBar: Number(e.currentTarget.value) })}>
      {#each SIGS as n}<option value={n}>{n}/4</option>{/each}
    </select>
    {#each CADENCES as c}
      <button class="pill" class:on={$metronome.cadence === c.id} onclick={() => actions.setMetronome({ cadence: c.id })}>{c.label}</button>
    {/each}
  </Group>

  <Group label="sound">
    {#each KITS as k}
      <button class="pill" class:on={$metronome.kit === k.id} onclick={() => actions.setMetronome({ kit: k.id })}>{k.label}</button>
    {/each}
  </Group>
</Box>

<style>
  .bar { display: flex; gap: 6px; padding: 4px 0 8px; }
  .dot { width: 10px; height: 10px; border-radius: 50%; background: var(--line); transition: background 60ms; }
  .dot.accent { width: 12px; height: 12px; }
  .dot.lit { background: var(--accent); }
  .dot.accent.lit { background: var(--accent); box-shadow: 0 0 6px var(--accent-dim); }
  .pill.on { color: var(--accent); border-color: var(--accent); }
  .pill.primary { font-weight: 600; }
  .unit { color: var(--muted); }
</style>
```

ADAPT to the real `NumberField`/`Button`/`Box`/`Group`/`select` conventions in this codebase — read those components and match their prop names and the existing `.pill`/control styling (copy the pill CSS from `ClickTrack.svelte` rather than hand-rolling if it differs). Use `--accent`/`--accent-dim` for on/lit states (no hardcoded colors). If the codebase has a styled select/segmented-control widget, use it instead of a bare `<select>`.

- [ ] **Step 2: Mount it unconditionally on the stage**

In `apps/desktop/src/App.svelte`, import `MetronomeBox` and place `<MetronomeBox />` in the stage's box row, but OUTSIDE the `{#if $openSong}` gate that wraps the other boxes — the metronome must show with no song. Read the stage markup (the `.boxes` container, ~lines 119-132) and place `<MetronomeBox />` so it renders whether or not a song is open (e.g. as the first box in the row, before the `{#if $openSong}` block, or with its own always-true placement). Confirm the stage container itself renders without a song (the `main.stage` is always present; only its inner boxes gate on `$openSong`).

- [ ] **Step 3: Typecheck + test**

Run: `cd apps/desktop && pnpm svelte-check` (0 new errors) and `pnpm vitest run` (all pass).

- [ ] **Step 4: Commit**

```bash
git add apps/desktop/src/components/MetronomeBox.svelte apps/desktop/src/App.svelte
git commit -m "feat(metronome): Metronome control box + always-on stage mount"
```

---

## Task 9: Full verification

- [ ] **Step 1: Whole suite**

Run: `just test`
Expected: `cargo test --workspace` + `pnpm vitest run` all pass.

- [ ] **Step 2: Lint gate**

Run: `just lint`
Expected: clippy `-D warnings` clean, `cargo fmt --check` clean (run `cargo fmt` if needed and commit), svelte-check 0 errors, theme guardrail ok.

- [ ] **Step 3: Build + manual smoke (human checklist)**

Run: `just build` then `just run`. The WebKitGTK webview can't be driven from here, so verify by hand:
- With **no song loaded**, the Metronome box is visible on the stage; **start** produces a click; the bar dots light in time; **stop** silences it.
- BPM field changes tempo live; **tap** a few times sets BPM by feel; time-signature changes the dot count and where the accent falls.
- **Cadence**: bar = downbeat only; ½ bar = two clicks; beat = all.
- **Sound kits**: click / kick-snare / cowbell each sound distinct; the downbeat is accented.
- Open a song → **sync** appears and seeds BPM from the analyzed tempo; the metronome can run over song playback.
- Relaunch → the box restored BPM/sig/cadence/kit but is **stopped** (running not persisted).

- [ ] **Step 4: Commit any fmt/lint fixups**

```bash
git add -- crates apps
git commit -m "chore(metronome): lint/test fixups"
```

---

## Self-review notes

- **Spec coverage:** placement as always-on stage box (Task 8); BPM + start/stop (Tasks 7-8); tap tempo (Tasks 6-8); sync-to-song (Tasks 7-8); time signature (Tasks 2,7,8); downbeat accent (Tasks 2,8); cadence beat/half/bar (Tasks 2,5,7,8); sound kits click/kick-snare/cowbell synthesized (Tasks 1,5,7,8); visual bar indicator via beat events (Tasks 2,3,5,7,8); engine generator in render core, runs with no song (Tasks 2,3); SetMetronome Copy command + MetronomeBeat event (Task 3); persistence minus running (Tasks 5,7); all spec test bullets mapped (Tasks 1,2,3,5,6). Every spec section maps to a task.
- **Deferred (per spec):** subdivisions, compound-meter sub-accents, independent accent/normal pickers, polyrhythm, programmable accent patterns — none implemented.
- **Cross-cutting note for the implementer:** Task 3 restructures `RenderCore::fill` so the metronome mix runs unconditionally (including the no-pipeline branch) and `SetMetronome` is intercepted before the pipeline apply — verify there's no early `return` that would skip the metronome mix.
