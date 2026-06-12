# earworm — Plan 11: one-button song prepare with progress modal

> **For agentic workers:** Use superpowers:executing-plans. Checkboxes track steps.

**Goal:** One **PREPARE** button per song: runs structure/beat analysis then stem separation (sequentially — both are GPU-heavy; SongFormer alone peaks ~8 GiB), with a modal showing per-step progress. Replaces the scattered "Analyze" (Sections) and "Separate stems" (StemMixer) buttons.

**Honest progress:** the subprocesses don't emit percentages. The modal shows two step rows (analysis, stems), each `pending → running → done | failed`, over a coarse overall bar (0 → 50 → 100). Failures show the error text inline (e.g. the demucs install hint) and never block the other step.

---

### Task 1: `Modal` UI primitive

- [ ] `lib/ui/Modal.svelte`: fixed overlay (`rgba(0,0,0,.6)`), centered panel (bg `--bg-raised` or existing panel token, 1 px `--line` border, 2 px radius, `min(420px, 90vw)`), `title` prop, `closable` prop (when true: Esc + click-outside + an × Button close via `onclose`). No animation beyond the existing 120 ms fade. Render via `{#if open}` portal-at-root pattern (App.svelte hosts it).
- [ ] `pnpm build` clean. Commit: `feat(desktop): Modal primitive`

### Task 2: Server — `library_changed` + small affordances

- [ ] `song.import` (incl. capture.grab path) and `song.delete`-if-exists push a `library_changed` event through the existing job-events mpsc so socket-driven imports refresh the UI. Test: dispatch import → `tick()` events include `library_changed` (extend an existing app test file).
- [ ] Desktop `main.rs`: honor `EARWORM_DB` env for the store path (dev/test affordance beside `EARWORM_OPEN`).
- [ ] `cargo test -p server` green. Commit: `feat(server): library_changed event + EARWORM_DB affordance`

### Task 3: Prepare flow (stores + modal component + button)

- [ ] `stores.ts`: `prepare()` action + state machine store `prepareState: { open, steps: { analysis: "pending|running|done|failed|cached", stems: ... }, errors: {} } | null`. Sequence: `analysis.run` → wait for `analysis_progress` terminal (or `{state:"cached"}` response short-circuits) → `stems.separate` → wait for `stems_progress` terminal → on any terminal pair: refresh via `openSong(current)` (loads stems + analysis + triggers section suggestions exactly as today). Auto-close modal 1.5 s after all-success; failures leave it open with a close Button.
- [ ] `components/PrepareModal.svelte`: Modal with two rows (step name, status glyph: `·` pending / spinner `◌` animated / `✓` done-or-cached / `✗` failed + muted error text) and the overall bar (simple div fill 0/50/100%, accent).
- [ ] **PREPARE Button** in the transport toolbar's last Group (accent when nothing cached yet; label `RE-PREPARE`, non-accent, when both cached — state from `openSong` response fields `stems`/`analysis`). Keybinding **`a`** triggers it (help footer updated). Remove the "Separate stems" button from StemMixer and the "Analyze" button from Sections (the suggestions flow stays, fed by the analysis-done refresh).
- [ ] `pnpm build && pnpm vitest run` clean. Commit: `feat(desktop): one-button prepare with progress modal`

### Task 4: Visual verification

Setup (avoids touching the real library, exercises the real pipeline):
```bash
# seed a temp db with a quick song via headless earwormd, then kill it
ffmpeg -y -loglevel error -f lavfi -i "sine=frequency=220:duration=20" -af volume=4 -ac 2 /tmp/prep-song.wav
target/release/earwormd --socket /tmp/prep.sock --db /tmp/prep.db &  # import via python socket helper, then kill
# launch the UI on that db with the song auto-opened
EARWORM_DB=/tmp/prep.db EARWORM_OPEN=1 target/release/earworm &
```
- [ ] Trigger PREPARE without a mouse: `hyprctl dispatch sendshortcut ", a, address:$ADDR"` (focus the window first; if sendshortcut is unavailable in this Hyprland, fall back to a temporary `EARWORM_AUTOPREPARE=1` env read in stores init — and keep it, undocumented, it's harmless).
- [ ] Screenshot the modal mid-run (analysis running) and after completion (both ✓, then auto-closed → stem mixer + beat grid visible). Read the PNGs yourself; iterate until the modal matches the design language. Leave as /tmp/ew-prep-*.png. Note: with the user's game using VRAM, SongFormer may fall back to novelty — fine, the modal still shows analysis ✓.
- [ ] Kill app, clean temp files. Commit: `fix(desktop): prepare modal verification` (if fixes)

### Task 5: Gate

- [ ] `cargo test && cargo clippy --workspace -- -D warnings && cargo fmt && pnpm vitest run && pnpm build`; README: PREPARE replaces the two buttons (one-line edit). Commit final state.
