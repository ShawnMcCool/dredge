# Direction 1 — Instrument-readout header above a section list

## Style description (visual language, mood)

Near-black surface (`--bg #101014`), one amber accent (`--accent #e0a458`), and
monospace for every number. The mood is *instrument*, not *app*: the top of the
panel reads like the LCD on a hardware looper or drum machine — four tight cells
of big tabular-numeric values under whisper-quiet uppercase labels. Everything
else is restrained chrome: 1px `--line` hairlines, `--radius 2px` corners, no
shadows, no gradients, no pills. Section labels are 9–11px uppercase with
`.08em` tracking in `--muted`, so the eye reads the *data* (names, times, BPM)
and treats structure as background. Amber appears only where it means something:
the active tab, the active section, the Save action, and the unsaved-edits dot.

## Design decisions (why this layout / hierarchy)

- **One readout replaces the stats card.** The four headline facts (BPM, meter,
  bars, beats) become a single 4-up monospace strip with a unifying border,
  rather than a separate boxed "stats card." It's the most glanceable possible
  encoding and it physically *is* the top of the section view, so the two old
  boxes are now one object with one frame.
- **Engine is a caption, not a stat.** "SongFormer" is provenance, not a number
  you read every session, so it drops to a quiet sub-line with a green status
  dot — present but never competing with the readout.
- **Section list is the body.** Below the readout, rows are dead-simple:
  name left, mono time-range right, hairline divider between. A left amber
  border + amber name marks the active (clicked → waveform-highlighted) row.
  Hover raises the row to `--bg-raised` and lifts the time to `--meter`,
  previewing the click affordance without shouting.
- **One re-analyze, tucked in the header.** It lives as a single small
  `⟲ Re-analyze` action in the "Analysis" header row — the natural home for "redo
  the thing that produced these numbers." There is no second analyze button
  anywhere in the analyzed states.
- **Edit is a quiet toggle.** Display mode shows a ghost `Edit sections` link in
  the footer. In edit mode the same rows gain inline name + start/end inputs,
  per-row ↑ ↓ × controls, and a footer with `+ Add`, `Revert to analysis`, and an
  amber `Save`, plus an `Unsaved edits` hint. Editing reuses the row grid, so it
  reads as the *same* list in a writable state, never as a settings form.

## Requirements mapping

- *Unify two boxes into one coherent view* → single bordered readout strip +
  section list under one panel body; no competing cards.
- *Instrument-readout header replacing the stats card* → the 4-cell `.readout`
  with mono `--ro-val`, tiny uppercase `--ro-lbl`, engine as caption.
- *Numbers in `--mono`, tabular feel* → `font-variant-numeric: tabular-nums` on
  readout values, section times, and all edit inputs.
- *Scannable section list* → name/time rows, subtle dividers, ellipsis on long
  names, fixed-width mono times that align into a column.
- *Clicked row highlights its waveform span* → `.srow.active` (amber left border
  + amber name + raised bg); `intro` shown active in state 1, and `:hover`
  previews the treatment.
- *Exactly one re-analyze affordance* → single `⟲ Re-analyze` in the header.
- *Edit toggle with rename / numeric start-end / reorder / delete; footer add /
  revert / save; unsaved hint* → all present in state 2.
- *Unanalyzed CTA, single amber button, no duplication* → state 3, one
  `Analyze track` button.
- *In situ, ~300px, left border, tab nav with `structure` active* → 300px
  `.panel` with `border-left:1px solid --line`, tab row with amber active tab,
  rendered against full `--bg`.
- *Aesthetic* → near-black, single amber, mono numbers, hairlines, 2px radius,
  no shadows/gradients/pills/emoji-as-UI.

## Trade-offs

- **Gains:** maximum glanceability — the four key numbers are an instrument
  cluster you read in one fixation; the section list stays the dominant, dense,
  scannable body; the whole panel is one coherent object. It's the confident,
  conventional unification: nothing surprising, everything legible at 300px.
- **Sacrifices:** the readout is fixed at four facts — adding a fifth (e.g. key,
  duration) means re-balancing the 4-up grid or wrapping to two rows. The strip
  is information-display only; it carries no controls except the tucked
  re-analyze, so any future per-metric action would need a new home. And because
  edit mode reuses the row width, the inline time inputs are deliberately narrow
  (`m:ss`), trading fine-grained numeric editing for keeping the list rhythm
  intact at panel width.
