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
- **Drill box** — a live workbench for the active loop, minimal by default
  (a fresh loop just plays); each tool is opt-in:
  - *Tempo trainer* (`d`): ramp the speed across loop passes — a +per-pass
    ladder, an oscillation that touches full speed early, or a constant dwell
  - *Region shaping*: nudge an edge, isolate a half, or add a bar of run-up to
    rehearse the entrance — all on a scratch span, your saved loop is untouched
  - *Recall*: mute the recording for a pass (or every Nth pass) so you play it
    from memory while the loop stays in time
- **Capture anything** (v2): tap any app's PipeWire node — Spotify, Firefox,
  whatever — into a rolling 3-minute buffer and *grab what just played* as a
  loopable song.
- **Local stems** (v3): 4-stem separation (vocals/drums/**bass**/other) via
  Demucs, per-stem faders, one key (`m`) to mute the recorded bass and play
  it yourself. No cloud, ever.
- **Bass focus** (`b`): octave-up + low-pass — the transcriber's trick for
  hearing buried bass lines.
- **Instrument tuner** (v6): a chromatic tuner box in the stage — power it on
  (⏻), pick your audio input once (remembered across sessions, behind the gear),
  and tune by note + cents with a hold-to-lock "in tune" confirm. Always
  available, even with no song open. Listens to a mic or interface via the same
  PipeWire capture path; detection is pure-Rust (YIN), no cloud.
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
crates/practice   pure domain logic: model, loop naming, store, sidecar
crates/engine     audio: decode, loop, stretch, filter, PipeWire out + capture
crates/server     App dispatcher, control socket, earwormd headless binary
apps/desktop      Tauri 2 + Svelte 5 UI
docs/superpowers  design spec + the six implementation plans
```

Annotations (sections/loops) mirror to plain JSON sidecars
(`<song>.earworm.json`) next to your audio files — git-able, grep-able,
not locked in.

## Installation

Earworm is **Linux-only**. The audio engine is PipeWire-native (output *and*
capture), so a **PipeWire** audio stack is mandatory — there is no
ALSA/PulseAudio fallback. Pick the install path for your distro; each one puts
`earworm` (desktop app) and `earwormd` (headless daemon) on your `PATH`. Audio
**decoding** is pure-Rust (symphonia: mp3/flac/ogg/wav/aac/m4a) and SQLite is
bundled — no ffmpeg, no system SQLite.

### Arch / Arch-based (AUR)

```bash
yay -S earworm-bin      # prebuilt — no toolchain, just the runtime libs
# …or build from source:
yay -S earworm
```

`earworm-bin` is the fast path; both pull the runtime deps
(`rubberband pipewire webkit2gtk-4.1 gtk3`) automatically.

### Debian / Ubuntu (.deb)

Download the latest `earworm_*_amd64.deb` from the
[releases page](https://github.com/ShawnMcCool/earworm/releases), then:

```bash
sudo apt install ./earworm_*_amd64.deb
```

Needs **Ubuntu 24.04+ / Debian 13+** (PipeWire 1.0+, plus `libwebkit2gtk-4.1-0`);
`apt` resolves the runtime deps (`librubberband2`, `libpipewire-0.3-0`,
`libwebkit2gtk-4.1-0`, `libgtk-3-0`).

### Build from source

Install the native libraries + build toolchain. On **Arch**:

```bash
sudo pacman -S rubberband pipewire webkit2gtk-4.1 gtk3        # runtime libs
sudo pacman -S rust nodejs pnpm just clang pkgconf base-devel # toolchain
```

On **Debian/Ubuntu**:

```bash
sudo apt install librubberband-dev libpipewire-0.3-dev libspa-0.2-dev \
  libwebkit2gtk-4.1-dev libgtk-3-dev clang pkg-config build-essential
# plus rustup, Node + pnpm, and just
```

| Dependency | Why it's needed |
|------------|-----------------|
| `rubberband` (≥3.0) | pitch-preserving time-stretch (Rubber Band R3), FFI-linked by the engine |
| `pipewire` | all audio output and capture |
| `webkit2gtk-4.1`, `gtk3` | the Tauri webview that renders the UI (desktop app only) |
| `clang` / libclang | bindgen builds the PipeWire/`libspa-sys` bindings — the build fails without it |
| `pkgconf`, `base-devel` | `pkg-config` + a C compiler/linker for the FFI crates |
| `rust`, `nodejs`, `pnpm`, `just` | build the Rust workspace and the Svelte/Tauri frontend |

Then build:

```bash
git clone https://github.com/ShawnMcCool/earworm.git && cd earworm
just build         # daemon -> target/release/earwormd, then desktop app +
                   # .deb bundle -> target/release/{earworm,bundle/deb/}
```

`just build` compiles the daemon, then `pnpm tauri build` (installs frontend
deps, bundles the Svelte UI into `earworm`, and emits a `.deb`). `just package`
stages just the `.deb` into `dist/`; `just artifacts` adds a portable tarball +
`SHA256SUMS`. Copy `earworm`/`earwormd` onto your `PATH`, or `sudo apt install`
the `.deb`.

### Enable the optional ML features

Beat/section **analyze** and **stem** separation are **off by default** and
self-bootstrap on first use — the app runs fine without them. One command sets
them up ahead of time:

```bash
earworm-enable-ml all        # analyze + songformer + stems
# …or individually:
earworm-enable-ml analyze    # beats / downbeats / BPM + novelty sections
earworm-enable-ml songformer # higher-quality section labels (wants ~8 GB VRAM)
earworm-enable-ml stems      # 4-stem separation (vocals/drums/bass/other, demucs)
```

Needs [`uv`](https://docs.astral.sh/uv/) on `PATH`. An NVIDIA GPU is optional
(CPU works, just much slower); several GB of disk go to the PyTorch venvs and
downloaded model weights. The `analyze` wrapper also self-bootstraps its venv on
the first real analysis if you skip this. From a source checkout the helper is
`scripts/earworm-enable-ml`.

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
| Analyze wrapper | repo `scripts/analyze`, or `earworm-analyze` on `PATH` (packages install it; impls under `/usr/lib/earworm`) | `EARWORM_ANALYZE` |

Everything the UI can do is also reachable over the JSON-lines control socket —
see [Socket quick taste](#socket-quick-taste) below.

## Development

The system + toolchain dependencies are the same as
[Build from source](#build-from-source) above (Rust, Node, pnpm, `just`, clang,
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

**Dev builds decode audio ~100× slower than release.** Unoptimized
symphonia/rubato make `song.open` take *minutes* for a song with cached stems —
the main mix is re-decoded and resampled to 48 kHz on every open (sections,
analysis, and waveform peaks are already cached to SQLite/disk; the decoded mix
is not). `[profile.dev.package."*"] opt-level = 2` in the workspace `Cargo.toml`
optimizes dependencies (including the audio `engine`) while keeping our own
crates at `opt-level 0` for fast incremental rebuilds — that one line is what
makes `just dev` usable. Don't remove it.

### Tuner pitch detection — lessons

The tuner is split between `crates/engine/src/pitch.rs` (detection, pure +
unit-tested) and `crates/server/src/tuner.rs` (input capture + the ~50 ms
sampler thread). What it took to make it behave like a real tuning pedal rather
than a twitchy frequency meter:

- **YIN, not McLeod/MPM.** The `pitch-detection` crate ships both. McLeod is
  documented to be inaccurate at low pitches (the bass range) and measured 2–4
  cents off there in our characterization, while YIN was exact on
  41/82/98/196/330 Hz — and a few cents *is* the whole game for a tuner. The
  crate maps `clarity_threshold` to YIN's absolute threshold as
  `1 − clarity_threshold`; we pass 0.8 → **0.2**, the value TarsosDSP uses for
  real instruments (the YIN paper's 0.1 is stricter and rejects the
  inharmonicity of real strings). YIN's returned `clarity` is unreliable in this
  crate, so it is *not* used as a confidence signal.
- **The periodicity threshold rejects noise; the power gate is only an energy
  floor.** `POWER_THRESHOLD` is deliberately low (≈ amp 0.016) so weak/decaying
  high strings keep registering — non-periodic noise is rejected by YIN's
  threshold, not by loudness. (Verified: white noise louder than a detectable
  quiet tone is still rejected.) Setting the power gate too high is what made
  soft high strings vanish; too low alone made them jump.
- **Stabilization is a median + octave-fold, never an average.** A 5-frame
  (~250 ms) **median** discards the single-frame outliers an EMA would smear in;
  each new estimate is **octave-folded** toward the running median to kill the
  2×/½× harmonic jumps that make a readout leap octaves. A short **release-hold**
  re-sends the last reading through brief dropouts so the gauge doesn't flicker
  out mid-note. This trio (median + fold + hold) is the difference between
  "jumps around erratically" and "locks steady."
- **Note/cents is computed frontend-side** (`apps/desktop/src/lib/tuner-math.ts`,
  pure + unit-tested); the wire event `tuner_pitch` carries just
  `{ hz, confidence }`. `EARWORM_DEBUG=1` makes the sampler log the raw
  per-frame Hz before stabilization — the fastest way to tell octave errors
  (raw bouncing 98↔196) from lag (raw solid, display trailing).

## Socket quick taste

```bash
printf '%s\n' '{"id":1,"cmd":"song.import","params":{"path":"/path/song.flac"}}' | \
  python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect("'"$XDG_RUNTIME_DIR"'/earworm.sock"); s.sendall(sys.stdin.buffer.read()); print(s.recv(65536).decode())'
```

Commands: `song.*`, `section.replace`, `loop.*`, `capture.*`, `stems.*`,
`analysis.*`, `settings.*`, transport
(`play/pause/seek/rate/volume/pitch/loop.set/bass_focus/mute`), `subscribe`
for the event stream.

## Why these mechanics (short version)

What feels productive — slow monotone repetition, constant feedback — is
systematically not what produces durable skill (Bjork's "desirable
difficulties"). The drill box leans on that: a tempo trainer that varies speed
across passes rather than grinding one rate, and a recall mode that makes you
play from memory. The spec (`docs/superpowers/specs/`) cites the research —
Furuya 2014 (slow≠fast motor control), Keller 2013 (auditory-before-motor),
Driskell 1994 (mental practice).
