# Metronome — design

## Concept

A standalone practice metronome: set a tempo and it clicks, **with or without a
song loaded**. It runs on its own clock, fully independent of the transport and
of any analyzed beat grid — you can have it ticking while nothing is playing,
while a song is paused, or layered over a playing song.

This is distinct from the three existing tempo/click features:

- **count-in** — pre-roll click before playback (needs a song + analysis).
- **section-click** — click on every analyzed beat during marked sections (needs
  a song + grid).
- **tempo trainer** (drill box) — ramps the song's playback *rate* across loop
  passes (needs a loop).

The metronome needs none of those — it is a song-independent click generator.

## Placement

A new **Metronome control box** on the stage (`Box` widget), shown **always**,
including on an empty stage with no song. It is its own box — NOT folded into the
Click track box, because that box gates on an analyzed grid and the metronome
must work without one.

This relaxes the stage's current "only render boxes when a song is open" rule for
this one box: the Metronome box renders unconditionally; all other boxes
(Click track, Isolation, Notes, Tuner, Drill) keep their existing song gating. On
an empty stage the Metronome box stands alone, turning the empty stage into a
practice surface.

## The box — controls

- **BPM** — a `NumberField` (range 30–300, default 120).
- **Tap tempo** — a button; tapping it a few times sets BPM from the average of
  recent inter-tap intervals (frontend logic, see Tap tempo below).
- **Sync to song** — a button shown only when a song with analysis is open; seeds
  BPM from the analyzed tempo (`$openSong.analysis.bpm`, rounded).
- **Start / stop** — the primary verb; toggles the metronome running state. This
  is separate from the transport's play/pause.
- **Time signature** — a selector of common signatures (2/4, 3/4, 4/4, 5/4, 6/8,
  7/8, plus a small curated list). The **numerator** sets beats-per-bar (drives
  the number of beat lights and the bar length and where the downbeat accent
  falls). v1 places a single accent on beat 1; compound-meter sub-grouping
  (e.g. accents every 3 in 6/8) is explicitly deferred.
- **Cadence** — click on **every beat**, **every half-bar**, or **every bar**
  (downbeat only). Controls how sparse the click is:
  - every beat → all N beats of the bar click;
  - every bar → only beat 1 clicks;
  - every half-bar → beat 1 and the mid-bar beat click (`floor(N/2)` offset; for
    odd N this is the nearest sensible split, documented as approximate).
- **Sound kit** — a selector; each kit pairs a **downbeat** sound with an
  **other-beats** sound. v1 ships exactly three kits:
  - **Click** — high ping (downbeat) / low ping (other beats) — the existing
    `click_wave` voice at two pitches.
  - **Kick/Snare** — kick (downbeat) / snare (other beats).
  - **Cowbell** — high cowbell (downbeat) / low cowbell (other beats) — one
    simple synth at two pitches.

  The voice/kit abstraction is open for adding kits later, but v1 is these three.
  (Judgment call, flagged in design: kits rather than independent accent/normal
  pickers — one selector, musical, matches the kick/snare framing. Switchable to
  two independent pickers later if wanted.)

## Visual bar indicator

A row of dots inside the box — one dot per beat in the bar (count = the time
signature numerator). As the metronome advances, the current beat's dot lights;
beat 1 is emphasized (size/accent color); beats that don't sound under the
current cadence are shown dimmer. Driven by the `MetronomeBeat` event the engine
emits each beat (so the visual is sample-accurate to the audio, not a separate
frontend timer). When stopped, the indicator rests (no dot lit, or beat 1
pre-highlighted).

## Tap tempo

Pure frontend logic in `apps/desktop/src/lib/metronome.ts` (colocated test):

- On each tap, record a timestamp. Keep a short rolling window (last ~4 taps).
- BPM = 60000 / (average of consecutive intervals in ms), over the window.
- Reset the window if a tap arrives after a long gap (e.g. > 2 s) — that starts a
  fresh count rather than averaging across a pause.
- Reject implausible results (clamp to the 30–300 BPM range).
- The computed BPM flows through the same `setBpm` path as the number field.

## Engine

A new `Metronome` generator (engine module `metronome.rs`), owned by the
**render core** (`render_core.rs`), NOT by the song `Pipeline` — so it runs even
when `current_song` is `None` and no pipeline exists.

- **Mixing:** after the render core fills `out` from the pipeline (song audio) or
  with silence (no song), it advances the metronome across the block and **mixes**
  its voices over `out` (add). The metronome is scaled by its own level (and the
  user volume), independent of the song's play/pause gain and the speed fader.
- **State:** running flag, beat interval in frames (from BPM), beats-per-bar,
  cadence, kit, a frame counter to the next beat, and the active voice(s).
- **Per beat:** when the frame counter reaches a beat boundary, compute the beat
  index within the bar; decide whether it sounds (per cadence); pick the voice
  (downbeat → kit accent voice, else kit normal voice); trigger it; push a
  `MetronomeBeat { beat, of, sounded }` event.
- **Control:** one `Copy` command on the existing `EngineCmd` ring —
  `SetMetronome { running, beat_secs, beats_per_bar, cadence, kit }` (cadence and
  kit as small enums/`u8`). The render core intercepts it in the command-drain
  loop (like the `SetVolume` latch) and applies it to `self.metronome` rather
  than the pipeline. Changing BPM/time-sig/etc. while running re-derives the
  interval and keeps the phase sane (no awkward bar restart; recompute frames to
  next beat proportionally or continue the current beat then apply).
- **Events:** `MetronomeBeat` is a new `EngineEvent` variant, surfaced through
  `poll_events` → the app's broadcast → the frontend event channel, mirroring how
  count-in beat state reaches the UI today.
- **Sounds — synthesized, no sample assets** (RT-safe, asset-free, consistent
  with the existing synthesized click):
  - **click** — the existing `click_wave` (sine ping + exp decay), two pitches.
  - **kick** — low sine with a fast downward pitch sweep (~150→50 Hz over ~20 ms)
    and fast exponential amplitude decay (~150 ms).
  - **snare** — a short white-noise burst (a tiny inline xorshift/LCG noise
    source, no allocation) shaped by a band emphasis + a short tone (~180 Hz),
    fast decay (~120 ms).
  - **cowbell** — two detuned square/tone oscillators (~540 Hz + ~800 Hz) with a
    short decay, rendered at two pitches for downbeat vs other beats.

  A `MetroVoice` abstraction renders a voice given its envelope age; a `Kit`
  maps (downbeat?, kit) → which voice to trigger. The click voice reuses
  `click_wave` for DRY with count-in/section-click.

## Server & frontend wiring

- **Server (`app.rs`):** a `metronome.set` command persisting a `metronome`
  setting `{ running?, bpm, time_sig, cadence, kit }` and forwarding a
  `SetMetronome` to the engine via `AudioControl`. A `push_metronome()` (parallel
  to `push_count_in`) derives `beat_secs` from BPM and sends the command; the
  `running` flag is transient (not persisted) so launch never auto-starts. The
  `MetronomeBeat` engine event is broadcast to clients as a `{event, data}`
  push. `AudioControl` gains `set_metronome(...)` (real engine + `MockEngine`).
- **Frontend (`stores.ts`):** a `metronome` store mirroring the settings shape +
  a transient `running` + the current beat (from the event); `metronomeBeat`
  derived/handled in the event subscription; actions `setMetronome(patch)`,
  `toggleMetronome()`, `tapTempo()`, `syncMetronomeToSong()`. Hydrate the
  persisted fields in `loadSettings`. A `MetronomeBox.svelte` component renders
  the controls + the bar indicator. Pure logic (tap-tempo, indicator mapping,
  BPM clamp) lives in `lib/metronome.ts` with a colocated test.

## Persistence

A global `metronome` setting (BPM, time signature, cadence, kit) stored in the
SQLite `settings` table like `count_in`. The **running** state is not persisted.

## Edge cases

- **No song open:** the box and the metronome are fully functional — the core use
  case. (Requires the render-core-level generator, which has no song dependency.)
- **Metronome + song both sounding:** both are heard (valid — play to the click).
- **Changing BPM / time-sig / cadence / kit while running:** applies cleanly
  without an awkward bar restart.
- **Tap tempo after a long pause:** starts a fresh tap window rather than
  averaging across the gap.
- **Very fast/slow BPM:** clamped to 30–300.
- **Stopping:** silences the voice and rests the indicator; no lingering click.

## Testing

- **Frontend (vitest):** tap-tempo math (intervals → BPM, window reset on long
  gap, outlier/clamp handling); bar-indicator state mapping (which dot is lit,
  downbeat emphasis, dimmed silent beats per cadence); BPM clamp.
- **Engine:** the generator fires beats at the correct frame interval for a given
  BPM; the accent voice lands on beat 1; cadence gates the correct beats (every
  beat / half-bar / bar); `MetronomeBeat` events carry the right
  `beat`/`of`/`sounded`; the metronome runs and produces audio with **no song
  loaded**; each kit's voices produce non-silent output; changing config while
  running doesn't panic or drop beats.
- **Server:** `metronome.set` persists the setting (minus `running`) and forwards
  a `SetMetronome` to the mock; `push_metronome` derives `beat_secs` correctly;
  the running flag is not persisted.

## Out of scope (v1, deferred)

- Subdivisions (8ths/triplets within the beat).
- Compound-meter sub-accents (e.g. accents every 3 in 6/8) beyond the single
  beat-1 accent.
- Independent accent/normal sound pickers (kits only for now).
- Polyrhythm / multiple simultaneous click layers.
- Programmable per-bar accent patterns.
