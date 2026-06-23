# Count-in — design

A BPM-based count-in for the player: clicks before playback starts, leading you
into the song or loop.

## Behavior (settled)

- **Tempo source:** the song's analyzed BPM only. With no analysis, count-in is
  unavailable — the control is hidden and the engine is forced off.
- **Length:** a beat-count stepper styled like the pitch control, `0` = off,
  default `4`. Plain beat count — no meter derivation for now.
- **Rate tracking:** clicks follow the speed fader. At 70% speed the count-in is
  at 70% tempo, leading you in at the pace you'll actually hear.
- **When it fires:**
  - Every manual play (play button / Space) when enabled.
  - Loop behavior is selectable: **first loop** (count-in only on the initial
    manual play; seamless wraps after) or **every loop** (count-in before every
    loop repeat). Irrelevant when no loop is active.
- **Click sound:** synthesized in-engine — short sine ping (~40 ms) with
  exponential decay, first beat accented. Scaled by the volume knob, independent
  of the song mute (recall-silent still counts you in). No audio assets.

## Architecture

### Engine: pre-roll state in `Pipeline`

New copy-only command:

```rust
EngineCmd::SetCountIn { beats: u32, beat_secs: f64, every_loop: bool }
```

`beats: 0` disables. `beat_secs` is the 1× beat interval (`60 / bpm`); the
pipeline divides by the current `rate`, so rate tracking is automatic. The
`Pipeline` stores this config plus a small runtime state (`remaining`,
`frames_to_next_click`).

On `Play` with `beats > 0`: arm the pre-roll — hold the looper at the start
position, keep song gain at 0, and fill the output buffer with synthesized
clicks. Beat spacing in output frames = `round(beat_secs / rate * SAMPLE_RATE)`.
When the last beat elapses (possibly mid-buffer), deactivate and fill the rest of
that same buffer from the looper — seamless hand-off into the song.

Click synthesis: a short sine ping with exponential decay; first beat accented
(higher pitch / louder). Amplitude scaled by the volume target, applied
independent of the play/pause/mute gain ramp.

### Engine: every-loop

Stretcher latency means the feed-side and output-side timelines differ, so
every-loop works on the **output** timeline:

- Suppress the looper's seamless internal wrap. Stop feeding at the loop end, let
  the stretcher fully drain (audio plays right up to the loop end), then run the
  pre-roll, then reset + seek to loop start and resume feeding.
- One new looper method: "frames until region end," so the feed read stops
  cleanly at the boundary instead of wrapping.

`first loop` mode = today's seamless wrap; count-in only on the initial manual
play.

### Config & wiring (`server`)

- App state: `count_in { beats: u32, loop_mode: First | Every }` (`beats == 0`
  is off). Persisted in the settings table as JSON, survives restart.
- New dispatch command `countin.set { beats, loopMode }` → persist + send
  `SetCountIn`.
- On song open / config change, the App computes `beat_secs = 60 / analysis.bpm`
  and pushes `SetCountIn` down. No analyzed `bpm` → force `beats = 0`.

### Frontend

- `stores.ts`: a `countIn` store mirroring `{ beats, loopMode }`, plus a derived
  `hasBpm` from the open song's analysis. `actions.setCountIn(...)`.
- `Transport.svelte`: a new segment after **pitch**, before the reset control,
  matching the `mlabel` + stepper pattern:
  - Stepper `− [off|N] +`, `0` = off, default `4`.
  - A `first ⇄ every` chip shown only when count-in is on and a loop is active.
  - When the song has no analysis (no BPM), the whole segment is **hidden** — not
    dimmed. It appears once analysis lands. This mirrors the notes and drill
    boxes, which are absent until their precondition exists, and avoids a dead
    control that would otherwise want explainer copy.

## Testing

- Engine: pre-roll holds the looper and emits the right beat cadence; spacing
  scales with rate; hand-off into the song is seamless; every-loop drains then
  re-counts; `beats: 0` is a no-op.
- Frontend: stepper math (off/N), `hasBpm` gating, store ↔ wire shape.

## Out of scope (revisit later)

- Meter-aware default (one bar from the grid).
- A running metronome during playback.
- Separate enable toggle (folded into the stepper for now).
- Per-song count-in settings (global preference for now).
