# earworm v1 — Plan 2: `engine` crate

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the audio engine: decode → sample-accurate crossfaded looping → Rubber Band R3 time-stretch/pitch → bass-focus filter → PipeWire output, with a command/event interface safe for a real-time audio thread.

**Architecture:** The DSP core (`Looper` → `Stretcher` → `Biquad` → gain) is a pure `Pipeline` with `render(&mut [f32])` — fully testable by rendering to buffers. PipeWire is a thin shell that drains a lock-free command ring, calls `render`, and pushes events out a lock-free event ring. Song buffers swap in via `ArcSwapOption` (no allocation in the audio callback). All engine positions are **source frames** (48 kHz stereo interleaved f32); seconds only at the API boundary.

**Tech Stack:** symphonia (decode), rubato (offline resample to 48k), Rubber Band 4.0 C API via hand-written FFI + pkg-config (system lib, R3 engine), rtrb (SPSC rings), arc-swap, pipewire crate, blake3 (file hash), hound (dev: WAV fixtures).

**Key constants:** sample rate 48000, channels 2, loop crossfade 480 frames (10 ms, equal-power), gain ramp 240 frames (5 ms), rate range 0.25–2.0, pitch ±12 semitones + cents, bass-focus low-pass 400 Hz Q 0.707.

**Spec:** `docs/superpowers/specs/2026-06-12-earworm-design.md`

---

### Task 1: Engine crate deps + Rubber Band FFI binding

**Files:**
- Modify: `crates/engine/Cargo.toml`
- Create: `crates/engine/build.rs`, `crates/engine/src/ffi.rs`
- Modify: `crates/engine/src/lib.rs`

- [ ] **Step 1: Dependencies and build script**

`crates/engine/Cargo.toml`:
```toml
[package]
name = "engine"
version = "0.1.0"
edition.workspace = true

[dependencies]
thiserror.workspace = true
symphonia = { version = "0.5", features = ["mp3", "flac", "vorbis", "wav", "pcm", "isomp4", "aac"] }
rubato = "0.16"
rtrb = "0.3"
arc-swap = "1"
pipewire = "0.9"
libspa = "0.9"
blake3 = "1"
serde.workspace = true
serde_json.workspace = true
dirs = "6"

[dev-dependencies]
hound = "3"

[build-dependencies]
pkg-config = "0.3"
```

If `pipewire`/`libspa` `0.9` doesn't resolve, use the latest published version and keep both at the same version. Same rule for `dirs`/`hound`.

`crates/engine/build.rs`:
```rust
fn main() {
    pkg_config::Config::new()
        .atleast_version("3.0")
        .probe("rubberband")
        .expect("librubberband not found (pacman -S rubberband)");
}
```

- [ ] **Step 2: Write the FFI module with a link-proof test**

`crates/engine/src/ffi.rs` — hand-written bindings for the small C API in `/usr/include/rubberband/rubberband-c.h`. **Verify every signature and constant against that header before finishing this task** (read the header; do not trust memory):

```rust
#![allow(non_camel_case_types, dead_code)]
use std::os::raw::{c_int, c_uint};

pub enum RubberBandState_ {}
pub type RubberBandState = *mut RubberBandState_;
pub type RubberBandOptions = c_int;

// Verify these against rubberband-c.h:
pub const OPTION_PROCESS_REAL_TIME: RubberBandOptions = 0x00000001;
pub const OPTION_ENGINE_FINER: RubberBandOptions = 0x20000000; // R3
pub const OPTION_PITCH_HIGH_CONSISTENCY: RubberBandOptions = 0x06000000;

extern "C" {
    pub fn rubberband_new(
        sample_rate: c_uint,
        channels: c_uint,
        options: RubberBandOptions,
        initial_time_ratio: f64,
        initial_pitch_scale: f64,
    ) -> RubberBandState;
    pub fn rubberband_delete(state: RubberBandState);
    pub fn rubberband_set_time_ratio(state: RubberBandState, ratio: f64);
    pub fn rubberband_set_pitch_scale(state: RubberBandState, scale: f64);
    pub fn rubberband_get_samples_required(state: RubberBandState) -> c_uint;
    pub fn rubberband_process(
        state: RubberBandState,
        input: *const *const f32,
        samples: c_uint,
        final_block: c_int,
    );
    pub fn rubberband_available(state: RubberBandState) -> c_int;
    pub fn rubberband_retrieve(
        state: RubberBandState,
        output: *const *mut f32,
        samples: c_uint,
    ) -> c_uint;
    pub fn rubberband_reset(state: RubberBandState);
    pub fn rubberband_get_start_delay(state: RubberBandState) -> c_uint;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_create_and_destroy_r3_realtime_stretcher() {
        unsafe {
            let s = rubberband_new(
                48000,
                2,
                OPTION_PROCESS_REAL_TIME | OPTION_ENGINE_FINER | OPTION_PITCH_HIGH_CONSISTENCY,
                1.0,
                1.0,
            );
            assert!(!s.is_null());
            assert!(rubberband_get_samples_required(s) > 0);
            rubberband_delete(s);
        }
    }
}
```

`lib.rs`: `pub mod ffi;` (keep the existing doc comment).

- [ ] **Step 3: Run test**

Run: `cargo test -p engine`
Expected: `can_create_and_destroy_r3_realtime_stretcher` PASS (proves linkage). If constants disagree with the header, fix the constants, not the header.

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(engine): rubberband 4 FFI binding (R3 realtime), pkg-config link"
```

---

### Task 2: Decode + resample to canonical `SongBuffer`

**Files:**
- Create: `crates/engine/src/buffer.rs`, `crates/engine/src/decode.rs`, `crates/engine/src/error.rs`
- Modify: `crates/engine/src/lib.rs`
- Test: `crates/engine/tests/decode.rs`

- [ ] **Step 1: Types**

`crates/engine/src/buffer.rs`:
```rust
pub const SAMPLE_RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

/// Whole song in memory: interleaved stereo f32 at 48 kHz.
#[derive(Debug, Clone, PartialEq)]
pub struct SongBuffer {
    pub data: Vec<f32>,
}

impl SongBuffer {
    pub fn frames(&self) -> usize {
        self.data.len() / CHANNELS
    }
    pub fn duration_secs(&self) -> f64 {
        self.frames() as f64 / SAMPLE_RATE as f64
    }
}
```

`crates/engine/src/error.rs`: thiserror enum `Error { Decode(String), Io(#[from] std::io::Error), Unsupported(String) }`, `pub type Result<T>`.

- [ ] **Step 2: Write failing integration test**

`crates/engine/tests/decode.rs`:
```rust
use engine::buffer::{CHANNELS, SAMPLE_RATE};
use engine::decode::decode_file;

/// 1 s of 440 Hz mono sine at 44.1 kHz — exercises resample AND mono→stereo.
fn write_test_wav(path: &std::path::Path) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 44_100,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..44_100 {
        let t = i as f32 / 44_100.0;
        let v = (t * 440.0 * std::f32::consts::TAU).sin() * 0.5;
        w.write_sample((v * i16::MAX as f32) as i16).unwrap();
    }
    w.finalize().unwrap();
}

#[test]
fn decodes_resamples_and_upmixes() {
    let dir = std::env::temp_dir().join("earworm-decode-test");
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("sine.wav");
    write_test_wav(&wav);

    let buf = decode_file(&wav).unwrap();
    // ~1 s at 48 kHz (resampler may trim edges slightly)
    let frames = buf.frames();
    assert!((47_000..=49_000).contains(&frames), "frames = {frames}");
    assert_eq!(buf.data.len() % CHANNELS, 0);
    // stereo channels identical after mono upmix
    assert_eq!(buf.data[1000 * 2], buf.data[1000 * 2 + 1]);
    // energy preserved: RMS of a 0.5-amplitude sine ≈ 0.35
    let rms = (buf.data.iter().map(|s| (*s as f64).powi(2)).sum::<f64>()
        / buf.data.len() as f64)
        .sqrt();
    assert!((0.30..=0.40).contains(&rms), "rms = {rms}");
    let _ = SAMPLE_RATE; // canonical-rate contract referenced above
}

#[test]
fn missing_file_is_an_error() {
    assert!(decode_file(std::path::Path::new("/nope/missing.flac")).is_err());
}
```

- [ ] **Step 3: Run, verify failure; implement `decode.rs`**

`pub fn decode_file(path: &Path) -> Result<SongBuffer>`:
1. symphonia: probe format from file extension hint, default decode loop collecting planar f32 (use `SampleBuffer<f32>` per packet, push interleaved).
2. Downmix/upmix to stereo: mono → duplicate; >2ch → average extras into L/R (`L = mean(even chans)`, simple).
3. If source rate ≠ 48000: resample per-channel with `rubato::SincFixedIn::<f64>` (chunk 1024, `SincInterpolationParameters` defaults from rubato docs), then re-interleave.
4. Also expose `pub fn file_hash(path: &Path) -> Result<String>` using blake3 (streaming, 1 MiB chunks, hex string).

Add `pub mod buffer; pub mod decode; pub mod error;` to lib.rs.

- [ ] **Step 4: Run tests, verify pass**

Run: `cargo test -p engine --test decode`
Expected: 2 PASS.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(engine): symphonia decode + rubato resample to canonical 48k stereo"
```

---

### Task 3: Waveform peaks + cache

**Files:**
- Create: `crates/engine/src/peaks.rs`
- Modify: `crates/engine/src/lib.rs` (add `pub mod peaks;`)

- [ ] **Step 1: Write failing tests (inline)**

`crates/engine/src/peaks.rs`:
```rust
use crate::buffer::{SongBuffer, CHANNELS};
use serde::{Deserialize, Serialize};

pub const FRAMES_PER_BUCKET: usize = 1024;

/// Per-bucket (min, max) over both channels — what the waveform draws.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Peaks {
    pub frames_per_bucket: usize,
    pub buckets: Vec<(f32, f32)>,
}

pub fn compute_peaks(buf: &SongBuffer) -> Peaks {
    todo!()
}

/// Cache under ~/.cache/earworm/peaks/<file_hash>.json; load if present.
pub fn load_or_compute(buf: &SongBuffer, file_hash: &str) -> std::io::Result<Peaks> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaks_capture_min_and_max_per_bucket() {
        // bucket 0: constant 0.5; bucket 1: constant -0.25
        let mut data = vec![0.5f32; FRAMES_PER_BUCKET * CHANNELS];
        data.extend(vec![-0.25f32; FRAMES_PER_BUCKET * CHANNELS]);
        let p = compute_peaks(&SongBuffer { data });
        assert_eq!(p.buckets.len(), 2);
        assert_eq!(p.buckets[0], (0.5, 0.5));
        assert_eq!(p.buckets[1], (-0.25, -0.25));
    }

    #[test]
    fn partial_final_bucket_included() {
        let data = vec![0.1f32; (FRAMES_PER_BUCKET + 10) * CHANNELS];
        let p = compute_peaks(&SongBuffer { data });
        assert_eq!(p.buckets.len(), 2);
    }

    #[test]
    fn cache_roundtrip() {
        let data = vec![0.3f32; FRAMES_PER_BUCKET * CHANNELS];
        let buf = SongBuffer { data };
        let hash = format!("test-{}", std::process::id());
        let first = load_or_compute(&buf, &hash).unwrap();
        // second call must hit the cache file (delete buf data influence: pass empty buffer)
        let cached = load_or_compute(&SongBuffer { data: vec![] }, &hash).unwrap();
        assert_eq!(first, cached);
        // cleanup
        let dir = dirs::cache_dir().unwrap().join("earworm/peaks");
        let _ = std::fs::remove_file(dir.join(format!("{hash}.json")));
    }
}
```

- [ ] **Step 2: Run (fail), implement**

`compute_peaks`: chunk `buf.data` by `FRAMES_PER_BUCKET * CHANNELS`, fold min/max over all samples in the chunk. `load_or_compute`: path = `dirs::cache_dir().join("earworm/peaks/<hash>.json")`; read+parse if exists (parse failure → recompute), else compute, `create_dir_all`, write JSON.

- [ ] **Step 3: Run tests, verify pass; commit**

Run: `cargo test -p engine peaks` — 3 PASS.

```bash
git add -A && git commit -m "feat(engine): waveform peaks with disk cache"
```

---

### Task 4: Sample-accurate crossfaded looper

**Files:**
- Create: `crates/engine/src/looper.rs`
- Modify: `crates/engine/src/lib.rs` (add `pub mod looper;`)

- [ ] **Step 1: Write failing tests**

`crates/engine/src/looper.rs`:
```rust
use crate::buffer::{SongBuffer, CHANNELS};
use std::sync::Arc;

pub const XFADE_FRAMES: usize = 480; // 10 ms

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ReadInfo {
    pub frames: usize,
    pub wrapped: bool,
    pub finished: bool,
}

/// Reads source frames; when a loop is set, wraps end→start with an
/// equal-power crossfade. The crossfade blends the loop tail
/// [end-x, end) with the head [start, start+x); after the blend the
/// position continues from start+x, so the loop period is exactly
/// end-start frames.
pub struct Looper {
    buf: Arc<SongBuffer>,
    pos: usize, // current source frame
    region: Option<(usize, usize)>,
}

impl Looper {
    pub fn new(buf: Arc<SongBuffer>) -> Self {
        Self { buf, pos: 0, region: None }
    }

    pub fn pos_frames(&self) -> usize {
        self.pos
    }

    pub fn seek(&mut self, frame: usize) {
        self.pos = frame.min(self.buf.frames());
    }

    /// Set loop [start, end) in frames; jumps into the region if outside.
    pub fn set_region(&mut self, start: usize, end: usize) {
        todo!()
    }

    pub fn clear_region(&mut self) {
        self.region = None;
    }

    /// Fill `out` (len = frames*CHANNELS). Returns ReadInfo.
    pub fn read(&mut self, out: &mut [f32]) -> ReadInfo {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Buffer where frame i has value i (both channels) — positions are
    /// directly observable in the output.
    fn ramp(frames: usize) -> Arc<SongBuffer> {
        let mut data = Vec::with_capacity(frames * CHANNELS);
        for i in 0..frames {
            data.push(i as f32);
            data.push(i as f32);
        }
        Arc::new(SongBuffer { data })
    }

    fn read_frames(l: &mut Looper, n: usize) -> (Vec<f32>, usize) {
        let mut out = vec![0.0f32; n * CHANNELS];
        let mut wraps = 0;
        let mut filled = 0;
        while filled < n {
            let chunk = (n - filled).min(256);
            let info = l.read(&mut out[filled * CHANNELS..(filled + chunk) * CHANNELS]);
            if info.wrapped {
                wraps += 1;
            }
            if info.finished {
                break;
            }
            filled += info.frames;
        }
        (out.iter().step_by(CHANNELS).copied().collect(), wraps)
    }

    #[test]
    fn no_region_plays_through_and_finishes() {
        let mut l = Looper::new(ramp(1000));
        let mut out = vec![0.0f32; 600 * CHANNELS];
        let a = l.read(&mut out);
        assert_eq!(a, ReadInfo { frames: 600, wrapped: false, finished: false });
        assert_eq!(out[599 * 2], 599.0);
        let b = l.read(&mut out);
        assert_eq!(b.frames, 400);
        assert!(b.finished);
    }

    #[test]
    fn loop_period_is_exact() {
        let mut l = Looper::new(ramp(50_000));
        l.set_region(10_000, 20_000); // 10k period
        let (_, wraps) = read_frames(&mut l, 40_000);
        assert_eq!(wraps, 4);
    }

    #[test]
    fn crossfade_is_continuous_and_lands_at_head_plus_xfade() {
        let mut l = Looper::new(ramp(50_000));
        l.set_region(10_000, 20_000);
        // read up to 2 frames past the wrap point: period = 10_000
        let (vals, _) = read_frames(&mut l, 10_001);
        // frame 0 of output = source 10_000
        assert_eq!(vals[0], 10_000.0);
        // last blended frame ends at head+xfade: source 10_000 + XFADE
        let landing = vals[10_000];
        assert!((landing - (10_000 + XFADE_FRAMES) as f32).abs() < 1.5, "landing = {landing}");
        // continuity: no sample-to-sample jump bigger than the blend slope bound.
        // ramp slope is 1/frame; blend moves value from ~19,520 to ~10,480
        // over 480 frames → max step ≈ (19520-10480)/480 + 1 ≈ 20.
        for w in vals.windows(2) {
            assert!((w[1] - w[0]).abs() <= 25.0, "discontinuity {} -> {}", w[0], w[1]);
        }
    }

    #[test]
    fn outside_region_jumps_to_start() {
        let mut l = Looper::new(ramp(50_000));
        l.seek(40_000);
        l.set_region(10_000, 20_000);
        let mut out = vec![0.0f32; 2];
        l.read(&mut out);
        assert_eq!(out[0], 10_000.0);
    }

    #[test]
    fn tiny_region_shrinks_crossfade_instead_of_breaking() {
        let mut l = Looper::new(ramp(50_000));
        l.set_region(100, 300); // 200-frame loop < 2*XFADE
        let (_, wraps) = read_frames(&mut l, 2_000);
        assert_eq!(wraps, 10); // period still exactly 200
    }
}
```

- [ ] **Step 2: Run (fail), implement**

Implementation core of `read` (per-frame; clarity over micro-optimization — at 48 kHz this is nowhere near a bottleneck):

```rust
    pub fn set_region(&mut self, start: usize, end: usize) {
        let end = end.min(self.buf.frames());
        let start = start.min(end);
        self.region = Some((start, end));
        if self.pos < start || self.pos >= end {
            self.pos = start;
        }
    }

    pub fn read(&mut self, out: &mut [f32]) -> ReadInfo {
        let total = self.buf.frames();
        let frames_req = out.len() / CHANNELS;
        let mut info = ReadInfo::default();
        for f in 0..frames_req {
            match self.region {
                None => {
                    if self.pos >= total {
                        info.finished = true;
                        break;
                    }
                    let i = self.pos * CHANNELS;
                    out[f * CHANNELS] = self.buf.data[i];
                    out[f * CHANNELS + 1] = self.buf.data[i + 1];
                    self.pos += 1;
                }
                Some((start, end)) => {
                    let len = end - start;
                    if len == 0 {
                        info.finished = true;
                        break;
                    }
                    let xfade = XFADE_FRAMES.min(len / 4);
                    let fade_start = end - xfade;
                    let i = self.pos * CHANNELS;
                    if self.pos >= fade_start {
                        // blend tail with head
                        let k = self.pos - fade_start;
                        let t = (k as f32 + 0.5) / xfade.max(1) as f32;
                        let theta = t * std::f32::consts::FRAC_PI_2;
                        let (g_out, g_in) = (theta.cos(), theta.sin());
                        let j = (start + k) * CHANNELS;
                        out[f * CHANNELS] =
                            self.buf.data[i] * g_out + self.buf.data[j] * g_in;
                        out[f * CHANNELS + 1] =
                            self.buf.data[i + 1] * g_out + self.buf.data[j + 1] * g_in;
                        self.pos += 1;
                        if self.pos >= end {
                            self.pos = start + xfade;
                            info.wrapped = true;
                        }
                    } else {
                        out[f * CHANNELS] = self.buf.data[i];
                        out[f * CHANNELS + 1] = self.buf.data[i + 1];
                        self.pos += 1;
                    }
                }
            }
            info.frames += 1;
        }
        info
    }
```

Note the period check: positions emitted per cycle are `start..fade_start` then `xfade` blended frames = `end - start` exactly.

- [ ] **Step 3: Run tests, verify pass; commit**

Run: `cargo test -p engine looper` — 5 PASS.

```bash
git add -A && git commit -m "feat(engine): sample-accurate looper with equal-power crossfade"
```

---

### Task 5: Safe stretcher wrapper (Rubber Band R3 realtime)

**Files:**
- Create: `crates/engine/src/stretch.rs`
- Modify: `crates/engine/src/lib.rs` (add `pub mod stretch;`)

- [ ] **Step 1: Write failing tests**

`crates/engine/src/stretch.rs`:
```rust
use crate::buffer::CHANNELS;
use crate::ffi;

/// Real-time R3 stretcher. `rate` is playback speed (RB time_ratio = 1/rate).
/// All buffers pre-allocated; `feed`/`pull` are allocation-free.
pub struct Stretcher {
    state: ffi::RubberBandState,
    // pre-allocated deinterleave/interleave scratch (BLOCK frames)
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
}

pub const BLOCK_FRAMES: usize = 1024;

unsafe impl Send for Stretcher {}

impl Stretcher {
    pub fn new() -> Self { todo!() }
    pub fn set_rate(&mut self, rate: f64) { todo!() }     // clamps 0.25..=2.0
    pub fn set_pitch_scale(&mut self, scale: f64) { todo!() } // 2^(semis/12) etc.
    /// Frames RB wants next (cap at BLOCK_FRAMES).
    pub fn frames_wanted(&self) -> usize { todo!() }
    /// Feed interleaved stereo (≤ BLOCK_FRAMES frames).
    pub fn feed(&mut self, interleaved: &[f32]) { todo!() }
    pub fn available(&self) -> usize { todo!() }
    /// Pull up to out.len()/2 frames, interleaved; returns frames written.
    pub fn pull(&mut self, out: &mut [f32]) -> usize { todo!() }
    pub fn reset(&mut self) { todo!() }
}

impl Drop for Stretcher {
    fn drop(&mut self) {
        unsafe { ffi::rubberband_delete(self.state) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frames: usize, freq: f32) -> Vec<f32> {
        let mut v = Vec::with_capacity(frames * CHANNELS);
        for i in 0..frames {
            let s = (i as f32 / 48_000.0 * freq * std::f32::consts::TAU).sin() * 0.5;
            v.push(s);
            v.push(s);
        }
        v
    }

    /// Push `input` through at `rate`, draining as we go.
    fn run(rate: f64, input: &[f32]) -> Vec<f32> {
        let mut st = Stretcher::new();
        st.set_rate(rate);
        let mut out = Vec::new();
        let mut fed = 0;
        let frames_in = input.len() / CHANNELS;
        let mut pull_buf = vec![0.0f32; BLOCK_FRAMES * CHANNELS];
        while fed < frames_in {
            let want = st.frames_wanted().max(1).min(frames_in - fed);
            st.feed(&input[fed * CHANNELS..(fed + want) * CHANNELS]);
            fed += want;
            while st.available() > 0 {
                let n = st.pull(&mut pull_buf);
                out.extend_from_slice(&pull_buf[..n * CHANNELS]);
                if n == 0 { break; }
            }
        }
        out
    }

    #[test]
    fn half_rate_roughly_doubles_output_length() {
        let input = sine(48_000, 440.0);
        let out = run(0.5, &input);
        let out_frames = out.len() / CHANNELS;
        // realtime mode holds back some latency; generous bounds
        assert!((80_000..=110_000).contains(&out_frames), "{out_frames}");
    }

    #[test]
    fn unity_rate_passes_roughly_same_length() {
        let input = sine(48_000, 440.0);
        let out = run(1.0, &input);
        let out_frames = out.len() / CHANNELS;
        assert!((40_000..=50_000).contains(&out_frames), "{out_frames}");
    }

    #[test]
    fn output_is_not_silence() {
        let input = sine(48_000, 440.0);
        let out = run(0.75, &input);
        let rms = (out.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / out.len() as f64).sqrt();
        assert!(rms > 0.1, "rms = {rms}");
    }
}
```

- [ ] **Step 2: Run (fail), implement**

- `new()`: `rubberband_new(48000, 2, REAL_TIME | ENGINE_FINER | PITCH_HIGH_CONSISTENCY, 1.0, 1.0)`; allocate the four scratch vecs at `BLOCK_FRAMES`.
- `set_rate`: `rubberband_set_time_ratio(state, 1.0 / rate.clamp(0.25, 2.0))`.
- `feed`: deinterleave into `in_l`/`in_r`, build `[in_l.as_ptr(), in_r.as_ptr()]`, call `rubberband_process(state, ptrs.as_ptr(), frames, 0)`.
- `pull`: `rubberband_retrieve` into `out_l`/`out_r` (≤ BLOCK_FRAMES and ≤ requested), interleave into `out`.
- `frames_wanted`: `rubberband_get_samples_required` capped at BLOCK_FRAMES.

- [ ] **Step 3: Run tests, verify pass; commit**

Run: `cargo test -p engine stretch` — 3 PASS.

```bash
git add -A && git commit -m "feat(engine): safe realtime R3 stretcher wrapper"
```

---

### Task 6: Bass-focus biquad filter

**Files:**
- Create: `crates/engine/src/filter.rs`
- Modify: `crates/engine/src/lib.rs` (add `pub mod filter;`)

- [ ] **Step 1: Write failing tests**

`crates/engine/src/filter.rs`:
```rust
/// RBJ cookbook low-pass biquad, per-channel state, Direct Form 1.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    b0: f32, b1: f32, b2: f32, a1: f32, a2: f32,
    x1: f32, x2: f32, y1: f32, y2: f32,
}

impl Biquad {
    pub fn lowpass(sample_rate: f32, fc: f32, q: f32) -> Self { todo!() }
    pub fn process(&mut self, x: f32) -> f32 { todo!() }
}

/// Stereo bass-focus low-pass (400 Hz) applied in-place to interleaved audio.
pub struct BassFocus {
    ch: [Biquad; 2],
}

impl BassFocus {
    pub fn new() -> Self {
        Self { ch: [Biquad::lowpass(48_000.0, 400.0, std::f32::consts::FRAC_1_SQRT_2); 2] }
    }
    pub fn process_interleaved(&mut self, buf: &mut [f32]) {
        for fr in buf.chunks_exact_mut(2) {
            fr[0] = self.ch[0].process(fr[0]);
            fr[1] = self.ch[1].process(fr[1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rms_through(freq: f32) -> f64 {
        let mut f = Biquad::lowpass(48_000.0, 400.0, std::f32::consts::FRAC_1_SQRT_2);
        let n = 48_000;
        let mut acc = 0.0f64;
        for i in 0..n {
            let x = (i as f32 / 48_000.0 * freq * std::f32::consts::TAU).sin();
            let y = f.process(x);
            if i > 4_800 { acc += (y as f64).powi(2); } // skip transient
        }
        (acc / (n - 4_800) as f64).sqrt()
    }

    #[test]
    fn passes_bass_attenuates_treble() {
        let low = rms_through(100.0);   // bass region
        let high = rms_through(2_000.0); // guitar/vocal region
        assert!(low > 0.6, "low rms = {low}");   // ~unity (sine rms ≈ 0.707)
        assert!(high < 0.1, "high rms = {high}"); // ≥ -17 dB
    }
}
```

- [ ] **Step 2: Run (fail), implement**

RBJ cookbook lowpass: `w0 = 2π·fc/sr`, `alpha = sin(w0)/(2q)`, `cosw0 = cos(w0)`;
`b0 = (1-cosw0)/2`, `b1 = 1-cosw0`, `b2 = b0`, `a0 = 1+alpha`, `a1 = -2cosw0`, `a2 = 1-alpha`; normalize b*, a1, a2 by `a0`. `process`: `y = b0x + b1x1 + b2x2 - a1y1 - a2y2`, shift states.

- [ ] **Step 3: Run tests, verify pass; commit**

Run: `cargo test -p engine filter` — 1 PASS.

```bash
git add -A && git commit -m "feat(engine): bass-focus lowpass biquad"
```

---

### Task 7: Pipeline (commands, render, events)

**Files:**
- Create: `crates/engine/src/pipeline.rs`
- Modify: `crates/engine/src/lib.rs` (add `pub mod pipeline;`)

- [ ] **Step 1: Write failing tests**

`crates/engine/src/pipeline.rs`:
```rust
use crate::buffer::{SongBuffer, CHANNELS, SAMPLE_RATE};
use crate::filter::BassFocus;
use crate::looper::Looper;
use crate::stretch::{Stretcher, BLOCK_FRAMES};
use std::sync::Arc;

pub const GAIN_RAMP_FRAMES: usize = 240; // 5 ms

/// Copy-only commands — safe to ship over an SPSC ring into the RT thread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineCmd {
    Play,
    Pause,
    SeekSecs(f64),
    SetLoopSecs { start: f64, end: f64 },
    ClearLoop,
    SetRate(f64),
    /// semitones + cents, combined at the boundary into one scale factor
    SetPitchScale(f64),
    BassFocus(bool),
    /// RecallSilent: audio muted, position keeps advancing.
    Mute(bool),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EngineEvent {
    Position { secs: f64, rate: f64, playing: bool },
    LoopWrapped,
    Finished,
}

pub struct Pipeline {
    looper: Looper,
    stretch: Stretcher,
    bass_focus: Option<BassFocus>,
    rate: f64,
    pitch_scale: f64,
    playing: bool,
    gain: f32,
    target_gain: f32,
    feed_buf: Vec<f32>,
}

impl Pipeline {
    pub fn new(buf: Arc<SongBuffer>) -> Self { todo!() }
    pub fn apply(&mut self, cmd: EngineCmd) { todo!() }
    /// Render interleaved stereo into `out`; push events into `events`.
    pub fn render(&mut self, out: &mut [f32], events: &mut Vec<EngineEvent>) { todo!() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_buf(secs: f64) -> Arc<SongBuffer> {
        let frames = (secs * SAMPLE_RATE as f64) as usize;
        let mut data = Vec::with_capacity(frames * CHANNELS);
        for i in 0..frames {
            let s = (i as f32 / SAMPLE_RATE as f32 * 220.0 * std::f32::consts::TAU).sin() * 0.5;
            data.push(s);
            data.push(s);
        }
        Arc::new(SongBuffer { data })
    }

    fn render_secs(p: &mut Pipeline, secs: f64) -> (Vec<f32>, Vec<EngineEvent>) {
        let total = (secs * SAMPLE_RATE as f64) as usize;
        let mut out = Vec::new();
        let mut events = Vec::new();
        let mut block = vec![0.0f32; 256 * CHANNELS];
        let mut rendered = 0;
        while rendered < total {
            p.render(&mut block, &mut events);
            out.extend_from_slice(&block);
            rendered += 256;
        }
        (out, events)
    }

    fn rms(v: &[f32]) -> f64 {
        (v.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
    }

    #[test]
    fn paused_pipeline_renders_silence() {
        let mut p = Pipeline::new(sine_buf(2.0));
        let (out, _) = render_secs(&mut p, 0.5);
        assert_eq!(rms(&out), 0.0);
    }

    #[test]
    fn playing_renders_audio_and_position_advances() {
        let mut p = Pipeline::new(sine_buf(4.0));
        p.apply(EngineCmd::Play);
        let (out, events) = render_secs(&mut p, 1.0);
        assert!(rms(&out) > 0.2);
        let last_pos = events.iter().rev().find_map(|e| match e {
            EngineEvent::Position { secs, .. } => Some(*secs),
            _ => None,
        });
        let secs = last_pos.unwrap();
        assert!((0.5..=1.5).contains(&secs), "pos = {secs}");
    }

    #[test]
    fn loop_wrap_events_fire_with_output_period_scaled_by_rate() {
        let mut p = Pipeline::new(sine_buf(10.0));
        p.apply(EngineCmd::SetLoopSecs { start: 1.0, end: 2.0 }); // 1 s loop
        p.apply(EngineCmd::SetRate(0.5));
        p.apply(EngineCmd::Play);
        let (_, events) = render_secs(&mut p, 7.0);
        let wraps = events.iter().filter(|e| **e == EngineEvent::LoopWrapped).count();
        // 7 s of output at half speed covers ~3.5 loop periods (minus RB latency)
        assert!((2..=4).contains(&wraps), "wraps = {wraps}");
    }

    #[test]
    fn mute_keeps_position_moving_but_output_silent() {
        let mut p = Pipeline::new(sine_buf(10.0));
        p.apply(EngineCmd::SetLoopSecs { start: 0.0, end: 1.0 });
        p.apply(EngineCmd::Play);
        p.apply(EngineCmd::Mute(true));
        let (out, events) = render_secs(&mut p, 2.5);
        // gain ramp at the start; steady state is silent
        let tail = &out[out.len() / 2..];
        assert!(rms(tail) < 1e-4, "tail rms = {}", rms(tail));
        let wraps = events.iter().filter(|e| **e == EngineEvent::LoopWrapped).count();
        assert!(wraps >= 1, "loop must keep wrapping while muted");
    }

    #[test]
    fn bass_focus_attenuates_a_high_sine() {
        // 2 kHz sine through bass focus should drop hard
        let frames = SAMPLE_RATE as usize * 4;
        let mut data = Vec::with_capacity(frames * CHANNELS);
        for i in 0..frames {
            let s = (i as f32 / SAMPLE_RATE as f32 * 2_000.0 * std::f32::consts::TAU).sin() * 0.5;
            data.push(s);
            data.push(s);
        }
        let mut p = Pipeline::new(Arc::new(SongBuffer { data }));
        p.apply(EngineCmd::Play);
        p.apply(EngineCmd::BassFocus(true));
        let (out, _) = render_secs(&mut p, 1.0);
        let tail = &out[out.len() / 2..];
        assert!(rms(tail) < 0.05, "rms = {}", rms(tail));
    }

    #[test]
    fn finished_event_after_song_end_without_loop() {
        let mut p = Pipeline::new(sine_buf(0.5));
        p.apply(EngineCmd::Play);
        let (_, events) = render_secs(&mut p, 1.5);
        assert!(events.contains(&EngineEvent::Finished));
    }
}
```

- [ ] **Step 2: Run (fail), implement**

`render` core loop:
1. If `!playing` and gain has ramped to 0: zero `out`, return (emit one Position per render for UI liveness is fine but not required by tests).
2. While `stretch.available() < frames_requested`: `want = stretch.frames_wanted()`; read `want` frames from looper into `feed_buf`; on `info.wrapped` push `LoopWrapped`; on `info.finished` → feed what's there, emit `Finished` once, pause; `stretch.feed(...)`.
3. `stretch.pull(out)`; if fewer frames than requested, zero the remainder.
4. `bass_focus.process_interleaved(out)` when on.
5. Gain ramp: per frame `gain += (target_gain - gain) / GAIN_RAMP_FRAMES as f32` style linear move (clamp at target), multiply both samples. `Play` sets `target_gain=1` & `playing=true` (unless muted), `Pause` → `target_gain=0`, after ramp completes playing=false. `Mute(true)` → `target_gain=0` but **playing stays true**. `Mute(false)` → restore 1.
6. Position event once per `render` call: `looper.pos_frames() as f64 / SAMPLE_RATE`, current rate, playing.
7. `SetRate` → store + `stretch.set_rate`. `SetLoopSecs`/`SeekSecs`: secs × SAMPLE_RATE → frames; call looper; `stretch.reset()` on seek/loop-jump to flush stale audio.
8. `SetPitchScale(s)` → `stretch.set_pitch_scale(s)`. (Octave-up bass trick = UI sends scale 2.0 × user pitch.)

`Pause` exact semantics for the test `paused_pipeline_renders_silence`: initial state `playing=false, gain=0, target_gain=0` — render zeros without touching the stretcher.

- [ ] **Step 3: Run tests, verify pass; commit**

Run: `cargo test -p engine pipeline` — 6 PASS.

```bash
git add -A && git commit -m "feat(engine): render pipeline with commands, events, mute and bass focus"
```

---

### Task 8: PipeWire output + `Engine` facade + smoke example

**Files:**
- Create: `crates/engine/src/output.rs`, `crates/engine/src/engine.rs`, `crates/engine/examples/play.rs`
- Modify: `crates/engine/src/lib.rs` (add `pub mod output; pub mod engine;` and re-export `pub use engine::Engine;`)

This task is glue around tested parts; PipeWire specifics should be adapted
from the pipewire-rs `audio-src` example for the resolved crate version
(check docs.rs — the stream API moved between 0.7/0.8/0.9). No unit tests;
verification is the example binary.

- [ ] **Step 1: Engine facade**

`crates/engine/src/engine.rs`:
```rust
use crate::buffer::SongBuffer;
use crate::pipeline::{EngineCmd, EngineEvent, Pipeline};
use arc_swap::ArcSwapOption;
use std::sync::Arc;

pub struct Engine {
    cmd_tx: rtrb::Producer<EngineCmd>,
    evt_rx: rtrb::Consumer<EngineEvent>,
    song_slot: Arc<ArcSwapOption<SongBuffer>>,
    _pw_thread: std::thread::JoinHandle<()>,
}

impl Engine {
    /// Spawns the PipeWire thread; returns the control handle.
    pub fn start() -> crate::error::Result<Self> { /* output::spawn(...) */ }

    /// Swap in a new song; audio thread picks it up at the next block.
    pub fn load(&self, buf: SongBuffer) {
        self.song_slot.store(Some(Arc::new(buf)));
    }

    pub fn send(&mut self, cmd: EngineCmd) {
        let _ = self.cmd_tx.push(cmd); // ring full = drop oldest-style: acceptable for UI cmds
    }

    pub fn poll_events(&mut self) -> Vec<EngineEvent> {
        let mut out = Vec::new();
        while let Ok(e) = self.evt_rx.pop() {
            out.push(e);
        }
        out
    }
}
```

Rings: `rtrb::RingBuffer::<EngineCmd>::new(256)`, `rtrb::RingBuffer::<EngineEvent>::new(1024)`.

- [ ] **Step 2: PipeWire output thread**

`crates/engine/src/output.rs` — `pub fn spawn(cmd_rx, evt_tx, song_slot) -> Result<JoinHandle>`:
- Thread runs a PipeWire `MainLoop` with a playback `Stream`, format F32LE 48 kHz 2ch, `AUTOCONNECT | MAP_BUFFERS | RT_PROCESS`, node name `"earworm"`.
- Process callback state: `Option<Pipeline>` + a generation counter on the song slot (use `ArcSwapOption::load_full` and `Arc::ptr_eq` against the pipeline's current buffer to detect swaps; on swap, construct a fresh `Pipeline` — construction allocates, acceptable at song-load boundaries only).
- Callback body: drain `cmd_rx` → `pipeline.apply`; render into the stream buffer datas; push events via `evt_tx.push` (drop on full).
- No song loaded → write silence.
- Set `PIPEWIRE_LATENCY`-friendly node latency property: `node.latency = "1024/48000"` (playback tool, not an instrument chain — modest quantum is right; do NOT request 128 like Slopsmith).

- [ ] **Step 3: Smoke example**

`crates/engine/examples/play.rs`:
```rust
// Usage: cargo run -p engine --example play -- <audio-file> [loop_start] [loop_end] [rate]
// Plays via PipeWire; Ctrl-C to stop. Prints position/wrap events.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = std::path::Path::new(&args[1]);
    let buf = engine::decode::decode_file(path).expect("decode");
    println!("decoded: {:.1}s", buf.duration_secs());
    let mut eng = engine::Engine::start().expect("engine");
    eng.load(buf);
    if let (Some(s), Some(e)) = (args.get(2), args.get(3)) {
        eng.send(engine::pipeline::EngineCmd::SetLoopSecs {
            start: s.parse().unwrap(),
            end: e.parse().unwrap(),
        });
    }
    if let Some(r) = args.get(4) {
        eng.send(engine::pipeline::EngineCmd::SetRate(r.parse().unwrap()));
    }
    eng.send(engine::pipeline::EngineCmd::Play);
    loop {
        for ev in eng.poll_events() {
            println!("{ev:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
```

- [ ] **Step 4: Verify**

Run: `cargo build -p engine --example play` — must compile.
Run: `cargo test -p engine && cargo clippy -p engine -- -D warnings && cargo fmt`
Expected: all green.

If a real audio file is available, smoke-test:
`timeout 10 cargo run -p engine --example play -- <file> 10 14 0.75`
Expected: audible looped playback, `LoopWrapped` events printed roughly every 5.3 s (4 s loop at 0.75×). If no audio file is available, generate one: `ffmpeg -f lavfi -i "sine=frequency=220:duration=30" -ac 2 /tmp/test-sine.flac`.

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(engine): pipewire output thread, engine facade, play example"
```

---

## Self-review checklist

- Spec coverage: decode ✔, R3 stretch + pitch ✔, sample-accurate crossfaded loop ✔, bass-focus toggle ✔ (filter; octave-up arrives as pitch scale from the client), mute-with-advancing-position (RecallSilent) ✔, peaks + cache ✔, events (position/wrap/finished) ✔, PipeWire out ✔, RT discipline (no alloc/lock in callback except at song swap) ✔.
- Deferred: socket/dispatcher (Plan 3), UI (Plan 4), capture (v2), stems (v3).
- Known risk: pipewire-rs API drift between versions — Task 8 explicitly instructs adapting from the version-matched example.
