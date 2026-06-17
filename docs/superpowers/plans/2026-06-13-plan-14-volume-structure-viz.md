# dredge — Plan 14: finish loading UX, playback volume, structure on the waveform

> **For agentic workers:** Use superpowers:executing-plans. Checkboxes track steps.

**Goal:** (A) Complete plan 13's loading indication (Tasks 3–4 — partial edits are in `git stash`, pop and judge/reuse). (B) Playback volume control end-to-end. (C) Structure analysis becomes visible: labeled, color-coded section spans on the waveform — including unsaved suggestions — with click-to-loop.

---

### Task 1: Finish plan-13 loading indication

- [x] `git stash pop`; review the partial `Library.svelte`/`stores.ts` edits against plan 13 Task 3 spec (openingSong store, row spinner `◌`, disabled rows, stage `opening…` / 2 px indeterminate accent bar on song switch). Complete/correct, build, vitest. (Stash was correct but incomplete: spinner had no CSS, stage states missing — added spin animation + `opening…` text + indeterminate bar.)
- [x] Plan 13 Task 4 timing proof (temp DB, seeded 44.1k stems): report first-open vs post-upgrade-open numbers vs the ~5-sequential-decode baseline. (First stems open 2.27 s, post-upgrade 1.80–1.83 s, baseline ≈ 9 s — see plan 13.)
- [x] Check plan 13's boxes in its file. Commit: `feat(desktop): song-open loading indication` then `feat: plan 13 complete — fast opens with loading indication`

### Task 2: Playback volume (engine → UI)

- [x] `engine::pipeline`: `EngineCmd::SetVolume(f32)` (clamped 0.0–1.5) — a user-volume multiplier SEPARATE from the mute/pause gain ramp, with its own ~5 ms linear ramp to the target (no zipper noise). Render applies `gain * volume`. Tests: render at volume 0.5 halves RMS vs 1.0; mute still silences regardless of volume; wrap events unaffected.
- [x] `server`: `"volume" {value}` dispatch arm → SetVolume. Persist nothing server-side beyond settings: stores already has the settings mirror — UI sends `volume` on startup from setting `playback_volume` (default 1.0).
- [x] UI: Transport gains a volume Group — compact horizontal Fader (~90 px, non-accent) + `%` readout, placed before BASS FOCUS/MUTE. Writes through to `settings.set playback_volume` (debounced ~300 ms) and dispatches `volume` live while dragging.
- [x] Tests green workspace-wide. Commit: `feat: playback volume — engine ramped multiplier, transport fader, persisted`

### Task 3: Structure visualization on the waveform

Today the waveform's section lane shows saved sections only, monochrome; analysis suggestions live solely in the Sections tab. Make structure visible:

- [x] **Label palette** (`lib/waveform-colors.ts`): fixed muted-hue map for the SongFormer 8-class labels (verified from the HF snapshot's `dataset/label2id.py` + `modeling_songformer.py` — inference uses `SongForm-HX-8Class`, i.e. `intro, verse, chorus, bridge, inst, outro, silence, pre-chorus`; the plan's guessed `solo` is NOT in the inference set; chorus = the amber accent family, others desaturated distinct hues consistent with the dark palette) + deterministic fallback hue for unknown/novelty labels (A/B/C…). Export `labelColor(label): {fill, edge}`. Vitest: known labels stable, unknown deterministic, all fills within muted alpha bounds.
- [x] **Saved sections**: lane spans use `labelColor` fill (low alpha) + solid 1 px edge + label text (existing font rules).
- [x] **Suggestions** (analysis present, sections not yet saved — the existing `suggested` state in stores): drawn in the same lane with dashed edges + lower alpha + italic label; visible immediately after PREPARE completes. If saved sections exist, suggestions are NOT drawn on the waveform (avoid double-lane noise — tab still shows them).
- [x] **Click-to-loop**: clicking a span (saved or suggested) in the lane sets the transport loop to that span (existing `loop.set` transport path) and highlights it; double-click on a *suggested* span additionally seeds the selection (so `l`/`p` work on it). Hit-testing in the existing canvas pointer handlers (lane y-band).
- [x] Help footer unchanged (no new keys). `pnpm build && pnpm vitest run` clean. Commit: `feat(desktop): color-coded structure lane with suggestions and click-to-loop`

### Task 4: Visual verification + gate

- [x] Temp DB; import a real-music file — generate something with actual structure variety: concatenate two distinct ffmpeg segments (e.g. 30 s of 220 Hz arpeggio-ish + 30 s of noise-ish) so novelty/SongFormer yields ≥2 distinct labeled sections; PREPARE via sendshortcut `a`; screenshot: suggestion spans visible with colors + dashed edges; click-to-loop not capturable without a mouse — verify by code review and say so. Screenshot the volume fader in the transport. Read and judge PNGs; leave as /tmp/ew-p14-*.png.
      Done with a real PREPARE (beat_this + SongFormer on GPU, demucs stems; temp DB + `XDG_DATA_HOME` so the real stems cache stayed untouched). SongFormer labeled both halves `inst` (2 sections, same label) — suggestion screenshot shows two dashed/italic `inst` spans (hue 320). Palette variety proven on saved sections seeded via socket (`intro/verse/chorus/outro` — 4 distinct hues, chorus amber, solid edges, suggestions correctly suppressed). Click-to-loop verified by code review only (no pointer synthesis available). PNGs: /tmp/ew-p14-{loading,transport,suggestions,saved-sections}.png.
- [x] Full gate: `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt && pnpm vitest run && pnpm build`. README: volume + structure-lane lines. Commit: `feat: plan 14 complete` (131 cargo tests + 36 vitest green)
