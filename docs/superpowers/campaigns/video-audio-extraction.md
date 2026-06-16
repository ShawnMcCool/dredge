# Campaign: load video files for their audio (waveform-only)

Let the library accept video containers (MP4/MOV) and work with their audio
track — waveform, looping, tempo, analysis, stems — without ever playing video.
Designed 2026-06-16 (brainstorm in-thread). Work directly on `main`.

> **For agentic workers:** phases are dependency-ordered, each with a
> verification gate and a commit. The center of gravity is Phase 2 (route the
> external Python tools through a Rust-decoded WAV); Phases 1 and 3 are small
> and bracket it. Phase 4 is verification + docs.

## What it is

Earworm's Rust decoder is **symphonia**, compiled with `isomp4` + `aac`
(`crates/engine/Cargo.toml:8`). Symphonia is probe-based, not
extension-based (`decode.rs:27`): it sniffs the container and decodes a track.
Waveform peaks are computed purely from decoded PCM (`peaks.rs`), so they're
already format-agnostic.

> **Premise correction (found during Phase 4 verification):** the original plan
> assumed MP4/MOV "decode today with zero engine changes." **False.**
> `decode_file` selected `format.default_track()`, and in a video container the
> *default* track is the video track — symphonia then tried to build an h264
> decoder and failed with `unsupported codec`. The fix (Phase 1.5 below) is to
> select the first track the **audio** codec registry can decode, skipping
> video. Empirical import of an h264+AAC mp4 confirmed both the bug and the fix.

The real subtlety is the **out-of-process Python tools** (beat/section analysis
via librosa + SongFormer; Demucs stems). They don't see our decoded PCM — they
re-read the *original file path* (`analysis.rs:72` `cmd.arg(audio)`;
`stems.rs:44`; `analyze_impl.py:86` `librosa.load`). For AAC-in-MP4 that path
goes librosa→soundfile(fails)→audioread→**ffmpeg**, i.e. a system runtime
dependency. (This is the same path our existing `m4a` support already implies.)

## The decision (final)

**Make symphonia the single decode authority.** Before handing a file to any
external tool, decode it in Rust to a canonical WAV and pass *that* path. This:

- **removes ffmpeg from the runtime entirely** — for *every* format, not just
  video. librosa / torchaudio / Demucs all read plain WAV through libsndfile.
- **eliminates the dual-decode fragility** — Rust and Python now analyze
  byte-identical PCM.
- makes video support fall out for free: the WAV is just decoded PCM; its
  source container is irrelevant downstream.

So the runtime story becomes **compile-time only**. Symphonia is pure Rust,
statically linked — no ffmpeg, no gstreamer.

**Uniform path, accepted tradeoff:** all formats route through the WAV, even
lossless FLAC/WAV sources. Demucs works internally at 44.1k; a 44.1k source now
round-trips 44.1→48 (our canonical resample) →44.1 (inside Demucs). For lossy
sources (mp3/m4a/video AAC) the audio is already lossy — negligible. For
lossless sources it's a tiny, real double-resample on the stems. We take it to
keep one decode path rather than per-format branching. The user confirmed
"we're expecting songs for sure."

**Scope:** MP4/MOV only (already compiled in). MKV/WebM (Matroska demuxer +
Opus) is explicitly **out of scope** — it would need a new symphonia feature
*and* still works downstream only because of this WAV hand-off; revisit later.

## Existing surfaces this rides (verified 2026-06-16)

- `engine::decode::decode_file` (`decode.rs:20`) → canonical interleaved stereo
  f32 @ 48 kHz `SongBuffer`.
- `engine::capture::write_wav` (`capture.rs:365`) → writes interleaved stereo
  16-bit WAV @ 48 kHz with a correct header. `hound` is already an engine dep.
- `analysis_run` / `stems_run` spawn background threads that build
  `audio_path = PathBuf::from(&song.path)` and pass it to the tool
  (`app.rs:701`, `app.rs:806`). Both already run **outside the `App` mutex** on
  their own thread — the decode slots in there with no new lock phasing.
- No app-level test drives `analysis_run`/`stems_run` with fakes (the fakes are
  tested at the trait level in `analysis.rs`/`stems.rs`), so inserting a real
  decode into those threads breaks no existing test.
- Frontend allowlist: `apps/desktop/src/lib/file-picker.ts:3`
  (`["mp3","flac","ogg","wav","m4a"]`), UI-only — the server does no extension
  validation.

## Phase 1 — `decode_to_wav` engine helper (Rust, TDD)

**Goal:** one call that decodes any supported file to a canonical WAV on disk.

**Files:** `crates/engine/src/decode.rs`; `crates/engine/tests/decode.rs`.

- [ ] **1.1** Add `pub fn decode_to_wav(src: &Path, dst: &Path) -> Result<()>`
  to `decode.rs`: `let buf = decode_file(src)?; crate::capture::write_wav(dst, &buf.data)`.
- [ ] **1.2** Test (write first): synth a short WAV fixture (the test file
  already does this), run `decode_to_wav` into a tempdir, assert the output is
  a readable 48 kHz stereo WAV (`wav_header_rate` == 48000; `WavReader` channel
  count == 2; sample count > 0).
- [ ] **Gate:** `cargo test -p engine` green. **Commit:**
  `feat(engine): decode_to_wav — canonical WAV for external tools`.

## Phase 1.5 — Select the audio track, not the default track (Rust) ✅

**Found necessary during verification — see premise correction above.**

- [x] In `decode_file` (`decode.rs:36`), replace `format.default_track()` with
  "first track the audio codec registry (`get_codecs()`) can decode," so video
  tracks in mp4/mov are skipped. Audio-only files are unaffected (their audio
  track is still chosen).
- [x] Committed fixture `crates/engine/tests/fixtures/video_with_audio.mp4`
  (14 KB, h264 + AAC) + test `decodes_audio_track_from_a_video_container` —
  decodes without ffmpeg at test time.

## Phase 2 — Route analysis + stems through a decoded WAV (Rust)

**Goal:** the Python tools read a Rust-decoded WAV, never the original file.

**Files:** `crates/server/Cargo.toml`; `crates/server/src/app.rs`.

- [ ] **2.1** Promote `tempfile = "3"` from `[dev-dependencies]` into
  `[dependencies]` in `crates/server/Cargo.toml` (keep the dev entry or rely on
  the single dep; cargo dedupes).
- [ ] **2.2** Add a helper in `app.rs`:
  `fn canonical_wav_for_tools(src: &Path) -> Result<(tempfile::TempDir, PathBuf), String>`
  — make a fresh temp dir, decode `src` into `<dir>/audio.wav` via
  `engine::decode::decode_to_wav`, return `(dir, wav_path)`. The caller holds the
  `TempDir` for the tool's lifetime; it auto-removes on drop. Fixed stem
  `audio` keeps Demucs's `file_stem`-derived output dir
  (`stems.rs:82`) deterministic.
- [ ] **2.3** In the `analysis_run` thread (`app.rs:816`): decode first; pass the
  WAV path to `analyze_with_recovery` instead of `audio_path`; on decode error,
  short-circuit to a failed `Result` so the existing event/profile flow reports
  it. Keep the `TempDir` bound for the whole thread body. Add a `timer.stage("decode", …)`
  for visibility.
- [ ] **2.4** In the `stems_run` thread (`app.rs:713`): same — decode first, pass
  the WAV to `separator.separate`, hold the `TempDir`, surface decode failure
  through the `stems_progress` event.
- [ ] **Gate:** `cargo test -p server` green; `just lint` (clippy `-D warnings`)
  clean. **Commit:**
  `feat(server): feed external analysis/stems a Rust-decoded WAV (drops ffmpeg runtime dep)`.

## Phase 3 — Accept video files in the picker (frontend)

**Goal:** the open dialog offers MP4/MOV; nothing else changes.

**Files:** `apps/desktop/src/lib/file-picker.ts`.

- [ ] **3.1** Add `"mp4", "mov"` to `AUDIO_EXTENSIONS`. Rename the filter label
  from `"audio"` to `"audio / video"` so users understand video files are
  accepted for their audio. (Server already does no extension validation, so no
  backend change.)
- [ ] **Gate:** `just lint` (svelte-check) clean. **Commit:**
  `feat(desktop): accept mp4/mov in the file picker (audio track only)`.

## Phase 4 — Verify end-to-end + document

**Goal:** prove a real MP4 loads, waveforms, and analyzes; record the decision.

- [ ] **4.1** Build (`just build`) and empirically load an MP4 with an AAC audio
  track: confirm it imports, the waveform renders, looping works, and analysis
  completes (the analyze venv must be present). Note: stems needs Demucs
  installed; verify if available, else note as untested.
- [ ] **4.2** README/feature note: supported inputs now include MP4/MOV (audio
  track only, no video playback); MKV/WebM not yet. Mention that external
  analysis/stems no longer require ffmpeg.
- [ ] **Gate:** `just check` (full test + lint) green. **Commit:**
  `docs: note video-file (mp4/mov) audio support`.

## Deliberately excluded

- **Video playback / thumbnails** — never; this is audio-first by design.
- **MKV/WebM/Opus** — needs a symphonia Matroska feature; revisit on demand.
- **Caching the decoded WAV across analysis+stems runs** — decode is cheap
  beside torch; ephemeral per-job temp dirs avoid a cache-lifecycle burden.
  Revisit only if profiling shows the double decode matters.
