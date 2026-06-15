# Direction 3 — Minimal Editorial

## Style description

A near-black, type-driven structure panel that reads like the table of contents
or liner notes of a record, not a dashboard. The section list is the hero: each
section name is set large in `--fg`, its time range set small and quiet in
`--mono` `--muted`, with a faint monospace ordinal to the left. There is almost
no chrome — no cards, no pills, no boxes, no shadows. Vertical rhythm and
typographic contrast do all the work of separating one section from the next.
The single amber accent appears only where it earns attention: the active tab,
the active section, the unsaved-edits dot, and the one primary push button.

## Design decisions

### The list is the hero (no chrome)
Rows are bare. Separation comes from generous padding (≈9px vertical) and the
size jump between the 15px section name and the 11px mono range — not from
borders. A 2px transparent left border is the only structural element, and it
only lights up (`--accent-dim` on hover, `--accent` when active) to mirror the
waveform span selection. The resting state is serene: nine names, nine ranges,
nine faint ordinals.

### Stat subordination
The analysis stats (BPM, meter, bars, beats, engine) are compressed into a
single 11px `--mono` `--muted` line directly under the header — `128 BPM · 4/4
· 103 bars · 412 beats · SongFormer`. The dot separators are `--line` (dimmer
than the text), and the engine name is `--accent-dim` so the provenance is
legible but recessive. It reads as a byline, never as a stats dashboard. The
old analysis-stats card is gone entirely; this one quiet line replaces it and
sits clearly below the section list in the visual hierarchy.

### Hover-reveal affordances
At rest, no per-row buttons exist visually. On row hover (or `.show-tools` in
the mockup, to demonstrate), three things fade in: a drag grip on the far left,
and a `Loop / Rename / Delete` text-button row that animates open beneath the
name. `Delete` only turns red (`--miss`) on its own hover, so the destructive
action stays quiet until deliberately approached. This keeps the editorial
calm — the panel looks like reading material until you reach for a row.

### Exactly one re-analyze, one edit toggle
The header carries two text buttons: `Edit` (toggles edit mode) and
`Re-analyze` (the single re-analyze affordance). There is no second analyze CTA
anywhere in the analyzed states; the only other analyze button lives in the
empty state, where it's the whole point.

### Edit mode stays calm
Edit mode swaps each row for inline fields — a full-width name input and a pair
of narrow mono start/end time inputs with a `–` between them — plus a small
up/down reorder control and a quiet `Delete`. It is deliberately not a dense
grid form: one section per stacked block, the same vertical rhythm as display
mode. The footer adds a dashed `+ Add section`, an `unsaved edits` hint (amber
dot + mono label), `Revert` (ghost), and `Save` (the one amber push button).

### Empty state
Minimal: a plain "Not analyzed yet" title, one explanatory line in `--muted`,
and a single amber `Analyze track` button. No card, no illustration.

## Requirements mapping

- **Unifies analysis-stats + section editor** → one panel: meta byline (stats)
  above the section list (editor), no separate boxes.
- **All required data shown** → 128 BPM, 4/4, 412 beats, 103 bars, 9 sections,
  SongFormer in the meta line; all nine sections with exact names and m:ss
  ranges in the list.
- **Click highlights waveform span** → `.row.active` + left-border treatment
  represents the selected span; hint text states the behavior.
- **One re-analyze** → single `Re-analyze` text button in the header.
- **Edit toggle** → `Edit` / `Done` text button in the header.
- **Edit mode extras** → rename input, numeric start/end inputs, reorder
  up/down, delete, add section, revert, save, and the unsaved-edits hint.
- **~300px panel, left border, tab nav** → fixed 300px column, 1px `--line`
  left border, uppercase tab row with `structure` in `--accent`.
- **Affordances discoverable on hover, invisible at rest** → `.row-tools` and
  `.grip` are `opacity:0`/`max-height:0` until `:hover`.
- **Mono numbers** → ordinals, ranges, meta line, and time inputs all `--mono`.
- **Tokens verbatim** → all design tokens copied onto `:root` exactly as given.

## Trade-offs

- **Restraint vs. discoverability.** Hiding affordances until hover is the most
  serene resting state of the three directions, but a first-time user sees no
  buttons. Mitigated with one italic `--muted` hint line under the list; the
  `Edit` toggle remains always visible as the obvious entry to mutation.
- **No always-on per-row loop button.** Looping a section is one of the most
  common actions, yet it's hover-gated here. Acceptable for this direction's
  thesis (calm editorial); a future tweak could surface Loop on the active row
  only.
- **Time-as-text inputs.** Edit mode uses `m:ss` text inputs rather than
  steppers to stay visually quiet and match the display format; real
  validation/parsing would live in the Svelte layer.
- **Ordinals in `--line`.** The faint ordinals risk being too dim to read at
  rest; they brighten to `--muted` on row hover. This trades a little
  always-on legibility for a calmer default — consistent with the direction.
