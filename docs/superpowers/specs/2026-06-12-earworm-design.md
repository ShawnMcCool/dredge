# earworm — ear-first practice looper (design)

**Date:** 2026-06-12
**Status:** approved design, pre-implementation

## One sentence

A Linux-native looper for learning songs by ear that makes evidence-based
practice the default path — listen-first passes, junction loops, section
rotation, oscillating tempo, recall tests — with sample-accurate looping and
pitch-preserving slowdown.

## Why this app

Research synthesis (motor learning + music pedagogy) and a 2026 market survey
both point at the same gap:

- The practices that produce durable skill — interleaving sections, touching
  near-target tempo early, drilling chunk *junctions*, spacing across days,
  faded auditory feedback — are exactly the ones that feel worse in-session,
  so no one does them unaided (Bjork's "desirable difficulties").
- Every existing tool (Transcribe!, Anytune, Moises, Song Master,
  PracticeSession) competes on stretch quality and stems. Only Capo (Mac-only)
  has shipped a single pedagogy-driven mechanic. The practice *schedule* as
  the product is open territory.
- Linux has unfair advantages: PipeWire can tap any application's audio node
  (the Spotify-loop workflow the whole market lost in 2022), and local
  Demucs/ONNX stems need no cloud.

Anti-goals, by conviction: no note highways, no gamification, no real-time
correctness HUD, no notation engine. Sound is the interface. Visuals are a
waveform and lists.

## Research grounding → mechanics

| Finding (strength) | Mechanic |
|---|---|
| Auditory-before-motor learning is fast and improves accuracy (moderate) | **Listen-first passes**: a plan step that loops a section playback-only before play-along reps |
| Slow-only practice transfers poorly to speed; train submaximal ~85–95%, touch target early (mechanism strong) | **Tempo curves**: oscillating mode (slow ↔ near-target) and submaximal dwell; fixed +N% ladder available but not default |
| Chunk junctions are the failure points (strong) | **Junction loops**: auto-derived loops spanning the end of section N + start of N+1, first-class practice targets |
| Interleaved beats blocked at retention (strong in motor learning, moderate in music) | **Rotation plans**: cycle A/B/C sections rather than massing one; massing allowed for first-contact phase |
| Distributed > massed; sleep consolidates (very strong) | **Spaced resurfacing** of sections across days; session journal measures **next-day retest**, not in-session smoothness |
| Constant feedback breeds dependence; summary/faded is better (well replicated) | Feedback = a per-plan summary at the end, never a live meter |
| Recall testing (Capo 4.7's direction) | **Recall mode**: alternate the recording with silent bars — play from memory into the gap |

## Architecture

Single Rust workspace, one process, Tauri v2 shell. Crates:

### `engine`
Owns the audio thread.
- Output: `pipewire-rs`.
- Decode: `symphonia` (mp3/flac/ogg/wav/m4a) → 48 kHz f32 in memory.
- Stretch/pitch: **Rubber Band** via FFI. Rate 0.25–2.0×, pitch ±12
  semitones + cents, independent.
- Sample-accurate A-B loop points with a short equal-power crossfade at the
  seam (a click on every repeat is ear poison).
- Lock-free SPSC rings between control thread and audio callback; the audio
  callback never allocates, locks, or touches the filesystem.
- Emits: position timebase (rate + epoch, not per-frame ticks), loop-wrap
  events, precomputed waveform peaks per file (cached on disk).

### `practice`
The novel layer. Pure logic + persistence; no audio types.
- Entities: **Song** (file hash, path, metadata) → **Section** (named span,
  ordered) → **Loop** (named A-B span, may be a derived junction loop) →
  **Plan** (ordered steps over loops: listen-first, play reps at tempo curve,
  rotation set, recall test) → **Session/Rep journal** (per-step outcomes,
  self-rated, timestamped) → **Resurfacing queue** (spaced scheduling state).
- Junction derivation: for adjacent sections (A,B), generate loop covering
  the last `n` beats of A + first `n` beats of B (default: last/first bar;
  beats unknown in v1 → time-based tail/head, configurable).
- Tempo curves as data: `ladder(start, step, reps)`, `oscillate(low, high,
  pattern)`, `dwell(rate)` — interpreted by the plan runner.
- Persistence: SQLite via `rusqlite`, migrations embedded. Annotations
  (sections/loops/plans) mirror to a plain-JSON sidecar
  (`<song>.earworm.json`) on every save — git-able, shareable, survives DB
  loss.

### `server`
- JSON-lines control socket at `$XDG_RUNTIME_DIR/earworm.sock`, mpv-style:
  `{"cmd":"loop.set","start":83.2,"end":91.7}`, `{"cmd":"rate","value":0.85}`,
  `{"cmd":"plan.start","id":...}`, plus an event subscription stream.
- **One dispatch layer** shared by socket and Tauri commands — the UI is just
  another client, so scriptability can't rot. Foot pedals, shell scripts,
  and future tools talk to the same surface.

### Tauri shell (`app`)
- Canvas waveform: zoom/pan, drag-to-create loop, drag handles, section
  lane above the waveform. Peaks from engine cache; playhead animated
  client-side from the timebase sync.
- Panels: song library, section/loop list, plan builder, plan runner
  (current step, rep count, big start/stop), journal/retest view.
- Keyboard-first: spacebar-class bindings for loop restart, rate nudge,
  next/prev loop. (Pedal support arrives free via the socket.)
- Deliberately spare visual design. No progress confetti.

## Phases

- **v1 (this spec):** local files → waveform → named persistent
  sections/loops → speed/pitch → plans (listen-first, rotation, tempo
  curves, junction loops, recall mode) → journal + spaced resurfacing →
  control socket. Complete, daily-usable product.
- **v2:** PipeWire capture-anything — tap a chosen app node into a ring
  buffer; "loop what just played"; promote a capture to a library song.
- **v3:** local stems — Demucs as subprocess first, ONNX (`ort`) in-process
  later; per-stem gain inside the loop player.
- **Out of scope (all versions):** instrument-input scoring, chord/key
  detection, notation rendering, cloud anything, mobile.

## Error handling

- Audio-thread failure must never lose annotations: engine isolated behind
  its control channel; a dead stream is restartable without app restart.
- SQLite writes transactional; JSON sidecar mirror on save.
- Socket clients are untrusted input: schema-validate, never panic the
  dispatcher.
- Decode failures surface per-file in the library, never crash import.

## Testing

- `practice`: TDD, pure-logic — plan progression, junction derivation,
  tempo-curve interpretation, resurfacing scheduler, journal queries.
- `engine`: render-to-buffer integration tests — loop seam is
  sample-accurate, crossfade applied, rate/pitch changes glitch-free;
  null-sink PipeWire test where the environment allows.
- `server`: golden-file protocol tests over the shared dispatcher.
- UI: thin by design; manual + a few Tauri e2e smoke tests.

## Open questions deferred to implementation planning

- Rubber Band binding choice (existing `rubberband` crate vs. thin custom
  `-sys` wrapper) — decide in plan after a spike.
- Crossfade length default (likely 5–15 ms) — tune by ear.
- Beat grid in v1 is manual/none; junction loops use time-based windows
  until beat detection exists (post-v3 candidate).
