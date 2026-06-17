# Per-section notes with inline tablature — design

**Date:** 2026-06-17
**Status:** approved design, pre-plan

## Intent

While practicing a passage, you often jot loose notes — reminders and rough
tablature ("not necessarily accurate, just text"). Give each **song section** a
notes document you can view and edit in a dedicated stage box, including an
interactive **tablature widget** you can drop inline and resize.

## Identity: notes key on the section's occurrence label

Sections have no durable row identity — `store::replace_sections` deletes every
section for a song and re-inserts on each structure save, minting fresh
`SectionId`s. So notes do **not** key on `SectionId`. They key on the
human-meaningful **occurrence label** already computed in `naming.rs`:
section name + 1-based ordinal among same-named sections in `position` order —
"the 2nd `verse` is `verse 2`". (`naming::occurrence_label`, currently private,
becomes `pub` so the server can compute a section's label for the wire.)

Consequences (accepted):

- The label is stable across boundary nudges (moving where `verse 2` starts/ends
  does not change that it is `verse 2`), so structure re-saves never disturb
  notes.
- Renames and renumbering (insert/reorder) can detach or re-point a note, since
  the note follows the **label**, not the audio span.
- **Orphans are kept, never auto-deleted.** A note whose label matches no current
  section is retained; if a section with that label reappears, its note returns.
  The box surfaces orphaned notes so nothing is lost silently (see UX).

## Data model

### Note document

A section's notes are an ordered list of **blocks**, each a text block or a tab
block:

```
NotesDoc   = { blocks: Block[] }
Block      = { kind: "text", text: string }
           | { kind: "tab", strings: number, width: number, rows: string[] }
```

- A `text` block is free monospace text.
- A `tab` block is a grid of `strings` rows × `width` columns. `rows` holds
  exactly `strings` strings, each exactly `width` characters, `-` for an empty
  cell. **Row order is high→low pitch top→bottom; the bottom row is the lowest
  string** (standard tab orientation). `rows[strings-1]` is the lowest string.

Invariants enforced on save and on edit:
- `rows.length === strings`; every `rows[i].length === width`.
- `1 ≤ strings ≤ 12`, `1 ≤ width ≤ 256` (sane bounds; not user-facing limits in
  normal use).

### Persistence

New SQLite schema version block in `store.rs` (additive — a new
`PRAGMA user_version` step, never an edit to a shipped one):

```sql
CREATE TABLE section_notes (
  song_id    INTEGER NOT NULL,
  label      TEXT    NOT NULL,   -- occurrence label, e.g. "verse 2"
  doc_json   TEXT    NOT NULL,   -- serialized NotesDoc
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (song_id, label)
);
```

The doc is stored as `serde_json` in `doc_json`, consistent with how the app
stores sub-objects (LoopKind, analysis vectors). A row with an effectively empty
doc (no blocks, or a single empty text block) is deleted rather than stored.

### Settings

Two optional settings in the existing JSON `settings` table:

- `default_tab_strings` — rows a new tab block starts with; fallback **4**.
- `default_tab_width` — columns a new tab block starts with; fallback **16**.

Both optional: unset → fallback. Surfaced in the settings tab.

## Wire protocol

`song.open` (and refreshes) extend each section in the payload with:

- `label: string` — its computed occurrence label.
- `notes: NotesDoc | null` — its stored doc, if any.

The open-song payload also gains:

- `orphan_notes: { label: string, doc: NotesDoc }[]` — stored notes whose label
  matches no current section.

Commands (one new verb):

- `section.notes.set { label: string, doc: NotesDoc }` — upsert the doc for
  `(open song, label)`; an empty doc deletes the row. Light command (no
  decode/IO), runs inside the `App` mutex like other small writes. Returns ok;
  the UI refreshes the open song so state stays single-sourced.

## UX

### Placement

A new **notes box** in the stage boxes row (with isolation / tuner / drill),
full-row (`wide`), monospace. It appears once the open song has ≥1 section (no
sections → nothing to attach to; the structure tab already drives analysis). The
box has a **max height with internal scroll** so a long note never dominates the
stage. Header reads `notes — <active label>`, e.g. `notes — verse 2`.

### Active section (hybrid)

The box shows the section the playhead is in, unless pinned. Resolved by a pure
function `activeLabel(sections, playheadSecs, pinnedLabel, editing)`:

- **Playing, editor not focused** → the section containing the playhead;
  switches at section boundaries.
- **Click a section** (structure tab row or waveform) → pins that label.
- **Focus the notes editor** → pins the current label (cannot switch mid-edit).
- **Play again / click away / blur with playback** → release the pin, resume
  following the playhead.

Implemented as a tested pure function in `lib/`; the box and a small
`pinnedLabel`/`editing` local state drive it.

### Editing & autosave

- Editing a text block or a tab cell **debounce-autosaves** (~500 ms idle) via
  `section.notes.set`; the box then mirrors the refreshed open song (no local
  source of truth beyond the in-flight edit buffer).
- **`+ tab`** control in the box inserts a new tab block (default strings/width
  from settings) **after the focused block**; if no block is focused it appends
  at the end. A trailing empty text block is always kept so you can type after a
  tab. (Caret-split insertion within a text block is a possible later
  refinement, out of v1.)
- Each block has a **delete** affordance. No reordering in v1.

### Orphan notes

Orphaned notes (label has no matching section) surface as a small expandable
footer in the box — "N notes from removed sections ▸" — that lists each by its
label and lets you read, or clear it. Never auto-deleted.

### Empty states

- Section with no note yet → placeholder ("jot tab or notes for verse 2…").
- Song with no sections → the notes box is absent.

## Tablature widget

A `tab` block renders as rows bounded by `|` on each side:

```
|----------5--7--------|
|--7--7----------------|
|----------------------|
|----------------------|
```

### Resize

- **Vertical = string count**, one drag handle on the **top** edge. Growing
  **prepends** empty rows at the top (new higher strings); shrinking removes rows
  from the top and **erases** their content. Content stays anchored to the bottom
  (the lowest string), so switching e.g. a 4-string set to 6 leaves entered
  content in place and adds the two higher strings above. No bottom handle.
- **Horizontal = width**, one drag handle on the **right** edge only. Growing
  **appends** `-` to the end of each row, preserving content; shrinking truncates
  each row from the right and may **erase** content past the new width. No left
  handle.
- Drags snap to whole rows / columns (one cell per step).

### Cell editing

- Each position is one monospace cell, default `-`.
- **Overtype**: typing a digit/letter writes that char to the focused cell and
  advances one cell right; width is unchanged, columns never drift. Multi-digit
  frets ("12") simply occupy adjacent cells, as in hand-written tab.
- **Backspace** clears the focused cell back to `-` (and steps left); typing into
  the last cell of a row does not auto-grow width (use the right handle).
- **Arrow keys** move the focused cell up/down/left/right within the grid (and
  across the row boundary at the ends). Focus is a single active cell.

## Out of scope (v1)

- Block **reordering** (add/delete only).
- **Tuning / string labels** (EADGBe down the left edge).
- **Time-anchored** notes / waveform markers.
- **Caret-split** tab insertion inside a text block.
- Rich text / markdown (notes are plain monospace text by design).

## Testing

**Rust**
- `occurrence_label` exposed and unchanged in behavior (existing tests cover it).
- `section_notes` upsert; empty-doc deletes the row.
- Orphan detection: a stored label with no matching section is reported in
  `orphan_notes`; survives `replace_sections`.
- `NotesDoc` (de)serialization round-trip; invariant validation
  (`rows.length === strings`, each row width).

**Frontend**
- `activeLabel(...)` resolver across the hybrid cases (playing/pinned/editing).
- Tab-block transforms as pure helpers: grow/shrink strings (top-anchored),
  grow/shrink width (right-anchored), overtype, backspace, arrow navigation,
  bounds clamping — each unit-tested in `lib/*.test.ts`.
- Debounced autosave fires once after idle; empty doc deletes.

## Component shape

- `crates/practice`: `NotesDoc`/`Block` model types; `store` methods
  (`get_section_notes`, `set_section_notes`, `list_orphan_notes`); schema block.
- `crates/server/app.rs`: `section.notes.set`; section-label + notes enrichment
  in the open-song payload.
- `apps/desktop/src/lib`: `notes-doc.ts` (types + pure tab transforms),
  `active-section.ts` (resolver), both with tests; store wiring in `stores.ts`.
- `apps/desktop/src/components`: `Notes.svelte` (the box, block list, autosave,
  orphans) and `TabBlock.svelte` (the widget: grid, handles, cell editor).
