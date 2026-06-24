# Click track — design

## Concept

Unify dredge's click features into one "click track" with two modes that share a
sound:

- **Count-in** — the existing pre-roll click before playback. Behavior
  unchanged; only its *controls* relocate (see Surfaces).
- **Section click** (new) — a click on every beat *during* sections the user
  marks, locked to the analyzed beat grid. Use case: when you isolate the drums
  to play along, a section may have no drums; the section click fills that gap so
  you hold time through it and land in the pocket when the band re-enters.

Both modes use the same synthesized click voice (`click_sample` in
`pipeline.rs`), with downbeats accented. They never overlap in time (count-in is
pre-roll; section click is during playback), so one consistent sound is correct.

## Surfaces

- **Click track control box** (new, on the stage) — a `Box` (`lib/ui/Box.svelte`)
  with two `Group`s:
  - **count in** — the controls moved out of `Transport.svelte` (on/off, beat
    count stepper, loop mode).
  - **section click** — a master on/off only (v1).

  The box appears only once the song has a beat grid, the same gating count-in
  uses today (`countInAvailable`).
- **Structure tab** (`Sections.svelte`, right panel) — each section row gets a
  small click toggle: the per-section "guide on here" selection. This is the
  natural home because the structure tab already owns the per-section list; a
  scrolling checklist does not belong in a compact control box.
- **Transport** (`Transport.svelte`) — the count-in *controls* move out into the
  Click track box, but the count-in **playhead pulse stays** (it is live
  feedback during the pre-roll, not a control). No other change.

## Data model & persistence

- **Per-song selection:** add `Section.click_guide: bool` to `practice::model`
  (`#[serde(default)]` so existing bundles load with it `false`). Persisted in
  each bundle's `dredge.json`. This is the per-section selection.
- **Global master arm:** a `section_click` setting (`{ "enabled": bool }`) in the
  SQLite `settings` table, mirroring how `count_in` is stored and pushed. Global,
  like count-in — turning it on lets each song click *its own* marked sections.

Accent is fixed-on (downbeats accented, matching count-in). Click level reuses
the count-in click amplitude. Both are deferrable knobs, intentionally omitted
from v1 (YAGNI).

Why this split: the master arm is global app behavior (like count-in); *which*
sections click is per-song structure, so it lives on the section in the bundle.

## Server (`crates/server/src/app.rs`)

- **Set a section's flag:** a command that sets `Section.click_guide` for a
  section id and rewrites the affected `dredge.json` atomically (the existing
  no-save-button edit pattern). Triggers a schedule recompute.
- **Set the master arm:** persist the `section_click` setting (same shape as the
  existing `count_in` setting handling), then recompute.
- **`push_section_click()`** (parallel to `push_count_in()`): builds the click
  schedule and hands it to the engine. Steps:
  1. If the master is off or the open song has no analysis, push an empty
     schedule and return.
  2. Intersect `Analysis.beats` with the `[start, end)` spans of sections whose
     `click_guide` is true.
  3. Tag each kept beat that also appears in `Analysis.downbeats` as an accent.
  4. Produce `Arc<[ClickMark]>` where `ClickMark { secs: f64, accent: bool }`,
     sorted by `secs`, and swap it into the engine's click slot.

  Recomputed on: song open, section-flag change, master-arm toggle, and analysis
  arrival (the same points `push_count_in()` already fires, plus the flag
  change).

The schedule builder (beats + downbeats + marked spans → `Vec<ClickMark>`) is a
pure function so it can be unit-tested without the engine. It lives in the server
crate (it needs `Analysis` + `Section`), e.g. a `section_click` module.

## Engine (`crates/engine/src/pipeline.rs`)

The click schedule is variable-length, so it cannot ride the `Copy`-only
`EngineCmd` ring. It uses the same lock-free hand-off the song already uses: a
new `Arc<ArcSwapOption<[ClickMark]>>` click slot (alongside the existing
`song_slot`), written by the control thread, loaded in `render`.

Pipeline state additions:

- the loaded schedule (cloned `Arc` only when the slot pointer changes, like the
  song-swap check in `render_core.rs`),
- a cursor index into the schedule,
- the per-beat click runtime already exists for count-in (`ci_click_age`,
  `ci_accent`) — section click adds its own small runtime so the two can't
  collide, or reuses a shared one-shot click voice. Either way the *synth*
  (`click_sample`) is shared.

Overlay behavior in `render_song`:

1. Compute the block's song-time span **once per render buffer**
   `[block_start, block_end)` from the looper position and rate. If no schedule
   mark falls in that span and no click is mid-decay, **early-out** — no
   per-frame work. This is what keeps the off-cost (and the on-but-quiet cost) at
   one bounds check per buffer.
2. Otherwise, for each mark in range, find its frame offset within the block and
   trigger the click voice there (accent → accent freq/amp).
3. **Mix** the click sample *over* the rendered song (add), not replace it the
   way the silent pre-roll does.

Cursor handling:

- On a seek or loop-wrap, binary-search the cursor to the first mark at/after the
  new position, so looping across a drumless→drums boundary clicks only inside
  the marked span.
- In steady state the cursor advances forward through 0–1 marks per buffer; no
  search.

Timing properties (fall out of the design, no extra work):

- The schedule is in **song time**, so clicks ride the speed fader — at 0.5× they
  slow with the music and stay on the beats.
- Pitch shift does not affect the click (synthesized at a fixed frequency).

A new `EngineCmd` is not required for the schedule itself (it goes through the
slot). A tiny `Copy` signal may still be used to notify the RT thread that a new
schedule is present, matching how song swaps are detected by pointer compare; the
implementation plan picks whichever matches `render_core.rs` conventions.

## Frontend (`apps/desktop/src`)

- `lib/stores.ts`:
  - `sectionClick` store mirroring the master `{ enabled }` setting.
  - section data carries `clickGuide` per section (from the manifest).
  - actions `setSectionClick({ enabled })` and `toggleSectionClick(id)` that
    dispatch the server commands; UI re-derives from the responses/events (no
    second source of truth).
  - a `clickTrackAvailable` derived value (song has a beat grid), gating the box.
- `components/ClickTrack.svelte` (new): the Box with the two Groups. The count-in
  markup moves here from `Transport.svelte`.
- `components/Sections.svelte`: per-row click toggle wired to `toggleSectionClick`.
- `components/Transport.svelte`: remove the count-in control markup; keep the
  playhead-pulse feedback.
- Pure logic stays testable: the section-click schedule preview / any client-side
  derivation lives in a `lib/*.ts` with a colocated `*.test.ts`.

Vocabulary (consistent in code, CSS, and conversation): the box is the **Click
track** control box; its groups are **count in** and **section click**; the
per-section control is the section's **click** toggle.

## Edge cases

- **No analysis:** the section-click group and the per-section toggles are
  hidden/disabled; the schedule is empty. A `click_guide` flag set earlier
  persists harmlessly and takes effect once analysis exists.
- **Count-in + a marked first section:** the pre-roll plays, the song starts,
  then section click begins — distinct phases, no overlap.
- **Loop spanning drumless→drums:** clicks sound only inside the marked span;
  the cursor reseeks on loop-wrap so the next pass repeats correctly.
- **Master off:** empty schedule pushed; per-section selections are retained for
  when it is turned back on.

## Performance

- **Off:** `push_section_click()` runs only on discrete events and returns an
  empty schedule. In the engine, the extra `ArcSwapOption::load()` joins the one
  already done per buffer; the per-buffer early-out means **no per-frame work**.
- **On:** the click voice runs only during its ~40 ms decay window after each
  beat (~8% of the time at 120 bpm) — a couple of transcendental ops per frame,
  negligible beside the stretcher and filters. The cursor advances through 0–1
  marks per buffer in steady state. Memory is 16 bytes per beat (a few KB per
  song).

## Testing

- **Rust (engine):** overlay fires clicks at the correct frame offsets given a
  schedule and an advancing position; mix-adds rather than replaces; accents
  downbeats; cursor reseeks correctly on seek and loop-wrap; early-out when no
  marks are in range.
- **Rust (server):** the pure schedule builder — beats intersected with marked
  spans, downbeat accent tagging, empty result when the master is off or there is
  no analysis.
- **Frontend (vitest):** per-section toggle and master-arm store logic;
  `clickTrackAvailable` gating.
