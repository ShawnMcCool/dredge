# Campaign: generalize "bass focus" into a selectable "focus" feature

Status: backlog
Raised: 2026-06-13

## Idea

"Bass focus" (`b`) is loved — it octave-ups + low-passes the mix to surface a
buried bass line. Generalize it into a **focus** feature where the user picks a
target from several presets, not just bass: e.g. **bass**, **vocal** (mid
band-pass), **treble / cymbals** (high-pass), maybe **mid**. Each is a filter
preset; a single toggle plus a small picker.

## Context

- Bass focus is an **engine filter** (octave-up + low-pass), already wired:
  command `bass_focus` (`server::app`), filter in `crates/engine` (CLAUDE.md:
  "filters (bass focus)"); frontend `bassFocusOn` store + the `b` key + the
  BASS FOCUS transport toggle.
- So the engine already has the filter infrastructure; generalizing means more
  filter configs + a selection parameter on the command, plus a UI picker.

## Design fork (decide in brainstorm)

1. **Filter-preset focus** — add bass/vocal/treble/mid filter configs to the
   engine; the toggle gains a preset selector. Pure DSP, song-agnostic.
2. **Stem-based focus** — lean on the existing per-stem **solo** in the stem
   mixer (M/S buttons already exist) for "focus on vocals/drums/bass/other".
   Only works when stems are separated.

These could even coexist (DSP presets for non-stemmed tracks, stem-solo when
stems exist).

## Next step

Brainstorm → spec → plan → build. Resolve the fork above; decide the preset set,
the keybinding/UI (single `b` cycles? a picker next to the toggle?), and whether
focus state persists per song.
