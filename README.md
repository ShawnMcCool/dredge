# earworm

An ear-first practice looper for Linux. Learn songs by sound — loop sections,
slow them down without pitch change, and practice them the way the motor-learning
research says actually works.

No note highways, no gamification, no chord sheets. A waveform, your ears,
and a practice engine.

## What it does

- **Sample-accurate looping** with a click-free crossfaded seam, 0.25–2.0×
  pitch-preserving speed (Rubber Band R3), ±12 semitones + cents pitch.
- **Loop by selecting** — drag a span and hit ⟳ to loop it instantly
  (transient, nothing saved) or 🖫 to save it to the Loops tab. Saved loops are
  named automatically from the sections they cover (`verse 2`,
  `verse 2 → chorus 1`); double-click to pin your own name, or *fit* to snap a
  loop's edges to the nearest section boundaries.
- **Evidence-based practice plans** — the novel part. Plans are built from
  steps the literature supports over folklore:
  - *Listen-first* passes (audiation before playing)
  - *Oscillating tempo* (touch full speed early; dwell ~85–95%) instead of
    only the classic +5%-per-rep ladder (also available)
  - *Junction loops* auto-derived across section boundaries — transitions are
    where songs fall apart
  - *Rotation* steps that interleave sections (better next-day retention than
    grinding one loop)
  - *Recall tests* — alternating audible / silent passes, play from memory
  - *Spaced resurfacing* of loops across days; progress is measured by
    **next-day retests**, not in-session smoothness
- **Ephemeral practice** (`p`): select a span on the waveform, press `p` —
  an instant micro-session runs on it (listen ×2 → oscillating play reps).
  Rate it at the end to keep the auto-named loop; discard leaves no trace.
- **Capture anything** (v2): tap any app's PipeWire node — Spotify, Firefox,
  whatever — into a rolling 3-minute buffer and *grab what just played* as a
  loopable song.
- **Local stems** (v3): 4-stem separation (vocals/drums/**bass**/other) via
  Demucs, per-stem faders, one key (`m`) to mute the recorded bass and play
  it yourself. No cloud, ever.
- **Bass focus** (`b`): octave-up + low-pass — the transcriber's trick for
  hearing buried bass lines.
- **Analyze** (v4): one **PREPARE** button (`a`) runs analysis then stem
  separation with a progress modal — beats/downbeats/BPM (beat_this) plus
  suggested sections (SongFormer, novelty fallback). Beat ticks on the
  waveform, loop edges snap to downbeats (`g` toggles), and saved sections
  derive bar-aware junction loops. Runs through the repo-shipped
  `scripts/analyze` wrapper, which bootstraps its own venv on first use —
  swapping models never touches Rust.
- **Structure on the waveform** (v5): the section lane is color-coded by
  label (chorus rides the amber accent; novelty's A/B/C labels get stable
  fallback hues). Fresh analysis suggestions appear right on the waveform —
  dashed, italic — until you save real sections; clicking any span points the
  transport loop at it, double-clicking a suggestion seeds the selection.
- **Playback volume** (v5): a compact transport fader (0–150%, click-free
  ramped in the engine, separate from mute/pause), persisted across sessions.
- **Durable settings** (`,` or the gear button): UI scale, grid-snap default,
  capture buffer length, playback volume — stored server-side in the
  practice DB.
- **Escape asks before quitting**: `esc` walks back quick-prompt → selection →
  an exit confirmation (`enter`/`y` exits, `esc`/`n` stays).
- **Scriptable**: a JSON-lines control socket at `$XDG_RUNTIME_DIR/earworm.sock`
  (mpv-style) drives everything the UI can do — foot pedals, shell scripts,
  whatever. The desktop UI and the socket share one session.

## Layout

```
crates/practice   pure practice logic: plans, runner, scheduler, store, sidecar
crates/engine     audio: decode, loop, stretch, filter, PipeWire out + capture
crates/server     App dispatcher, control socket, earwormd headless binary
apps/desktop      Tauri 2 + Svelte 5 UI
docs/superpowers  design spec + the six implementation plans
```

Annotations (sections/loops/plans) mirror to plain JSON sidecars
(`<song>.earworm.json`) next to your audio files — git-able, grep-able,
not locked in.

## Installation

Earworm is **Linux-only** and built from source — there's no prebuilt binary or
package. The audio engine is PipeWire-native (output *and* capture), so a
PipeWire audio stack is mandatory; there is no ALSA/PulseAudio fallback.

### 1. System dependencies

The core app links four native libraries at runtime and needs a small build
toolchain to compile. On **Arch**:

```bash
# runtime + build libraries
sudo pacman -S rubberband pipewire webkit2gtk-4.1 gtk3
# build toolchain
sudo pacman -S rust nodejs pnpm just clang pkgconf base-devel
```

| Dependency | Why it's needed |
|------------|-----------------|
| `rubberband` (≥3.0) | pitch-preserving time-stretch (Rubber Band R3), FFI-linked by the engine |
| `pipewire` | all audio output and capture |
| `webkit2gtk-4.1`, `gtk3` | the Tauri webview that renders the UI (desktop app only) |
| `clang` / libclang | bindgen builds the PipeWire/`libspa-sys` bindings — the build fails without it |
| `pkgconf`, `base-devel` | `pkg-config` + a C compiler/linker for the FFI crates |
| `rust`, `nodejs`, `pnpm`, `just` | build the Rust workspace and the Svelte/Tauri frontend |

> **Debian/Ubuntu** (package names approximate): `librubberband-dev
> libpipewire-0.3-dev libspa-0.2-dev libwebkit2gtk-4.1-dev libgtk-3-dev clang
> pkg-config build-essential`, plus `rustup`, Node + `pnpm`, and `just`.

Audio **decoding** is pure-Rust (symphonia: mp3/flac/ogg/wav/aac/m4a) and SQLite
is bundled into the binary — no ffmpeg and no system SQLite required.

### 2. Build the binaries

```bash
git clone <repo-url> earworm && cd earworm
just build         # desktop app -> target/release/earworm
                   # headless daemon -> target/release/earwormd
```

`just build` runs `pnpm tauri build` (which installs frontend deps and bundles
the Svelte UI into the `earworm` binary) followed by `cargo build -p server
--release` for the daemon. Tauri OS bundling is disabled, so the output is the
raw executables under `target/release/` — copy `earworm` onto your `PATH` if you
want it installed system-wide.

### 3. Optional ML features

Both are **off by default** and self-bootstrap on first use — the app runs fine
without them, you simply don't get that feature. Both want an NVIDIA GPU with
CUDA (they fall back to CPU, just much slower) and several GB of disk for the
PyTorch venvs and downloaded model weights.

- **Analyze** (beats / downbeats / BPM / sections) needs [`uv`](https://docs.astral.sh/uv/)
  on `PATH`. The first analysis bootstraps `~/.local/share/earworm/analyze-venv`
  (python 3.12, beat_this + torch) automatically. For the higher-quality
  SongFormer section labels, additionally create the SongFormer venv (the
  wrapper prefers it when present and falls back to a novelty detector
  otherwise; SongFormer wants ~8 GB of free VRAM):

  ```bash
  uv venv --python 3.11 ~/.local/share/earworm/songformer-venv
  uv pip install --python ~/.local/share/earworm/songformer-venv/bin/python \
    torch==2.4.0 torchaudio==2.4.0 "numpy<2" transformers==4.51.1 librosa \
    soundfile ema-pytorch loguru omegaconf tqdm safetensors muq x-transformers \
    msaf einops huggingface_hub
  ```

- **Stems** (vocals / drums / bass / other separation) needs a `demucs` binary
  on `PATH`:

  ```bash
  uv tool install demucs --with torchcodec   # PyTorch ~2.5 GB; torchcodec is
                                             # required by torchaudio 2.9+ to save stems
  ```

## Usage

Launch the desktop app:

```bash
just run        # builds first if target/release/earworm is missing
# or run the binary directly:
target/release/earworm
```

Or the headless daemon (no UI — just the control socket, for foot pedals and
scripts):

```bash
just daemon
# earwormd [--socket <path>] [--db <path>]
```

**Basic workflow:** import a song (drag in, or `song.import` over the socket),
press **`a` (PREPARE)** to run analysis + stem separation with a progress modal,
then select a span on the waveform and loop / slow / practice it. Sections and
loops mirror to a git-able JSON sidecar (`<song>.earworm.json`) next to the
audio file.

**Keyboard-first** (the UI is keyboard-driven; keys are skipped while typing in a
field):

| Key | Action | Key | Action |
|-----|--------|-----|--------|
| `space` | play / pause | `p` | quick practice on selection |
| `a` | PREPARE (analyze + stems) | `b` | bass focus (octave-up + low-pass) |
| `l` | loop selection (transient) | `m` | mute the recorded bass stem |
| `r` | restart loop | `g` | toggle grid (downbeat) snap |
| `[` `]` | rate ∓5% | `1` `2` `3` | rate rep miss / shaky / solid |
| `ctrl ±` / `ctrl 0` | zoom in/out / reset | `,` | settings |
| `esc` | clear selection → quit prompt | | |

Settings (UI scale, grid-snap default, capture buffer length, playback volume)
live in the gear menu (`,`) and persist in the practice DB.

**Paths & overrides** (defaults shown):

| What | Default | Override |
|------|---------|----------|
| Database | `~/.local/share/earworm/earworm.db` | `EARWORM_DB` (desktop), `--db` (daemon) |
| Control socket | `$XDG_RUNTIME_DIR/earworm.sock` | `--socket` (daemon) |
| Analyze venv | `~/.local/share/earworm/analyze-venv` | `EARWORM_ANALYZE_VENV` |
| SongFormer venv | `~/.local/share/earworm/songformer-venv` | `EARWORM_SONGFORMER_VENV` |
| Analyze wrapper | repo `scripts/analyze` | `EARWORM_ANALYZE` |

Everything the UI can do is also reachable over the JSON-lines control socket —
see [Socket quick taste](#socket-quick-taste) below.

## Development

The system + toolchain dependencies are the same as
[Installation](#1-system-dependencies) above (Rust, Node, pnpm, `just`, clang,
and the native dev libraries). No extra runtime is needed for the dev loop;
clippy, rustfmt, and `svelte-check` ship with the toolchains. Rust edition 2021,
recent stable Rust; Tauri 2 + Svelte 5, Vite, pnpm (not npm) for the frontend.

```bash
just dev        # desktop app with vite hot-reload + debug Rust host
just test       # full suite: cargo test --workspace + pnpm vitest run
just lint       # clippy (-D warnings), cargo fmt --check, svelte-check
just check      # pre-commit gate: test + lint
just fmt        # cargo fmt
just clean      # remove build artifacts
just            # list all recipes
```

Targeted runs (invoke directly):

```bash
cargo test -p practice store::tests::name_of_test     # single Rust test
cargo test -p engine                                  # single crate
cd apps/desktop && pnpm vitest run lib/waveform-math.test.ts   # single frontend test
```

**Project layout** — see [Layout](#layout) above. The whole backend is one
command dispatcher (`crates/server/src/app.rs`); the Unix socket and the Tauri
webview are two transports over the same dispatcher, and the Svelte UI is "just
another client" with no second source of truth. Pure frontend logic lives in
`apps/desktop/src/lib/*.ts` with colocated `*.test.ts` vitest files. Python
analysis/stems run out-of-process through the wrappers in `scripts/` — the Rust
side only parses their JSON, so swapping models never touches Rust.

A couple of conventions worth knowing before you build: the `time` crate is
pinned `>=0.3, <0.3.48` (a later regression breaks `tauri-utils`), and errors
crossing the protocol boundary collapse to a single `error: String` channel via
the `ErrStr`/`err_str` helper in `app.rs`. See `CLAUDE.md` for the full
architecture notes.

## Socket quick taste

```bash
printf '%s\n' '{"id":1,"cmd":"song.import","params":{"path":"/path/song.flac"}}' | \
  python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect("'"$XDG_RUNTIME_DIR"'/earworm.sock"); s.sendall(sys.stdin.buffer.read()); print(s.recv(65536).decode())'
```

Commands: `song.*`, `section.replace`, `loop.*`, `junctions.derive`, `plan.*`,
`practice.quick*`, `rep.rate`, `due.list`, `retention`, `capture.*`, `stems.*`,
`analysis.*`, `settings.*`, transport
(`play/pause/seek/rate/volume/pitch/loop.set/bass_focus/mute`), `subscribe`
for the event stream.

## Why these mechanics (short version)

What feels productive — slow monotone repetition, massing one loop, constant
feedback — is systematically not what produces durable skill (Bjork's
"desirable difficulties"). The spec (`docs/superpowers/specs/`) cites the
research behind each mechanic: Furuya 2014 (slow≠fast motor control),
Stambaugh 2011 (interleaving), Walker 2002 (sleep consolidation), Keller 2013
(auditory-before-motor), Driskell 1994 (mental practice).
