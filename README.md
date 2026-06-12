# earworm

An ear-first practice looper for Linux. Learn songs by sound — loop sections,
slow them down without pitch change, and practice them the way the motor-learning
research says actually works.

No note highways, no gamification, no chord sheets. A waveform, your ears,
and a practice engine.

## What it does

- **Sample-accurate looping** with a click-free crossfaded seam, 0.25–2.0×
  pitch-preserving speed (Rubber Band R3), ±12 semitones + cents pitch.
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
- **Analyze** (v4): one click per song — beats/downbeats/BPM (beat_this) plus
  suggested sections (SongFormer, novelty fallback). Beat ticks on the
  waveform, loop edges snap to downbeats (`g` toggles), and saved sections
  derive bar-aware junction loops. Runs through the repo-shipped
  `scripts/analyze` wrapper, which bootstraps its own venv on first use —
  swapping models never touches Rust.
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

## Build

Arch deps: `rubberband pipewire webkit2gtk-4.1 gtk3` (pkg-config used at build).

```bash
cargo test                                  # 120 tests
cd apps/desktop && pnpm install && pnpm tauri build   # UI binary -> target/release/earworm
cargo build -p server --release             # headless -> target/release/earwormd
```

Optional, for stems: `uv tool install demucs --with torchcodec` (PyTorch, ~2.5 GB; torchcodec is required by torchaudio 2.9+ for saving stems).

Optional, for Analyze: nothing — `scripts/analyze` bootstraps
`~/.local/share/earworm/analyze-venv` (uv, python 3.12, beat_this + torch)
on first run. For SongFormer section labels, additionally create
`~/.local/share/earworm/songformer-venv` (python 3.11, `torch==2.4.0
torchaudio==2.4.0 "numpy<2" transformers==4.51.1 librosa soundfile ema-pytorch
loguru omegaconf tqdm safetensors muq x-transformers msaf einops
huggingface_hub`); the wrapper prefers it when present and falls back to a
novelty detector otherwise. SongFormer wants ~8 GB of free VRAM.

## Socket quick taste

```bash
printf '%s\n' '{"id":1,"cmd":"song.import","params":{"path":"/path/song.flac"}}' | \
  python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect("'"$XDG_RUNTIME_DIR"'/earworm.sock"); s.sendall(sys.stdin.buffer.read()); print(s.recv(65536).decode())'
```

Commands: `song.*`, `section.replace`, `loop.*`, `junctions.derive`, `plan.*`,
`practice.quick*`, `rep.rate`, `due.list`, `retention`, `capture.*`, `stems.*`,
`analysis.*`, transport
(`play/pause/seek/rate/pitch/loop.set/bass_focus/mute`), `subscribe` for the
event stream.

## Why these mechanics (short version)

What feels productive — slow monotone repetition, massing one loop, constant
feedback — is systematically not what produces durable skill (Bjork's
"desirable difficulties"). The spec (`docs/superpowers/specs/`) cites the
research behind each mechanic: Furuya 2014 (slow≠fast motor control),
Stambaugh 2011 (interleaving), Walker 2002 (sleep consolidation), Keller 2013
(auditory-before-motor), Driskell 1994 (mental practice).
