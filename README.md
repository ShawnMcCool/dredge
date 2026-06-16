<p align="center">
  <img src="docs/earworm.png" alt="earworm — a waveform with detected song sections, a stem mixer, a tuner, and the song-structure panel" width="820">
</p>

<h1 align="center">earworm</h1>

<p align="center">
  An ear-first practice looper for Linux — loop a section, slow it down without
  changing pitch, and drill it until it's yours.
</p>

<p align="center">
  <a href="https://github.com/ShawnMcCool/earworm/releases">Releases</a> ·
  <a href="#install">Install</a> ·
  <a href="#build-from-source">Build from source</a>
</p>

---

## Features

- **Sample-accurate looping** — click-free crossfaded seam, instant from a drag.
- **Pitch-preserving speed** — 0.25–2.0× (Rubber Band R3), plus ±12 semitones and cents.
- **Auto-named loops** — named from the sections they cover (`verse 2 → chorus 1`); edges snap to downbeats.
- **Drill box** — a tempo trainer that ramps speed across passes, region shaping, and a recall mode that makes you play from memory.
- **Local stems** — 4-stem separation (vocals / drums / **bass** / other) via Demucs, with per-stem faders. No cloud.
- **Bass focus** — octave-up + low-pass to hear buried basslines.
- **Song structure** — beats, downbeats, BPM, and labelled sections detected and drawn on the waveform.
- **Instrument tuner** — a chromatic tuner in the stage; note + cents with a hold-to-lock confirm, works with no song open.
- **Export** — render the current mix (stem balance, speed, pitch, bass focus baked in) to WAV or MP3.
- **Plain-text annotations** — sections and loops mirror to a git-able JSON sidecar next to your audio.
- **Scriptable** — a JSON control socket drives everything the UI can; wire up a foot pedal or a shell script.

No note highways, no gamification, no chord sheets — a waveform, your ears, and a practice engine.

## Install

Linux only. The audio engine is PipeWire-native (PipeWire 1.0+ required; no ALSA/PulseAudio fallback).

**Arch / Arch-based**

```bash
yay -S earworm-bin      # prebuilt; or `yay -S earworm` to build from source
```

**Debian / Ubuntu** (24.04+ / Debian 13+)

Download the latest `earworm_*_amd64.deb` from the
[releases page](https://github.com/ShawnMcCool/earworm/releases), then:

```bash
sudo apt install ./earworm_*_amd64.deb
```

`apt` pulls the runtime libraries automatically — the whole looper works with nothing else installed.

**Optional tools** unlock extra features. Run **`earworm-doctor`** any time to see what's installed and the command to add what's missing:

| Feature | Needs |
|---------|-------|
| MP3 export · mkv/webm containers · stem export | `ffmpeg` — `sudo apt install ffmpeg` |
| Beat / section analysis · stem separation | `uv` + [`earworm-enable-ml`](#optional-ml-features) |

## Optional ML features

Analysis and stem separation are off by default and self-bootstrap on first use. To set them up ahead of time (needs [`uv`](https://docs.astral.sh/uv/) on `PATH`):

```bash
earworm-enable-ml all        # analyze + songformer + stems (demucs)
```

A GPU is optional — CPU works, just slower. The PyTorch venvs and model weights take several GB of disk.

## Build from source

```bash
# Debian / Ubuntu native deps:
sudo apt install librubberband-dev libpipewire-0.3-dev libspa-0.2-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev clang pkg-config build-essential
# Arch: pacman -S rubberband pipewire webkit2gtk-4.1 gtk3 clang pkgconf base-devel
# plus the toolchain: rustup, Node + pnpm, and just

git clone https://github.com/ShawnMcCool/earworm.git && cd earworm
just build      # -> target/release/{earworm, earwormd} + a .deb bundle
just dev        # hot-reload dev app
just check      # tests + lint
```

Audio decoding is pure-Rust (symphonia); awkward video containers fall back to `ffmpeg` when it's on your `PATH`. Architecture notes and the full command list live in [`CLAUDE.md`](CLAUDE.md); the design spec and the research behind the practice model are under [`docs/superpowers/`](docs/superpowers/).

## License

MIT.
