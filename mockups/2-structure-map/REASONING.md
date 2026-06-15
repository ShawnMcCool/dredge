# Structure panel redesign — Direction 2: Proportional Structure Map

## Style

Near-black, one-amber-accent, monospace-for-numbers. The panel lives in the
right pane of the 3-column app (~300px, 1px `--line` left border) under a row of
small uppercase tabs with **structure** active in amber. Everything sits on the
flat `--bg`; the only raised surfaces are the editable rows and buttons
(`--bg-raised`). No drop shadows, no rounded "cards", no gradients. Section
labels are 10–11px uppercase with `.08em` tracking in `--muted`; all
times/counts are mono. Hover lifts a border to `--muted`; the active selection
is a 1px amber inset ring. The aesthetic target is "a diagram you read", not a
dashboard you scan.

## The core idea: the map IS the section list

The old design had two separate things — an analysis-stats card and a section
editor list. This direction fuses them into a single **vertical proportional
map**: one column of blocks where **block height ∝ section duration**. A 12s
intro is a sliver; a 34s outro is a tall block. Because the column is drawn to
scale, the song's *shape* is legible at a glance before you read a single label:
you can see the choruses getting longer toward the end, the bridge as a distinct
mass, the short pre-chorus tucked between verse and chorus.

This is the whole bet of the direction: structure is spatial information, so draw
it spatially. Clicking a block selects it and highlights its span on the
waveform — there is no separate list to keep in sync, the map *is* the list.

Supporting pieces:

- **One-line stat readout** above the map: `128 BPM · 4/4 · 103 bars ·
  SongFormer`, with a `412 beats · 9 sections · 3:48` substat. Compact, mono,
  no card chrome — it's a caption for the map, not a widget.
- **Time ruler** on the left (0:00 / 1:00 / 2:00 / 3:00 / 3:48). Because blocks
  are proportional, the ruler ticks line up with real positions, reinforcing
  "this is to scale" and letting you read absolute time, not just relative size.
- **Legend** at the bottom maps the five tones to section types.

## Tone-coding by TYPE (not a rainbow)

The brief's hard constraint: stay near-monochrome with restrained amber/cyan,
never a bright multicolor chart. So tone is keyed to section **type**, with a
deliberately narrow palette:

| Type           | Tone                          | Rationale |
|----------------|-------------------------------|-----------|
| intro / outro  | quiet muted grey (`--t-edge`) | the song's quiet bookends recede |
| verse          | one neutral lift (`--t-verse`)| the "default" body tone |
| pre-chorus     | warm transitional (`--t-pre`) | warming toward the chorus amber |
| chorus         | amber family, dim (`--t-chorus`) | the single accent = the hook |
| bridge         | cyan (`--t-bridge`)           | the one structural departure |

Repeated types **rhyme** — chorus 1/2/3 are visually identical in fill, and each
block carries a 2px left rail in its type color so the rhyme is unmistakable even
where blocks aren't adjacent. The amber chorus tone is the dimmed
`--t-chorus`/`--t-chorus-fg` pair, not the full `--accent` (which is reserved for
the *selected* block's ring and the primary CTA), so the map never gets loud.
Cyan for the bridge follows the project convention that cyan is the secondary
accent for "the one thing that's different".

Short blocks (intro, pre-chorus) auto-collapse to a single inline row
(name + time side by side) so they stay legible at ~18–22px instead of clipping
two stacked lines; taller blocks stack name over time.

## How editing works in this paradigm

A proportional map is great for *reading* and *selecting* but a poor surface for
*precise numeric editing* — dragging tiny blocks to set a boundary to the second
is fiddly, and a 12s sliver can't hold a rename field. So **edit mode flattens
the map into uniform editable rows** while keeping the map's visual vocabulary:
each row carries the same type-colored left rail, so you never lose the
intro/verse/chorus/bridge reading you built up in display mode. The trade is
deliberate — you give up proportional height (which you can't usefully edit
anyway) to gain equal-height rows with room for a rename input and two mono
start/end fields.

Edit affordances, quiet but discoverable:

- An **edit** toggle sits next to re-analyze in the map header.
- Each row: drag handle (`⠿`) for reorder, inline **rename** field (underlines
  amber on focus), mono **start/end** numeric fields, and a `×` **delete** that
  reddens (`--miss`) on hover.
- **+ add section** is a dashed full-width row at the bottom.
- An **"● unsaved edits"** banner appears in amber with **revert** / **save**
  (save is the one amber button). Dirty rows (e.g. an edited pre-chorus
  boundary) get an amber-tinted border and amber name text, so you can see
  exactly what changed.

## Requirements mapping

- **Unify two boxes into one view** → the map is simultaneously the stats
  context (readout above) and the section list (the blocks).
- **All required stats** (128 BPM, 4/4, 412 beats, 103 bars, 9 sections,
  SongFormer, 3:48) → the readout line + substat.
- **All 9 sections with names + m:ss** → the nine blocks, drawn to scale.
- **Click highlights span on waveform** → blocks are buttons; hover shows the
  `--muted` border, the active state shows the amber inset ring (chorus 1 shown
  active).
- **One re-analyze** → exactly one `re-analyze` button in the map header. No
  second analyze CTA anywhere in the analyzed/edit states.
- **Edit toggle + rename + numeric start/end + reorder + delete + add +
  revert/save + unsaved hint** → all present in State 2.
- **Empty state with single amber "Analyze track"** → State 3: a faded
  proportional-block "ghost" (hinting what analysis produces), a "Not analyzed
  yet" line, and one amber button.
- **Fits ~300px, mono numbers, dark/spare/amber** → 300px panel, mono throughout
  for times/counts, flat near-black surfaces.

## Trade-offs & decisions

- **Proportional height vs. fixed rows in display mode.** Proportional wins for
  *reading the song's shape* — the whole point of the direction — at the cost
  that very short sections get thin. Mitigated by the inline-sliver layout so
  even a 12s intro stays readable, and by the fact that thinness *is the
  information* (it's genuinely short).
- **Map height.** Nine sections of a 3:48 song fit comfortably in the panel
  without scroll at a ~0.6s/px scale. A much longer song would scroll
  vertically — acceptable, and vertical is the right axis for a narrow pane.
- **Editing leaves the map.** I chose flatten-to-rows over edit-in-place on the
  proportional blocks. Editing on tiny scaled blocks would be hostile; uniform
  rows are honest about the task (precise numeric work) while the type rails keep
  continuity. The cost is a mode switch, signalled clearly by the `edit` toggle
  and the unsaved-edits banner.
- **Dim amber for choruses vs. full accent.** Using the full `--accent` for
  every chorus would make three loud blocks and steal the accent's meaning. Dim
  amber for the type, full amber only for *selection* and the *primary CTA*,
  keeps the accent scarce and meaningful.
- **Legend vs. self-evident.** A first-time user needs the tone→type key once;
  the legend is small uppercase muted text at the bottom, easy to ignore once
  learned.
