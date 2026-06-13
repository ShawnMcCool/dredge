# Campaign: waveform shows many lingering vertical indicators (loops not cleaned up)

Status: backlog (bug)
Raised: 2026-06-13

## Symptom

The waveform accumulates **many thin vertical orange/amber lines** across the
section region (see the 2026-06-13 screenshot) — they read as leftover
selection / playhead markers that "aren't cleaning up."

## What it actually is (investigation so far)

The canvas is **fully cleared every frame** — `draw()` in
`apps/desktop/src/components/Waveform.svelte` fills `--bg` over the whole area
before redrawing (`ctx.fillRect(0, 0, w, LANE_H + WAVE_H)` ~line 101-102). So
this is **not** stale-pixel persistence.

The vertical lines are **loop-region edges**: for each loop in
`open.loops`, `draw()` paints a translucent fill plus two vertical accent edge
lines (`Waveform.svelte` ~line 177-192; junction loops dashed, manual solid). So
the screenshot means the open song simply **has a lot of loops**, all rendering
at once. The real bug is loop *data* accumulating, not rendering.

## Likely root causes to check

- **Ephemeral / quick-practice loops** (`p`) that should be discarded when not
  rated are being persisted. Check the quick-session keep/discard path
  (`quickActive`, `practice.quick_discard`, the auto-named `loop N`).
- **Junction loops** re-derived on every save/re-analyze without replacing the
  prior set → duplicates pile up (`junctions.derive` / `derive_junctions_snapped`).
- No easy "clear all loops" affordance, so cruft never goes away.

## Next step

Systematic-debugging: inspect `open.loops` count + kinds for an affected song
(`just cmd '{"id":1,"cmd":"loop.list","params":{"song_id":N}}'`), find which
path is creating/keeping them, fix the lifecycle (discard ephemeral on no-rating;
replace-not-append on junction derive; optionally a bulk clear). Then confirm the
waveform clears down. Frontend rendering is fine; the fix is in loop lifecycle
(server `app.rs` loop/junction/quick handlers + the quick-session frontend flow).
