# Export Tab — Design

**Date:** 2026-06-16
**Status:** Implemented

## Problem

The practice mix a user builds inside Earworm — slowed down, pitch-shifted,
drums muted, bass-focused — lives only inside live playback. There is no way to
take that mix *out* of the app: a slowed backing track to a phone, a drumless
loop into a DAW, a bass-focused passage to listen to away from the desk. There
is no export path of any kind today.

An **export tab** in the right panel closes that loop: render the current track
to an audio file that audibly matches what you're hearing, for the whole song or
a chosen span.

## Core values (guided every decision)

1. **WYSIWYH — what you see is what you hear.** The file matches current
   playback exactly, minus master volume. Export never re-derives or re-asks for
   mix settings.
2. **Confidence before a slow operation.** Rendering takes seconds and writes a
   file to disk; the user should know exactly *what* they'll get and *where*
   before committing.
3. **Consistency with the existing panel/tab pattern.** It is "just another
   tab," wearing the SettingsPanel skin — not a modal, not a new surface.

## Decisions (from brainstorming)

- **What bakes in:** the *current session mix* — per-stem levels/mute/solo
  (stems setup), playback speed, pitch, and bass-focus filter. **Master playback
  volume is explicitly excluded** — it is a monitoring level, not a mix decision.
  Export reflects the live mix; it never exposes a second set of export-only
  knobs.
- **Scope — three separate choices:** **Export all** (whole song), **Active
  loop** (the working or saved loop region), and **Current selection** (the
  waveform drag-selection). These are distinct options, not a single "selection"
  concept. An option whose underlying span doesn't exist (no active loop, no
  drag-selection) is **disabled and labeled unavailable** rather than hidden.
- **Format — WAV / MP3 selector with graceful fallback:** WAV is always
  available (lossless, no extra dependency). MP3 is offered only when the encoder
  (ffmpeg) is present; when it is missing, the MP3 option is **disabled up front
  with a plain-language reason**, never offered-then-failed at render time.
- **Bass-focus bakes in like everything else** — no per-export toggle. WYSIWYH.
- **Visual direction — Receipt-first (direction B):** the tab leads with a
  read-only **"you'll get" receipt card** (duration · format · estimated size ·
  the baked settings, and an explicit "master volume not applied" note) that
  updates live as scope/format/stems change. Controls (scope, format, filename,
  folder, Export) sit below it. Chosen over a plain settings-form (summary read
  last) and a compact caption (terser, no size estimate) because the feature's
  whole point is *"take the mix I built out"* — the receipt answers *"did I get
  the right mix?"* before any control is touched. Accepted cost: a hero card
  slightly off the other tabs' idiom, and computing an estimated file size.
- **Destination:** an editable **filename** pre-filled from the song name + the
  active settings (e.g. `song — 0.75x — drumless`), and a **folder** chosen via
  the native OS picker.
- **States:** a **rendering** state with visible progress and a **cancel**
  affordance (cancel must not present a partial file as success), and a **done**
  state showing the written filename + size with **reveal-in-folder** and
  **export-again**. With no song open, the tab shows an **empty state** rather
  than dead controls.

## Acceptance Criteria

- [ ] An "export" tab appears in the right-panel tab strip and is reachable like
      any other tab.
- [ ] The receipt card shows playback duration, chosen format, an estimated file
      size, and a human-readable summary of the baked settings (speed, pitch,
      bass-focus, stem mutes/levels); it states master volume is **not** applied.
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

## Anti-patterns (what this must NOT become)

- **Settings divergence:** a second set of export-only mix knobs. Export reflects
  the current session mix; it never re-asks for speed/pitch/stems.
- **Silent long operation:** a frozen panel during a multi-second render.
  Progress and cancel are mandatory.
- **Blind commit:** clicking Export without knowing scope, format, destination,
  and what's baked in. The receipt exists to prevent this.
- **Modal takeover:** a dialog that covers the stage. It's a tab, consistent with
  the panel.
- **Format-failure surprise:** offering MP3 then failing at render time. Encoder
  availability is reflected up front.

## Deferred (out of scope)

Batch / multi-loop export, video export, loudness normalization, custom
bitrate / sample-rate controls, drag-out-to-DAW, export presets / history.

## Related

First export feature; no prior DDR to supersede. Reads the same session-mix
state the stems box (`StemMixer`), transport (speed/pitch/volume), and
bass-focus control already own. Recent engine work that decodes tracks to a
Rust-rendered WAV for external analysis/stems is the closest existing capability
to lean on at implementation time.

## Mockup

`docs/superpowers/mockups/export-tab.html` — all three explored directions
side-by-side (A settings-form, B receipt-first ✓ chosen, C compact) plus the
shared rendering / done / MP3-unavailable states, rendered in the real theme.
