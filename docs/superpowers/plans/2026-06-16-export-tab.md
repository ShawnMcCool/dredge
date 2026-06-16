# Export Tab — Design Plan

> **Design-level only.** This describes WHAT the export tab should be, not HOW to
> build it. The implementation plan (files, wire commands, render path) comes
> later via `/new-feature` referencing this document.

**Status:** Approved design; implementation not yet planned.
**Design record:** `docs/superpowers/specs/2026-06-16-export-tab-design.md`

## Problem Statement

The mix a user dials in for practice — slowed, pitch-shifted, stems muted,
bass-focused — exists only in live playback. There is no way to render it to a
file for use elsewhere (phone, DAW, sharing, archiving). Earworm has no export
path today.

## Design Objectives

- **WYSIWYH:** the file matches current playback exactly, minus master volume.
- **Confidence before a slow operation:** the user knows what they'll get and
  where, before committing to a multi-second disk write.
- **Consistency:** a panel tab wearing the existing SettingsPanel skin.

## User-Facing Behavior

A new **export** tab in the right-panel tab strip. Its body, top to bottom:

1. **Receipt card** (read-only, live): playback duration, chosen format,
   estimated file size, and a human-readable summary of the baked settings
   (speed, pitch, bass-focus, stem mutes/levels), with an explicit note that
   master volume is not applied. Updates as scope/format/mix change.
2. **Scope:** three radios — Export all / Active loop / Current selection. An
   option with no underlying span is disabled and labeled unavailable.
3. **Format:** WAV / MP3 selector. WAV always available; MP3 disabled with a
   plain-language reason when the encoder is missing.
4. **Destination:** editable filename (pre-filled from song + active settings)
   and a folder chosen via the native OS picker.
5. **Export** button.

State transitions: configuring → **rendering** (visible progress + cancel) →
**done** (final filename + size, reveal-in-folder, export-again). Cancel never
presents a partial file as success. With no song open, the tab shows an empty
state instead of dead controls.

## Acceptance Criteria

- [ ] An "export" tab appears in the right-panel tab strip and is reachable like
      any other tab.
- [ ] The receipt card shows playback duration, chosen format, an estimated file
      size, and a summary of the baked settings; it states master volume is not
      applied.
- [ ] The receipt updates live when scope, format, or any mix setting changes.
- [ ] Scope offers whole-song, active-loop, and current-selection; an option with
      no underlying span is disabled and labeled unavailable.
- [ ] The exported audio audibly matches current playback (stems mix + speed +
      pitch + bass-focus) and ignores master volume.
- [ ] Exporting a scope writes only that span's audio.
- [ ] WAV is always available; MP3 is offered only when the encoder is present,
      otherwise disabled with a plain-language reason.
- [ ] Filename is pre-filled and editable; folder is chosen via the OS picker.
- [ ] During render the user sees progress and can cancel; cancel leaves no
      partial file presented as success.
- [ ] On success the tab shows the final filename + size with reveal-in-folder
      and export-again.
- [ ] With no song open, the tab shows an empty state rather than dead controls.

## Anti-patterns

- **Settings divergence:** a second set of export-only mix knobs. Export reflects
  the current session mix; it never re-asks for speed/pitch/stems.
- **Silent long operation:** a frozen panel during render. Progress + cancel are
  mandatory.
- **Blind commit:** Export without knowing scope, format, destination, and what's
  baked in. The receipt prevents this.
- **Modal takeover:** a dialog over the stage. It's a tab.
- **Format-failure surprise:** offering MP3 then failing at render time. Encoder
  availability is surfaced up front.

## Deferred

Batch / multi-loop export, video export, loudness normalization, custom
bitrate / sample-rate controls, drag-out-to-DAW, export presets / history.

## Decisions

See `docs/superpowers/specs/2026-06-16-export-tab-design.md` for full rationale
and the explored visual directions (`docs/superpowers/mockups/export-tab.html`).
