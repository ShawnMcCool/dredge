# Transient looping + dynamic loop naming — design

Date: 2026-06-14

## Problem

Today, clicking the waveform "loop" glyph (⟳) both starts a loop *and* persists a
`LoopRegion` to the DB, so the Loops list grows every time you casually loop a
section while navigating. That conflates two different use cases:

- **Casual navigation** — "loop this bit right now while I work it out." Should be
  instant and ephemeral; nothing should pile up in a list.
- **Building practice material** — deliberately keeping a loop to reuse later, or
  to assemble into a practice sequence. *That* belongs in a persistent list.

The waveform already has both paths internally: the **▶ (play selection)** glyph
sets the engine loop region transiently (`loop.set`, no DB write), while the
**⟳ (loop)** glyph persists (`loop.create`) *and* sets the region. The redesign
reorganizes these so transient looping is the obvious default and saving is a
deliberate, separate act.

## Interaction model

A waveform selection shows **two glyphs**:

1. **Loop (⟳) — primary / everyday.** Starts looping the selection immediately,
   **transient** (engine region only, no DB write). This is the behavior the
   current ▶ button already has (`playSelection` → `loop.set`); it just becomes
   the primary glyph.
2. **Save (💾) — deliberate.** Persists the loop to the DB (`loop.create`), then
   switches the right-hand panel to the **loops** tab so the new entry is visible.
   It does **not** change what's currently playing.

The **Loops tab** keeps its current behavior (the list, click-to-load as a
transient transport loop, rename, delete, derived junctions). It is simply no
longer auto-fed by casual looping — it becomes the deliberate "saved loops /
practice" surface.

## Creating well-aligned selections

**Drag across the section headers** in the structure lane → the selection snaps to
the **outer boundaries of the touched sections**: first touched section's start →
last touched section's end. This is the natural way to produce a clean
`verse 2 → chorus 1` loop, and it makes "fully covered" the common case rather
than a lucky accident. (Today a single lane-click activates exactly one span; this
extends it to a drag across multiple spans.)

## Dynamic, algorithmic loop naming

A loop's name is **computed** from its bounds versus the song's sections. It is
**not frozen at save time** — it recomputes whenever inputs change (see Recompute
triggers). Section labels are used **verbatim** (songformer gives lowercase
`verse`; the novelty fallback gives letters `A`/`B`), so you get `verse 2` and
`A 2`.

"Occurrence number" = the **Nth section with that name**, counted across the whole
song (1-based). So the second section named `verse` is `verse 2`.

### Naming rules

Given the loop's `[start, end]` against the section list:

1. **Exactly one section, fully covered** → that section's name + occurrence →
   `verse 2`.
2. **Inside one section** (loop is a strict subset) → `sub verse 2`.
3. **Spans multiple sections** → name only the **first and last** sections the loop
   touches, joined with ` → `, middle sections dropped → `verse 2 → chorus 1`.
4. **Spans multiple, but an endpoint section is only partially covered** → prefix
   `sub` on whichever endpoint is partial → `verse 2 → sub chorus 1`,
   `sub verse 2 → chorus 1`, or both.

### "Fully covered" tolerance

A loop edge counts as "on" a section boundary when it's within a small epsilon of
that boundary. Header-drag selections (above) hit boundaries exactly, so they
always read as full. Hand-drawn edges need the tolerance; "fit to section" (below)
is the explicit way to force a clean snap. Epsilon is a small constant
(~50 ms, tunable) — outside it, the endpoint reads as a `sub` (partial) edge.

### Collisions

If a computed name already exists among the song's loops, append a parenthetical
numeric indicator → `sub verse 2 (2)`, `sub verse 2 (3)`, …

### No section underneath

If the loop touches no section (sits in a gap, or the song has no sections /
analysis yet), fall back to the existing timestamp style → `riff 1:23–1:45`.

## Manual override

Double-clicking a loop name opens a rename input. Typing a name sets a
**`name_override`** — a manual name that supersedes the dynamic one. Once a loop is
overridden, the algorithm stops touching its name. The override is the escape
hatch from dynamic renaming.

- **Enter submits** the rename input (today you have to blur / click away).

## Fit to section

The Loops tab gains a per-loop **"fit to section"** action. It snaps **each edge
independently to the nearest section boundary**, then re-triggers dynamic naming.
This rescues a hand-drawn loop into a clean, well-named one.

## Recompute triggers

A non-overridden loop's name recomputes when:

- Its **own bounds change** (waveform resize, "fit to section").
- The song's **sections change** (rename, boundary nudge, `section.replace`).

Overridden loops never auto-recompute.

## Data model

`LoopRegion` gains an optional `name_override: Option<String>`. The effective
name is the computed name when the override is empty, otherwise the override.

- New incremental schema version block (V4) in `crates/practice/src/store.rs`:
  `ALTER TABLE loops ADD COLUMN name_override TEXT` (nullable).

### Where naming lives — source of truth

The naming algorithm is **domain logic in the `practice` crate** (pure function +
colocated unit tests). The **server** is the single source of truth:

- On `loop.create` / `loop.update` / "fit to section", recompute the loop's `name`
  from its bounds + the song's sections (unless `name_override` is set).
- On `section.replace`, recompute `name` for every non-overridden loop of that song.
- The stored `name` column always holds the effective display name, so the
  frontend (Loops list, waveform labels) and the headless daemon / sidecar files
  all agree without a second implementation. The frontend just renders
  `loop.name`; no second source of truth.

The pure naming function signature (conceptual):

```
fn loop_name(start, end, sections: &[Section], existing_names: &[String]) -> String
```

with occurrence-numbering, the four rules, the epsilon tolerance, and collision
suffixing all unit-tested in the `practice` crate.

## Out of scope

- Junction loops keep their current `from → to` auto-naming; this change targets
  manual loops. (Junction naming may later converge on the same helper, but not
  here.)
- No change to the engine's loop/crossfade execution (`looper.rs`).
- No change to the practice-plan / sequence assembly feature itself — only that the
  Loops tab is now the deliberate surface that feeds it.
