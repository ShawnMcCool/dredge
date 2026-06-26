# Campaign: Practice Routines

A saved, looping sequence of practice **blocks** — drill one passage through
changing mixes, speeds, and approaches, auto-advancing on each loop pass.
Designed 2026-06-26 (brainstorm + from-first-principles redesign). Work directly
on `main`.

> **For agentic workers:** phases are dependency-ordered. Each phase has a
> verification gate and a commit, and **each phase leaves the tree coherent and
> shippable** — never a half-wired state. This was an explicit constraint: the
> architecture stays fully realized at all times, never a bolt-on. Phases 1–5 are
> the coherent build the user approved in full ("take it all"). Phase 6 (Drill-box
> convergence) is the scheduled migration that retires the one deliberate
> inconsistency this campaign introduces; it can ship separately.

## What it is

A **routine** is a named, ordered list of **blocks** saved in the song bundle. A
block is a snapshot of *how to practice a span*: where, what you hear, how fast,
and how you come in. Launch a routine and it loops through its blocks — every loop
pass advances to the next block, swapping the looped region, the stem mix, the
speed, and the count-in in lock-step with the audio. It lives in a new **Routines
panel tab**; while running, the existing Transport and Isolation controls animate
to reflect the active block, with a small "block i/n" indicator.

Worked example (the one that motivated it): the first two verses, heard five ways
back-to-back — bass isolated → full band (custom stem levels) → drums only → band
minus bass → full band → loop.

## The core idea (from first principles)

> The things that determine **what you hear** — span, mix, speed, count-in,
> lead-in — form one bundle. The **live** version is what the Transport / Isolation
> / count-in controls already edit. A **block is a saved snapshot of that bundle**.
> A **routine is an ordered list of snapshots**. A **scheduler swaps the live
> bundle to the next snapshot on each loop pass.**

Everything follows from that one sentence. "Capture current → block" = snapshot the
live bundle. "Apply a block" = load a snapshot. The runner = swap snapshots on pass
boundaries. The Drill box = editing the live bundle with no snapshot saved. One
concept, no duplicated representation.

## Decisions (final)

These are the architecture-level decisions; recorded here because this repo keeps
design rationale in campaigns/specs, not separate ADR files.

- **`Mix` becomes a value type.** Today the mix (four stem gains + bass-focus) is
  scattered live state (`stemMix`, `bassFocus`). A block must not "store five loose
  floats copied from wherever" — that is two representations of one idea. `Mix`
  lives in `practice::model`; the live isolation state *is* a `Mix`; a block
  *stores* a `Mix`; capture is `block.mix = current.clone()`. One type, one
  serialization, one source of truth. (CountIn is already a typed config — same
  pattern, already coherent.)
- **The scheduler lives behind the dispatch surface, not in the frontend.** The
  architecture is explicit: one tick pump, one dispatch surface, "the UI is just
  another client," "all UI state derives from dispatch responses + events — no
  second source of truth." A frontend runner would make routine-run state UI-local:
  unscriptable, invisible to socket clients, a second source of truth. The original
  v1 spec already listed `plan.start` as a *socket* command. So the runner is a
  **server module** driven by the engine's `loop_wrapped` events, issuing the
  existing EngineCmds, emitting `routine` state events the UI reflects. Blocks
  switch in lock-step with the audio wrap, not after a UI round-trip.
- **Stem gains become ramped setpoints in the engine.** "Smooth, no zipper" between
  blocks is an audio-layer concern and belongs in the engine, not faked around the
  feature. Rate already ramps via the stretcher; gains should too. This is a small,
  correct engine improvement, not a hack.
- **Blocks store raw, beat-snapped bounds**, like loops / `drillSpan`.
  Section/loop-relative spans are deferred.
- **Mix = captured live state.** Authoring reuses every existing Isolation /
  Transport control; no duplicate mix editor.
- **Routines are bundle-canonical** — a `routines` array on `BundleManifest`,
  atomic rewrite, portable with the song (copy the folder, the routines come with
  it).
- **Lead-in is the persisted form of the Drill box's run-up.** `lead_in_beats`
  pushes the looped region start back N beats (beat-snapped); each pass plays
  `lead-in → section → wrap`.
- **Count-in reuses the shipped count-in.** A block carries its own
  `{beats, loop_mode}`; the scheduler pushes it via the existing mechanism on block
  entry. `first` = count me in when this block starts; `every` = before every pass.
  Stacks with lead-in: clicks → lead-in audio → section.
- **Routines always loop** (the example's default). A play-once toggle is deferred.
- **One deliberate, scheduled inconsistency.** The Drill box already keeps per-pass
  scheduling state in the frontend — the exact shape this campaign refuses to
  replicate. The new scheduler is coherent; the Drill box is *not* left as a silent
  orphan — Phase 6 converges it behind the same scheduler.

## The block primitive

```
Mix    { bass_focus, stems[4] }              // shared value type; live state is an instance
Block  { span{start,end}, mix: Mix, speed,
         passes, lead_in_beats,
         count_in{beats, loop_mode}, name? } // name? falls back to a mix-derived label
Routine{ id, name, blocks[] }                // always loops
```

- **passes** — loop wraps to hold the block (default 1).
- **name** — optional; falls back to a derived label ("bass", "full band",
  "drums") for the indicator.

## Layer map

- **`engine`** — stays clean. Lead-in = `SetLoopSecs` with an earlier start (no new
  concept). The scheduler reuses `SetLoopSecs` / `rate` / stem gains / bass-focus /
  `SetCountIn`. One correct change: **stem gains as ramped setpoints** (de-zipper).
- **`practice`** — `Mix` value type; `Block` / `Routine`; `routines` on
  `BundleManifest`; atomic write. Pure data + persistence, no audio types.
- **`server`** — light dispatch CRUD (normal `App`-mutex path, no heavy phasing);
  the **scheduler module** consuming `EngineEvent::loop_wrapped`, advancing per
  `passes`, applying the snapshot, emitting `routine` events; `routine.start/stop`.
- **frontend** — Routines tab + "block i/n" indicator, both thin reflectors of
  backend `routine` state. Authoring = snapshot current. Audio smoothness from the
  engine ramp; visual fader smoothness from a UI tween toward the emitted target
  mix.

## Runtime behavior (baked in)

- **A running routine owns the transport** — it drives loop / rate / mix / count-in
  exclusively; manual nudges are transient and overwritten on the next advance.
  Updating a block is an explicit recapture, never implicit. Stop releases control
  and leaves the last block's state in place.
- **Drill-box seeding is suppressed while a routine runs** — the routine sets the
  looped region itself; the Drill box must not fight it.
- **Glides settle during the count-in / seam** — when the next block has a count-in
  (engine holds at start), the mix/rate ramp completes during that hold, so audio
  resumes already in the new mix.

## Phases

### Phase 1 — `Mix` value type (foundational refactor)
Introduce `Mix` in `practice::model`. Route the live isolation state (`stemMix`
+ `bassFocus`) through it: a `Mix` is what the Isolation box edits and what
`stems.gains` / bass-focus apply. Provide capture (`current → Mix`) and apply
(`Mix → engine cmds`) helpers.
**Gate:** existing isolation behavior unchanged; a `Mix` round-trips
(capture → serialize → apply) with no observable difference. **Commit.**

### Phase 2 — Block / Routine model + persistence
`Block`, `Routine` types; `routines: Vec<Routine>` on `BundleManifest` (after
`recordings`); atomic `dredge.json` rewrite. Dispatch CRUD
(`routine.list` / `routine.save` / `routine.delete`, block reorder; capture a
block from current state).
**Gate:** author a ≥2-block routine via `just cmd`; it persists in `dredge.json`,
reloads, and survives a bundle copy to another path. **Commit.**

### Phase 3 — Engine gain ramp (de-zipper)
Stem gains become ramped setpoints so block-to-block mix switches are click-free.
**Gate:** a stem-gain change ramps to target; no zipper noise on an abrupt
0↔unity change. **Commit.**

### Phase 4 — Backend scheduler
Server module consuming `loop_wrapped`: count wraps, advance per `passes`, on
advance apply the block snapshot — `SetLoopSecs` (incl. `lead_in_beats`), `rate`,
`Mix`, `SetCountIn` — and emit a `routine` state event (`{id, blockIndex,
passesRemaining, ...}`). `routine.start {id}` / `routine.stop`. Owns transport,
suppresses drill seeding while running.
**Gate:** launch a routine over the socket; observe blocks advance and each
block's region + mix + speed + count-in applied, looping at the end. Verify
`loop_wrapped` still increments the pass counter correctly under count-in
*every-loop* mode. **Commit.**

### Phase 5 — Frontend Routines tab + indicator
Add `routines` to `ALL_TABS` / `TAB_VIEWS`. Tab: list, author (snapshot current
→ block, including span + speed + lead-in + count-in + name), reorder, delete,
launch / stop. "block i/n" indicator. Faders tween toward the emitted target mix.
**Gate:** end-to-end in the vite + chrome runtime smoke test (no effect loops,
faders animate, indicator tracks); a routine authored in the UI persists and
replays. **Commit.**

### Phase 6 — Drill-box convergence (deferred, scheduled)
Pull the Drill box's per-pass logic behind the Phase 4 scheduler: the live drill
span becomes an unsaved single block, region toys become block-span edits, the
tempo trainer becomes block-speed-as-a-curve, recall becomes a per-pass modifier,
and "save current drill → routine block" is the bridge. Retires the deliberate
inconsistency. **Ships separately.**

## Existing surfaces this rides (verified 2026-06-26)

| Need | Mechanism | Location |
|------|-----------|----------|
| Stem gains | `stems.gains` command; `stemMix` store; order vocals/drums/bass/other; levels 0..100 | `app.rs:683`, `stores.ts:344,135,168` |
| Bass focus | `bassFocus` action + store | `stores.ts:771,322` |
| Count-in | `countin.set` → `count_in` setting → `EngineCmd::SetCountIn`; `push_count_in` | `app.rs:661,874,914,891` |
| Loop region (transient) | `loop.set {start,end}` → `EngineCmd::SetLoopSecs` | `stores.ts:579`, `app.rs:597`, `looper.rs:48` |
| Rate | `rate {value}` 0.25–2.0; `position.rate` | `stores.ts:539`, `app.rs:577`, `stretch.rs:42` |
| Loop cycle event | `loop_wrapped` (per crossfaded wrap) | `pipeline.rs:130`, `app.rs:946`, `stores.ts:981` |
| Lead-in / run-up math | beat-snapped start extension (`runUp`, downbeats) | `stores.ts:923`, `waveform-math.ts` |
| Bundle persistence | `BundleManifest` (add `routines` after `recordings`); atomic rewrite | `bundle.rs:26,38` |
| Beat grid | `$openSong.analysis.{beats,downbeats,bpm}` | `stores.ts:93,143` |
| Snap helpers | `subdivisionTimes`, `snapToGrid` | `waveform-math.ts:110,127` |
| Panel tab registry | `ALL_TABS`, `TAB_VIEWS` | `App.svelte:37,40` |
| Box / stage widgets | `Box.svelte`; `.boxes` row; `installKeys()` | `lib/ui/Box.svelte`, `App.svelte:126`, `keys.ts` |

## Deferred

Phase 6 Drill-box convergence (above); per-block pitch; routine play-once toggle;
launch/stop/next hotkeys; section/loop-relative spans; per-pass recall inside
routines; count-in/lead-in beyond what's reused; the heavier v1 "Plan" (journal,
spaced resurfacing); cross-song routines.
