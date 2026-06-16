# Building Earworm on macOS

Earworm was Linux-first (PipeWire audio I/O, Linux-only deps). The codebase now
compiles for macOS behind `#[cfg(target_os = "...")]`: audio I/O uses **cpal**
(CoreAudio) on non-Linux and **PipeWire** on Linux, sharing one DSP core. This
note captures what a macOS build needs.

> **Status:** the macOS path has **never been compiled or run** — it was
> developed on Linux, where cpal's CoreAudio bindings can't be built (they need
> the macOS SDK). Everything below is the intended setup; expect to debug the
> first build on real hardware. See `docs/superpowers/plans/2026-06-16-macos-port.md`
> for the full task breakdown and follow-ups.

## What runs on macOS

- Load a song, loop sections, pitch-preserving slow-down, tempo trainer.
- The **tuner** (mic/instrument input via cpal).
- **Structure analysis** (beat_this + SongFormer) and **stem separation**
  (Demucs) — out-of-process Python, unchanged in spirit, with an MPS/Metal
  device branch (falls back to CPU).

## What was removed (both platforms)

System-audio **capture** + **grab-back** (recording another app's output) is
gone. It relied on PipeWire monitor-port tapping, which macOS has no user-space
equivalent for. The capture tab no longer exists. The tuner's *input* capture is
unaffected.

## Prerequisites

```sh
# Native libs + toolchain
brew install rubber-band pkg-config ffmpeg uv

# Rubber Band is found via pkg-config; Homebrew doesn't put its .pc on the
# default search path, so export this for the build (and any cargo invocation):
export PKG_CONFIG_PATH="$(brew --prefix)/lib/pkgconfig"
```

- `rubber-band` — pitch/time stretch (linked via `pkg-config` in
  `crates/engine/build.rs`).
- `ffmpeg` — MP3 export (`engine::encode`) and Demucs's `torchcodec`.
- `uv` — bootstraps the analysis venvs on first run.

## Build

```sh
export PKG_CONFIG_PATH="$(brew --prefix)/lib/pkgconfig"
just build           # release desktop app (.app/.dmg) + earwormd daemon
# or: cd apps/desktop && pnpm tauri build
```

The bundle is configured for `app`/`dmg` targets with
`bundle.macOS.minimumSystemVersion = 11.0`. `Info.plist` adds
`NSMicrophoneUsageDescription` so the tuner can open an input device.

## Analysis / stems on macOS

- The `analyze` wrapper resolves its Python impls (`analyze_impl.py`,
  `songformer_impl.py`) from, in order: its own dir, `$EARWORM_IMPL_DIR`,
  `/usr/lib/earworm`, `/usr/local/lib/earworm`, `/opt/homebrew/lib/earworm`.
  For a `.app` bundle, set **`EARWORM_IMPL_DIR`** to wherever the impls ship, or
  drop them in one of those dirs.
- First run downloads torch (several GB) into
  `~/.local/share/earworm/analyze-venv` (override `$EARWORM_ANALYZE_VENV`).
- **MPS caveat:** beat_this / SongFormer / Demucs may hit unimplemented Metal
  ops. If a model raises `NotImplementedError`, run with
  `PYTORCH_ENABLE_MPS_FALLBACK=1` (per-op CPU fallback) or force CPU. Record
  which models run clean on MPS.

## Known gaps to validate / fix on-device

- **Sample rate:** the cpal backends request 48 kHz; if a device rejects it,
  `build_*_stream` errors. Needs a `rubato` resampling fallback.
- **Mono mics:** the input backend requests stereo; a strictly-mono device may
  fail to start or skew the tuner. Needs `default_input_config()` +
  mono→stereo upmix.
- **Audio-thread death:** a stream error after `play()` goes silent with no
  signal to `Engine` (shared with the PipeWire path).
- **`clippy::infinite_loop`:** the output backend's park loop may trip this on
  macOS; add `#[allow(...)]` with a justification if so.
- **Mic entitlement:** confirm Tauri actually merges `Info.plist` into the app
  bundle and macOS grants access.
